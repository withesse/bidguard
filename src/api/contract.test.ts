// IPC 契约测试：TS DTO 是 Rust serde(camelCase) 输出的手写镜像。这里用代表性 wire 样本 +
// `satisfies` 把样本绑定到 DTO 类型——前端任一字段重命名/漏字段/改可空性，tsc 会在此文件报错
// （`npm run build` 拦截）；运行时再断言关键 camelCase 键齐备，锁定线协议形状。
//
// 局限：无法自动发现 Rust 侧改名（那需 tauri-specta 之类的类型生成）。样本值对应当前 Rust
// serde 输出；改 Rust 字段时应同步更新对应 DTO 与此处样本。
import { describe, it, expect } from "vitest";
import type {
  AlignedSegmentDto,
  AnnotationDto,
  AppInfoDto,
  ClusterSummaryDto,
  CompareSummary,
  DiagnosticItem,
  DocumentDto,
  JobDto,
  LicenseStatusDto,
  ModelStatusDto,
  ProgressEvent,
  StorageInfoDto,
  TemplateDto,
  TerminalEvent,
  WorkspaceDto,
} from "./types";

const documentSample = {
  id: "d1",
  workspaceId: "w1",
  fileName: "bid.docx",
  filePath: "/x/bid.docx",
  fileHash: "abc",
  fileType: "docx",
  status: "parsed",
  parseError: null,
  parseMethod: "docx",
  pageCount: 12,
  charCount: 3400,
  fingerprintJson: null,
  chunkCount: 42,
  createdAt: "2026-07-02T00:00:00.000Z",
  updatedAt: "2026-07-02T00:00:00.000Z",
  truncationNotice: null,
  evasionSummary: null,
  docRole: "bid",
} satisfies DocumentDto;

const jobSample = {
  id: "j1",
  workspaceId: "w1",
  jobType: "compare",
  name: null,
  status: "completed",
  configJson: "{}",
  progress: 1,
  message: null,
  errorMessage: null,
  errorCode: null,
  starred: false,
  matrixJson: null,
  collusionLevel: "none",
  createdAt: "2026-07-02T00:00:00.000Z",
  startedAt: null,
  finishedAt: null,
} satisfies JobDto;

const progressSample = {
  jobId: "j1",
  jobType: "import",
  stage: "parse",
  message: "解析中",
  current: 1,
  total: 3,
  percent: 0.33,
} satisfies ProgressEvent;

const terminalSample = {
  jobId: "j1",
  jobType: "import",
  status: "completed",
} satisfies TerminalEvent;

// —— 扩面样本（与 src-tauri/src/contract_wire.rs 的 Rust 半边一一对应，两处一起改）——

const workspaceSample = {
  id: "w1",
  name: "评标项目",
  createdAt: "2026-08-26T00:00:00.000Z",
  updatedAt: "2026-08-26T00:00:00.000Z",
  settingsJson: "{}",
  documentCount: 3,
  latestJobStatus: "completed",
} satisfies WorkspaceDto;

const licenseSample = {
  state: "trial",
  active: true,
  plan: "trial",
  licenseeName: null,
  expiresAt: null,
  remainingUses: 10,
  usedUses: 0,
  trialExpiresAt: null,
  machineCode: "BG2-XXXXX",
  clockTamper: false,
  tamper: false,
  message: null,
} satisfies LicenseStatusDto;

const annotationSample = {
  id: "a1",
  workspaceId: "w1",
  documentId: "d1",
  chunkId: null,
  clusterId: null,
  page: 3,
  quote: "原文引用",
  note: "复核意见",
  createdAt: "2026-08-26T00:00:00.000Z",
  updatedAt: "2026-08-26T00:00:00.000Z",
} satisfies AnnotationDto;

const templateSample = {
  id: "t1",
  name: "法规引用",
  text: "依据《招标投标法》……",
  category: null,
  enabled: true,
  createdAt: "2026-08-26T00:00:00.000Z",
  hitCount: 2,
} satisfies TemplateDto;

const clusterSample = {
  id: "c1",
  jobId: "j1",
  clusterType: "same",
  topic: null,
  summary: "整段一致",
  severity: "high",
  score: 0.98,
  sectionKind: "tech",
  reviewStatus: "pending",
  sectionPath: "第一章 › 1.1",
  page: 5,
  documentIds: ["d1", "d2"],
  memberCount: 2,
  exemptReason: null,
  multiDocAnomaly: false,
  confidence: 0.87,
  band: "review",
  rerankScore: null,
} satisfies ClusterSummaryDto;

const segmentSample = {
  id: "s1",
  docAId: "d1",
  docBId: "d2",
  anchorCount: 14,
  verbatimChars: 620,
  aCoveredChars: 1800,
  bCoveredChars: 1750,
  aCoverage: 0.82,
  bCoverage: 0.8,
  avgScore: 0.91,
  aSectionPath: "第三章 › 3.2",
  bSectionPath: "第三章 › 3.2",
  aPageStart: 12,
  aPageEnd: 15,
  bPageStart: 11,
  bPageEnd: 14,
} satisfies AlignedSegmentDto;

const modelStatusSample = {
  ocrPresent: true,
  ocrLocation: "/models",
  embedCacheDir: null,
  embeddingModels: [{ key: "bge-zh", label: "bge-large-zh", cached: true, sizeBytes: 1024 }],
  rerankCacheDir: null,
  rerankModels: [
    { key: "bge-reranker-base-int8", label: "复核模型", sizeLabel: "~300MB", cached: false, sizeBytes: 0 },
  ],
} satisfies ModelStatusDto;

const storageSample = {
  dbBytes: 1_000_000,
  embeddingRows: 42,
  documentCount: 5,
  jobCount: 3,
} satisfies StorageInfoDto;

const diagnosticSample = {
  key: "pdfium",
  label: "PDF 引擎",
  ok: true,
  detail: "已就绪",
} satisfies DiagnosticItem;

const appInfoSample = {
  version: "0.6.0",
  buildSha: "abc123def456",
  logDir: "/logs",
  maxDocs: 10,
  minDocs: 2,
  embeddingModels: [{ key: "bge-zh", label: "bge-large-zh" }],
  ocrModels: [{ key: "v6-small", label: "标准档", sizeLabel: "~30MB", bundled: true, present: true }],
  defaultOcrModel: "v6-small",
} satisfies AppInfoDto;

const compareSummarySample = {
  documentCount: 2,
  chunkCount: 40,
  clusterCount: 3,
  sameCount: 1,
  minorChangeCount: 0,
  rewriteCount: 0,
  changedCount: 1,
  addedCount: 0,
  deletedCount: 0,
  conflictCount: 1,
  uncertainCount: 0,
  highRiskCount: 1,
  semanticDegraded: false,
  tenderRefChunkCount: 0,
  backgroundExemptChunkCount: 0,
  zoneLegalCount: 0,
  zonePriceCount: 1,
  zoneTechCount: 2,
  zoneBusinessCount: 0,
  zoneOtherCount: 0,
  boqItemCount: 0,
  boqAlignedItemCount: 0,
  boqAlignRate: 0,
  boqTableCount: 0,
  boqSkippedTableCount: 0,
  bandPassCount: 0,
  bandReviewCount: 3,
  bandFlagCount: 0,
  bandUncalibratedCount: 0,
  calibrationVersion: "1",
  calibrationKind: "experimental-synthetic",
  calibrationRouting: "review-all",
  calibrationAlpha: 0.05,
  calibrationBeta: 0.1,
  rerankDegraded: false,
  rerankReviewedCount: 0,
} satisfies CompareSummary;

describe("IPC DTO 契约（camelCase 镜像）", () => {
  it("DocumentDto 关键字段齐备（含 truncationNotice / evasionSummary / docRole）", () => {
    expect(Object.keys(documentSample)).toEqual(
      expect.arrayContaining([
        "id", "workspaceId", "fileName", "status", "parseError",
        "chunkCount", "truncationNotice", "evasionSummary", "docRole",
      ]),
    );
  });

  it("JobDto 关键字段齐备（含 collusionLevel）", () => {
    expect(Object.keys(jobSample)).toEqual(
      expect.arrayContaining([
        "id", "workspaceId", "jobType", "status", "starred",
        "matrixJson", "collusionLevel",
      ]),
    );
  });

  it("ProgressEvent / TerminalEvent 关键字段齐备", () => {
    expect(Object.keys(progressSample)).toEqual(
      expect.arrayContaining(["jobId", "jobType", "stage", "current", "total", "percent"]),
    );
    expect(Object.keys(terminalSample)).toEqual(
      expect.arrayContaining(["jobId", "jobType", "status"]),
    );
  });

  it("授权 / 工作区 / 批注 / 样板关键字段齐备", () => {
    expect(Object.keys(licenseSample)).toEqual(
      expect.arrayContaining(["state", "active", "remainingUses", "machineCode", "clockTamper", "tamper"]),
    );
    expect(Object.keys(workspaceSample)).toEqual(
      expect.arrayContaining(["id", "name", "documentCount", "latestJobStatus"]),
    );
    expect(Object.keys(annotationSample)).toEqual(
      expect.arrayContaining(["workspaceId", "documentId", "clusterId", "quote", "note"]),
    );
    expect(Object.keys(templateSample)).toEqual(
      expect.arrayContaining(["name", "text", "category", "enabled", "hitCount"]),
    );
  });

  it("结果屏 DTO（条款组 / 区段 / 八类统计）关键字段齐备", () => {
    expect(Object.keys(clusterSample)).toEqual(
      expect.arrayContaining([
        "clusterType", "severity", "reviewStatus", "documentIds",
        "exemptReason", "multiDocAnomaly", "confidence", "band",
      ]),
    );
    expect(Object.keys(segmentSample)).toEqual(
      expect.arrayContaining(["docAId", "docBId", "anchorCount", "verbatimChars", "aCoverage", "bCoverage"]),
    );
    expect(Object.keys(compareSummarySample)).toEqual(
      expect.arrayContaining([
        "clusterCount", "conflictCount", "semanticDegraded",
        "tenderRefChunkCount", "bandReviewCount", "calibrationRouting", "rerankDegraded",
      ]),
    );
  });

  it("工具箱与关于面板 DTO 关键字段齐备（含 buildSha / logDir）", () => {
    expect(Object.keys(modelStatusSample)).toEqual(
      expect.arrayContaining(["ocrPresent", "embeddingModels", "rerankModels"]),
    );
    expect(Object.keys(storageSample)).toEqual(
      expect.arrayContaining(["dbBytes", "embeddingRows", "documentCount", "jobCount"]),
    );
    expect(Object.keys(diagnosticSample)).toEqual(expect.arrayContaining(["key", "label", "ok", "detail"]));
    expect(Object.keys(appInfoSample)).toEqual(
      expect.arrayContaining(["version", "buildSha", "logDir", "maxDocs", "defaultOcrModel"]),
    );
  });
});
