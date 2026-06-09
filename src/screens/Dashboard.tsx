// 屏 0 · 首页 / 仪表盘 —— 概览统计 + 最近任务 + 快捷入口（区别于「新建查重任务」页）
import { useEffect, useState } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { MiniMatrix } from "../components/Matrix";
import { BID_TASKS, type BidTask } from "../data/mock";
import { useTheme } from "../theme";
import type { Screen } from "../routes";
import { isTauri, listTasks, type TaskSummary } from "../engine";
import { reviewStatus } from "./Tasks";

function fmtDate(ms: number): string {
  try {
    return new Date(ms).toLocaleString("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export function Dashboard({
  onGo,
  onOpen,
}: {
  onGo: (s: Screen) => void;
  onOpen: (id: string) => void;
}) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const real = isTauri();
  const [tasks, setTasks] = useState<TaskSummary[] | null>(real ? null : []);
  useEffect(() => {
    if (real) listTasks().then(setTasks).catch(() => setTasks([]));
  }, [real]);

  const list = tasks ?? [];
  const total = real ? list.length : 24;
  const clusterSum = real ? list.reduce((a, t) => a + t.clusterCount, 0) : 38;
  const flagged = real ? list.filter((t) => t.peak >= 0.8).length : 2;
  const peakMax = real ? (list.length ? Math.round(Math.max(...list.map((t) => t.peak)) * 100) : 0) : 92;

  const stats = [
    { label: "查重任务", value: String(total), icon: "folder", color: accent },
    { label: "雷同条款累计", value: String(clusterSum), icon: "diff", color: C.hi3 },
    { label: "围标嫌疑", value: String(flagged), icon: "info", color: C.danger },
    { label: "最高相似度", value: `${peakMax}%`, icon: "sparkle", color: peakMax >= 80 ? C.danger : C.ok },
  ];

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="首页"
        sub="概览与最近的查重"
        actions={
          <Button kind="primary" size="md" icon="plus" onClick={() => onGo("new")}>
            新建查重任务
          </Button>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 1080, margin: "0 auto", display: "flex", flexDirection: "column", gap: 22 }}>
          {/* Hero */}
          <div
            style={{
              background: dark ? "rgba(79,88,168,0.10)" : `${accent}08`,
              border: `1px solid ${accent}33`,
              borderRadius: 14,
              padding: "26px 30px",
            }}
          >
            <div style={{ fontSize: 11, color: accent, fontWeight: 700, letterSpacing: "0.08em", textTransform: "uppercase" }}>
              原本 · 标书查重
            </div>
            <div
              style={{
                fontSize: 24,
                fontWeight: 700,
                color: ink,
                marginTop: 8,
                fontFamily: C.serif,
                letterSpacing: "-0.018em",
                lineHeight: 1.3,
              }}
            >
              把候选的几份标书，一起摆在桌上看。
            </div>
            <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.6, maxWidth: 560 }}>
              选 2–5 份标书做交叉比对，识别条款级雷同、共用模板与围标嫌疑。全程本地完成，不上传任何文件。
            </div>
            <div style={{ display: "flex", gap: 10, marginTop: 16 }}>
              <Button kind="primary" size="md" icon="plus" onClick={() => onGo("new")}>
                新建查重任务
              </Button>
              <Button kind="secondary" size="md" icon="book" onClick={() => onGo("library")}>
                管理查重源
              </Button>
            </div>
          </div>

          {/* 统计 */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 14 }}>
            {stats.map((s) => (
              <div key={s.label} style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Icon name={s.icon} size={14} style={{ color: s.color }} />
                  <span style={{ fontSize: 11, color: mute, fontWeight: 600 }}>{s.label}</span>
                </div>
                <div
                  style={{
                    fontSize: 30,
                    fontWeight: 700,
                    color: ink,
                    marginTop: 8,
                    letterSpacing: "-0.02em",
                    fontFamily: C.font,
                  }}
                >
                  {s.value}
                </div>
              </div>
            ))}
          </div>

          {/* 最近任务 */}
          <div>
            <div style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
              <span style={{ fontSize: 14, fontWeight: 700, color: ink }}>最近的查重</span>
              <div style={{ flex: 1 }} />
              <Button kind="ghost" size="sm" iconRight="chevR" onClick={() => onGo("tasks")}>
                全部任务
              </Button>
            </div>
            {real && tasks === null ? (
              <div style={{ color: mute, fontSize: 13, textAlign: "center", padding: "40px 0" }}>加载中…</div>
            ) : real && list.length === 0 ? (
              <EmptyState onGo={onGo} ink={ink} mute={mute} border={border} cardBg={cardBg} dark={dark} />
            ) : real ? (
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                {list.slice(0, 5).map((t) => (
                  <RealRow key={t.id} t={t} onOpen={() => onOpen(t.id)} ink={ink} mute={mute} border={border} cardBg={cardBg} />
                ))}
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                {BID_TASKS.slice(0, 5).map((t, i) => (
                  <MockRow key={i} t={t} onClick={() => onGo("matrix")} ink={ink} mute={mute} border={border} cardBg={cardBg} />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function RealRow({
  t,
  onOpen,
  ink,
  mute,
  border,
  cardBg,
}: {
  t: TaskSummary;
  onOpen: () => void;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
}) {
  const pct = Math.round(t.peak * 100);
  const sev = pct >= 80 ? C.danger : pct >= 60 ? C.hi3 : C.ok;
  const st = reviewStatus(t);
  return (
    <div
      onClick={onOpen}
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "12px 16px",
        display: "grid",
        gridTemplateColumns: "52px 1fr 64px 16px",
        gap: 14,
        alignItems: "center",
        cursor: "pointer",
      }}
    >
      <MiniMatrix m={t.matrix} size={48} />
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 700, color: ink, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {t.name}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 4 }}>
          <Pill bg={st.bg} fg={st.fg} size={10}>
            {st.label}
          </Pill>
          <span style={{ fontSize: 11, color: mute, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {t.docCount} 份 · {t.clusterCount} 组雷同 · {fmtDate(t.createdAt)}
          </span>
        </div>
      </div>
      <div style={{ textAlign: "right" }}>
        <span style={{ fontSize: 16, fontWeight: 700, color: sev, fontFamily: C.font }}>
          {pct}
          <span style={{ fontSize: 10, color: mute }}>%</span>
        </span>
      </div>
      <Icon name="chevR" size={13} style={{ color: mute }} />
    </div>
  );
}

function MockRow({
  t,
  onClick,
  ink,
  mute,
  border,
  cardBg,
}: {
  t: BidTask;
  onClick: () => void;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
}) {
  const sev = t.peak >= 80 ? C.danger : t.peak >= 60 ? C.hi3 : C.ok;
  const review = t.peak >= 80;
  return (
    <div
      onClick={onClick}
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "12px 16px",
        display: "grid",
        gridTemplateColumns: "52px 1fr 64px 16px",
        gap: 14,
        alignItems: "center",
        cursor: "pointer",
      }}
    >
      <MiniMatrix m={t.matrix} size={48} />
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 700, color: ink, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {t.name}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 4 }}>
          <Pill bg={review ? C.dangerSoft : C.okSoft} fg={review ? C.danger : C.ok} size={10}>
            {review ? "需复核" : "正常"}
          </Pill>
          <span style={{ fontSize: 11, color: mute, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {t.sub} · {t.time}
          </span>
        </div>
      </div>
      <div style={{ textAlign: "right" }}>
        <span style={{ fontSize: 16, fontWeight: 700, color: sev, fontFamily: C.font }}>
          {t.peak}
          <span style={{ fontSize: 10, color: mute }}>%</span>
        </span>
      </div>
      <Icon name="chevR" size={13} style={{ color: mute }} />
    </div>
  );
}

function EmptyState({
  onGo,
  ink,
  mute,
  border,
  cardBg,
  dark,
}: {
  onGo: (s: Screen) => void;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
  dark: boolean;
}) {
  return (
    <div
      style={{
        background: cardBg,
        border: `1px dashed ${border}`,
        borderRadius: 12,
        padding: "48px 0",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 12,
      }}
    >
      <div
        style={{
          width: 52,
          height: 52,
          borderRadius: 14,
          background: dark ? "rgba(255,255,255,0.04)" : C.paper2,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="folder" size={22} style={{ color: mute }} />
      </div>
      <div style={{ fontSize: 14, fontWeight: 700, color: ink, fontFamily: C.serif }}>还没有查重记录</div>
      <div style={{ fontSize: 12.5, color: mute }}>选 2–5 份标书，开始第一次交叉比对。</div>
      <Button kind="primary" size="md" icon="plus" onClick={() => onGo("new")}>
        新建查重任务
      </Button>
    </div>
  );
}
