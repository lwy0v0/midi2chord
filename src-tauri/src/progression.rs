//! 和弦在调内的角色：罗马数字、功能分组、终止式、经典走向模板，以及调外和弦的解释。
//!
//! 职责边界：输入「一个调 + 一串和弦」，输出「每个和弦在这个调里是什么、为什么」。

use crate::chord::{Chord, Kind, Quality};
use crate::key::{Key, Mode};
use crate::note::Note;

/// 功能分组（T 主 / S 下属 / D 属）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Function {
    Tonic,
    Subdominant,
    Dominant,
    None,
}

impl Function {
    pub fn label(self) -> &'static str {
        match self {
            Function::Tonic => "T",
            Function::Subdominant => "S",
            Function::Dominant => "D",
            Function::None => "—",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Function::Tonic => "主功能",
            Function::Subdominant => "下属功能",
            Function::Dominant => "属功能",
            Function::None => "无明确功能",
        }
    }

    /// 由音级（1..7）推断功能；0 表示调外。
    pub fn from_degree(degree: u8) -> Function {
        match degree {
            1 | 3 | 6 => Function::Tonic,
            2 | 4 => Function::Subdominant,
            5 | 7 => Function::Dominant,
            _ => Function::None,
        }
    }
}

/// 和弦与调的关系。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoleCategory {
    /// 完全在自然音阶内。
    Diatonic,
    /// 由和声/旋律小调的变化音解释。
    HarmonicMinor,
    /// 副属和弦（V/x）。
    SecondaryDominant,
    /// 同主音大小调借用（含皮卡迪三度）。
    ModalMixture,
    /// 那不勒斯和弦（bII）。
    Neapolitan,
    /// 调外，未找到常规解释。
    Chromatic,
}

impl RoleCategory {
    pub fn id(self) -> &'static str {
        match self {
            RoleCategory::Diatonic => "diatonic",
            RoleCategory::HarmonicMinor => "harmonicMinor",
            RoleCategory::SecondaryDominant => "secondaryDominant",
            RoleCategory::ModalMixture => "modalMixture",
            RoleCategory::Neapolitan => "neapolitan",
            RoleCategory::Chromatic => "chromatic",
        }
    }

    /// 分类对应的展示标签（「调内」之外一律附带「调外」）。
    pub fn tags(self) -> Vec<String> {
        let primary = match self {
            RoleCategory::Diatonic => "调内",
            RoleCategory::HarmonicMinor => "和声小调",
            RoleCategory::SecondaryDominant => "副属和弦",
            RoleCategory::ModalMixture => "同主音借用",
            RoleCategory::Neapolitan => "那不勒斯",
            RoleCategory::Chromatic => "调外",
        };
        if self == RoleCategory::Diatonic {
            vec![primary.to_string()]
        } else {
            vec![primary.to_string(), "调外".to_string()]
        }
    }

    /// 分类在「同一和弦多解」时的择优优先级。
    pub fn fit_rank(self) -> i32 {
        match self {
            RoleCategory::Diatonic => 100,
            RoleCategory::HarmonicMinor => 80,
            RoleCategory::SecondaryDominant => 55,
            RoleCategory::ModalMixture => 45,
            RoleCategory::Neapolitan => 40,
            RoleCategory::Chromatic => 0,
        }
    }
}

/// 一个和弦在某个调中的角色。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChordRole {
    /// 音级 1..7；0 表示根音不在音阶内。
    pub degree: u8,
    /// 罗马数字，如 `V7`、`bVII`、`V7/vi`。
    pub roman: String,
    pub function: Function,
    pub category: RoleCategory,
    pub explanation: String,
    pub tags: Vec<String>,
    /// 拼写方向建议：true 表示优先用降号。
    pub prefer_flats: bool,
}

/// 终止式。
#[derive(Clone, PartialEq, Debug)]
pub struct Cadence {
    /// 结束和弦的下标。
    pub at: usize,
    /// 机器可读类型。
    pub kind: String,
    /// 如 `V7 → I`。
    pub formula: String,
    pub description: String,
    /// 打分加成。
    pub bonus: f64,
}

/// 走向模板命中结果。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TemplateMatch {
    pub name: String,
    /// 起始下标。
    pub start: usize,
    /// 结束下标（含）。
    pub end: usize,
    /// 如 `I–V–vi–IV`。
    pub formula: String,
    pub note: String,
}

/// 走向模板。
pub struct ProgressionTemplate {
    pub name: &'static str,
    /// 音级序列（1..7）。
    pub degrees: &'static [u8],
    /// 限定调式。
    pub mode: Option<Mode>,
    pub note: &'static str,
}

/// 内置的常见走向。
pub const PROGRESSION_TEMPLATES: &[ProgressionTemplate] = &[
    ProgressionTemplate {
        name: "流行四和弦（I–V–vi–IV）",
        degrees: &[1, 5, 6, 4],
        mode: Some(Mode::Major),
        note: "华语流行与欧美流行最常用的进行",
    },
    ProgressionTemplate {
        name: "50 年代进行（I–vi–IV–V）",
        degrees: &[1, 6, 4, 5],
        mode: Some(Mode::Major),
        note: "doo-wop 与老摇滚的标准进行",
    },
    ProgressionTemplate {
        name: "卡农进行（I–V–vi–iii–IV–I–IV–V）",
        degrees: &[1, 5, 6, 3, 4, 1, 4, 5],
        mode: Some(Mode::Major),
        note: "帕赫贝尔卡农，抒情歌曲常用",
    },
    ProgressionTemplate {
        name: "爵士 ii–V–I",
        degrees: &[2, 5, 1],
        mode: None,
        note: "爵士和声最核心的终止进行",
    },
    ProgressionTemplate {
        name: "Rhythm Changes A 段（I–vi–ii–V）",
        degrees: &[1, 6, 2, 5],
        mode: Some(Mode::Major),
        note: "爵士标准曲常用的循环进行",
    },
    ProgressionTemplate {
        name: "正格进行（I–IV–V–I）",
        degrees: &[1, 4, 5, 1],
        mode: None,
        note: "功能和声的经典三和弦进行",
    },
    ProgressionTemplate {
        name: "安达卢西亚进行（i–VII–VI–V）",
        degrees: &[1, 7, 6, 5],
        mode: Some(Mode::Minor),
        note: "弗拉门戈与流行小调常用的下行进行",
    },
    ProgressionTemplate {
        name: "12 小节布鲁斯",
        degrees: &[1, 1, 1, 1, 4, 4, 1, 1, 5, 4, 1, 5],
        mode: None,
        note: "布鲁斯的标准 12 小节框架",
    },
];

const MIN_TEMPLATE_WINDOW: usize = 4;

/// 判定一个和弦在调内的角色。
pub fn analyze_role(key: &Key, chord: &Chord) -> ChordRole {
    let root = chord.root;
    let mask = chord.mask();
    let degree = key.degree_number(root);

    // 1) 完全在自然音阶内
    if degree > 0 && key.mask_in_scale(mask) {
        return build_role(
            key,
            chord,
            degree,
            0,
            RoleCategory::Diatonic,
            Function::from_degree(degree),
            format!(
                "完全属于 {} 的自然音阶（第 {} 级，{}）",
                key.display_name(),
                degree,
                Function::from_degree(degree).describe()
            ),
        );
    }

    // 2) 小调：和声/旋律小调的变化音（升六级、升七级）
    if !key.is_major() && key.mask_in_extended(mask) {
        let raised_slot = raised_minor_slot(key, root);
        if degree > 0 || raised_slot.is_some() {
            let (slot_degree, accidental) = match raised_slot {
                Some(slot) => slot,
                None => nearest_slot(key, root),
            };
            return build_role(
                key,
                chord,
                slot_degree,
                accidental,
                RoleCategory::HarmonicMinor,
                Function::from_degree(slot_degree),
                format!(
                    "含和声/旋律小调的升六级或升七级音，是 {} 的{}",
                    key.display_name(),
                    Function::from_degree(slot_degree).describe()
                ),
            );
        }
    }

    // 3) 小调里的同主音大三和弦（皮卡迪三度）
    if !key.is_major() && root == key.tonic() && chord.quality == Quality::Major {
        return build_role(
            key,
            chord,
            1,
            0,
            RoleCategory::ModalMixture,
            Function::Tonic,
            format!(
                "{} 的皮卡迪三度：用小调主音配大三和弦，常见于乐句结尾",
                key.display_name()
            ),
        );
    }

    // 4) 副属和弦 V/x
    if let Some((target_degree, target_roman)) = secondary_dominant_target(key, chord) {
        let seventh = if chord.quality == Quality::Dominant {
            "7"
        } else {
            ""
        };
        let mut role = build_role(
            key,
            chord,
            target_degree,
            0,
            RoleCategory::SecondaryDominant,
            Function::Dominant,
            format!(
                "{} 是第 {} 级（{}）的属和弦，写作 V{}/{}，用来把和声临时推向它",
                chord.name(),
                target_degree,
                target_roman,
                seventh,
                target_roman
            ),
        );
        // 副属和弦的罗马数字用 V/x 记法，而不是它自己的临时级数
        role.roman = format!("V{}/{}", seventh, target_roman);
        return role;
    }

    // 5) 那不勒斯和弦 bII
    if root == key.tonic() + 1 && chord.quality == Quality::Major {
        return build_role(
            key,
            chord,
            2,
            -1,
            RoleCategory::Neapolitan,
            Function::Subdominant,
            "降二级大三和弦（那不勒斯和弦），常用于代替 iv 并推向属和弦".to_string(),
        );
    }

    // 6) 同主音大小调借用
    if key.mask_in_parallel(mask) {
        if let Some(role) = mixture_role(key, chord) {
            return role;
        }
    }

    // 7) 兜底：调外
    let (slot_degree, accidental) = nearest_slot(key, root);
    let outside: Vec<String> = crate::note::pitches_of(mask & !key.natural_mask())
        .iter()
        .map(|note| note.name())
        .collect();
    build_role(
        key,
        chord,
        slot_degree,
        accidental,
        RoleCategory::Chromatic,
        Function::None,
        format!(
            "在 {} 中未找到常规解释（含调外音 {}）",
            key.display_name(),
            if outside.is_empty() {
                "—".to_string()
            } else {
                outside.join(" ")
            }
        ),
    )
}

/// 由同主音另一调式借用的和弦（bIII / bVI / bVII / iv / v 等）。
fn mixture_role(key: &Key, chord: &Chord) -> Option<ChordRole> {
    let parallel = key.parallel();
    let root = chord.root;
    let degree = parallel.degree_number(root);
    if degree == 0 {
        return None;
    }
    let parallel_note = parallel.degree_note(degree)?;
    let own_note = key.degree_note(degree)?;
    let accidental = signed_diff(own_note, parallel_note);
    let borrowed_from = match key.mode() {
        Mode::Major => "同主音小调",
        Mode::Minor => "同主音大调",
    };
    Some(build_role(
        key,
        chord,
        degree,
        accidental,
        RoleCategory::ModalMixture,
        Function::from_degree(degree),
        format!(
            "从{}借来的第 {} 级和弦，为 {} 增加色彩变化",
            borrowed_from,
            degree,
            key.display_name()
        ),
    ))
}

/// 找副属和弦的目标级数（属和弦根音比目标高五度）。
fn secondary_dominant_target(key: &Key, chord: &Chord) -> Option<(u8, String)> {
    if !matches!(chord.quality, Quality::Major | Quality::Dominant) {
        return None;
    }
    let target_root_pitch = (chord.root + 5).pitch;
    for degree in 1..=7u8 {
        let target = key.degree_note(degree)?;
        if target.pitch != target_root_pitch {
            continue;
        }
        // 目标是主音时说明这就是本调的属和弦，不算副属
        if degree == 1 {
            return None;
        }
        let target_quality = diatonic_triad_quality(key, degree)?;
        let roman = roman_numeral(degree, 0, target_quality, "", Kind::Triad);
        return Some((degree, roman));
    }
    None
}

/// 自然音阶上第 `degree` 级三和弦的性质。
fn diatonic_triad_quality(key: &Key, degree: u8) -> Option<Quality> {
    let root = key.degree_note(degree)?;
    let third = key.degree_note(step_up(degree, 2))?;
    let fifth = key.degree_note(step_up(degree, 4))?;
    Some(match (root.interval_to(third), root.interval_to(fifth)) {
        (4, 7) => Quality::Major,
        (3, 7) => Quality::Minor,
        (3, 6) => Quality::Diminished,
        (4, 8) => Quality::Augmented,
        _ => Quality::Altered,
    })
}

/// 音级在自然音阶上向上走 `steps` 步（1..7 循环）。
fn step_up(degree: u8, steps: u8) -> u8 {
    ((degree - 1 + steps) % 7) + 1
}

/// 组装调内角色：标签与拼写方向都由分类推导，避免调用点参数错位。
fn build_role(
    key: &Key,
    chord: &Chord,
    degree: u8,
    accidental: i8,
    category: RoleCategory,
    function: Function,
    explanation: String,
) -> ChordRole {
    // 拼写方向：借用与那不勒斯用降号，副属与和声小调用升号，其余跟随调号。
    let prefer_flats = match category {
        RoleCategory::Diatonic | RoleCategory::Chromatic => key.prefer_flats(),
        RoleCategory::ModalMixture => key.is_major(),
        RoleCategory::Neapolitan => true,
        RoleCategory::HarmonicMinor | RoleCategory::SecondaryDominant => false,
    };
    let roman = roman_numeral(degree, accidental, chord.quality, &chord.suffix, chord.kind);
    ChordRole {
        degree,
        roman,
        function,
        category,
        explanation,
        tags: category.tags(),
        prefer_flats,
    }
}

/// 罗马数字（含升降号前缀与和弦后缀）。
pub fn roman_numeral(
    degree: u8,
    accidental: i8,
    quality: Quality,
    suffix: &str,
    kind: Kind,
) -> String {
    let clamped = if degree == 0 { 1 } else { degree.min(7) };
    let numeral = ["I", "II", "III", "IV", "V", "VI", "VII"][(clamped - 1) as usize];
    let numeral = if quality.is_minorish() {
        numeral.to_lowercase()
    } else {
        numeral.to_string()
    };
    let prefix = match accidental {
        1 => "#",
        2 => "##",
        -1 => "b",
        -2 => "bb",
        _ => "",
    };
    // 只有两个音时用音程名标注，避免与和弦后缀（如 m7）混淆。
    let tail = if kind == Kind::Interval {
        format!("({})", suffix)
    } else {
        roman_suffix(suffix)
    };
    format!("{}{}{}", prefix, numeral, tail)
}

/// 和弦后缀 → 罗马数字后缀（只处理和弦，不处理音程）。
pub fn roman_suffix(suffix: &str) -> String {
    let (base, paren) = match suffix.find('(') {
        Some(index) => (&suffix[..index], &suffix[index..]),
        None => (suffix, ""),
    };
    let mapped = match base {
        "" | "m" => "",
        "dim" => "°",
        "aug" => "+",
        "7" | "m7" => "7",
        "maj7" => "maj7",
        "mM7" => "(maj7)",
        "dim7" => "°7",
        "m7b5" => "ø7",
        "6" | "m6" => "6",
        "add9" | "madd9" => "add9",
        "add11" | "madd11" => "add11",
        "9" | "m9" => "9",
        "maj9" => "maj9",
        "7b9" | "7#9" | "7#11" | "7add13" | "7add11" => base,
        "maj7#11" => "maj7#11",
        "7b5" | "7#5" | "maj7b5" | "maj7#5" | "dimM7" => base,
        "sus2" | "sus4" | "7sus2" | "7sus4" | "maj7sus2" | "maj7sus4" | "9sus4" => base,
        "6/9" => "6/9",
        "11" | "m11" => "11",
        "13" | "m13" => "13",
        "maj13" => "maj13",
        other => other,
    };
    format!("{}{}", mapped, paren)
}

/// 音高差（-6..=6）。
fn signed_diff(from: Note, to: Note) -> i8 {
    let diff = (to.pitch as i8 - from.pitch as i8).rem_euclid(12);
    if diff > 6 {
        diff - 12
    } else {
        diff
    }
}

/// 小调里正好的升六级 / 升七级。
fn raised_minor_slot(key: &Key, note: Note) -> Option<(u8, i8)> {
    let tonic = key.tonic();
    if note.pitch == (tonic + 11).pitch {
        Some((7, 0))
    } else if note.pitch == (tonic + 9).pitch {
        Some((6, 0))
    } else {
        None
    }
}

/// 在调的音阶上找最近的级数与升降号。
fn nearest_slot(key: &Key, note: Note) -> (u8, i8) {
    let mut best: (u8, i8, i8) = (1, 0, i8::MAX);
    for degree in 1..=7u8 {
        if let Some(scale_note) = key.degree_note(degree) {
            let accidental = signed_diff(scale_note, note);
            let cost = accidental.abs();
            if cost < best.2 {
                best = (degree, accidental, cost);
            }
        }
    }
    let (degree, accidental, _) = best;
    // 小调的升六级 / 升七级是调内写法，不加升号
    if !key.is_major() && accidental == 1 && (degree == 6 || degree == 7) {
        return (degree, 0);
    }
    (degree, accidental)
}

/// 该和弦是否为调的主和弦。
pub fn is_tonic_chord(key: &Key, chord: &Chord) -> bool {
    chord.root == key.tonic() && (chord.mask() & key.tonic_triad_mask()) == key.tonic_triad_mask()
}

/// 该和弦是否具有属功能（V 或 V7）。
pub fn is_dominant_chord(key: &Key, chord: &Chord) -> bool {
    chord.root.pitch == (key.tonic() + 7).pitch
        && matches!(chord.quality, Quality::Major | Quality::Dominant)
}

/// 终止式识别。
pub fn detect_cadences(key: &Key, chords: &[Chord], roles: &[ChordRole]) -> Vec<Cadence> {
    let mut cadences: Vec<Cadence> = Vec::new();
    if chords.len() < 2 {
        return cadences;
    }
    for index in 0..chords.len() - 1 {
        let from = &chords[index];
        let to = &chords[index + 1];
        let from_roman = &roles[index].roman;
        let to_roman = &roles[index + 1].roman;
        if is_dominant_chord(key, from) && is_tonic_chord(key, to) {
            cadences.push(Cadence {
                at: index + 1,
                kind: "authentic".to_string(),
                formula: format!("{} → {}", from_roman, to_roman),
                description: "完全终止（属 → 主），最强的收束感".to_string(),
                bonus: 1.0,
            });
            continue;
        }
        if is_dominant_chord(key, from) && roles[index + 1].degree == 6 {
            cadences.push(Cadence {
                at: index + 1,
                kind: "deceptive".to_string(),
                formula: format!("{} → {}", from_roman, to_roman),
                description: "阻碍终止（属 → 六级），推迟解决".to_string(),
                bonus: 0.3,
            });
            continue;
        }
        if roles[index].degree == 4 && is_tonic_chord(key, to) {
            cadences.push(Cadence {
                at: index + 1,
                kind: "plagal".to_string(),
                formula: format!("{} → {}", from_roman, to_roman),
                description: "变格终止（下属 → 主），常用于结尾阿门式收束".to_string(),
                bonus: 0.5,
            });
            continue;
        }
        if roles[index].degree == 2
            && is_dominant_chord(key, to)
            && index + 2 < chords.len()
            && is_tonic_chord(key, &chords[index + 2])
        {
            cadences.push(Cadence {
                at: index + 2,
                kind: "ii-V-I".to_string(),
                formula: format!("{} → {} → {}", from_roman, to_roman, roles[index + 2].roman),
                description: "ii–V–I，爵士和声最常见的终止".to_string(),
                bonus: 0.8,
            });
        }
    }
    if let Some(last) = chords.last() {
        if is_dominant_chord(key, last) {
            cadences.push(Cadence {
                at: chords.len() - 1,
                kind: "half".to_string(),
                formula: roles[roles.len() - 1].roman.clone(),
                description: "半终止（停在属和弦），造成未完成的期待".to_string(),
                bonus: 0.2,
            });
        }
    }
    cadences
}

/// 走向模板匹配。
pub fn match_templates(roles: &[ChordRole], key: &Key) -> Vec<TemplateMatch> {
    let degrees: Vec<u8> = roles.iter().map(|role| role.degree).collect();
    let mut matches: Vec<TemplateMatch> = Vec::new();
    for template in PROGRESSION_TEMPLATES {
        if let Some(mode) = template.mode {
            if mode != key.mode() {
                continue;
            }
        }
        let length = template.degrees.len();
        for start in 0..degrees.len() {
            let available = degrees.len() - start;
            let take = length.min(available);
            if take < MIN_TEMPLATE_WINDOW {
                continue;
            }
            let window = &degrees[start..start + take];
            if window.contains(&0) {
                continue;
            }
            if window != &template.degrees[..take] {
                continue;
            }
            matches.push(TemplateMatch {
                name: template.name.to_string(),
                start,
                end: start + take - 1,
                formula: window
                    .iter()
                    .map(|degree| format!("{}", degree))
                    .collect::<Vec<String>>()
                    .join("–"),
                note: template.note.to_string(),
            });
            break;
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::interpret;
    use crate::note::Note;

    fn key(name: &str, mode: Mode) -> Key {
        Key::from_name(name, mode)
    }

    fn chord(names: &[&str]) -> Chord {
        let notes: Vec<Note> = names
            .iter()
            .map(|name| Note::parse(name).unwrap())
            .collect();
        interpret(&notes)
    }

    #[test]
    fn diatonic_roles_and_functions() {
        let c = key("C", Mode::Major);
        let role = analyze_role(&c, &chord(&["C", "E", "G"]));
        assert_eq!(role.roman, "I");
        assert_eq!(role.function, Function::Tonic);
        assert_eq!(role.category, RoleCategory::Diatonic);

        let role = analyze_role(&c, &chord(&["G", "B", "D", "F"]));
        assert_eq!(role.roman, "V7");
        assert_eq!(role.function, Function::Dominant);

        let role = analyze_role(&c, &chord(&["A", "C", "E"]));
        assert_eq!(role.roman, "vi");
        assert_eq!(role.function, Function::Tonic);
    }

    #[test]
    fn harmonic_minor_accepts_dominant_five() {
        let a = key("A", Mode::Minor);
        let role = analyze_role(&a, &chord(&["E", "G#", "B"]));
        assert_eq!(role.roman, "V");
        assert_eq!(role.category, RoleCategory::HarmonicMinor);
        // vii° 也一样（G#dim）
        let role = analyze_role(&a, &chord(&["G#", "B", "D"]));
        assert_eq!(role.roman, "vii°");
    }

    #[test]
    fn secondary_dominants_are_explained() {
        let c = key("C", Mode::Major);
        let role = analyze_role(&c, &chord(&["E", "G#", "B"]));
        assert_eq!(role.category, RoleCategory::SecondaryDominant);
        assert!(role.roman.contains("/vi"), "实际 = {}", role.roman);

        let role = analyze_role(&c, &chord(&["D", "F#", "A"]));
        assert!(role.roman.contains("/V"), "实际 = {}", role.roman);
    }

    #[test]
    fn modal_mixture_and_neapolitan() {
        let c = key("C", Mode::Major);
        let role = analyze_role(&c, &chord(&["Bb", "D", "F"]));
        assert_eq!(role.category, RoleCategory::ModalMixture);
        assert_eq!(role.roman, "bVII");
        assert!(role.prefer_flats);

        let role = analyze_role(&c, &chord(&["Db", "F", "Ab"]));
        assert_eq!(role.category, RoleCategory::Neapolitan);
        assert_eq!(role.roman, "bII");
    }

    #[test]
    fn cadences_and_templates() {
        let c = key("C", Mode::Major);
        let chords: Vec<Chord> = [
            ["C", "E", "G"],
            ["A", "C", "E"],
            ["D", "F", "A"],
            ["G", "B", "D"],
        ]
        .iter()
        .map(|names| chord(names))
        .collect();
        let roles: Vec<ChordRole> = chords.iter().map(|chord| analyze_role(&c, chord)).collect();
        assert_eq!(
            roles
                .iter()
                .map(|role| role.roman.as_str())
                .collect::<Vec<_>>(),
            ["I", "vi", "ii", "V"]
        );
        let cadences = detect_cadences(&c, &chords, &roles);
        assert!(cadences.iter().any(|cadence| cadence.kind == "half"));
        let templates = match_templates(&roles, &c);
        assert!(
            templates
                .iter()
                .any(|hit| hit.name.contains("Rhythm Changes")),
            "实际 = {:?}",
            templates
        );
    }

    #[test]
    fn authentic_cadence_is_detected() {
        let c = key("C", Mode::Major);
        let rows: [&[&str]; 3] = [&["D", "F", "A"], &["G", "B", "D", "F"], &["C", "E", "G"]];
        let chords: Vec<Chord> = rows.iter().map(|names| chord(names)).collect();
        let roles: Vec<ChordRole> = chords.iter().map(|chord| analyze_role(&c, chord)).collect();
        let cadences = detect_cadences(&c, &chords, &roles);
        assert!(cadences.iter().any(|cadence| cadence.kind == "authentic"));
        assert!(cadences.iter().any(|cadence| cadence.kind == "ii-V-I"));
    }
}
