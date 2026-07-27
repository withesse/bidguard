// 屏 4 · 检测报告 · 交叉矩阵 —— 移植自 app-design/project/src/c/bid-b.jsx (BidScrMatrix)
// 数据驱动：直接消费 CompareSummaryDto（原生），无真实结果则真空态。
import { Fragment, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { C, severityColor } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill, SegControl } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { useCompareSummary } from "../queries/data";
import type { Screen } from "../routes";
import type { Collusion, Fingerprint } from "../engine";
import type { CompareSummaryDto } from "../api/types";
import { docColor, docTag } from "../utils/docTag";
import { simBand } from "../utils/clusterUi";

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
  docIds: string[];
  /** 主口径：残差·剔除后·聚类覆盖率（风险分级与围标信号①的唯一输入）。 */
  matrix: number[][];
  /** 未对减口径（原始相似度）；无对减时等于 matrix。 */
  matrixOriginal: number[][];
  /** 区段口径（对齐区段按 chunk 去重后覆盖率）；无区段时为 null → 不提供口径切换。 */
  segMatrix: number[][] | null;
  peakPct: number;
  segPeakPct: number;
  peakColor: string;
  peakPair: string;
  /** W3-2 招标对减：剔除的引用块数（0=无对减）与原始峰值（对照用）。 */
  tenderRefCount: number;
  peakOriginalPct: number;
  conclusion: { pill: string; statement: string; desc: string };
  pairRows: PairRow[];
  insights: Insight[];
  forensicSignals: { tag: string; fg: string; bg: string; detail: string; weight: number }[];
  /** M6 商务标数值层：有清单数据才提供「商务标数值」入口（独立屏，决策 5）。 */
  hasNumeric: boolean;
  numericAlarmPairs: number;
  numericArithErrors: number;
}

/** 区段口径峰值有效（非空矩阵且有非零项）→ 才提供口径切换（旧任务/无区段不切）。 */
function hasSegmentCaliber(seg: number[][] | null): boolean {
  return !!seg && seg.length > 0 && seg.some((row) => row.some((v) => v > 0));
}

// 围标信号 kind → 洞察标签配色。取证四类走「取证/图片/错误」红/橙档，规避走最高红档；
// 未知 kind 回落「相似」（旧格式 collusion_json 缺字段不报错）。
const KIND_META: Record<string, { tag: string; fg: string; bg: string }> = {
  metadata: { tag: "指纹", fg: C.danger, bg: C.dangerSoft },
  cluster: { tag: "雷同", fg: C.warn, bg: C.warnSoft },
  sharedTerms: { tag: "同源", fg: C.warn, bg: C.warnSoft },
  facts: { tag: "报价", fg: C.warn, bg: C.warnSoft },
  rsid: { tag: "取证", fg: C.danger, bg: C.dangerSoft },
  pdfLineage: { tag: "取证", fg: C.danger, bg: C.dangerSoft },
  imageReuse: { tag: "图片", fg: C.hi2, bg: C.hi2Soft },
  sharedErrors: { tag: "错误", fg: C.hi4, bg: C.hi4Soft },
  evasion: { tag: "规避特征", fg: C.danger, bg: C.dangerSoft },
  // W3-3 多家异常一致：独立「待复核」档（不自动 high）；detail 自带涉嫌措辞 + 条例第四十条 + 评标委员会脚注。
  multiDocAnomaly: { tag: "待复核 · 涉嫌一致", fg: C.danger, bg: C.dangerSoft },
  // M6 商务标数值层（W5-6）：四类数值信号 + 后置的机制反事实。detail 自带 §1.5 口径/线索/核对措辞。
  numericIdentical: { tag: "清单雷同率", fg: C.danger, bg: C.dangerSoft },
  numericArithError: { tag: "共享算术错误", fg: C.hi4, bg: C.hi4Soft },
  numericPattern: { tag: "线索 · 规律性差异", fg: C.hi2, bg: C.hi2Soft },
  numericCorrelation: { tag: "单价相关性", fg: C.hi3, bg: C.hi3Soft },
  numericMechanism: { tag: "评标机制反事实", fg: C.hi3, bg: C.hi3Soft },
};
const KIND_META_DEFAULT = { tag: "相似", fg: C.hi3, bg: C.brandSoft };
// 取证指纹折叠区消费的信号 kind（rsid/PDF 血缘/图片同源/共同错误）。
const FORENSIC_KINDS = ["rsid", "pdfLineage", "imageReuse", "sharedErrors"];
const FORENSIC_DISCLAIMER =
  "取证信号未命中不构成清白证明（另存为 / 元数据清洗可消除痕迹）；是否构成围标须由评标委员会依法认定。";

function sev(pct: number): { c: string; label: string } {
  const b = simBand(pct);
  return { c: b.color, label: b.label };
}

const LEVEL_META: Record<string, { pill: string; color: string; statement: string }> = {
  high: { pill: "围标嫌疑 · 高", color: C.danger, statement: "命中多项同源信号，高度疑似围标，建议立即人工复核。" },
  medium: { pill: "重点复核 · 中", color: C.hi3, statement: "存在明显雷同与同源迹象，建议重点复核核心章节。" },
  low: { pill: "轻度雷同 · 低", color: C.hi2, statement: "检出一定程度雷同，多为通用模板，建议抽查。" },
  none: { pill: "未见明显围标", color: C.ink, statement: "各份标书差异充分，未见高度雷同或同源迹象。" },
};

/** 由某口径矩阵派生「对比结果一览」行（按百分比降序）。矩阵切口径时随之刷新。 */
function buildPairRows(matrix: number[][], docs: ViewDoc[]): PairRow[] {
  const n = docs.length;
  const rows: PairRow[] = [];
  for (let i = 0; i < n; i++)
    for (let j = i + 1; j < n; j++) {
      const pct = Math.round((matrix[i]?.[j] ?? 0) * 100);
      const sv = sev(pct);
      rows.push({
        pair: `${docs[i].tag} × ${docs[j].tag}`,
        pct,
        label: sv.label,
        c: sv.c,
        secs: `${docs[i].short} ↔ ${docs[j].short}`,
      });
    }
  rows.sort((a, b) => b.pct - a.pct);
  return rows;
}

function fromSummary(sm: CompareSummaryDto): MatrixView {
  const docIds = sm.matrix?.documentIds ?? [];
  const matrix = sm.matrix?.matrix ?? [];
  const matrixOriginal = sm.matrix?.matrixOriginal ?? matrix;
  const segMatrix = sm.matrix?.segmentMatrix ?? null;
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

  const pairRows = buildPairRows(matrix, docs);

  // 围标综合判定驱动结论与洞察
  const collusion = sm.collusion as unknown as Collusion | undefined;
  const level = collusion?.level ?? "none";
  const lv = LEVEL_META[level] ?? LEVEL_META.none;
  const signals = collusion?.signals ?? [];

  // §1.5 措辞分级：confirmed 用红色「规避特征」+ 强措辞；仅 suspect（无 confirmed）软化为
  // 「异常字符（可能来自复制粘贴）」，避免把复制粘贴零宽残留说成规避。判级取自各文档 evasionSummary
  // （与 collusion evasion 信号同源 evasion_json，一致）——纯呈现分支，不改融合规则。
  const anyEvasionConfirmed = sm.documents.some((d) => d.evasionSummary?.severity === "confirmed");
  const insights: Insight[] = signals.map((sig) => {
    if (sig.kind === "evasion" && !anyEvasionConfirmed) {
      return {
        tag: "异常字符",
        fg: C.warn,
        bg: C.warnSoft,
        title: `信号权重 ${(sig.weight * 100).toFixed(0)}%`,
        body: "检测到异常字符（可能来自复制粘贴），建议人工留意；未达规避特征确认级，未必构成规避。",
      };
    }
    const meta = KIND_META[sig.kind] ?? KIND_META_DEFAULT;
    return { ...meta, title: `信号权重 ${(sig.weight * 100).toFixed(0)}%`, body: sig.detail };
  });
  // 取证指纹折叠区：逐条列出 rsid/PDF 血缘/图片同源/共同错误信号（明细含天干对与免责纪律）。
  const forensicSignals = signals
    .filter((sig) => FORENSIC_KINDS.includes(sig.kind))
    .map((sig) => {
      const meta = KIND_META[sig.kind] ?? KIND_META_DEFAULT;
      return { tag: meta.tag, fg: meta.fg, bg: meta.bg, detail: sig.detail, weight: sig.weight };
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
  const hasSeg = hasSegmentCaliber(segMatrix);
  return {
    docs,
    docIds,
    matrix,
    matrixOriginal,
    segMatrix: hasSeg ? segMatrix : null,
    peakPct,
    segPeakPct: Math.round((sm.matrix?.segmentPeak ?? 0) * 100),
    peakColor,
    peakPair: `${docs[pi].tag} ←→ ${docs[pj].tag}`,
    tenderRefCount: sm.summary?.tenderRefChunkCount ?? 0,
    peakOriginalPct: Math.round((sm.matrix?.peakOriginal ?? sm.matrix?.peak ?? 0) * 100),
    conclusion: { pill: lv.pill, statement, desc },
    pairRows,
    insights,
    forensicSignals,
    hasNumeric: (sm.numeric?.pairs?.length ?? 0) > 0,
    numericAlarmPairs: sm.numeric?.pairs?.filter((p) => p.alarm).length ?? 0,
    numericArithErrors:
      sm.numeric?.pairs?.reduce((s, p) => s + (p.sharedArithErrors?.length ?? 0), 0) ?? 0,
  };
}

export function MatrixScreen({ onGo, jobId }: { onGo: (s: Screen) => void; jobId?: string }) {
  const { dark } = useTheme();
  const nav = useNavigate();
  const { wsId } = useParams<{ wsId: string }>();
  // 口径切换（§1.4）：null=跟随后端默认口径；用户手动切换后固定。
  const [mode, setMode] = useState<"cluster" | "segment" | null>(null);
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
  // 主口径永远是聚类·剔除后（风险分级/围标判定的唯一输入，与峰值卡一致）；矩阵展示口径可切到
  // 区段。默认聚类：区段覆盖天然更稀疏（无对齐区段的文档对区段值=0），作默认易被误读为「不相似」。
  const activeMode: "cluster" | "segment" =
    (mode ?? "cluster") === "segment" && v.segMatrix ? "segment" : "cluster";
  const activeMatrix = activeMode === "segment" && v.segMatrix ? v.segMatrix : v.matrix;
  const activePairRows = buildPairRows(activeMatrix, v.docs);
  const caliberLabel = activeMode === "segment" ? "区段口径" : "聚类口径（剔除后）";
  const goSegments = (r: number, c: number) =>
    nav(`/workspace/${wsId}/job/${jobId}/segments?a=${v.docIds[r]}&b=${v.docIds[c]}`);
  // 单元格对照口径（角标）：聚类模式对照未对减原始值；区段模式对照聚类剔除后值。差异 >10pp 标注。
  const cornerOf = (r: number, c: number): { pct: number; hot: boolean } | null => {
    if (r === c) return null;
    const primary = Math.round((activeMatrix[r]?.[c] ?? 0) * 100);
    const other =
      activeMode === "segment"
        ? Math.round((v.matrix[r]?.[c] ?? 0) * 100)
        : Math.round((v.matrixOriginal[r]?.[c] ?? 0) * 100);
    if (other === primary) return null;
    return { pct: other, hot: Math.abs(other - primary) > 10 };
  };
  const titleOf = (r: number, c: number): string => {
    if (r === c) return "";
    const cluster = Math.round((v.matrix[r]?.[c] ?? 0) * 100);
    const original = Math.round((v.matrixOriginal[r]?.[c] ?? 0) * 100);
    const seg = v.segMatrix ? Math.round((v.segMatrix[r]?.[c] ?? 0) * 100) : null;
    const parts = [`聚类·剔除后 ${cluster}%`, `未对减 ${original}%`];
    if (seg != null) parts.push(`区段 ${seg}%`);
    const spread = Math.max(cluster, original, seg ?? cluster) - Math.min(cluster, original, seg ?? cluster);
    const tail = spread > 10 ? `　口径差异 ${spread}pp（>10pp，注意口径选择）` : "";
    return `${v.docs[r].tag}×${v.docs[c].tag} · ${parts.join(" · ")}${tail} · 点击查看对齐区段`;
  };
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
              {v.segMatrix && (
                <div style={{ fontSize: 11, color: mute, marginTop: 8, lineHeight: 1.6 }}>
                  围标判定基于<b style={{ color: ink }}>聚类口径（剔除后 · 峰值 {v.peakPct}%）</b>；
                  区段口径峰值 {v.segPeakPct}% 仅供矩阵展示，不改变分级。
                </div>
              )}
              <div style={{ display: "flex", gap: 10, marginTop: 16, flexWrap: "wrap" }}>
                <Button kind="primary" size="md" icon="diff" onClick={() => onGo("compare")}>
                  查看逐对对比
                </Button>
                <Button kind="secondary" size="md" icon="folder" onClick={() => onGo("clusters")}>
                  查看重复条款
                </Button>
                {/* M6：数值证据为独立屏（决策 5），仅在识别出报价清单时出现 */}
                {v.hasNumeric && (
                  <Button kind="secondary" size="md" onClick={() => onGo("numeric")}>
                    商务标数值
                    {v.numericAlarmPairs > 0
                      ? ` · 告警 ${v.numericAlarmPairs} 对`
                      : v.numericArithErrors > 0
                        ? ` · 算术错误 ${v.numericArithErrors} 条`
                        : ""}
                  </Button>
                )}
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
              {/* W3-2 招标对减：剔除后为主口径，原始峰值作对照（Pill 切换完整版在 M5）。 */}
              {v.tenderRefCount > 0 && (
                <div style={{ fontSize: 11, color: mute, marginTop: 10, lineHeight: 1.6 }}>
                  已剔除招标文件引用 <span style={{ color: ink, fontWeight: 700 }}>{v.tenderRefCount}</span> 块
                  {v.peakOriginalPct !== v.peakPct && (
                    <>
                      {" · "}原始峰值 <span style={{ color: ink, fontWeight: 700 }}>{v.peakOriginalPct}%</span> → 剔除后{" "}
                      <span style={{ color: ink, fontWeight: 700 }}>{v.peakPct}%</span>
                    </>
                  )}
                  <br />
                  风险分级采用剔除后口径（对招标条款的合法逐字应答已剥离）
                </div>
              )}
            </div>
          </div>

          {/* 矩阵 + 洞察 */}
          <div style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: 16 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 24 }}>
              <div style={{ display: "flex", alignItems: "center", marginBottom: 12, gap: 10, flexWrap: "wrap" }}>
                <span style={{ fontSize: 14, fontWeight: 700, color: ink }}>
                  {n} × {n} 标书相似度矩阵
                </span>
                <span style={{ fontSize: 11, color: mute }}>
                  {activeMode === "segment" ? "对齐区段 · 覆盖率" : "语义级 · 段落粒度"}
                </span>
                <div style={{ flex: 1 }} />
                {v.segMatrix && (
                  <div style={{ width: 190 }}>
                    <SegControl
                      options={["聚类口径", "区段口径"]}
                      value={activeMode === "segment" ? 1 : 0}
                      onChange={(i) => setMode(i === 1 ? "segment" : "cluster")}
                    />
                  </div>
                )}
                <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <span style={{ fontSize: 10.5, color: mute }}>低</span>
                  <div
                    style={{
                      width: 72,
                      height: 8,
                      borderRadius: 4,
                      background: `linear-gradient(to right, ${C.okSoft}, ${C.hi1}, ${C.hi2}, ${C.hi3}, ${C.hi4})`,
                    }}
                  />
                  <span style={{ fontSize: 10.5, color: mute }}>高</span>
                </div>
              </div>
              {/* §1.4 双口径：单元格大数=当前口径，左上角标=对照口径（差异>10pp 标红）；tooltip 三口径对照。 */}
              <div style={{ fontSize: 10.5, color: mute, marginBottom: 12, lineHeight: 1.6 }}>
                当前口径 <b style={{ color: ink }}>{caliberLabel}</b> · 角标为对照口径
                {activeMode === "segment" ? "（聚类·剔除后）" : "（未对减原始）"}
                ；围标判定固定采用<b style={{ color: ink }}>聚类口径（剔除后）</b>，与展示口径无关。点单元格看对齐区段。
              </div>
              <BigMatrix docs={v.docs} matrix={activeMatrix} cornerOf={cornerOf} titleOf={titleOf} onCell={goSegments} />

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
                  对比结果一览 · {caliberLabel}
                </div>
                {activePairRows.map((row, i) => (
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
                        {d.fp &&
                          (d.fp.author ||
                            d.fp.lastModifiedBy ||
                            d.fp.app ||
                            d.fp.templateName ||
                            (d.fp.rsids?.length ?? 0) > 0 ||
                            (d.fp.fontSubsetTags?.length ?? 0) > 0) && (
                            <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                              {d.fp.author && <FpChip k="作者" v={d.fp.author} mute={mute} />}
                              {d.fp.lastModifiedBy && <FpChip k="改" v={d.fp.lastModifiedBy} mute={mute} />}
                              {d.fp.app && <FpChip k="软件" v={d.fp.app} mute={mute} />}
                              {d.fp.templateName && <FpChip k="模板" v={d.fp.templateName} mute={mute} />}
                              {(d.fp.rsids?.length ?? 0) > 0 && (
                                <FpChip k="rsid" v={`×${d.fp.rsids!.length}`} mute={mute} />
                              )}
                              {(d.fp.fontSubsetTags?.length ?? 0) > 0 && (
                                <FpChip k="字体" v={`×${d.fp.fontSubsetTags!.length}`} mute={mute} />
                              )}
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

              {/* 取证指纹折叠区：仅在有取证信号时渲染（§1.5 空态不渲染、不出现「检查通过」）。 */}
              {v.forensicSignals.length > 0 && (
                <details
                  open
                  style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}
                >
                  <summary style={{ fontSize: 13, fontWeight: 700, color: ink, cursor: "pointer" }}>
                    取证指纹 · {v.forensicSignals.length} 项
                  </summary>
                  <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 12 }}>
                    {v.forensicSignals.map((s, i) => (
                      <div
                        key={i}
                        style={{
                          padding: 12,
                          borderRadius: 8,
                          background: dark ? "rgba(255,255,255,0.025)" : C.paper2,
                          border: `1px solid ${border}`,
                        }}
                      >
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <Pill bg={s.bg} fg={s.fg} size={10}>
                            {s.tag}
                          </Pill>
                          <span style={{ fontSize: 11, color: mute, fontFamily: C.mono }}>
                            权重 {(s.weight * 100).toFixed(0)}%
                          </span>
                        </div>
                        <div style={{ fontSize: 11, color: mute, marginTop: 6, lineHeight: 1.6 }}>{s.detail}</div>
                      </div>
                    ))}
                  </div>
                  <div style={{ fontSize: 10.5, color: mute, marginTop: 12, lineHeight: 1.55, fontStyle: "italic" }}>
                    {FORENSIC_DISCLAIMER}
                  </div>
                </details>
              )}
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

function BigMatrix({
  docs,
  matrix,
  onCell,
  cornerOf,
  titleOf,
}: {
  docs: ViewDoc[];
  matrix: number[][];
  onCell?: (r: number, c: number) => void;
  /** 对照口径角标（左上）：差异 >10pp 时 hot 标红。null=无对照或对角线。 */
  cornerOf?: (r: number, c: number) => { pct: number; hot: boolean } | null;
  /** 单元格 tooltip（三口径对照）。 */
  titleOf?: (r: number, c: number) => string;
}) {
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
            const corner = diag ? null : (cornerOf?.(r, c) ?? null);
            return (
              <div
                key={c}
                onClick={diag ? undefined : () => onCell?.(r, c)}
                title={diag ? undefined : (titleOf?.(r, c) ?? "查看对齐区段")}
                role={diag ? undefined : "button"}
                tabIndex={diag ? undefined : 0}
                onKeyDown={
                  diag
                    ? undefined
                    : (e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          onCell?.(r, c);
                        }
                      }
                }
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
                {corner && (
                  <span
                    title="对照口径数值"
                    style={{
                      position: "absolute",
                      top: 4,
                      left: 6,
                      fontSize: 9,
                      fontWeight: 700,
                      fontFamily: C.mono,
                      color: corner.hot ? "#fff" : cellFg(val),
                      background: corner.hot ? C.danger : "rgba(0,0,0,0.12)",
                      padding: "0px 4px",
                      borderRadius: 4,
                      opacity: corner.hot ? 1 : 0.75,
                    }}
                  >
                    {corner.pct}
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
