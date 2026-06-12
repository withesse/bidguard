// 新通路 API：薄函数层，一个 command 一个函数。
import { call } from "./client";
import type {
  AnnotationDto,
  AppInfoDto,
  ClusterDetailDto,
  ClusterFilter,
  ClusterSummaryDto,
  CompareRequest,
  CompareSummaryDto,
  DocumentDto,
  DocumentPreviewDto,
  JobDto,
  PageResult,
  TemplateDto,
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
export const importDocuments = (workspaceId: string, paths: string[]) =>
  call<JobDto>("import_documents", { workspaceId, paths });
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
export const exportReport = (jobId: string, format: string, path: string) =>
  call<{ path: string; format: string }>("export_report", { jobId, format, path });

// —— 设置 / 模板 / 应用信息 ——
export const getAppSettings = () => call<Record<string, unknown> | null>("get_app_settings");
export const setAppSettings = (settings: Record<string, unknown>) =>
  call<void>("set_app_settings", { settings });
export const getAppInfo = () => call<AppInfoDto>("get_app_info");
export const listSourceTemplates = () => call<TemplateDto[]>("list_source_templates");
export const saveSourceTemplate = (name: string, text: string, id?: string) =>
  call<TemplateDto>("save_source_template", { id: id ?? null, name, text });
export const deleteSourceTemplate = (id: string) =>
  call<void>("delete_source_template", { id });

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
