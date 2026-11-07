/**
 * 适配层：把业务结果翻译成通用组件的输入。
 *
 * 这是**业务与通用组件之间唯一的耦合点**。
 * `PianoRoll.vue` 不认识 `MidiAnalysisResult`，只认识 `RollNote`；
 * 将来后端契约一变，只改这里，卷帘窗组件一行都不用动。
 */

import type {
  AnnotatedNote,
  ChordSegment,
  MidiAnalysisResult,
} from "../types";
import type { RollNote, RollSegment } from "../components/PianoRoll.vue";
import {
  NOTE_ROLE_COLORS,
  NOTE_ROLE_LABELS,
  pitchName,
} from "../constants/music";

/** 和弦段的背景色键（与音符角色的键分开，避免撞色）。 */
export const SEGMENT_GROUP = "chord";

/** 分析结果 → 卷帘窗音符。 */
export function toRollNotes(result: MidiAnalysisResult | null): RollNote[] {
  if (!result) return [];
  return result.notes.map((n: AnnotatedNote) => ({
    pitch: n.pitch,
    start: n.startBeat,
    duration: n.durBeats,
    group: n.role,
    velocity: n.velocity,
    data: n,
  }));
}

/** 分析结果 → 卷帘窗背景区块，标签就是和弦名。 */
export function toRollSegments(result: MidiAnalysisResult | null): RollSegment[] {
  if (!result) return [];
  return result.segments.map((s: ChordSegment) => ({
    start: s.startBeat,
    duration: s.lenBeats,
    label: s.chord.symbol,
    group: SEGMENT_GROUP,
    data: s,
  }));
}

/** 卷帘窗配色表（角色色 + 段背景色）。 */
export function rollColors(): Record<string, string> {
  return { ...NOTE_ROLE_COLORS, [SEGMENT_GROUP]: "#3b82f6" };
}

/** 卷帘窗悬停提示：音名 · 角色 · 位置 · 时长。 */
export function noteTooltip(note: RollNote): string {
  const role = note.group ? (NOTE_ROLE_LABELS[note.group] ?? note.group) : "";
  const head = role ? `${pitchName(note.pitch)} · ${role}` : pitchName(note.pitch);
  return `${head} · 第 ${note.start.toFixed(2)} 拍起 · 长 ${note.duration.toFixed(2)} 拍`;
}

/** 只保留结果里实际出现过的角色，用于图例。 */
export function presentRoles(result: MidiAnalysisResult | null): string[] {
  if (!result) return [];
  const seen = new Set(result.notes.map((n) => n.role));
  return Object.keys(NOTE_ROLE_LABELS).filter((k) => seen.has(k as never));
}

/** 和弦进行压成一行。 */
export function progressionLine(result: MidiAnalysisResult | null): string {
  return result ? result.progression.join(" → ") : "";
}

/**
 * 每小节大约几个和弦——一句话说明和声节奏。
 *
 * 放在这里而不是 `api/`：这是**展示口径**，不是后端接口的一部分。
 */
export function harmonicRhythmHint(result: MidiAnalysisResult | null): string {
  if (!result || result.segments.length === 0 || result.bars <= 0) {
    return "—";
  }
  const perBar = result.segments.length / result.bars;
  if (perBar <= 1.2) return "每小节约 1 个和弦";
  if (perBar <= 2.5) return "每小节约 2 个和弦";
  if (perBar <= 4.5) return "每小节约 4 个和弦";
  return `每小节约 ${Math.round(perBar)} 个和弦`;
}
