<script setup lang="ts">
/**
 * 候选调列表 —— **把可能性摊开，让用户自己挑**。
 *
 * 调性判断天然有歧义（关系大小调、五度关系都能自圆其说），
 * 所以这里不藏候选，而是把每种读法和它的依据都列出来。
 */
import type { KeyOption } from "../../types";

defineProps<{
  options: KeyOption[];
  /** 当前选中的下标。 */
  picked: number;
}>();

const emit = defineEmits<{
  (e: "pick", index: number): void;
}>();

/** 一句话说明这个候选凭什么排在这里。 */
function reason(o: KeyOption): string {
  if (o.startsOnTonic && o.endsOnTonic) return "开头结尾都是主和弦";
  if (o.startsOnTonic) return "开头就是主和弦";
  if (o.endsOnTonic) return "结尾是主和弦";
  if (o.tonicShare >= 25) return `主和弦占 ${o.tonicShare}%`;
  return "主和弦没出现";
}

/** 没有主和弦支撑的候选要标黄——它不是错，但值得怀疑。 */
function weak(o: KeyOption): boolean {
  return !o.startsOnTonic && !o.endsOnTonic && o.tonicShare === 0;
}
</script>

<template>
  <div v-if="options.length > 1" class="opts">
    <div class="opts__title">
      调性有 {{ options.length }} 种读法，点一个切换
      <span class="opts__title-sub">和弦名不变，只有罗马数字跟着变</span>
    </div>
    <div class="opts__list">
      <div
        v-for="(o, i) in options"
        :key="i"
        class="opts__item"
        :class="{ 'opts__item--on': picked === i, 'opts__item--weak': weak(o) }"
        @click="emit('pick', i)"
      >
        <div class="opts__row">
          <span class="opts__name">{{ o.name }}</span>
          <span
            class="opts__reason"
            :class="{ 'opts__reason--strong': o.startsOnTonic || o.endsOnTonic }"
          >
            {{ reason(o) }}
          </span>
          <span class="opts__score" :title="`音级分布分 ${o.pitchScore} / 综合分 ${o.rankScore}`">
            音级 {{ o.pitchScore }}
          </span>
        </div>
        <div class="opts__prog">{{ o.progression.join(" → ") || "—" }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.opts {
  margin-bottom: 12px;
}

.opts__title {
  font-size: 12px;
  font-weight: 600;
  opacity: 0.75;
  margin-bottom: 6px;
}

.opts__title-sub {
  font-weight: 400;
  opacity: 0.55;
  margin-left: 8px;
}

.opts__list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.opts__item {
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid rgba(128, 128, 128, 0.22);
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s;
}

.opts__item:hover {
  background: rgba(128, 128, 128, 0.1);
}

.opts__item--on {
  border-color: #3b82f6;
  background: rgba(59, 130, 246, 0.14);
}

.opts__item--weak {
  opacity: 0.72;
}

.opts__row {
  display: flex;
  align-items: baseline;
  gap: 10px;
}

.opts__name {
  font-weight: 600;
  font-size: 13px;
  min-width: 150px;
}

.opts__reason {
  font-size: 11px;
  opacity: 0.6;
}

.opts__reason--strong {
  opacity: 0.95;
  color: #22c55e;
}

.opts__score {
  margin-left: auto;
  font-size: 11px;
  opacity: 0.45;
}

.opts__prog {
  font-size: 11px;
  opacity: 0.5;
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
