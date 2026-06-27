// 残留共享类型（与 src-tauri/src/engine/report.rs 的 serde camelCase 输出对应）。
// 结果屏 Matrix/Compare/Export 已原生消费 src/api/types.ts 的 DTO（useJobReport 适配器已移除）；
// 此处仅保留仍被各屏引用的 Fingerprint / DiffOp / Collusion，以及文件选择工具 pickBidFiles。
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

export interface DiffOp {
  op: "eq" | "ins" | "del";
  text: string;
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
