// 八类差异分类与风险等级的展示约定（文案/配色）——单一来源。
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
