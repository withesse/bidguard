// 屏 · 商务标数值证据（W5-4/W5-6，M6）—— 独立屏（§2 已拍板决策 5），Matrix 页提供入口。
// 上：逐项单价雷同率两两热力表（复用矩阵热力配色）+ 文档对选择器；
// 中：纯 SVG 散点图（对角线参考线 + 折扣带高亮 + 悬停 alignKey），不引图表库；
// 下：规律性/相关性面板 + 相同项/共享算术错误明细表（行点击跳 DocPreview 对应 chunk）。
// §1.5 铁律：雷同率口径、规律性「统一下浮」线索、相关性强证据条件、算术错误的计价软件提示
// 全部随后端 notes 原样展示 —— 本屏只呈现事实与比率，不下任何定性结论。
import { Fragment, useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C, severityColor } from "../design/tokens";
import { useTheme } from "../theme";
import { useCompareSummary } from "../queries/data";
import type { BoqScatterPoint, MechanismDto, NumericDto, NumericPairDto } from "../api/types";
import { docColor, docTag } from "../utils/docTag";
import {
  defaultPair,
  discountBand,
  flipStatement,
  identicalMatrix,
  isStrongCorrelation,
  mechanismPositionLabel,
  pairIndex,
  pairKey,
  patternLabel,
  pct1,
  priceSourceLabel,
  reasonLabel,
  SCATTER_MAX,
  shiftLabel,
  toSvgXY,
} from "../utils/numericView";

/** 散点绘图区边长（px）。坐标已由后端归一到中位价并裁剪至 [0,3]。 */
const PLOT = 380;
const PLOT_PAD = 34;

export function BusinessNumeric() {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();
  const { dark } = useTheme();
  const [sp, setSp] = useSearchParams();
  const { data: summary, isLoading } = useCompareSummary(jobId);

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const numeric: NumericDto | null = summary?.numeric ?? null;
  const docIds: string[] = numeric?.documentIds ?? summary?.matrix?.documentIds ?? [];
  const nameOf = (i: number) => {
    const d = summary?.documents.find((x) => x.id === docIds[i]);
    return d ? d.fileName.replace(/\.[^.]+$/, "") : (docIds[i] ?? "");
  };

  const pairs = useMemo(() => numeric?.pairs ?? [], [numeric]);
  const index = useMemo(() => pairIndex(pairs), [pairs]);
  const urlPair = sp.get("pair");
  const selected: NumericPairDto | null =
    (urlPair ? index.get(urlPair) : undefined) ?? defaultPair(pairs);
  const setPair = (p: NumericPairDto) => {
    const next = new URLSearchParams(sp);
    next.set("pair", pairKey(p.a, p.b));
    setSp(next, { replace: true });
  };

  const back = (
    <Button kind="secondary" size="sm" onClick={() => nav(`/workspace/${wsId}/job/${jobId}`)}>
      返回报告
    </Button>
  );

  // 空态：明确写清「为什么没有」——无清单表 / 数值层关闭 / 扫描件 PDF 不覆盖。
  if (!numeric || pairs.length === 0) {
    return (
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: bg }}>
        <Topbar title="商务标数值证据" sub="报价清单逐项比对" actions={back} />
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 40,
          }}
        >
          <div style={{ maxWidth: 560, fontSize: 12.5, color: mute, lineHeight: 1.85, textAlign: "center" }}>
            {isLoading
              ? "正在加载数值证据…"
              : !numeric
                ? "本次比对没有可用的报价清单数据。可能原因：参评文件中未识别出报价清单表；本次比对关闭了商务标数值层；或清单为扫描件 PDF——数值层仅覆盖 xlsx / docx / 文本 PDF 中可识别的清单表，扫描件走 OCR 不产表格行，不在覆盖范围内。"
                : "已识别到清单条目，但没有可比的文档对（跨文档对齐后双方均有单价的清单项不足），故不出任何数值结论。"}
          </div>
        </div>
      </div>
    );
  }

  const n = docIds.length;
  const heat = identicalMatrix(pairs, n);
  const alarmCount = pairs.filter((p) => p.alarm).length;
  const errCount = pairs.reduce((s, p) => s + p.sharedArithErrors.length, 0);

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: bg, overflow: "hidden" }}>
      <Topbar
        title="商务标数值证据"
        sub={`清单条目 ${numeric.itemCount} 条 · 跨文档对齐 ${numeric.alignedItemCount} 条 · 告警线 ${Math.round(numeric.identicalRateAlarm * 100)}%`}
        actions={back}
      />

      {/* 文档对选择器 + 概览徽标 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "10px 24px",
          borderBottom: `1px solid ${border}`,
          flexWrap: "wrap",
        }}
      >
        {pairs.map((p) => (
          <PairChip
            key={pairKey(p.a, p.b)}
            label={`${docTag(p.a)}×${docTag(p.b)}${p.identicalRate != null ? ` · ${Math.round(p.identicalRate * 100)}%` : ""}`}
            active={!!selected && selected.a === p.a && selected.b === p.b}
            alarm={p.alarm}
            mute={mute}
            border={border}
            onClick={() => setPair(p)}
          />
        ))}
        <span style={{ flex: 1 }} />
        {alarmCount > 0 && (
          <Pill fg={C.danger} bg={C.dangerSoft} size={10.5} weight={700}>
            达告警线 {alarmCount} 对
          </Pill>
        )}
        {errCount > 0 && (
          <Pill fg={C.hi4} bg={C.hi4Soft} size={10.5} weight={700}>
            共享算术错误 {errCount} 条
          </Pill>
        )}
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "20px 24px 40px" }}>
        <div style={{ maxWidth: 1280, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
          {/* §1.5 措辞：原样展示后端下发的三条说明，任何呈现层都不得省略 */}
          <div
            style={{
              background: cardBg,
              border: `1px solid ${border}`,
              borderRadius: 12,
              padding: "14px 18px",
              fontSize: 11.5,
              color: mute,
              lineHeight: 1.8,
            }}
          >
            <div>{numeric.notes.identicalRate}</div>
            <div>{numeric.notes.sharedArithError}</div>
            <div>{numeric.notes.coverage}</div>
          </div>

          {/* 雷同率热力表 + 散点图 */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 20 }}>
              <div style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>逐项单价雷同率</div>
              <div style={{ fontSize: 10.5, color: mute, marginTop: 6, lineHeight: 1.7 }}>
                单价按「分」比对；暂估价 / 暂列 / 信息价 / 甲供材行不进分母（招标人给定价各家本就相同）。
                可比条目不足 {numeric.minComparable} 项的文档对显示「—」，不出结论。
              </div>
              <div style={{ marginTop: 14 }}>
                <HeatTable heat={heat} n={n} nameOf={nameOf} onCell={(a, b) => {
                  const p = index.get(pairKey(a, b));
                  if (p) setPair(p);
                }} />
              </div>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 20 }}>
              <div style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>
                单价散点 · {selected ? `${docTag(selected.a)} × ${docTag(selected.b)}` : "—"}
              </div>
              <div style={{ fontSize: 10.5, color: mute, marginTop: 6, lineHeight: 1.7 }}>
                坐标 = 各自单价 ÷ 全体投标人该项中位价（裁剪至 0–{SCATTER_MAX}）。落在对角线上 = 单价完全一致；
                整体平行于对角线的直线带 = 恒定折扣。
              </div>
              <Scatter pair={selected} ink={ink} mute={mute} border={border} />
            </div>
          </div>

          {selected && (
            <>
              <StatPanels pair={selected} cardBg={cardBg} border={border} ink={ink} mute={mute} minComparable={numeric.minComparable} />
              <ArithErrors
                pair={selected}
                docIds={docIds}
                minComparable={numeric.minComparable}
                note={numeric.notes.sharedArithError}
                cardBg={cardBg}
                border={border}
                ink={ink}
                mute={mute}
                onChunk={(docId, chunkId) => nav(`/workspace/${wsId}/doc/${docId}?chunk=${chunkId}`)}
              />
            </>
          )}

          {numeric.mechanism && (
            <MechanismPanel
              mech={numeric.mechanism}
              cardBg={cardBg}
              border={border}
              ink={ink}
              mute={mute}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function PairChip({
  label,
  active,
  alarm,
  mute,
  border,
  onClick,
}: {
  label: string;
  active: boolean;
  alarm: boolean;
  mute: string;
  border: string;
  onClick: () => void;
}) {
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      style={{
        fontSize: 11,
        padding: "4px 10px",
        borderRadius: 999,
        cursor: "pointer",
        fontFamily: C.serif,
        background: active ? "rgba(79,88,168,0.15)" : "transparent",
        color: active ? "var(--accent, #4F58A8)" : alarm ? C.danger : mute,
        border: `1px solid ${active ? "var(--accent, #4F58A8)" : alarm ? C.danger : border}`,
        fontWeight: active || alarm ? 700 : 500,
      }}
    >
      {label}
    </span>
  );
}

/** N×N 雷同率热力表（复用矩阵热力配色）。null = 不出结论，显示「—」而非 0%。 */
function HeatTable({
  heat,
  n,
  nameOf,
  onCell,
}: {
  heat: (number | null)[][];
  n: number;
  nameOf: (i: number) => string;
  onCell: (a: number, b: number) => void;
}) {
  const { dark } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  return (
    <div style={{ display: "grid", gridTemplateColumns: `74px repeat(${n}, 1fr)`, gap: 5 }}>
      <div />
      {Array.from({ length: n }, (_, i) => (
        <div key={i} style={{ textAlign: "center" }}>
          <div
            style={{
              width: 22,
              height: 22,
              margin: "0 auto",
              borderRadius: 5,
              background: docColor(i),
              color: "#fff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 12,
              fontWeight: 700,
              fontFamily: C.serif,
            }}
          >
            {docTag(i)}
          </div>
        </div>
      ))}
      {Array.from({ length: n }, (_, r) => (
        <Fragment key={r}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 6, paddingRight: 6 }}>
            <div style={{ textAlign: "right", minWidth: 0 }}>
              <div style={{ fontSize: 11.5, fontWeight: 700, color: ink, fontFamily: C.serif }}>{docTag(r)}</div>
              <div
                style={{
                  fontSize: 9.5,
                  color: mute,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  maxWidth: 62,
                }}
              >
                {nameOf(r)}
              </div>
            </div>
          </div>
          {Array.from({ length: n }, (_, c) => {
            const diag = r === c;
            const v = heat[r]?.[c] ?? null;
            const fg = v != null && v >= 0.7 ? "#fff" : ink;
            return (
              <div
                key={c}
                role={diag || v == null ? undefined : "button"}
                tabIndex={diag || v == null ? undefined : 0}
                onClick={diag || v == null ? undefined : () => onCell(r, c)}
                onKeyDown={
                  diag || v == null
                    ? undefined
                    : (e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          onCell(r, c);
                        }
                      }
                }
                title={
                  diag
                    ? undefined
                    : v == null
                      ? "可比清单项不足，不出雷同率结论"
                      : `${docTag(r)}×${docTag(c)} 逐项单价雷同率 ${pct1(v)}`
                }
                style={{
                  aspectRatio: "1.5 / 1",
                  borderRadius: 7,
                  background: diag
                    ? dark
                      ? "rgba(255,255,255,0.04)"
                      : C.paper2
                    : v == null
                      ? dark
                        ? "rgba(255,255,255,0.02)"
                        : C.paper2
                      : severityColor(v, C.okSoft),
                  color: diag || v == null ? mute : fg,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  cursor: diag || v == null ? "default" : "pointer",
                  fontFamily: C.mono,
                  fontSize: 15,
                  fontWeight: 700,
                }}
              >
                {diag || v == null ? "—" : `${Math.round(v * 100)}`}
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}

/** 纯 SVG 散点图：对角线参考线 + 折扣带高亮 + 悬停显示 alignKey/名称。不引图表库。 */
function Scatter({
  pair,
  ink,
  mute,
  border,
}: {
  pair: NumericPairDto | null;
  ink: string;
  mute: string;
  border: string;
}) {
  const [hover, setHover] = useState<BoqScatterPoint | null>(null);
  const points = pair?.scatter ?? [];
  const band = discountBand(pair?.pattern?.kind, pair?.pattern?.a);
  const size = PLOT;
  const w = size + PLOT_PAD * 2;

  if (points.length === 0) {
    return (
      <div style={{ fontSize: 11.5, color: mute, padding: "48px 4px", textAlign: "center", lineHeight: 1.8 }}>
        该文档对没有可绘制的散点（无对齐清单项，或该项缺少全体投标人的中位价基准）。
      </div>
    );
  }

  const tick = (v: number) => (v / SCATTER_MAX) * size;
  return (
    <div style={{ marginTop: 12 }}>
      <svg
        width="100%"
        viewBox={`0 0 ${w} ${w}`}
        role="img"
        aria-label="单价归一化散点图"
        style={{ display: "block", maxWidth: w }}
      >
        <g transform={`translate(${PLOT_PAD},${PLOT_PAD})`}>
          <rect x={0} y={0} width={size} height={size} fill="none" stroke={border} />
          {[1, 2, 3].map((v) => (
            <Fragment key={v}>
              <line x1={tick(v)} y1={0} x2={tick(v)} y2={size} stroke={border} strokeDasharray="3 4" />
              <line x1={0} y1={size - tick(v)} x2={size} y2={size - tick(v)} stroke={border} strokeDasharray="3 4" />
              <text x={tick(v)} y={size + 14} fontSize={9.5} fill={mute} textAnchor="middle">
                {v}
              </text>
              <text x={-6} y={size - tick(v) + 3} fontSize={9.5} fill={mute} textAnchor="end">
                {v}
              </text>
            </Fragment>
          ))}
          {/* 折扣带（等比规律）：y = a·x 的高亮直线，画在对角线之下/之上 */}
          {band && (
            <line
              x1={0}
              y1={size}
              x2={size}
              y2={size - Math.min(SCATTER_MAX, SCATTER_MAX * band.slope) * (size / SCATTER_MAX)}
              stroke={C.hi2}
              strokeWidth={6}
              strokeOpacity={0.25}
              strokeLinecap="round"
            />
          )}
          {/* 对角线参考线：点落其上 = 双方单价完全一致 */}
          <line x1={0} y1={size} x2={size} y2={0} stroke={C.danger} strokeWidth={1.2} strokeDasharray="5 4" />
          {points.map((p, i) => {
            const { cx, cy } = toSvgXY(p, size);
            const on = hover?.alignKey === p.alignKey;
            return (
              <circle
                key={`${p.alignKey}-${i}`}
                cx={cx}
                cy={cy}
                r={on ? 4.5 : 2.6}
                fill={on ? C.brand : C.hi3}
                fillOpacity={on ? 0.95 : 0.55}
                onMouseEnter={() => setHover(p)}
                onMouseLeave={() => setHover(null)}
              >
                <title>{`${p.name ?? p.alignKey} · x=${p.x.toFixed(3)} · y=${p.y.toFixed(3)}`}</title>
              </circle>
            );
          })}
        </g>
        <text x={PLOT_PAD + size / 2} y={w - 4} fontSize={10} fill={mute} textAnchor="middle">
          {pair ? `${docTag(pair.a)} 单价 ÷ 中位价` : ""}
        </text>
        <text
          x={12}
          y={PLOT_PAD + size / 2}
          fontSize={10}
          fill={mute}
          textAnchor="middle"
          transform={`rotate(-90 12 ${PLOT_PAD + size / 2})`}
        >
          {pair ? `${docTag(pair.b)} 单价 ÷ 中位价` : ""}
        </text>
      </svg>
      <div style={{ fontSize: 10.5, color: hover ? ink : mute, marginTop: 6, minHeight: 17, lineHeight: 1.6 }}>
        {hover
          ? `${hover.name ?? "（未命名清单项）"} · ${hover.alignKey} · x=${hover.x.toFixed(3)} · y=${hover.y.toFixed(3)}`
          : `共 ${points.length} 个可比清单项（超过 2000 项时已等距下采样）。红色虚线 = 单价完全一致；橙色带 = 恒定折扣线。`}
      </div>
    </div>
  );
}

/** 规律性 + 相关性面板：§1.5 强制把比值 CV 与散点形态、强证据条件写在同屏。 */
function StatPanels({
  pair,
  cardBg,
  border,
  ink,
  mute,
  minComparable,
}: {
  pair: NumericPairDto;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  minComparable: number;
}) {
  const pat = pair.pattern ?? null;
  const cor = pair.correlation ?? null;
  const strong = isStrongCorrelation(cor);
  const box = {
    background: cardBg,
    border: `1px solid ${border}`,
    borderRadius: 14,
    padding: 20,
  } as const;
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
      <div style={box}>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>规律性差异</div>
        {pat ? (
          <>
            <div style={{ marginTop: 10 }}>
              <Pill fg={C.hi2} bg={C.hi2Soft} size={10.5} weight={700}>
                线索 · {patternLabel(pat.kind)}
              </Pill>
            </div>
            <div style={{ fontSize: 12, color: ink, marginTop: 10, lineHeight: 1.9, fontFamily: C.mono }}>
              y = {pat.a.toFixed(4)}·x {pat.b >= 0 ? "+" : "−"} {Math.abs(pat.b).toFixed(2)} 元
              <br />
              R² = {pat.r2.toFixed(4)} · n = {pat.n}
              {pat.ratioCv != null && <> · 比值 CV = {(pat.ratioCv * 100).toFixed(3)}%</>}
              <br />
              差值极差 = {pat.diffRange.toFixed(2)} 元 · 辅证{pat.corroborated ? "成立" : "不成立"}
            </div>
            <div style={{ fontSize: 11, color: mute, marginTop: 10, lineHeight: 1.75 }}>{pat.note}</div>
          </>
        ) : (
          <div style={{ fontSize: 11.5, color: mute, marginTop: 10, lineHeight: 1.8 }}>
            未检出规律性差异：剔除双方单价相同的条目后不足 {minComparable} 项，或最小二乘拟合优度未达 R²≥0.999。
            无结论不代表无问题，也不代表存在问题。
          </div>
        )}
      </div>
      <div style={box}>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>相关性</div>
        {cor ? (
          <>
            <div style={{ marginTop: 10 }}>
              <Pill
                fg={strong ? C.danger : C.ink3}
                bg={strong ? C.dangerSoft : "rgba(107,107,118,0.12)"}
                size={10.5}
                weight={700}
              >
                {strong ? "满足强证据双条件" : "不构成强证据"}
              </Pill>
            </div>
            <div style={{ fontSize: 12, color: ink, marginTop: 10, lineHeight: 1.9, fontFamily: C.mono }}>
              Pearson r = {cor.pearson.toFixed(4)}
              <br />
              Spearman ρ = {cor.spearman.toFixed(4)} · n = {cor.n}
              <br />
              比值 CV = {cor.ratioCv != null ? `${(cor.ratioCv * 100).toFixed(3)}%` : "—（存在零单价，无法计算）"}
            </div>
            <div style={{ fontSize: 11, color: mute, marginTop: 10, lineHeight: 1.75 }}>{cor.note}</div>
          </>
        ) : (
          <div style={{ fontSize: 11.5, color: mute, marginTop: 10, lineHeight: 1.8 }}>
            不出相关性结论：可比清单项不足 {minComparable} 项，或任一侧单价方差为 0。
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 基准价敏感性（W5-5 机制感知筛查）——【描述性块，不参与围标分级】。
 * 展示：评标办法公式全文（人工录入回显）、逐份投标总价与来源打标、候选组及其【构造依据】、
 * 中标人翻转比例、断崖式报价标记；后端下发的强制措辞原样列出，不得省略、不得改写。
 */
function MechanismPanel({
  mech,
  cardBg,
  border,
  ink,
  mute,
}: {
  mech: MechanismDto;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
}) {
  const th = {
    fontSize: 10.5,
    fontWeight: 700,
    color: mute,
    textAlign: "left" as const,
    padding: "6px 8px",
    borderBottom: `1px solid ${border}`,
    whiteSpace: "nowrap" as const,
  };
  const td = {
    fontSize: 11.5,
    color: ink,
    padding: "7px 8px",
    borderBottom: `1px solid ${border}`,
  };
  const money = (v: number) => v.toLocaleString("zh-CN", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  const b = mech.benchmark ?? null;
  return (
    <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 20 }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, flexWrap: "wrap" }}>
        <span style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>基准价敏感性</span>
        <Pill fg={C.ink3} bg="rgba(107,107,118,0.12)" size={10.5} weight={700}>
          反事实解释性分析 · 不参与围标分级
        </Pill>
      </div>
      {/* §1.5 强制措辞：原样展示后端下发的每一条，任何呈现层都不得省略 */}
      <div style={{ fontSize: 11, color: mute, marginTop: 8, lineHeight: 1.8 }}>
        {mech.notes.map((n, i) => (
          <div key={i}>{n}</div>
        ))}
      </div>
      <div style={{ fontSize: 11.5, color: ink, marginTop: 12, lineHeight: 1.8 }}>
        <b>评标办法（人工录入）：</b>
        {mech.formula}
      </div>

      {!mech.applicable ? (
        <div style={{ fontSize: 11.5, color: mute, marginTop: 12, lineHeight: 1.8 }}>
          {mech.notApplicableReason ?? "本节不适用，未作任何反事实计算。"}
        </div>
      ) : (
        <>
          {b && (
            <div style={{ fontSize: 12, color: ink, marginTop: 12, lineHeight: 1.9, fontFamily: C.mono }}>
              基准价（系数 {b.coeffMid.toFixed(4)}）= {money(b.benchmarkMid)} 元 · 去 {b.trimHighest} 高 / 去{" "}
              {b.trimLowest} 低 · 系数区间 [{b.coeffMin.toFixed(4)}, {b.coeffMax.toFixed(4)}] 取 {b.gridPoints} 个均匀格点
              <br />
              该系数下中标人：{docTag(b.winnerMid)}
            </div>
          )}
          {mech.lowest && (
            <div style={{ fontSize: 12, color: ink, marginTop: 12, lineHeight: 1.9, fontFamily: C.mono }}>
              最低投标总价：{docTag(mech.lowest.winner)} · {money(mech.lowest.lowest)} 元 · 次低{" "}
              {money(mech.lowest.secondLowest)} 元 · 间距 {money(mech.lowest.gap)} 元（中位间距{" "}
              {money(mech.lowest.medianGap)} 元）
              <br />
              {mech.lowest.isolated ? "最低价与其余报价断崖，建议核对成本构成" : "未见断崖式孤立"}
            </div>
          )}

          <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 18 }}>投标总价与来源</div>
          <div style={{ overflowX: "auto", marginTop: 8 }}>
            <table style={{ width: "100%", borderCollapse: "collapse", minWidth: 480 }}>
              <thead>
                <tr>
                  <th style={th}>编号</th>
                  <th style={th}>投标总价（元）</th>
                  <th style={th}>来源</th>
                </tr>
              </thead>
              <tbody>
                {mech.prices.map((p) => (
                  <tr key={p.docIndex}>
                    <td style={td}>{docTag(p.docIndex)}</td>
                    <td style={{ ...td, fontFamily: C.mono }}>{money(p.total)}</td>
                    <td style={td}>{priceSourceLabel(p.source, p.sourceLabel)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {b && (
            <>
              <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 18 }}>
                候选组反事实结果（{b.groups.length} 组）
              </div>
              {b.groups.length === 0 ? (
                <div style={{ fontSize: 11.5, color: mute, marginTop: 8, lineHeight: 1.8 }}>
                  未构造出候选组：参评文档间未出现可作依据的既有文档证据（文本相似峰值 / 逐项单价雷同率 / 元数据同源），
                  故不作剔除重算。
                </div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 8 }}>
                  {b.groups.map((g) => (
                    <div
                      key={g.docs.join("-")}
                      style={{ border: `1px solid ${border}`, borderRadius: 10, padding: "10px 14px" }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                        <span style={{ fontSize: 12.5, fontWeight: 700, color: ink, fontFamily: C.serif }}>
                          {g.docs.map((d) => docTag(d)).join(" × ")}
                        </span>
                        <Pill fg={C.hi2} bg={C.hi2Soft} size={10.5} weight={700}>
                          {flipStatement(g.flipProb)}
                        </Pill>
                        {g.supportBidDocs.length > 0 && (
                          <Pill fg={C.hi4} bg={C.hi4Soft} size={10.5} weight={700}>
                            含断崖式报价 {g.supportBidDocs.map((d) => docTag(d)).join("、")}
                          </Pill>
                        )}
                      </div>
                      <div style={{ fontSize: 11.5, color: ink, marginTop: 6, lineHeight: 1.85, fontFamily: C.mono }}>
                        基准价偏移 {shiftLabel(g.benchmarkShiftPct)} · 同规模子集分位 {pct1(g.shiftPercentile)}（共{" "}
                        {g.subsetsCompared} 个子集）· 中标人 {docTag(g.winnerFull)} → {docTag(g.winnerExcluded)}
                      </div>
                      <div style={{ fontSize: 11, color: mute, marginTop: 6, lineHeight: 1.75 }}>
                        组的构造依据：{g.basis.map((x) => x.detail).join("；") || "—"}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {mech.supportBids.length > 0 && (
            <>
              <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 18 }}>
                断崖式报价（support-bid 形态）
              </div>
              <div style={{ overflowX: "auto", marginTop: 8 }}>
                <table style={{ width: "100%", borderCollapse: "collapse", minWidth: 560 }}>
                  <thead>
                    <tr>
                      <th style={th}>编号</th>
                      <th style={th}>投标总价（元）</th>
                      <th style={th}>位置</th>
                      <th style={th}>与次邻间距（元）</th>
                      <th style={th}>偏离中位数</th>
                    </tr>
                  </thead>
                  <tbody>
                    {mech.supportBids.map((s) => (
                      <tr key={s.docIndex}>
                        <td style={td}>{docTag(s.docIndex)}</td>
                        <td style={{ ...td, fontFamily: C.mono }}>{money(s.total)}</td>
                        <td style={td}>{mechanismPositionLabel(s.position)}</td>
                        <td style={{ ...td, fontFamily: C.mono }}>
                          {money(s.gap)}（中位间距 {money(s.medianGap)}）
                        </td>
                        <td style={{ ...td, fontFamily: C.mono }}>{shiftLabel(s.deviationPct)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

/** 相同项计数 + 共享算术错误明细表（行点击跳 DocPreview 对应 chunk 原文）。 */
function ArithErrors({
  pair,
  docIds,
  minComparable,
  note,
  cardBg,
  border,
  ink,
  mute,
  onChunk,
}: {
  pair: NumericPairDto;
  docIds: string[];
  minComparable: number;
  note: string;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  onChunk: (docId: string, chunkId: string) => void;
}) {
  const th = {
    fontSize: 10.5,
    fontWeight: 700,
    color: mute,
    textAlign: "left" as const,
    padding: "6px 8px",
    borderBottom: `1px solid ${border}`,
    whiteSpace: "nowrap" as const,
  };
  const td = {
    fontSize: 11.5,
    color: ink,
    padding: "7px 8px",
    borderBottom: `1px solid ${border}`,
  };
  return (
    <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 20 }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, flexWrap: "wrap" }}>
        <span style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>
          {docTag(pair.a)} × {docTag(pair.b)} 逐项比对明细
        </span>
        <span style={{ fontSize: 11, color: mute }}>
          可比 {pair.comparable} 项 · 单价相同 {pair.identical} 项 ·{" "}
          {pair.identicalRate != null
            ? `雷同率 ${pct1(pair.identicalRate)}`
            : reasonLabel(pair.reason, minComparable)}
        </span>
        {pair.alarm && (
          <Pill fg={C.danger} bg={C.dangerSoft} size={10.5} weight={700}>
            达告警线 · 需重点核查
          </Pill>
        )}
      </div>

      <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 18 }}>
        共享算术错误（{pair.sharedArithErrors.length} 条）
      </div>
      <div style={{ fontSize: 11, color: mute, marginTop: 6, lineHeight: 1.75 }}>{note}</div>
      {pair.sharedArithErrors.length === 0 ? (
        <div style={{ fontSize: 11.5, color: mute, marginTop: 12, lineHeight: 1.8 }}>
          该文档对未发现共享算术错误（双方同一清单项的工程量、单价与算错的合价三者全等）。
          检测已排除可由常见舍入规则解释的差值；未命中不构成清白证明。
        </div>
      ) : (
        <div style={{ overflowX: "auto", marginTop: 12 }}>
          <table style={{ width: "100%", borderCollapse: "collapse", minWidth: 720 }}>
            <thead>
              <tr>
                <th style={th}>清单项</th>
                <th style={th}>工程量</th>
                <th style={th}>综合单价</th>
                <th style={th}>报出合价</th>
                <th style={th}>应为</th>
                <th style={th}>原文</th>
              </tr>
            </thead>
            <tbody>
              {pair.sharedArithErrors.map((e, i) => (
                <tr key={`${e.alignKey}-${i}`}>
                  <td style={td}>
                    <div style={{ fontWeight: 600 }}>{e.name ?? "（未命名清单项）"}</div>
                    <div style={{ fontSize: 10, color: mute, fontFamily: C.mono }}>{e.alignKey}</div>
                  </td>
                  <td style={{ ...td, fontFamily: C.mono }}>{e.qty}</td>
                  <td style={{ ...td, fontFamily: C.mono }}>{e.unitPrice.toFixed(2)}</td>
                  <td style={{ ...td, fontFamily: C.mono, color: C.danger, fontWeight: 700 }}>
                    {e.total.toFixed(2)}
                  </td>
                  <td style={{ ...td, fontFamily: C.mono }}>{e.expectedTotal.toFixed(2)}</td>
                  <td style={td}>
                    <div style={{ display: "flex", gap: 6 }}>
                      {[pair.a, pair.b].map((di, k) => {
                        const chunkId = e.chunkIds[k];
                        const docId = docIds[di];
                        if (!chunkId || !docId) return null;
                        return (
                          <Button
                            key={di}
                            kind="ghost"
                            size="sm"
                            onClick={() => onChunk(docId, chunkId)}
                          >
                            {docTag(di)} 原文
                          </Button>
                        );
                      })}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
