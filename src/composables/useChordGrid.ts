/**
 * 和弦输入网格的状态：行列尺寸、单元格读写、本地校验与错误定位。
 *
 * 纯状态逻辑，不含任何 UI；组件只负责展示与派发事件。
 */

import { computed, ref, watch, type ComputedRef, type Ref } from "vue";
import { MAX_CHORDS, MAX_NOTES_PER_CHORD, MIN_CHORDS, MIN_NOTES_PER_CHORD } from "../constants/notes";
import type { CellError } from "../types";

/** 在保留已有输入的前提下重建网格。 */
function createGrid(rows: number, cols: number, previous: string[][] = []): string[][] {
  const grid: string[][] = [];
  for (let row = 0; row < rows; row += 1) {
    const line: string[] = [];
    for (let col = 0; col < cols; col += 1) {
      line.push(previous[row]?.[col] ?? "");
    }
    grid.push(line);
  }
  return grid;
}

function clamp(value: number | null, min: number, max: number, fallback: number): number {
  if (value === null || Number.isNaN(value)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, Math.round(value)));
}

export interface ChordGrid {
  rows: Ref<number>;
  cols: Ref<number>;
  chords: Ref<string[][]>;
  errors: Ref<CellError[]>;
  /** 有重复音的行号（后端会自动去重，这里只做提示）。 */
  duplicateRows: ComputedRef<number[]>;
  errorAt: (row: number, col: number) => CellError | undefined;
  rowHasError: (row: number) => boolean;
  setCell: (row: number, col: number, value: string) => void;
  setErrors: (errors: CellError[]) => void;
  clearErrors: () => void;
  /** 本地校验：只拦截「空」与「不足两个不同的音」，其余交给后端。 */
  validate: () => CellError[];
  reset: () => void;
}

export function useChordGrid(initialRows = 2, initialCols = 3): ChordGrid {
  const rows = ref(clamp(initialRows, MIN_CHORDS, MAX_CHORDS, 2));
  const cols = ref(clamp(initialCols, MIN_NOTES_PER_CHORD, MAX_NOTES_PER_CHORD, 3));
  const chords = ref<string[][]>(createGrid(rows.value, cols.value));
  const errors = ref<CellError[]>([]);

  watch([rows, cols], () => {
    rows.value = clamp(rows.value, MIN_CHORDS, MAX_CHORDS, 2);
    cols.value = clamp(cols.value, MIN_NOTES_PER_CHORD, MAX_NOTES_PER_CHORD, 3);
    chords.value = createGrid(rows.value, cols.value, chords.value);
    errors.value = [];
  });

  const duplicateRows = computed(() => {
    const found: number[] = [];
    chords.value.forEach((line, index) => {
      const selected = line.filter((note) => note !== "");
      if (selected.length !== new Set(selected).size) {
        found.push(index);
      }
    });
    return found;
  });

  function errorAt(row: number, col: number): CellError | undefined {
    return errors.value.find((error) => error.row === row && error.col === col);
  }

  function rowHasError(row: number): boolean {
    return errors.value.some((error) => error.row === row);
  }

  function setCell(row: number, col: number, value: string): void {
    const line = chords.value[row];
    if (!line || col >= line.length) {
      return;
    }
    line[col] = value;
    errors.value = errors.value.filter((error) => !(error.row === row && error.col === col));
  }

  function setErrors(list: CellError[]): void {
    errors.value = list;
  }

  function clearErrors(): void {
    errors.value = [];
  }

  function validate(): CellError[] {
    const found: CellError[] = [];
    chords.value.forEach((line, row) => {
      line.forEach((note, col) => {
        if (note.trim() === "") {
          found.push({ row, col, message: `第 ${row + 1} 个和弦的第 ${col + 1} 个音还没有选择` });
        }
      });
      const distinct = new Set(line.filter((note) => note.trim() !== ""));
      if (distinct.size < MIN_NOTES_PER_CHORD) {
        found.push({
          row,
          col: 0,
          message: `第 ${row + 1} 个和弦至少需要 ${MIN_NOTES_PER_CHORD} 个不同的音`,
        });
      }
    });
    return found;
  }

  function reset(): void {
    chords.value = createGrid(rows.value, cols.value);
    errors.value = [];
  }

  return {
    rows,
    cols,
    chords,
    errors,
    duplicateRows,
    errorAt,
    rowHasError,
    setCell,
    setErrors,
    clearErrors,
    validate,
    reset,
  };
}
