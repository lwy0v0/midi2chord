//! Loop 提取探针：批量跑语料，打印调性、逐段和弦、代表音、音符角色。
//!
//! 用途：改完算法先跑这个，一眼看出哪些用例转正、哪些翻车。
//!
//! 跑法（在 `src-tauri` 目录下）：
//!   cargo run --example probe                    # 跑全部语料
//!   cargo run --example probe -- <某个.mid 路径>   # 只跑一个
//!
//! 语料默认在仓库根目录的 `testdata/midi`。

use chords_lib::extract::{self, NoteRole};
use chords_lib::midi::Smf;
use chords_lib::note::Note;

use std::path::{Path, PathBuf};

fn collect_midi(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_midi(&p, out);
            } else if matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("mid") | Some("midi")
            ) {
                out.push(p);
            }
        }
    }
}

fn main() {
    let arg = std::env::args().nth(1);
    let mut files: Vec<PathBuf> = Vec::new();
    match &arg {
        Some(a) => files.push(PathBuf::from(a)),
        None => collect_midi(Path::new("../testdata/midi"), &mut files),
    }
    files.sort();

    if files.is_empty() {
        println!("没找到 MIDI 文件（在 ../testdata/midi 下）");
        return;
    }

    for f in &files {
        let rel = f
            .strip_prefix("../testdata/midi")
            .unwrap_or(f)
            .to_string_lossy()
            .to_string();
        println!("\n══════════ {}", rel);
        let bytes = match std::fs::read(f) {
            Ok(b) => b,
            Err(e) => {
                println!("  读文件失败：{}", e);
                continue;
            }
        };
        let smf = match Smf::parse(&bytes) {
            Ok(s) => s,
            Err(e) => {
                println!("  解析失败：{}", e);
                continue;
            }
        };

        let a = extract::analyze(&smf);

        println!(
            "  {} · {}/4 · {} 音符 · {} 轨 · BPM {}{}",
            if a.bpm_explicit {
                "有 tempo"
            } else {
                "无 tempo（按默认 120）"
            },
            a.beats_per_bar as u32,
            smf.note_count(),
            smf.tracks.len(),
            a.bpm.round() as u32,
            if a.bpm_explicit { "" } else { " ⚠" }
        );
        if a.pickup_beats > 0.0 {
            println!("  弱起：第一个音在第 {:.2} 拍", a.pickup_beats);
        }

        for l in &a.layers {
            println!("  层：{}", l);
        }

        if a.single_line {
            println!(
                "  调性：{}（{}%）  ← 单声部，只给调性",
                a.key_name, a.key_confidence
            );
            let c: Vec<String> = a
                .key_candidates
                .iter()
                .map(|(k, v)| format!("{} {}", k, v))
                .collect();
            if !c.is_empty() {
                println!("  候选：{}", c.join(" | "));
            }
            for w in &a.warnings {
                println!("  ! {}", w);
            }
            continue;
        }

        println!(
            "  调性：{}（{}%）   音级分布判据：{}",
            a.key_name, a.key_confidence, a.key_from_pitches
        );
        let c: Vec<String> = a
            .key_candidates
            .iter()
            .map(|(k, v)| format!("{} {}", k, v))
            .collect();
        println!("  候选：{}", c.join(" | "));

        println!("  进行：{}", a.progression.join(" → "));
        if !a.romans.is_empty() {
            println!("  罗马：{}", a.romans.join(" "));
        }
        // 候选调：按综合分排序，标出"开头是主和弦"和"结尾是主和弦"
        if !a.key_options.is_empty() {
            println!("  候选调（{} 个，* = 默认）", a.key_options.len());
            for o in &a.key_options {
                let mark = if o.selected { "*" } else { " " };
                let flags = format!(
                    "{}{}",
                    if o.starts_on_tonic { "开头主" } else { "" },
                    if o.ends_on_tonic { " 结尾主" } else { "" },
                );
                println!(
                    "   {}{:<22} 音级{:>3} 综合{:>6.2} 主和弦占{:>3}% 低音占{:>3}% {:<10} {}",
                    mark,
                    o.name,
                    o.pitch_score,
                    o.rank_score,
                    o.tonic_share,
                    o.bass_on_tonic,
                    flags,
                    o.progression.join(" → ")
                );
            }
        }
        println!("  分段（{} 段）：", a.segments.len());
        for (i, s) in a.segments.iter().enumerate() {
            let roman = a.romans.get(i).cloned().unwrap_or_default();
            println!(
                "    {:.0}.{:.0}  {:<10} {:<8} {:.2} 拍  根={} 低音={}{}  音:[{}]{}",
                s.bar,
                s.beat_in_bar,
                s.chord.symbol,
                roman,
                s.len_beats,
                Note::new(s.chord.root).name(),
                Note::new(s.chord.bass).name(),
                if s.chord.slash { " (斜杠)" } else { "" },
                s.chord_tones.join(" "),
                if s.excluded.is_empty() {
                    String::new()
                } else {
                    format!("  外音:[{}]", s.excluded.join(" "))
                }
            );
            // 代表音：每个和弦音只取一个（同音高类里时值最长的那个）
            if !s.voicing.is_empty() {
                let v: Vec<String> = s
                    .voicing
                    .iter()
                    .map(|n| {
                        format!(
                            "{}-{}({:.2}){}",
                            n.name,
                            n.role.label(),
                            n.dur_beats,
                            if n.count > 1 {
                                format!("×{}", n.count)
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect();
                println!("        代表音：{}", v.join("  "));
            }
            // 音符级标注：每个音后面标出角色和时值，便于肉眼核对
            let mut seg_notes: Vec<String> = Vec::new();
            for n in a.notes.iter().filter(|n| n.segment == Some(i)) {
                seg_notes.push(format!("{}-{}({:.2})", n.name, n.role.label(), n.dur_beats));
            }
            if !seg_notes.is_empty() {
                let shown = seg_notes
                    .iter()
                    .take(24)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("  ");
                let more = if seg_notes.len() > 24 {
                    format!("  …共 {} 个", seg_notes.len())
                } else {
                    String::new()
                };
                println!("        音符：{}{}", shown, more);
            }
        }

        let total = a.notes.len();
        let chord_tones = a.notes.iter().filter(|n| n.role.is_chord_tone()).count();
        let melody = a.notes.iter().filter(|n| n.role == NoteRole::Melody).count();
        let outside = a.notes.iter().filter(|n| n.role == NoteRole::Outside).count();
        println!(
            "  音符统计：共 {} · 和弦音 {} · 旋律 {} · 段外 {}",
            total, chord_tones, melody, outside
        );
        for w in &a.warnings {
            println!("  ! {}", w);
        }
    }
}
