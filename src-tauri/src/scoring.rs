//! 调性打分：对 24 个候选调逐个评分、排序，并解决「一个和弦多解」的择优问题。
//!
//! 打分模型（每一项都可解释，便于向用户说明理由）：
//! 1. 每个和弦按「与调的关系」取贴合度：调内 1.0、和声小调 0.75、副属 0.35、借用 0.25、那不勒斯 0.2、调外 0；
//!    首尾和弦权重加倍（走向的头尾最能说明调性）。
//! 2. 主和弦加成：出现主和弦 +0.15、以主和弦开头 +0.20、以主和弦结尾 +0.25；
//!    **完全没出现主和弦则 -0.30**（这是区分关系大小调的关键）。
//! 3. 终止式与经典走向模板提供小幅加成；调号越多扣分越多（作为同分时的取舍）。
//!
//! 职责边界：本模块只做评估与排序，不解析输入、不生成对外 JSON。

use crate::chord::Chord;
use crate::key::Key;
use crate::progression::{self, Cadence, ChordRole, Function, RoleCategory, TemplateMatch};

/// 某个调语境下某个和弦的最终解释。
#[derive(Clone, Debug)]
pub struct ChordChoice {
    /// 该调语境下选中的那个解释。
    pub chord: Chord,
    pub role: ChordRole,
    /// 其它可能解释的名字（升号拼写），供前端展示。
    pub alternatives: Vec<String>,
}

/// 一个候选调的完整评估结果。
#[derive(Clone, Debug)]
pub struct KeyEvaluation {
    pub key: Key,
    pub choices: Vec<ChordChoice>,
    pub cadences: Vec<Cadence>,
    pub templates: Vec<TemplateMatch>,
    /// 完全落在自然音阶内的和弦数（与旧版「匹配和弦数」语义一致）。
    pub matched: usize,
    /// 能在该调中得到解释的和弦数（含副属、借用等）。
    pub explained: usize,
    /// 纯贴合度：各和弦与调的关系的加权平均，0..1（不含位置、终止式等加成）。
    pub fit: f64,
    /// 综合排序分：贴合度 + 主和弦位置 + 终止式 + 走向模板 - 调号复杂度，通常 0..1.5。
    pub score: f64,
    pub reasons: Vec<String>,
}

const EDGE_WEIGHT: f64 = 2.0;
const TONIC_PRESENT_BONUS: f64 = 0.15;
const NO_TONIC_PENALTY: f64 = 0.30;
const FIRST_TONIC_BONUS: f64 = 0.20;
const LAST_TONIC_BONUS: f64 = 0.25;
const UNEXPLAINED_TAIL_PENALTY: f64 = 0.15;
const CADENCE_BONUS_CAP: f64 = 0.15;
const TEMPLATE_BONUS: f64 = 0.05;
const TEMPLATE_BONUS_CAP: f64 = 0.10;
const ACCIDENTAL_PENALTY: f64 = 0.02;

/// 一个和弦在给定调中的贴合度。
fn fit(category: RoleCategory) -> f64 {
    match category {
        RoleCategory::Diatonic => 1.0,
        RoleCategory::HarmonicMinor => 0.75,
        RoleCategory::SecondaryDominant => 0.35,
        RoleCategory::ModalMixture => 0.25,
        RoleCategory::Neapolitan => 0.2,
        RoleCategory::Chromatic => 0.0,
    }
}

/// 在多个解释中挑选最贴合该调的那个。
///
/// 优先级：上下文分类 → 「本身就是该调主和弦」→ 识别阶段的首选（低音为根音）→ 主功能微调。
pub fn choose(key: &Key, interpretations: &[Chord]) -> ChordChoice {
    let mut best: Option<(i32, usize, ChordRole)> = None;
    for (index, chord) in interpretations.iter().enumerate() {
        let role = progression::analyze_role(key, chord);
        let mut rank = role.category.fit_rank() - index as i32;
        if index == 0 {
            // 首选解释（低音即根音）应当胜过其它同分类解释
            rank += 4;
        }
        if progression::is_tonic_chord(key, chord) {
            rank += 20;
        }
        if role.category == RoleCategory::Diatonic && role.function == Function::Tonic {
            // 只做同分微调，绝不能盖过「首选解释」，否则 IV 会被读成 vi(no5)
            rank += 2;
        }
        if best
            .as_ref()
            .is_none_or(|(best_rank, _, _)| rank > *best_rank)
        {
            best = Some((rank, index, role));
        }
    }
    match best {
        Some((_, index, role)) => ChordChoice {
            chord: interpretations[index].clone(),
            role,
            alternatives: interpretations
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, chord)| chord.name())
                .collect(),
        },
        None => {
            // 上层保证至少传入一个解释；这里给出兜底，避免任何 panic。
            let chord = crate::chord::interpret(&[]);
            let role = progression::analyze_role(key, &chord);
            ChordChoice {
                chord,
                role,
                alternatives: Vec::new(),
            }
        }
    }
}

/// 评估单个调。
pub fn evaluate(key: &Key, interpretations: &[Vec<Chord>]) -> KeyEvaluation {
    let total = interpretations.len();
    let choices: Vec<ChordChoice> = interpretations.iter().map(|set| choose(key, set)).collect();

    let mut weighted = 0.0;
    let mut weight_sum = 0.0;
    let mut matched = 0;
    let mut explained = 0;

    for (index, choice) in choices.iter().enumerate() {
        let is_edge = total > 1 && (index == 0 || index + 1 == total);
        let weight = if is_edge { EDGE_WEIGHT } else { 1.0 };
        weight_sum += weight;
        weighted += fit(choice.role.category) * weight;
        if choice.role.category == RoleCategory::Diatonic {
            matched += 1;
        }
        if choice.role.category != RoleCategory::Chromatic {
            explained += 1;
        }
    }

    let fit = if weight_sum > 0.0 {
        weighted / weight_sum
    } else {
        0.0
    };
    let mut score = fit;

    let chords: Vec<Chord> = choices.iter().map(|choice| choice.chord.clone()).collect();
    let roles: Vec<ChordRole> = choices.iter().map(|choice| choice.role.clone()).collect();
    let cadences = progression::detect_cadences(key, &chords, &roles);
    let templates = progression::match_templates(&roles, key);

    let tonic_positions: Vec<bool> = chords
        .iter()
        .map(|chord| progression::is_tonic_chord(key, chord))
        .collect();
    if tonic_positions.iter().any(|present| *present) {
        score += TONIC_PRESENT_BONUS;
    } else {
        score -= NO_TONIC_PENALTY;
    }
    if tonic_positions.first().copied().unwrap_or(false) {
        score += FIRST_TONIC_BONUS;
    }
    if tonic_positions.last().copied().unwrap_or(false) {
        score += LAST_TONIC_BONUS;
    }
    if roles
        .last()
        .is_some_and(|role| role.category == RoleCategory::Chromatic)
    {
        score -= UNEXPLAINED_TAIL_PENALTY;
    }

    let cadence_bonus: f64 = cadences
        .iter()
        .map(|cadence| cadence.bonus)
        .sum::<f64>()
        .min(CADENCE_BONUS_CAP);
    score += cadence_bonus;
    score += (templates.len() as f64 * TEMPLATE_BONUS).min(TEMPLATE_BONUS_CAP);
    score -= key.accidentals().abs() as f64 * ACCIDENTAL_PENALTY;
    // 不截断到 1：保留候选之间的区分度，展示用的百分比由 fit 决定。
    let score = score.max(0.0);

    KeyEvaluation {
        key: key.clone(),
        choices,
        cadences,
        templates,
        matched,
        explained,
        fit,
        score,
        reasons: build_reasons(key, matched, total, explained, &roles),
    }
}

/// 评估全部候选调并按分数排序（同分时调号更简单者优先）。
pub fn evaluate_all(keys: &[Key], interpretations: &[Vec<Chord>]) -> Vec<KeyEvaluation> {
    let mut evaluations: Vec<KeyEvaluation> = keys
        .iter()
        .map(|key| evaluate(key, interpretations))
        .collect();
    evaluations.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.accidentals().abs().cmp(&b.key.accidentals().abs()))
    });
    evaluations
}

fn build_reasons(
    key: &Key,
    matched: usize,
    total: usize,
    explained: usize,
    roles: &[ChordRole],
) -> Vec<String> {
    let mut reasons = vec![format!(
        "{}/{} 个和弦完全落在 {} 的音阶内",
        matched,
        total,
        key.display_name()
    )];
    if explained > matched {
        reasons.push(format!(
            "另有 {} 个和弦可用副属、借用或和声小调解释",
            explained - matched
        ));
    }
    if key.accidentals() != 0 {
        reasons.push(format!(
            "调号：{} 个{}号",
            key.accidentals().abs(),
            if key.accidentals() > 0 { "升" } else { "降" }
        ));
    }
    if roles
        .iter()
        .any(|role| role.category == RoleCategory::HarmonicMinor)
    {
        reasons.push("含和声小调的属功能写法，符合小调习惯".to_string());
    }
    if roles
        .iter()
        .any(|role| role.category == RoleCategory::ModalMixture)
    {
        reasons.push("含同主音大小调借用和弦".to_string());
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::interpretations;
    use crate::note::Note;

    fn sets(rows: &[&[&str]]) -> Vec<Vec<Chord>> {
        rows.iter()
            .map(|row| {
                let notes: Vec<Note> = row.iter().map(|name| Note::parse(name).unwrap()).collect();
                let mut found = interpretations(&notes);
                if found.is_empty() {
                    found.push(crate::chord::interpret(&notes));
                }
                found
            })
            .collect()
    }

    fn top(evaluations: &[KeyEvaluation], count: usize) -> Vec<String> {
        evaluations
            .iter()
            .take(count)
            .map(|evaluation| evaluation.key.display_name())
            .collect()
    }

    fn find<'a>(evaluations: &'a [KeyEvaluation], name: &str) -> &'a KeyEvaluation {
        evaluations
            .iter()
            .find(|evaluation| evaluation.key.display_name() == name)
            .expect("候选调应存在")
    }

    #[test]
    fn c_major_progression_ranks_c_major_first() {
        let chords = sets(&[
            &["C", "E", "G"],
            &["A", "C", "E"],
            &["D", "F", "A"],
            &["G", "B", "D"],
        ]);
        let evaluations = evaluate_all(&Key::all(), &chords);
        assert_eq!(top(&evaluations, 1), ["C 大调"]);
        assert_eq!(evaluations[0].matched, 4);
        assert!(evaluations[0].score > 0.9, "实际 {}", evaluations[0].score);
        // 关系调 F 大调也能解释大部分和弦，但缺少主和弦，必须明显落后
        assert!(find(&evaluations, "F 大调").score < evaluations[0].score - 0.3);
    }

    #[test]
    fn minor_progression_prefers_minor_tonic() {
        // Am – E7 – Am：旧实现完全找不到调；现在应把 A 小调排在第一。
        let chords = sets(&[&["A", "C", "E"], &["E", "G#", "B", "D"], &["A", "C", "E"]]);
        let evaluations = evaluate_all(&Key::all(), &chords);
        assert_eq!(top(&evaluations, 1), ["A 小调"]);
        assert_eq!(evaluations[0].matched, 2);
        assert!(evaluations[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("和声小调")));
        assert!(evaluations[0]
            .cadences
            .iter()
            .any(|cadence| cadence.kind == "authentic"));
    }

    #[test]
    fn last_note_actually_counts() {
        // C – C#dim：没有任何调能解释全部和弦，C 大调只能匹配 1 个。
        let chords = sets(&[&["C", "E", "G"], &["C#", "E", "G"]]);
        let evaluations = evaluate_all(&Key::all(), &chords);
        let c_major = find(&evaluations, "C 大调");
        assert_eq!(c_major.matched, 1);
        assert_eq!(c_major.explained, 1);
    }

    #[test]
    fn ambiguous_chord_follows_the_key_context() {
        // 同一组音 C E G A：C 大调读作 C6（主和弦），A 小调读作 Am7（主和弦）。
        let chords = sets(&[&["C", "E", "G", "A"]]);
        let c_major = evaluate(&Key::from_name("C", crate::key::Mode::Major), &chords);
        assert_eq!(c_major.choices[0].chord.name(), "C6");
        let a_minor = evaluate(&Key::from_name("A", crate::key::Mode::Minor), &chords);
        assert_eq!(a_minor.choices[0].chord.name(), "Am7");
        assert!(!a_minor.choices[0].alternatives.is_empty());
    }

    #[test]
    fn fourth_degree_is_not_misread_as_vi_without_fifth() {
        // F–A–C 在 C 大调里必须读作 IV（F 大三和弦），而不是被主功能加成拉去当 vi(no5)
        let chords = sets(&[
            &["C", "E", "G"],
            &["F", "A", "C"],
            &["G", "B", "D"],
            &["C", "E", "G"],
        ]);
        let c_major = evaluate(&Key::from_name("C", crate::key::Mode::Major), &chords);
        assert_eq!(c_major.choices[1].chord.name(), "F");
        assert_eq!(c_major.choices[1].role.roman, "IV");
        assert_eq!(
            c_major
                .choices
                .iter()
                .map(|c| c.role.roman.as_str())
                .collect::<Vec<_>>(),
            ["I", "IV", "V", "I"]
        );
    }

    #[test]
    fn third_degree_shape_keeps_its_own_root() {
        // D–F–A 在 A 小调里应读作 iv（Dm），而不是 F6(no5)
        let chords = sets(&[&["A", "C", "E"], &["D", "F", "A"], &["E", "G#", "B"]]);
        let a_minor = evaluate(&Key::from_name("A", crate::key::Mode::Minor), &chords);
        assert_eq!(a_minor.choices[1].chord.name(), "Dm");
        assert_eq!(a_minor.choices[1].role.roman, "iv");
    }

    #[test]
    fn borrowing_is_explained_but_costs_points() {
        let chords = sets(&[&["C", "E", "G"], &["Bb", "D", "F"], &["C", "E", "G"]]);
        let c_major = evaluate(&Key::from_name("C", crate::key::Mode::Major), &chords);
        assert_eq!(c_major.matched, 2); // Bb 不在 C 大调音阶内
        assert_eq!(c_major.explained, 3);
        assert_eq!(c_major.choices[1].role.roman, "bVII");
        assert!(c_major.choices[1].role.prefer_flats);
    }
}
