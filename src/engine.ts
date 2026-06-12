// 报告数据形状（与 src-tauri/src/engine/report.rs 的 serde camelCase 输出对应）。
// 结果屏 Matrix/Compare/Export 经 useJobReport 适配器消费这套 Report 形状；
// 新通路的原生 DTO 在 src/api/types.ts。
import { open } from "@tauri-apps/plugin-dialog";

export interface Fingerprint {
  author: string | null;
  lastModifiedBy: string | null;
  created: string | null;
  modified: string | null;
  app: string | null;
  revision: string | null;
  totalEditMinutes: number | null;
  riskFlags: string[];
}

export interface DocInfo {
  id: string;
  name: string;
  docType: string;
  pages: number;
  charCount: number;
  fingerprint: Fingerprint;
  parseError: string | null;
}

export interface DiffOp {
  op: "eq" | "ins" | "del";
  text: string;
}

export interface SegMatch {
  textA: string;
  textB: string;
  score: number;
  diff: DiffOp[];
}

export interface PairDetail {
  a: number;
  b: number;
  score: number;
  matches: SegMatch[];
}

export interface ClusterSeg {
  doc: number;
  text: string;
}

export interface Cluster {
  avgScore: number;
  peak: number;
  docs: number[];
  segments: ClusterSeg[];
}

export interface CollusionSignal {
  kind: string; // similarity | cluster | metadata | sharedTerms | facts
  detail: string;
  weight: number;
}

export interface Collusion {
  level: "high" | "medium" | "low" | "none" | string;
  score: number;
  signals: CollusionSignal[];
}

export interface SectionStat {
  doc: number;
  section: "tech" | "business" | "other" | string;
  intensity: number;
  matches: number;
}

export interface SharedTerm {
  term: string;
  docs: number[];
}

export interface Report {
  docs: DocInfo[];
  matrix: number[][];
  peak: number;
  pairs: PairDetail[];
  clusters: Cluster[];
  collusion?: Collusion;
  sections?: SectionStat[];
  sharedTerms?: SharedTerm[];
}

/** 弹出系统文件选择框，选择待比对的标书，返回绝对路径数组。 */
export async function pickBidFiles(): Promise<string[]> {
  const sel = await open({
    multiple: true,
    title: "选择 2 至 10 份标书",
    filters: [{ name: "标书文件", extensions: ["docx", "pdf", "txt", "md", "xlsx", "xls"] }],
  });
  if (!sel) return [];
  return Array.isArray(sel) ? sel : [sel];
}
