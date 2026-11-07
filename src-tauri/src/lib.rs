//! 和弦与和弦走向分析库。
//!
//! # 模块划分（单向依赖，无环）
//! - [`note`]：音高类、音名解析与拼写 —— 最底层，不依赖任何模块，且不 panic
//! - [`key`]：24 个候选调、音阶掩码、按调号拼写 —— 依赖 `note`
//! - [`chord`]：和弦识别（模板、省略音、转位、多解）—— 依赖 `note`、`key`
//! - [`progression`]：调内角色、罗马数字、终止式、走向模板、调外解释 —— 依赖 `key`、`chord`
//! - [`scoring`]：候选调打分与排序、多解择优 —— 依赖 `key`、`chord`、`progression`
//! - [`midi`]：标准 MIDI 文件解析（字节 → 音符/速度/拍号）—— 只依赖标准库
//! - [`extract`]：从一段 loop 里提取调性与和弦进行 —— 依赖以上全部
//! - [`analysis`]：键盘输入（一组一组和弦名）的对外入口与 JSON 契约
//! - [`midi_analysis`]：MIDI 输入的对外入口与 Tauri 命令
//!
//! 两条入口共用同一套底层：键盘输入走 `chord` + `scoring`，
//! MIDI 输入走 `midi` + `extract`，互不影响。

pub mod analysis;
pub mod chord;
pub mod extract;
pub mod key;
pub mod midi;
pub mod midi_analysis;
pub mod note;
pub mod progression;
pub mod scoring;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            analysis::analyze,
            midi_analysis::analyze_midi
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
