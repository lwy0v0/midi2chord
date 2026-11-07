//! MIDI 分析的对外入口：字节 → 解析 → 提取 → 可序列化的结果。
//!
//! 职责边界：只做「字节进、结构出」和错误归一化，不含算法。
//! 算法在 `extract`（分析和弦与调性）、`midi`（解析文件格式）。

use crate::analysis::AnalyzeError;
use crate::extract::{self, LoopAnalysis};
use crate::midi::Smf;

fn err(code: &str, message: impl Into<String>) -> AnalyzeError {
    AnalyzeError {
        code: code.to_string(),
        message: message.into(),
        row: None,
        col: None,
    }
}

/// 分析一段 MIDI 字节。测试与批处理直接用这个，不经过 Tauri。
pub fn analyze_bytes(bytes: &[u8]) -> Result<LoopAnalysis, AnalyzeError> {
    if bytes.is_empty() {
        return Err(err("emptyFile", "文件是空的"));
    }
    let smf = Smf::parse(bytes).map_err(|e| err("midiParse", e))?;
    if smf.note_count() == 0 {
        return Err(err(
            "noNotes",
            "文件里没有音符（可能只有空轨或只有控制事件）",
        ));
    }
    Ok(extract::analyze(&smf))
}

/// Tauri 命令：把整份 MIDI 文件交给前端解析结果。
///
/// 前端把文件读成字节数组传进来（不经过文件系统插件，也不用改权限配置）。
#[tauri::command]
pub fn analyze_midi(bytes: Vec<u8>) -> Result<LoopAnalysis, AnalyzeError> {
    analyze_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_rejected() {
        let e = analyze_bytes(&[]).unwrap_err();
        assert_eq!(e.code, "emptyFile");
    }

    #[test]
    fn non_midi_is_rejected() {
        let e = analyze_bytes(b"RIFFxxxx").unwrap_err();
        assert_eq!(e.code, "midiParse");
    }

    /// 最小可用文件：一个 C 大三和弦（C4 E4 G4），**同起同落**，一拍长。
    fn c_major_smf() -> Vec<u8> {
        let mut trk: Vec<u8> = Vec::new();
        for pitch in [60u8, 64, 67] {
            trk.extend_from_slice(&[0x00, 0x90, pitch, 100]);
        }
        // 第一个 note-off 前走 96 tick（一拍），其余三个 delta 为 0——
        // 三个音必须同时结束；否则音在"依次消失"，分段自然会被切开（那是正确行为）。
        trk.extend_from_slice(&[0x60, 0x80, 60, 0]);
        trk.extend_from_slice(&[0x00, 0x80, 64, 0]);
        trk.extend_from_slice(&[0x00, 0x80, 67, 0]);
        trk.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&96u16.to_be_bytes());
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(trk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trk);
        bytes
    }

    #[test]
    fn recognises_a_simple_triad() {
        let a = analyze_bytes(&c_major_smf()).expect("应能分析");
        assert!(!a.single_line, "三音同响应判为有和声层");
        assert_eq!(
            a.segments.len(),
            1,
            "一拍内只该有一个和弦段，实际 = {:?}",
            a.segments
                .iter()
                .map(|s| (s.chord.symbol.clone(), s.len_beats))
                .collect::<Vec<_>>()
        );
        let s = &a.segments[0];
        assert_eq!(s.chord.symbol, "C", "实际 {}", s.chord.symbol);
        assert_eq!(s.voicing.len(), 3, "C 大三和弦应有三个代表音");
        assert_eq!(a.notes.len(), 3);
        assert!(a.notes.iter().all(|n| n.role.is_chord_tone()));
    }

    #[test]
    fn single_note_is_reported_as_single_line() {
        // 只弹一个音，不该硬凑和弦
        let mut trk: Vec<u8> = Vec::new();
        trk.extend_from_slice(&[0x00, 0x90, 60, 100]);
        trk.extend_from_slice(&[0x60, 0x80, 60, 0]);
        trk.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&96u16.to_be_bytes());
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(trk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trk);

        let a = analyze_bytes(&bytes).expect("应能分析");
        assert!(a.single_line, "单声部应被识别为单声部");
        assert!(a.segments.is_empty(), "单声部不该输出和弦段");
    }
}
