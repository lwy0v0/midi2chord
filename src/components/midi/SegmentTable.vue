<script setup lang="ts">
/**
 * 和弦段列表。
 *
 * 只负责把段列出来并处理选中；点第二次取消选中。
 * 不认识「分析结果」整体，只吃段数组与罗马数字。
 */
import type { ChordSegment } from "../../types";
import { pitchClassName } from "../../constants/music";

const props = defineProps<{
  segments: ChordSegment[];
  /** 与 segments 等长的罗马数字。 */
  romans: string[];
  /** 当前选中的下标。 */
  selected: number | null;
}>();

const emit = defineEmits<{
  (e: "select", index: number | null): void;
}>();

function toggle(index: number): void {
  emit("select", props.selected === index ? null : index);
}
</script>

<template>
  <div class="segments">
    <div class="segments__row segments__row--head">
      <span>位置</span>
      <span>和弦</span>
      <span>罗马</span>
      <span>拍数</span>
      <span>低音</span>
    </div>
    <div
      v-for="(s, i) in segments"
      :key="i"
      class="segments__row"
      :class="{ 'segments__row--active': selected === i }"
      @click="toggle(i)"
    >
      <span>{{ s.bar }}.{{ s.beatInBar }}</span>
      <span class="segments__chord">{{ s.chord.symbol }}</span>
      <span class="segments__roman">{{ romans[i] ?? "" }}</span>
      <span>{{ s.lenBeats }}</span>
      <span>{{ pitchClassName(s.chord.bass) }}{{ s.chord.slash ? "·" : "" }}</span>
    </div>
  </div>
</template>

<style scoped>
.segments {
  max-height: 260px;
  overflow: auto;
  font-size: 13px;
  border-radius: 8px;
  border: 1px solid rgba(128, 128, 128, 0.2);
}

.segments__row {
  display: grid;
  grid-template-columns: 74px 1fr 92px 58px 56px;
  gap: 6px;
  padding: 5px 10px;
  cursor: pointer;
  border-bottom: 1px solid rgba(128, 128, 128, 0.12);
}

.segments__row:last-child {
  border-bottom: none;
}

.segments__row:hover {
  background: rgba(128, 128, 128, 0.08);
}

.segments__row--head {
  position: sticky;
  top: 0;
  z-index: 1;
  font-weight: 600;
  opacity: 0.7;
  background: rgba(30, 35, 45, 0.96);
  cursor: default;
}

.segments__row--active {
  background: rgba(59, 130, 246, 0.16);
}

.segments__chord {
  font-weight: 600;
}

.segments__roman {
  opacity: 0.8;
}
</style>
