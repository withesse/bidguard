// 商务标数值证据（W5-4/W5-6）的纯派生逻辑 —— 与 React 解耦，便于单测。
// 铁律：这里只做「把后端事实排布成可视形状」，不重算任何指标、不改判任何结论。
import type { BoqCorrelationDto, BoqPatternKind, BoqScatterPoint, NumericPairDto } from "../api/types";

/** 强证据双条件（§1.5）：r>0.99 且比值 CV<0.5%。两者缺一都只是「天然同源」的噪声。 */
export const STRONG_R_MIN = 0.99;
export const STRONG_CV_MAX = 0.005;

/** 规律性形态的中文标签（与后端 boq::PatternKind 的 snake_case 稳定标识一一对应）。 */
export function patternLabel(kind: BoqPatternKind | string): string {
  switch (kind) {
    case "arith_seq":
      return "等差（各项差额恒定）";
    case "geo_discount":
      return "等比 / 恒定折扣（各项系数恒定）";
    case "affine":
      return "仿射（系数与差额均非平凡）";
    default:
      return kind;
  }
}

/** 雷同率缺席原因的中文文案（不出结论也要出原因）。 */
export function reasonLabel(reason: string | null | undefined, minComparable: number): string {
  if (reason === "insufficient") return `可比清单项不足 ${minComparable} 项，不出雷同率结论`;
  if (reason) return reason;
  return "无可比清单项";
}

/**
 * 相关性是否达到「强证据」双条件。达不到时面板必须显式说明「不构成强证据」——
 * 投标人单价天然同源（同一定额库/信息价）会让 r 普遍 0.9+。
 */
export function isStrongCorrelation(c: BoqCorrelationDto | null | undefined): boolean {
  if (!c) return false;
  return c.pearson > STRONG_R_MIN && c.ratioCv != null && c.ratioCv < STRONG_CV_MAX;
}

/** 文档对的无向键（(a,b) 归一为 a<b），供选择器与查表共用。 */
export function pairKey(a: number, b: number): string {
  return a <= b ? `${a}|${b}` : `${b}|${a}`;
}

/**
 * N×N 雷同率热力矩阵：对角线与缺结论的格子为 null（渲染成「—」，不得当 0 展示——
 * 0% 与「没算」是两回事）。
 */
export function identicalMatrix(pairs: NumericPairDto[], n: number): (number | null)[][] {
  const m: (number | null)[][] = Array.from({ length: n }, () => Array<number | null>(n).fill(null));
  for (const p of pairs) {
    if (p.a >= n || p.b >= n) continue;
    const v = p.identicalRate ?? null;
    m[p.a][p.b] = v;
    m[p.b][p.a] = v;
  }
  return m;
}

/** 按 pairKey 索引的文档对统计，供选择器切换后 O(1) 取用。 */
export function pairIndex(pairs: NumericPairDto[]): Map<string, NumericPairDto> {
  return new Map(pairs.map((p) => [pairKey(p.a, p.b), p]));
}

/** 面板默认选中的文档对：优先告警对 → 其次雷同率最高 → 再次首个有可比项的对。 */
export function defaultPair(pairs: NumericPairDto[]): NumericPairDto | null {
  if (pairs.length === 0) return null;
  const ranked = [...pairs].sort((x, y) => {
    if (x.alarm !== y.alarm) return x.alarm ? -1 : 1;
    const rx = x.identicalRate ?? -1;
    const ry = y.identicalRate ?? -1;
    if (rx !== ry) return ry - rx;
    return y.comparable - x.comparable;
  });
  return ranked[0];
}

/** 散点图坐标系（归一化到中位价，后端已裁剪至 [0,3]）。 */
export const SCATTER_MAX = 3;

/** 散点 → SVG 视口坐标（左下原点；size 为绘图区边长，单位 px）。 */
export function toSvgXY(p: BoqScatterPoint, size: number): { cx: number; cy: number } {
  const clamp = (v: number) => Math.max(0, Math.min(SCATTER_MAX, v));
  return {
    cx: (clamp(p.x) / SCATTER_MAX) * size,
    cy: size - (clamp(p.y) / SCATTER_MAX) * size,
  };
}

/**
 * 折扣带：等比规律时高亮 y = a·x 的直线带（|a−1| 太小则退化成对角线，不再单独高亮）。
 * 返回 null = 不画带（无规律性结论或就是对角线本身）。
 */
export function discountBand(
  kind: BoqPatternKind | string | null | undefined,
  a: number | null | undefined,
): { slope: number } | null {
  if (kind !== "geo_discount" || a == null || !Number.isFinite(a)) return null;
  if (Math.abs(a - 1) < 1e-4) return null;
  if (a <= 0) return null;
  return { slope: a };
}

/** 百分比文案（雷同率/占比统一口径：一位小数）。 */
export function pct1(v: number): string {
  return `${(v * 100).toFixed(1)}%`;
}
