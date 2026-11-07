<script lang="ts">
/** 图例项：颜色 + 名称。放普通 script 块里才能被外部 import。 */
export interface LegendItem {
  key: string;
  label: string;
  color: string;
}
</script>

<script setup lang="ts">
/**
 * 通用图例：一组「颜色 + 名称」。
 *
 * 不认识任何业务字段，传什么就显示什么；换项目也能用。
 */
defineProps<{
  items: LegendItem[];
}>();
</script>

<template>
  <div v-if="items.length" class="legend">
    <span v-for="item in items" :key="item.key" class="legend__item">
      <i class="legend__dot" :style="{ background: item.color }" />
      {{ item.label }}
    </span>
  </div>
</template>

<style scoped>
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-bottom: 8px;
  font-size: 12px;
  opacity: 0.82;
}

.legend__item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.legend__dot {
  width: 10px;
  height: 10px;
  border-radius: 2px;
  display: inline-block;
}
</style>
