<script setup lang="ts">
/**
 * MIDI 分析面板 —— **只做编排**。
 *
 * 具体展示拆在 `components/midi/` 下：
 * - `MidiDropZone`  拿文件
 * - `KeySummary`    调性与基本事实
 * - `NoteLegend`    图例
 * - `SegmentTable`  和弦段列表
 * - `SegmentDetail` 选中段详情
 * 卷帘窗是独立的通用组件 `PianoRoll`，通过 `adapters/roll.ts` 对接。
 */
import { computed, ref, watch } from "vue";
import PianoRoll from "./PianoRoll.vue";
import MidiDropZone from "./midi/MidiDropZone.vue";
import KeySummary from "./midi/KeySummary.vue";
import KeyOptions from "./midi/KeyOptions.vue";
import NoteLegend from "./midi/NoteLegend.vue";
import SegmentTable from "./midi/SegmentTable.vue";
import SegmentDetail from "./midi/SegmentDetail.vue";
import {
  noteTooltip,
  presentRoles,
  rollColors,
  toRollNotes,
  toRollSegments,
} from "../adapters/roll";
import { NOTE_ROLE_COLORS, NOTE_ROLE_LABELS } from "../constants/music";
import type { MidiAnalysisResult } from "../types";
import type { AnalysisError } from "../api/analyze";

const props = defineProps<{
  result: MidiAnalysisResult | null;
  error: AnalysisError | null;
  loading: boolean;
  fileName: string;
}>();

const emit = defineEmits<{
  (e: "pick", file: File): void;
  (e: "clear"): void;
}>();

/** 当前选中的和弦段（状态放在编排层，向下传）。 */
const selected = ref<number | null>(null);
const copied = ref(false);
/** 用户挑的候选调下标（0 = 后端默认那个）。 */
const pickedKey = ref(0);

// 换了文件就清掉选中
watch(
  () => props.result,
  () => {
    selected.value = null;
    copied.value = false;
    pickedKey.value = 0;
  },
);

/** 用户选中的候选调；没候选时是 null。 */
const activeOption = computed(() => {
  const opts = props.result?.keyOptions ?? [];
  if (opts.length === 0) return null;
  return opts[Math.min(pickedKey.value, opts.length - 1)] ?? null;
});

/** 罗马数字跟着选中的调走（和弦名不变，只是编号变了）。 */
const activeRomans = computed(() => activeOption.value?.romans ?? props.result?.romans ?? []);

const rollNotes = computed(() => toRollNotes(props.result));
const rollSegments = computed(() => toRollSegments(props.result));
const colors = rollColors();

const legendItems = computed(() =>
  presentRoles(props.result).map((k) => ({
    key: k,
    label: NOTE_ROLE_LABELS[k],
    color: NOTE_ROLE_COLORS[k],
  })),
);

const selectedSegment = computed(() => {
  const r = props.result;
  if (!r || selected.value === null) return null;
  return r.segments[selected.value] ?? null;
});

/** 总长度：取段结束位置与小节总数的较大者，避免末尾被截断。 */
const totalBeats = computed(() => {
  const r = props.result;
  if (!r) return 0;
  let end = r.bars * r.beatsPerBar;
  for (const s of r.segments) end = Math.max(end, s.startBeat + s.lenBeats);
  return end;
});

async function copyProgression(): Promise<void> {
  const r = props.result;
  if (!r) return;
  try {
    await navigator.clipboard.writeText(r.progression.join(" - "));
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1500);
  } catch {
    copied.value = false;
  }
}
</script>

<template>
  <n-card size="small" class="midi">
    <div class="midi__head">
      <div>
        <h2 class="midi__title">MIDI 文件 · 提取调性与和弦</h2>
        <p class="midi__hint">
          把一个 loop（.mid / .midi）拖进来，自动识别调性、和弦进行与和声节奏
        </p>
      </div>
      <n-flex v-if="result" :size="8">
        <n-button size="small" ghost @click="copyProgression">
          {{ copied ? "已复制" : "复制进行" }}
        </n-button>
        <n-button size="small" ghost type="warning" @click="emit('clear')">换一个</n-button>
      </n-flex>
    </div>

    <n-alert v-if="error" type="error" :bordered="false" class="midi__alert">
      {{ error.message }}
    </n-alert>

    <!-- 还没有结果：显示投放区 -->
    <MidiDropZone
      v-if="!result"
      :loading="loading"
      :file-name="fileName"
      @pick="(f) => emit('pick', f)"
    />

    <!-- 有结果 -->
    <template v-else>
      <KeySummary :result="result" :file-name="fileName" :option="activeOption" />

      <n-alert v-if="result.singleLine" type="warning" :bordered="false" class="midi__alert">
        这个文件只有单声部（旋律或贝斯），没有和弦层——只给了调性，没有硬凑和弦名。
      </n-alert>
      <div v-for="(w, i) in result.warnings" :key="i" class="midi__warn">⚠ {{ w }}</div>

      <!-- 候选调：把可能性摊开，让用户自己挑 -->
      <KeyOptions
        :options="result.keyOptions"
        :picked="pickedKey"
        @pick="(i) => (pickedKey = i)"
      />

      <template v-if="!result.singleLine">
        <div class="midi__section">
          钢琴卷帘窗
          <span class="midi__section-sub">点击和弦段可查看详情</span>
        </div>
        <NoteLegend :items="legendItems" />
        <PianoRoll
          :notes="rollNotes"
          :segments="rollSegments"
          :colors="colors"
          :beats-per-bar="result.beatsPerBar"
          :total-beats="totalBeats"
          :selected-segment="selected"
          :note-label="noteTooltip"
          :max-height="380"
          @segment-click="(_s, i) => (selected = i)"
        />

        <SegmentDetail :segment="selectedSegment" />

        <div class="midi__section">和弦段（{{ result.segments.length }} 段）</div>
        <SegmentTable
          :segments="result.segments"
          :romans="activeRomans"
          :selected="selected"
          @select="(i) => (selected = i)"
        />
      </template>
    </template>
  </n-card>
</template>

<style scoped>
.midi__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 12px;
}

.midi__title {
  margin: 0;
  font-size: 16px;
}

.midi__hint {
  margin: 4px 0 0;
  font-size: 12px;
  opacity: 0.6;
}

.midi__alert {
  margin-bottom: 12px;
}

.midi__warn {
  font-size: 12px;
  color: #f59e0b;
  margin-bottom: 6px;
}

.midi__section {
  margin: 16px 0 8px;
  font-size: 13px;
  font-weight: 600;
  opacity: 0.85;
}

.midi__section-sub {
  margin-left: 8px;
  font-weight: 400;
  font-size: 12px;
  opacity: 0.5;
}
</style>
