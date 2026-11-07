/**
 * 后端调用层：唯一与 Tauri `invoke` 打交道的地方。
 *
 * 所有失败都被归一化成 {@link AnalysisError}，前端其它模块只处理这一种错误形态。
 */

import { invoke } from "@tauri-apps/api/core";
import type { AnalysisResult, AnalyzeError } from "../types";

/** 归一化后的分析错误。 */
export class AnalysisError extends Error {
  readonly code: string;
  readonly row: number | null;
  readonly col: number | null;

  constructor(payload: AnalyzeError) {
    super(payload.message);
    this.name = "AnalysisError";
    this.code = payload.code;
    this.row = payload.row;
    this.col = payload.col;
  }
}

/** 是否运行在 Tauri 窗口内（浏览器里 `invoke` 不可用）。 */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 把任意后端返回值归一化为 {@link AnalyzeError}。 */
export function normalizeError(raw: unknown): AnalyzeError {
  if (raw && typeof raw === "object") {
    const payload = raw as Partial<AnalyzeError>;
    if (typeof payload.message === "string") {
      return {
        code: typeof payload.code === "string" ? payload.code : "unknown",
        message: payload.message,
        row: typeof payload.row === "number" ? payload.row : null,
        col: typeof payload.col === "number" ? payload.col : null,
      };
    }
  }
  return {
    code: "unknown",
    message: typeof raw === "string" ? raw : String(raw ?? "未知错误"),
    row: null,
    col: null,
  };
}

/**
 * 分析和弦走向。
 *
 * @param chords 二维音名数组，每行是一个和弦；行内第一个音按约定视为低音。
 */
export async function analyzeChords(chords: string[][]): Promise<AnalysisResult> {
  if (!isTauriRuntime()) {
    throw new AnalysisError({
      code: "notTauri",
      message: "当前不在桌面应用内运行，请使用 npm run tauri dev 启动",
      row: null,
      col: null,
    });
  }
  try {
    return await invoke<AnalysisResult>("analyze", { chordSequence: chords });
  } catch (raw) {
    throw new AnalysisError(normalizeError(raw));
  }
}
