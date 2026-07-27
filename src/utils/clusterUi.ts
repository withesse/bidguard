// 八类差异分类与风险等级的展示约定（文案/配色）——单一来源。
import { C } from "../design/tokens";

// 相似度百分比 → 分档（颜色 + 文案）：矩阵/逐对/配置共用，避免 80/60/30 阈值各写一份。
export function simBand(pct: number): { color: string; label: string } {
  if (pct >= 80) return { color: C.danger, label: "高度雷同" };
  if (pct >= 60) return { color: C.hi3, label: "高相似" };
  if (pct >= 30) return { color: C.hi2, label: "中相似" };
  return { color: C.hi1, label: "低相似" };
}

export const TYPE_UI: Record<string, { label: string; fg: string; bg: string }> = {
  same: { label: "相同", fg: "#75646C", bg: "rgba(117,100,108,0.10)" },
  minor_change: { label: "轻微修改", fg: "#8a6d3b", bg: "rgba(194,132,48,0.12)" },
  rewrite: { label: "改写", fg: "#8A5BA6", bg: "rgba(138,91,166,0.12)" },
  changed: { label: "修改", fg: "#B06A3B", bg: "rgba(176,106,59,0.12)" },
  added: { label: "基准缺失", fg: "#4A7FB5", bg: "rgba(74,127,181,0.12)" },
  deleted: { label: "基准独有", fg: "#4A7FB5", bg: "rgba(74,127,181,0.12)" },
  conflict: { label: "事实冲突", fg: "#B54545", bg: "rgba(181,69,69,0.12)" },
  uncertain: { label: "待复核", fg: "#75646C", bg: "rgba(117,100,108,0.12)" },
};

export function typeUi(t: string) {
  return TYPE_UI[t] ?? { label: t, fg: "#75646C", bg: "rgba(117,100,108,0.10)" };
}

export const SEVERITY_UI: Record<string, { label: string; fg: string; bg: string }> = {
  high: { label: "高风险", fg: "#B54545", bg: "rgba(181,69,69,0.14)" },
  medium: { label: "中风险", fg: "#B06A3B", bg: "rgba(176,106,59,0.14)" },
  low: { label: "低风险", fg: "#8a6d3b", bg: "rgba(194,132,48,0.10)" },
  review: { label: "需人工", fg: "#6B73C9", bg: "rgba(107,115,201,0.12)" },
  none: { label: "", fg: "", bg: "" },
};

export function severityUi(s: string | null | undefined) {
  return s ? SEVERITY_UI[s] ?? null : null;
}

export const REVIEW_UI: Record<string, { label: string; fg: string }> = {
  pending: { label: "待确认", fg: "#75646C" },
  confirmed: { label: "已确认", fg: "#0E9A8F" },
  ignored: { label: "已忽略", fg: "#75646C" },
};

// 五区（W3-5）章节分区展示约定：徽标文案 + 配色，ClustersScreen 筛选与 ClusterDetail 徽标共用。
export const ZONE_UI: Record<string, { label: string; fg: string; bg: string }> = {
  tech: { label: "技术标", fg: "#4A7FB5", bg: "rgba(74,127,181,0.12)" },
  business: { label: "商务标", fg: "#8A5BA6", bg: "rgba(138,91,166,0.12)" },
  legal: { label: "法定格式", fg: "#0E9A8F", bg: "rgba(14,154,143,0.12)" },
  price: { label: "报价清单", fg: "#B06A3B", bg: "rgba(176,106,59,0.12)" },
  other: { label: "其他", fg: "#75646C", bg: "rgba(117,100,108,0.10)" },
};

export function zoneUi(s: string | null | undefined) {
  return (s && ZONE_UI[s]) || ZONE_UI.other;
}

// 复核路由三带（W6-4）：文案是产品硬约束（方案 §1.5-1），三处必须一字不差——
// 「低优先级抽查 / 需人工复核 / 重点标红」。【禁用「自动放行」「漏检保证」字样】：
// 共形保证只在合成校准语料的分布上成立，真实标书分布漂移时失效，在监管场景把统计假设
// 讲成对评标方的承诺有法律暴露。band=null（旧任务/未校准）走 uncalibrated 档，
// 显示「未校准」而不是留空——空白会被读成「没问题」。
export const BAND_UI: Record<string, { label: string; fg: string; bg: string; hint: string }> = {
  flag: {
    label: "重点标红",
    fg: "#B54545",
    bg: "rgba(181,69,69,0.14)",
    hint: "校准概率高于高位阈值：建议优先复核",
  },
  review: {
    label: "需人工复核",
    fg: "#6B73C9",
    bg: "rgba(107,115,201,0.12)",
    hint: "校准概率位于两条阈值之间：判读不确定，需人工复核",
  },
  pass: {
    label: "低优先级抽查",
    fg: "#75646C",
    bg: "rgba(117,100,108,0.10)",
    hint: "校准概率低于低位阈值：默认排在最后并折叠，条款仍完整保留、可展开可导出，建议抽查",
  },
  uncalibrated: {
    label: "未校准",
    fg: "#75646C",
    bg: "rgba(117,100,108,0.08)",
    hint: "本次比对未启用概率校准（旧任务或校准文件不可用）：按既有风险等级复核",
  },
};

export function bandUi(band: string | null | undefined) {
  return BAND_UI[band ?? "uncalibrated"] ?? BAND_UI.uncalibrated;
}

// cross-encoder 复核建议（W6-2）：【这是排序建议，不是判读结论】。产品纪律 §1.5-3——
// cross-encoder 是黑盒模型且训练目标是「检索相关性」，「相关」≠「同源改写」，因此它【不改判
// 分类】：簇仍是「待复核」，UI 只展示倾向与分数，人工确认后才改分类。文案里一律不出现
// 「判定 / 认定 / 确认为」这类断言词。
export const RERANK_DISCLAIMER = "仅为复核排序建议，不改变条款分类，结论需人工确认";

/** 复核建议分 → 倾向徽标（label 形如「AI 复核倾向：洗稿（0.83）」）。null = 未复核。 */
export function rerankUi(
  score: number | null | undefined,
): { label: string; fg: string; bg: string; hint: string } | null {
  if (score == null || !Number.isFinite(score)) return null;
  const lean = score >= 0.5 ? "洗稿" : "无关";
  const strong = score >= 0.5;
  return {
    label: `AI 复核倾向：${lean}（${score.toFixed(2)}）`,
    fg: strong ? "#8A5BA6" : "#75646C",
    bg: strong ? "rgba(138,91,166,0.12)" : "rgba(117,100,108,0.10)",
    hint: `交叉编码器复核建议分 ${score.toFixed(2)}：${RERANK_DISCLAIMER}`,
  };
}

/** 三带/置信度相关文案的强制限定语（§1.5-1/2）：一处定义，各屏引用。 */
export const CALIBRATION_QUALIFIER =
  "在合成校准语料上校准，仅用于复核排序参考，不是串通概率";

/** 分流模式说明（review-all = 本版语料不足以支撑分带，全部按需人工复核）。 */
export function routingNote(routing: string | undefined, alpha?: number, beta?: number): string {
  if (routing === "three-band") {
    const a = Math.round((alpha ?? 0) * 100);
    const b = Math.round((beta ?? 0) * 100);
    return `三带分流已启用：低优先级抽查带漏检率目标 α=${a}%、重点标红带误报率目标 β=${b}%，均为在合成校准语料上测得的带内错误率，不是对真实标书的承诺；低优先级抽查带只排序与折叠，不隐藏任何条款。`;
  }
  if (routing === "review-all") {
    return "三带分流未启用：本版校准语料不含「独立编制但表面相似」的难负样本，相似度分在簇的分数区间内无分辨力，据此分流会把几乎所有条款推进同一条带。全部条款按「需人工复核」处理，置信度仅作参考。";
  }
  return "本次比对未启用概率校准（旧任务或校准文件不可用）：条款按既有风险等级复核。";
}
