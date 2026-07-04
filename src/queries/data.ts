// TanStack Query 封装：查询键约定 ['workspaces'] ['documents', wsId] ['jobs', wsId?]
// ['job', jobId] ['compareSummary', jobId] ['clusters', jobId, filter] ['cluster', cid]
// ['pairDetail', jobId, a, b]（逐对懒加载）。运行中任务由事件驱动失效 + 轮询兜底。
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type InfiniteData,
  type QueryKey,
} from "@tanstack/react-query";
import * as api from "../api";
import { errMsg } from "../api/client";
import { toast } from "../components/Toast";
import type {
  ClusterFilter,
  CompareRequest,
  JobDto,
  NewTemplateDto,
  TemplateDto,
} from "../api/types";

export function useWorkspaces() {
  return useQuery({ queryKey: ["workspaces"], queryFn: api.listWorkspaces });
}

export function useAppInfo() {
  return useQuery({ queryKey: ["appInfo"], queryFn: api.getAppInfo, staleTime: Infinity });
}

// —— 工具：模型 / 存储 / 自检 ——
export function useModelStatus() {
  return useQuery({ queryKey: ["modelStatus"], queryFn: api.getModelStatus });
}
export function useStorageInfo() {
  return useQuery({ queryKey: ["storageInfo"], queryFn: api.getStorageInfo });
}
export function useDiagnostics(enabled: boolean) {
  return useQuery({ queryKey: ["diagnostics"], queryFn: api.runDiagnostics, enabled });
}
export function useDownloadModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.downloadEmbeddingModel,
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["modelStatus"] }),
  });
}
export function useClearModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.clearEmbeddingModel,
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["modelStatus"] }),
  });
}
export function useDownloadOcrModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.downloadOcrModel,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["appInfo"] });
      void qc.invalidateQueries({ queryKey: ["modelStatus"] });
    },
  });
}
export function useClearOcrModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.clearOcrModel,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["appInfo"] });
      void qc.invalidateQueries({ queryKey: ["modelStatus"] });
    },
  });
}
export function useClearEmbeddingCache() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.clearEmbeddingCache,
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["storageInfo"] }),
  });
}
export function useVacuumDb() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.vacuumDb,
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["storageInfo"] }),
  });
}
export function useCleanupOldJobs() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.cleanupOldJobs,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["jobs"] });
      void qc.invalidateQueries({ queryKey: ["storageInfo"] });
    },
  });
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
    onError: (e) => toast("添加批注失败：" + errMsg(e), "error"),
  });
}

export function useUpdateAnnotation(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, note }: { id: string; note: string }) => api.updateAnnotation(id, note),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["annotations", workspaceId] }),
    onError: (e) => toast("更新批注失败：" + errMsg(e), "error"),
  });
}

export function useDeleteAnnotation(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteAnnotation(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["annotations", workspaceId] }),
    onError: (e) => toast("删除批注失败：" + errMsg(e), "error"),
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

type ReviewClusterItem = { id: string; reviewStatus?: string };
type ClusterListData = InfiniteData<{ items: ReviewClusterItem[]; offset: number; total: number }>;

/** 人工确认状态：详情 + 列表(所有 filter 变体的无限查询页)同步乐观更新，失败回滚。 */
export function useSetReviewStatus(jobId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ clusterId, status }: { clusterId: string; status: string }) =>
      api.setClusterReviewStatus(clusterId, status),
    onMutate: async ({ clusterId, status }) => {
      await qc.cancelQueries({ queryKey: ["cluster", clusterId] });
      await qc.cancelQueries({ queryKey: ["clusters", jobId] });
      // 详情缓存
      const prevDetail = qc.getQueryData(["cluster", clusterId]);
      qc.setQueryData(["cluster", clusterId], (old: unknown) => {
        if (!old || typeof old !== "object") return old;
        const o = old as { cluster?: { reviewStatus?: string } };
        return { ...o, cluster: { ...o.cluster, reviewStatus: status } };
      });
      // 列表缓存：逐个 filter 变体乐观更新。按 reviewStatus 过滤的视图（如「只看待确认」）里，
      // 若新状态不再匹配该过滤，则把命中项【移出列表并减 total】——否则会留下「已确认却仍在待确认
      // 列表」的自相矛盾条目；其余视图只就地改 reviewStatus。这样 onSettled 用 refetchType:"none"
      // 不重取也不产生不一致（既避免深滚动重取风暴，又不漏更新过滤态）。
      const prevLists = qc.getQueriesData<ClusterListData>({ queryKey: ["clusters", jobId] });
      for (const [key] of prevLists) {
        const filter = (key as unknown[])[2] as { reviewStatus?: string } | undefined;
        const drop = !!filter?.reviewStatus && filter.reviewStatus !== status;
        qc.setQueryData<ClusterListData>(key as QueryKey, (old) => {
          if (!old) return old;
          if (!drop) {
            return {
              ...old,
              pages: old.pages.map((p) => ({
                ...p,
                items: p.items.map((it) =>
                  it.id === clusterId ? { ...it, reviewStatus: status } : it,
                ),
              })),
            };
          }
          const present = old.pages.some((p) => p.items.some((it) => it.id === clusterId));
          return {
            ...old,
            pages: old.pages.map((p) => ({
              ...p,
              items: p.items.filter((it) => it.id !== clusterId),
              total: present ? Math.max(0, p.total - 1) : p.total,
            })),
          };
        });
      }
      return { prevDetail, prevLists };
    },
    onError: (e, { clusterId }, ctx) => {
      if (ctx?.prevDetail !== undefined) qc.setQueryData(["cluster", clusterId], ctx.prevDetail);
      ctx?.prevLists?.forEach(([key, data]) => qc.setQueryData(key as QueryKey, data));
      toast("人工确认失败：" + errMsg(e), "error");
    },
    onSettled: () => {
      // 乐观更新已改好列表/详情；这里只标脏不重取（refetchType:"none"）——否则活跃 infiniteQuery
      // 会逐页串行重取，深滚动大列表下每次行内确认都触发几十次 IPC。下次挂载/聚焦自然收敛。
      void qc.invalidateQueries({ queryKey: ["clusters", jobId], refetchType: "none" });
      void qc.invalidateQueries({ queryKey: ["cluster"], refetchType: "none" });
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
    mutationFn: ({
      name,
      text,
      id,
      category,
    }: {
      name: string;
      text: string;
      id?: string;
      category?: string | null;
    }) => api.saveSourceTemplate(name, text, { id, category }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

export function useSetTemplateEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.setSourceTemplateEnabled(id, enabled),
    // 乐观更新：列表立即生效，失败回滚由失效兜底
    onMutate: async ({ id, enabled }) => {
      await qc.cancelQueries({ queryKey: ["templates"] });
      const prev = qc.getQueryData<TemplateDto[]>(["templates"]);
      qc.setQueryData<TemplateDto[]>(["templates"], (old) =>
        old?.map((t) => (t.id === id ? { ...t, enabled } : t)),
      );
      return { prev };
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(["templates"], ctx.prev);
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

export function useBatchSaveTemplates() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (items: NewTemplateDto[]) => api.batchSaveSourceTemplates(items),
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
