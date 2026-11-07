/** 十二个音名的可选项（值与后端 `Note::parse` 接受的写法一致）。 */

import type { SelectOption } from "naive-ui";

export const NOTE_OPTIONS: SelectOption[] = [
  { label: "C", value: "C" },
  { label: "C# / Db", value: "C#" },
  { label: "D", value: "D" },
  { label: "D# / Eb", value: "D#" },
  { label: "E", value: "E" },
  { label: "F", value: "F" },
  { label: "F# / Gb", value: "F#" },
  { label: "G", value: "G" },
  { label: "G# / Ab", value: "G#" },
  { label: "A", value: "A" },
  { label: "A# / Bb", value: "A#" },
  { label: "B", value: "B" },
];

/** 每个和弦允许的最少 / 最多音数。 */
export const MIN_NOTES_PER_CHORD = 2;
export const MAX_NOTES_PER_CHORD = 6;

/** 和弦数量范围。 */
export const MIN_CHORDS = 1;
export const MAX_CHORDS = 12;
