// 文档位次标签（十天干）——全前端唯一来源，不要在屏幕里复制 TAGS 数组。
// 上限由后端 config::MAX_DOCS 决定（经 DTO 下发），超界回落「文N」。
export const STEMS = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"] as const;

export function docTag(i: number): string {
  return STEMS[i] ?? `文${i + 1}`;
}

/** 位次配色：与高亮体系区分的稳定十色（深浅模式下都可读）。 */
const COLORS = [
  "#4F58A8", "#0E9A8F", "#C28430", "#B54545", "#6B73C9",
  "#2E7D62", "#8A5BA6", "#B06A3B", "#4A7FB5", "#75646C",
];

export function docColor(i: number): string {
  return COLORS[i % COLORS.length];
}
