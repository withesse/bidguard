// 屏 · 对齐区段（W4-5，M5b）—— 新增证据层，【只读】，与聚类经 chunk_id 互链。
// 上：文档对选择（十天干 docTag 复用）；中：区段卡片列表（章节/页码/双向覆盖条/锚点/逐字）；
// 展开：双栏按 chunk 顺序高亮 —— 深红=逐字铁证(verbatim)、橙=锚点雷同、黄=gap 细化差异，
// 招标豁免块（tenderCoverage≥0.8）显示「引用招标文件」徽标；块级「查看所属条款」跳 ClusterDetail。
// 复核三态仍只挂 cluster，本屏不引入第二套状态。
import { Fragment, useEffect, useMemo, useState, type ReactNode } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { DiffOp } from "../engine";
import type { AlignedSegmentDto, SegmentChunkDto, VerbatimIntervalDto } from "../api/types";
import { useAlignedSegments, useCompareSummary, useSegmentDetail } from "../queries/data";
import { docColor, docTag } from "../utils/docTag";

// —— 三级视觉语义（§1.5 图例）——
const VERBATIM_BG = "rgba(181,69,69,0.22)"; // 深红：逐字铁证
const VERBATIM_FG = "#B54545";
const ANCHOR_BORDER = "#E0A064"; // 橙：锚点雷同
const ANCHOR_BG = "rgba(224,160,100,0.12)";
const GAP_BORDER = "#E3C28A"; // 黄：gap 细化差异
const GAP_DIFF_BG = "rgba(227,194,138,0.5)";

/** 首版渲染上限：长区段（数百 chunk）先渲染前 N 块，超出「展开更多」抬高上限。 */
const RENDER_CAP = 200;

type Pair = { a: string; b: string };

function samePair(x: Pair, y: Pair): boolean {
  return (x.a === y.a && x.b === y.b) || (x.a === y.b && x.b === y.a);
}

/** section_path 是 JSON 数组字符串（与成员卡同源）；坏值只影响面包屑。 */
function pathText(raw: string | null): string {
  if (!raw) return "";
  try {
    const arr = JSON.parse(raw) as string[];
    return Array.isArray(arr) ? arr.join(" › ") : String(raw);
  } catch {
    return raw;
  }
}

function pageRange(s: number | null, e: number | null): string {
  if (s == null) return "";
  return e != null && e !== s ? `第 ${s}–${e} 页` : `第 ${s} 页`;
}

export function PairSegments() {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();
  const { dark } = useTheme();
  const [sp, setSp] = useSearchParams();

  const { data: summary } = useCompareSummary(jobId);
  const { data: segs, isLoading } = useAlignedSegments(jobId);

  const docIds: string[] = summary?.matrix?.documentIds ?? [];
  const idxOf = (id: string) => docIds.indexOf(id);
  const tagOf = (id: string) => {
    const i = idxOf(id);
    return i >= 0 ? docTag(i) : "?";
  };
  const nameOf = (id: string) => {
    const d = summary?.documents.find((x) => x.id === id);
    return d ? d.fileName.replace(/\.[^.]+$/, "") : id;
  };

  const all: AlignedSegmentDto[] = segs ?? [];

  // 存在区段的文档对（去重、方向归一），供文档对选择器。计数用于 chip 提示。
  const presentPairs = useMemo(() => {
    const m = new Map<string, { pair: Pair; count: number }>();
    for (const s of all) {
      const [a, b] = idxOf(s.docAId) <= idxOf(s.docBId) ? [s.docAId, s.docBId] : [s.docBId, s.docAId];
      const key = `${a}|${b}`;
      const cur = m.get(key);
      if (cur) cur.count += 1;
      else m.set(key, { pair: { a, b }, count: 1 });
    }
    return [...m.values()].sort(
      (x, y) => idxOf(x.pair.a) - idxOf(y.pair.a) || idxOf(x.pair.b) - idxOf(y.pair.b),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [all, docIds.join(",")]);

  // 选中文档对（URL a/b 优先，供 Matrix 单元格深链）；无则「全部」。
  const urlA = sp.get("a") ?? undefined;
  const urlB = sp.get("b") ?? undefined;
  const selectedPair: Pair | null = urlA && urlB ? { a: urlA, b: urlB } : null;
  const setPair = (p: Pair | null) => {
    const next = new URLSearchParams(sp);
    if (p) {
      next.set("a", p.a);
      next.set("b", p.b);
    } else {
      next.delete("a");
      next.delete("b");
    }
    next.delete("seg"); // 换对后清掉旧的展开选择
    setSp(next, { replace: true });
  };

  const shown = useMemo(
    () => (selectedPair ? all.filter((s) => samePair({ a: s.docAId, b: s.docBId }, selectedPair)) : all),
    [all, selectedPair],
  );

  // 选中展开的区段（URL seg 深链，供 ClusterDetail 反向跳转定位）。
  const selectedSeg = sp.get("seg") ?? undefined;
  const setSeg = (id: string | undefined) => {
    const next = new URLSearchParams(sp);
    if (id) next.set("seg", id);
    else next.delete("seg");
    setSp(next, { replace: true });
  };

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: bg, overflow: "hidden" }}>
      <Topbar
        title="对齐区段"
        sub={`证据成型 · ${all.length} 段对齐区段`}
        actions={
          <Button kind="secondary" size="sm" onClick={() => nav(`/workspace/${wsId}/job/${jobId}`)}>
            返回报告
          </Button>
        }
      />

      {/* 文档对选择 + 图例 */}
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
        <PairChip label="全部" active={!selectedPair} mute={mute} border={border} onClick={() => setPair(null)} />
        {presentPairs.map(({ pair, count }) => (
          <PairChip
            key={`${pair.a}|${pair.b}`}
            label={`${tagOf(pair.a)}×${tagOf(pair.b)} · ${count}`}
            active={!!selectedPair && samePair(pair, selectedPair)}
            mute={mute}
            border={border}
            onClick={() => setPair(pair)}
          />
        ))}
        <span style={{ flex: 1 }} />
        <Legend mute={mute} />
      </div>

      {/* 区段列表 */}
      <div style={{ flex: 1, overflowY: "auto", padding: "16px 24px 40px" }}>
        <div style={{ maxWidth: 1280, margin: "0 auto", display: "flex", flexDirection: "column", gap: 12 }}>
          {isLoading ? (
            <div style={{ fontSize: 13, color: mute, padding: "40px 4px", textAlign: "center" }}>正在加载对齐区段…</div>
          ) : shown.length === 0 ? (
            <div style={{ fontSize: 12.5, color: mute, padding: "40px 4px", textAlign: "center", lineHeight: 1.7 }}>
              {all.length === 0
                ? "该任务无对齐区段数据（对齐成型于 M5 引入，更早的历史任务不含此层）。"
                : "当前文档对下没有对齐区段。"}
            </div>
          ) : (
            shown.map((s) => (
              <SegmentCard
                key={s.id}
                s={s}
                tagA={tagOf(s.docAId)}
                tagB={tagOf(s.docBId)}
                nameA={nameOf(s.docAId)}
                nameB={nameOf(s.docBId)}
                colorA={docColor(idxOf(s.docAId))}
                colorB={docColor(idxOf(s.docBId))}
                open={selectedSeg === s.id}
                onToggle={() => setSeg(selectedSeg === s.id ? undefined : s.id)}
                onCluster={(cid) => nav(`/workspace/${wsId}/job/${jobId}/cluster/${cid}`)}
                cardBg={cardBg}
                border={border}
                ink={ink}
                mute={mute}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function PairChip({
  label,
  active,
  mute,
  border,
  onClick,
}: {
  label: string;
  active: boolean;
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
        color: active ? "var(--accent, #4F58A8)" : mute,
        border: `1px solid ${active ? "var(--accent, #4F58A8)" : border}`,
        fontWeight: active ? 700 : 500,
      }}
    >
      {label}
    </span>
  );
}

function Legend({ mute }: { mute: string }) {
  const item = (color: string, label: string, filled: boolean) => (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5 }}>
      <span
        style={{
          width: 11,
          height: 11,
          borderRadius: 3,
          background: filled ? color : "transparent",
          border: filled ? "none" : `2px solid ${color}`,
        }}
      />
      <span style={{ fontSize: 10.5, color: mute }}>{label}</span>
    </span>
  );
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 14, flexWrap: "wrap" }}>
      {item(VERBATIM_BG, "逐字铁证", true)}
      {item(ANCHOR_BORDER, "锚点雷同", false)}
      {item(GAP_DIFF_BG, "gap 细化差异", true)}
    </span>
  );
}

function SegmentCard({
  s,
  tagA,
  tagB,
  nameA,
  nameB,
  colorA,
  colorB,
  open,
  onToggle,
  onCluster,
  cardBg,
  border,
  ink,
  mute,
}: {
  s: AlignedSegmentDto;
  tagA: string;
  tagB: string;
  nameA: string;
  nameB: string;
  colorA: string;
  colorB: string;
  open: boolean;
  onToggle: () => void;
  onCluster: (cid: string) => void;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
}) {
  const covPct = Math.round(Math.max(s.aCoverage, s.bCoverage) * 100);
  return (
    <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, overflow: "hidden" }}>
      {/* 卡头（点击展开/收起） */}
      <div
        role="button"
        tabIndex={0}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
        style={{ padding: "13px 16px", cursor: "pointer", display: "flex", flexDirection: "column", gap: 8 }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <DocBadge tag={tagA} color={colorA} />
          <span style={{ fontSize: 12.5, fontWeight: 600, color: ink, maxWidth: 260, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {pathText(s.aSectionPath) || nameA}
          </span>
          <span style={{ color: mute, fontSize: 13 }}>↔</span>
          <DocBadge tag={tagB} color={colorB} />
          <span style={{ fontSize: 12.5, fontWeight: 600, color: ink, maxWidth: 260, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {pathText(s.bSectionPath) || nameB}
          </span>
          <span style={{ flex: 1 }} />
          <Pill fg={covPct >= 80 ? VERBATIM_FG : "#B06A3B"} bg={covPct >= 80 ? "rgba(181,69,69,0.12)" : "rgba(176,106,59,0.12)"} size={10.5} weight={700}>
            覆盖 {covPct}%
          </Pill>
          <Pill fg={mute} bg="rgba(128,128,128,0.12)" size={10.5}>
            锚点 {s.anchorCount}
          </Pill>
          {s.verbatimChars > 0 && (
            <Pill fg={VERBATIM_FG} bg="rgba(181,69,69,0.12)" size={10.5} weight={700}>
              逐字 {s.verbatimChars} 字
            </Pill>
          )}
          <span style={{ fontSize: 11, color: mute, marginLeft: 2 }}>{open ? "收起 ▲" : "展开 ▼"}</span>
        </div>
        {/* 双向覆盖条 + 页码 */}
        <div style={{ display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap" }}>
          <CoverageBar tag={tagA} color={colorA} pct={Math.round(s.aCoverage * 100)} mute={mute} border={border} />
          <CoverageBar tag={tagB} color={colorB} pct={Math.round(s.bCoverage * 100)} mute={mute} border={border} />
          {(s.aPageStart != null || s.bPageStart != null) && (
            <span style={{ fontSize: 10.5, color: mute }}>
              {[pageRange(s.aPageStart, s.aPageEnd), pageRange(s.bPageStart, s.bPageEnd)].filter(Boolean).join(" · ")}
            </span>
          )}
        </div>
      </div>

      {open && (
        <div style={{ borderTop: `1px solid ${border}`, padding: 16 }}>
          <SegmentDetailPane
            segmentId={s.id}
            tagA={tagA}
            tagB={tagB}
            onCluster={onCluster}
            ink={ink}
            mute={mute}
          />
        </div>
      )}
    </div>
  );
}

function DocBadge({ tag, color }: { tag: string; color: string }) {
  return (
    <span
      style={{
        width: 20,
        height: 20,
        borderRadius: 5,
        background: color,
        color: "#fff",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: 11.5,
        fontWeight: 700,
        fontFamily: C.serif,
        flexShrink: 0,
      }}
    >
      {tag}
    </span>
  );
}

function CoverageBar({
  tag,
  color,
  pct,
  mute,
  border,
}: {
  tag: string;
  color: string;
  pct: number;
  mute: string;
  border: string;
}) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
      <span style={{ fontSize: 10.5, color: mute, fontFamily: C.serif, fontWeight: 700 }}>{tag}</span>
      <span style={{ width: 88, height: 6, borderRadius: 3, background: border, overflow: "hidden" }}>
        <span style={{ display: "block", height: "100%", width: `${Math.min(100, pct)}%`, background: color }} />
      </span>
      <span style={{ fontSize: 10.5, color: mute, fontFamily: C.mono, minWidth: 30 }}>{pct}%</span>
    </span>
  );
}

// —— 双栏区段详情 ——

type Block =
  | { kind: "chunk"; chunk: SegmentChunkDto; isAnchor: boolean; verbatim: Array<[number, number]> }
  | { kind: "gap"; ops: DiffOp[]; span: SegmentChunkDto[] };

/** 合并/裁剪逐字高亮区间到 [0,len]。 */
function mergeRanges(ranges: Array<[number, number]>, len: number): Array<[number, number]> {
  const clean = ranges
    .map(([a, b]) => [Math.max(0, Math.min(a, b)), Math.min(len, Math.max(a, b))] as [number, number])
    .filter(([a, b]) => b > a)
    .sort((x, y) => x[0] - y[0]);
  const out: Array<[number, number]> = [];
  for (const r of clean) {
    const last = out[out.length - 1];
    if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else out.push([r[0], r[1]]);
  }
  return out;
}

/** 逐字铁证：chunk 原文按 char 切片，命中区间深红底。offset 按原文 char 计（后端保证）。 */
function VerbatimText({ text, ranges }: { text: string; ranges: Array<[number, number]> }) {
  const chars = useMemo(() => [...text], [text]);
  const merged = useMemo(() => mergeRanges(ranges, chars.length), [ranges, chars.length]);
  if (merged.length === 0) return <>{text}</>;
  const out: ReactNode[] = [];
  let pos = 0;
  merged.forEach(([lo, hi], i) => {
    if (lo > pos) out.push(<Fragment key={`p${i}`}>{chars.slice(pos, lo).join("")}</Fragment>);
    out.push(
      <span key={`v${i}`} style={{ background: VERBATIM_BG, borderRadius: 2 }}>
        {chars.slice(lo, hi).join("")}
      </span>,
    );
    pos = hi;
  });
  if (pos < chars.length) out.push(<Fragment key="tail">{chars.slice(pos).join("")}</Fragment>);
  return <>{out}</>;
}

/** gap 细化：side=a 显示 eq+del、side=b 显示 eq+ins；差异（del/ins）黄底（复用 diff 语义）。 */
function GapDiffText({ ops, side }: { ops: DiffOp[]; side: "a" | "b" }) {
  return (
    <>
      {ops.map((op, i) => {
        if (op.op === "eq") return <Fragment key={i}>{op.text}</Fragment>;
        const mine = (side === "a" && op.op === "del") || (side === "b" && op.op === "ins");
        if (!mine) return null;
        return (
          <span key={i} style={{ background: GAP_DIFF_BG, borderRadius: 2 }}>
            {op.text}
          </span>
        );
      })}
    </>
  );
}

/** 某一侧的逐字高亮区间（按 chunkId 归组）。方向无关：verbatim 行两种存储朝向都映射。 */
function verbatimRangesForSide(
  verbatims: VerbatimIntervalDto[],
  chunks: SegmentChunkDto[],
  sideDocId: string,
): Map<string, Array<[number, number]>> {
  const pos = new Map(chunks.map((c, i) => [c.chunkId, i]));
  const lenOf = new Map(chunks.map((c) => [c.chunkId, [...c.text].length]));
  const map = new Map<string, Array<[number, number]>>();
  const push = (id: string, r: [number, number]) => {
    const arr = map.get(id) ?? [];
    arr.push(r);
    map.set(id, arr);
  };
  for (const v of verbatims) {
    let sc: string, so: number, ec: string, eo: number;
    if (v.docAId === sideDocId) {
      [sc, so, ec, eo] = [v.aStartChunkId, v.aStartOffset, v.aEndChunkId, v.aEndOffset];
    } else if (v.docBId === sideDocId) {
      [sc, so, ec, eo] = [v.bStartChunkId, v.bStartOffset, v.bEndChunkId, v.bEndOffset];
    } else {
      continue;
    }
    const ps = pos.get(sc);
    const pe = pos.get(ec);
    if (ps == null || pe == null) continue; // 端点不在已加载跨度内 → 保守跳过
    const [lo, hi] = ps <= pe ? [ps, pe] : [pe, ps];
    for (let i = lo; i <= hi; i++) {
      const ch = chunks[i];
      const l = lenOf.get(ch.chunkId) ?? 0;
      const rLo = ch.chunkId === sc ? so : 0;
      const rHi = ch.chunkId === ec ? eo : l;
      push(ch.chunkId, [rLo, rHi]);
    }
  }
  return map;
}

/** 走 chunk 顺序建块：gap-first 块吞掉其后至下一锚点/下一 gap 的非锚点块，其余按 chunk 渲染。 */
function buildBlocks(
  chunks: SegmentChunkDto[],
  anchorIds: Set<string>,
  gapFirst: Map<string, DiffOp[]>,
  verbatim: Map<string, Array<[number, number]>>,
): Block[] {
  const blocks: Block[] = [];
  let i = 0;
  while (i < chunks.length) {
    const ch = chunks[i];
    const ops = gapFirst.get(ch.chunkId);
    if (ops) {
      let j = i + 1;
      while (j < chunks.length && !anchorIds.has(chunks[j].chunkId) && !gapFirst.has(chunks[j].chunkId)) j++;
      blocks.push({ kind: "gap", ops, span: chunks.slice(i, j) });
      i = j;
    } else {
      blocks.push({ kind: "chunk", chunk: ch, isAnchor: anchorIds.has(ch.chunkId), verbatim: verbatim.get(ch.chunkId) ?? [] });
      i += 1;
    }
  }
  return blocks;
}

function SegmentDetailPane({
  segmentId,
  tagA,
  tagB,
  onCluster,
  ink,
  mute,
}: {
  segmentId: string;
  tagA: string;
  tagB: string;
  onCluster: (cid: string) => void;
  ink: string;
  mute: string;
}) {
  const { data, isLoading } = useSegmentDetail(segmentId);
  const [capA, setCapA] = useState(RENDER_CAP);
  const [capB, setCapB] = useState(RENDER_CAP);
  useEffect(() => {
    setCapA(RENDER_CAP);
    setCapB(RENDER_CAP);
  }, [segmentId]);

  const built = useMemo(() => {
    if (!data) return null;
    const seg = data.segment;
    // 锚点存储在区段自身朝向（a=segment.docAId 侧）：直接取 a/b chunk id。
    const anchorA = new Set(data.anchors.map((a) => a.aChunkId));
    const anchorB = new Set(data.anchors.map((a) => a.bChunkId));
    // gap 首块 → DiffOps（diff a_chunk_id/b_chunk_id 亦为区段朝向）。
    const gapA = new Map<string, DiffOp[]>();
    const gapB = new Map<string, DiffOp[]>();
    for (const d of data.diffs) {
      let ops: DiffOp[];
      try {
        ops = JSON.parse(d.diffJson) as DiffOp[];
      } catch {
        continue; // 坏 diff 行不阻塞
      }
      if (d.aChunkId) gapA.set(d.aChunkId, ops);
      if (d.bChunkId) gapB.set(d.bChunkId, ops);
    }
    const vA = verbatimRangesForSide(data.verbatims, data.aChunks, seg.docAId);
    const vB = verbatimRangesForSide(data.verbatims, data.bChunks, seg.docBId);
    return {
      blocksA: buildBlocks(data.aChunks, anchorA, gapA, vA),
      blocksB: buildBlocks(data.bChunks, anchorB, gapB, vB),
      totalA: data.aChunks.length,
      totalB: data.bChunks.length,
      clusterIds: data.clusterIds,
    };
  }, [data]);

  if (isLoading || !data || !built) {
    return <div style={{ fontSize: 12, color: mute, padding: "12px 4px" }}>正在加载区段详情…</div>;
  }
  if (built.totalA === 0 && built.totalB === 0) {
    return <div style={{ fontSize: 12, color: mute, padding: "12px 4px" }}>区段跨度内的分块已不可用（文档可能已删除）。</div>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* 反向互链：查看所属条款 */}
      {built.clusterIds.length > 0 && (
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <span style={{ fontSize: 11, color: mute }}>关联条款</span>
          {built.clusterIds.map((cid) => (
            <span
              key={cid}
              role="button"
              tabIndex={0}
              onClick={() => onCluster(cid)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onCluster(cid);
                }
              }}
              style={{
                fontSize: 10.5,
                padding: "3px 9px",
                borderRadius: 999,
                cursor: "pointer",
                color: "var(--accent, #4F58A8)",
                border: `1px solid var(--accent, #4F58A8)`,
                fontWeight: 600,
              }}
            >
              查看所属条款
            </span>
          ))}
        </div>
      )}

      {/* 双栏 */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
        <SideColumn
          title={tagA}
          blocks={built.blocksA}
          side="a"
          cap={capA}
          total={built.totalA}
          onMore={() => setCapA((v) => v + RENDER_CAP)}
          ink={ink}
          mute={mute}
        />
        <SideColumn
          title={tagB}
          blocks={built.blocksB}
          side="b"
          cap={capB}
          total={built.totalB}
          onMore={() => setCapB((v) => v + RENDER_CAP)}
          ink={ink}
          mute={mute}
        />
      </div>
    </div>
  );
}

function SideColumn({
  title,
  blocks,
  side,
  cap,
  total,
  onMore,
  ink,
  mute,
}: {
  title: string;
  blocks: Block[];
  side: "a" | "b";
  cap: number;
  total: number;
  onMore: () => void;
  ink: string;
  mute: string;
}) {
  // 按已渲染 chunk 数截断块序列（首版性能护栏）。
  let used = 0;
  const visible: Block[] = [];
  for (const b of blocks) {
    if (used >= cap) break;
    visible.push(b);
    used += b.kind === "gap" ? b.span.length : 1;
  }
  const truncated = used < total;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ fontSize: 11, fontWeight: 700, color: mute, fontFamily: C.serif }}>{title} 侧</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        {visible.map((b, i) =>
          b.kind === "gap" ? (
            <div
              key={i}
              style={{
                borderLeft: `3px solid ${GAP_BORDER}`,
                paddingLeft: 10,
                paddingTop: 2,
                paddingBottom: 2,
              }}
            >
              <div style={{ fontSize: 9.5, color: mute, marginBottom: 2 }}>细化差异</div>
              <div style={{ fontSize: 12.5, lineHeight: 1.85, color: ink, userSelect: "text" }}>
                <GapDiffText ops={b.ops} side={side} />
              </div>
            </div>
          ) : (
            <div
              key={i}
              style={{
                borderLeft: `3px solid ${b.isAnchor ? ANCHOR_BORDER : "transparent"}`,
                background: b.isAnchor ? ANCHOR_BG : "transparent",
                paddingLeft: 10,
                paddingRight: 8,
                paddingTop: 3,
                paddingBottom: 3,
                borderRadius: 4,
              }}
            >
              {b.chunk.tenderCoverage != null && b.chunk.tenderCoverage >= 0.8 && (
                <div style={{ marginBottom: 3 }}>
                  <Pill fg="#7A5AB8" bg="rgba(122,90,184,0.13)" size={9.5}>
                    引用招标文件 · 覆盖 {Math.round(b.chunk.tenderCoverage * 100)}%
                  </Pill>
                </div>
              )}
              <div style={{ fontSize: 12.5, lineHeight: 1.85, color: ink, userSelect: "text" }}>
                <VerbatimText text={b.chunk.text} ranges={b.verbatim} />
              </div>
            </div>
          ),
        )}
        {total === 0 && <div style={{ fontSize: 11, color: mute }}>（该侧无可展示分块）</div>}
        {truncated && (
          <div style={{ marginTop: 4 }}>
            <Button kind="secondary" size="sm" onClick={onMore}>
              展开更多（已显示 {Math.min(cap, total)} / {total} 块）
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
