// 新通路 DTO（与 Rust serde camelCase 输出一一对应）。
import type { DiffOp } from "../engine";

export interface WorkspaceDto {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  settingsJson?: string;
  documentCount: number;
  latestJobStatus: string | null;
}

export interface DocumentDto {
  id: string;
  workspaceId: string;
  fileName: string;
  filePath: string;
  fileHash: string;
  fileType: string;
  status: "pending" | "parsing" | "parsed" | "failed" | string;
  parseError: string | null;
  parseMethod: string | null;
  pageCount: number | null;
  charCount: number | null;
  fingerprintJson: string | null;
  chunkCount: number;
  createdAt: string;
  updatedAt: string;
  /** 解析期「内容不完整」告知语（扫描件超页 / docx XML 截断）；前端以警示条展示。 */
  truncationNotice: string | null;
}

export interface JobDto {
  id: string;
  workspaceId: string;
  jobType: "import" | "compare" | "export" | string;
  name: string | null;
  status: "pending" | "running" | "cancelling" | "cancelled" | "failed" | "completed" | string;
  configJson: string;
  progress: number;
  message: string | null;
  errorMessage: string | null;
  errorCode: string | null;
  starred: boolean;
  matrixJson: string | null;
  collusionLevel: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface TemplateDto {
  id: string;
  name: string;
  text: string;
  /** 分类（旧行/未填为 null，前端归一显示「未分类」）。 */
  category: string | null;
  enabled: boolean;
  createdAt: string;
  /** 命中过该样板的文档数（仅反映重新导入后记录的命中）。 */
  hitCount: number;
}

/** 批量导入解析后的单条。 */
export interface NewTemplateDto {
  category?: string | null;
  name: string;
  text: string;
}

export interface BatchTemplateResult {
  inserted: number;
  skipped: number;
}

export interface EmbedModelInfo {
  key: string;
  label: string;
}

/** OCR 档位（PP-OCRv6 tiny/small/medium）。 */
export interface OcrModelInfo {
  key: string;
  label: string;
  sizeLabel: string;
  bundled: boolean;
  present: boolean;
}

export interface AppInfoDto {
  version: string;
  maxDocs: number;
  minDocs: number;
  embeddingModels: EmbedModelInfo[];
  ocrModels: OcrModelInfo[];
  defaultOcrModel: string;
}

export interface ProgressEvent {
  jobId: string;
  jobType: string;
  stage: string;
  message: string;
  current: number;
  total: number;
  percent: number;
}

export interface TerminalEvent {
  jobId: string;
  jobType: string;
  status: "completed" | "failed" | "cancelled" | string;
  errorCode?: string;
  errorMessage?: string;
}

export interface CompareRequest {
  documentIds: string[];
  name?: string;
  baseDocumentId?: string;
  chunkLevel?: "section" | "paragraph" | "sentence";
  similarityThreshold?: number;
  candidateTopK?: number;
  enableSemantic?: boolean;
  enableFactConflict?: boolean;
  ignoreTemplates?: boolean;
  detectMovedParagraph?: boolean;
  scope?: "full" | "tech" | "business";
  embeddingModel?: string;
}

export interface CompareSummaryDto {
  job: JobDto;
  documents: DocumentDto[];
  config: Record<string, unknown> & { documentIds?: string[] };
  summary: CompareSummary | null;
  matrix: { documentIds: string[]; matrix: number[][]; peak: number } | null;
  collusion: Record<string, unknown> | null;
  sharedTerms: unknown[] | null;
  sections: unknown[] | null;
}

export interface CompareSummary {
  documentCount: number;
  chunkCount: number;
  clusterCount: number;
  sameCount: number;
  minorChangeCount: number;
  rewriteCount: number;
  changedCount: number;
  addedCount: number;
  deletedCount: number;
  conflictCount: number;
  uncertainCount: number;
  highRiskCount: number;
  semanticDegraded: boolean;
}

export interface ClusterSummaryDto {
  id: string;
  jobId: string;
  clusterType: string;
  topic: string | null;
  summary: string | null;
  severity: string | null;
  score: number | null;
  sectionKind: string | null;
  reviewStatus: "pending" | "confirmed" | "ignored" | string;
  /** 底版分块位置：「第一章 › 1.1 报价」 */
  sectionPath: string | null;
  page: number | null;
  documentIds: string[];
  memberCount: number;
}

export interface PageResult<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface ClusterFilter {
  clusterType?: string;
  severity?: string;
  reviewStatus?: string;
  sectionKind?: string;
  documentId?: string;
}

export interface MemberDetailDto {
  documentId: string;
  documentName: string;
  chunkId: string;
  text: string;
  sectionPath: string | null;
  sectionKind: string | null;
  page: number | null;
  orderIndex: number;
  role: "primary" | "duplicate_candidate" | "missing" | string;
  score: number | null;
}

export interface DiffRowDto {
  baseChunkId: string | null;
  targetChunkId: string | null;
  diffType: "char" | "word" | "sentence" | string;
  diffJson: string;
  summary: string | null;
}

export interface FactRowDto {
  chunkId: string;
  documentId: string;
  subject: string | null;
  action: string | null;
  object: string | null;
  amount: string | null;
  date: string | null;
  duration: string | null;
  percentage: string | null;
  condition: string | null;
  obligationType: string | null;
  confidence: number | null;
}

export interface ClusterDetailDto {
  cluster: ClusterSummaryDto;
  members: MemberDetailDto[];
  diffs: DiffRowDto[];
  facts: FactRowDto[];
  conflictJson: string | null;
}

export interface PairMatchDto {
  textA: string;
  textB: string;
  score: number;
  diffType: string;
  diff: DiffOp[];
}

export interface DocumentPreviewDto {
  document: DocumentDto;
  chunks: Array<{
    id: string;
    documentId: string;
    chunkType: string;
    sectionPath: string | null;
    sectionKind: string | null;
    text: string;
    page: number | null;
    orderIndex: number;
  }>;
}

/** 批注（评审记录，叠加于预览，原文件只读）。 */
export interface AnnotationDto {
  id: string;
  workspaceId: string;
  documentId: string | null;
  chunkId: string | null;
  clusterId: string | null;
  page: number | null;
  quote: string | null;
  note: string;
  createdAt: string;
  updatedAt: string;
}

/** 扫描件 OCR 的一行识别文本（页内归一化坐标 0..1，原点左上）。 */
export interface OcrLine {
  t: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

// —— 工具：模型 / 存储 / 自检 ——
export interface EmbedModelStatus {
  key: string;
  label: string;
  cached: boolean;
  sizeBytes: number;
}
export interface ModelStatusDto {
  ocrPresent: boolean;
  ocrLocation: string | null;
  embedCacheDir: string | null;
  embeddingModels: EmbedModelStatus[];
}
export interface StorageInfoDto {
  dbBytes: number;
  embeddingRows: number;
  documentCount: number;
  jobCount: number;
}
export interface DiagnosticItem {
  key: string;
  label: string;
  ok: boolean;
  detail: string;
}

// —— 授权 / 激活（与 Rust license::LicenseStatus 对应）——
export interface LicenseStatusDto {
  /** trial | licensed | grace | expired | exhausted | machineMismatch | unlicensed */
  state: string;
  /** 是否可用（trial / licensed / grace）；false 时路由守卫拦到激活页 */
  active: boolean;
  plan: string | null;
  licenseeName: string | null;
  expiresAt: string | null;
  /** null = 不限次 */
  remainingUses: number | null;
  usedUses: number | null;
  trialExpiresAt: string | null;
  machineCode: string;
  clockTamper: boolean;
  tamper: boolean;
  message: string | null;
}
