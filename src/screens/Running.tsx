// 检测进度页：全局事件实时进度 + useJob 轮询兜底（刷新/深链接后事件缺失也能收敛）。
// 完成自动跳报告；失败/取消给出明确出口；运行中可取消。
import { useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import { useCancelJob, useJob, useStartCompare } from "../queries/data";
import { useProgressStore } from "../stores/progressStore";
import { STAGE_LABEL, statusUi } from "../utils/jobStatus";
import type { CompareRequest } from "../api/types";

const STAGES = ["load", "semantic", "recall", "score", "cluster", "facts", "persist"] as const;

export function Running() {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();
  const toast = useToast();
  const { dark, accent } = useTheme();
  const { data: job } = useJob(jobId);
  const cancel = useCancelJob();
  const startCompare = useStartCompare(wsId!);
  const prog = useProgressStore((s) => (jobId ? s.progress[jobId] : undefined));

  // 重试：用本任务存的配置(jobs.config_json，含 documentIds + 全部选项)原样重发一次比对。
  const onRetry = async () => {
    if (!job || !wsId) return;
    let cfg: Partial<CompareRequest> & { documentIds?: string[] };
    try {
      cfg = JSON.parse(job.configJson);
    } catch {
      toast.show("无法读取原任务配置，请返回重新配置", "warn");
      return;
    }
    if (!cfg.documentIds || cfg.documentIds.length < 2) {
      toast.show("原任务缺少文档信息，请返回重新配置", "warn");
      return;
    }
    try {
      const newJob = await startCompare.mutateAsync({
        documentIds: cfg.documentIds,
        name: job.name ?? undefined,
        baseDocumentId: cfg.baseDocumentId,
        chunkLevel: cfg.chunkLevel,
        similarityThreshold: cfg.similarityThreshold,
        candidateTopK: cfg.candidateTopK,
        enableSemantic: cfg.enableSemantic,
        enableFactConflict: cfg.enableFactConflict,
        ignoreTemplates: cfg.ignoreTemplates,
        detectMovedParagraph: cfg.detectMovedParagraph,
        scope: cfg.scope,
        embeddingModel: cfg.embeddingModel,
      });
      nav(`/workspace/${wsId}/job/${newJob.id}/running`, { replace: true });
    } catch (e) {
      toast.show("重试失败：" + errMsg(e), "error");
    }
  };

  // 完成 → 自动进报告
  useEffect(() => {
    if (job?.status === "completed") {
      nav(`/workspace/${wsId}/job/${jobId}`, { replace: true });
    }
  }, [job?.status, nav, wsId, jobId]);

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;

  const percent = Math.round((prog?.percent ?? job?.progress ?? 0) * 100);
  const stage = prog?.stage ?? "";
  const message = prog?.message ?? job?.message ?? "准备中…";
  const live = job?.status === "pending" || job?.status === "running" || job?.status === "cancelling";
  const st = job ? statusUi(job.status) : null;

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar title={job?.name ?? "检测中"} sub="本地引擎正在交叉比对，不上传任何文件" />
      <div
        style={{
          flex: 1,
          overflowY: "auto",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: 24,
        }}
      >
        <div
          style={{
            width: 520,
            maxWidth: "100%",
            background: cardBg,
            border: `1px solid ${border}`,
            borderRadius: 16,
            padding: "30px 34px",
            display: "flex",
            flexDirection: "column",
            gap: 18,
          }}
        >
          <div style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
            <div style={{ fontSize: 44, fontWeight: 800, color: ink, fontVariantNumeric: "tabular-nums" }}>
              {percent}%
            </div>
            {st && (
              <Pill fg={st.fg} bg={st.bg} size={11}>
                {st.label}
              </Pill>
            )}
          </div>

          <div style={{ height: 6, background: dark ? "rgba(255,255,255,0.08)" : C.paper2, borderRadius: 3 }}>
            <div
              style={{
                height: "100%",
                width: `${percent}%`,
                background: accent,
                borderRadius: 3,
                transition: "width 0.3s ease",
              }}
            />
          </div>

          {/* 阶段轨道 */}
          {job?.jobType === "compare" && (
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {STAGES.map((s) => {
                const reached = STAGES.indexOf(stage as (typeof STAGES)[number]) >= STAGES.indexOf(s);
                const active = stage === s;
                return (
                  <span
                    key={s}
                    style={{
                      fontSize: 10.5,
                      padding: "3px 8px",
                      borderRadius: 999,
                      background: active
                        ? "rgba(79,88,168,0.16)"
                        : reached
                          ? dark
                            ? "rgba(255,255,255,0.07)"
                            : C.paper2
                          : "transparent",
                      color: active ? "var(--accent, #4F58A8)" : reached ? ink : mute,
                      border: `1px solid ${active ? "var(--accent, #4F58A8)" : border}`,
                      fontWeight: active ? 700 : 500,
                    }}
                  >
                    {STAGE_LABEL[s]}
                  </span>
                );
              })}
            </div>
          )}

          <div style={{ fontSize: 12.5, color: mute, minHeight: 18 }}>{message}</div>

          {job?.status === "failed" && (
            <div style={{ fontSize: 12.5, color: "#B54545", lineHeight: 1.6 }}>
              检测失败：{job.errorMessage ?? "未知原因"}
            </div>
          )}
          {job?.status === "cancelled" && (
            <div style={{ fontSize: 12.5, color: mute }}>任务已取消，结果未保留。</div>
          )}

          <div style={{ display: "flex", gap: 10 }}>
            {live && (
              <Button
                kind="secondary"
                disabled={cancel.isPending || job?.status === "cancelling"}
                onClick={() =>
                  cancel.mutate(jobId!, {
                    onSuccess: () => toast.show("已请求取消，正在收尾…", "info"),
                    onError: (e) => toast.show("取消失败：" + errMsg(e), "error"),
                  })
                }
              >
                {cancel.isPending || job?.status === "cancelling" ? "取消中…" : "取消检测"}
              </Button>
            )}
            {(job?.status === "failed" || job?.status === "cancelled") && (
              <Button kind="primary" disabled={startCompare.isPending} onClick={onRetry}>
                {startCompare.isPending ? "重试中…" : "重试本次比对"}
              </Button>
            )}
            {!live && (
              <Button kind="secondary" onClick={() => nav(`/workspace/${wsId}/new`)}>
                返回配置
              </Button>
            )}
            {job?.status === "completed" && (
              <Button kind="primary" onClick={() => nav(`/workspace/${wsId}/job/${jobId}`)}>
                查看报告
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
