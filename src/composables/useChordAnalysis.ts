/**
 * 分析流程编排：本地校验 → 调用后端 → 归一化结果与错误。
 *
 * 错误分两类对外暴露：
 * - {@link error}：整条消息（用于顶部提示）
 * - {@link cellErrors}：定位到具体格子（用于输入网格高亮）
 */

import { ref, type Ref } from "vue";
import { AnalysisError, analyzeChords } from "../api/analyze";
import type { AnalysisResult, AnalyzeError, CellError } from "../types";

export interface ChordAnalysis {
  result: Ref<AnalysisResult | null>;
  error: Ref<AnalyzeError | null>;
  cellErrors: Ref<CellError[]>;
  loading: Ref<boolean>;
  /** 当前结果对应的输入快照，便于判断是否需要重新分析。 */
  analyzedInput: Ref<string>;
  run: (chords: string[][], sizeErrors: CellError[]) => Promise<void>;
  clear: () => void;
}

export function useChordAnalysis(): ChordAnalysis {
  const result = ref<AnalysisResult | null>(null);
  const error = ref<AnalyzeError | null>(null);
  const cellErrors = ref<CellError[]>([]);
  const loading = ref(false);
  const analyzedInput = ref("");

  function clear(): void {
    result.value = null;
    error.value = null;
    cellErrors.value = [];
    analyzedInput.value = "";
  }

  async function run(chords: string[][], sizeErrors: CellError[]): Promise<void> {
    error.value = null;
    cellErrors.value = [];

    if (sizeErrors.length > 0) {
      cellErrors.value = sizeErrors;
      error.value = {
        code: "localValidation",
        message: sizeErrors[0].message,
        row: sizeErrors[0].row,
        col: sizeErrors[0].col,
      };
      result.value = null;
      return;
    }

    loading.value = true;
    analyzedInput.value = JSON.stringify(chords);
    try {
      result.value = await analyzeChords(chords);
    } catch (raw) {
      const normalized = raw instanceof AnalysisError ? raw : new AnalysisError({
        code: "unknown",
        message: String(raw),
        row: null,
        col: null,
      });
      error.value = {
        code: normalized.code,
        message: normalized.message,
        row: normalized.row,
        col: normalized.col,
      };
      cellErrors.value =
        normalized.row === null
          ? []
          : [{ row: normalized.row, col: normalized.col ?? 0, message: normalized.message }];
      result.value = null;
    } finally {
      loading.value = false;
    }
  }

  return { result, error, cellErrors, loading, analyzedInput, run, clear };
}
