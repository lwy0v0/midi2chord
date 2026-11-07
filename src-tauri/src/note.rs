//! 音高类（pitch class）与音名拼写。
//!
//! 本模块的职责边界：音名 ⇄ 音高（0..11）的转换、以及音高集合的位掩码工具。
//! 它**不理解调性与和弦**，也**不 panic**：所有解析失败都返回 `None`。

use std::fmt;

/// 音高集合位掩码：bit i 表示音高类 i 是否出现。
pub type PitchMask = u16;

/// 升号拼写。
pub const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
/// 降号拼写。
pub const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];
/// 音名字母（C D E F G A B 顺序，级数推算用）。
pub const LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
/// 每个字母对应的自然音高：C=0 D=2 E=4 F=5 G=7 A=9 B=11。
pub const LETTER_PITCHES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

/// 单个音高类（0 = C，11 = B），与八度无关。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Note {
    pub pitch: u8,
}

impl Note {
    pub const fn new(pitch: u8) -> Self {
        Note { pitch: pitch % 12 }
    }

    /// 由任意半音数（可为负）构造。
    pub fn from_semitone(semitone: i32) -> Self {
        Note {
            pitch: semitone.rem_euclid(12) as u8,
        }
    }

    /// 位掩码。
    pub fn mask(self) -> PitchMask {
        1u16 << self.pitch
    }

    /// 升号拼写（兼容旧接口）。
    pub fn name(self) -> String {
        SHARP_NAMES[self.pitch as usize].to_string()
    }

    /// 降号拼写。
    pub fn flat_name(self) -> String {
        FLAT_NAMES[self.pitch as usize].to_string()
    }

    /// 音名解析（支持 `C` `c` `C#` `Db` `C♯` `E♭` `C##` `Cbb`）；失败返回 `None`，绝不 panic。
    pub fn parse(input: &str) -> Option<Note> {
        SpelledNote::parse(input).map(|spelled| spelled.pitch())
    }

    /// 兼容旧接口名。
    pub fn new4str(input: &str) -> Option<Note> {
        Note::parse(input)
    }

    /// `self` 向上到 `other` 的半音数（0..11）。
    pub fn interval_to(self, other: Note) -> u8 {
        (other.pitch + 12 - self.pitch) % 12
    }
}

impl std::ops::Add<i32> for Note {
    type Output = Note;
    fn add(self, other: i32) -> Note {
        Note::from_semitone(self.pitch as i32 + other)
    }
}

impl std::ops::Sub<i32> for Note {
    type Output = Note;
    fn sub(self, other: i32) -> Note {
        Note::from_semitone(self.pitch as i32 - other)
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Debug for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// 带字母与升降号的音名（用于正确拼写，如 `Bb` 而不是 `A#`）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SpelledNote {
    /// 字母下标：0=C 1=D 2=E 3=F 4=G 5=A 6=B
    pub letter: u8,
    /// 升降号数量：-2..=2
    pub accidental: i8,
}

impl SpelledNote {
    pub fn new(letter: u8, accidental: i8) -> Option<Self> {
        if letter <= 6 && accidental.abs() <= 2 {
            Some(SpelledNote { letter, accidental })
        } else {
            None
        }
    }

    /// 对应音高。
    pub fn pitch(self) -> Note {
        Note::from_semitone(LETTER_PITCHES[self.letter as usize] as i32 + self.accidental as i32)
    }

    /// 显示名，如 `Bb`。
    pub fn name(self) -> String {
        format!(
            "{}{}",
            LETTERS[self.letter as usize],
            accidental_suffix(self.accidental)
        )
    }

    /// 解析音名。允许 `C` / `c` / `C#` / `Db` / `C♯` / `E♭` / `C##`。
    pub fn parse(input: &str) -> Option<SpelledNote> {
        let trimmed = input.trim();
        let mut chars = trimmed.chars();
        let letter = letter_index(chars.next()?)?;
        let mut sharps: i8 = 0;
        let mut flats: i8 = 0;
        for ch in chars {
            match ch {
                '#' | '♯' => sharps += 1,
                'b' | '♭' => flats += 1,
                _ => return None,
            }
        }
        // 混用升降号（如 `C#b`）不是合法音名
        if sharps > 0 && flats > 0 {
            return None;
        }
        SpelledNote::new(letter, sharps - flats)
    }
}

/// 把音高按指定字母拼写出来；偏差超过两个升降号则返回 `None`。
pub fn spell_pitch(pitch: u8, letter: u8) -> Option<SpelledNote> {
    if letter > 6 {
        return None;
    }
    let diff = (pitch as i8 - LETTER_PITCHES[letter as usize] as i8).rem_euclid(12);
    let accidental = if diff > 6 { diff - 12 } else { diff };
    SpelledNote::new(letter, accidental)
}

/// 升降号后缀。
pub fn accidental_suffix(accidental: i8) -> &'static str {
    match accidental {
        1 => "#",
        2 => "##",
        -1 => "b",
        -2 => "bb",
        _ => "",
    }
}

/// 字母字符 → 下标。
pub fn letter_index(ch: char) -> Option<u8> {
    match ch.to_ascii_uppercase() {
        'C' => Some(0),
        'D' => Some(1),
        'E' => Some(2),
        'F' => Some(3),
        'G' => Some(4),
        'A' => Some(5),
        'B' => Some(6),
        _ => None,
    }
}

/// 音高集合 → 掩码。
pub fn mask_of(notes: &[Note]) -> PitchMask {
    notes.iter().fold(0, |mask, note| mask | note.mask())
}

/// 掩码 → 升序音高列表。
pub fn pitches_of(mask: PitchMask) -> Vec<Note> {
    (0..12u8)
        .filter(|pitch| mask & (1u16 << pitch) != 0)
        .map(Note::new)
        .collect()
}

/// 掩码中的音数。
pub fn count_of(mask: PitchMask) -> u32 {
    mask.count_ones()
}

/// 按输入顺序去重（保留首次出现的位置）。
pub fn dedupe(notes: &[Note]) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::with_capacity(notes.len());
    for &note in notes {
        if !out.contains(&note) {
            out.push(note);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_common_forms() {
        assert_eq!(Note::new4str("C"), Some(Note::new(0)));
        assert_eq!(Note::new4str("c"), Some(Note::new(0)));
        assert_eq!(Note::new4str(" C# "), Some(Note::new(1)));
        assert_eq!(Note::new4str("Db"), Some(Note::new(1)));
        assert_eq!(Note::new4str("C♯"), Some(Note::new(1)));
        assert_eq!(Note::new4str("E♭"), Some(Note::new(3)));
        assert_eq!(Note::new4str("C##"), Some(Note::new(2)));
        assert_eq!(Note::new4str("Cbb"), Some(Note::new(10)));
        assert_eq!(Note::new4str("b"), Some(Note::new(11)));
    }

    #[test]
    fn parse_never_panics() {
        for bad in ["", " ", "H", "♯", "♭", "C$x", "C###", "1", "C#b#b#"] {
            assert_eq!(Note::parse(bad), None, "输入 {bad:?} 应解析失败");
        }
    }

    #[test]
    fn spelling_round_trip() {
        assert_eq!(SpelledNote::new(6, -1).unwrap().name(), "Bb");
        assert_eq!(SpelledNote::new(6, -1).unwrap().pitch(), Note::new(10));
        assert_eq!(spell_pitch(10, 6).unwrap().name(), "Bb");
        assert_eq!(spell_pitch(6, 3).unwrap().name(), "F#");
        assert_eq!(spell_pitch(6, 6), None); // B 到 F# 需要 6 个升号，不接受
    }

    #[test]
    fn mask_helpers() {
        let notes = [Note::new(0), Note::new(4), Note::new(7)];
        assert_eq!(count_of(mask_of(&notes)), 3);
        assert_eq!(pitches_of(mask_of(&notes)), notes.to_vec());
        assert_eq!(dedupe(&[Note::new(0), Note::new(0), Note::new(4)]).len(), 2);
        assert_eq!(Note::new(0).interval_to(Note::new(7)), 7);
        assert_eq!(Note::new(7).interval_to(Note::new(0)), 5);
    }
}
