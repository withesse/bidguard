// 屏 4 · 检测报告 · 交叉矩阵 —— 移植自 app-design/project/src/c/bid-b.jsx (BidScrMatrix)
// 数据驱动：直接消费 CompareSummaryDto（原生），无真实结果则真空态。
import { Fragment } from "react";
import { C, severityColor } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { useCompareSummary } from "../queries/data";
import type { Screen } from "../routes";
import type { Collusion, Fingerprint } from "../engine";
import type { CompareSummaryDto } from "../api/types";
import { docColor, docTag } from "../utils/docTag";

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

interface ViewDoc {
  tag: string;
  short: string;
  full: string;
  color: string;
  note?: string;
  fp?: Fingerprint;
}
interface PairRow {
  pair: string;
  pct: number;
  label: string;
  c: string;
  secs: string;
}
interface Insight {
  tag: string;
  fg: string;
  bg: string;
  title: string;
  body: string;
}
interface MatrixView {
  docs: ViewDoc[];
  matrix: number[][];
  peakPct: number;
  peakColor: string;
  peakPair: string;
  conclusion: { pill: string; statement: string; desc: string };
  pairRows: PairRow[];
  insights: Insight[];
}


function sev(pct: number): { c: string; label: string } {
  if (pct >= 80) return { c: C.danger, label: "高度雷同" };
  if (pct >= 60) return { c: C.hi3, label: "高相似" };
  if (pct >= 30) return { c: C.hi2, label: "中相似" };
  return { c: C.hi1, label: "低相似" };
}

const LEVEL_META: Record<string, { pill: string; color: string; statement: string }> = {
  high: { pill: "围标嫌疑 · 高", color: C.danger, statement: "命中多项同源信号，高度疑似围标，建议立即人工复核。" },
  medium: { pill: "重点复核 · 中", color: C.hi3, statement: "存在明显雷同与同源迹象，建议重点复核核心章节。" },
  low: { pill: "轻度雷同 · 低", color: C.hi2, statement: "检出一定程度雷同，多为通用模板，建议抽查。" },
  none: { pill: "未见明显围标", color: C.ink, statement: "各份标书差异充分，未见高度雷同或同源迹象。" },
};

function fromSummary(sm: CompareSummaryDto): MatrixView {
  const docIds = sm.matrix?.documentIds ?? [];
  const matrix = sm.matrix?.matrix ?? [];
  const byId = new Map(sm.documents.map((d) => [d.id, d]));
  const fpOf = (id: string): Fingerprint => {
    try {
      return { ...EMPTY_FP, ...JSON.parse(byId.get(id)?.fingerprintJson ?? "{}") };
    } catch {
      return EMPTY_FP; // 指纹损坏不影响主报告
    }
  };
  const n = docIds.length;
  const docs: ViewDoc[] = docIds.map((id, i) => {
    const d = byId.get(id);
    const name = d?.fileName ?? "未知文档";
    return {
      tag: docTag(i),
      short: name.replace(/\.[^.]+$/, "").slice(0, 8),
      full: name,
      color: docColor(i),
      note: d?.parseError ?? undefined,
      fp: fpOf(id),
    };
  });

  let pi = 0;
  let pj = n > 1 ? 1 : 0;
  let pv = -1;
  for (let i = 0; i < n; i++)
    for (let j = i + 1; j < n; j++)
      if (matrix[i][j] > pv) {
        pv = matrix[i][j];
        pi = i;
        pj = j;
      }
  const peakPct = Math.round((sm.matrix?.peak || 0) * 100);

  const pairRows: PairRow[] = [];
  for (let i = 0; i < n; i++)
    for (let j = i + 1; j < n; j++) {
      const pct = Math.round(matrix[i][j] * 100);
      const sv = sev(pct);
      pairRows.push({ pair: `${docs[i].tag} × ${docs[j].tag}`, pct, label: sv.label, c: sv.c, secs: `${docs[i].short} ↔ ${docs[j].short}` });
    }
  pairRows.sort((a, b) => b.pct - a.pct);

  // 围标综合判定驱动结论与洞察
  const collusion = sm.collusion as unknown as Collusion | undefined;
  const level = collusion?.level ?? "none";
  const lv = LEVEL_META[level] ?? LEVEL_META.none;
  const signals = collusion?.signals ?? [];

  const insights: Insight[] = signals.map((sig) => {
    const meta =
      sig.kind === "metadata"
        ? { tag: "指纹", fg: C.danger, bg: C.dangerSoft }
        : sig.kind === "cluster"
          ? { tag: "雷同", fg: C.warn, bg: C.warnSoft }
          : sig.kind === "sharedTerms"
            ? { tag: "同源", fg: C.warn, bg: C.warnSoft }
            : { tag: "相似", fg: C.hi3, bg: C.brandSoft };
    return { ...meta, title: `信号权重 ${(sig.weight * 100).toFixed(0)}%`, body: sig.detail };
  });
  const seen = new Set<string>();
  docs.forEach((dv, i) =>
    dv.fp?.riskFlags.forEach((f) => {
      if (!seen.has(f)) {
        seen.add(f);
        insights.push({ tag: "元数据", fg: C.danger, bg: C.dangerSoft, title: `${docs[i].tag} · 指纹`, body: f });
      }
    }),
  );
  if (insights.length === 0)
    insights.push({
      tag: "差异",
      fg: C.ok,
      bg: C.okSoft,
      title: "未发现明显雷同",
      body: "各份标书两两相似度均在低位，未检出共享作者或元数据异常。",
    });

  const peakColor = lv.color === C.ink && peakPct >= 60 ? C.hi3 : lv.color;
  const statement = level === "high" || level === "medium" ? `${docs[pi].tag}、${docs[pj].tag} 等标书${lv.statement}` : lv.statement;
  const desc =
    (signals.length ? signals.map((sig) => sig.detail).join("；") + "。" : "") +
    `本次共比对 ${n} 份标书、${(n * (n - 1)) / 2} 对组合，峰值相似度 ${peakPct}%，全部在本地完成。`;
  return {
    docs,
    matrix,
    peakPct,
    peakColor,
    peakPair: `${docs[pi].tag} ←→ ${docs[pj].tag}`,
    conclusion: { pill: lv.pill, statement, desc },
    pairRows,
    insights,
  };
}

export function MatrixScreen({ onGo, jobId }: { onGo: (s: Screen) => void; jobId?: string }) {
  const { dark } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const toast = useToast();
  const { data: sm, isLoading } = useCompareSummary(jobId);
  if (!sm || !sm.matrix || sm.matrix.documentIds.length < 2) {
    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: bg,
          color: mute,
          fontSize: 13,
        }}
      >
        {isLoading ? "正在加载报告…" : "暂无可展示的检测报告 —— 完成一次查重后在此查看。"}
      </div>
    );
  }
  const v = fromSummary(sm);
  const n = v.docs.length;
  const share = async () => {
    const text = [
      "标书查重报告",
      `参评 ${n} 份 · ${(n * (n - 1)) / 2} 对比对 · 峰值相似度 ${v.peakPct}%`,
      v.conclusion.statement,
      "",
      ...v.pairRows.map((row) => `${row.pair}  ${row.pct}%  ${row.label}`),
    ].join("\n");
    try {
      await navigator.clipboard.writeText(text);
      toast.show("报告摘要已复制到剪贴板", "success");
    } catch {
      toast.show("复制失败，可改用「导出报告」", "error");
    }
  };

  return (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="检测报告"
        sub={`本地完成 · ${n} 份标书 · ${(n * (n - 1)) / 2} 对比对`}
        actions={
          <>
            <Button kind="ghost" size="md" icon="share" onClick={share}>
              分享
            </Button>
            <Button kind="secondary" size="md" icon="download" onClick={() => onGo("export")}>
              导出报告
            </Button>
            <Button kind="primary" size="md" icon="diff" onClick={() => onGo("compare")}>
              逐对对比
            </Button>
          </>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "24px 40px 40px" }}>
        <div style={{ maxWidth: 1200, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
          {/* 结论 + 峰值 */}
          <div
            style={{
              background: cardBg,
              border: `1px solid ${border}`,
              borderRadius: 14,
              padding: "22px 28px",
              display: "grid",
              gridTemplateColumns: "1.6fr 1fr",
              gap: 32,
            }}
          >
            <div>
              <Pill bg={`${v.peakColor}1a`} fg={v.peakColor} size={11}>
                <Icon name="info" size={10} />
                {v.conclusion.pill}
              </Pill>
              <div
                style={{
                  fontSize: 22,
                  fontWeight: 700,
                  color: ink,
                  marginTop: 10,
                  letterSpacing: "-0.014em",
                  fontFamily: C.serif,
                  lineHeight: 1.3,
                }}
              >
                {v.conclusion.statement}
              </div>
              <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.65 }}>{v.conclusion.desc}</div>
              <div style={{ display: "flex", gap: 10, marginTop: 16 }}>
                <Button kind="primary" size="md" icon="diff" onClick={() => onGo("compare")}>
                  查看逐对对比
                </Button>
                <Button kind="secondary" size="md" icon="folder" onClick={() => onGo("clusters")}>
                  查看重复条款
                </Button>
              </div>
            </div>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "flex-end",
                justifyContent: "center",
                textAlign: "right",
              }}
            >
              <div
                style={{
                  fontSize: 10.5,
                  fontWeight: 700,
                  color: mute,
                  letterSpacing: "0.08em",
                  textTransform: "uppercase",
                }}
              >
                峰值相似度
              </div>
              <div
                style={{
                  fontSize: 96,
                  fontWeight: 700,
                  color: v.peakColor,
                  letterSpacing: "-0.04em",
                  lineHeight: 1,
                  fontFamily: C.font,
                  marginTop: 4,
                }}
              >
                {v.peakPct}
                <span style={{ fontSize: 36, color: mute, fontWeight: 500 }}>%</span>
              </div>
              <div style={{ fontSize: 12, color: mute, marginTop: 8 }}>
                出现在 <span style={{ color: ink, fontWeight: 700 }}>{v.peakPair}</span> 之间
              </div>
            </div>
          </div>

          {/* 矩阵 + 洞察 */}
          <div style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: 16 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 24 }}>
              <div style={{ display: "flex", alignItems: "baseline", marginBottom: 18 }}>
                <span style={{ fontSize: 14, fontWeight: 700, color: ink }}>
                  {n} × {n} 标书相似度矩阵
                </span>
                <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>语义级 · 段落粒度</span>
                <div style={{ flex: 1 }} />
                <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <span style={{ fontSize: 10.5, color: mute }}>低</span>
                  <div
                    style={{
                      width: 92,
                      height: 8,
                      borderRadius: 4,
                      background: `linear-gradient(to right, ${C.okSoft}, ${C.hi1}, ${C.hi2}, ${C.hi3}, ${C.hi4})`,
                    }}
                  />
                  <span style={{ fontSize: 10.5, color: mute }}>高</span>
                </div>
              </div>
              <BigMatrix docs={v.docs} matrix={v.matrix} />

              <div
                style={{
                  marginTop: 18,
                  paddingTop: 18,
                  borderTop: `1px solid ${border}`,
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <div
                  style={{
                    fontSize: 10.5,
                    fontWeight: 700,
                    color: mute,
                    letterSpacing: "0.06em",
                    textTransform: "uppercase",
                    marginBottom: 4,
                  }}
                >
                  对比结果一览
                </div>
                {v.pairRows.map((row, i) => (
                  <div
                    key={i}
                    style={{
                      display: "grid",
                      gridTemplateColumns: "64px 60px 1fr 110px",
                      gap: 12,
                      alignItems: "center",
                      padding: "6px 8px",
                      borderRadius: 6,
                      background: i === 0 && row.pct >= 80 ? (dark ? "rgba(181,69,69,0.10)" : C.dangerSoft) : "transparent",
                    }}
                  >
                    <span style={{ fontFamily: C.serif, fontWeight: 700, fontSize: 12.5, color: ink }}>{row.pair}</span>
                    <span
                      style={{ fontSize: 13, fontWeight: 700, color: row.c, fontFamily: C.mono, letterSpacing: "-0.005em" }}
                    >
                      {row.pct}%
                    </span>
                    <span style={{ fontSize: 11.5, color: mute }}>{row.secs}</span>
                    <Pill bg={`${row.c}1a`} fg={row.c} size={10.5}>
                      {row.label}
                    </Pill>
                  </div>
                ))}
              </div>
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
                <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>参评标书</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  {v.docs.map((d, i) => (
                    <div key={i} style={{ display: "flex", alignItems: "center", gap: 9 }}>
                      <div
                        style={{
                          width: 22,
                          height: 22,
                          borderRadius: 5,
                          background: d.color,
                          color: "#fff",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          fontSize: 12,
                          fontWeight: 700,
                          fontFamily: C.serif,
                          flexShrink: 0,
                        }}
                      >
                        {d.tag}
                      </div>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: 12, fontWeight: 600, color: ink, lineHeight: 1.3 }}>{d.short}</div>
                        <div
                          style={{
                            fontSize: 10.5,
                            color: d.note ? C.danger : mute,
                            marginTop: 1,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {d.note ? `解析失败：${d.note}` : d.full}
                        </div>
                        {d.fp && (d.fp.author || d.fp.lastModifiedBy || d.fp.app) && (
                          <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                            {d.fp.author && <FpChip k="作者" v={d.fp.author} mute={mute} />}
                            {d.fp.lastModifiedBy && <FpChip k="改" v={d.fp.lastModifiedBy} mute={mute} />}
                            {d.fp.app && <FpChip k="软件" v={d.fp.app} mute={mute} />}
                          </div>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
                <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>关键洞察</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
                  {v.insights.map((ins, i) => (
                    <div
                      key={i}
                      style={{
                        padding: 12,
                        borderRadius: 8,
                        background: dark ? "rgba(255,255,255,0.025)" : C.paper2,
                        border: `1px solid ${border}`,
                      }}
                    >
                      <Pill bg={ins.bg} fg={ins.fg} size={10}>
                        {ins.tag}
                      </Pill>
                      <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 7 }}>{ins.title}</div>
                      <div style={{ fontSize: 11, color: mute, marginTop: 4, lineHeight: 1.6 }}>{ins.body}</div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function FpChip({ k, v, mute }: { k: string; v: string; mute: string }) {
  const { dark } = useTheme();
  return (
    <span
      style={{
        fontSize: 9.5,
        color: mute,
        background: dark ? "rgba(255,255,255,0.05)" : "#F0EFF4",
        borderRadius: 4,
        padding: "1px 5px",
        maxWidth: 130,
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      }}
    >
      {k}·{v}
    </span>
  );
}

function BigMatrix({ docs, matrix }: { docs: ViewDoc[]; matrix: number[][] }) {
  const { dark } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cellFg = (v: number) => (v >= 0.7 ? "#fff" : ink);
  return (
    <div style={{ display: "grid", gridTemplateColumns: `92px repeat(${docs.length}, 1fr)`, gap: 6 }}>
      <div />
      {docs.map((d, i) => (
        <div
          key={i}
          style={{ textAlign: "center", display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}
        >
          <div
            style={{
              width: 26,
              height: 26,
              borderRadius: 6,
              background: d.color,
              color: "#fff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 14,
              fontWeight: 700,
              fontFamily: C.serif,
            }}
          >
            {d.tag}
          </div>
          <div style={{ fontSize: 10.5, color: mute, fontWeight: 600 }}>{d.short}</div>
        </div>
      ))}
      {docs.map((d, r) => (
        <Fragment key={r}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8, paddingRight: 8 }}>
            <div style={{ textAlign: "right" }}>
              <div style={{ fontSize: 11.5, fontWeight: 700, color: ink, fontFamily: C.serif }}>{d.tag}</div>
              <div style={{ fontSize: 10, color: mute }}>{d.short}</div>
            </div>
            <div
              style={{
                width: 22,
                height: 22,
                borderRadius: 5,
                background: d.color,
                color: "#fff",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 12,
                fontWeight: 700,
                fontFamily: C.serif,
              }}
            >
              {d.tag}
            </div>
          </div>
          {matrix[r].map((val, c) => {
            const diag = r === c;
            const isHot = val >= 0.9 && !diag;
            return (
              <div
                key={c}
                style={{
                  aspectRatio: "1.3 / 1",
                  borderRadius: 8,
                  background: diag ? (dark ? "rgba(255,255,255,0.04)" : C.paper2) : severityColor(val, C.okSoft),
                  color: diag ? mute : cellFg(val),
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  justifyContent: "center",
                  cursor: diag ? "default" : "pointer",
                  boxShadow: isHot ? `0 0 0 2px ${C.danger}` : "none",
                  position: "relative",
                }}
              >
                {diag ? (
                  "—"
                ) : (
                  <>
                    <span
                      style={{
                        fontSize: 22,
                        fontWeight: 700,
                        fontFamily: C.mono,
                        letterSpacing: "-0.014em",
                        lineHeight: 1,
                      }}
                    >
                      {(val * 100).toFixed(0)}
                    </span>
                    <span style={{ fontSize: 10, opacity: 0.7, fontWeight: 600, marginTop: 3 }}>%</span>
                  </>
                )}
                {isHot && (
                  <span
                    style={{
                      position: "absolute",
                      top: 5,
                      right: 6,
                      fontSize: 9.5,
                      fontWeight: 700,
                      color: "#fff",
                      background: "rgba(0,0,0,0.25)",
                      padding: "1px 5px",
                      borderRadius: 999,
                      letterSpacing: "0.04em",
                    }}
                  >
                    雷同
                  </span>
                )}
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
