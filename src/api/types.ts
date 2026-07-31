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
  /** 随包概率校准的只读台账（W6-4）：设置页展示，不可运行时调整（改 α 即改承诺语义）。 */
  calibration?: CalibrationInfoDto;
}

export interface CalibrationInfoDto {
  available: boolean;
  version: string;
  /** 'experimental-synthetic' = 合成语料拟合的实验性校准。 */
  kind: string;
  /** 'platt' | 'isotonic'。 */
  calibrator: string;
  /** 'three-band' = 三带分流生效；'review-all' = 分流未启用，全部按需人工复核。 */
  routing: string;
  alpha: number;
  beta: number;
  tLow: number;
  tHigh: number;
  corpusHash: string;
  note: string;
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
  /** 商务标数值层（W5-1）：识别报价清单表并跨文档对齐清单行。默认 true；
   *  仅支持 xlsx / docx / 文本 PDF 的清单表——扫描件 PDF 走 OCR 不产表格行，自然空转。 */
  enableNumeric?: boolean;
  /** 逐项单价雷同率告警线（W5-2）：默认 0.80，后端 clamp 到 0.5–1.0，随任务配置快照持久化。 */
  identicalRateAlarm?: number;
  embeddingModel?: string;
  /** 交叉复核（W6-2）：对「待复核」条款跑 cross-encoder 产出【复核建议分】。默认 false。
   *  只影响复核队列排序与倾向徽标，【不改判分类】——结论仍需人工确认。 */
  enableRerank?: boolean;
  /** 复核模型档位，默认 bge-reranker-base-int8。 */
  rerankModel?: string;
  /** 评标办法（W5-5 机制感知筛查）：【仅单次任务级】——每个项目评标办法不同，不进全局默认。
   *  缺省 = 不录入 ⇒ 不做任何反事实计算。
   *  【产品纪律】只驱动「基准价敏感性」描述性分析，不参与围标信号与分级。 */
  evaluation?: EvaluationConfigDto;
}

/** 评标办法（v1 只支持「(去 m 高 n 低后) 算术平均 × 系数，最接近基准价者价格分最高」一族；
 *  lowest 只作最低价孤立度描述）。人工录入，UI 必须回显公式全文供核对。 */
export interface EvaluationConfigDto {
  method: 'avg_benchmark' | 'lowest';
  /** 计算基准价前去掉的最低报价个数 n。 */
  trimLowest: number;
  /** 计算基准价前去掉的最高报价个数 m。 */
  trimHighest: number;
  /** 系数区间（含端点），后端在其上取 ≥200 个均匀格点逐点重算。 */
  coeffMin: number;
  coeffMax: number;
}

/** 一份投标总价及其来源打标（取自投标总价行 / 取自清单合计 / 启发式回落）。 */
export interface MechanismPriceDto {
  docIndex: number;
  total: number;
  source: string;
  sourceLabel: string;
}

/** 候选组的构造依据一条（textPeak / identicalRate / metadata）。 */
export interface MechanismBasisDto {
  kind: string;
  detail: string;
}

/** 一个候选嫌疑组的反事实结果。flipProb 是【反事实占比】，不是概率、不是显著性。 */
export interface MechanismGroupDto {
  docs: number[];
  /** 组的构造依据（必须随组展示，防循环论证观感）。 */
  basis: MechanismBasisDto[];
  flipProb: number;
  flippedPoints: number;
  /** 剔除该组后基准价相对全量的偏移（%）。 */
  benchmarkShiftPct: number;
  /** |偏移| 在同规模子集穷举中的分位。 */
  shiftPercentile: number;
  subsetsCompared: number;
  winnerFull: number;
  winnerExcluded: number;
  supportBidDocs: number[];
}

/** 均值基准价一族的反事实块（method=lowest 时缺席）。 */
export interface MechanismBenchmarkDto {
  trimLowest: number;
  trimHighest: number;
  coeffMin: number;
  coeffMax: number;
  gridPoints: number;
  coeffMid: number;
  benchmarkMid: number;
  winnerMid: number;
  groups: MechanismGroupDto[];
}

/** 最低评标价法的最低价孤立度（禁用均值类统计）。 */
export interface MechanismLowestDto {
  winner: number;
  lowest: number;
  secondLowest: number;
  gap: number;
  medianGap: number;
  isolated: boolean;
}

/** 断崖式报价（support-bid 形态）标记。 */
export interface MechanismSupportBidDto {
  docIndex: number;
  total: number;
  position: 'lowest' | 'highest' | string;
  gap: number;
  medianGap: number;
  deviationPct: number;
}

/** 机制感知筛查结果（W5-5，numeric_json.mechanism）。
 *  【仅供展示】：不参与围标分级；notes 为强制措辞，呈现层不得省略。 */
export interface MechanismDto {
  applicable: boolean;
  /** 公式不匹配 / 数据不足时的原因（此时不出任何计算结果）。 */
  notApplicableReason?: string;
  method: string;
  /** 评标办法公式全文（人工录入回显）。 */
  formula: string;
  prices: MechanismPriceDto[];
  benchmark?: MechanismBenchmarkDto;
  lowest?: MechanismLowestDto;
  supportBids: MechanismSupportBidDto[];
  notes: string[];
}

/** 一条共享算术错误（W5-2）：同一对齐清单项在两份文档中 工程量/单价/（算错的）合价三者全等。
 *  chunkIds 是双方原文锚点，可下钻 DocPreview 核对原文。 */
export interface SharedArithErrorDto {
  alignKey: string;
  name: string | null;
  qty: number;
  unitPrice: number;
  total: number;
  /** 正确值（工程量×单价）。 */
  expectedTotal: number;
  chunkIds: string[];
}

/** 规律性差异的形态（W5-3）：等差 / 等比（恒定折扣）/ 仿射。 */
export type BoqPatternKind = 'arith_seq' | 'geo_discount' | 'affine';

/** 规律性差异拟合结果（W5-3）。缺席 = 未达门槛（剔除双方相等项后 n<10 或 R²<0.999）。
 *  【定位为线索，不得表述为认定串通】：note 必须原样展示。 */
export interface BoqPatternDto {
  kind: BoqPatternKind;
  /** 最小二乘斜率（等比时即折扣系数）。 */
  a: number;
  /** 最小二乘截距（等差时即恒定差额，元）。 */
  b: number;
  r2: number;
  /** 参与拟合的条目数（已剔除双方单价到分相等的条目）。 */
  n: number;
  /** 比值向量 y/x 的变异系数；<0.5% 佐证等比。 */
  ratioCv: number | null;
  /** 差值向量 y−x 的极差（元）；<1 分佐证等差。 */
  diffRange: number;
  /** 辅证是否成立（等比看 ratioCv、等差看 diffRange）。 */
  corroborated: boolean;
  note: string;
}

/** 单价向量相关性（W5-4）。面板必须与 ratioCv、散点形态同屏展示：
 *  只有 r>0.99 且比值 CV≈0 才是强证据。 */
export interface BoqCorrelationDto {
  n: number;
  pearson: number;
  /** Spearman 秩相关（并列取均秩）。 */
  spearman: number;
  ratioCv: number | null;
  note: string;
}

/** 一个归一化散点（W5-4）：坐标 = 各自单价 / 全体投标人该项中位价，裁剪至 [0,3]。
 *  完全雷同 = 点落对角线；恒定折扣 = 平行于对角线的直线带。每对下采样至 ≤2000 点。 */
export interface BoqScatterPoint {
  alignKey: string;
  name: string | null;
  x: number;
  y: number;
}

/** 单文档单价尾数分布（W5-3）：分位/角位 χ² 均匀性 + 0/5 尾占比。
 *  （Benford 首位检验已砍——单价只跨 2–3 个数量级，前提不成立。） */
export interface BoqDigitStatsDto {
  n: number;
  centCounts: number[];
  jiaoCounts: number[];
  centChiSquare: number;
  jiaoChiSquare: number;
  /** df=9、α=0.001 的临界值。 */
  critical: number;
  zeroFiveRatio: number;
  clustered: boolean;
  note: string;
}

/** 单文档数值画像（W5-3）。digitStats 为 null = 单价样本不足，不出结论。 */
export interface NumericDocDto {
  docIndex: number;
  documentId: string;
  digitStats: BoqDigitStatsDto | null;
}

/** 一个文档对的数值统计（W5-2）。identicalRate 为 null 时 reason 给出原因
 *  （insufficient = 可比条目不足 minComparable，此时不出结论）。 */
export interface NumericPairDto {
  /** 文档在本次任务请求次序里的位次（十天干标签口径，与 matrix.documentIds 同序）。 */
  a: number;
  b: number;
  /** 可比条目数：双方均有单价、且非暂估价/信息价类的对齐项。 */
  comparable: number;
  /** 单价到分相等的条目数。 */
  identical: number;
  identicalRate: number | null;
  alarm: boolean;
  reason: string | null;
  sharedArithErrors: SharedArithErrorDto[];
  /** 规律性差异（W5-3）：null = 未达门槛。 */
  pattern?: BoqPatternDto | null;
  /** 单价向量相关性（W5-4）：null = 可比条目 <10 或方差为 0。 */
  correlation?: BoqCorrelationDto | null;
  /** 归一化散点（W5-4）：旧任务缺键。 */
  scatter?: BoqScatterPoint[];
}

/** 商务标数值证据（W5-2，jobs.numeric_json）。旧任务/无清单表为 null，前端隐藏数值面板。 */
export interface NumericDto {
  documentIds: string[];
  /** 本次任务生效的雷同率告警线（配置快照，保证报告可复现）。 */
  identicalRateAlarm: number;
  /** 出雷同率结论所需的最小可比条目数。 */
  minComparable: number;
  itemCount: number;
  alignedItemCount: number;
  pairs: NumericPairDto[];
  /** 单文档数值画像（W5-3）：与 documentIds 同序；旧任务缺键。 */
  docs?: NumericDocDto[];
  /** 机制感知筛查（W5-5）：未录入评标办法 / 旧任务 → 缺键，前端隐藏「基准价敏感性」块。 */
  mechanism?: MechanismDto | null;
  /** 强制随数据下发的措辞（§1.5）：雷同率口径说明、共享算术错误的人工核对提示、覆盖范围声明。
   *  呈现层必须原样展示，不得省略。 */
  notes: {
    identicalRate: string;
    sharedArithError: string;
    coverage: string;
  };
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
  /** 商务标数值证据（W5-2）；旧任务/无清单表为 null 或缺键 → 前端隐藏数值面板。可视化见 W5-4。 */
  numeric?: NumericDto | null;
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
  /** 商务标数值层（W5-1）：解析出的报价清单条目总数；无清单表（纯技术标/扫描件）时为 0。旧任务缺键。 */
  boqItemCount?: number;
  /** 归入跨文档对齐组（≥2 份文档共有）的条目数。 */
  boqAlignedItemCount?: number;
  /** 对齐率 = 对齐条目数 / 条目总数。对齐率本身是「同一单位编制」的结构性线索，判读需结合取证类证据。 */
  boqAlignRate?: number;
  /** 识别为报价清单的表数 / 表头未识别或列数不一致被跳过的表数。 */
  boqTableCount?: number;
  boqSkippedTableCount?: number;
  /**
   * 复核路由三带计数（W6-4）：四者之和恒等于 clusterCount。旧任务缺键 → 全部按「未校准」渲染。
   * 【低优先级抽查带只排序与折叠，不隐藏任何条款】。
   */
  bandPassCount?: number;
  bandReviewCount?: number;
  bandFlagCount?: number;
  bandUncalibratedCount?: number;
  /** 生效校准版本 / 来源标签 / 分流模式（'three-band' | 'review-all'）；空 = 本次未校准。 */
  calibrationVersion?: string;
  calibrationKind?: string;
  calibrationRouting?: string;
  /** 目标漏检率 α / 误报率 β：【在合成校准语料上测得】，不是对真实标书的承诺。 */
  calibrationAlpha?: number;
  calibrationBeta?: number;
  /** 交叉复核（W6-2）：开了复核但模型不可用（未缓存 + 离线）→ true，比对照常完成。
   *  【不静默失败】：缺失的倾向分不等于「没有嫌疑」。 */
  rerankDegraded?: boolean;
  /** 实际拿到复核建议分的簇数。 */
  rerankReviewedCount?: number;
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
  /**
   * 校准置信度（W6-4）：【在合成校准语料上校准的数值，不是串通概率】。
   * null = 未校准（旧任务或校准文件不可用）→ UI 显示「未校准」，不留空白。
   */
  confidence?: number | null;
  /** 复核路由三带码值：'pass' | 'review' | 'flag'；null = 未校准。 */
  band?: string | null;
  /** cross-encoder 复核建议分（W6-2，默认关闭）；null = 未跑复核层。 */
  rerankScore?: number | null;
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
  /** 按三带筛选（W6-4）：'pass' | 'review' | 'flag' | 'uncalibrated'。只筛不藏。 */
  band?: string;
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
/** 复核模型（cross-encoder，W6-2）：sizeLabel 是标称体积（未下载也要能看到要占多少盘），
 *  sizeBytes 是实测占用（未就绪为 0）。 */
export interface RerankModelStatus {
  key: string;
  label: string;
  sizeLabel: string;
  cached: boolean;
  sizeBytes: number;
}
export interface ModelStatusDto {
  ocrPresent: boolean;
  ocrLocation: string | null;
  embedCacheDir: string | null;
  embeddingModels: EmbedModelStatus[];
  rerankCacheDir: string | null;
  rerankModels: RerankModelStatus[];
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
