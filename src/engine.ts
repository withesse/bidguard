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
  // —— M1 取证扩展（可选：旧任务的 fingerprint_json 无这些字段）——
  /** w:rsids 修订会话标识（去重、大写归一） */
  rsids?: string[];
  /** w:rsidRoot：相同即高度指示派生自同一母文件 */
  rsidRoot?: string | null;
  /** docProps/app.xml Template 模板名 */
  templateName?: string | null;
  /** zip 条目序列 sha256（同一生成工具/打包管线稳定一致） */
  zipEntryFp?: string | null;
  zipEntryCount?: number | null;
  // —— M1 PDF 血缘取证 ——
  /** PDF trailer /ID 首半（hex）：创建时生成、再保存不变的血缘键 */
  pdfIdFirst?: string | null;
  /** PDF trailer /ID 次半（hex）：每次保存变化，供人工核对 */
  pdfIdSecond?: string | null;
  /** XMP xmpMM:DocumentID 文档 GUID */
  xmpDocumentId?: string | null;
  /** XMP xmpMM:InstanceID 保存实例 GUID */
  xmpInstanceId?: string | null;
  /** XMP xmpMM:DerivedFrom → 母文件 GUID */
  xmpDerivedFrom?: string | null;
  /** XMP xmp:CreatorTool 生成工具 */
  creatorTool?: string | null;
  /** 逐页 BaseFont 全集（去重排序） */
  pdfFonts?: string[];
  /** 子集内嵌字体标签（如 ABCDEF+SimSun） */
  fontSubsetTags?: string[];
}

export interface DiffOp {
  op: "eq" | "ins" | "del";
  text: string;
}

export interface CollusionSignal {
  // similarity | cluster | metadata | sharedTerms | facts
  // | rsid | pdfLineage | imageReuse | sharedErrors（M1 取证）| evasion（M2 规避）
  kind: string;
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
