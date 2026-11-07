<script setup lang="ts">
/**
 * MIDI 文件投放区：拖放或点击选择。
 *
 * 只管「拿到一个 File」这一件事，不认识分析结果。
 */
import { ref } from "vue";

defineProps<{
  loading: boolean;
  fileName: string;
}>();

const emit = defineEmits<{
  (e: "pick", file: File): void;
}>();

const inputEl = ref<HTMLInputElement | null>(null);
const dragging = ref(false);

function openPicker(): void {
  inputEl.value?.click();
}

function onFileChange(e: Event): void {
  const el = e.target as HTMLInputElement;
  const file = el.files?.[0];
  if (file) emit("pick", file);
  // 清空，允许重复选同一个文件
  el.value = "";
}

function onDrop(e: DragEvent): void {
  dragging.value = false;
  const file = e.dataTransfer?.files?.[0];
  if (file) emit("pick", file);
}
</script>

<template>
  <div
    class="drop"
    :class="{ 'drop--over': dragging, 'drop--loading': loading }"
    @click="openPicker"
    @dragover.prevent="dragging = true"
    @dragleave.prevent="dragging = false"
    @drop.prevent="onDrop"
  >
    <input
      ref="inputEl"
      type="file"
      accept=".mid,.midi,audio/midi,audio/x-midi"
      class="drop__input"
      @change="onFileChange"
    />
    <div class="drop__icon">🎹</div>
    <div class="drop__text">
      {{ loading ? `正在分析 ${fileName} …` : "把 MIDI 文件拖到这里，或点击选择" }}
    </div>
    <div class="drop__sub">支持采样包里的单轨 loop、分轨 kit、完整编曲</div>
  </div>
</template>

<style scoped>
.drop {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 34px 16px;
  border: 2px dashed rgba(128, 128, 128, 0.35);
  border-radius: 12px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}

.drop:hover,
.drop--over {
  border-color: #3b82f6;
  background: rgba(59, 130, 246, 0.08);
}

.drop--loading {
  opacity: 0.7;
  cursor: progress;
}

.drop__input {
  display: none;
}

.drop__icon {
  font-size: 26px;
}

.drop__text {
  font-size: 14px;
}

.drop__sub {
  font-size: 12px;
  opacity: 0.55;
}
</style>
