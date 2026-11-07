<script setup lang="ts">
/** 单个和弦的小卡片：显示调内拼写的记号、罗马数字与功能，悬浮查看解释。 */
import { computed } from "vue";
import type { ChordWithLevel } from "../types";
import { categoryLabel, functionType } from "../utils/format";

const props = defineProps<{
  chord: ChordWithLevel;
}>();

const tagType = computed<"info" | "warning" | "error">(() => {
  if (props.chord.inScale) {
    return "info";
  }
  return props.chord.category === "chromatic" ? "error" : "warning";
});
</script>

<template>
  <n-popover trigger="hover" placement="top" :width="320" style="max-width: 90vw">
    <template #trigger>
      <n-tag :type="tagType" :bordered="false" size="small" class="chord-chip">
        <span class="chord-chip__symbol">{{ chord.symbol }}</span>
        <span class="chord-chip__roman">{{ chord.roman }}</span>
        <n-tag
          v-if="chord.function !== '—'"
          :type="functionType(chord.function)"
          size="tiny"
          :bordered="false"
          class="chord-chip__function"
        >
          {{ chord.function }}
        </n-tag>
      </n-tag>
    </template>

    <div class="chord-detail">
      <div class="chord-detail__title">
        <strong>{{ chord.symbol }}</strong>
        <span class="chord-detail__label">{{ chord.label }}</span>
      </div>
      <div>罗马数字：{{ chord.roman }}（{{ chord.functionLabel }}，{{ categoryLabel(chord.category) }}）</div>
      <div>实际音：{{ chord.notes.join(" ") }}</div>
      <div v-if="chord.missing.length">省略音：{{ chord.missing.join(" ") }}</div>
      <div v-if="chord.extras.length">结构外音：{{ chord.extras.join(" ") }}</div>
      <div class="chord-detail__explain">{{ chord.explanation }}</div>
      <div v-if="chord.alternatives.length">其它可能解释：{{ chord.alternatives.join("、") }}</div>
      <div v-if="chord.warning" class="chord-detail__warning">⚠ {{ chord.warning }}</div>
    </div>
  </n-popover>
</template>

<style scoped>
.chord-chip {
  cursor: default;
}

.chord-chip__symbol {
  font-weight: 600;
}

.chord-chip__roman {
  margin-left: 6px;
  opacity: 0.85;
}

.chord-chip__function {
  margin-left: 6px;
}

.chord-detail {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  line-height: 1.5;
}

.chord-detail__title {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 14px;
}

.chord-detail__label {
  opacity: 0.7;
}

.chord-detail__explain {
  padding-top: 4px;
  border-top: 1px solid rgba(128, 128, 128, 0.25);
}

.chord-detail__warning {
  color: #f0a020;
}
</style>
