<script setup lang="ts">
/** 和弦输入网格：展示每个和弦的若干音位，并把错误高亮到具体格子。 */
import { computed } from "vue";
import { NOTE_OPTIONS } from "../constants/notes";
import type { CellError } from "../types";

const props = defineProps<{
  chords: string[][];
  errors: CellError[];
  /** 含重复音的行号（后端会自动去重，这里只做提示）。 */
  duplicateRows: number[];
}>();

const emit = defineEmits<{
  (event: "update-cell", payload: { row: number; col: number; value: string }): void;
}>();

const errorMap = computed(() => {
  const map = new Map<string, CellError>();
  props.errors.forEach((error) => map.set(`${error.row}:${error.col}`, error));
  return map;
});

function cellError(row: number, col: number): CellError | undefined {
  return errorMap.value.get(`${row}:${col}`);
}

function rowHasError(row: number): boolean {
  return props.errors.some((error) => error.row === row);
}

function onSelect(row: number, col: number, value: string | number | null): void {
  emit("update-cell", { row, col, value: value === null ? "" : String(value) });
}
</script>

<template>
  <div class="chord-grid">
    <div
      v-for="(line, row) in chords"
      :key="row"
      class="chord-row"
      :class="{ 'chord-row--error': rowHasError(row) }"
    >
      <div class="chord-row__head">
        <n-text strong>和弦 {{ row + 1 }}</n-text>
        <n-tag v-if="rowHasError(row)" type="error" size="small" :bordered="false">需修正</n-tag>
        <n-tag
          v-else-if="duplicateRows.includes(row)"
          type="warning"
          size="small"
          :bordered="false"
        >
          含重复音
        </n-tag>
      </div>

      <div class="chord-row__cells">
        <div v-for="(note, col) in line" :key="col" class="chord-cell">
          <n-select
            :value="note || null"
            :options="NOTE_OPTIONS"
            :placeholder="col === 0 ? '低音' : `音 ${col + 1}`"
            :status="cellError(row, col) ? 'error' : undefined"
            filterable
            clearable
            class="chord-cell__select"
            @update:value="(value: string | number | null) => onSelect(row, col, value)"
          />
          <n-text v-if="cellError(row, col)" class="chord-cell__error" depth="3">
            {{ cellError(row, col)?.message }}
          </n-text>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chord-grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.chord-row {
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: rgba(24, 160, 88, 0.06);
}

.chord-row--error {
  border-color: #d03050;
  background: rgba(208, 48, 80, 0.08);
}

.chord-row__head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.chord-row__cells {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.chord-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.chord-cell__select {
  width: 128px;
}

.chord-cell__error {
  max-width: 128px;
  font-size: 11px;
  line-height: 1.3;
}
</style>
