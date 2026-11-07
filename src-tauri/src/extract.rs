//! 从一段 loop（MIDI）里提取**调性与和弦进行**。
//!
//! 产品定位决定了两条硬约束（见 `docs/midi-writing-patterns.md`）：
//! 1. 输出要"人话"：不是 `Cmaj9/D`，而是 `C/D` 或 `C`。精确音级只作为证据。
//! 2. 输入常常只有旋律或贝斯：此时不硬凑和弦，只给调性并明确说明。
//!
//! 处理链：分角色（剔鼓、分旋律/和声）→ 网格剖面 → 分段 → 基础和弦 → 折叠 → 调性。

use std::collections::HashMap;

use serde::Serialize;

use crate::chord::{Chord, Kind, Quality};
use crate::key::Key;
use crate::midi::{MidiNote, Smf};
use crate::note::Note;
use crate::progression;

/// 每拍切成多少个分析格（4 = 十六分音符网格）。
const GRID_PER_BEAT: u64 = 4;
/// 判断"同起点成簇"时容忍的时间差（刻度）。约等于 1/24 拍（96 刻度/拍 下为 4）。
const CLUSTER_TOLERANCE: u64 = 4;

/// 折叠后的基础和弦（给人看的那个）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicChord {
    pub root: u8,
    pub triad_suffix: &'static str,
    pub seventh: bool,
    /// 七音相对根音的半音（10 = 小七，11 = 大七）。
    pub seventh_interval: u8,
    /// 其余不属于骨架的音（相对根音的半音），如 9、#11。
    pub tensions: Vec<u8>,
    /// 低音（不属于骨架时为斜杠低音）。
    pub bass: u8,
    pub slash: bool,
    pub symbol: String,
    /// 0..100
    pub confidence: u8,
}

impl BasicChord {
    pub fn root_note(&self) -> Note {
        Note::new(self.root)
    }

    /// 骨架音的音高类（根、三音、五音、七音）。
    ///
    /// 这个必须完整：调性打分是拿和弦的**全部音**去比对调内/调外的，
    /// 只给根音会让"三音在不在调内"这个关键信息丢失——
    /// 例如 `Cmaj7` 的 B 不在降 B 大调里，缺了它就分不出 C 大调和降 B 大调。
    pub fn skeleton_pcs(&self) -> Vec<u8> {
        // 注意：小三的五音是 7（0,3,7），只有减三才是 6（0,3,6）。
        // 这两者共用分支会把小三的五音判成"延伸音"，是最容易写错的一处。
        let (t3, t5) = match self.triad_suffix {
            "m" => (3, 7),
            "dim" => (3, 6),
            "aug" => (4, 8),
            "sus4" => (5, 7),
            "sus2" => (2, 7),
            _ => (4, 7),
        };
        let mut v = vec![
            self.root,
            (self.root + t3) % 12,
            (self.root + t5) % 12,
        ];
        if self.seventh {
            v.push((self.root + self.seventh_interval) % 12);
        }
        v
    }

    fn quality(&self) -> Quality {
        match self.triad_suffix {
            "m" => Quality::Minor,
            "dim" => Quality::Diminished,
            "aug" => Quality::Augmented,
            "sus4" | "sus2" => Quality::Suspended,
            _ => {
                if self.seventh && self.seventh_interval == 10 {
                    Quality::Dominant
                } else {
                    Quality::Major
                }
            }
        }
    }
}

/// 一个和弦段。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// 从 loop 起点算的第几拍（0 起，可为小数）。
    pub start_beat: f64,
    pub len_beats: f64,
    /// 1 起的小节号 / 小节内第几拍。
    pub bar: f64,
    pub beat_in_bar: f64,
    pub chord: BasicChord,
    /// 该段是否有一半以上时间由"上一个和弦的残留低音"支撑（持续音）。
    pub pedal_bass: bool,
    /// 被判定为和弦音的音名。
    pub chord_tones: Vec<String>,
    /// 落在该段但没被判定为和弦音的音（可能是旋律或装饰音）。
    pub excluded: Vec<String>,
    /// 代表音：每个和弦音**只取一个**具体音符（同音高类里时值最长、其次音高最低）。
    ///
    /// 一个和弦在 MIDI 里常常有好几个重复音（八度加倍、多层叠置、supersaw），
    /// 画钢琴卷帘时不需要把它们全标出来——每个和弦音给一个最"实在"的就够。
    pub voicing: Vec<VoicingNote>,
}

/// 代表音：和弦音在具体八度上的一个实例。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicingNote {
    pub pitch: u8,
    pub name: String,
    pub role: NoteRole,
    pub start_beat: f64,
    pub dur_beats: f64,
    /// 该音高类在本段一共出现了几次（八度加倍、叠层会让它大于 1）。
    pub count: usize,
}

struct LayerInfo {
    pub label: String,
    pub kind: LayerKind,
}

/// 一个音符在和声里的角色。
///
/// 这是给"人眼核对"用的：可视化时每个音符按角色上色，就能一眼看出
/// "这个音被当成和弦音了，其实它是旋律"。没有这一层，判错只能靠猜。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteRole {
    Root,
    Third,
    Fifth,
    Seventh,
    /// 属于该段和弦、但不在骨架里（九音、十一音之类）
    Extension,
    /// 判为非和弦音（旋律 / 经过 / 装饰）
    Melody,
    /// 落在任何和弦段之外（休止，或段边界外的尾巴）
    Outside,
}

impl NoteRole {
    pub fn label(self) -> &'static str {
        match self {
            NoteRole::Root => "根音",
            NoteRole::Third => "三音",
            NoteRole::Fifth => "五音",
            NoteRole::Seventh => "七音",
            NoteRole::Extension => "延伸音",
            NoteRole::Melody => "旋律",
            NoteRole::Outside => "段外",
        }
    }

    /// 是否算和弦音。
    pub fn is_chord_tone(self) -> bool {
        !matches!(self, NoteRole::Melody | NoteRole::Outside)
    }

    /// 给界面用的颜色语义（前端可直接映射）。
    pub fn kind_id(self) -> &'static str {
        match self {
            NoteRole::Root => "root",
            NoteRole::Third => "third",
            NoteRole::Fifth => "fifth",
            NoteRole::Seventh => "seventh",
            NoteRole::Extension => "extension",
            NoteRole::Melody => "melody",
            NoteRole::Outside => "outside",
        }
    }
}

/// 被标注过角色的单个音符。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotatedNote {
    /// MIDI 音高（带八度，0..127）。
    pub pitch: u8,
    /// 音名，如 `C4`。
    pub name: String,
    /// 来源轨道下标（多轨文件里用来区分是哪一层）。
    pub track: usize,
    /// 相对 loop 起点的拍位置。
    pub start_beat: f64,
    pub dur_beats: f64,
    pub velocity: u8,
    /// 归属的和弦段下标（没有则为 `None`）。
    pub segment: Option<usize>,
    pub role: NoteRole,
    /// 判定时用到的和声权重（0..1）：时值越长、同起点越成簇，越高。
    /// 这一项就是"为什么它被判成旋律"的直接解释。
    pub weight: f64,
    /// 同一起点是否有别的音（成簇）。
    pub clustered: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LayerKind {
    Drums,
    Bass,
    Harmony,
    Melody,
    Empty,
}

/// 一次提取的完整结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopAnalysis {
    pub bpm: f64,
    /// false 表示文件里没有 tempo 事件，bpm 是 SMF 默认值。
    pub bpm_explicit: bool,
    pub beats_per_bar: f64,
    pub bars: f64,
    /// 第一个音距离 loop 起点的拍数（> 0.5 视为弱起/前导静音）。
    pub pickup_beats: f64,
    pub key_name: String,
    pub key_confidence: u8,
    pub key_candidates: Vec<(String, u8)>,
    /// 仅由音级分布得出的调性（与和弦序列互校）。
    pub key_from_pitches: String,
    /// 置信度偏低时的原因说明（给界面直接显示；不要在界面上写死文案）。
    pub key_note: String,
    /// 折叠并合并重复后的和弦进行。
    pub progression: Vec<String>,
    /// 每个段对应的罗马数字。
    pub romans: Vec<String>,
    pub segments: Vec<Segment>,
    pub layers: Vec<String>,
    pub warnings: Vec<String>,
    /// 只有单声部（没有和声层），此时不给和弦。
    pub single_line: bool,
    /// 音符级标注：每个音是和弦音还是旋律音（含判定依据）。
    pub notes: Vec<AnnotatedNote>,
    /// **所有候选调，以及每个调下的完整解释**。
    ///
    /// 调性判断本来就有歧义（尤其关系大小调、五度关系），与其强行挑一个，
    /// 不如把可能性摊开、带排序依据，让用户自己点。
    pub key_options: Vec<KeyOption>,
}

/// 一个候选调，连同它下面的解释。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyOption {
    pub name: String,
    /// 音级分布分（0..100，相对最高分）。
    pub pitch_score: u8,
    /// 和弦解码总代价（越小越好）。
    pub chord_cost: f64,
    /// 综合排序分（越小越靠前）= 和弦代价 + 主音证据。
    pub rank_score: f64,
    /// 开头第一个和弦就是主和弦 —— **排序的首要依据**。
    pub starts_on_tonic: bool,
    /// 结尾和弦是主和弦。
    pub ends_on_tonic: bool,
    /// 主和弦占总时长的百分比。
    pub tonic_share: u8,
    /// 低音落在主音的百分比。
    pub bass_on_tonic: u8,
    /// 是否被选为默认显示的那一个。
    pub selected: bool,
    /// 这个调下的和弦进行。
    pub progression: Vec<String>,
    /// 这个调下每段的罗马数字（与段一一对应）。
    pub romans: Vec<String>,
}

/// 主入口。
pub fn analyze(smf: &Smf) -> LoopAnalysis {
    let mut warnings: Vec<String> = smf.warnings.clone();
    let ppq = smf.ppq().max(1.0);
    let (num, den) = smf.first_time_sig();
    let beats_per_bar = if den == 0 { 4.0 } else { num as f64 * 4.0 / den as f64 };
    let beats_per_bar = if beats_per_bar <= 0.0 { 4.0 } else { beats_per_bar };
    let bpm_explicit = smf.first_bpm().is_some();
    let bpm = smf.first_bpm().unwrap_or(120.0);

    // ---- 分角色 ----
    let mut layers = Vec::new();
    let mut harmony_notes: Vec<MidiNote> = Vec::new();
    let mut single_line = false;

    let mut classified: Vec<(usize, LayerKind)> = Vec::new();
    for track in &smf.tracks {
        if track.notes.is_empty() {
            layers.push(LayerInfo {
                label: format!("轨{}（空）", track.index),
                kind: LayerKind::Empty,
            });
            classified.push((track.index, LayerKind::Empty));
            continue;
        }
        let kind = classify(track.name.as_deref(), track.notes.as_slice(), track.is_drum_only());
        let name = track
            .name
            .clone()
            .unwrap_or_else(|| format!("轨{}", track.index));
        layers.push(LayerInfo {
            label: format!(
                "{} → {}（{} 音，{}–{}）",
                name,
                kind_label(kind),
                track.notes.len(),
                track
                    .notes
                    .iter()
                    .map(|n| n.pitch)
                    .min()
                    .map(|p| pc_name(p))
                    .unwrap_or_else(|| "?".into()),
                track
                    .notes
                    .iter()
                    .map(|n| n.pitch)
                    .max()
                    .map(|p| pc_name(p))
                    .unwrap_or_else(|| "?".into()),
            ),
            kind,
        });
        classified.push((track.index, kind));
        if kind == LayerKind::Harmony {
            // 注意要按**音符**过滤通道 9：采样包里常有"和弦 + 底鼓混在同一条轨"的导出，
            // 只在轨道级判 is_drum_only 会漏掉这些鼓音，它们会被当成和弦音。
            harmony_notes.extend(track.notes.iter().filter(|n| n.channel != 9).cloned());
        }
    }

    // 没有明显的和声层：把非鼓、非空的音全用上（可能是单轨里混了旋律和伴奏）
    if harmony_notes.is_empty() {
        for (idx, kind) in &classified {
            if matches!(kind, LayerKind::Bass | LayerKind::Melody) {
                if let Some(t) = smf.tracks.iter().find(|t| t.index == *idx) {
                    harmony_notes.extend(t.notes.iter().filter(|n| n.channel != 9).cloned());
                }
            }
        }
    }

    let all_non_drum: Vec<MidiNote> = smf
        .tracks
        .iter()
        .filter(|t| !t.is_drum_only())
        .flat_map(|t| t.notes.iter().filter(|n| n.channel != 9).cloned())
        .collect();

    if all_non_drum.is_empty() {
        warnings.push("文件里没有可用音符（只有鼓或空轨）".to_string());
        return empty_result(bpm, bpm_explicit, beats_per_bar, layers, warnings);
    }

    // 单声部判定：**平均**同时发声音数低，且**最大值**也上不去。
    // 只看平均会把"点状和弦"判死（50 毫秒的 3 音和弦平均下来很近单声部）；
    // 只看最大值会被贝斯的同音重叠骗过。两个一起看才稳。
    let mean_poly = mean_polyphony(&harmony_notes);
    let max_poly = max_polyphony(&harmony_notes);
    let single_poly = mean_poly < 1.15 && max_poly < 3;
    if harmony_notes.is_empty() || single_poly {
        single_line = true;
        warnings.push(
            "没有找到和弦层（单声部素材）。只给出调性判断；要判和弦需要伴奏声部。".to_string(),
        );
    }

    // ---- 音级分布（独立于和弦的调性判据）----
    let hist = pitch_class_histogram(&all_non_drum);
    let (key_pitch_name, _, _) = key_from_pitches(&hist);

    if single_line {
        // 单声部没有和弦可解，调性只能靠音级分布——但候选照样列出来给用户挑
        let scored = key_scores_from_pitches(&hist);
        let name = scored.first().map(|(k, _)| k.full_name()).unwrap_or_default();
        let conf = (((scored.first().map(|(_, s)| *s).unwrap_or(0.0) + 0.8) / 1.8) * 100.0)
            .clamp(0.0, 100.0) as u8;
        let top = scored
            .first()
            .map(|(_, s)| *s)
            .unwrap_or(1.0)
            .abs()
            .max(1e-6);
        let cands: Vec<(String, u8)> = scored.iter().take(3).map(|(k, _)| (k.full_name(), conf)).collect();
        let key_options: Vec<KeyOption> = scored
            .iter()
            .take(6)
            .enumerate()
            .map(|(i, (k, s))| KeyOption {
                name: k.full_name(),
                pitch_score: ((s / top) * 100.0).clamp(0.0, 100.0) as u8,
                chord_cost: 0.0,
                rank_score: 0.0,
                starts_on_tonic: false,
                ends_on_tonic: false,
                tonic_share: 0,
                bass_on_tonic: 0,
                selected: i == 0,
                progression: Vec::new(),
                romans: Vec::new(),
            })
            .collect();
        // 单声部也要给音符级标注：全部标"旋律"，让界面能显示"这不是和弦"
        let notes = annotate_single_line(&all_non_drum, ppq);
        return LoopAnalysis {
            bpm,
            bpm_explicit,
            beats_per_bar,
            bars: estimate_bars(smf, ppq, beats_per_bar),
            pickup_beats: estimate_pickup(&all_non_drum, ppq),
            key_name: name.clone(),
            key_confidence: conf,
            key_candidates: cands,
            key_from_pitches: name,
            key_note: String::new(),
            progression: Vec::new(),
            romans: Vec::new(),
            segments: Vec::new(),
            layers: layers.into_iter().map(|l| l.label).collect(),
            warnings,
            single_line: true,
            notes,
            key_options,
        };
    }

    // ---- 网格剖面 ----
    let step = (ppq / GRID_PER_BEAT as f64).max(1.0);
    let end_tick = all_non_drum.iter().map(|n| n.end()).max().unwrap_or(0);
    let total_cells = ((end_tick as f64 / step).ceil() as u64).max(1);

    let cluster = build_cluster_map(&harmony_notes);
    let cells = build_cells(&harmony_notes, ppq, step, total_cells, &cluster, beats_per_bar);

    // ---- 调性候选：由**全部音符的音级分布**决定，与和弦无关 ----
    //
    // 调性看"用了哪些音、各占多少"就够了；关系大小调的调内音完全相同，
    // 会一起进入候选，再交给下面的解码用主音证据裁决。
    let key_scores = key_scores_from_pitches(&hist);
    // 容差放在这里而不是写死在函数里：放宽 → 候选更多（用户要的自己挑），
    // 收紧 → 候选更少（默认答案更"自信"）。0.2 是实测下来覆盖率与可读性的平衡点。
    let key_cands = pick_key_candidates(&key_scores, 0.3);

    // ---- 解码：在候选调里逐个解码，**保留全部候选的解释** ----
    let (segments, dp_key, key_options) = match solve_all(&cells, &key_cands) {
        Some(v) => v,
        None => return empty_result(bpm, bpm_explicit, beats_per_bar, layers, warnings),
    };

    // ---- 计算段信息 ----
    let mut out_segments: Vec<Segment> = Vec::new();
    for seg in &segments {
        let start_beat = seg.start_cell as f64 / GRID_PER_BEAT as f64;
        let len_beats = (seg.end_cell - seg.start_cell) as f64 / GRID_PER_BEAT as f64;
        let bar = (start_beat / beats_per_bar).floor() + 1.0;
        let beat_in_bar = start_beat % beats_per_bar + 1.0;
        out_segments.push(Segment {
            start_beat,
            len_beats,
            bar,
            beat_in_bar,
            chord: seg.chord.clone(),
            pedal_bass: seg.pedal,
            chord_tones: seg.tones.iter().map(|&p| pc_name(p)).collect(),
            excluded: seg.extras.iter().map(|&p| pc_name(p)).collect(),
            voicing: Vec::new(),
        });
    }

    // ---- 调性：默认显示排第一的候选（其余在 `key_options` 里交给用户挑）----
    let key_name = dp_key.full_name();
    // 置信度看"默认调与次优候选的每格平均差距"：
    // 用总差距会被小节数稀释（长曲子必然拉得开），必须按格归一化。
    let cells_n = cells.len().max(1) as f64;
    let gap = if key_options.len() > 1 {
        (key_options[1].rank_score - key_options[0].rank_score).abs()
    } else {
        20.0
    };
    let gap_pc = gap / cells_n;
    let mut conf: f64 = if gap_pc >= 0.08 {
        90.0
    } else if gap_pc >= 0.04 {
        75.0
    } else if gap_pc >= 0.015 {
        60.0
    } else {
        45.0
    };
    // 两个独立判据（和弦序列 / 音级分布）指向同一个调时，提高可信度
    if key_name == key_pitch_name {
        conf = (conf + 8.0).min(95.0);
    }
    let key_confidence = conf as u8;
    // 置信度低时说明**具体原因**，交给界面显示——文案由这里决定，前端不要写死
    let key_note = if key_confidence >= 60 {
        String::new()
    } else if gap < 2.0 {
        format!(
            "两个调的调内和弦几乎完全一样（关系大小调），只能靠结束和弦与低音判断；另一个可能是 {}",
            key_options
                .get(1)
                .map(|o| o.name.as_str())
                .unwrap_or("—")
        )
    } else {
        "候选调得分接近，建议核对低音落点与结束和弦".to_string()
    };
    let key_candidates: Vec<(String, u8)> = key_options
        .iter()
        .map(|o| (o.name.clone(), o.pitch_score))
        .collect();

    let romans: Vec<String> = segments
        .iter()
        .map(|s| progression::analyze_role(&dp_key, &to_chord(&s.chord)).roman)
        .collect();

    // 两个调性判据不一致时提示（关系大小调最容易打架）
    if !key_name.contains(&key_pitch_name.split(' ').next().unwrap_or("")) {
        warnings.push(format!(
            "和弦序列看是 {}，音级分布看是 {}（短 loop 的关系大小调常有歧义，建议核对低音与终止感）",
            key_name, key_pitch_name
        ));
    }

    let progression = fold_progression(&segments);

    // 音符级标注 + 每段的代表音（同音高类只留最"实在"的一个）
    let annotated = annotate_notes(&harmony_notes, &segments, ppq, &cluster);
    fill_voicing(&mut out_segments, &annotated);

    LoopAnalysis {
        bpm,
        bpm_explicit,
        beats_per_bar,
        bars: estimate_bars(smf, ppq, beats_per_bar),
        pickup_beats: estimate_pickup(&all_non_drum, ppq),
        key_name,
        key_confidence,
        key_candidates,
        key_from_pitches: key_pitch_name,
        key_note,
        progression,
        romans,
        segments: out_segments,
        layers: layers
            .into_iter()
            .filter(|l| l.kind != LayerKind::Empty)
            .map(|l| l.label)
            .collect(),
        warnings,
        single_line: false,
        notes: annotated,
        key_options,
    }
}

fn empty_result(
    bpm: f64,
    bpm_explicit: bool,
    beats_per_bar: f64,
    layers: Vec<LayerInfo>,
    warnings: Vec<String>,
) -> LoopAnalysis {
    LoopAnalysis {
        bpm,
        bpm_explicit,
        beats_per_bar,
        bars: 0.0,
        pickup_beats: 0.0,
        key_name: "—".to_string(),
        key_confidence: 0,
        key_candidates: Vec::new(),
        key_from_pitches: "—".to_string(),
        key_note: String::new(),
        progression: Vec::new(),
        romans: Vec::new(),
        segments: Vec::new(),
        layers: layers.into_iter().map(|l| l.label).collect(),
        warnings,
        single_line: true,
        notes: Vec::new(),
        key_options: Vec::new(),
    }
}

fn kind_label(k: LayerKind) -> &'static str {
    match k {
        LayerKind::Drums => "鼓",
        LayerKind::Bass => "贝斯",
        LayerKind::Harmony => "和声",
        LayerKind::Melody => "旋律",
        LayerKind::Empty => "空",
    }
}

fn pc_name(pitch: u8) -> String {
    Note::new(pitch).name()
}

/// 轨道角色判定：音名提示 + 音域 + 复音度。
fn classify(name: Option<&str>, notes: &[MidiNote], drum_only: bool) -> LayerKind {
    if drum_only {
        return LayerKind::Drums;
    }
    if notes.is_empty() {
        return LayerKind::Empty;
    }
    let lower = name.unwrap_or("").to_lowercase();
    if lower.contains("drum") || lower.contains("kick") || lower.contains("snare") || lower.contains("hat") {
        return LayerKind::Drums;
    }

    let poly = max_polyphony(notes);
    let mean: f64 = notes.iter().map(|n| n.pitch as f64).sum::<f64>() / notes.len() as f64;

    if lower.contains("bass") || lower.contains("808") {
        return LayerKind::Bass;
    }
    if lower.contains("chord") || lower.contains("pad") || lower.contains("harmon") || lower.contains("keys") {
        return LayerKind::Harmony;
    }
    if poly >= 2 {
        return LayerKind::Harmony;
    }
    if mean < 52.0 {
        return LayerKind::Bass;
    }
    LayerKind::Melody
}

/// 同时发声音数的最大值。
fn max_polyphony(notes: &[MidiNote]) -> usize {
    if notes.is_empty() {
        return 0;
    }
    let mut events: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
    for n in notes {
        events.push((n.start, 1));
        events.push((n.end().max(n.start + 1), -1));
    }
    // 同一 tick 先处理结束再处理开始，避免把"接续"算成同时
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut cur = 0i32;
    let mut max = 0i32;
    for (_, d) in events {
        cur += d;
        if cur > max {
            max = cur;
        }
    }
    max as usize
}

/// 同时发声音数的**时间加权平均**（用均值判单声部，不能用最大值）。
///
/// 贝斯/旋律常有同音重叠或快速重复，最大值会被撑到 2 以上，
/// 于是一条纯单声部的素材被误判成"有和声层"，进而硬凑出和弦名。
fn mean_polyphony(notes: &[MidiNote]) -> f64 {
    if notes.is_empty() {
        return 0.0;
    }
    let mut events: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
    for n in notes {
        events.push((n.start, 1));
        events.push((n.end().max(n.start + 1), -1));
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let first = events.first().map(|e| e.0).unwrap_or(0);
    let mut cur = 0i32;
    let mut last = first;
    let mut area = 0f64;
    for (t, d) in &events {
        let dt = t.saturating_sub(last) as f64;
        if dt > 0.0 {
            area += cur as f64 * dt;
            last = *t;
        }
        cur += *d;
    }
    let span = (last.saturating_sub(first)) as f64;
    if span <= 0.0 {
        return max_polyphony(notes) as f64;
    }
    area / span
}

fn pitch_class_histogram(notes: &[MidiNote]) -> [f64; 12] {
    let mut h = [0.0f64; 12];
    for n in notes {
        let dur = n.dur.max(1) as f64;
        h[(n.pitch % 12) as usize] += dur;
    }
    let total: f64 = h.iter().sum();
    if total > 0.0 {
        for v in h.iter_mut() {
            *v /= total;
        }
    }
    h
}

/// 用全部音符的音级分布给 24 个调打分，按分数降序返回。
///
/// **这一步与和弦识别无关**——调性主要由"用了哪些音、各占多少"决定。
/// 声部、节奏、和弦怎么切分都不影响它，所以它比"靠和弦反推调性"稳。
fn key_scores_from_pitches(hist: &[f64; 12]) -> Vec<(Key, f64)> {
    let mut scored: Vec<(Key, f64)> = Vec::with_capacity(24);
    for key in Key::all() {
        let scale = key.natural_mask();
        let mut score = 0.0f64;
        // 调内音加分、调外音扣分（扣得比加得多，因为一个调外音是很强的信号）
        for pc in 0..12 {
            let in_scale = scale & (1u16 << pc) != 0;
            score += hist[pc] * if in_scale { 1.0 } else { -0.8 };
        }
        // 主音、三音、五音的权重更高（旋律常落在这些音上）
        let tonic = key.tonic().pitch as usize;
        score += hist[tonic] * 0.5;
        let third = (tonic + if key.is_major() { 4 } else { 3 }) % 12;
        score += hist[third] * 0.22;
        let fifth = (tonic + 7) % 12;
        score += hist[fifth] * 0.15;
        // 调号越复杂越不优先（同分时的取舍）
        score -= key.accidentals().abs() as f64 * 0.004;
        scored.push((key, score));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// 只靠音级分布的调性判断（与和弦序列互校用）。
fn key_from_pitches(hist: &[f64; 12]) -> (String, u8, Vec<(String, u8)>) {
    let scored = key_scores_from_pitches(hist);
    let cands: Vec<(String, u8)> = scored
        .iter()
        .take(3)
        .map(|(k, s)| {
            (
                k.full_name(),
                ((s + 0.8) / 1.8 * 100.0).clamp(0.0, 100.0).round() as u8,
            )
        })
        .collect();
    let name = cands.first().map(|c| c.0.clone()).unwrap_or_default();
    let conf = cands.first().map(|c| c.1).unwrap_or(0);
    (name, conf, cands)
}

/// 从音级分布的排序里挑出"分数足够接近"的调，作为和弦解码的候选。
///
/// 关系大小调的调内音完全相同，音级分布也几乎一样，所以它们会一起被选进来；
/// 差得远的调直接排除——没必要为它们跑一遍解码。
fn pick_key_candidates(scored: &[(Key, f64)], tolerance: f64) -> Vec<(Key, f64)> {
    let best = scored.first().map(|(_, s)| *s).unwrap_or(0.0);
    let mut out: Vec<(Key, f64)> = scored
        .iter()
        .filter(|(_, s)| best - s <= tolerance)
        .cloned()
        .collect();
    // 至少给 3 个，且最多 8 个（再多就是音级分布没区分度了）
    if out.len() < 3 {
        out = scored.iter().take(3).cloned().collect();
    }
    out.truncate(8);
    out
}

fn estimate_bars(smf: &Smf, ppq: f64, beats_per_bar: f64) -> f64 {
    let end = smf.end_tick() as f64;
    if end <= 0.0 {
        return 0.0;
    }
    let beats = end / ppq;
    (beats / beats_per_bar * 100.0).round() / 100.0
}

fn estimate_pickup(notes: &[MidiNote], ppq: f64) -> f64 {
    let first = notes.iter().map(|n| n.start).min().unwrap_or(0);
    let beats = first as f64 / ppq;
    if beats < 0.25 {
        0.0
    } else {
        (beats * 100.0).round() / 100.0
    }
}

struct RawSegment {
    start_cell: u64,
    end_cell: u64,
    chord: BasicChord,
    pedal: bool,
    tones: Vec<u8>,
    extras: Vec<u8>,
}

/// 一个音的"和声权重"：时值 × 是否成簇，取值 0..1。
///
/// 采样包的混装形态是「和弦音长、旋律音短而零散」，所以时值是主判据；
/// 「同起点是否成簇」用来救回点状和弦（时值虽短，但多个音一起响）。
fn note_weight(dur_beats: f64, clustered: bool) -> f64 {
    let w_dur = if dur_beats >= 0.5 {
        1.0
    } else if dur_beats >= 0.25 {
        0.8
    } else if dur_beats >= 0.125 {
        0.5
    } else {
        0.15
    };
    let w_cluster = if clustered { 1.0 } else { 0.55 };
    w_dur * w_cluster
}

/// 每个起音刻度上有几个音（用于判断同起点是否成簇）。
fn build_cluster_map(notes: &[MidiNote]) -> HashMap<u64, u32> {
    let mut m: HashMap<u64, u32> = HashMap::new();
    for n in notes {
        *m.entry(n.start).or_insert(0) += 1;
    }
    m
}

/// 该刻度附近是否有另一个音一起响（容忍微小的演奏偏移）。
fn is_clustered(map: &HashMap<u64, u32>, tick: u64) -> bool {
    let mut count = 0u32;
    if let Some(&v) = map.get(&tick) {
        count += v;
    }
    for d in 1..=CLUSTER_TOLERANCE {
        if let Some(&v) = map.get(&tick.saturating_add(d)) {
            count += v;
        }
        if tick >= d {
            if let Some(&v) = map.get(&(tick - d)) {
                count += v;
            }
        }
    }
    count >= 2
}

fn format_symbol(
    root: u8,
    triad: &str,
    seventh: bool,
    seventh_interval: u8,
    bass: u8,
    slash: bool,
) -> String {
    let name = Note::new(root).name();
    let mut s = String::new();
    s.push_str(&name);
    if seventh {
        match (triad, seventh_interval) {
            ("m", 10) => s.push_str("m7"),
            ("m", 11) => s.push_str("mM7"),
            ("dim", 10) => s.push_str("m7b5"),
            ("dim", 11) => s.push_str("dimM7"),
            ("", 11) => s.push_str("maj7"),
            (t, 10) => {
                s.push_str(t);
                s.push('7');
            }
            (t, _) => {
                s.push_str(t);
                s.push_str("maj7");
            }
        }
    } else {
        s.push_str(triad);
    }
    if slash {
        s.push('/');
        s.push_str(&Note::new(bass).name());
    }
    s
}

/// 连续相同的和弦合并成一条进行。
fn fold_progression(segments: &[RawSegment]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in segments {
        let sym = s.chord.symbol.clone();
        if out.last() != Some(&sym) {
            out.push(sym);
        }
    }
    out
}

/// 转成现有的 `Chord`（供调性打分/罗马数字复用）。
///
/// 注意：`notes` 必须给出**完整的骨架音**（根/三/五/七），不能只给根音——
/// `scoring.rs` 是按 `Chord::mask()` 判断"是否全在调内"的，缺音会让调性判定失准。
fn to_chord(b: &BasicChord) -> Chord {
    let root = Note::new(b.root);
    let mut notes: Vec<Note> = b.skeleton_pcs().iter().map(|&p| Note::new(p)).collect();
    notes.sort_by_key(|n| n.pitch);
    notes.dedup();
    let kind = if b.seventh { Kind::Seventh } else { Kind::Triad };
    Chord {
        notes,
        root,
        bass: Note::new(b.bass),
        suffix: b.symbol.clone(),
        quality: b.quality(),
        kind,
        label: String::new(),
        missing: Vec::new(),
        extras: Vec::new(),
        exact: true,
        score: 0,
        warning: None,
    }
}

// =============================================================
// 全局解码：枚举「调 + 调内级数和弦」，用动态规划求整条最优序列
//
// 思路（不猜分段、不剪枝）：
//   4 小节的 loop 状态空间很小 —— 24 个调 × 7 个级数 × (三和弦 | 七和弦) = 336 个候选。
//   对每个调跑一遍动态规划，代价 = Σ(每格的匹配代价 + 换和弦代价)，取总代价最小的调。
//   分段不是"找边界"，而是解码结果的副产物：连续落在同一个候选上的格，就是一段。
//
// 这样做的两个好处：
//   1. 调性与和弦序列互相约束，不再"先定调再识别"（那一步错就全错）；
//   2. 「一小节两个和弦」由解码器自己决定，不需要固定窗口。
// =============================================================

/// 一个分析格的加权剖面。
struct Cell {
    /// 每个音高类的加权存在度（0..1）。
    weighted: [f64; 12],
    /// 加权最低音。
    bass: u8,
    /// 拍位强度：正拍 1.0，弱拍更低（和弦换在强拍上更划算）。
    metric: f64,
}

/// 候选和弦：某个调里的某一级。
#[derive(Clone)]
struct Cand {
    root: u8,
    triad: &'static str,
    seventh: bool,
    seventh_interval: u8,
    mask: u16,
    third_pc: u8,
    fifth_pc: u8,
    /// 是否是「借用」性质（不是严格调内）：用的时候要付额外代价。
    borrowed: bool,
}

/// 调内的三和弦性质（大三 / 小三 / 减三）。
fn triad_of(key: &Key, degree: u8) -> (u8, &'static str) {
    if key.is_major() {
        match degree {
            1 | 4 | 5 => (4, ""),
            2 | 3 | 6 => (3, "m"),
            7 => (3, "dim"),
            _ => (4, ""),
        }
    } else {
        match degree {
            1 | 4 | 5 => (3, "m"),
            2 => (3, "dim"),
            3 | 6 | 7 => (4, ""),
            _ => (3, "m"),
        }
    }
}

/// 调内七音的半音数（10 = 小七，11 = 大七）。
fn seventh_of(key: &Key, degree: u8) -> u8 {
    if key.is_major() {
        match degree {
            1 | 4 => 11,
            _ => 10,
        }
    } else {
        match degree {
            3 | 6 => 11,
            _ => 10,
        }
    }
}

/// 某个调的全部候选：7 个级数 × {三和弦, 七和弦}，外加少量「借用（借来）」变体。
fn candidates_for_key(key: &Key) -> Vec<Cand> {
    let mut out: Vec<Cand> = Vec::with_capacity(18);
    for degree in 1..=7u8 {
        let root = match key.degree_note(degree) {
            Some(n) => n.pitch,
            None => continue,
        };
        let (t3, triad) = triad_of(key, degree);
        let t5_off = if triad == "dim" { 6 } else { 7 };
        let third_pc = (root + t3) % 12;
        let fifth_pc = (root + t5_off) % 12;
        let mask3 = (1u16 << root) | (1u16 << third_pc) | (1u16 << fifth_pc);
        out.push(Cand {
            root,
            triad,
            seventh: false,
            seventh_interval: 10,
            mask: mask3,
            third_pc,
            fifth_pc,
            borrowed: false,
        });

        let s7 = seventh_of(key, degree);
        // 大调 vii 级、小调 ii 级的七和弦是半减七，三和弦本来就标成 dim
        let mask7 = mask3 | (1u16 << ((root + s7) % 12));
        out.push(Cand {
            root,
            triad,
            seventh: true,
            seventh_interval: s7,
            mask: mask7,
            third_pc,
            fifth_pc,
            borrowed: false,
        });

        // 借用变体：调内算出来是**减三**的级数（大调 vii、小调 ii），
        // 实际演奏里常被弹成小三（爵士/lo-fi 里 vii° 太古典，vii7 更常用）。
        //
        // 实测案例：`Bbmaj7 → Am7 → Gm7` 的降 B 大调读法是 `I - vii - vi`，
        // 但 vii 级给了减三（A C Eb），匹配不上 `Am7`（A C E G），
        // 于是整个 loop 被判成 F 大调（IV-iii-ii）。
        //
        // **必须带 borrowed 标记**：不带代价地放开，等于每个调都能解释任何和弦，
        // 区分度会崩（试过不改代价，弄坏 3 个原本正确的文件）。
        if triad == "dim" {
            let third_m = (root + 3) % 12;
            let fifth_m = (root + 7) % 12;
            let mask_m = (1u16 << root) | (1u16 << third_m) | (1u16 << fifth_m);
            out.push(Cand {
                root,
                triad: "m",
                seventh: false,
                seventh_interval: 10,
                mask: mask_m,
                third_pc: third_m,
                fifth_pc: fifth_m,
                borrowed: true,
            });
            out.push(Cand {
                root,
                triad: "m",
                seventh: true,
                seventh_interval: 10,
                mask: mask_m | (1u16 << ((root + 10) % 12)),
                third_pc: third_m,
                fifth_pc: fifth_m,
                borrowed: true,
            });
        }
    }
    out
}

/// 借用一个和弦的额外代价（比调内和弦贵，但远小于「调外」）。
///
/// 这个值是按**格**累加的，而借用和弦可能占半首曲子（实测有一例占 16/64 格），
/// 所以必须定得很轻：0.55 就足以压过主音证据、把本来正确的调判翻。
/// 0.18 是试出来的平衡点——既能让借用成立，又不至于淹没调性证据。
const COST_BORROWED: f64 = 0.18;

/// 某一格用某个候选来解释的代价（越小越好）。
fn emit_cost(cell: &Cell, c: &Cand) -> f64 {
    let mut cost = 0.0;
    // 借用（非严格调内）的和弦要贵一点：能解释，但不是首选
    if c.borrowed {
        cost += COST_BORROWED;
    }
    // 缺音：根音和三音是"身份"，缺了就重罚；五音最常被省略，轻罚
    if cell.weighted[c.root as usize] < 0.20 {
        cost += 1.2;
    }
    if cell.weighted[c.third_pc as usize] < 0.20 {
        cost += 1.2;
    }
    if cell.weighted[c.fifth_pc as usize] < 0.15 {
        cost += 0.25;
    }
    // 多余音：明显响着却不属于这个和弦
    for pc in 0..12usize {
        if c.mask & (1u16 << pc) != 0 {
            continue;
        }
        let w = cell.weighted[pc];
        if w > 0.30 {
            cost += 0.6;
        }
    }
    // 低音线索：最低音正好是根音，加分
    if cell.bass == c.root {
        cost -= 0.35;
    } else if c.mask & (1u16 << cell.bass) != 0 {
        cost -= 0.10;
    }
    cost
}

/// 从上一个格子的和弦换到当前格子的代价。
fn trans_cost(a: &Cand, b: &Cand, metric: f64) -> f64 {
    let same = a.root == b.root && a.seventh == b.seventh && a.triad == b.triad;
    if same {
        return 0.0;
    }
    // 同根音、只是加了七音（C → C7）：很常见，便宜
    if a.root == b.root {
        return 0.30;
    }
    // 换和弦：落在强拍上便宜，弱拍上贵
    0.45 + 1.0 * (1.0 - metric)
}

/// 拍位强度。
fn beat_metric(cell_index: u64, beats_per_bar: f64) -> f64 {
    let beat_pos = cell_index as f64 / GRID_PER_BEAT as f64;
    let in_bar = beat_pos % beats_per_bar;
    let frac = in_bar.fract();
    if frac.abs() < 1e-6 {
        // 整拍：第 1 拍最强，其次第 3 拍
        if in_bar.abs() < 1e-6 || (in_bar - 2.0).abs() < 1e-6 {
            1.0
        } else {
            0.6
        }
    } else if (frac - 0.5).abs() < 1e-6 {
        0.4
    } else {
        0.2
    }
}

/// 按格构建加权剖面。
fn build_cells(
    notes: &[MidiNote],
    ppq: f64,
    step: f64,
    total: u64,
    cluster: &HashMap<u64, u32>,
    beats_per_bar: f64,
) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(total as usize);
    for i in 0..total {
        let from = (i as f64 * step) as u64;
        let to = ((i + 1) as f64 * step) as u64;
        let span = (to.saturating_sub(from)).max(1) as f64;
        let mut weighted = [0f64; 12];
        let mut bass: Option<(f64, u8)> = None;
        for n in notes {
            if n.start < to && n.end() > from {
                let s = n.start.max(from);
                let e = n.end().min(to);
                let d = (e.saturating_sub(s)) as f64;
                let w = note_weight(n.dur as f64 / ppq, is_clustered(cluster, n.start));
                weighted[(n.pitch % 12) as usize] += d * w / span;
                let score = d * w;
                let better = match bass {
                    Some((pw, _)) => score > pw,
                    None => true,
                };
                if better {
                    bass = Some((score, n.pitch));
                }
            }
        }
        for v in weighted.iter_mut() {
            *v = v.min(1.0);
        }
        cells.push(Cell {
            weighted,
            bass: bass.map(|(_, p)| p % 12).unwrap_or(0),
            metric: beat_metric(i, beats_per_bar),
        });
    }
    cells
}

/// 对某个调跑动态规划，返回（总代价, 每格选中的候选下标, 候选表）。
fn dp_for_key(key: &Key, cells: &[Cell]) -> (f64, Vec<usize>, Vec<Cand>) {
    let cands = candidates_for_key(key);
    let n = cells.len();
    let m = cands.len();
    if n == 0 || m == 0 {
        return (f64::INFINITY, Vec::new(), cands);
    }

    let mut dp: Vec<f64> = (0..m).map(|c| emit_cost(&cells[0], &cands[c])).collect();
    let mut back: Vec<Vec<usize>> = vec![vec![0usize; m]; n];

    for i in 1..n {
        let mut next = vec![f64::INFINITY; m];
        let mut ec = [0f64; 32];
        for c in 0..m {
            ec[c] = emit_cost(&cells[i], &cands[c]);
        }
        for c in 0..m {
            let mut best = f64::INFINITY;
            let mut bp = 0usize;
            for p in 0..m {
                if dp[p].is_infinite() {
                    continue;
                }
                let v = dp[p] + trans_cost(&cands[p], &cands[c], cells[i].metric);
                if v < best {
                    best = v;
                    bp = p;
                }
            }
            next[c] = best + ec[c];
            back[i][c] = bp;
        }
        dp = next;
    }

    let mut best = f64::INFINITY;
    let mut bc = 0usize;
    for c in 0..m {
        if dp[c] < best {
            best = dp[c];
            bc = c;
        }
    }
    let mut path = vec![0usize; n];
    path[n - 1] = bc;
    for i in (1..n).rev() {
        path[i - 1] = back[i][path[i]];
    }
    (best, path, cands)
}

fn cand_to_basic(c: &Cand, bass: u8) -> BasicChord {
    let slash = bass != c.root && c.mask & (1u16 << bass) == 0;
    BasicChord {
        root: c.root,
        triad_suffix: c.triad,
        seventh: c.seventh,
        seventh_interval: c.seventh_interval,
        tensions: Vec::new(),
        bass,
        slash,
        symbol: format_symbol(c.root, c.triad, c.seventh, c.seventh_interval, bass, slash),
        confidence: 85,
    }
}

/// 主音证据：关系大小调（C 大调 / A 小调）的**调内和弦完全一样**，
/// 光比"和弦能不能对上"永远分不出谁是谁。只能看主音落在哪里：
/// 结尾和弦、开头和弦、主和弦占比、低音落点。
///
/// 返回的是代价修正（负数 = 更可信）。
fn tonic_evidence(key: &Key, path: &[usize], cands: &[Cand], cells: &[Cell]) -> f64 {
    if path.is_empty() || cells.is_empty() {
        return 0.0;
    }
    let tonic = key.tonic().pitch;
    let mut delta = 0.0;

    // 结尾落在主和弦：最强的终止证据（loop 常结束在主和弦上）
    if let Some(&last) = path.last() {
        if cands[last].root == tonic {
            delta -= 2.5;
        }
    }
    // 开头落在主和弦。权重给得高：循环 loop 常从主和弦起，
    // 而"把真正的主和弦当成别人的 IV / V"是实测里最常见的误判来源。
    //
    // 五度关系的调（如降 D 大调 vs 降 A 大调）音阶只差一个音、和弦集合高度重叠，
    // 只有"谁是开头那个和弦"能分开它们，所以这个权重必须压得住 emit 的微小差异。
    if let Some(&first) = path.first() {
        if cands[first].root == tonic {
            delta -= 3.0;
        }
    }
    // 主和弦占总时长的比例
    let tonic_cells = path.iter().filter(|&&c| cands[c].root == tonic).count();
    delta -= (tonic_cells as f64 / path.len() as f64) * 3.0;

    // 主和弦一次都没出现 → 惩罚。
    //
    // 这是区分"邻近调"的关键：`Bbmaj7 → Am7 → Gm7` 在降 B 大调（I-vii-vi）
    // 和 F 大调（IV-iii-ii）里和弦名完全一样，但 F 大调里**根本没有 F 和弦**——
    // 一个调的主和弦通常总会以某种形式出现，完全不出现就是疑点。
    if tonic_cells == 0 {
        delta += 2.0;
    }

    // 低音落在主音的时间占比——低音是判断调性最可靠的线索之一
    let bass_on_tonic = cells.iter().filter(|c| c.bass == tonic).count();
    delta -= (bass_on_tonic as f64 / cells.len() as f64) * 1.5;

    delta
}

/// 一个候选调的解码结果（内部用）。
struct KeySolution {
    key: Key,
    pitch_score: f64,
    chord_cost: f64,
    rank_score: f64,
    path: Vec<usize>,
    cands: Vec<Cand>,
    starts_on_tonic: bool,
    ends_on_tonic: bool,
    tonic_share: f64,
    bass_on_tonic: f64,
}

/// 把解码路径压成和弦段（连续落在同一候选上的格 = 一段）。
fn compress_path(path: &[usize], cands: &[Cand], cells: &[Cell]) -> Vec<RawSegment> {
    let mut segs: Vec<RawSegment> = Vec::new();
    let mut i = 0usize;
    while i < path.len() {
        let ci = path[i];
        let mut j = i + 1;
        while j < path.len() && path[j] == ci {
            j += 1;
        }
        let c = &cands[ci];
        let bass = cells[i].bass;
        let basic = cand_to_basic(c, bass);
        let tones: Vec<u8> = (0..12u8).filter(|&pc| c.mask & (1u16 << pc) != 0).collect();
        let extras: Vec<u8> = (0..12u8)
            .filter(|&pc| c.mask & (1u16 << pc) == 0 && cells[i].weighted[pc as usize] > 0.30)
            .collect();
        segs.push(RawSegment {
            start_cell: i as u64,
            end_cell: j as u64,
            chord: basic,
            pedal: false,
            tones,
            extras,
        });
        i = j;
    }
    segs
}

/// 在候选调里逐个解码，**返回全部候选的解**，而不只是最优的那一个。
///
/// 调性本来就有歧义（关系大小调、五度关系都能自圆其说），与其强行挑一个，
/// 不如把每种可能性连同它的解释一起交给用户。
///
/// **排序依据**：开头和弦是不是主和弦优先（循环 loop 常从主和弦起），
/// 其次是"和弦代价 + 主音证据"的综合分。
fn solve_all(
    cells: &[Cell],
    scored: &[(Key, f64)],
) -> Option<(Vec<RawSegment>, Key, Vec<KeyOption>)> {
    if cells.is_empty() || scored.is_empty() {
        return None;
    }
    let mut sols: Vec<KeySolution> = Vec::new();
    for (key, pscore) in scored {
        let (cost, path, cands) = dp_for_key(key, cells);
        if cost.is_infinite() {
            continue;
        }
        let ev = tonic_evidence(key, &path, &cands, cells);
        let tonic = key.tonic().pitch;
        let starts = path.first().map(|&c| cands[c].root == tonic).unwrap_or(false);
        let ends = path.last().map(|&c| cands[c].root == tonic).unwrap_or(false);
        let share = path.iter().filter(|&&c| cands[c].root == tonic).count() as f64
            / path.len().max(1) as f64;
        let bass = cells.iter().filter(|c| c.bass == tonic).count() as f64
            / cells.len().max(1) as f64;
        sols.push(KeySolution {
            key: key.clone(),
            pitch_score: *pscore,
            chord_cost: cost,
            rank_score: cost + ev,
            path,
            cands,
            starts_on_tonic: starts,
            ends_on_tonic: ends,
            tonic_share: share,
            bass_on_tonic: bass,
        });
    }
    if sols.is_empty() {
        return None;
    }

    // 排序：综合分（已含"开头/结尾是主和弦"的奖励）。
    //
    // 注意这里**不能**把 starts_on_tonic 当硬排序键：实测 `Dm7 → G7 → Bm7b5 → Am7`
    // 开头正好是 Dm7，于是 D 小调排第一——但它有 G7、Bm7b5 两个借用和弦，
    // 明显不如"全程调内的 A 小调"。开头是主和弦只是一个信号，不是结论。
    // 该信号另外以 `starts_on_tonic` 字段给到界面，由用户自己判断。
    sols.sort_by(|a, b| {
        a.rank_score
            .partial_cmp(&b.rank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 音级分布分归一化到 0..100（相对最高分）
    let top_pitch = sols
        .iter()
        .map(|s| s.pitch_score)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut options: Vec<KeyOption> = Vec::with_capacity(sols.len());
    let mut best_segs: Vec<RawSegment> = Vec::new();
    let best_key = sols[0].key.clone();
    for (i, s) in sols.iter().enumerate() {
        let segs = compress_path(&s.path, &s.cands, cells);
        let progression = fold_progression(&segs);
        let romans: Vec<String> = segs
            .iter()
            .map(|seg| progression::analyze_role(&s.key, &to_chord(&seg.chord)).roman)
            .collect();
        if i == 0 {
            best_segs = segs;
        }
        options.push(KeyOption {
            name: s.key.full_name(),
            pitch_score: ((s.pitch_score / top_pitch.max(1e-6)) * 100.0).clamp(0.0, 100.0) as u8,
            chord_cost: (s.chord_cost * 100.0).round() / 100.0,
            rank_score: (s.rank_score * 100.0).round() / 100.0,
            starts_on_tonic: s.starts_on_tonic,
            ends_on_tonic: s.ends_on_tonic,
            tonic_share: (s.tonic_share * 100.0).round() as u8,
            bass_on_tonic: (s.bass_on_tonic * 100.0).round() as u8,
            selected: i == 0,
            progression,
            romans,
        });
    }
    Some((best_segs, best_key, options))
}

// =============================================================
// 音符级标注：每个音是和弦音还是旋律音
//
// 这一层的用途是**给人核对**：可视化时把每个音符按角色上色，
// 判错的音会立刻暴露（比如某个旋律音被涂成了"五音"）。
// =============================================================

/// MIDI 音高 → 带八度的音名（MIDI 60 = C4）。
fn pitch_name(pitch: u8) -> String {
    let pc = (pitch % 12) as usize;
    let octave = pitch as i32 / 12 - 1;
    format!("{}{}", crate::note::SHARP_NAMES[pc], octave)
}

/// 在一段里找"上层分界"（下面弹和弦、上面走旋律的典型谱型）。
///
/// 两个信号一起用，只用任何一个都会失灵：
/// 1. 音高上有一个明显空隙（≥ 5 半音）；
/// 2. 空隙**上方**的音平均时值明显更短（≤ 下方的 60%）。
///
/// - 只看音隙：和弦带延伸音时，上层和和弦会连成一片（实测有一例只差 8 半音）；
/// - 只看时值：旋律里有长音时会被并进和弦。
///
/// 返回分界音高：`>=` 它的音算上层。
fn find_upper_split(notes: &mut Vec<(u8, f64)>) -> Option<u8> {
    if notes.len() < 3 {
        return None;
    }
    // 同一音高会出现多次，聚合成"该音高上的最长时值"
    notes.sort_by_key(|n| n.0);
    let mut levels: Vec<(u8, f64)> = Vec::new();
    for &(pitch, dur) in notes.iter() {
        match levels.last_mut() {
            Some(last) if last.0 == pitch => {
                if dur > last.1 {
                    last.1 = dur;
                }
            }
            _ => levels.push((pitch, dur)),
        }
    }
    if levels.len() < 3 {
        return None;
    }

    for i in 1..levels.len() {
        let above_n = levels.len() - i;
        let gap = levels[i].0.saturating_sub(levels[i - 1].0);

        // 分档：
        // - 上层有 2 个以上音高：宽松一些（音隙 ≥ 5）；
        // - 上层只有 1 个音高：要离得很远（≥ 一个八度）才敢切，
        //   否则纯和弦/pad 里一个偏高的音就会被误判成旋律。
        let min_gap = if above_n >= 2 { 5 } else { 12 };
        // 下方至少要有 2 个音高，不然切完剩下的根本不成和弦
        if i < 2 || gap < min_gap {
            continue;
        }
        let below_avg = levels[..i].iter().map(|l| l.1).sum::<f64>() / i as f64;
        let above_avg = levels[i..].iter().map(|l| l.1).sum::<f64>() / above_n as f64;
        // 上层要明显更短（旋律是零散的短音）
        if below_avg <= 0.0 || above_avg > below_avg * 0.5 {
            continue;
        }
        // 而且上层占用的时间不能多——旋律不会铺满整段
        let above_total: f64 = levels[i..].iter().map(|l| l.1).sum();
        let below_total: f64 = levels[..i].iter().map(|l| l.1).sum();
        if below_total <= 0.0 || above_total > below_total * 0.5 {
            continue;
        }
        return Some(levels[i].0);
    }
    None
}

/// 找出某个拍位落在哪个和弦段里。
fn segment_at(segments: &[RawSegment], beat: f64) -> Option<usize> {
    if segments.is_empty() {
        return None;
    }
    let idx = beat * GRID_PER_BEAT as f64;
    if idx < segments[0].start_cell as f64 {
        return Some(0);
    }
    segments
        .iter()
        .position(|s| idx >= s.start_cell as f64 && idx < s.end_cell as f64)
        // 超出最后一段的音（尾巴、延音）算进最后一段
        .or(Some(segments.len() - 1))
}

/// 给每个音符标注角色。
///
/// 判定顺序（前一条最先，命中即止）：
/// 1. 落在两个八度以上的"上层"且时值明显短于该段和弦 → 旋律（最常见：上面一条旋律线）；
/// 2. 孤立起音 + 时值短 → 旋律；
/// 3. 音高类是骨架里的根/三/五/七 → 对应角色；
/// 4. 属于和弦（骨架外）或响得明显 → 延伸音；
/// 5. 其余 → 旋律。
fn annotate_notes(
    notes: &[MidiNote],
    segments: &[RawSegment],
    ppq: f64,
    cluster: &HashMap<u64, u32>,
) -> Vec<AnnotatedNote> {
    // 先给每一段算出"上层分界"：和弦在下、旋律在上时，中间会留下一个很大的音程空隙
    let mut upper_split: Vec<Option<u8>> = vec![None; segments.len()];
    let mut seg_span: Vec<f64> = vec![0.0; segments.len()];
    for (i, seg) in segments.iter().enumerate() {
        let from = (seg.start_cell as f64 / GRID_PER_BEAT as f64 * ppq) as u64;
        let to = (seg.end_cell as f64 / GRID_PER_BEAT as f64 * ppq) as u64;
        seg_span[i] = (to.saturating_sub(from)) as f64 / ppq;
        let mut in_seg: Vec<(u8, f64)> = notes
            .iter()
            .filter(|n| n.start < to && n.end() > from)
            .map(|n| (n.pitch, n.dur as f64 / ppq))
            .collect();
        upper_split[i] = find_upper_split(&mut in_seg);
    }

    let mut out: Vec<AnnotatedNote> = Vec::with_capacity(notes.len());
    for n in notes {
        let start_beat = n.start as f64 / ppq;
        let dur_beats = n.dur as f64 / ppq;
        let seg_idx = segment_at(segments, start_beat);
        let clustered = is_clustered(cluster, n.start);
        let weight = note_weight(dur_beats, clustered);
        let pc = n.pitch % 12;

        let role = match seg_idx {
            None => NoteRole::Outside,
            Some(i) => {
                let ch = &segments[i].chord;
                let sk = ch.skeleton_pcs();
                // 判据 1：上层 + 短于该段和弦的主体 → 旋律。
                // 这是"下面和弦、上面旋律"的标准谱型，比只看时值可靠得多。
                let in_upper = upper_split[i].map(|split| n.pitch >= split).unwrap_or(false);
                let shorter_than_chord = seg_span[i] > 0.0 && dur_beats < seg_span[i] * 0.85;
                if in_upper && shorter_than_chord {
                    NoteRole::Melody
                } else {
                    // 判据 2：孤立起音 + 时值很短
                    let melody_like = !clustered && dur_beats < 0.35;
                    if melody_like {
                        NoteRole::Melody
                    } else if pc == ch.root {
                        NoteRole::Root
                    } else if sk.get(1) == Some(&pc) {
                        NoteRole::Third
                    } else if sk.get(2) == Some(&pc) {
                        NoteRole::Fifth
                    } else if ch.seventh && sk.get(3) == Some(&pc) {
                        NoteRole::Seventh
                    } else if sk.contains(&pc) || weight >= 0.30 {
                        // 属于和弦但不在骨架，或者响得明显：延伸音。
                        // 也可能是和弦判错了——正是要让它在可视化里显眼。
                        NoteRole::Extension
                    } else {
                        NoteRole::Melody
                    }
                }
            }
        };

        out.push(AnnotatedNote {
            pitch: n.pitch,
            name: pitch_name(n.pitch),
            track: n.track,
            start_beat: (start_beat * 100.0).round() / 100.0,
            dur_beats: (dur_beats * 100.0).round() / 100.0,
            velocity: n.vel,
            segment: seg_idx,
            role,
            weight: (weight * 100.0).round() / 100.0,
            clustered,
        });
    }
    out
}

/// 单声部素材：全部标成旋律（没有和弦可言，但界面仍要能画出音符）。
fn annotate_single_line(notes: &[MidiNote], ppq: f64) -> Vec<AnnotatedNote> {
    notes
        .iter()
        .map(|n| AnnotatedNote {
            pitch: n.pitch,
            name: pitch_name(n.pitch),
            track: n.track,
            start_beat: (n.start as f64 / ppq * 100.0).round() / 100.0,
            dur_beats: (n.dur as f64 / ppq * 100.0).round() / 100.0,
            velocity: n.vel,
            segment: None,
            role: NoteRole::Melody,
            weight: 0.0,
            clustered: false,
        })
        .collect()
}

/// 为每个段挑出代表音：同一音高类只保留一个（时值最长、其次音高最低）。
///
/// 一个和弦在 MIDI 里常有好几个重复音（八度加倍、多层叠置、supersaw），
/// 卷帘窗上不需要把 8 个音全标出来——每个和弦音给一个最"实在"的就够。
fn fill_voicing(segments: &mut [Segment], notes: &[AnnotatedNote]) {
    for (i, seg) in segments.iter_mut().enumerate() {
        // 音高类 → (最长时值, 该时值对应的音高, 出现次数, 角色, 起始拍)
        let mut groups: HashMap<u8, (f64, u8, usize, NoteRole, f64)> = HashMap::new();
        for n in notes
            .iter()
            .filter(|n| n.segment == Some(i) && n.role.is_chord_tone())
        {
            let pc = n.pitch % 12;
            let e = groups
                .entry(pc)
                .or_insert((0.0, n.pitch, 0, n.role, n.start_beat));
            e.2 += 1;
            let cnt = e.2;
            if n.dur_beats > e.0 || (n.dur_beats == e.0 && n.pitch < e.1) {
                *e = (n.dur_beats, n.pitch, cnt, n.role, n.start_beat);
            }
        }
        let mut v: Vec<VoicingNote> = groups
            .into_iter()
            .map(|(_, (dur, pitch, count, role, start))| VoicingNote {
                pitch,
                name: pitch_name(pitch),
                role,
                start_beat: start,
                dur_beats: dur,
                count,
            })
            .collect();
        v.sort_by_key(|n| n.pitch);
        seg.voicing = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{Division, MidiNote, TempoChange, TimeSig, TrackInfo};

    /// 直接在内存里拼一个单轨文件（不走字节解析，测算法本身）。
    fn smf_with(notes: Vec<MidiNote>) -> Smf {
        let end = notes.iter().map(|n| n.end()).max().unwrap_or(0);
        Smf {
            format: 0,
            division: Division::Ppq(96),
            tracks: vec![TrackInfo {
                index: 0,
                name: Some("测试".to_string()),
                channels: vec![0],
                notes,
                end_tick: end,
            }],
            tempos: vec![TempoChange {
                tick: 0,
                us_per_quarter: 500_000,
            }],
            timesigs: vec![TimeSig {
                tick: 0,
                numerator: 4,
                denominator_pow: 2,
            }],
            key_sig: None,
            warnings: Vec::new(),
        }
    }

    /// 在 `start` 拍处放一个和弦（若干音同起音，持续 `beats` 拍）。
    fn chord_notes(start_beat: f64, beats: f64, pitches: &[u8]) -> Vec<MidiNote> {
        let tick = (start_beat * 96.0) as u64;
        let dur = (beats * 96.0) as u64;
        pitches
            .iter()
            .map(|&p| MidiNote {
                pitch: p,
                start: tick,
                dur,
                vel: 100,
                channel: 0,
                track: 0,
            })
            .collect()
    }

    #[test]
    fn triad_gets_one_voicing_note_per_chord_tone() {
        // C4 E4 G4，一整小节
        let smf = smf_with(chord_notes(0.0, 4.0, &[60, 64, 67]));
        let a = analyze(&smf);
        assert!(!a.single_line);
        assert_eq!(a.segments.len(), 1, "整小节同和弦应只有一段");
        let s = &a.segments[0];
        assert_eq!(s.chord.symbol, "C", "实际 = {}", s.chord.symbol);
        assert_eq!(s.voicing.len(), 3, "C 大三应有 3 个代表音");
        // 三个代表音的角色必须各不相同，并且都在骨架里
        let roles: Vec<NoteRole> = s.voicing.iter().map(|v| v.role).collect();
        assert!(roles.contains(&NoteRole::Root));
        assert!(roles.contains(&NoteRole::Third));
        assert!(roles.contains(&NoteRole::Fifth));
    }

    #[test]
    fn two_chords_in_one_bar_are_split() {
        // 第 1 小节：C（2 拍）+ F（2 拍）
        let mut notes = chord_notes(0.0, 2.0, &[60, 64, 67]);
        notes.extend(chord_notes(2.0, 2.0, &[65, 69, 72]));
        let a = analyze(&smf_with(notes));
        let syms: Vec<&str> = a.segments.iter().map(|s| s.chord.symbol.as_str()).collect();
        assert_eq!(syms, vec!["C", "F"], "实际 = {:?}", syms);
        assert!((a.segments[0].len_beats - 2.0).abs() < 0.01);
        assert!((a.segments[1].len_beats - 2.0).abs() < 0.01);
    }

    #[test]
    fn four_chords_in_one_bar_are_split() {
        // 每拍一个：C / Am / F / G
        let mut notes = chord_notes(0.0, 1.0, &[60, 64, 67]);
        notes.extend(chord_notes(1.0, 1.0, &[57, 60, 64]));
        notes.extend(chord_notes(2.0, 1.0, &[65, 69, 72]));
        notes.extend(chord_notes(3.0, 1.0, &[67, 71, 74]));
        let a = analyze(&smf_with(notes));
        let syms: Vec<&str> = a.segments.iter().map(|s| s.chord.symbol.as_str()).collect();
        assert_eq!(syms, vec!["C", "Am", "F", "G"], "实际 = {:?}", syms);
    }

    #[test]
    fn minor_fifth_is_labelled_as_fifth_not_extension() {
        // 回归：小三和弦的五音曾被写成减三的 6，导致 E 被标成"延伸音"
        let a = analyze(&smf_with(chord_notes(0.0, 4.0, &[57, 60, 64]))); // A3 C4 E4
        let s = &a.segments[0];
        assert_eq!(s.chord.symbol, "Am", "实际 = {}", s.chord.symbol);
        let e_role = s
            .voicing
            .iter()
            .find(|v| v.pitch % 12 == 4)
            .map(|v| v.role);
        assert_eq!(e_role, Some(NoteRole::Fifth), "Am 的 E 应是五音");
    }

    #[test]
    fn single_note_is_reported_as_single_line() {
        let a = analyze(&smf_with(chord_notes(0.0, 2.0, &[60])));
        assert!(a.single_line, "单声部应拒答");
        assert!(a.segments.is_empty());
        assert_eq!(a.notes.len(), 1);
        assert_eq!(a.notes[0].role, NoteRole::Melody);
    }

    #[test]
    fn short_isolated_note_is_marked_melody() {
        // C 大三持续一整小节，叠一个短促的 D5（不属于 C 和弦、且孤立起音）
        let mut notes = chord_notes(0.0, 4.0, &[60, 64, 67]);
        notes.extend(chord_notes(1.5, 0.2, &[74])); // D5 短音
        let a = analyze(&smf_with(notes));
        let d = a
            .notes
            .iter()
            .find(|n| n.pitch == 74)
            .expect("应能找到 D5");
        assert_eq!(d.role, NoteRole::Melody, "短促孤立音应判为旋律");
        assert!(!d.clustered, "它没有同起点伙伴");
    }

    #[test]
    fn drum_channel_is_ignored() {
        // 通道 9（界面上的通道 10）是鼓，不该参与和声
        let mut notes = chord_notes(0.0, 4.0, &[60, 64, 67]);
        notes.extend(chord_notes(0.0, 0.5, &[36]).into_iter().map(|mut n| {
            n.channel = 9; // 底鼓
            n
        }));
        let a = analyze(&smf_with(notes));
        assert_eq!(a.segments.len(), 1);
        assert_eq!(a.segments[0].chord.symbol, "C");
        // 鼓音不应出现在音符标注里（它不在和声层）
        assert!(a.notes.iter().all(|n| n.pitch != 36));
    }

    #[test]
    fn whole_note_chord_is_not_fragmented() {
        // 长和弦不该被切碎
        let a = analyze(&smf_with(chord_notes(0.0, 8.0, &[60, 64, 67])));
        assert_eq!(a.segments.len(), 1, "8 拍同一和弦应合成一段");
        assert!((a.segments[0].len_beats - 8.0).abs() < 0.01);
    }

    /// 回归：下面是长和弦、上面是短旋律的典型谱型，旋律不能被吞进和弦音。
    ///
    /// 实测样本里和弦带延伸音（`E4`）会把上下两层的音隙压到 8 个半音，
    /// 只看音隙会漏判，必须结合时值对比。
    #[test]
    fn upper_voice_is_separated_as_melody() {
        // 下面：Dm7 长音（含延伸音 E4）
        let mut notes = chord_notes(0.0, 4.0, &[50, 53, 57, 60, 64]);
        // 上面：三个八分音符旋律
        notes.extend(chord_notes(0.5, 0.5, &[72])); // C5
        notes.extend(chord_notes(1.5, 0.5, &[79])); // G5
        notes.extend(chord_notes(2.5, 0.5, &[86])); // D6
        let a = analyze(&smf_with(notes));

        let melody: Vec<u8> = a
            .notes
            .iter()
            .filter(|n| n.role == NoteRole::Melody)
            .map(|n| n.pitch)
            .collect();
        assert_eq!(melody.len(), 3, "三个高音旋律都该判为旋律，实际 = {:?}", melody);
        // 低音区的长音仍应是和弦音
        let low_all_chord = a
            .notes
            .iter()
            .filter(|n| n.pitch < 70)
            .all(|n| n.role.is_chord_tone());
        assert!(low_all_chord, "低音区的长音应全部是和弦音");
    }

    /// 回归：关系大小调的和弦集合完全一样，必须靠"结束在主和弦"来区分。
    ///
    /// `Dm7 → G7 → Bm7b5 → Am7` 在 C 大调里是 ii-V-vii-vi，
    /// 在 A 小调里是 iv-VII-iiø-i——后者结束在主和弦，才是正确的读法。
    #[test]
    fn relative_minor_is_chosen_by_ending_chord() {
        let mut notes = Vec::new();
        notes.extend(chord_notes(0.0, 4.0, &[50, 53, 57, 60])); // Dm7
        notes.extend(chord_notes(4.0, 4.0, &[55, 59, 62, 65])); // G7
        notes.extend(chord_notes(8.0, 4.0, &[59, 62, 65, 69])); // Bm7b5
        notes.extend(chord_notes(12.0, 4.0, &[57, 60, 64, 67])); // Am7
        let a = analyze(&smf_with(notes));
        assert!(
            a.key_name.contains("A 小调"),
            "应判 A 小调（结束在 Am7），实际 = {}",
            a.key_name
        );
        assert!(
            a.key_confidence >= 60,
            "有了主音证据后置信度不该偏低，实际 = {}",
            a.key_confidence
        );
    }
}
