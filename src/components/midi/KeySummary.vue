<script setup lang="ts">
/**
 * 调性与基本事实摘要。
 *
 * 只读展示：拿到已算好的结果，不重新计算任何东西。
 * 如果用户在候选列表里换了调，传 `option` 进来覆盖默认显示。
 */
import { computed } from "vue";
import type { KeyOption, MidiAnalysisResult } from "../../types";
import { harmonicRhythmHint, progressionLine } from "../../adapters/roll";

const props = defineProps<{
  result: MidiAnalysisResult;
  fileName: string;
  /** 用户选中的候选调；null = 用后端默认那个。 */
  option?: KeyOption | null;
}>();

/** 当前显示的调名（用户选了就用选的）。 */
const keyName = computed(() => props.option?.name ?? props.result.keyName);

/** 当前显示的进行（跟着调走——进行本身不变，但取的是该候选算出来的那份）。 */
const progression = computed(() =>
  props.option ? props.option.progression.join(" → ") : progressionLine(props.result),
);

const rhythm = computed(() => harmonicRhythmHint(props.result));

/** 除当前显示外的其他候选，最多两个。 */
const altKeys = computed(() =>
  props.result.keyOptions
    .filter((o) => o.name !== keyName.value)
    .slice(0, 2)
    .map((o) => o.name),
);
</script>

<template>
  <div class="summary">
    <div class="summary__key">
      <div class="summary__key-name">{{ keyName }}</div>
      <div class="summary__key-meta">
        <template v-if="option">
          {{ option.startsOnTonic ? "开头就是主和弦" : option.endsOnTonic ? "结尾是主和弦" : `主和弦占 ${option.tonicShare}%` }}
        </template>
        <template v-else>
          置信度 {{ result.keyConfidence }}%
          <span v-if="result.keyNote" class="summary__warn">· {{ result.keyNote }}</span>
        </template>
      </div>
      <div v-if="altKeys.length" class="summary__key-alt">
        也可能是
        <span v-for="(k, i) in altKeys" :key="i" class="summary__alt">{{ k }}</span>
      </div>
    </div>

    <div class="summary__facts">
      <div>
        <span class="summary__k">进行</span>
        <span class="summary__progress">{{ progression || "—" }}</span>
      </div>
      <div>
        <span class="summary__k">和声节奏</span>{{ rhythm }}
      </div>
      <div>
        <span class="summary__k">文件</span>
        {{ fileName }} · {{ result.bars }} 小节 · {{ result.bpm }} 拍/分
        <span v-if="!result.bpmExplicit" class="summary__warn">（文件未写速度，按默认 120）</span>
      </div>
      <div v-if="result.pickupBeats > 0">
        <span class="summary__k">弱起</span>第一个音在第 {{ result.pickupBeats }} 拍
      </div>
    </div>
  </div>
</template>

<style scoped>
.summary {
  display: flex;
  gap: 20px;
  flex-wrap: wrap;
  margin-bottom: 12px;
}

.summary__key {
  min-width: 168px;
  padding: 10px 14px;
  border-radius: 10px;
  background: rgba(59, 130, 246, 0.12);
}

.summary__key-name {
  font-size: 20px;
  font-weight: 600;
}

.summary__key-meta {
  font-size: 12px;
  opacity: 0.75;
  margin-top: 2px;
}

.summary__key-alt {
  font-size: 12px;
  opacity: 0.6;
  margin-top: 4px;
}

.summary__alt {
  margin-right: 6px;
}

.summary__facts {
  flex: 1;
  min-width: 260px;
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 13px;
  justify-content: center;
}

.summary__k {
  display: inline-block;
  min-width: 56px;
  opacity: 0.55;
}

.summary__progress {
  font-weight: 600;
}

.summary__warn {
  color: #f59e0b;
}
</style>
