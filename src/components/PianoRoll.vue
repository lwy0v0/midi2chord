<script lang="ts">
/**
 * 通用钢琴卷帘窗的数据约定。
 *
 * 刻意**不依赖任何业务类型**：本应用把「和弦分析结果」映射成这几个字段，
 * 换个应用（MIDI 编辑器、采样器、打谱工具）只要有音符和拍，就能直接用。
 */

/** 一个音符。时间单位是「拍」，与拍号无关。 */
export interface RollNote {
  /** MIDI 音高 0..127（60 = C4）。 */
  pitch: number;
  /** 起始位置（拍）。 */
  start: number;
  /** 时长（拍）。 */
  duration: number;
  /** 着色分组键，如 `root` / `melody`；不填用默认色。 */
  group?: string;
  /** 力度 0..127，仅用于透明度。 */
  velocity?: number;
  /** 附加数据，点击时原样回传，组件不解释它。 */
  data?: unknown;
}

/** 背景区块（和弦段、段落标记之类）。 */
export interface RollSegment {
  start: number;
  duration: number;
  label?: string;
  group?: string;
  data?: unknown;
}
</script>

<script setup lang="ts">
/**
 * 钢琴卷帘窗（纯展示 + 交互，Canvas 渲染）。
 *
 * 用法：
 *   <PianoRoll :notes="notes" :segments="segments" :colors="{ root: '#3b82f6' }" />
 *
 * 特点：
 * - 用 Canvas 画，几千个音符也能一次绘完（DOM/SVG 到 6000 个就会卡）；
 * - props 变化即重绘（用 requestAnimationFrame 合并连续变更）；
 * - 悬停出提示、点击回传原始数据，组件不理解数据含义。
 */
import { computed, onBeforeUnmount, nextTick, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    /** 音符列表。 */
    notes: RollNote[];
    /** 背景区块（和弦段）。 */
    segments?: RollSegment[];
    /** 每小节几拍。 */
    beatsPerBar?: number;
    /** 总长度（拍）；不给则按内容自动算。 */
    totalBeats?: number;
    /** 最低音高；与 pitchMax 一起给就锁定音域，否则自动。 */
    pitchMin?: number;
    pitchMax?: number;
    /** 每拍多少像素。 */
    beatWidth?: number;
    /** 每个半音多少像素。 */
    rowHeight?: number;
    /** 分组 → 颜色；没给的分组按调色板自动分配。 */
    colors?: Record<string, string>;
    /** 高亮第几个背景区块（-1 或 null 表示不高亮）。 */
    selectedSegment?: number | null;
    /** 提示条文字，默认「音名 时间」。 */
    noteLabel?: (note: RollNote, index: number) => string;
    /** 容器最大高度，超出滚动。 */
    maxHeight?: number;
    /** 是否显示左侧音名。 */
    showPitchLabels?: boolean;
  }>(),
  {
    segments: () => [],
    beatsPerBar: 4,
    beatWidth: 46,
    rowHeight: 9,
    colors: () => ({}),
    selectedSegment: null,
    maxHeight: 360,
    showPitchLabels: true,
  },
);

const emit = defineEmits<{
  (e: "note-click", note: RollNote, index: number): void;
  (e: "segment-click", segment: RollSegment, index: number): void;
}>();

/** 默认调色板：按分组首次出现顺序分配，保证同一份数据每次颜色一致。 */
const PALETTE = [
  "#3b82f6",
  "#22c55e",
  "#f59e0b",
  "#a855f7",
  "#06b6d4",
  "#ef4444",
  "#eab308",
  "#94a3b8",
];

const PITCH_NAMES = [
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
];
/** 黑键的音高类。 */
const BLACK_KEYS = new Set([1, 3, 6, 8, 10]);

const canvasEl = ref<HTMLCanvasElement | null>(null);
const hovered = ref<{ note: RollNote; index: number; x: number; y: number } | null>(
  null,
);

/** 音名（带八度）。 */
function pitchName(pitch: number): string {
  return `${PITCH_NAMES[pitch % 12]}${Math.floor(pitch / 12) - 1}`;
}

const range = computed(() => {
  let lo = props.pitchMin ?? -1;
  let hi = props.pitchMax ?? -1;
  if (lo < 0 || hi < 0) {
    let min = 127;
    let max = 0;
    for (const n of props.notes) {
      if (n.pitch < min) min = n.pitch;
      if (n.pitch > max) max = n.pitch;
    }
    if (props.notes.length === 0) {
      min = 48;
      max = 72;
    }
    lo = props.pitchMin ?? Math.max(0, min - 1);
    hi = props.pitchMax ?? Math.min(127, max + 1);
  }
  if (hi < lo) [lo, hi] = [hi, lo];
  return { lo, hi };
});

const totalBeats = computed(() => {
  if (props.totalBeats && props.totalBeats > 0) return props.totalBeats;
  let end = 0;
  for (const n of props.notes) end = Math.max(end, n.start + n.duration);
  for (const s of props.segments) end = Math.max(end, s.start + s.duration);
  return Math.max(end, props.beatsPerBar);
});

const contentWidth = computed(() => totalBeats.value * props.beatWidth);
const contentHeight = computed(
  () => (range.value.hi - range.value.lo + 1) * props.rowHeight,
);

const colorMap = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {};
  const seen = new Set<string>();
  for (const n of props.notes) {
    if (n.group) seen.add(n.group);
  }
  let i = 0;
  for (const g of seen) {
    map[g] = props.colors[g] ?? PALETTE[i % PALETTE.length];
    i += 1;
  }
  return map;
});

function colorOf(note: RollNote): string {
  const key = note.group ?? "";
  return colorMap.value[key] ?? props.colors[key] ?? "#8b9bb4";
}

/** 座标反查音符（从后往前，后画的在上层）。 */
function hitTest(x: number, y: number): { note: RollNote; index: number } | null {
  const beat = x / props.beatWidth;
  const row = Math.floor(y / props.rowHeight);
  const pitch = range.value.hi - row;
  if (pitch < range.value.lo || pitch > range.value.hi) return null;
  for (let i = props.notes.length - 1; i >= 0; i -= 1) {
    const n = props.notes[i];
    if (n.pitch === pitch && beat >= n.start && beat <= n.start + n.duration) {
      return { note: n, index: i };
    }
  }
  return null;
}

/** 取一个圆角矩形的路径（兼容没有 roundRect 的环境）。 */
function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.lineTo(x + w - radius, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + radius);
  ctx.lineTo(x + w, y + h - radius);
  ctx.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
  ctx.lineTo(x + radius, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - radius);
  ctx.lineTo(x, y + radius);
  ctx.quadraticCurveTo(x, y, x + radius, y);
  ctx.closePath();
}

function draw(): void {
  const canvas = canvasEl.value;
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, contentWidth.value);
  const h = Math.max(1, contentHeight.value);
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const { lo, hi } = range.value;
  const rh = props.rowHeight;
  const bw = props.beatWidth;
  const bpb = props.beatsPerBar;

  // ---- 背景：黑白键 ----
  for (let pitch = hi; pitch >= lo; pitch -= 1) {
    const y = (hi - pitch) * rh;
    ctx.fillStyle = BLACK_KEYS.has(pitch % 12) ? "#1b2029" : "#232936";
    ctx.fillRect(0, y, w, rh);
  }

  // ---- 横向线：每个八度的 C 加粗 ----
  for (let pitch = lo; pitch <= hi; pitch += 1) {
    if (pitch % 12 !== 0) continue;
    const y = (hi - pitch + 1) * rh;
    ctx.strokeStyle = "rgba(255,255,255,0.10)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, y + 0.5);
    ctx.lineTo(w, y + 0.5);
    ctx.stroke();
  }

  // ---- 纵向线：拍线 + 小节线 ----
  const beats = Math.ceil(totalBeats.value);
  for (let beat = 0; beat <= beats; beat += 1) {
    const x = beat * bw;
    const isBar = beat % bpb === 0;
    ctx.strokeStyle = isBar ? "rgba(255,255,255,0.22)" : "rgba(255,255,255,0.07)";
    ctx.lineWidth = isBar ? 1 : 1;
    ctx.beginPath();
    ctx.moveTo(x + 0.5, 0);
    ctx.lineTo(x + 0.5, h);
    ctx.stroke();
  }

  // ---- 背景区块（和弦段）----
  props.segments.forEach((seg, i) => {
    const x = seg.start * bw;
    const sw = Math.max(1, seg.duration * bw);
    const selected = props.selectedSegment === i;
    const base = seg.group ? (props.colors[seg.group] ?? "#4b5563") : "#4b5563";
    ctx.save();
    ctx.globalAlpha = selected ? 0.34 : 0.16;
    ctx.fillStyle = base;
    ctx.fillRect(x, 0, sw, h);
    ctx.restore();
    if (selected) {
      ctx.strokeStyle = base;
      ctx.lineWidth = 2;
      ctx.strokeRect(x + 1, 1, sw - 2, h - 2);
    }
    if (seg.label) {
      ctx.save();
      ctx.font = "600 10px ui-sans-serif, system-ui, sans-serif";
      ctx.fillStyle = selected ? "#ffffff" : "rgba(255,255,255,0.72)";
      ctx.textBaseline = "top";
      ctx.fillText(seg.label, x + 4, 3, Math.max(4, sw - 8));
      ctx.restore();
    }
  });

  // ---- 音符 ----
  const hoveredIndex = hovered.value?.index ?? -1;
  props.notes.forEach((n, i) => {
    if (n.pitch < lo || n.pitch > hi) return;
    const x = n.start * bw;
    const nw = Math.max(2, n.duration * bw);
    const y = (hi - n.pitch) * rh;
    const nh = Math.max(2, rh - 1);
    const isHover = i === hoveredIndex;
    ctx.save();
    ctx.globalAlpha = n.velocity === undefined ? 0.95 : 0.55 + 0.45 * (n.velocity / 127);
    ctx.fillStyle = colorOf(n);
    roundRect(ctx, x + 0.5, y + 0.5, nw - 1, nh, Math.min(3, rh / 3));
    ctx.fill();
    if (isHover) {
      ctx.globalAlpha = 1;
      ctx.strokeStyle = "#ffffff";
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
    ctx.restore();
  });

  // ---- 左侧音名（画在最上层，避免被音符盖住）----
  if (props.showPitchLabels) {
    ctx.save();
    ctx.font = "9px ui-monospace, SFMono-Regular, monospace";
    ctx.textBaseline = "middle";
    for (let pitch = hi; pitch >= lo; pitch -= 1) {
      if (pitch % 12 !== 0) continue;
      const y = (hi - pitch) * rh + rh / 2;
      ctx.fillStyle = "rgba(255,255,255,0.42)";
      ctx.fillRect(0, (hi - pitch) * rh, 22, rh);
      ctx.fillStyle = "rgba(255,255,255,0.85)";
      ctx.fillText(pitchName(pitch), 2, y);
    }
    ctx.restore();
  }
}

// ---- 重绘调度：把连续变化合并到一帧 ----
let rafId = 0;
function scheduleDraw(): void {
  if (rafId) return;
  rafId = requestAnimationFrame(() => {
    rafId = 0;
    draw();
  });
}

watch(
  () => [
    props.notes,
    props.segments,
    props.beatsPerBar,
    props.totalBeats,
    props.pitchMin,
    props.pitchMax,
    props.beatWidth,
    props.rowHeight,
    props.colors,
    props.selectedSegment,
    props.showPitchLabels,
  ],
  scheduleDraw,
);

watch(hovered, scheduleDraw);

// ---- 交互 ----
function localPoint(e: MouseEvent): { x: number; y: number } {
  const canvas = canvasEl.value;
  if (!canvas) return { x: 0, y: 0 };
  const rect = canvas.getBoundingClientRect();
  return { x: e.clientX - rect.left, y: e.clientY - rect.top };
}

function onMouseMove(e: MouseEvent): void {
  const { x, y } = localPoint(e);
  const hit = hitTest(x, y);
  if (hit) {
    hovered.value = { note: hit.note, index: hit.index, x, y };
  } else {
    hovered.value = null;
  }
}

function onMouseLeave(): void {
  hovered.value = null;
}

function onClick(e: MouseEvent): void {
  const { x, y } = localPoint(e);
  // 先看是不是点到某个背景区块（音符优先）
  const hit = hitTest(x, y);
  if (hit) {
    emit("note-click", hit.note, hit.index);
    return;
  }
  const beat = x / props.beatWidth;
  for (let i = props.segments.length - 1; i >= 0; i -= 1) {
    const s = props.segments[i];
    if (beat >= s.start && beat <= s.start + s.duration) {
      emit("segment-click", s, i);
      return;
    }
  }
}

const tipText = computed(() => {
  const h = hovered.value;
  if (!h) return "";
  if (props.noteLabel) return props.noteLabel(h.note, h.index);
  return `${pitchName(h.note.pitch)} · 第 ${h.note.start.toFixed(2)} 拍起 · 长 ${h.note.duration.toFixed(2)} 拍`;
});

const tipStyle = computed(() => {
  const h = hovered.value;
  if (!h) return {};
  return {
    left: `${h.x + 12}px`,
    top: `${h.y + 12}px`,
  };
});

/** 供父组件在「原地修改数组」之后手动触发重绘。 */
function redraw(): void {
  scheduleDraw();
}

defineExpose({ redraw });

onBeforeUnmount(() => {
  if (rafId) cancelAnimationFrame(rafId);
});

// 首次挂载后画一次（等 DOM 就绪）
watch(
  canvasEl,
  (el) => {
    if (el) {
      nextTick(scheduleDraw);
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="roll" :style="{ maxHeight: `${maxHeight}px` }">
    <div class="roll__scroller">
      <canvas
        ref="canvasEl"
        class="roll__canvas"
        @mousemove="onMouseMove"
        @mouseleave="onMouseLeave"
        @click="onClick"
      />
      <div v-if="hovered" class="roll__tip" :style="tipStyle">
        {{ tipText }}
      </div>
    </div>
    <div v-if="notes.length === 0" class="roll__empty">没有音符数据</div>
  </div>
</template>

<style scoped>
.roll {
  position: relative;
  border-radius: 10px;
  overflow: hidden;
  background: #161b22;
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.roll__scroller {
  position: relative;
  overflow: auto;
  max-height: inherit;
}

.roll__canvas {
  display: block;
  cursor: crosshair;
}

.roll__tip {
  position: absolute;
  z-index: 3;
  pointer-events: none;
  padding: 3px 7px;
  border-radius: 6px;
  font-size: 11px;
  line-height: 1.5;
  white-space: nowrap;
  color: #f8fafc;
  background: rgba(15, 23, 42, 0.94);
  border: 1px solid rgba(255, 255, 255, 0.16);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.45);
}

.roll__empty {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  color: rgba(255, 255, 255, 0.45);
}
</style>
