// 前端 ↔ Rust 引擎调用层。
// 注意：invoke / dialog 仅在真实 Tauri 窗口内有效；浏览器预览里 isTauri() 为 false。
import { invoke, Channel } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

// 与 src-tauri/src/engine/report.rs 的 serde(camelCase) 输出一一对应
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
  kind: string; // similarity | cluster | metadata | sharedTerms
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

export const SECTION_LABEL: Record<string, string> = {
  tech: "技术标",
  business: "商务标",
  other: "其他",
};

export interface Progress {
  stage: "parse" | "compare" | "cluster" | "done" | string;
  done: number;
  total: number;
  note: string;
}

/** 是否运行在真实 Tauri 窗口内（用于在浏览器预览中优雅降级到演示数据）。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 弹出系统文件选择框，选择 2-5 份标书，返回绝对路径数组。 */
export async function pickBidFiles(): Promise<string[]> {
  const sel = await open({
    multiple: true,
    title: "选择 2 至 5 份标书",
    filters: [{ name: "标书文件", extensions: ["docx", "pdf", "txt", "md"] }],
  });
  if (!sel) return [];
  return Array.isArray(sel) ? sel : [sel];
}

/** 调用 Rust 引擎做交叉比对；templates 为查重源样板（命中则剔除）；onProgress 实时回传进度。 */
export function runAnalysis(
  paths: string[],
  templates: string[],
  semantic: boolean,
  threshold: number,
  scope: string,
  onProgress?: (p: Progress) => void,
): Promise<Report> {
  const channel = new Channel<Progress>();
  if (onProgress) channel.onmessage = onProgress;
  return invoke<Report>("analyze_paths", {
    paths,
    templates,
    semantic,
    threshold,
    scope,
    onProgress: channel,
  });
}

// —— 历史任务持久化 ——
export interface TaskSummary {
  id: string;
  name: string;
  createdAt: number;
  docCount: number;
  pairCount: number;
  clusterCount: number;
  peak: number;
  collusionLevel?: string; // high|medium|low|none（旧任务可能缺，回落用 peak）
  matrix: number[][];
}

export function saveTask(name: string, report: Report): Promise<string> {
  return invoke<string>("save_task", { name, report });
}
export function listTasks(): Promise<TaskSummary[]> {
  return invoke<TaskSummary[]>("list_tasks");
}
export function getTask(id: string): Promise<Report> {
  return invoke<Report>("get_task", { id });
}
export function deleteTask(id: string): Promise<void> {
  return invoke<void>("delete_task", { id });
}

export type ExportKind = "xlsx" | "docx" | "html";

const EXPORT_META: Record<ExportKind, { title: string; name: string; ext: string; cmd: string }> = {
  xlsx: { title: "导出 Excel 报告", name: "标书查重报告.xlsx", ext: "xlsx", cmd: "export_excel" },
  docx: { title: "导出 Word 报告", name: "标书查重报告.docx", ext: "docx", cmd: "export_docx" },
  html: { title: "导出网页报告（可打印为 PDF）", name: "标书查重报告.html", ext: "html", cmd: "export_html" },
};

/** 导出报告（xlsx / docx / html）；弹保存框选路径，返回保存路径（取消则 null）。 */
export async function exportReport(report: Report, kind: ExportKind): Promise<string | null> {
  const meta = EXPORT_META[kind];
  const path = await save({
    title: meta.title,
    defaultPath: meta.name,
    filters: [{ name: meta.title, extensions: [meta.ext] }],
  });
  if (!path) return null;
  await invoke<void>(meta.cmd, { report, path });
  return path;
}

/** 兼容旧调用：导出 Excel。 */
export function exportExcel(report: Report): Promise<string | null> {
  return exportReport(report, "xlsx");
}
