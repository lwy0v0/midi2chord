/**
 * MIDI 分析的状态与流程。
 *
 * 与 `useChordAnalysis` 同构：负责调用、错误归一化、加载状态；
 * 不碰界面，界面组件只消费这里的 ref。
 */

import { ref, shallowRef } from "vue";
import type { MidiAnalysisResult } from "../types";
import { AnalysisError } from "../api/analyze";
import { analyzeMidiFile, looksLikeMidi } from "../api/midi";

export function useMidiAnalysis() {
  /** 分析结果；用 shallowRef，因为只有整体替换、不做深层响应。 */
  const result = shallowRef<MidiAnalysisResult | null>(null);
  const error = ref<AnalysisError | null>(null);
  const loading = ref(false);
  /** 当前分析的文件名，用于显示。 */
  const fileName = ref("");

  async function run(file: File): Promise<void> {
    error.value = null;
    if (!looksLikeMidi(file)) {
      error.value = new AnalysisError({
        code: "notMidi",
        message: `不是 MIDI 文件：${file.name}（需要 .mid 或 .midi）`,
        row: null,
        col: null,
      });
      return;
    }
    loading.value = true;
    fileName.value = file.name;
    try {
      result.value = await analyzeMidiFile(file);
    } catch (e) {
      result.value = null;
      error.value =
        e instanceof AnalysisError
          ? e
          : new AnalysisError({
              code: "unknown",
              message: String(e),
              row: null,
              col: null,
            });
    } finally {
      loading.value = false;
    }
  }

  function clear(): void {
    result.value = null;
    error.value = null;
    fileName.value = "";
  }

  return { result, error, loading, fileName, run, clear };
}
