<script setup lang="ts">
/** 行列数量控制（纯展示组件，尺寸由父级持有）。 */
import {
  MAX_CHORDS,
  MAX_NOTES_PER_CHORD,
  MIN_CHORDS,
  MIN_NOTES_PER_CHORD,
} from "../constants/notes";

defineProps<{
  rows: number;
  cols: number;
}>();

const emit = defineEmits<{
  (event: "update:rows", value: number): void;
  (event: "update:cols", value: number): void;
}>();

function onRows(value: number | null): void {
  emit("update:rows", value ?? MIN_CHORDS);
}

function onCols(value: number | null): void {
  emit("update:cols", value ?? MIN_NOTES_PER_CHORD);
}
</script>

<template>
  <n-flex align="center" :size="20" class="size-controls" wrap>
    <n-form-item label="每个和弦的音数" :show-feedback="false" class="size-controls__item">
      <n-input-number
        :value="cols"
        :min="MIN_NOTES_PER_CHORD"
        :max="MAX_NOTES_PER_CHORD"
        class="size-controls__input"
        @update:value="onCols"
      />
    </n-form-item>
    <n-form-item label="和弦数量" :show-feedback="false" class="size-controls__item">
      <n-input-number
        :value="rows"
        :min="MIN_CHORDS"
        :max="MAX_CHORDS"
        class="size-controls__input"
        @update:value="onRows"
      />
    </n-form-item>
    <n-text depth="3" class="size-controls__hint">行内第一个音按约定视为低音，用于识别转位</n-text>
  </n-flex>
</template>

<style scoped>
.size-controls {
  margin-bottom: 4px;
}

.size-controls__input {
  width: 150px;
}

.size-controls__hint {
  font-size: 12px;
}
</style>
