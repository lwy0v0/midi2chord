/** 展示用的格式化工具。 */

import type { KeyCandidate } from "../types";

/** 调性类型描述（旧版靠调名后缀猜大小调，现在直接读后端字段）。 */
export function keyModeLabel(candidate: KeyCandidate): string {
  return candidate.isMajor ? "大调" : "小调";
}

/** 调号描述，如 `2 个升号` / `1 个降号` / `无升降号`。 */
export function signatureLabel(accidentals: number): string {
  if (accidentals === 0) {
    return "无升降号";
  }
  return `${Math.abs(accidentals)} 个${accidentals > 0 ? "升" : "降"}号`;
}

/** 置信度对应的 Naive UI 标签颜色。 */
export function confidenceType(confidence: number): "success" | "info" | "warning" | "default" {
  if (confidence >= 85) {
    return "success";
  }
  if (confidence >= 65) {
    return "info";
  }
  if (confidence >= 40) {
    return "warning";
  }
  return "default";
}

/** 调内关系分类 → 中文标签。 */
export const CATEGORY_LABELS: Record<string, string> = {
  diatonic: "调内",
  harmonicMinor: "和声小调",
  secondaryDominant: "副属和弦",
  modalMixture: "同主音借用",
  neapolitan: "那不勒斯",
  chromatic: "调外",
};

export function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category;
}

/** 功能标记（T/S/D）对应的颜色。 */
export function functionType(func: string): "success" | "info" | "warning" | "default" {
  switch (func) {
    case "T":
      return "success";
    case "S":
      return "info";
    case "D":
      return "warning";
    default:
      return "default";
  }
}
