<script setup lang="ts">
/**
 * 选中和弦段的详情：和弦、低音、代表音、被排除的音。
 *
 * 代表音是「每个和弦音只取一个具体音符」，用来核对识别对不对。
 */
import { computed } from "vue";
import type { ChordSegment } from "../../types";
import { NOTE_ROLE_LABELS, pitchClassName } from "../../constants/music";

const props = defineProps<{
  segment: ChordSegment | null;
}>();

const voicingText = computed(() => {
  const s = props.segment;
  if (!s || s.voicing.length === 0) return "—";
  return s.voicing
    .map((v) => `${v.name}(${NOTE_ROLE_LABELS[v.role] ?? v.role})`)
    .join("　");
});
</script>

<template>
  <div v-if="segment" class="detail">
    <div class="detail__head">
      第 {{ segment.bar }} 小节第 {{ segment.beatInBar }} 拍 · 持续 {{ segment.lenBeats }} 拍
    </div>
    <div class="detail__body">
      <div>
        <span class="detail__k">和弦</span>
        <span class="detail__chord">{{ segment.chord.symbol }}</span>
      </div>
      <div>
        <span class="detail__k">低音</span>
        {{ pitchClassName(segment.chord.bass) }}
        <span v-if="segment.chord.slash" class="detail__tag">斜杠</span>
      </div>
      <div v-if="segment.chord.tensions.length">
        <span class="detail__k">延伸音</span>
        {{ segment.chord.tensions.map((t) => `+${t}`).join(" ") }}
      </div>
      <div class="detail__wide">
        <span class="detail__k">代表音</span>{{ voicingText }}
      </div>
      <div v-if="segment.excluded.length" class="detail__wide">
        <span class="detail__k">排除音</span>{{ segment.excluded.join(" ") }}
      </div>
      <div v-if="segment.pedalBass">
        <span class="detail__k">持续音</span>低音是上一个和弦的残留
      </div>
    </div>
  </div>
</template>

<style scoped>
.detail {
  margin-top: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  background: rgba(128, 128, 128, 0.09);
}

.detail__head {
  font-size: 12px;
  font-weight: 600;
  opacity: 0.8;
  margin-bottom: 6px;
}

.detail__body {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 22px;
  font-size: 13px;
}

.detail__wide {
  flex-basis: 100%;
}

.detail__k {
  display: inline-block;
  min-width: 52px;
  opacity: 0.55;
}

.detail__chord {
  font-weight: 600;
  font-size: 15px;
}

.detail__tag {
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 4px;
  font-size: 11px;
  background: rgba(245, 158, 11, 0.2);
  color: #f59e0b;
}
</style>
