<script setup lang="ts">
/** 分析结果面板：最佳调性摘要、识别结果、警告与候选调列表。 */
import { computed, ref } from "vue";
import type { AnalysisResult, AnalyzeError } from "../types";
import { confidenceType } from "../utils/format";
import KeyCandidateCard from "./KeyCandidateCard.vue";

const props = defineProps<{
  result: AnalysisResult | null;
  error: AnalyzeError | null;
}>();

/** 默认只展示前几个候选，避免一次铺开 12 张卡片。 */
const VISIBLE_KEYS = 3;
const showAll = ref(false);

const candidates = computed(() => props.result?.keys ?? []);
const best = computed(() => candidates.value[0] ?? null);
const visible = computed(() =>
  showAll.value ? candidates.value : candidates.value.slice(0, VISIBLE_KEYS),
);
const hiddenCount = computed(() => Math.max(0, candidates.value.length - VISIBLE_KEYS));
</script>

<template>
  <div class="analysis">
    <n-alert
      v-if="error"
      type="error"
      :title="error.message"
      class="analysis__alert"
      closable
    >
      <template v-if="error.row !== null" #default>
        定位到第 {{ (error.row ?? 0) + 1 }} 个和弦<template v-if="error.col !== null">的第 {{ (error.col ?? 0) + 1 }} 个音</template>
      </template>
    </n-alert>

    <template v-if="result">
      <n-alert
        v-for="(warning, index) in result.warnings"
        :key="`warning-${index}`"
        type="warning"
        class="analysis__alert"
      >
        {{ warning }}
      </n-alert>

      <n-card v-if="best" size="small" class="analysis__summary">
        <n-flex align="center" :size="12" wrap>
          <n-text strong>最佳匹配调性：</n-text>
          <n-tag type="primary" size="large" :bordered="false">{{ best.displayName }}</n-tag>
          <n-tag :type="confidenceType(best.confidence)" :bordered="false">
            贴合度 {{ best.confidence }}%
          </n-tag>
          <n-text depth="3" class="analysis__recognized">
            识别为：{{ result.recognized.join(" | ") }}
          </n-text>
        </n-flex>
      </n-card>

      <n-empty
        v-if="candidates.length === 0"
        description="没有找到匹配的调性，请检查输入的和弦"
        class="analysis__empty"
      />

      <template v-else>
        <n-divider>候选调性（按贴合度排序）</n-divider>
        <n-space vertical :size="12">
          <KeyCandidateCard
            v-for="(candidate, index) in visible"
            :key="candidate.key + candidate.mode"
            :candidate="candidate"
            :best="index === 0"
            :best-score="best?.score ?? 0"
          />
        </n-space>
        <n-button
          v-if="hiddenCount > 0"
          text
          type="primary"
          class="analysis__more"
          @click="showAll = !showAll"
        >
          {{ showAll ? "收起其它候选" : `展开其余 ${hiddenCount} 个候选调` }}
        </n-button>
      </template>
    </template>
  </div>
</template>

<style scoped>
.analysis {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.analysis__alert {
  border-radius: 10px;
}

.analysis__summary {
  border-radius: 12px;
}

.analysis__recognized {
  font-size: 12px;
}

.analysis__empty {
  padding: 24px 0;
}

.analysis__more {
  align-self: center;
}
</style>
