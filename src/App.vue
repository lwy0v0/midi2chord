<script setup lang="ts">
/**
 * 应用外壳：主题与中文语言配置 + 两个入口（和弦序列 / MIDI 文件）。
 * 业务状态全部来自 composables，本组件只做编排与布局。
 */
import { computed, onMounted, ref, watch } from "vue";
import { darkTheme, dateZhCN, zhCN } from "naive-ui";
import AnalysisPanel from "./components/AnalysisPanel.vue";
import ChordInputGrid from "./components/ChordInputGrid.vue";
import ChordSizeControls from "./components/ChordSizeControls.vue";
import MidiAnalysisPanel from "./components/MidiAnalysisPanel.vue";
import { useChordAnalysis } from "./composables/useChordAnalysis";
import { useChordGrid } from "./composables/useChordGrid";
import { useMidiAnalysis } from "./composables/useMidiAnalysis";

const {
  rows,
  cols,
  chords,
  errors,
  duplicateRows,
  setCell,
  setErrors,
  validate,
  reset,
} = useChordGrid();

const { result, error, cellErrors, loading, run, clear } = useChordAnalysis();

const {
  result: midiResult,
  error: midiError,
  loading: midiLoading,
  fileName: midiFileName,
  run: runMidi,
  clear: clearMidi,
} = useMidiAnalysis();

/** 当前入口：keyboard = 键盘输入和弦；midi = 读 MIDI 文件。 */
const mode = ref<"keyboard" | "midi">("midi");

const THEME_KEY = "chords.theme";
const isDark = ref(false);

onMounted(() => {
  isDark.value = localStorage.getItem(THEME_KEY) === "dark";
});

watch(isDark, (value) => {
  localStorage.setItem(THEME_KEY, value ? "dark" : "light");
});

const theme = computed(() => (isDark.value ? darkTheme : null));

// 后端返回的格子级错误同步到网格高亮
watch(cellErrors, (list) => setErrors(list));

function onUpdateCell(payload: { row: number; col: number; value: string }): void {
  setCell(payload.row, payload.col, payload.value);
}

async function onAnalyze(): Promise<void> {
  await run(chords.value, validate());
}

function onReset(): void {
  reset();
  clear();
}

function onPickMidi(file: File): void {
  void runMidi(file);
}
</script>

<template>
  <n-config-provider :theme="theme" :locale="zhCN" :date-locale="dateZhCN">
    <n-message-provider>
      <n-global-style />
      <main class="app">
        <header class="app__header">
          <div>
            <h1 class="app__title">🎵 和弦与走向分析</h1>
            <p class="app__subtitle">
              键盘输入一串和弦，或读进一个 MIDI loop——提取调性、和弦进行与和声节奏
            </p>
          </div>
          <n-flex align="center" :size="8">
            <n-text depth="3" class="app__theme-label">暗色</n-text>
            <n-switch v-model:value="isDark" size="small" />
          </n-flex>
        </header>

        <n-tabs v-model:value="mode" type="line" animated class="app__tabs">
          <n-tab-pane name="midi" tab="🎹 MIDI 文件">
            <MidiAnalysisPanel
              :result="midiResult"
              :error="midiError"
              :loading="midiLoading"
              :file-name="midiFileName"
              @pick="onPickMidi"
              @clear="clearMidi"
            />
          </n-tab-pane>

          <n-tab-pane name="keyboard" tab="⌨️ 手动输入和弦">
            <n-card size="small" class="app__panel">
              <ChordSizeControls
                :rows="rows"
                :cols="cols"
                @update:rows="rows = $event"
                @update:cols="cols = $event"
              />
              <n-divider class="app__divider" />
              <ChordInputGrid
                :chords="chords"
                :errors="errors"
                :duplicate-rows="duplicateRows"
                @update-cell="onUpdateCell"
              />
              <n-flex class="app__actions" :size="12">
                <n-button type="primary" :loading="loading" @click="onAnalyze">
                  {{ loading ? "分析中…" : "🔍 分析走向" }}
                </n-button>
                <n-button ghost type="warning" @click="onReset">重置</n-button>
              </n-flex>
            </n-card>

            <AnalysisPanel :result="result" :error="error" />
          </n-tab-pane>
        </n-tabs>
      </main>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
.app {
  max-width: 1080px;
  margin: 0 auto;
  padding: 20px 20px 48px;
}

.app__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}

.app__title {
  margin: 0;
  font-size: 24px;
}

.app__subtitle {
  margin: 6px 0 0;
  font-size: 13px;
  opacity: 0.62;
}

.app__theme-label {
  font-size: 12px;
}

.app__panel {
  border-radius: 14px;
}

.app__tabs {
  margin-bottom: 12px;
}

.app__divider {
  margin: 12px 0;
}

.app__actions {
  margin-top: 16px;
}
</style>
