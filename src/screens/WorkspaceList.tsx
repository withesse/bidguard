// 首页：工作区列表 + 最近任务。「新建查重任务」一键建工作区直进配置页，
// 轻用户无感知工作区概念；重用户可在此管理多个工作区。
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { Icon } from "../design/Icon";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg, isTauri } from "../api/client";
import {
  useCreateWorkspace,
  useDeleteWorkspace,
  useJobs,
  useRenameWorkspace,
  useWorkspaces,
} from "../queries/data";
import { jobRoute, statusUi } from "../utils/jobStatus";

export function WorkspaceList() {
  const { dark } = useTheme();
  const nav = useNavigate();
  const toast = useToast();
  const { data: workspaces } = useWorkspaces();
  const { data: jobs } = useJobs();
  const create = useCreateWorkspace();
  const del = useDeleteWorkspace();
  const rename = useRenameWorkspace();
  const [confirmDel, setConfirmDel] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const commitRename = (id: string) => {
    const name = draft.trim();
    setRenaming(null);
    if (!name) return;
    rename.mutate(
      { id, name },
      { onError: (e) => toast.show("重命名失败：" + errMsg(e), "error") },
    );
  };

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;

  const onCreate = async () => {
    if (!isTauri()) {
      toast.show("桌面功能仅在应用内可用", "info");
      return;
    }
    try {
      const now = new Date();
      const name = `查重 ${now.getMonth() + 1}-${now.getDate()} ${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
      const ws = await create.mutateAsync(name);
      nav(`/workspace/${ws.id}/new`);
    } catch (e) {
      toast.show("创建失败：" + errMsg(e), "error");
    }
  };

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar
        title="首页"
        sub="离线标书交叉比对与围标识别"
        actions={
          <Button kind="primary" icon="plus" onClick={onCreate}>
            新建查重任务
          </Button>
        }
      />
      <div style={{ flex: 1, overflowY: "auto", padding: 24 }}>
        {/* 工作区 */}
        <SectionTitle>查重任务</SectionTitle>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
            gap: 12,
            marginBottom: 28,
          }}
        >
          {(workspaces ?? []).map((w) => {
            const st = w.latestJobStatus ? statusUi(w.latestJobStatus) : null;
            return (
              <div
                key={w.id}
                onClick={() => nav(`/workspace/${w.id}/new`)}
                style={{
                  background: cardBg,
                  border: `1px solid ${border}`,
                  borderRadius: 12,
                  padding: "14px 16px",
                  cursor: "pointer",
                  display: "flex",
                  flexDirection: "column",
                  gap: 8,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Icon name="folder" size={14} style={{ color: mute, flexShrink: 0 }} />
                  {renaming === w.id ? (
                    <input
                      autoFocus
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                      onClick={(e) => e.stopPropagation()}
                      onBlur={() => commitRename(w.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") commitRename(w.id);
                        if (e.key === "Escape") setRenaming(null);
                      }}
                      style={{
                        flex: 1,
                        minWidth: 0,
                        fontSize: 13,
                        fontWeight: 600,
                        color: ink,
                        background: "transparent",
                        border: `1px solid ${border}`,
                        borderRadius: 5,
                        padding: "2px 6px",
                        outline: "none",
                        fontFamily: C.font,
                      }}
                    />
                  ) : (
                    <div
                      style={{
                        flex: 1,
                        minWidth: 0,
                        fontSize: 13,
                        fontWeight: 600,
                        color: ink,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {w.name}
                    </div>
                  )}
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      setDraft(w.name);
                      setRenaming(w.id);
                    }}
                    style={{ fontSize: 11, color: mute, flexShrink: 0, padding: "2px 2px" }}
                    title="重命名"
                  >
                    ✎
                  </span>
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      if (confirmDel === w.id) {
                        del.mutate(w.id, {
                          onError: (err) => toast.show("删除失败：" + errMsg(err), "error"),
                        });
                        setConfirmDel(null);
                      } else {
                        setConfirmDel(w.id);
                        setTimeout(() => setConfirmDel((c) => (c === w.id ? null : c)), 2600);
                      }
                    }}
                    style={{
                      fontSize: 10.5,
                      color: confirmDel === w.id ? "#B54545" : mute,
                      fontWeight: confirmDel === w.id ? 700 : 400,
                      flexShrink: 0,
                      padding: "2px 4px",
                    }}
                    title="删除工作区（含文档与结果）"
                  >
                    {confirmDel === w.id ? "确认删除?" : "✕"}
                  </span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11, color: mute }}>
                  <span>{w.documentCount} 份文档</span>
                  {st && (
                    <Pill fg={st.fg} bg={st.bg} size={10}>
                      {st.label}
                    </Pill>
                  )}
                  <span style={{ marginLeft: "auto" }}>{w.updatedAt.slice(0, 10)}</span>
                </div>
              </div>
            );
          })}
          {(workspaces ?? []).length === 0 && (
            <div
              onClick={onCreate}
              style={{
                border: `1.5px dashed ${border}`,
                borderRadius: 12,
                padding: "28px 16px",
                textAlign: "center",
                color: mute,
                fontSize: 12.5,
                cursor: "pointer",
              }}
            >
              还没有查重任务 — 点击新建，导入 2-10 份标书开始比对
            </div>
          )}
        </div>

        {/* 最近任务 */}
        <SectionTitle>最近检测</SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {(jobs ?? [])
            .filter((j) => j.jobType === "compare")
            .slice(0, 8)
            .map((j) => {
              const st = statusUi(j.status);
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
                  <Icon name="diff" size={13} style={{ color: mute }} />
                  <div style={{ flex: 1, minWidth: 0, fontSize: 12.5, fontWeight: 600, color: ink }}>
                    {j.name ?? "未命名比对"}
                  </div>
                  <Pill fg={st.fg} bg={st.bg} size={10}>
                    {st.label}
                  </Pill>
                  <span style={{ fontSize: 11, color: mute }}>{j.createdAt.slice(0, 16).replace("T", " ")}</span>
                </div>
              );
            })}
          {(jobs ?? []).filter((j) => j.jobType === "compare").length === 0 && (
            <div style={{ fontSize: 12, color: mute, padding: "6px 2px" }}>暂无检测记录</div>
          )}
        </div>
      </div>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  const { dark } = useTheme();
  return (
    <div
      style={{
        fontSize: 11,
        fontWeight: 700,
        letterSpacing: "0.06em",
        color: dark ? "rgba(255,255,255,0.45)" : C.ink3,
        margin: "2px 2px 10px",
        textTransform: "uppercase",
      }}
    >
      {children}
    </div>
  );
}
