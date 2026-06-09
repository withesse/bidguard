// 屏 2 · 我的任务 / 历史记录 —— 真实模式：本地持久化的历史任务；mock 模式：演示。
import { useEffect, useState } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill, SegControl } from "../components/primitives";
import { MiniMatrix } from "../components/Matrix";
import { BID_TASKS, type BidTask } from "../data/mock";
import { useTheme } from "../theme";
import type { Screen } from "../routes";
import { isTauri, listTasks, deleteTask, type TaskSummary } from "../engine";
import { getStarred, toggleStarred } from "../starred";

export function Tasks({
  onGo,
  onOpen,
  title,
  mode = "all",
}: {
  onGo: (s: Screen) => void;
  onOpen?: (id: string) => void;
  title?: string;
  mode?: "starred" | "all";
}) {
  if (isTauri() && onOpen) return <RealTasks onGo={onGo} onOpen={onOpen} title={title} mode={mode} />;
  return <MockTasks onGo={onGo} />;
}

// ─────────────────────────────────────────────────────────────
// 真实模式
// ─────────────────────────────────────────────────────────────
function fmtDate(ms: number): string {
  try {
    return new Date(ms).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

// 从结果派生的任务状态：围标 high/medium（或旧任务峰值≥80%）→ 需复核，否则正常。
export function reviewStatus(t: TaskSummary): { review: boolean; label: string; fg: string; bg: string } {
  const level = t.collusionLevel;
  const review = level ? level === "high" || level === "medium" : t.peak >= 0.8;
  return review
    ? { review: true, label: "需复核", fg: C.danger, bg: C.dangerSoft }
    : { review: false, label: "正常", fg: C.ok, bg: C.okSoft };
}

function RealTasks({
  onGo,
  onOpen,
  title,
  mode,
}: {
  onGo: (s: Screen) => void;
  onOpen: (id: string) => void;
  title?: string;
  mode: "starred" | "all";
}) {
  const { dark } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const [tasks, setTasks] = useState<TaskSummary[] | null>(null);
  const [starred, setStarred] = useState<Set<string>>(() => getStarred());
  const [q, setQ] = useState("");
  const [statusFilter, setStatusFilter] = useState(0); // 0 全部 / 1 需复核 / 2 正常

  const reload = () => listTasks().then(setTasks).catch(() => setTasks([]));
  useEffect(() => {
    reload();
  }, []);

  const onDelete = async (id: string) => {
    await deleteTask(id).catch(() => {});
    reload();
  };
  const onStar = (id: string) => {
    toggleStarred(id);
    setStarred(getStarred());
  };
  const starredCount = tasks ? tasks.filter((t) => starred.has(t.id)).length : 0;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title={title ?? "我的任务"}
        sub={
          tasks
            ? mode === "starred"
              ? `${starredCount} 个收藏任务 · 全部本地存储`
              : `共 ${tasks.length} 个查重记录 · 全部本地存储`
            : "加载中…"
        }
        search={
          tasks && tasks.length > 0
            ? { value: q, onChange: setQ, placeholder: "按名称搜索任务" }
            : undefined
        }
        actions={
          <Button kind="primary" size="md" icon="plus" onClick={() => onGo("new")}>
            新建任务
          </Button>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "24px 48px 40px" }}>
        {tasks === null ? (
          <div style={{ color: mute, fontSize: 13, textAlign: "center", padding: "60px 0" }}>加载中…</div>
        ) : tasks.length === 0 ? (
          <Empty
            icon="history"
            title="还没有查重记录"
            desc="去首页选 2-5 份标书，开始第一次交叉比对。"
            cta="新建查重任务"
            onCta={() => onGo("new")}
            ink={ink}
            mute={mute}
            dark={dark}
          />
        ) : mode === "starred" && starredCount === 0 ? (
          <Empty
            icon="folder"
            title="还没有收藏的任务"
            desc="在「历史记录」里点 ☆ 把常用的查重收藏到这里。"
            cta="去历史记录"
            onCta={() => onGo("history")}
            ink={ink}
            mute={mute}
            dark={dark}
          />
        ) : (
          (() => {
            let base = mode === "starred" ? tasks.filter((t) => starred.has(t.id)) : tasks;
            if (statusFilter === 1) base = base.filter((t) => reviewStatus(t).review);
            else if (statusFilter === 2) base = base.filter((t) => !reviewStatus(t).review);
            const shown = base.filter((t) => !q || t.name.toLowerCase().includes(q.toLowerCase()));
            return (
              <>
                <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
                  <SegControl
                    options={["全部", "需复核", "正常"]}
                    value={statusFilter}
                    onChange={setStatusFilter}
                  />
                  <div style={{ flex: 1 }} />
                  <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>{shown.length} 个</span>
                </div>
                {shown.length === 0 ? (
                  <div style={{ color: mute, fontSize: 13, textAlign: "center", padding: "40px 0" }}>
                    {q ? `没有匹配「${q}」的任务。` : "没有符合该筛选的任务。"}
                  </div>
                ) : (
                  <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
                    {shown.map((t) => (
                      <RealTaskRow
                        key={t.id}
                        t={t}
                        starred={starred.has(t.id)}
                        onOpen={() => onOpen(t.id)}
                        onDelete={() => onDelete(t.id)}
                        onStar={() => onStar(t.id)}
                        ink={ink}
                        mute={mute}
                        border={border}
                      />
                    ))}
                  </div>
                )}
              </>
            );
          })()
        )}
      </div>
    </div>
  );
}

function Empty({
  icon,
  title,
  desc,
  cta,
  onCta,
  ink,
  mute,
  dark,
}: {
  icon: string;
  title: string;
  desc: string;
  cta: string;
  onCta: () => void;
  ink: string;
  mute: string;
  dark: boolean;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12, padding: "80px 0", color: mute }}>
      <div style={{ width: 52, height: 52, borderRadius: 14, background: dark ? "rgba(255,255,255,0.04)" : C.paper2, display: "flex", alignItems: "center", justifyContent: "center" }}>
        <Icon name={icon} size={22} style={{ color: mute }} />
      </div>
      <div style={{ fontSize: 14, fontWeight: 700, color: ink, fontFamily: C.serif }}>{title}</div>
      <div style={{ fontSize: 12.5 }}>{desc}</div>
      <Button kind="primary" size="md" icon="plus" onClick={onCta}>
        {cta}
      </Button>
    </div>
  );
}

function RealTaskRow({
  t,
  starred,
  onOpen,
  onDelete,
  onStar,
  ink,
  mute,
  border,
}: {
  t: TaskSummary;
  starred: boolean;
  onOpen: () => void;
  onDelete: () => void;
  onStar: () => void;
  ink: string;
  mute: string;
  border: string;
}) {
  const { dark } = useTheme();
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const pct = Math.round(t.peak * 100);
  const sevColor = pct >= 80 ? C.hi3 : pct >= 60 ? C.hi2 : C.ok;
  const sevLabel = pct >= 80 ? "高相似" : pct >= 60 ? "中相似" : "低相似";
  const st = reviewStatus(t);

  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "16px 20px",
        display: "grid",
        gridTemplateColumns: "92px 1fr 92px 92px 28px 24px",
        gap: 18,
        alignItems: "center",
      }}
    >
      <div onClick={onOpen} style={{ cursor: "pointer" }}>
        <MiniMatrix m={t.matrix} />
      </div>
      <div onClick={onOpen} style={{ cursor: "pointer", minWidth: 0 }}>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: ink, letterSpacing: "-0.005em", lineHeight: 1.35, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {t.name}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 9, marginTop: 6 }}>
          <span style={{ fontSize: 11, color: mute }}>
            {t.docCount} 份 · {t.pairCount} 对 · {t.clusterCount} 组雷同
          </span>
          <span style={{ fontSize: 11, color: mute }}>·</span>
          <span style={{ fontSize: 11, color: mute }}>{fmtDate(t.createdAt)}</span>
        </div>
      </div>
      <div onClick={onOpen} style={{ textAlign: "right", cursor: "pointer" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: sevColor, letterSpacing: "-0.012em", fontFamily: C.font, lineHeight: 1 }}>
          {pct}
          <span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>%</span>
        </div>
        <div style={{ fontSize: 10.5, color: mute, marginTop: 3 }}>{sevLabel} · 峰值</div>
      </div>
      <Pill bg={st.bg} fg={st.fg} size={11}>
        <span style={{ width: 5, height: 5, borderRadius: "50%", background: st.fg }} />
        {st.label}
      </Pill>
      <span
        onClick={onStar}
        title={starred ? "取消收藏" : "收藏到「我的任务」"}
        style={{
          fontSize: 17,
          lineHeight: 1,
          cursor: "pointer",
          color: starred ? "#E0A92E" : mute,
          textAlign: "center",
          userSelect: "none",
        }}
      >
        {starred ? "★" : "☆"}
      </span>
      <Icon name="x" size={13} style={{ color: mute, cursor: "pointer" }} onClick={onDelete} />
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// mock 模式（浏览器预览演示）
// ─────────────────────────────────────────────────────────────
function MockTasks({ onGo }: { onGo: (s: Screen) => void }) {
  const { dark } = useTheme();
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="我的任务"
        sub="共 24 个查重任务 · 本月新增 7 个"
        actions={
          <>
            <Button kind="ghost" size="md" icon="filter">
              筛选
            </Button>
            <Button kind="primary" size="md" icon="plus" onClick={() => onGo("new")}>
              新建任务
            </Button>
          </>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "24px 48px 40px" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
          <SegControl options={["全部", "进行中", "需复核", "已完成"]} value={0} />
          <div style={{ flex: 1 }} />
          <Pill bg={dark ? "rgba(255,255,255,0.04)" : C.white} fg={mute} style={{ border: `1px solid ${border}`, padding: "4px 10px" }}>
            最近 30 天
          </Pill>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {BID_TASKS.map((t, i) => (
            <MockTaskRow key={i} t={t} onClick={() => onGo("matrix")} />
          ))}
        </div>
      </div>
    </div>
  );
}

function MockTaskRow({ t, onClick }: { t: BidTask; onClick: () => void }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const sev = t.peak >= 80 ? "high" : t.peak >= 60 ? "mid" : "low";
  const sevColor = sev === "high" ? C.hi3 : sev === "mid" ? C.hi2 : C.ok;
  const sevLabel = sev === "high" ? "高相似" : sev === "mid" ? "中相似" : "低相似";
  const statusMeta = {
    running: { label: "进行中", fg: accent, bg: C.brandSoft },
    review: { label: "需复核", fg: C.warn, bg: C.warnSoft },
    done: { label: "已完成", fg: C.ok, bg: C.okSoft },
  }[t.status];

  return (
    <div
      onClick={onClick}
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "16px 20px",
        display: "grid",
        gridTemplateColumns: "160px 1fr 140px 110px 110px 24px",
        gap: 18,
        alignItems: "center",
        cursor: "pointer",
      }}
    >
      <MiniMatrix m={t.matrix} />
      <div>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: ink, letterSpacing: "-0.005em", lineHeight: 1.35 }}>{t.name}</div>
        <div style={{ display: "flex", alignItems: "center", gap: 9, marginTop: 6 }}>
          <span style={{ fontSize: 11, color: mute }}>{t.sub}</span>
          <span style={{ fontSize: 11, color: mute }}>·</span>
          <span style={{ fontSize: 11, color: mute }}>{t.time}</span>
          {t.status === "running" && (
            <div style={{ marginLeft: 4, width: 100, height: 3, background: dark ? "rgba(255,255,255,0.05)" : C.paper2, borderRadius: 2, overflow: "hidden" }}>
              <div style={{ height: "100%", width: `${t.progress * 100}%`, background: accent }} />
            </div>
          )}
        </div>
      </div>
      <Pill bg={sev === "high" ? C.dangerSoft : sev === "mid" ? C.warnSoft : C.okSoft} fg={sevColor} size={11}>
        <Icon name="info" size={10} />
        {t.hint}
      </Pill>
      <div style={{ textAlign: "right" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: sevColor, letterSpacing: "-0.012em", fontFamily: C.font, lineHeight: 1 }}>
          {t.peak}
          <span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>%</span>
        </div>
        <div style={{ fontSize: 10.5, color: mute, marginTop: 3 }}>{sevLabel} · 峰值</div>
      </div>
      <Pill bg={statusMeta.bg} fg={statusMeta.fg} size={11}>
        <span style={{ width: 5, height: 5, borderRadius: "50%", background: statusMeta.fg }} />
        {statusMeta.label}
      </Pill>
      <Icon name="chevR" size={13} style={{ color: mute }} />
    </div>
  );
}
