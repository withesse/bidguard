// 过渡适配器：把新通路的 DTO 拼回旧 Report 形状，让 Matrix/Compare/Clusters/Export
// 四个结果屏零视觉改动接上新数据源。阶段 6 各屏原生化后删除本文件。
import { useQuery } from "@tanstack/react-query";
import * as api from "../api";
import type {
  Cluster,
  Collusion,
  DocInfo,
  Fingerprint,
  PairDetail,
  Report,
  SectionStat,
  SharedTerm,
} from "../engine";

const EMPTY_FP: Fingerprint = {
  author: null,
  lastModifiedBy: null,
  created: null,
  modified: null,
  app: null,
  revision: null,
  totalEditMinutes: null,
  riskFlags: [],
};

/** 旧 UI 一屏最多展示的聚合数（与旧引擎 MAX_CLUSTERS 一致） */
const ADAPTER_CLUSTER_LIMIT = 40;

async function buildReport(jobId: string): Promise<Report | null> {
  const s = await api.getCompareSummary(jobId);
  if (s.job.status !== "completed" || !s.matrix) return null;

  const docIds: string[] = s.matrix.documentIds;
  const idxOf = new Map(docIds.map((id, i) => [id, i]));

  const docs: DocInfo[] = docIds.map((id) => {
    const d = s.documents.find((x) => x.id === id);
    let fingerprint = EMPTY_FP;
    try {
      fingerprint = { ...EMPTY_FP, ...JSON.parse(d?.fingerprintJson ?? "{}") };
    } catch {
      // 指纹损坏不影响主报告
    }
    return {
      id,
      name: d?.fileName ?? "未知文档",
      docType: d?.fileType ?? "",
      pages: d?.pageCount ?? 0,
      charCount: d?.charCount ?? 0,
      fingerprint,
      parseError: null,
    };
  });

  // 聚合条款：取前 40 个（旧 UI 的上限），逐个取成员文本
  const page = await api.listClusters(jobId, undefined, 0, ADAPTER_CLUSTER_LIMIT);
  const clusters: Cluster[] = await Promise.all(
    page.items
      .filter((c) => c.clusterType !== "deleted" && c.clusterType !== "added")
      .map(async (c) => {
        const detail = await api.getClusterDetail(c.id);
        const docsIdx = [...new Set(c.documentIds.map((id) => idxOf.get(id) ?? 0))].sort(
          (a, b) => a - b,
        );
        return {
          avgScore: c.score ?? 0,
          peak: c.score ?? 0,
          docs: docsIdx,
          segments: detail.members.map((m) => ({
            doc: idxOf.get(m.documentId) ?? 0,
            text: m.text,
          })),
        };
      }),
  );

  // 逐对明细：≤45 对，按需拉取（本地 SQLite + 即时 diff，毫秒级）
  const pairs: PairDetail[] = [];
  for (let a = 0; a < docIds.length; a++) {
    for (let b = a + 1; b < docIds.length; b++) {
      pairs.push({ a, b, score: s.matrix.matrix[a][b], matches: [] });
    }
  }
  await Promise.all(
    pairs.map(async (p) => {
      const matches = await api.getPairDetail(jobId, docIds[p.a], docIds[p.b]);
      p.matches = matches.map((m) => ({
        textA: m.textA,
        textB: m.textB,
        score: m.score,
        diff: m.diff,
      }));
    }),
  );

  return {
    docs,
    matrix: s.matrix.matrix,
    peak: s.matrix.peak,
    pairs,
    clusters,
    collusion: (s.collusion as unknown as Collusion) ?? undefined,
    sections: (s.sections as unknown as SectionStat[]) ?? undefined,
    sharedTerms: (s.sharedTerms as unknown as SharedTerm[]) ?? undefined,
  };
}

export function useJobReport(jobId: string | undefined) {
  return useQuery({
    queryKey: ["jobReport", jobId],
    queryFn: () => buildReport(jobId!),
    enabled: !!jobId,
    // 完成的比对结果不可变
    staleTime: Infinity,
  });
}
