// 新通路 API：薄函数层，一个 command 一个函数。
import { call } from "./client";
import type {
  AnnotationDto,
  AppInfoDto,
  DiagnosticItem,
  LicenseStatusDto,
  ModelStatusDto,
  StorageInfoDto,
  AlignedSegmentDto,
  ClusterDetailDto,
  ClusterFilter,
  ClusterSegmentRefDto,
  ClusterSummaryDto,
  CompareRequest,
  CompareSummaryDto,
  SegmentDetailDto,
  DocRole,
  DocumentDto,
  DocumentPreviewDto,
  JobDto,
  PageResult,
  TemplateDto,
  NewTemplateDto,
  BatchTemplateResult,
  WorkspaceDto,
} from "./types";
import type { PairMatchDto } from "./types";

// —— 工作区 ——
export const createWorkspace = (name: string) => call<WorkspaceDto>("create_workspace", { name });
export const listWorkspaces = () => call<WorkspaceDto[]>("list_workspaces");
export const getWorkspace = (workspaceId: string) =>
  call<WorkspaceDto>("get_workspace", { workspaceId });
export const renameWorkspace = (workspaceId: string, name: string) =>
  call<void>("rename_workspace", { workspaceId, name });
export const setWorkspaceSettings = (workspaceId: string, settingsJson: string | null) =>
  call<void>("set_workspace_settings", { workspaceId, settingsJson });
export const deleteWorkspace = (workspaceId: string) =>
  call<void>("delete_workspace", { workspaceId });

// —— 文档 ——
/** docRole 缺省按投标文件（bid）导入；招标文件/补遗答疑传 tender / tender_supplement。 */
export const importDocuments = (workspaceId: string, paths: string[], docRole?: DocRole) =>
  call<JobDto>("import_documents", { workspaceId, paths, docRole: docRole ?? null });
export const listDocuments = (workspaceId: string) =>
  call<DocumentDto[]>("list_documents", { workspaceId });
export const getDocumentPreview = (documentId: string) =>
  call<DocumentPreviewDto>("get_document_preview", { documentId });
export const removeDocument = (documentId: string) =>
  call<void>("remove_document", { documentId });
/** 原始文件字节（原文版式预览数据源），后端以 raw IPC 返回 ArrayBuffer。 */
export const readDocumentFile = (documentId: string) =>
  call<ArrayBuffer>("read_document_file", { documentId });
/** 扫描件 OCR 行级版面（JSON 字符串：每页一组归一化 {t,x,y,w,h}）；非扫描件 null。 */
export const getDocumentOcrLayout = (documentId: string) =>
  call<string | null>("get_document_ocr_layout", { documentId });

// —— 批注 ——
export const addAnnotation = (a: {
  workspaceId: string;
  note: string;
  documentId?: string;
  chunkId?: string;
  clusterId?: string;
  page?: number;
  quote?: string;
}) =>
  call<AnnotationDto>("add_annotation", {
    workspaceId: a.workspaceId,
    note: a.note,
    documentId: a.documentId ?? null,
    chunkId: a.chunkId ?? null,
    clusterId: a.clusterId ?? null,
    page: a.page ?? null,
    quote: a.quote ?? null,
  });
export const listAnnotations = (workspaceId: string) =>
  call<AnnotationDto[]>("list_annotations", { workspaceId });
export const updateAnnotation = (annotationId: string, note: string) =>
  call<void>("update_annotation", { annotationId, note });
export const deleteAnnotation = (annotationId: string) =>
  call<void>("delete_annotation", { annotationId });

// —— 任务 ——
export const getJob = (jobId: string) => call<JobDto>("get_job", { jobId });
export const listJobs = (workspaceId?: string) =>
  call<JobDto[]>("list_jobs", { workspaceId: workspaceId ?? null });
export const cancelJob = (jobId: string) => call<void>("cancel_job", { jobId });
export const setJobStarred = (jobId: string, starred: boolean) =>
  call<void>("set_job_starred", { jobId, starred });
export const deleteJob = (jobId: string) => call<void>("delete_job", { jobId });
export const cleanupOldJobs = (days: number) => call<number>("cleanup_old_jobs", { days });

// —— 导出 ——
export const exportReport = (
  jobId: string,
  format: string,
  path: string,
  opts?: { includeRawText?: boolean; includeConfig?: boolean },
) =>
  call<{ path: string; format: string }>("export_report", {
    jobId,
    format,
    path,
    includeRawText: opts?.includeRawText ?? null,
    includeConfig: opts?.includeConfig ?? null,
  });

// —— 工具：模型 / 存储 / 自检 ——
export const getModelStatus = () => call<ModelStatusDto>("get_model_status");
export const downloadEmbeddingModel = (modelKey: string) =>
  call<void>("download_embedding_model", { modelKey });
export const clearEmbeddingModel = (modelKey: string) =>
  call<number>("clear_embedding_model", { modelKey });
/** 按需下载某复核模型（cross-encoder，W6-2）。返回本地占用字节数。 */
export const downloadRerankerModel = (modelKey: string) =>
  call<number>("download_reranker_model", { modelKey });
export const clearRerankerModel = (modelKey: string) =>
  call<number>("clear_reranker_model", { modelKey });
/** 按需下载某 OCR 高精档（medium）。返回写入字节数。 */
export const downloadOcrModel = (modelKey: string) =>
  call<number>("download_ocr_model", { modelKey });
export const clearOcrModel = (modelKey: string) =>
  call<number>("clear_ocr_model", { modelKey });
export const getStorageInfo = () => call<StorageInfoDto>("get_storage_info");
export const clearEmbeddingCache = () => call<number>("clear_embedding_cache");
export const vacuumDb = () => call<void>("vacuum_db");
export const runDiagnostics = () => call<DiagnosticItem[]>("run_diagnostics");

// —— 授权 / 激活 ——
export const getLicenseStatus = () => call<LicenseStatusDto>("get_license_status");
export const getMachineCode = () => call<string>("get_machine_code");
/** input：armored 许可文本（粘贴）或本机 .lic 文件路径。 */
export const importLicense = (input: string) =>
  call<LicenseStatusDto>("import_license", { input });

// —— 设置 / 模板 / 应用信息 ——
export const getAppSettings = () => call<Record<string, unknown> | null>("get_app_settings");
export const setAppSettings = (settings: Record<string, unknown>) =>
  call<void>("set_app_settings", { settings });
export const getAppInfo = () => call<AppInfoDto>("get_app_info");
export const listSourceTemplates = () => call<TemplateDto[]>("list_source_templates");
export const saveSourceTemplate = (
  name: string,
  text: string,
  opts?: { id?: string; category?: string | null },
) =>
  call<TemplateDto>("save_source_template", {
    id: opts?.id ?? null,
    name,
    text,
    category: opts?.category ?? null,
  });
export const setSourceTemplateEnabled = (id: string, enabled: boolean) =>
  call<void>("set_source_template_enabled", { id, enabled });
export const batchSaveSourceTemplates = (items: NewTemplateDto[]) =>
  call<BatchTemplateResult>("batch_save_source_templates", { items });
export const deleteSourceTemplate = (id: string) =>
  call<void>("delete_source_template", { id });
/** 读取文本文件内容（批量导入选 .txt/.csv/.json）。UTF-8 优先，GB18030 兜底。 */
export const readTextFile = (path: string) => call<string>("read_text_file", { path });

// —— 比对 ——
export const startCompare = (workspaceId: string, request: CompareRequest) =>
  call<JobDto>("start_compare", { workspaceId, request });
export const getCompareSummary = (jobId: string) =>
  call<CompareSummaryDto>("get_compare_summary", { jobId });
export const listClusters = (
  jobId: string,
  filter?: ClusterFilter,
  offset?: number,
  limit?: number,
) =>
  call<PageResult<ClusterSummaryDto>>("list_clusters", {
    jobId,
    filter: filter ?? null,
    offset: offset ?? null,
    limit: limit ?? null,
  });
export const getClusterDetail = (clusterId: string) =>
  call<ClusterDetailDto>("get_cluster_detail", { clusterId });
export const setClusterReviewStatus = (clusterId: string, status: string) =>
  call<void>("set_cluster_review_status", { clusterId, status });
export const getPairDetail = (jobId: string, documentA: string, documentB: string) =>
  call<PairMatchDto[]>("get_pair_detail", { jobId, documentA, documentB });

// —— 对齐区段（W4-5，M5b）——
/** 某任务的对齐区段列表；documentA/B 可选（方向无关过滤）。旧任务返回空数组。 */
export const listAlignedSegments = (jobId: string, documentA?: string, documentB?: string) =>
  call<AlignedSegmentDto[]>("list_aligned_segments", {
    jobId,
    documentA: documentA ?? null,
    documentB: documentB ?? null,
  });
/** 区段详情（双栏高亮 + 反向互链所需数据）。 */
export const getSegmentDetail = (segmentId: string) =>
  call<SegmentDetailDto>("get_segment_detail", { segmentId });
/** 某聚类反查关联的对齐区段（ClusterDetail「所在区段」Pill）。旧任务返回空数组。 */
export const getClusterSegments = (clusterId: string) =>
  call<ClusterSegmentRefDto[]>("get_cluster_segments", { clusterId });
