<script setup lang="ts">
/** 单个候选调卡片：调名、置信度、罗马数字进行、终止式、经典走向与理由。 */
import { computed } from "vue";
import type { KeyCandidate } from "../types";
import { confidenceType, keyModeLabel, signatureLabel } from "../utils/format";
import ChordChip from "./ChordChip.vue";

const props = defineProps<{
  candidate: KeyCandidate;
  /** 是否为首选（排名第一）。 */
  best: boolean;
  /** 首选候选的排序分，用于计算相对位置。 */
  bestScore: number;
}>();

const romanLine = computed(() => props.candidate.romans.join("  "));

const title = computed(
  () => props.candidate.displayName || `${props.candidate.key} ${keyModeLabel(props.candidate)}`,
);

/** 相对首选的排序分百分比，用来体现候选之间的差距。 */
const relative = computed(() => {
  if (props.best || props.bestScore <= 0) {
    return null;
  }
  return Math.max(1, Math.round((props.candidate.score / props.bestScore) * 100));
});
</script>

<template>
  <n-card
    size="small"
    :bordered="true"
    class="key-card"
    :class="{ 'key-card--best': best }"
  >
    <div class="key-card__head">
      <n-flex align="center" :size="10" wrap>
        <n-tag :type="best ? 'success' : 'default'" :bordered="false" size="large">
          {{ title }}
        </n-tag>
        <n-text depth="3" class="key-card__meta">
          匹配和弦 {{ candidate.matched }}/{{ candidate.total }}
          <template v-if="candidate.explained > candidate.matched">
            （可解释 {{ candidate.explained }}/{{ candidate.total }}）
          </template>
        </n-text>
        <n-tag :type="confidenceType(candidate.confidence)" size="small" :bordered="false">
          贴合度 {{ candidate.confidence }}%
        </n-tag>
        <n-text v-if="relative !== null" depth="3" class="key-card__meta">
          相对首选 {{ relative }}%
        </n-text>
        <n-text depth="3" class="key-card__meta">{{ signatureLabel(candidate.accidentals) }}</n-text>
      </n-flex>
    </div>

    <div class="key-card__romans">
      <n-text strong depth="2">进行：</n-text>
      <code class="key-card__roman-line">{{ romanLine }}</code>
    </div>

    <n-flex :size="8" wrap class="key-card__chords">
      <ChordChip v-for="(chord, index) in candidate.chords" :key="index" :chord="chord" />
    </n-flex>

    <div v-if="candidate.cadences.length" class="key-card__section">
      <n-text strong depth="2">终止式：</n-text>
      <n-tag
        v-for="(cadence, index) in candidate.cadences"
        :key="`cadence-${index}`"
        size="small"
        :bordered="false"
        type="warning"
      >
        {{ cadence.formula }} · {{ cadence.description }}
      </n-tag>
    </div>

    <div v-if="candidate.templates.length" class="key-card__section">
      <n-text strong depth="2">经典走向：</n-text>
      <n-tag
        v-for="(template, index) in candidate.templates"
        :key="`template-${index}`"
        size="small"
        :bordered="false"
        type="info"
      >
        {{ template.name }}
      </n-tag>
    </div>

    <div class="key-card__section">
      <n-text strong depth="2">判断依据：</n-text>
      <ul class="key-card__reasons">
        <li v-for="(reason, index) in candidate.reasons" :key="`reason-${index}`">{{ reason }}</li>
      </ul>
    </div>
  </n-card>
</template>

<style scoped>
.key-card {
  border-radius: 12px;
}

.key-card--best {
  border: 2px solid #18a058;
}

.key-card__head {
  margin-bottom: 8px;
}

.key-card__meta {
  font-size: 12px;
}

.key-card__romans {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 8px;
  font-size: 13px;
}

.key-card__roman-line {
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 14px;
  letter-spacing: 0.5px;
}

.key-card__chords {
  margin-bottom: 8px;
}

.key-card__section {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  font-size: 12px;
}

.key-card__reasons {
  margin: 0;
  padding-left: 18px;
  line-height: 1.6;
}
</style>
