/**
 * MIDI 分析的调用层。
 *
 * 单一职责：**调后端、归一化错误**。
 * 文件不在前端解析——只把字节交给 Rust 侧，算法全在 `extract.rs`。
 *
 * 常量与展示格式化在 `constants/music.ts` 与 `adapters/roll.ts`，
 * 这里只管「怎么发请求、怎么报错」。
 */

import { invoke } from "@tauri-apps/api/core";
import type { MidiAnalysisResult } from "../types";
import { AnalysisError, isTauriRuntime, normalizeError } from "./analyze";

/**
 * 分析一段 MIDI 字节。
 *
 * 注意：目前用 `Array.from` 传数组，几 MB 的文件会有一次性拷贝开销。
 * 如果以后要处理很大的文件，改成传文件路径（需要 file system 插件）会更快。
 */
export async function analyzeMidi(bytes: Uint8Array): Promise<MidiAnalysisResult> {
  if (!isTauriRuntime()) {
    throw new AnalysisError({
      code: "notTauri",
      message: "当前不在桌面应用内运行，请使用 npm run tauri dev 启动",
      row: null,
      col: null,
    });
  }
  try {
    return await invoke<MidiAnalysisResult>("analyze_midi", {
      bytes: Array.from(bytes),
    });
  } catch (raw) {
    throw new AnalysisError(normalizeError(raw));
  }
}

/** 直接分析一个 `File`（拖拽或文件选择框拿到的对象）。 */
export async function analyzeMidiFile(file: File): Promise<MidiAnalysisResult> {
  const buffer = await file.arrayBuffer();
  return analyzeMidi(new Uint8Array(buffer));
}

/** 按扩展名粗略判断是不是 MIDI 文件。 */
export function looksLikeMidi(file: File): boolean {
  return /\.(mid|midi)$/i.test(file.name);
}
