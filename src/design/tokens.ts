// 设计令牌 —— 移植自 app-design/project/src/c/tokens.jsx
// 冷静、克制、办公专业感，大量留白。

export const C = {
  font: '"Inter","Noto Sans SC",-apple-system,BlinkMacSystemFont,"PingFang SC","Microsoft YaHei UI","Segoe UI",system-ui,sans-serif',
  serif: '"Noto Serif SC","Songti SC","STSong",Georgia,serif',
  mono: '"JetBrains Mono","SF Mono","Cascadia Code",Menlo,Consolas,monospace',

  // 中性色 —— 微暖白 + 石板墨
  ink: "#16161B",
  ink2: "#3A3A44",
  ink3: "#6B6B76",
  ink4: "#A0A0AB",
  ink5: "#CECED5",
  line: "#ECEAE3",
  line2: "#F4F2EB",
  paper: "#FAFAF7",
  paper2: "#F4F2EB",
  paper3: "#EEEBE2",
  white: "#FFFFFF",

  // 主色 —— 静谧靛蓝
  brand: "#4F58A8",
  brandSoft: "#EEEFF9",
  brandInk: "#363D7A",

  // 语义色 —— 降饱和
  ok: "#3A8F5F",
  okSoft: "#E4F0E8",
  warn: "#C28430",
  warnSoft: "#F7ECD7",
  danger: "#B54545",
  dangerSoft: "#F6E2E2",

  // 相似度高亮（米底上的珊瑚/琥珀）
  hi1: "#E3C28A",
  hi1Soft: "#FAF1DC", // 低
  hi2: "#E0A064",
  hi2Soft: "#F8E3CB", // 中
  hi3: "#D67A5E",
  hi3Soft: "#F6D4C8", // 高
  hi4: "#B85546",
  hi4Soft: "#F1C7C2", // 极高

  shadow: {
    xs: "0 1px 0 rgba(20,18,14,0.04)",
    sm: "0 1px 2px rgba(20,18,14,0.05), 0 1px 1px rgba(20,18,14,0.03)",
    md: "0 6px 18px rgba(20,18,14,0.06), 0 1px 3px rgba(20,18,14,0.04)",
    lg: "0 16px 42px rgba(20,18,14,0.08), 0 2px 6px rgba(20,18,14,0.04)",
  },

  radius: { sm: 4, md: 6, lg: 8, xl: 12, xxl: 16, pill: 999 },
} as const;

// 文档类型 chip 配色
export interface DocChipStyle {
  fg: string;
  bg: string;
  label: string;
}
const DOC_CHIPS: Record<string, DocChipStyle> = {
  docx: { fg: "#1F4D8F", bg: "#E5EDF8", label: "DOCX" },
  pdf: { fg: "#9A3838", bg: "#F4DDDD", label: "PDF" },
  txt: { fg: "#5E5651", bg: "#ECE6DD", label: "TXT" },
  md: { fg: "#3A3A44", bg: "#E8E5DC", label: "MD" },
  ppt: { fg: "#A26425", bg: "#F6E4CC", label: "PPT" },
  pptx: { fg: "#A26425", bg: "#F6E4CC", label: "PPT" },
  xls: { fg: "#1F6E3D", bg: "#DCEFE0", label: "XLSX" },
  xlsx: { fg: "#1F6E3D", bg: "#DCEFE0", label: "XLSX" },
};
export function docChip(type: string): DocChipStyle {
  return DOC_CHIPS[type.toLowerCase()] ?? DOC_CHIPS.txt;
}

// 颜色提亮/压暗（用于按钮内阴影），移植自 shell.jsx 的 shadeC
export function shadeC(hex: string, amt: number): string {
  const h = hex.replace("#", "");
  const num = parseInt(h, 16);
  let r = (num >> 16) + amt * 2;
  let g = ((num >> 8) & 0xff) + amt * 2;
  let b = (num & 0xff) + amt * 2;
  r = Math.max(0, Math.min(255, r));
  g = Math.max(0, Math.min(255, g));
  b = Math.max(0, Math.min(255, b));
  return "#" + ((r << 16) | (g << 8) | b).toString(16).padStart(6, "0");
}

// 相似度 → 高亮档位颜色（迷你矩阵/构建矩阵共用）
export function severityColor(v: number, paperFallback: string): string {
  if (v >= 0.9) return C.hi4;
  if (v >= 0.7) return C.hi3;
  if (v >= 0.5) return C.hi2;
  if (v >= 0.3) return C.hi1;
  return paperFallback;
}
