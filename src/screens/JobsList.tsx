// 任务列表：/history 全部、/starred 仅收藏。星标/删除落库，完成任务带迷你矩阵与围标徽标。
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Pill } from "../components/primitives";
import { MiniMatrix } from "../components/Matrix";
import { Icon } from "../design/Icon";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import type { JobDto } from "../api/types";
import { useDeleteJob, useJobs, useSetJobStarred } from "../queries/data";
import { jobRoute, statusUi } from "../utils/jobStatus";

export function JobsList({ title, mode }: { title: string; mode: "all" | "starred" }) {
  const { dark } = useTheme();
  const nav = useNavigate();
  const toast = useToast();
  const { data: jobs } = useJobs();
  const star = useSetJobStarred();
  const del = useDeleteJob();
  const [q, setQ] = useState("");
  const [confirmDel, setConfirmDel] = useState<string | null>(null);

  const rows = useMemo(
    () =>
      (jobs ?? [])
        .filter((j) => j.jobType === "compare")
        .filter((j) => (mode === "starred" ? j.starred : true))
        .filter((j) => !q.trim() || (j.name ?? "").includes(q.trim())),
    [jobs, q, mode],
  );

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;

  const matrixOf = (j: JobDto): number[][] | null => {
    try {
      return j.matrixJson ? (JSON.parse(j.matrixJson).matrix as number[][]) : null;
    } catch {
      return null;
    }
  };

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar
        title={title}
        sub={`${rows.length} 个检测任务`}
        search={{ value: q, onChange: setQ, placeholder: "搜索任务名…" }}
      />
      <div style={{ flex: 1, overflowY: "auto", padding: 24 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          {rows.map((j) => {
            const st = statusUi(j.status);
            const m = matrixOf(j);
            const needsReview = j.collusionLevel === "high" || j.collusionLevel === "medium";
            return (
              <div
                key={j.id}
                onClick={() => nav(jobRoute(j))}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  background: cardBg,
                  border: `1px solid ${border}`,
                  borderRadius: 10,
                  padding: "10px 14px",
                  cursor: "pointer",
                }}
              >
                <span
                  onClick={(e) => {
                    e.stopPropagation();
                    star.mutate(
                      { jobId: j.id, starred: !j.starred },
                      { onError: (err) => toast.show("操作失败：" + errMsg(err), "error") },
                    );
                  }}
                  title={j.starred ? "取消收藏" : "收藏"}
                  style={{
                    fontSize: 15,
                    color: j.starred ? "#C28430" : mute,
                    flexShrink: 0,
                    lineHeight: 1,
                  }}
                >
                  {j.starred ? "★" : "☆"}
                </span>
                {m ? (
                  <MiniMatrix m={m} size={34} />
                ) : (
                  <Icon name="diff" size={13} style={{ color: mute, flexShrink: 0 }} />
                )}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 12.5,
                      fontWeight: 600,
                      color: ink,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {j.name ?? "未命名比对"}
                  </div>
                  {j.message && <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>{j.message}</div>}
                </div>
                {needsReview && (
                  <Pill fg="#B54545" bg="rgba(181,69,69,0.12)" size={10} weight={700}>
                    需复核
                  </Pill>
                )}
                {j.status === "running" && (
                  <span style={{ fontSize: 11, color: mute, fontVariantNumeric: "tabular-nums" }}>
                    {Math.round(j.progress * 100)}%
                  </span>
                )}
                <Pill fg={st.fg} bg={st.bg} size={10}>
                  {st.label}
                </Pill>
                <span style={{ fontSize: 11, color: mute, flexShrink: 0 }}>
                  {j.createdAt.slice(0, 16).replace("T", " ")}
                </span>
                <span
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirmDel === j.id) {
                      del.mutate(j.id, {
                        onError: (err) => toast.show("删除失败：" + errMsg(err), "error"),
                      });
                      setConfirmDel(null);
                    } else {
                      setConfirmDel(j.id);
                      setTimeout(() => setConfirmDel((c) => (c === j.id ? null : c)), 2600);
                    }
                  }}
                  title="删除任务与其结果"
                  style={{
                    fontSize: 10.5,
                    color: confirmDel === j.id ? "#B54545" : mute,
                    fontWeight: confirmDel === j.id ? 700 : 400,
                    flexShrink: 0,
                    padding: "2px 4px",
                  }}
                >
                  {confirmDel === j.id ? "确认删除?" : "✕"}
                </span>
              </div>
            );
          })}
          {rows.length === 0 && (
            <div style={{ fontSize: 12.5, color: mute, padding: "18px 4px" }}>
              {mode === "starred" ? "还没有收藏的任务 — 点 ☆ 收藏重要检测" : "暂无检测任务 — 回到首页新建一次查重"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
