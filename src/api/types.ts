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

/** 文档角色：bid 投标（默认）| tender 招标文件 | tender_supplement 补遗/答疑。 */
export type DocRole = "bid" | "tender" | "tender_supplement";

/** 规避特征判级摘要（镜像 Rust engine::report::EvasionSummary，serde camelCase）。
 *  §1.5：severity 仅驱动呈现，机器不下「规避/串通/清白」定性；仅 confirmed 打徽标/挂告警条。 */
export interface EvasionSummaryDto {
  zeroWidth: number;
  bidi: number;
  tags: number;
  variation: number;
  confusableFolds: number;
  mixedScriptWords: number;
  affectedChunks: number;
  maxChunkConcentration: number;
  pdfHiddenRatio: number;
  pdfHiddenChars: number;
  /** 渲染-OCR 交叉验证命中机器标识（fontRemap/coordShuffle）；null=未命中/未做，不作清白背书。 */
  xcheckKind: string | null;
  xcheckLabel: string | null;
  severity: "none" | "suspect" | "confirmed" | string;
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
  /** 规避特征判级摘要（M2 入口对抗层）；null=无发现或旧任务导入。仅 confirmed 触发文档卡徽标/告警条。 */
  evasionSummary: EvasionSummaryDto | null;
  /** 文档角色；招标类（tender/tender_supplement）不可勾选参评、不占参评名额。 */
  docRole: DocRole | string;
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
  /** 剔除招标文件内容（W3-2 招标对减）：默认 true；工作区无招标文件时自然空转。 */
  subtractTender?: boolean;
  embeddingModel?: string;
}

export interface CompareSummaryDto {
  job: JobDto;
  documents: DocumentDto[];
  config: Record<string, unknown> & { documentIds?: string[] };
  summary: CompareSummary | null;
  matrix: {
    documentIds: string[];
    matrix: number[][];
    peak: number;
    /** 未对减口径（原始相似度）；旧任务缺键。M4 起填充。 */
    matrixOriginal?: number[][];
    peakOriginal?: number;
    /** 区段口径（对齐区段按 chunk 去重后覆盖率）；旧任务/无区段缺键或全 0。M5 起填充。 */
    segmentMatrix?: number[][];
    segmentPeak?: number;
    /** 前端主显口径："cluster" | "segment"；旧任务缺键按 cluster 渲染。 */
    mode?: string;
  } | null;
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
  /** 被识别为「引用招标文件」并从残差比对剔除的投标分块数（W3-2）；0 表示无对减。 */
  tenderRefChunkCount: number;
  /** 被内置静态范本背景库判为「行业范本套话」并从聚类剔除的分块数（W3-4）；仅 ignoreTemplates 开启时非零。 */
  backgroundExemptChunkCount: number;
  /** 分区分层五区簇计数（W3-5）：五者之和恒等于 clusterCount。legal 区阈值已上调；price 区证据主体为金额事实冲突。 */
  zoneLegalCount: number;
  zonePriceCount: number;
  zoneTechCount: number;
  zoneBusinessCount: number;
  zoneOtherCount: number;
}

export interface ClusterSummaryDto {
  id: string;
  jobId: string;
  clusterType: string;
  topic: string | null;
  summary: string | null;
  severity: string | null;
  score: number | null;
  /** 五区（W3-5）：'tech' | 'business' | 'legal'(法定格式·阈值上调) | 'price'(报价清单) | 'other'。 */
  sectionKind: string | null;
  reviewStatus: "pending" | "confirmed" | "ignored" | string;
  /** 底版分块位置：「第一章 › 1.1 报价」 */
  sectionPath: string | null;
  page: number | null;
  documentIds: string[];
  memberCount: number;
  /** k-共现查证（W3-3）：命中招标（'tender'）/背景库（'background'）的合法共享出处 → UI 置灰；null=未豁免。 */
  exemptReason: string | null;
  /** k-共现查证（W3-3）：『多家异常一致·待复核』——红色徽标、涉嫌措辞，需评标委员会依法认定。 */
  multiDocAnomaly: boolean;
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
  /** 五区筛选（W3-5）：'tech' | 'business' | 'legal' | 'price' | 'other'。 */
  sectionKind?: string;
  documentId?: string;
  /** 按豁免出处筛选（W3-3）：'tender' | 'background'。 */
  exemptReason?: string;
  /** 仅『多家异常一致·待复核』簇（W3-3）。 */
  multiDocAnomaly?: boolean;
  /** 仅『恰好两家共有』簇（W3-3 首要证据视图）。 */
  twoDocsOnly?: boolean;
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
  /** 引用招标文件覆盖率（W3-2）：命中招标指纹的字符占比；非豁免块为 null。≥0.8 显示徽标。 */
  tenderCoverage: number | null;
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

// —— 对齐区段（W4-5，M5b）：新增证据层，只读，与聚类经 chunk_id 互链 ——

/** 区段列表摘要行（镜像 Rust segment_repo::SegmentSummaryRow）。 */
export interface AlignedSegmentDto {
  id: string;
  docAId: string;
  docBId: string;
  anchorCount: number;
  verbatimChars: number;
  aCoveredChars: number;
  bCoveredChars: number;
  aCoverage: number;
  bCoverage: number;
  avgScore: number;
  aSectionPath: string | null;
  bSectionPath: string | null;
  aPageStart: number | null;
  aPageEnd: number | null;
  bPageStart: number | null;
  bPageEnd: number | null;
}

/** 区段头（aligned_segments 整行）。 */
export interface SegmentHeadDto {
  id: string;
  jobId: string;
  docAId: string;
  docBId: string;
  aStartChunkId: string;
  aEndChunkId: string;
  bStartChunkId: string;
  bEndChunkId: string;
  anchorCount: number;
  verbatimChars: number;
  aCoveredChars: number;
  bCoveredChars: number;
  aCoverage: number;
  bCoverage: number;
  avgScore: number;
  aSectionPath: string | null;
  bSectionPath: string | null;
  aPageStart: number | null;
  aPageEnd: number | null;
  bPageStart: number | null;
  bPageEnd: number | null;
}

/** 区段跨度内一个 chunk（双栏按 order 顺序渲染）。tenderCoverage≥0.8 显示「引用招标文件」徽标。 */
export interface SegmentChunkDto {
  chunkId: string;
  text: string;
  page: number | null;
  sectionPath: string | null;
  orderIndex: number;
  tenderCoverage: number | null;
}

/** 区段内一条链化锚点。kind: edge 残差边 | soft 软种子 | verbatim 逐字铁证。 */
export interface SegmentAnchorDto {
  aChunkId: string;
  bChunkId: string;
  kind: string;
  score: number;
}

/** 逐字铁证区间（深红底）。offset 按原文 char 计，chunk.text 的 char 切片 [startOffset,endOffset) 即匹配文本。 */
export interface VerbatimIntervalDto {
  id: string;
  docAId: string;
  docBId: string;
  aStartChunkId: string;
  aStartOffset: number;
  aEndChunkId: string;
  aEndOffset: number;
  bStartChunkId: string;
  bStartOffset: number;
  bEndChunkId: string;
  bEndOffset: number;
  charLen: number;
  sampleText: string;
  segmentId: string | null;
}

/** 区段内一条 gap 细化产物（黄底差异）。diffJson=DiffOp 序列。 */
export interface SegmentGapDiffDto {
  aChunkId: string | null;
  bChunkId: string | null;
  diffType: string;
  diffJson: string;
  eqChars: number;
}

/** 区段详情（双栏高亮 + 反向互链所需的全部只读数据）。 */
export interface SegmentDetailDto {
  segment: SegmentHeadDto;
  aChunks: SegmentChunkDto[];
  bChunks: SegmentChunkDto[];
  anchors: SegmentAnchorDto[];
  verbatims: VerbatimIntervalDto[];
  diffs: SegmentGapDiffDto[];
  /** 经锚点 chunk_id 反查关联的聚类 id 集合（区段↔聚类互链）。 */
  clusterIds: string[];
}

/** 聚类反查关联区段引用（ClusterDetail「所在区段」Pill 反向互链）。 */
export interface ClusterSegmentRefDto {
  segmentId: string;
  docAId: string;
  docBId: string;
  aCoverage: number;
  bCoverage: number;
  verbatimChars: number;
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
