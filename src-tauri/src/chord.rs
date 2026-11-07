//! 和弦识别：模板音程匹配、省略音、转位、以及多解候选。
//!
//! 职责边界：本模块只回答「这一组音能构成哪些和弦」，不判断调性、不解释走向。
//! 识别结果**永远是多个候选**（按可信度排序），由上层结合调性语境择优。

use crate::key::Key;
use crate::note::{dedupe, mask_of, pitches_of, Note, PitchMask};

/// 和弦色彩。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Dominant,
    Suspended,
    Altered,
    Interval,
    Unknown,
}

impl Quality {
    pub fn id(self) -> &'static str {
        match self {
            Quality::Major => "major",
            Quality::Minor => "minor",
            Quality::Diminished => "diminished",
            Quality::Augmented => "augmented",
            Quality::Dominant => "dominant",
            Quality::Suspended => "suspended",
            Quality::Altered => "altered",
            Quality::Interval => "interval",
            Quality::Unknown => "unknown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Major => "大",
            Quality::Minor => "小",
            Quality::Diminished => "减",
            Quality::Augmented => "增",
            Quality::Dominant => "属",
            Quality::Suspended => "挂留",
            Quality::Altered => "变化",
            Quality::Interval => "音程",
            Quality::Unknown => "未知",
        }
    }

    /// 是否属于「小三和弦色彩」（决定罗马数字用小写）。
    pub fn is_minorish(self) -> bool {
        matches!(self, Quality::Minor | Quality::Diminished)
    }
}

/// 和弦规模。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Interval,
    Triad,
    Seventh,
    Extended,
    Unknown,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::Interval => "interval",
            Kind::Triad => "triad",
            Kind::Seventh => "seventh",
            Kind::Extended => "extended",
            Kind::Unknown => "unknown",
        }
    }
}

/// 和弦模板。
#[derive(Clone, Copy, Debug)]
pub struct Template {
    /// 和弦后缀，如 `m7`、`(no5)` 由识别阶段追加。
    pub suffix: &'static str,
    /// 相对根音的半音音程（音高类，0..=11）。
    pub intervals: &'static [i8],
    pub quality: Quality,
    pub kind: Kind,
    /// 中文结构名。
    pub label: &'static str,
}

const fn template(
    suffix: &'static str,
    intervals: &'static [i8],
    quality: Quality,
    kind: Kind,
    label: &'static str,
) -> Template {
    Template {
        suffix,
        intervals,
        quality,
        kind,
        label,
    }
}

/// 全部模板。顺序即同分时的优先级：越靠前越常用。
pub const TEMPLATES: &[Template] = &[
    // ---------- 音程（2 个音） ----------
    template(
        "5",
        &[0, 7],
        Quality::Interval,
        Kind::Interval,
        "纯五度（强力和弦）",
    ),
    template("4", &[0, 5], Quality::Interval, Kind::Interval, "纯四度"),
    template("M3", &[0, 4], Quality::Interval, Kind::Interval, "大三度"),
    template("m3", &[0, 3], Quality::Interval, Kind::Interval, "小三度"),
    template("M6", &[0, 9], Quality::Interval, Kind::Interval, "大六度"),
    template("m6", &[0, 8], Quality::Interval, Kind::Interval, "小六度"),
    template("M7", &[0, 11], Quality::Interval, Kind::Interval, "大七度"),
    template("m7", &[0, 10], Quality::Interval, Kind::Interval, "小七度"),
    template("M2", &[0, 2], Quality::Interval, Kind::Interval, "大二度"),
    template("m2", &[0, 1], Quality::Interval, Kind::Interval, "小二度"),
    template("b5", &[0, 6], Quality::Interval, Kind::Interval, "减五度"),
    // ---------- 三和弦 ----------
    template("", &[0, 4, 7], Quality::Major, Kind::Triad, "大三和弦"),
    template("m", &[0, 3, 7], Quality::Minor, Kind::Triad, "小三和弦"),
    template(
        "dim",
        &[0, 3, 6],
        Quality::Diminished,
        Kind::Triad,
        "减三和弦",
    ),
    template(
        "aug",
        &[0, 4, 8],
        Quality::Augmented,
        Kind::Triad,
        "增三和弦",
    ),
    template(
        "sus2",
        &[0, 2, 7],
        Quality::Suspended,
        Kind::Triad,
        "挂二和弦",
    ),
    template(
        "sus4",
        &[0, 5, 7],
        Quality::Suspended,
        Kind::Triad,
        "挂四和弦",
    ),
    template(
        "(b5)",
        &[0, 4, 6],
        Quality::Altered,
        Kind::Triad,
        "大三减五",
    ),
    // ---------- 七和弦 ----------
    template(
        "7",
        &[0, 4, 7, 10],
        Quality::Dominant,
        Kind::Seventh,
        "属七和弦",
    ),
    template(
        "maj7",
        &[0, 4, 7, 11],
        Quality::Major,
        Kind::Seventh,
        "大七和弦",
    ),
    template(
        "m7",
        &[0, 3, 7, 10],
        Quality::Minor,
        Kind::Seventh,
        "小七和弦",
    ),
    template(
        "mM7",
        &[0, 3, 7, 11],
        Quality::Minor,
        Kind::Seventh,
        "小大七和弦",
    ),
    template(
        "dim7",
        &[0, 3, 6, 9],
        Quality::Diminished,
        Kind::Seventh,
        "减七和弦",
    ),
    template(
        "m7b5",
        &[0, 3, 6, 10],
        Quality::Diminished,
        Kind::Seventh,
        "半减七和弦",
    ),
    template(
        "maj7#5",
        &[0, 4, 8, 11],
        Quality::Augmented,
        Kind::Seventh,
        "增大七和弦",
    ),
    template(
        "7b5",
        &[0, 4, 6, 10],
        Quality::Altered,
        Kind::Seventh,
        "属七降五",
    ),
    template(
        "7#5",
        &[0, 4, 8, 10],
        Quality::Altered,
        Kind::Seventh,
        "属七升五",
    ),
    template(
        "maj7b5",
        &[0, 4, 6, 11],
        Quality::Altered,
        Kind::Seventh,
        "大七降五",
    ),
    template(
        "dimM7",
        &[0, 3, 6, 11],
        Quality::Diminished,
        Kind::Seventh,
        "减小大七和弦",
    ),
    template(
        "7sus4",
        &[0, 5, 7, 10],
        Quality::Suspended,
        Kind::Seventh,
        "属七挂四",
    ),
    template(
        "maj7sus4",
        &[0, 5, 7, 11],
        Quality::Suspended,
        Kind::Seventh,
        "大七挂四",
    ),
    template(
        "maj7sus2",
        &[0, 2, 7, 11],
        Quality::Suspended,
        Kind::Seventh,
        "大七挂二",
    ),
    template(
        "7sus2",
        &[0, 2, 7, 10],
        Quality::Suspended,
        Kind::Seventh,
        "属七挂二",
    ),
    template(
        "7add13",
        &[0, 4, 7, 9, 10],
        Quality::Dominant,
        Kind::Seventh,
        "属七加十三",
    ),
    template(
        "7add11",
        &[0, 4, 5, 7, 10],
        Quality::Dominant,
        Kind::Seventh,
        "属七加十一",
    ),
    // ---------- 六和弦与加音和弦 ----------
    template(
        "6",
        &[0, 4, 7, 9],
        Quality::Major,
        Kind::Seventh,
        "大六和弦",
    ),
    template(
        "m6",
        &[0, 3, 7, 9],
        Quality::Minor,
        Kind::Seventh,
        "小六和弦",
    ),
    template(
        "add9",
        &[0, 2, 4, 7],
        Quality::Major,
        Kind::Triad,
        "大三加九",
    ),
    template(
        "madd9",
        &[0, 2, 3, 7],
        Quality::Minor,
        Kind::Triad,
        "小三加九",
    ),
    template(
        "add11",
        &[0, 4, 5, 7],
        Quality::Major,
        Kind::Triad,
        "大三加十一",
    ),
    template(
        "madd11",
        &[0, 3, 5, 7],
        Quality::Minor,
        Kind::Triad,
        "小三加十一",
    ),
    // ---------- 延伸和弦 ----------
    template(
        "9",
        &[0, 2, 4, 7, 10],
        Quality::Dominant,
        Kind::Extended,
        "属九和弦",
    ),
    template(
        "maj9",
        &[0, 2, 4, 7, 11],
        Quality::Major,
        Kind::Extended,
        "大九和弦",
    ),
    template(
        "m9",
        &[0, 2, 3, 7, 10],
        Quality::Minor,
        Kind::Extended,
        "小九和弦",
    ),
    template(
        "7b9",
        &[0, 1, 4, 7, 10],
        Quality::Dominant,
        Kind::Extended,
        "属七降九",
    ),
    template(
        "7#9",
        &[0, 3, 4, 7, 10],
        Quality::Dominant,
        Kind::Extended,
        "属七升九",
    ),
    template(
        "7#11",
        &[0, 4, 6, 7, 10],
        Quality::Dominant,
        Kind::Extended,
        "属七升十一",
    ),
    template(
        "maj7#11",
        &[0, 4, 6, 7, 11],
        Quality::Major,
        Kind::Extended,
        "大七升十一",
    ),
    template(
        "6/9",
        &[0, 2, 4, 7, 9],
        Quality::Major,
        Kind::Extended,
        "六九和弦",
    ),
    template(
        "9sus4",
        &[0, 2, 5, 7, 10],
        Quality::Suspended,
        Kind::Extended,
        "属九挂四",
    ),
    template(
        "11",
        &[0, 2, 4, 5, 7, 10],
        Quality::Dominant,
        Kind::Extended,
        "属十一和弦",
    ),
    template(
        "m11",
        &[0, 2, 3, 5, 7, 10],
        Quality::Minor,
        Kind::Extended,
        "小十一和弦",
    ),
    template(
        "13",
        &[0, 2, 4, 7, 9, 10],
        Quality::Dominant,
        Kind::Extended,
        "属十三和弦",
    ),
    template(
        "maj13",
        &[0, 2, 4, 7, 9, 11],
        Quality::Major,
        Kind::Extended,
        "大十三和弦",
    ),
    template(
        "m13",
        &[0, 2, 3, 7, 9, 10],
        Quality::Minor,
        Kind::Extended,
        "小十三和弦",
    ),
];

/// 每个省略音的代价：高于「命中一个模板音」的收益，保证「精确匹配」永远优于「大而不全」。
const COST_MISSING_NOTE: i32 = 4;
const COST_EXTRA: i32 = 9;
const BONUS_PER_TEMPLATE_NOTE: i32 = 2;
const BONUS_ROOT_POSITION: i32 = 3;
const MAX_EXTRAS: u32 = 2;
const MAX_ALTERNATIVES: usize = 4;

/// 一个和弦（含识别出的结构信息）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Chord {
    /// 去重后的输入音（保持输入顺序；第一个音按约定视为低音）。
    pub notes: Vec<Note>,
    pub root: Note,
    pub bass: Note,
    /// 和弦后缀，含省略音标记，如 `7(no5)`。
    pub suffix: String,
    pub quality: Quality,
    pub kind: Kind,
    /// 中文结构名，如 `属七和弦（省略五音）`。
    pub label: String,
    pub missing: Vec<Note>,
    pub extras: Vec<Note>,
    pub exact: bool,
    pub score: i32,
    pub warning: Option<String>,
}

impl Chord {
    pub fn mask(&self) -> PitchMask {
        mask_of(&self.notes)
    }

    /// 升号拼写的和弦名（兼容旧字段）。
    pub fn name(&self) -> String {
        format!("{}{}", self.root.name(), self.suffix)
    }

    /// 升号拼写的音名列表（兼容旧字段）。
    pub fn note_names(&self) -> Vec<String> {
        self.notes.iter().map(|note| note.name()).collect()
    }

    /// 是否为转位（低音不是根音）。
    pub fn is_inversion(&self) -> bool {
        self.bass != self.root
    }

    /// 和弦根音在第 `degree` 级时所用的调内拼写。
    pub fn symbol_spelled(&self, root_name: &str, bass_name: &str) -> String {
        if self.is_inversion() {
            format!("{}{}/{}", root_name, self.suffix, bass_name)
        } else {
            format!("{}{}", root_name, self.suffix)
        }
    }

    /// 直接按调性拼写完整和弦记号（含斜杠低音）。
    pub fn symbol_in(&self, key: &Key, prefer_flats: bool) -> String {
        let root_name = key.spell_with(self.root, prefer_flats);
        let bass_name = key.spell_with(self.bass, prefer_flats);
        self.symbol_spelled(&root_name, &bass_name)
    }

    /// 按调性拼写的音名列表。
    pub fn notes_in(&self, key: &Key, prefer_flats: bool) -> Vec<String> {
        self.notes
            .iter()
            .map(|&note| key.spell_with(note, prefer_flats))
            .collect()
    }

    /// 是否省略了三音（此时大小调性不确定）。
    pub fn omits_third(&self) -> bool {
        let root = self.root;
        self.missing
            .iter()
            .any(|note| note.interval_to(root) == 3 || note.interval_to(root) == 4)
    }
}

/// 识别一组音，返回按可信度排序的候选（第一个为最佳解释）。
pub fn interpretations(notes: &[Note]) -> Vec<Chord> {
    let unique = dedupe(notes);
    if unique.len() < 2 {
        return Vec::new();
    }
    let input_mask = mask_of(&unique);
    let bass = unique[0];
    let mut candidates: Vec<(i32, usize, Chord)> = Vec::new();

    for (root_index, &root) in unique.iter().enumerate() {
        for (template_index, template) in TEMPLATES.iter().enumerate() {
            if let Some(chord) = match_template(template, root, bass, &unique, input_mask) {
                let order = template_index * 16 + root_index;
                candidates.push((chord.score, order, chord));
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates
        .into_iter()
        .map(|(_, _, chord)| chord)
        .take(MAX_ALTERNATIVES)
        .collect()
}

/// 识别一组音，取最佳解释；永远返回一个和弦（无法识别时给出带警告的兜底结果）。
pub fn interpret(notes: &[Note]) -> Chord {
    let unique = dedupe(notes);
    if let Some(chord) = interpretations(&unique).into_iter().next() {
        return chord;
    }
    let root = unique.first().copied().unwrap_or(Note::new(0));
    Chord {
        notes: unique,
        root,
        bass: root,
        suffix: "?".to_string(),
        quality: Quality::Unknown,
        kind: Kind::Unknown,
        label: "无法识别的音组".to_string(),
        missing: Vec::new(),
        extras: Vec::new(),
        exact: false,
        score: i32::MIN,
        warning: Some("这组音不构成常见和弦结构".to_string()),
    }
}

fn match_template(
    template: &Template,
    root: Note,
    bass: Note,
    notes: &[Note],
    input_mask: PitchMask,
) -> Option<Chord> {
    let mut pattern_mask = 0u16;
    for &interval in template.intervals {
        pattern_mask |= (root + interval as i32).mask();
    }

    let missing_mask = pattern_mask & !input_mask;
    let extras_mask = input_mask & !pattern_mask;

    // 音程模板要求完全精确，不允许省略或多余音。
    if template.kind == Kind::Interval && (missing_mask != 0 || extras_mask != 0) {
        return None;
    }

    let extras_count = extras_mask.count_ones();
    if extras_count > MAX_EXTRAS {
        return None;
    }
    if missing_mask.count_ones() as usize >= template.intervals.len() {
        return None;
    }

    let omittable = omittable_mask(template, root);
    if missing_mask & !omittable != 0 {
        return None; // 缺少不可省略的音
    }

    // 打分：命中越多的模板音越好，省略与额外音都要付代价。
    let missing_count = missing_mask.count_ones();
    let matched_notes = template.intervals.len() as u32 - missing_count;
    let mut score = matched_notes as i32 * BONUS_PER_TEMPLATE_NOTE
        - missing_count as i32 * COST_MISSING_NOTE
        - extras_count as i32 * COST_EXTRA;
    if root == bass {
        score += BONUS_ROOT_POSITION;
    }

    let mut missing: Vec<Note> = Vec::new();
    for &interval in template.intervals {
        let pitch = root + interval as i32;
        if missing_mask & pitch.mask() != 0 && !missing.contains(&pitch) {
            missing.push(pitch);
        }
    }

    let extras: Vec<Note> = pitches_of(extras_mask);
    let mut suffix = template.suffix.to_string();
    if !missing.is_empty() {
        let marks: Vec<String> = missing
            .iter()
            .map(|&note| format!("no{}", tone_label(root.interval_to(note))))
            .collect();
        suffix = format!("{}({})", suffix, marks.join(","));
    }

    let mut label = template.label.to_string();
    if !missing.is_empty() {
        let marks: Vec<String> = missing
            .iter()
            .map(|&note| tone_label(root.interval_to(note)).to_string())
            .collect();
        label = format!("{}（省略 {}）", label, marks.join("、"));
    }
    if !extras.is_empty() {
        label = format!("{}+额外音", label);
    }

    let warning = if missing.iter().any(|&note| {
        let interval = root.interval_to(note);
        interval == 3 || interval == 4
    }) {
        Some("缺少三音，无法确定大小调性".to_string())
    } else if !extras.is_empty() {
        Some(format!(
            "含结构外音 {}",
            extras
                .iter()
                .map(|note| note.name())
                .collect::<Vec<_>>()
                .join(" ")
        ))
    } else {
        None
    };

    Some(Chord {
        notes: notes.to_vec(),
        root,
        bass,
        suffix,
        quality: template.quality,
        kind: template.kind,
        label,
        missing,
        extras,
        exact: missing_mask == 0 && extras_mask == 0,
        score,
        warning,
    })
}

/// 允许省略的音：五音恒定可省；延伸和弦可省九音、十一音；三音代价较高但允许。
fn omittable_mask(template: &Template, root: Note) -> PitchMask {
    let mut mask = (root + 7).mask();
    if template.kind == Kind::Extended {
        mask |= (root + 2).mask() | (root + 5).mask();
    }
    if !matches!(template.quality, Quality::Suspended | Quality::Interval) {
        mask |= (root + 3).mask() | (root + 4).mask();
    }
    mask
}

/// 音程 → 和弦音代号（用于 `(no5)` 之类的标记）。
pub fn tone_label(interval: u8) -> &'static str {
    match interval % 12 {
        0 => "1",
        1 => "b9",
        2 => "9",
        3 | 4 => "3",
        5 => "11",
        6 => "b5",
        7 => "5",
        8 => "#5",
        9 => "13",
        10 | 11 => "7",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{Key, Mode};

    fn notes(names: &[&str]) -> Vec<Note> {
        names
            .iter()
            .map(|name| Note::parse(name).unwrap())
            .collect()
    }

    fn best(names: &[&str]) -> Chord {
        interpret(&notes(names))
    }

    #[test]
    fn triads_and_sevenths() {
        assert_eq!(best(&["C", "E", "G"]).name(), "C");
        assert_eq!(best(&["A", "C", "E"]).name(), "Am");
        assert_eq!(best(&["G", "B", "D", "F"]).name(), "G7");
        assert_eq!(best(&["C", "E", "G", "B"]).name(), "Cmaj7");
        assert_eq!(best(&["B", "D", "F"]).name(), "Bdim");
    }

    #[test]
    fn duplicate_notes_do_not_break_recognition() {
        // 旧实现里重复音会让音数错位并 panic。
        assert_eq!(best(&["C", "C", "E", "G"]).name(), "C");
        assert_eq!(best(&["C", "E", "G", "G", "C"]).name(), "C");
    }

    #[test]
    fn two_note_inputs_are_recognised() {
        assert_eq!(best(&["C", "G"]).name(), "C5");
        let third = best(&["C", "E"]);
        assert_eq!(third.kind, Kind::Interval);
        assert_eq!(third.name(), "CM3");
        // 间隔 1 个半音也要有结果，不能识别失败。
        assert!(interpret(&notes(&["C", "C#"])).kind != Kind::Unknown);
    }

    #[test]
    fn omitted_fifth_uses_shell_voicing() {
        let chord = best(&["C", "E", "Bb"]);
        assert_eq!(chord.name(), "C7(no5)");
        assert_eq!(chord.missing.len(), 1);
        assert!(chord.kind == Kind::Seventh);
    }

    #[test]
    fn omitted_third_is_flagged() {
        let chord = best(&["C", "G", "Bb"]);
        assert!(chord.suffix.contains("no3"), "实际后缀 = {}", chord.suffix);
        assert!(chord.warning.is_some());
    }

    #[test]
    fn five_note_chords_beyond_ninth_are_recognised() {
        assert_eq!(best(&["C", "E", "G", "A", "D"]).name(), "C6/9");
        assert_eq!(best(&["C", "E", "G", "Bb", "D"]).name(), "C9");
        assert_eq!(best(&["C", "E", "G", "B", "F#"]).name(), "Cmaj7#11");
        assert_eq!(best(&["C", "E", "G", "Bb", "A"]).name(), "C7add13");
        assert_eq!(best(&["C", "E", "G", "Bb", "D", "A"]).name(), "C13");
    }

    #[test]
    fn multiple_interpretations_are_offered() {
        // 同一组音既是 C6 也是 Am7，低音（第一个音）决定首选。
        let from_c = interpretations(&notes(&["C", "E", "G", "A"]));
        assert_eq!(from_c[0].name(), "C6");
        assert!(from_c.iter().any(|chord| chord.name() == "Am7"));
        let from_a = interpretations(&notes(&["A", "C", "E", "G"]));
        assert_eq!(from_a[0].name(), "Am7");
        assert!(from_a.iter().any(|chord| chord.name() == "C6"));
    }

    #[test]
    fn inversions_are_detected() {
        let chord = best(&["E", "G", "C"]);
        assert_eq!(chord.root, Note::new(0));
        assert_eq!(chord.bass, Note::new(4));
        assert!(chord.is_inversion());
        let key = Key::from_name("C", Mode::Major);
        assert_eq!(chord.symbol_in(&key, false), "C/E");
    }

    #[test]
    fn unknown_group_never_panics() {
        let chord = interpret(&notes(&["C", "C#", "D"]));
        assert!(chord.warning.is_some() || chord.kind != Kind::Unknown);
        let single = interpret(&notes(&["C"]));
        assert_eq!(single.kind, Kind::Unknown);
    }
}
