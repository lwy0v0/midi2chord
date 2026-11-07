// 和弦识别内核 · 实况基线探针
//
// 用途：把 docs/chord-and-midi-expansion-research.md §1.2 的实测表固化成可重跑的基线。
// 改内核之后重跑，一眼看出哪些用例转正、哪些翻车。
//
// 为什么不走 cargo：src-tauri/target 体积大，且本探针只关心 note/key/chord/progression/scoring
// 这五个零外部依赖的模块（唯一的外部依赖 serde 在 analysis.rs，探针不碰它）。
//
// 跑法（在仓库根目录）：
//   rustc --edition 2021 -A dead_code -A unused_imports -o scripts/chord-baseline-probe.exe scripts/chord-baseline-probe.rs
//   ./scripts/chord-baseline-probe.exe
//
// 读输出：`=>` 左边是最佳解释，`alts` 是备选。带 `warn:` 的表示内核自己也不确定。

#[path = "../src-tauri/src/note.rs"]
mod note;
#[path = "../src-tauri/src/key.rs"]
mod key;
#[path = "../src-tauri/src/chord.rs"]
mod chord;
#[path = "../src-tauri/src/progression.rs"]
mod progression;
#[path = "../src-tauri/src/scoring.rs"]
mod scoring;

use chord::{interpret, interpretations, Chord};
use key::{Key, Mode};
use note::Note;
use scoring::evaluate_all;

fn ns(names: &[&str]) -> Vec<Note> {
    names.iter().map(|n| Note::parse(n).unwrap()).collect()
}

fn report(label: &str, names: &[&str]) {
    let set = ns(names);
    let best = interpret(&set);
    let alts: Vec<String> = interpretations(&set).iter().map(|c| c.name()).collect();
    println!(
        "{:<26} {:<42} => {:<14} kind={:<9} alts={:?}{}",
        label,
        names.join(" "),
        best.name(),
        best.kind.id(),
        alts,
        best.warning
            .as_ref()
            .map(|w| format!(" | warn: {}", w))
            .unwrap_or_default()
    );
}

fn report_key(label: &str, seq: &[Vec<&str>]) {
    let interps: Vec<Vec<Chord>> = seq.iter().map(|c| interpretations(&ns(c))).collect();
    let evals = evaluate_all(&Key::all(), &interps);
    println!("\n{}  ({} 个和弦)", label, seq.len());
    for ev in evals.iter().take(2) {
        let parts: Vec<String> = ev
            .choices
            .iter()
            .map(|c| format!("{}[{}]", c.chord.name(), c.role.roman))
            .collect();
        println!(
            "   {:<10} score={:.2} fit={:.2} : {}",
            ev.key.full_name(),
            ev.score,
            ev.fit,
            parts.join("  ")
        );
    }
}

fn main() {
    println!("===== A. 单组音识别：模板覆盖度 =====");
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("大三", vec!["C", "E", "G"]),
        ("属七", vec!["G", "B", "D", "F"]),
        ("小七", vec!["C", "Eb", "G", "Bb"]),
        ("半减七", vec!["C", "Eb", "Gb", "Bb"]),
        ("减七", vec!["C", "Eb", "Gb", "A"]),
        ("增三", vec!["C", "E", "G#"]),
        ("六和弦", vec!["C", "E", "G", "A"]),
        ("加九", vec!["C", "E", "G", "D"]),
        ("大九", vec!["C", "E", "G", "B", "D"]),
        ("13#11", vec!["C", "E", "G", "Bb", "F#", "A"]),
        ("7b9b13", vec!["C", "E", "G", "Bb", "Db", "Ab"]),
        ("7alt(简化)", vec!["C", "E", "G", "Bb", "Db", "Eb"]),
        ("7#9", vec!["C", "E", "G", "Bb", "Eb"]),
        ("大七升十一", vec!["C", "E", "G", "B", "F#"]),
        ("无根音 C9", vec!["E", "G", "Bb", "D"]),
        ("无根音 Cmaj7", vec!["E", "G", "B"]),
        ("shell 3-7", vec!["E", "Bb"]),
        ("意大利增六", vec!["Ab", "C", "F#"]),
        ("德国增六", vec!["Ab", "C", "Eb", "F#"]),
        ("四度叠置", vec!["C", "F", "Bb", "Eb"]),
        ("So What", vec!["D", "G", "C", "F", "A"]),
        ("C/D 斜杠", vec!["D", "C", "E", "G"]),
        ("D/C 上层结构", vec!["C", "E", "Bb", "D", "F#", "A"]),
        ("无三音", vec!["C", "G", "Bb"]),
        ("三全音", vec!["C", "F#"]),
    ];
    for (label, names) in &cases {
        report(label, names);
    }

    println!("\n===== B. 一个外音混进来会怎样（装饰音敏感性） =====");
    let noisy: Vec<(&str, Vec<&str>)> = vec![
        ("Cmaj7（干净）", vec!["C", "E", "G", "B"]),
        ("+ 经过音 D", vec!["C", "E", "G", "B", "D"]),
        ("+ 经过音 F", vec!["C", "E", "G", "B", "F"]),
        ("C（干净）", vec!["C", "E", "G"]),
        ("+ 辅助音 F#", vec!["C", "E", "G", "F#"]),
        ("+ 倚音 Db", vec!["C", "E", "G", "Db"]),
        ("Am（干净）", vec!["A", "C", "E"]),
        ("+ 低音 G（经过）", vec!["G", "A", "C", "E"]),
        ("琶音交叠 C + Am", vec!["C", "E", "G", "A", "C", "E"]),
        ("两个和弦叠在一起", vec!["C", "E", "G", "D", "F#", "A"]),
    ];
    for (label, names) in &noisy {
        report(label, names);
    }

    println!("\n===== C. 序列级：调性判定与罗马数字 =====");
    let seqs: Vec<(&str, Vec<Vec<&str>>)> = vec![
        (
            "流行 I-vi-ii-V",
            vec![
                vec!["C", "E", "G"],
                vec!["A", "C", "E"],
                vec!["D", "F", "A"],
                vec!["G", "B", "D"],
            ],
        ),
        (
            "副属 V7/ii",
            vec![
                vec!["C", "E", "G"],
                vec!["A", "C#", "E", "G"],
                vec!["D", "F", "A", "C"],
                vec!["G", "B", "D", "F"],
            ],
        ),
        (
            "同主音借用 bVI",
            vec![
                vec!["C", "E", "G"],
                vec!["Ab", "C", "Eb"],
                vec!["G", "B", "D"],
            ],
        ),
        (
            "无根音 shell ii-V-I",
            vec![
                vec!["F", "C", "Eb"],
                vec!["F", "Bb", "D"],
                vec!["E", "Bb", "D"],
            ],
        ),
        (
            "一小节两个和弦（C -> Cmaj7）",
            vec![vec!["C", "E", "G"], vec!["C", "E", "G", "B"]],
        ),
        (
            "装饰音把 C 变成 Cadd9",
            vec![vec!["C", "E", "G", "D"], vec!["C", "E", "G"]],
        ),
    ];
    for (label, seq) in &seqs {
        report_key(label, seq);
    }

    println!("\n===== D. 单和弦在 C 大调里的角色（离调解释） =====");
    let c_major = Key::from_name("C", Mode::Major);
    for names in [
        vec!["A", "C#", "E", "G"],
        vec!["Ab", "C", "Eb"],
        vec!["Ab", "C", "Eb", "F#"],
        vec!["Db", "F", "Ab"],
        vec!["E", "G", "Bb", "D"],
    ] {
        let set = ns(&names);
        let ch = interpret(&set);
        let role = progression::analyze_role(&c_major, &ch);
        println!(
            "C 大调里 {:<20} => {:<14} roman={:<10} cat={:<20} {}",
            names.join(" "),
            ch.name(),
            role.roman,
            role.category.id(),
            role.explanation
        );
    }
}
