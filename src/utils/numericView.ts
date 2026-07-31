// 商务标数值证据（W5-4/W5-6）的纯派生逻辑 —— 与 React 解耦，便于单测。
// 铁律：这里只做「把后端事实排布成可视形状」，不重算任何指标、不改判任何结论。
import type {
  BoqCorrelationDto,
  BoqPatternKind,
  BoqScatterPoint,
  EvaluationConfigDto,
  NumericPairDto,
} from "../api/types";

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

// ── 机制感知筛查（W5-5「基准价敏感性」）的纯派生文案 ──
// 【产品纪律】本块只作描述性解释，不参与围标分级；措辞不得弱化、不得表述为概率或认定。

/** §1.5 强制提示：本块的性质声明（与后端 mechanism::MECHANISM_NOTE 同文）。 */
export const MECHANISM_DISCLAIMER =
  "本节为反事实解释性分析，不参与围标分级；评标办法为人工录入，请核对公式与参数。";

/**
 * 评标办法公式全文（配置页在【发起前】回显，与后端 EvaluationConfig::formula_text 同文案）——
 * 人工录错公式会让整节结论失真，必须让用户在发起前逐字核对。
 */
export function mechanismFormulaText(ev: EvaluationConfigDto): string {
  if (ev.method === "lowest") {
    return "最低评标价法：投标总价最低者价格分最高。本节只作「最低价孤立度」描述，不计算均值基准价。";
  }
  return (
    `基准价 =（全部有效投标总价去掉 ${ev.trimHighest} 个最高、${ev.trimLowest} 个最低后的算术平均）` +
    `× 系数 c，c ∈ [${ev.coeffMin.toFixed(4)}, ${ev.coeffMax.toFixed(4)}]；投标总价最接近基准价者价格分最高。`
  );
}

/** 评标办法参数是否可提交（与后端校验同口径：不合法直接拒绝，不静默纠正）。 */
export function evaluationError(ev: EvaluationConfigDto, docCount: number): string | null {
  if (ev.method === "lowest") return null;
  if (!(ev.coeffMin > 0) || !(ev.coeffMax >= ev.coeffMin) || ev.coeffMax > 2) {
    return "系数区间不合法：须满足 0 < 下限 ≤ 上限 ≤ 2";
  }
  if (ev.trimLowest + ev.trimHighest >= docCount) {
    return `去高（${ev.trimHighest}）与去低（${ev.trimLowest}）之和须小于参评份数（${docCount}）`;
  }
  return null;
}

/** 报价分布端点标签（与后端 mechanism_position_cn 同文案）。 */
export function mechanismPositionLabel(position: string): string {
  if (position === "lowest") return "报价最低端";
  if (position === "highest") return "报价最高端";
  return position;
}

/** 投标总价来源标签回落（后端已下发 sourceLabel，缺失时按稳定标识兜底）。 */
export function priceSourceLabel(source: string, label?: string): string {
  if (label) return label;
  if (source === "totalRow") return "取自投标总价行";
  if (source === "boqSum") return "取自清单合计";
  if (source === "heuristic") return "启发式回落（全文最大金额）";
  return source;
}

/** 一组反事实结论的口头表述（【占比】而非概率，措辞不得改写成「概率/显著」）。 */
export function flipStatement(flipProb: number): string {
  return `若剔除该组，中标人在 ${pct1(flipProb)} 的系数取值下改变`;
}

/** 基准价偏移文案（带符号，正 = 剔除后基准价上移）。 */
export function shiftLabel(pct: number): string {
  return `${pct >= 0 ? "+" : "−"}${Math.abs(pct).toFixed(2)}%`;
}
