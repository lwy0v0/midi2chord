//! 调性模型：24 个候选调、音阶掩码，以及「按调号拼写音名」。
//!
//! 职责边界：本模块只回答「某个音/某组音属于哪个调、在这个调里叫什么、是第几级」，
//! 不负责和弦识别（见 [`crate::chord`]）与走向解释（见 [`crate::progression`]）。

use crate::note::{pitches_of, spell_pitch, Note, PitchMask, SpelledNote};
use std::fmt;

/// 大调音阶音程。
pub const MAJOR_SCALE: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// 自然小调音阶音程。
pub const NATURAL_MINOR_SCALE: [i8; 7] = [0, 2, 3, 5, 7, 8, 10];
/// 小调相对自然小调额外允许的「升六级 / 升七级」（和声小调、旋律小调）。
pub const MINOR_RAISED: [i8; 2] = [6 + 3, 7 + 4];

/// 字母在五度圈上的位置（C=0 G=1 D=2 A=3 E=4 B=5 F=-1）。
const LETTER_FIFTHS: [i8; 7] = [0, 2, 4, -1, 1, 3, 5];

/// 大调调名表（每个音高的常用拼写）。
const MAJOR_KEY_NAMES: [(u8, &str); 12] = [
    (0, "C"),
    (1, "Db"),
    (2, "D"),
    (3, "Eb"),
    (4, "E"),
    (5, "F"),
    (6, "F#"),
    (7, "G"),
    (8, "Ab"),
    (9, "A"),
    (10, "Bb"),
    (11, "B"),
];

/// 小调调名表（每个音高的常用拼写）。
const MINOR_KEY_NAMES: [(u8, &str); 12] = [
    (0, "C"),
    (1, "C#"),
    (2, "D"),
    (3, "Eb"),
    (4, "E"),
    (5, "F"),
    (6, "F#"),
    (7, "G"),
    (8, "G#"),
    (9, "A"),
    (10, "Bb"),
    (11, "B"),
];

/// 调式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Mode {
    Major,
    Minor,
}

impl Mode {
    pub fn is_major(self) -> bool {
        matches!(self, Mode::Major)
    }

    pub fn id(self) -> &'static str {
        match self {
            Mode::Major => "major",
            Mode::Minor => "minor",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Major => "大调",
            Mode::Minor => "小调",
        }
    }

    /// 同主音的另一调式。
    pub fn parallel(self) -> Mode {
        match self {
            Mode::Major => Mode::Minor,
            Mode::Minor => Mode::Major,
        }
    }

    fn scale(self) -> &'static [i8; 7] {
        match self {
            Mode::Major => &MAJOR_SCALE,
            Mode::Minor => &NATURAL_MINOR_SCALE,
        }
    }
}

/// 一个具体的调。
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    tonic: SpelledNote,
    mode: Mode,
    scale: [Note; 7],
    natural_mask: PitchMask,
    extended_mask: PitchMask,
    parallel_mask: PitchMask,
    /// 调号升降号数量：正数=升号，负数=降号。
    accidentals: i8,
}

impl Key {
    /// 由主音拼写与调式构造。
    pub fn new(tonic: SpelledNote, mode: Mode) -> Self {
        let intervals = mode.scale();
        let mut scale = [Note::new(0); 7];
        let mut natural_mask = 0u16;
        for (index, &interval) in intervals.iter().enumerate() {
            let note = tonic.pitch() + interval as i32;
            scale[index] = note;
            natural_mask |= note.mask();
        }
        // 小调额外接受和声/旋律小调的升六、升七级音。
        let mut extended_mask = natural_mask;
        if !mode.is_major() {
            for &raised in MINOR_RAISED.iter() {
                extended_mask |= (tonic.pitch() + raised as i32).mask();
            }
        }
        let parallel_mask = Key::scale_mask(tonic.pitch(), mode.parallel());
        let accidentals = signature_accidentals(tonic, mode);
        Key {
            tonic,
            mode,
            scale,
            natural_mask,
            extended_mask,
            parallel_mask,
            accidentals,
        }
    }

    /// 由调名（如 `Bb`）与调式构造；名称非法时退化为 C。
    pub fn from_name(name: &str, mode: Mode) -> Self {
        let tonic = SpelledNote::parse(name).unwrap_or(SpelledNote {
            letter: 0,
            accidental: 0,
        });
        Key::new(tonic, mode)
    }

    /// 全部 24 个候选调（按半音顺序，大调在前），顺序稳定，便于结果复现。
    pub fn all() -> Vec<Key> {
        let mut keys = Vec::with_capacity(24);
        for pitch in 0..12u8 {
            if let Some((_, name)) = MAJOR_KEY_NAMES.iter().find(|(p, _)| *p == pitch) {
                keys.push(Key::from_name(name, Mode::Major));
            }
            if let Some((_, name)) = MINOR_KEY_NAMES.iter().find(|(p, _)| *p == pitch) {
                keys.push(Key::from_name(name, Mode::Minor));
            }
        }
        keys
    }

    fn scale_mask(tonic: Note, mode: Mode) -> PitchMask {
        mode.scale().iter().fold(0u16, |mask, &interval| {
            mask | (tonic + interval as i32).mask()
        })
    }

    pub fn tonic(&self) -> Note {
        self.tonic.pitch()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// 同主音的另一个调式（用于判断借用和弦）。
    pub fn parallel(&self) -> Key {
        Key::new(self.tonic, self.mode.parallel())
    }

    pub fn is_major(&self) -> bool {
        self.mode.is_major()
    }

    /// 调号升降号数量（正=升，负=降）。
    pub fn accidentals(&self) -> i8 {
        self.accidentals
    }

    /// 调名（主音拼写），与旧版 `key` 字段语义一致。
    pub fn name(&self) -> String {
        self.tonic.name()
    }

    /// 如 `Bb 大调`。
    pub fn display_name(&self) -> String {
        format!("{} {}", self.tonic.name(), self.mode.label())
    }

    /// 如 `Bb 大调（自然大调）` / `A 小调（自然/和声小调）`。
    pub fn full_name(&self) -> String {
        match self.mode {
            Mode::Major => format!("{} 大调", self.tonic.name()),
            Mode::Minor => format!("{} 小调（自然/和声小调）", self.tonic.name()),
        }
    }

    /// 自然音阶掩码。
    pub fn natural_mask(&self) -> PitchMask {
        self.natural_mask
    }

    /// 含和声/旋律小调变化音的扩展掩码（大调等于自然音阶）。
    pub fn extended_mask(&self) -> PitchMask {
        self.extended_mask
    }

    /// 同主音另一调式的音阶掩码（用于判断借用和弦）。
    pub fn parallel_mask(&self) -> PitchMask {
        self.parallel_mask
    }

    /// 第 `degree`（1..7）级的音；越界返回 `None`。
    pub fn degree_note(&self, degree: u8) -> Option<Note> {
        if (1..=7).contains(&degree) {
            Some(self.scale[(degree - 1) as usize])
        } else {
            None
        }
    }

    /// 自然音阶列表。
    pub fn scale_notes(&self) -> Vec<Note> {
        self.scale.to_vec()
    }

    /// 音级（0 起）→ 音；越界返回 `None`。
    pub fn scale_note_at(&self, index: usize) -> Option<Note> {
        self.scale.get(index).copied()
    }

    /// 是否属于自然音阶。
    pub fn contains(&self, note: Note) -> bool {
        self.natural_mask & note.mask() != 0
    }

    /// 是否属于「自然 + 和声/旋律小调」的扩展音阶。
    pub fn contains_extended(&self, note: Note) -> bool {
        self.extended_mask & note.mask() != 0
    }

    /// 一组音是否全在自然音阶内。
    pub fn mask_in_scale(&self, mask: PitchMask) -> bool {
        mask & !self.natural_mask == 0
    }

    /// 一组音是否全在扩展音阶内。
    pub fn mask_in_extended(&self, mask: PitchMask) -> bool {
        mask & !self.extended_mask == 0
    }

    /// 一组音是否全在同主音另一调式的音阶内。
    pub fn mask_in_parallel(&self, mask: PitchMask) -> bool {
        mask & !self.parallel_mask == 0
    }

    /// 音级（1..7），不在音阶内返回 0（与旧版 `level` 语义一致）。
    pub fn degree_number(&self, note: Note) -> u8 {
        self.scale
            .iter()
            .position(|&scale_note| scale_note == note)
            .map_or(0, |index| (index + 1) as u8)
    }

    /// 音级（0 起），不在音阶内返回 `None`。
    pub fn degree_index(&self, note: Note) -> Option<u8> {
        self.scale
            .iter()
            .position(|&scale_note| scale_note == note)
            .map(|index| index as u8)
    }

    /// 是否倾向降号拼写。
    pub fn prefer_flats(&self) -> bool {
        self.accidentals < 0
    }

    /// 按本调的音级字母拼写某个音高（不依赖具体和弦语境）。
    pub fn spell(&self, note: Note) -> String {
        self.spell_inner(note, None)
    }

    /// 按本调的音级字母拼写某个音高；`prefer_flats` 由和弦在调中的角色给出
    /// （例如同主音借用和弦用降号、副属和弦用升号）。
    pub fn spell_with(&self, note: Note, prefer_flats: bool) -> String {
        self.spell_inner(note, Some(prefer_flats))
    }

    fn spell_inner(&self, note: Note, preference: Option<bool>) -> String {
        // 和声/旋律小调的升六级、升七级是固定写法，直接用升号。
        if preference.is_none() && !self.mode.is_major() {
            let tonic = self.tonic.pitch();
            let degree = if note.pitch == (tonic + 11).pitch {
                Some(7u8)
            } else if note.pitch == (tonic + 9).pitch {
                Some(6u8)
            } else {
                None
            };
            if let Some(degree) = degree {
                let letter = (self.tonic.letter + degree - 1) % 7;
                if let Some(spelled) = spell_pitch(note.pitch, letter) {
                    return spelled.name();
                }
            }
        }

        let mut best: Option<(i32, String)> = None;
        for degree in 0..7u8 {
            let letter = (self.tonic.letter + degree) % 7;
            if let Some(spelled) = spell_pitch(note.pitch, letter) {
                let cost = spelled.accidental.abs() as i32 * 4
                    + self.direction_cost(degree + 1, spelled.accidental, preference);
                if best.as_ref().is_none_or(|(best_cost, _)| cost < *best_cost) {
                    best = Some((cost, spelled.name()));
                }
            }
        }
        best.map(|(_, name)| name).unwrap_or_else(|| note.name())
    }

    /// 拼写方向代价：0 = 该方向在本调属常规写法，1 = 可用但少见，3 = 不常规。
    fn direction_cost(&self, degree: u8, accidental: i8, preference: Option<bool>) -> i32 {
        if accidental == 0 {
            return 0;
        }
        let is_flat = accidental < 0;
        match preference {
            Some(prefer_flats) => {
                if is_flat == prefer_flats {
                    0
                } else {
                    3
                }
            }
            None => {
                // 大调习惯把升号留给 #1、#4；小调还会用到升 3、升 6、升 7。
                let sharp_conventional = match self.mode {
                    Mode::Major => matches!(degree, 1 | 4),
                    Mode::Minor => matches!(degree, 1 | 3 | 4 | 6 | 7),
                };
                if is_flat {
                    if sharp_conventional {
                        1
                    } else {
                        0
                    }
                } else if sharp_conventional {
                    0
                } else {
                    3
                }
            }
        }
    }

    /// 第 `degree`（1..7）级对应的字母拼写（用于罗马数字的调内拼写）。
    pub fn spell_degree(&self, degree: u8, accidental: i8) -> Option<String> {
        if !(1..=7).contains(&degree) {
            return None;
        }
        let letter = (self.tonic.letter + degree - 1) % 7;
        SpelledNote::new(letter, accidental).map(|spelled| spelled.name())
    }

    /// 主三和弦掩码。
    pub fn tonic_triad_mask(&self) -> PitchMask {
        let third = if self.mode.is_major() { 4 } else { 3 };
        self.tonic.pitch().mask()
            | (self.tonic.pitch() + third).mask()
            | (self.tonic.pitch() + 7).mask()
    }

    /// 自然音阶各音位掩码。
    pub fn natural_notes(&self) -> Vec<Note> {
        pitches_of(self.natural_mask)
    }
}

/// 由主音拼写与调式推算调号升降号数量。
fn signature_accidentals(tonic: SpelledNote, mode: Mode) -> i8 {
    let fifths = LETTER_FIFTHS[tonic.letter as usize] + 7 * tonic.accidental;
    match mode {
        // 小调调号与其关系大调相同（主音字母向下三个字母）。
        Mode::Minor => fifths - 3,
        Mode::Major => fifths,
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str, mode: Mode) -> Key {
        Key::from_name(name, mode)
    }

    #[test]
    fn all_keys_are_twenty_four_and_unique() {
        let keys = Key::all();
        assert_eq!(keys.len(), 24);
        let mut names: Vec<String> = keys
            .iter()
            .map(|key| format!("{}{}", key.name(), key.mode().id()))
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 24);
    }

    #[test]
    fn c_major_scale_is_natural() {
        let c = key("C", Mode::Major);
        let names: Vec<String> = c.scale_notes().iter().map(|n| n.name()).collect();
        assert_eq!(names, ["C", "D", "E", "F", "G", "A", "B"]);
        assert_eq!(c.accidentals(), 0);
        assert!(c.contains(Note::new(11)));
        assert!(!c.contains(Note::new(10)));
    }

    #[test]
    fn minor_extended_accepts_harmonic_and_melodic_notes() {
        let a_minor = key("A", Mode::Minor);
        // 自然小调没有 G#，扩展后应接受（和声小调的升七级）。
        assert!(!a_minor.contains(Note::new(8)));
        assert!(a_minor.contains_extended(Note::new(8)));
        assert!(a_minor.mask_in_extended(Note::new(8).mask() | Note::new(0).mask()));
        assert_eq!(a_minor.degree_number(Note::new(0)), 3);
    }

    #[test]
    fn signatures_match_circle_of_fifths() {
        assert_eq!(key("G", Mode::Major).accidentals(), 1);
        assert_eq!(key("F", Mode::Major).accidentals(), -1);
        assert_eq!(key("Bb", Mode::Major).accidentals(), -2);
        assert_eq!(key("F#", Mode::Major).accidentals(), 6);
        assert_eq!(key("A", Mode::Minor).accidentals(), 0);
        assert_eq!(key("E", Mode::Minor).accidentals(), 1);
        assert_eq!(key("D", Mode::Minor).accidentals(), -1);
        assert_eq!(key("C", Mode::Minor).accidentals(), -3);
    }

    #[test]
    fn spelling_follows_key_signature() {
        let c = key("C", Mode::Major);
        // C 大调里的降七级必须拼成 Bb 而不是 A#。
        assert_eq!(c.spell(Note::new(10)), "Bb");
        assert_eq!(c.spell(Note::new(6)), "F#");
        let f = key("F", Mode::Major);
        assert_eq!(f.spell(Note::new(10)), "Bb");
        assert!(f.prefer_flats());
        let a_minor = key("A", Mode::Minor);
        assert_eq!(a_minor.spell(Note::new(8)), "G#");
    }

    #[test]
    fn parallel_mask_describes_borrowed_notes() {
        let c = key("C", Mode::Major);
        // C 大调借用 C 小调：Eb / Ab / Bb 都在同主音小调音阶内。
        assert!(c.mask_in_parallel(Note::new(3).mask()));
        assert!(c.mask_in_parallel(Note::new(8).mask()));
        assert!(c.mask_in_parallel(Note::new(10).mask()));
        assert!(!c.mask_in_scale(Note::new(3).mask()));
    }

    #[test]
    fn tonic_triad_and_degrees() {
        let c = key("C", Mode::Major);
        assert_eq!(
            c.tonic_triad_mask(),
            Note::new(0).mask() | Note::new(4).mask() | Note::new(7).mask()
        );
        assert_eq!(c.degree_number(Note::new(7)), 5);
        assert_eq!(c.degree_number(Note::new(1)), 0);
        assert_eq!(c.degree_note(5), Some(Note::new(7)));
        assert_eq!(c.spell_degree(7, -1).unwrap(), "Bb");
    }
}
