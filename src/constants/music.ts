/**
 * 音乐相关的常量与纯函数。
 *
 * 所有「音名怎么写」「角色叫什么」「用什么颜色」都集中在这里，
 * 改拼写规则或换配色只动这一个文件。
 */

/** 音高类名称（升号拼写）。 */
export const PITCH_CLASS_NAMES = [
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
] as const;

/**
 * MIDI 音高 → 音名。
 *
 * @param pitch MIDI 音高 0..127（60 = C4）
 * @param withOctave 是否带八度，默认带
 */
export function pitchName(pitch: number, withOctave = true): string {
  const safe = ((Math.round(pitch) % 12) + 12) % 12;
  const name = PITCH_CLASS_NAMES[safe];
  if (!withOctave) return name;
  return `${name}${Math.floor(pitch / 12) - 1}`;
}

/** 音高类 → 音名（不带八度）。 */
export function pitchClassName(pitch: number): string {
  return pitchName(pitch, false);
}

/** 音符在和声里的角色 → 中文名。 */
export const NOTE_ROLE_LABELS: Record<string, string> = {
  root: "根音",
  third: "三音",
  fifth: "五音",
  seventh: "七音",
  extension: "延伸音",
  melody: "旋律",
  outside: "段外",
};

/**
 * 音符角色 → 颜色。
 *
 * 这套色是「骨架蓝 → 延伸橙 → 旋律灰」的渐变：
 * 和弦骨架用冷色，色彩音用暖色，被判为非和弦音用灰，
 * 这样卷帘窗上一眼就能看出哪些音参与了和弦。
 */
export const NOTE_ROLE_COLORS: Record<string, string> = {
  root: "#3b82f6",
  third: "#60a5fa",
  fifth: "#22d3ee",
  seventh: "#a78bfa",
  extension: "#f59e0b",
  melody: "#94a3b8",
  outside: "#475569",
};
