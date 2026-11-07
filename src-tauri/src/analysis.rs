//! 对外分析入口：输入校验 → 和弦识别 → 24 调打分 → 组装对外 JSON。
//!
//! ## 对外契约
//! 旧字段（`keys[].key`、`keys[].chords[].chord/name/level`）**保持原样**，
//! 新增字段一律追加，前端可渐进使用。

use serde::{Deserialize, Serialize};

use crate::chord::{self, Chord};
use crate::key::Key;
use crate::note::{dedupe, Note};
use crate::progression::RoleCategory;
use crate::scoring::{self, ChordChoice, KeyEvaluation};

/// 候选调最多返回多少个。
const MAX_KEYS: usize = 12;
/// 一个调都匹配不上时退回的候选数。
const FALLBACK_KEYS: usize = 3;

/// 输入校验错误：带行列定位，前端可直接高亮到具体格子。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeError {
    /// 机器可读错误码：emptyInput / emptyNote / invalidNote / tooFewNotes。
    pub code: String,
    pub message: String,
    /// 0 起的行号（和弦序号）。
    pub row: Option<usize>,
    /// 0 起的列号（音序号）。
    pub col: Option<usize>,
}

impl AnalyzeError {
    fn global(code: &str, message: impl Into<String>) -> Self {
        AnalyzeError {
            code: code.to_string(),
            message: message.into(),
            row: None,
            col: None,
        }
    }

    fn at(code: &str, message: impl Into<String>, row: usize, col: usize) -> Self {
        AnalyzeError {
            code: code.to_string(),
            message: message.into(),
            row: Some(row),
            col: Some(col),
        }
    }
}

/// 单个和弦在某个调下的分析结果。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordWithLevel {
    // ---------- 旧字段（保持兼容） ----------
    /// 和弦音名（升号拼写）。
    pub chord: Vec<String>,
    /// 和弦名（升号拼写）。
    pub name: String,
    /// 音级 1..7；0 表示根音不在该调音阶内。
    pub level: u8,
    // ---------- 新增字段 ----------
    /// 按调号拼写的和弦记号，转位带斜杠低音。
    pub symbol: String,
    pub root: String,
    pub bass: String,
    pub quality: String,
    pub quality_label: String,
    pub suffix: String,
    pub kind: String,
    /// 中文结构名，如「属七和弦（省略 5）」。
    pub label: String,
    /// 罗马数字。
    pub roman: String,
    pub degree: u8,
    /// T / S / D / —。
    pub function: String,
    pub function_label: String,
    /// 是否完全落在该调的自然音阶内。
    pub in_scale: bool,
    /// 与调的关系分类。
    pub category: String,
    /// 为什么这么解释。
    pub explanation: String,
    pub tags: Vec<String>,
    /// 其它可能的解释。
    pub alternatives: Vec<String>,
    /// 按调号拼写的音名。
    pub notes: Vec<String>,
    /// 被省略的音。
    pub missing: Vec<String>,
    /// 结构外的音。
    pub extras: Vec<String>,
    pub warning: Option<String>,
}

/// 终止式。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CadenceInfo {
    pub at: usize,
    pub kind: String,
    pub formula: String,
    pub description: String,
}

/// 命中的经典走向。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub formula: String,
    pub note: String,
}

/// 一个候选调。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCandidate {
    // ---------- 旧字段（保持兼容） ----------
    /// 调名（主音）。
    pub key: String,
    pub chords: Vec<ChordWithLevel>,
    // ---------- 新增字段 ----------
    pub is_major: bool,
    pub mode: String,
    pub display_name: String,
    pub full_name: String,
    /// 综合排序分（含主和弦位置、终止式、走向模板加成），通常 0..1.5。
    pub score: f64,
    /// 0..100 的整数百分比：调内贴合度（加权平均，不含位置与终止式加成）。
    pub confidence: u8,
    /// 排名，从 1 开始。
    pub rank: usize,
    /// 完全在调内的和弦数。
    pub matched: usize,
    /// 能解释的和弦数（含副属、借用等）。
    pub explained: usize,
    pub total: usize,
    pub romans: Vec<String>,
    pub cadences: Vec<CadenceInfo>,
    pub templates: Vec<TemplateInfo>,
    pub reasons: Vec<String>,
    pub scale_notes: Vec<String>,
    pub accidentals: i8,
}

/// 分析结果。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyResult {
    /// 旧字段：候选调列表（已按分数排序）。
    pub keys: Vec<KeyCandidate>,
    // ---------- 新增字段 ----------
    /// 与调性无关的和弦识别结果。
    pub recognized: Vec<String>,
    pub warnings: Vec<String>,
    pub total: usize,
}

/// 已解析的一个和弦。
struct ParsedChord {
    notes: Vec<Note>,
    duplicated: bool,
}

/// Tauri 命令入口。
#[tauri::command]
pub fn analyze(chord_sequence: Vec<Vec<String>>) -> Result<KeyResult, AnalyzeError> {
    analyze_sequence(&chord_sequence)
}

/// 纯函数入口，便于测试与复用。
pub fn analyze_sequence(chord_sequence: &[Vec<String>]) -> Result<KeyResult, AnalyzeError> {
    let parsed = parse_input(chord_sequence)?;
    let sets: Vec<Vec<Chord>> = parsed
        .iter()
        .map(|parsed_chord| interpretations_for(&parsed_chord.notes))
        .collect();

    let mut warnings: Vec<String> = Vec::new();
    for (index, parsed_chord) in parsed.iter().enumerate() {
        if parsed_chord.duplicated {
            warnings.push(format!("第 {} 个和弦有重复音，已按音高集合去重", index + 1));
        }
        let primary = &sets[index][0];
        if let Some(warning) = &primary.warning {
            warnings.push(format!(
                "第 {} 个和弦（{}）：{}",
                index + 1,
                primary.name(),
                warning
            ));
        }
    }

    let evaluations = scoring::evaluate_all(&Key::all(), &sets);
    let mut kept: Vec<KeyEvaluation> = evaluations
        .iter()
        .filter(|evaluation| evaluation.matched >= 1)
        .cloned()
        .collect();
    if kept.is_empty() {
        warnings.push("没有任何调能容纳全部和弦，下面是相对最接近的候选".to_string());
        kept = evaluations.iter().take(FALLBACK_KEYS).cloned().collect();
    }
    kept.truncate(MAX_KEYS);

    let total = sets.len();
    let keys = kept
        .iter()
        .enumerate()
        .map(|(index, evaluation)| build_candidate(evaluation, index + 1, total))
        .collect();
    let recognized = sets.iter().map(|set| set[0].name()).collect();

    Ok(KeyResult {
        keys,
        recognized,
        warnings,
        total,
    })
}

/// 校验并解析输入。
fn parse_input(rows: &[Vec<String>]) -> Result<Vec<ParsedChord>, AnalyzeError> {
    if rows.is_empty() {
        return Err(AnalyzeError::global("emptyInput", "请至少输入一个和弦"));
    }
    let mut parsed = Vec::with_capacity(rows.len());
    for (row_index, row) in rows.iter().enumerate() {
        let mut notes = Vec::with_capacity(row.len());
        for (col_index, cell) in row.iter().enumerate() {
            let trimmed = cell.trim();
            if trimmed.is_empty() {
                return Err(AnalyzeError::at(
                    "emptyNote",
                    format!(
                        "第 {} 个和弦的第 {} 个音还没有选择",
                        row_index + 1,
                        col_index + 1
                    ),
                    row_index,
                    col_index,
                ));
            }
            match Note::parse(trimmed) {
                Some(note) => notes.push(note),
                None => {
                    return Err(AnalyzeError::at(
                        "invalidNote",
                        format!(
                            "第 {} 个和弦的第 {} 个音「{}」不是有效音名",
                            row_index + 1,
                            col_index + 1,
                            trimmed
                        ),
                        row_index,
                        col_index,
                    ))
                }
            }
        }
        let unique = dedupe(&notes);
        if unique.len() < 2 {
            return Err(AnalyzeError::at(
                "tooFewNotes",
                format!(
                    "第 {} 个和弦至少需要 2 个不同的音（当前 {} 个）",
                    row_index + 1,
                    unique.len()
                ),
                row_index,
                0,
            ));
        }
        parsed.push(ParsedChord {
            duplicated: unique.len() != notes.len(),
            notes: unique,
        });
    }
    Ok(parsed)
}

/// 识别一个和弦的所有可能解释（至少一个）。
fn interpretations_for(notes: &[Note]) -> Vec<Chord> {
    let mut found = chord::interpretations(notes);
    if found.is_empty() {
        found.push(chord::interpret(notes));
    }
    found
}

fn build_candidate(evaluation: &KeyEvaluation, rank: usize, total: usize) -> KeyCandidate {
    let key = &evaluation.key;
    let chords: Vec<ChordWithLevel> = evaluation
        .choices
        .iter()
        .map(|choice| build_chord(choice, key))
        .collect();
    KeyCandidate {
        key: key.name(),
        chords,
        is_major: key.is_major(),
        mode: key.mode().id().to_string(),
        display_name: key.display_name(),
        full_name: key.full_name(),
        score: (evaluation.score * 1000.0).round() / 1000.0,
        confidence: (evaluation.fit * 100.0).round().clamp(0.0, 100.0) as u8,
        rank,
        matched: evaluation.matched,
        explained: evaluation.explained,
        total,
        romans: evaluation
            .choices
            .iter()
            .map(|choice| choice.role.roman.clone())
            .collect(),
        cadences: evaluation
            .cadences
            .iter()
            .map(|cadence| CadenceInfo {
                at: cadence.at,
                kind: cadence.kind.clone(),
                formula: cadence.formula.clone(),
                description: cadence.description.clone(),
            })
            .collect(),
        templates: evaluation
            .templates
            .iter()
            .map(|hit| TemplateInfo {
                name: hit.name.clone(),
                start: hit.start,
                end: hit.end,
                formula: hit.formula.clone(),
                note: hit.note.clone(),
            })
            .collect(),
        reasons: evaluation.reasons.clone(),
        scale_notes: key
            .scale_notes()
            .iter()
            .map(|note| key.spell(*note))
            .collect(),
        accidentals: key.accidentals(),
    }
}

fn build_chord(choice: &ChordChoice, key: &Key) -> ChordWithLevel {
    let chord = &choice.chord;
    let role = &choice.role;
    let prefer_flats = role.prefer_flats;
    ChordWithLevel {
        chord: chord.note_names(),
        name: chord.name(),
        level: role.degree,
        symbol: chord.symbol_in(key, prefer_flats),
        root: key.spell_with(chord.root, prefer_flats),
        bass: key.spell_with(chord.bass, prefer_flats),
        quality: chord.quality.id().to_string(),
        quality_label: chord.quality.label().to_string(),
        suffix: chord.suffix.clone(),
        kind: chord.kind.id().to_string(),
        label: chord.label.clone(),
        roman: role.roman.clone(),
        degree: role.degree,
        function: role.function.label().to_string(),
        function_label: role.function.describe().to_string(),
        in_scale: role.category == RoleCategory::Diatonic,
        category: role.category.id().to_string(),
        explanation: role.explanation.clone(),
        tags: role.tags.clone(),
        alternatives: choice.alternatives.clone(),
        notes: chord.notes_in(key, prefer_flats),
        missing: chord
            .missing
            .iter()
            .map(|note| key.spell_with(*note, prefer_flats))
            .collect(),
        extras: chord
            .extras
            .iter()
            .map(|note| key.spell_with(*note, prefer_flats))
            .collect(),
        warning: chord.warning.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(input: &[&[&str]]) -> Vec<Vec<String>> {
        input
            .iter()
            .map(|row| row.iter().map(|cell| cell.to_string()).collect())
            .collect()
    }

    #[test]
    fn legacy_fields_are_still_present() {
        let result = analyze_sequence(&rows(&[
            &["C", "E", "G"],
            &["A", "C", "E"],
            &["F", "A", "C"],
            &["G", "B", "D"],
        ]))
        .expect("应分析成功");
        let best = &result.keys[0];
        assert_eq!(best.key, "C");
        assert!(best.is_major);
        assert_eq!(best.confidence, 100);
        assert_eq!(best.chords.len(), 4);
        assert_eq!(best.chords[0].name, "C");
        assert_eq!(best.chords[0].chord, ["C", "E", "G"]);
        assert_eq!(best.chords[3].level, 5);
        assert_eq!(best.chords[3].roman, "V");
        assert_eq!(best.chords[3].function, "D");
    }

    #[test]
    fn minor_key_is_reported_as_minor() {
        let result = analyze_sequence(&rows(&[
            &["A", "C", "E"],
            &["E", "G#", "B"],
            &["A", "C", "E"],
        ]))
        .expect("应分析成功");
        let best = &result.keys[0];
        assert_eq!(best.key, "A");
        assert!(!best.is_major);
        assert_eq!(best.display_name, "A 小调");
        assert!(best.romans.iter().any(|roman| roman == "V"));
        assert!(best
            .cadences
            .iter()
            .any(|cadence| cadence.kind == "authentic"));
    }

    #[test]
    fn octave_and_duplicate_inputs_are_accepted() {
        let result = analyze_sequence(&rows(&[&["C", "C", "E", "G"], &["G", "B", "D"]]))
            .expect("重复音不应报错");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("重复音")));
    }

    #[test]
    fn two_note_and_shell_voicings_are_accepted() {
        let result = analyze_sequence(&rows(&[&["C", "G"], &["C", "E", "Bb"]]))
            .expect("2 个音与省略五音都不应报错");
        assert_eq!(result.recognized[0], "C5");
        assert_eq!(result.recognized[1], "C7(no5)");
    }

    #[test]
    fn validation_errors_point_at_the_right_cell() {
        let error =
            analyze_sequence(&rows(&[&["C", "E", "G"], &["D", "", "A"]])).expect_err("空音应报错");
        assert_eq!(error.code, "emptyNote");
        assert_eq!(error.row, Some(1));
        assert_eq!(error.col, Some(1));

        let error = analyze_sequence(&rows(&[&["C", "E", "G"], &["H", "A", "C"]]))
            .expect_err("非法音名应报错");
        assert_eq!(error.code, "invalidNote");
        assert_eq!(error.row, Some(1));
        assert_eq!(error.col, Some(0));

        let error = analyze_sequence(&rows(&[&["C", "C", "C"]])).expect_err("同音应报错");
        assert_eq!(error.code, "tooFewNotes");
        assert_eq!(error.row, Some(0));

        let error = analyze_sequence(&[]).expect_err("空输入应报错");
        assert_eq!(error.code, "emptyInput");
    }

    #[test]
    fn results_serialise_with_camel_case_keys() {
        let result = analyze_sequence(&rows(&[
            &["C", "E", "G"],
            &["G", "B", "D", "F"],
            &["C", "E", "G"],
        ]))
        .expect("应分析成功");
        let json = serde_json::to_value(&result).expect("应可序列化");
        let best = &json["keys"][0];
        assert!(best.get("isMajor").is_some());
        assert!(best.get("displayName").is_some());
        assert!(best["chords"][1].get("inScale").is_some());
        assert!(best["chords"][1].get("roman").is_some());
        assert_eq!(best["key"], "C");
    }
}
