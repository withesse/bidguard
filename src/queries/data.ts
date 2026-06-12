// TanStack Query 封装：查询键约定 ['workspaces'] ['documents', wsId] ['jobs', wsId?]
// ['job', jobId] ['jobReport', jobId] ['clusters', jobId, filter] ['cluster', cid]。
// 运行中任务由事件驱动失效 + 轮询兜底。
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import * as api from "../api";
import type { ClusterFilter, CompareRequest, JobDto } from "../api/types";

export function useWorkspaces() {
  return useQuery({ queryKey: ["workspaces"], queryFn: api.listWorkspaces });
}

export function useWorkspace(workspaceId: string | undefined) {
  return useQuery({
    queryKey: ["workspace", workspaceId],
    queryFn: () => api.getWorkspace(workspaceId!),
    enabled: !!workspaceId,
  });
}

export function useDocuments(workspaceId: string | undefined) {
  return useQuery({
    queryKey: ["documents", workspaceId],
    queryFn: () => api.listDocuments(workspaceId!),
    enabled: !!workspaceId,
    // 导入进行中每秒兜底刷新（事件丢失时仍能收敛）
    refetchInterval: (q) =>
      q.state.data?.some((d) => d.status === "parsing") ? 1000 : false,
  });
}

export function useDocumentPreview(documentId: string | undefined) {
  return useQuery({
    queryKey: ["docPreview", documentId],
    queryFn: () => api.getDocumentPreview(documentId!),
    enabled: !!documentId,
  });
}

/** 原始文件字节（原文版式预览）。大文件不随窗口重挂载反复读盘：staleTime 拉满。 */
export function useDocumentFile(documentId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: ["docFile", documentId],
    queryFn: () => api.readDocumentFile(documentId!),
    enabled: !!documentId && enabled,
    staleTime: Infinity,
    gcTime: 60_000,
  });
}

export function useDocumentOcrLayout(documentId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: ["docOcrLayout", documentId],
    queryFn: () => api.getDocumentOcrLayout(documentId!),
    enabled: !!documentId && enabled,
    staleTime: Infinity,
  });
}

// —— 批注 ——
export function useAnnotations(workspaceId: string | undefined) {
  return useQuery({
    queryKey: ["annotations", workspaceId],
    queryFn: () => api.listAnnotations(workspaceId!),
    enabled: !!workspaceId,
  });
}

export function useAddAnnotation(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.addAnnotation,
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["annotations", workspaceId] }),
  });
}

export function useUpdateAnnotation(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, note }: { id: string; note: string }) => api.updateAnnotation(id, note),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["annotations", workspaceId] }),
  });
}

export function useDeleteAnnotation(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteAnnotation(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["annotations", workspaceId] }),
  });
}

export function useJobs(workspaceId?: string) {
  return useQuery({
    queryKey: ["jobs", workspaceId ?? "all"],
    queryFn: () => api.listJobs(workspaceId),
    refetchInterval: (q) =>
      q.state.data?.some((j) => isLive(j)) ? 1000 : false,
  });
}

export function isLive(j: JobDto): boolean {
  return j.status === "pending" || j.status === "running" || j.status === "cancelling";
}

export function useJob(jobId: string | undefined) {
  return useQuery({
    queryKey: ["job", jobId],
    queryFn: () => api.getJob(jobId!),
    enabled: !!jobId,
    refetchInterval: (q) => (q.state.data && isLive(q.state.data) ? 800 : false),
  });
}

export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => api.createWorkspace(name),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}

export function useRenameWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => api.renameWorkspace(id, name),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}

export function useDeleteWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (workspaceId: string) => api.deleteWorkspace(workspaceId),
    onSuccess: () => void qc.invalidateQueries(),
  });
}

export function useImportDocuments(workspaceId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (paths: string[]) => api.importDocuments(workspaceId, paths),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["documents", workspaceId] });
      void qc.invalidateQueries({ queryKey: ["jobs"] });
    },
  });
}

export function useRemoveDocument(workspaceId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (documentId: string) => api.removeDocument(documentId),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["documents", workspaceId] }),
  });
}

export function useStartCompare(workspaceId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: CompareRequest) => api.startCompare(workspaceId, request),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["jobs"] }),
  });
}

export function useCancelJob() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (jobId: string) => api.cancelJob(jobId),
    onSuccess: (_d, jobId) => void qc.invalidateQueries({ queryKey: ["job", jobId] }),
  });
}

export function useSetJobStarred() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ jobId, starred }: { jobId: string; starred: boolean }) =>
      api.setJobStarred(jobId, starred),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["jobs"] }),
  });
}

export function useDeleteJob() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (jobId: string) => api.deleteJob(jobId),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["jobs"] }),
  });
}

const CLUSTER_PAGE = 60;

/** 聚合条款无限滚动分页（配合虚拟列表）。 */
export function useClustersInfinite(jobId: string | undefined, filter: ClusterFilter) {
  return useInfiniteQuery({
    queryKey: ["clusters", jobId, filter],
    queryFn: ({ pageParam }) => api.listClusters(jobId!, filter, pageParam, CLUSTER_PAGE),
    enabled: !!jobId,
    initialPageParam: 0,
    getNextPageParam: (last) =>
      last.offset + last.items.length < last.total ? last.offset + last.items.length : undefined,
    staleTime: 60_000,
  });
}

export function useClusterDetail(clusterId: string | undefined) {
  return useQuery({
    queryKey: ["cluster", clusterId],
    queryFn: () => api.getClusterDetail(clusterId!),
    enabled: !!clusterId,
  });
}

/** 人工确认状态（乐观更新：列表与详情立即生效，失败回滚由失效兜底）。 */
export function useSetReviewStatus(jobId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ clusterId, status }: { clusterId: string; status: string }) =>
      api.setClusterReviewStatus(clusterId, status),
    onMutate: async ({ clusterId, status }) => {
      // 乐观改详情缓存
      qc.setQueryData(["cluster", clusterId], (old: unknown) => {
        if (!old || typeof old !== "object") return old;
        const o = old as { cluster?: { reviewStatus?: string } };
        return { ...o, cluster: { ...o.cluster, reviewStatus: status } };
      });
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["clusters", jobId] });
      void qc.invalidateQueries({ queryKey: ["cluster"] });
    },
  });
}

export function useCompareSummary(jobId: string | undefined) {
  return useQuery({
    queryKey: ["compareSummary", jobId],
    queryFn: () => api.getCompareSummary(jobId!),
    enabled: !!jobId,
    staleTime: 60_000,
  });
}

// —— 设置与模板 ——

export function useAppSettings() {
  return useQuery({ queryKey: ["appSettings"], queryFn: api.getAppSettings });
}

export function useSaveAppSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: Record<string, unknown>) => api.setAppSettings(settings),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["appSettings"] }),
  });
}

export function useTemplates() {
  return useQuery({ queryKey: ["templates"], queryFn: api.listSourceTemplates });
}

export function useSaveTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, text, id }: { name: string; text: string; id?: string }) =>
      api.saveSourceTemplate(name, text, id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

export function useDeleteTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteSourceTemplate(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}
