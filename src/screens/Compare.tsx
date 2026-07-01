// 屏 5 · 逐对对比 —— 真实模式：按段落匹配对渲染（重建文本 + 高亮雷同）；mock 模式：原全页设计。
import { useMemo, useState } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { useQuery } from "@tanstack/react-query";
import { useTheme, type Highlight } from "../theme";
import type { Screen } from "../routes";
import type { DiffOp } from "../engine";
import type { CompareSummaryDto } from "../api/types";
import * as api from "../api";
import { useCompareSummary } from "../queries/data";
import { docColor, docTag } from "../utils/docTag";

type HiScheme = Record<string, string>;

function hiScheme(name: Highlight): HiScheme {
  if (name === "rose")
    return { hi1: "#E89FAE", hi2: "#D86E84", hi3: "#B83F5E", hi4: "#8C2444", hi1soft: "#F8D9DF", hi2soft: "#F4C5CF", hi3soft: "#EFAFBE", hi4soft: "#E89DAE" };
  if (name === "blue")
    return { hi1: "#A6BDDE", hi2: "#6B8BC4", hi3: "#3D63A8", hi4: "#1E4080", hi1soft: "#D8E2F1", hi2soft: "#BDCFE7", hi3soft: "#9FB8DA", hi4soft: "#7E9DCB" };
  return { hi1: C.hi1, hi2: C.hi2, hi3: C.hi3, hi4: C.hi4, hi1soft: C.hi1Soft, hi2soft: C.hi2Soft, hi3soft: C.hi3Soft, hi4soft: C.hi4Soft };
}

export function Compare({ onGo, jobId }: { onGo: (s: Screen) => void; jobId?: string }) {
  const { data: sm } = useCompareSummary(jobId);
  if (sm && sm.matrix && sm.matrix.documentIds.length >= 2)
    return <RealCompare summary={sm} jobId={jobId!} onGo={onGo} />;
  return <EmptyCompare />;
}

function EmptyCompare() {
  const { dark } = useTheme();
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: dark ? "#15151B" : C.paper,
        color: dark ? "rgba(255,255,255,0.55)" : C.ink3,
        fontSize: 13,
      }}
    >
      暂无逐对对比数据 —— 完成一次查重后在此查看。
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// 真实模式
// ─────────────────────────────────────────────────────────────
function RealCompare({ summary, jobId, onGo }: { summary: CompareSummaryDto; jobId: string; onGo: (s: Screen) => void }) {
  const { dark, accent, highlight } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const paperBg = dark ? "#22222A" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const HI = hiScheme(highlight);

  const docIds = summary.matrix!.documentIds;
  const matrix = summary.matrix!.matrix;
  const byId = useMemo(() => new Map(summary.documents.map((d) => [d.id, d] as const)), [summary.documents]);
  const docName = (i: number) => byId.get(docIds[i])?.fileName ?? docTag(i);
  // 配对列表来自矩阵（不预取明细）；选中哪对才懒加载哪对的匹配段落。
  const pairs = useMemo(() => {
    const arr: { a: number; b: number; score: number }[] = [];
    for (let a = 0; a < docIds.length; a++)
      for (let b = a + 1; b < docIds.length; b++) arr.push({ a, b, score: matrix[a][b] });
    return arr.sort((x, y) => y.score - x.score);
  }, [docIds, matrix]);
  const [sel, setSel] = useState(0);
  const pair = pairs[sel] ?? pairs[0];
  const { data: matchesRaw, isLoading } = useQuery({
    queryKey: ["pairDetail", jobId, pair?.a, pair?.b],
    queryFn: () => api.getPairDetail(jobId, docIds[pair.a], docIds[pair.b]),
    enabled: !!pair,
    staleTime: Infinity,
  });
  const matches = matchesRaw ?? [];

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="逐对对比"
        sub={`${docTag(pair.a)} ${docName(pair.a)} × ${docTag(pair.b)} ${docName(pair.b)} · ${isLoading ? "加载中…" : `${matches.length} 处匹配`}`}
        actions={
          <Button kind="primary" size="md" icon="check" onClick={() => onGo("matrix")}>
            返回报告
          </Button>
        }
      />
      {/* 配对选择 */}
      <div
        style={{
          minHeight: 56,
          flexShrink: 0,
          padding: "10px 24px",
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexWrap: "wrap",
          borderBottom: `1px solid ${border}`,
          background: dark ? "rgba(255,255,255,0.02)" : C.paper2,
        }}
      >
        <span style={{ fontSize: 11, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>
          对比组合
        </span>
        <div style={{ display: "flex", gap: 4, alignItems: "center", flexWrap: "wrap" }}>
          {pairs.map((p, i) => {
            const pct = Math.round(p.score * 100);
            const active = i === sel;
            return (
              <div
                key={`${p.a}-${p.b}`}
                onClick={() => setSel(i)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setSel(i);
                  }
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "5px 10px 5px 6px",
                  borderRadius: 7,
                  background: active ? (dark ? "rgba(255,255,255,0.08)" : "#fff") : "transparent",
                  border: `1px solid ${active ? accent : "transparent"}`,
                  boxShadow: active && !dark ? "0 1px 2px rgba(0,0,0,0.06)" : "none",
                  cursor: "pointer",
                }}
              >
                <div style={{ display: "flex", gap: 2 }}>
                  {[p.a, p.b].map((d) => (
                    <Tag key={d} idx={d} size={18} />
                  ))}
                </div>
                <span
                  style={{
                    fontSize: 11.5,
                    fontWeight: 700,
                    fontFamily: C.mono,
                    color: pct >= 80 ? C.danger : pct >= 60 ? C.hi3 : pct >= 30 ? C.hi2 : C.hi1,
                  }}
                >
                  {pct}%
                </span>
              </div>
            );
          })}
        </div>
        <div style={{ flex: 1 }} />
        <Pill bg={HI.hi3soft} fg={HI.hi3} size={11}>
          高亮 = 两份共享的雷同片段
        </Pill>
      </div>

      {/* 匹配段落列表 */}
      <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "18px 24px 40px" }}>
        {matches.length === 0 ? (
          <div style={{ textAlign: "center", color: mute, fontSize: 13, padding: "60px 0" }}>
            {isLoading ? "正在加载该组合的匹配段落…" : "该组合未发现达到阈值的雷同段落，差异充分。"}
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 1200, margin: "0 auto" }}>
            {/* 列头 */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 56px 1fr", gap: 12, alignItems: "center" }}>
              <PaneHeader idx={pair.a} ink={ink} />
              <div />
              <PaneHeader idx={pair.b} ink={ink} />
            </div>
            {matches.map((m, i) => {
              const pct = Math.round(m.score * 100);
              const c = pct >= 80 ? C.danger : pct >= 60 ? HI.hi3 : HI.hi2;
              return (
                <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 56px 1fr", gap: 12, alignItems: "stretch" }}>
                  <SegPane diff={m.diff} side="a" paperBg={paperBg} border={border} ink={ink} mute={mute} HI={HI} />
                  <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 4 }}>
                    <span style={{ fontSize: 14, fontWeight: 700, color: c, fontFamily: C.mono }}>{pct}%</span>
                    <Icon name="diff" size={13} style={{ color: mute }} />
                  </div>
                  <SegPane diff={m.diff} side="b" paperBg={paperBg} border={border} ink={ink} mute={mute} HI={HI} />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function Tag({ idx, size = 18 }: { idx: number; size?: number }) {
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: 4,
        background: docColor(idx),
        color: "#fff",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: size * 0.58,
        fontWeight: 700,
        fontFamily: C.serif,
      }}
    >
      {docTag(idx)}
    </div>
  );
}

function PaneHeader({ idx, ink }: { idx: number; ink: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <Tag idx={idx} size={22} />
      <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>{docTag(idx)} 方</span>
    </div>
  );
}

// 重建一侧文本：A = eq + del；B = eq + ins。eq(共享) 高亮，独有片段淡化。
function SegPane({
  diff,
  side,
  paperBg,
  border,
  ink,
  mute,
  HI,
}: {
  diff: DiffOp[];
  side: "a" | "b";
  paperBg: string;
  border: string;
  ink: string;
  mute: string;
  HI: HiScheme;
}) {
  const uniqueOp = side === "a" ? "del" : "ins";
  return (
    <div
      style={{
        background: paperBg,
        borderRadius: 8,
        border: `1px solid ${border}`,
        padding: "14px 16px",
        fontSize: 13,
        lineHeight: 1.85,
        color: ink,
        fontFamily: C.font,
      }}
    >
      {diff
        .filter((d) => d.op === "eq" || d.op === uniqueOp)
        .map((d, i) =>
          d.op === "eq" ? (
            <span key={i} style={{ borderBottom: `2px solid ${HI.hi3}`, paddingBottom: 1 }}>
              {d.text}
            </span>
          ) : (
            <span key={i} style={{ color: mute, background: side === "a" ? HI.hi1soft : C.okSoft, borderRadius: 2 }}>
              {d.text}
            </span>
          ),
        )}
    </div>
  );
}
