// 重复条款（原生版）：分页 + 虚拟列表（万级聚合不卡）+ 类型/风险/确认状态过滤 +
// 行内人工确认。点击行进入 ClusterDetail。
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { ClusterFilter, ClusterSummaryDto } from "../api/types";
import { useClustersInfinite, useCompareSummary, useSetReviewStatus } from "../queries/data";
import { docTag } from "../utils/docTag";
import {
  bandUi,
  RERANK_DISCLAIMER,
  rerankUi,
  REVIEW_UI,
  routingNote,
  severityUi,
  typeUi,
  zoneUi,
} from "../utils/clusterUi";

const TYPE_FILTERS: Array<{ key: string | undefined; label: string }> = [
  { key: undefined, label: "全部" },
  { key: "conflict", label: "冲突" },
  { key: "same", label: "相同" },
  { key: "minor_change", label: "轻微修改" },
  { key: "changed", label: "修改" },
  { key: "rewrite", label: "改写" },
  { key: "uncertain", label: "待复核" },
  { key: "added", label: "基准缺失" },
  { key: "deleted", label: "基准独有" },
];

// 五区筛选（W3-5）：标段分区。legal 阈值已上调、price 证据主体为金额事实冲突。
const ZONE_FILTERS: Array<{ key: string | undefined; label: string }> = [
  { key: undefined, label: "全部区" },
  { key: "tech", label: "技术标" },
  { key: "business", label: "商务标" },
  { key: "legal", label: "法定格式" },
  { key: "price", label: "报价清单" },
  { key: "other", label: "其他" },
];

// 复核路由三带筛选（W6-4）：【只筛不藏】——「低优先级抽查」带的条款一直在列表里
// （后端排序把它排到最后），这里的 chip 只是快速定位，不是可见性开关。
const BAND_FILTERS: Array<{ key: string | undefined; label: string }> = [
  { key: undefined, label: "全部" },
  { key: "flag", label: "重点标红" },
  { key: "review", label: "需人工复核" },
  { key: "pass", label: "低优先级抽查" },
  { key: "uncalibrated", label: "未校准" },
];

export function ClustersScreen() {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();
  const { dark } = useTheme();
  const [typeKey, setTypeKey] = useState<string | undefined>(undefined);
  const [zoneKey, setZoneKey] = useState<string | undefined>(undefined);
  const [onlyPending, setOnlyPending] = useState(false);
  // k-共现查证（W3-3）快捷筛选：仅两家共有（首要证据视图）/ 仅多家异常一致（待复核）。
  const [onlyTwoDocs, setOnlyTwoDocs] = useState(false);
  const [onlyAnomaly, setOnlyAnomaly] = useState(false);
  // 复核路由三带筛选（W6-4）：默认「全部」——三带只改排序，不做默认过滤，
  // 否则「低优先级抽查」带会在用户不知情时消失（§1.5-1 铁律：不隐藏任何簇）。
  const [bandKey, setBandKey] = useState<string | undefined>(undefined);
  // 「低优先级抽查」带默认折叠为单行（后端已把该带排到最后）。折叠 ≠ 隐藏：条款仍在列表里、
  // 仍计数、仍可点开、仍进导出——§1.5-1 铁律，这个开关只改行高不改可见性。
  const [collapsePass, setCollapsePass] = useState(true);

  const filter: ClusterFilter = useMemo(
    () => ({
      clusterType: typeKey,
      sectionKind: zoneKey,
      reviewStatus: onlyPending ? "pending" : undefined,
      twoDocsOnly: onlyTwoDocs ? true : undefined,
      multiDocAnomaly: onlyAnomaly ? true : undefined,
      band: bandKey,
    }),
    [typeKey, zoneKey, onlyPending, onlyTwoDocs, onlyAnomaly, bandKey],
  );
  const { data: summary } = useCompareSummary(jobId);
  const q = useClustersInfinite(jobId, filter);
  const review = useSetReviewStatus(jobId);

  const items: ClusterSummaryDto[] = useMemo(
    () => (q.data?.pages ?? []).flatMap((p) => p.items),
    [q.data],
  );
  const total = q.data?.pages[0]?.total ?? 0;

  // 三带计数（W6-4）：来自比对快照，与当前筛选无关；旧任务缺键 → 全部计入「未校准」。
  const bandCounts: Record<string, number | undefined> = useMemo(() => {
    const s = summary?.summary;
    if (!s) return {};
    const pass = s.bandPassCount ?? 0;
    const review = s.bandReviewCount ?? 0;
    const flag = s.bandFlagCount ?? 0;
    const uncal = s.bandUncalibratedCount ?? Math.max(0, s.clusterCount - pass - review - flag);
    return { all: s.clusterCount, pass, review, flag, uncalibrated: uncal };
  }, [summary?.summary]);

  const docOrder: string[] = (summary?.matrix?.documentIds as string[]) ?? [];
  const tagOf = (docId: string) => {
    const i = docOrder.indexOf(docId);
    return i >= 0 ? docTag(i) : "?";
  };

  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: items.length + (q.hasNextPage ? 1 : 0),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 92,
    overscan: 8,
  });

  // 滚到底部附近自动加载下一页（放 effect 里，不在 render 体内触发副作用）
  const vitems = virtualizer.getVirtualItems();
  const lastIndex = vitems.length > 0 ? vitems[vitems.length - 1].index : -1;
  useEffect(() => {
    if (lastIndex >= items.length - 5 && q.hasNextPage && !q.isFetchingNextPage) {
      void q.fetchNextPage();
    }
  }, [lastIndex, items.length, q]);

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar
        title="重复条款"
        sub={`${total} 组跨文档雷同条款`}
        actions={
          <div style={{ display: "flex", gap: 8 }}>
            <Button kind="secondary" size="sm" icon="diff" onClick={() => nav(`/workspace/${wsId}/job/${jobId}/segments`)}>
              对齐区段
            </Button>
            <Button kind="secondary" size="sm" onClick={() => nav(`/workspace/${wsId}/job/${jobId}`)}>
              返回报告
            </Button>
          </div>
        }
      />
      {/* 过滤器 */}
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
        {TYPE_FILTERS.map((f) => {
          const active = typeKey === f.key;
          return (
            <span
              key={f.label}
              role="button"
              tabIndex={0}
              onClick={() => setTypeKey(f.key)}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setTypeKey(f.key); } }}
              style={{
                fontSize: 11,
                padding: "4px 10px",
                borderRadius: 999,
                cursor: "pointer",
                background: active ? "rgba(79,88,168,0.15)" : "transparent",
                color: active ? "var(--accent, #4F58A8)" : mute,
                border: `1px solid ${active ? "var(--accent, #4F58A8)" : border}`,
                fontWeight: active ? 700 : 500,
              }}
            >
              {f.label}
            </span>
          );
        })}
        <span style={{ flex: 1 }} />
        <QuickToggle
          label="仅两家共有"
          active={onlyTwoDocs}
          activeColor="var(--accent, #4F58A8)"
          mute={mute}
          border={border}
          onToggle={() => setOnlyTwoDocs((v) => !v)}
        />
        <QuickToggle
          label="多家异常一致"
          active={onlyAnomaly}
          activeColor="#C0392B"
          mute={mute}
          border={border}
          onToggle={() => setOnlyAnomaly((v) => !v)}
        />
        <QuickToggle
          label="折叠低优先级抽查"
          active={collapsePass}
          activeColor="var(--accent, #4F58A8)"
          mute={mute}
          border={border}
          onToggle={() => setCollapsePass((v) => !v)}
        />
        <QuickToggle
          label="只看待确认"
          active={onlyPending}
          activeColor="var(--accent, #4F58A8)"
          mute={mute}
          border={border}
          onToggle={() => setOnlyPending((v) => !v)}
        />
      </div>

      {/* 复核路由三带（W6-4）：计数 chips + 分流说明。计数取自比对快照（summary），
          与列表筛选独立——即使当前筛掉了某带，计数仍显示全量，避免「看不见 = 不存在」。 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "8px 24px",
          borderBottom: `1px solid ${border}`,
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: 11, color: mute, marginRight: 2 }}>复核路由</span>
        {BAND_FILTERS.map((f) => {
          const active = bandKey === f.key;
          const ui = f.key ? bandUi(f.key) : null;
          const n = bandCounts[f.key ?? "all"];
          return (
            <span
              key={f.label}
              role="button"
              tabIndex={0}
              title={ui?.hint ?? "不过滤：三带只改排序，不隐藏任何条款"}
              onClick={() => setBandKey(f.key)}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setBandKey(f.key); } }}
              style={{
                fontSize: 11,
                padding: "4px 10px",
                borderRadius: 999,
                cursor: "pointer",
                background: active ? (ui ? ui.bg : "rgba(79,88,168,0.15)") : "transparent",
                color: active ? (ui ? ui.fg : "var(--accent, #4F58A8)") : mute,
                border: `1px solid ${active ? (ui ? ui.fg : "var(--accent, #4F58A8)") : border}`,
                fontWeight: active ? 700 : 500,
              }}
            >
              {f.label}
              {n != null ? ` ${n}` : ""}
            </span>
          );
        })}
        <span style={{ fontSize: 10.5, color: mute, flexBasis: "100%", marginTop: 2 }}>
          {routingNote(summary?.summary?.calibrationRouting, summary?.summary?.calibrationAlpha, summary?.summary?.calibrationBeta)}
        </span>
        {/* 交叉复核降级（W6-2）：开了复核但模型不可用 ⇒ 明说没跑成，不能让缺失的倾向分
            被读成「没有嫌疑」。已跑成时说明覆盖了多少条款。 */}
        {summary?.summary?.rerankDegraded && (
          <span style={{ fontSize: 10.5, color: "#B06A3B", flexBasis: "100%" }}>
            交叉复核未执行：复核模型未缓存且当前离线，本次比对未产出「AI 复核倾向」（其余结果不受影响）。可在工具箱预下载复核模型后重跑。
          </span>
        )}
        {!summary?.summary?.rerankDegraded && (summary?.summary?.rerankReviewedCount ?? 0) > 0 && (
          <span style={{ fontSize: 10.5, color: mute, flexBasis: "100%" }}>
            {`已对 ${summary?.summary?.rerankReviewedCount} 个「待复核」条款给出 AI 复核倾向：${RERANK_DISCLAIMER}。`}
          </span>
        )}
      </div>

      {/* 五区筛选（W3-5）：按标段分区过滤 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "8px 24px",
          borderBottom: `1px solid ${border}`,
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: 11, color: mute, marginRight: 2 }}>标段</span>
        {ZONE_FILTERS.map((f) => {
          const active = zoneKey === f.key;
          return (
            <span
              key={f.label}
              role="button"
              tabIndex={0}
              onClick={() => setZoneKey(f.key)}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setZoneKey(f.key); } }}
              style={{
                fontSize: 11,
                padding: "4px 10px",
                borderRadius: 999,
                cursor: "pointer",
                background: active ? "rgba(79,88,168,0.15)" : "transparent",
                color: active ? "var(--accent, #4F58A8)" : mute,
                border: `1px solid ${active ? "var(--accent, #4F58A8)" : border}`,
                fontWeight: active ? 700 : 500,
              }}
            >
              {f.label}
            </span>
          );
        })}
      </div>

      {/* 虚拟列表 */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "10px 24px" }}>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((vi) => {
            const c = items[vi.index];
            return (
              <div
                key={vi.key}
                data-index={vi.index}
                ref={virtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${vi.start}px)`,
                  paddingBottom: 8,
                }}
              >
                {c ? (
                  <ClusterRow
                    c={c}
                    collapsed={collapsePass && c.band === "pass"}
                    tagOf={tagOf}
                    cardBg={cardBg}
                    border={border}
                    ink={ink}
                    mute={mute}
                    onOpen={() => nav(`/workspace/${wsId}/job/${jobId}/cluster/${c.id}`)}
                    onReview={(status) => review.mutate({ clusterId: c.id, status })}
                  />
                ) : (
                  <div style={{ fontSize: 12, color: mute, padding: 12, textAlign: "center" }}>
                    加载更多…
                  </div>
                )}
              </div>
            );
          })}
        </div>
        {items.length === 0 && !q.isLoading && (
          <div style={{ fontSize: 12.5, color: mute, padding: "24px 4px", textAlign: "center" }}>
            当前过滤条件下没有条款
          </div>
        )}
      </div>
    </div>
  );
}

function ClusterRow({
  c,
  collapsed,
  tagOf,
  cardBg,
  border,
  ink,
  mute,
  onOpen,
  onReview,
}: {
  c: ClusterSummaryDto;
  /** 折叠为单行（低优先级抽查带默认折叠）。折叠只压缩行高，不影响可见性与可点击。 */
  collapsed?: boolean;
  tagOf: (id: string) => string;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  onOpen: () => void;
  onReview: (status: string) => void;
}) {
  const t = typeUi(c.clusterType);
  const sev = severityUi(c.severity);
  const zone = zoneUi(c.sectionKind);
  const band = bandUi(c.band);
  // 交叉复核建议（W6-2）：只在跑过复核层的簇上出现；【不改判分类】，仅供排序与人工参考。
  const lean = rerankUi(c.rerankScore);
  const review = REVIEW_UI[c.reviewStatus] ?? REVIEW_UI.pending;
  // k-共现查证（W3-3）：豁免簇（合法共享）置灰、标出处；异常簇标『待复核·涉嫌一致』红徽标。
  const exemptLabel =
    c.exemptReason === "tender" ? "引用招标文件" : c.exemptReason === "background" ? "行业范本套话" : null;
  if (collapsed) {
    // 折叠态：单行摘要（徽标 + 标题 + 相似度），点击照常进详情。折叠只是把复核注意力
    // 让给前面的带，条款本身没有被隐藏或剔除（§1.5-1）。
    return (
      <div
        role="button"
        tabIndex={0}
        title={`${band.label}：${band.hint}`}
        onClick={onOpen}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(); } }}
        style={{
          background: cardBg,
          border: `1px solid ${border}`,
          borderRadius: 8,
          padding: "6px 12px",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          gap: 8,
          opacity: 0.75,
        }}
      >
        <Pill fg={band.fg} bg={band.bg} size={10}>
          {band.label}
        </Pill>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            color: ink,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {c.topic ?? "（无标题条款）"}
        </span>
        {c.score != null && (
          <span style={{ fontSize: 11, color: mute, fontVariantNumeric: "tabular-nums" }}>
            {Math.round(c.score * 100)}%
          </span>
        )}
      </div>
    );
  }
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(); } }}
      style={{
        background: cardBg,
        border: `1px solid ${c.multiDocAnomaly ? "rgba(192,57,43,0.5)" : border}`,
        borderRadius: 10,
        padding: "11px 14px",
        cursor: "pointer",
        display: "flex",
        flexDirection: "column",
        gap: 7,
        opacity: exemptLabel ? 0.55 : 1,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Pill fg={t.fg} bg={t.bg} size={10.5} weight={700}>
          {t.label}
        </Pill>
        {/* 复核路由三带（W6-4）：与分类/风险并列的正交维度徽标 */}
        <span title={band.hint} style={{ display: "inline-flex" }}>
          <Pill fg={band.fg} bg={band.bg} size={10.5}>
            {band.label}
          </Pill>
        </span>
        <Pill fg={zone.fg} bg={zone.bg} size={10.5}>
          {zone.label}
        </Pill>
        {exemptLabel && (
          <Pill fg={mute} bg="rgba(128,128,128,0.14)" size={10.5}>
            {`合法共享 · ${exemptLabel}`}
          </Pill>
        )}
        {c.multiDocAnomaly && (
          <Pill fg="#C0392B" bg="rgba(192,57,43,0.12)" size={10.5} weight={700}>
            待复核 · 涉嫌多家异常一致
          </Pill>
        )}
        {lean && (
          <span title={lean.hint} style={{ display: "inline-flex" }}>
            <Pill fg={lean.fg} bg={lean.bg} size={10.5}>
              {lean.label}
            </Pill>
          </span>
        )}
        {sev && sev.label && !c.multiDocAnomaly && (
          <Pill fg={sev.fg} bg={sev.bg} size={10.5}>
            {sev.label}
          </Pill>
        )}
        <div
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12.5,
            fontWeight: 600,
            color: ink,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {c.topic ?? "（无标题条款）"}
        </div>
        {c.score != null && (
          <span style={{ fontSize: 11.5, color: mute, fontVariantNumeric: "tabular-nums" }}>
            {Math.round(c.score * 100)}%
          </span>
        )}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11, color: mute }}>
        <span style={{ display: "inline-flex", gap: 3 }}>
          {c.documentIds.map((id) => (
            <span
              key={id}
              style={{
                width: 17,
                height: 17,
                borderRadius: 5,
                background: "rgba(79,88,168,0.13)",
                color: "var(--accent, #4F58A8)",
                fontSize: 10,
                fontWeight: 700,
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              {tagOf(id)}
            </span>
          ))}
        </span>
        <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {[
            c.sectionPath,
            c.page != null ? `第 ${c.page} 页` : null,
            c.summary,
          ]
            .filter(Boolean)
            .join(" · ")}
        </span>
        <span style={{ color: review.fg, fontWeight: 600 }}>{review.label}</span>
        {c.reviewStatus === "pending" ? (
          <>
            <ReviewBtn label="确认" onClick={(e) => { e.stopPropagation(); onReview("confirmed"); }} />
            <ReviewBtn label="忽略" onClick={(e) => { e.stopPropagation(); onReview("ignored"); }} />
          </>
        ) : (
          <ReviewBtn label="重置" onClick={(e) => { e.stopPropagation(); onReview("pending"); }} />
        )}
      </div>
    </div>
  );
}

function QuickToggle({
  label,
  active,
  activeColor,
  mute,
  border,
  onToggle,
}: {
  label: string;
  active: boolean;
  activeColor: string;
  mute: string;
  border: string;
  onToggle: () => void;
}) {
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onToggle(); } }}
      style={{
        fontSize: 11,
        padding: "4px 10px",
        borderRadius: 999,
        cursor: "pointer",
        color: active ? activeColor : mute,
        border: `1px solid ${active ? activeColor : border}`,
        fontWeight: active ? 700 : 500,
      }}
    >
      {label}
    </span>
  );
}

function ReviewBtn({
  label,
  onClick,
}: {
  label: string;
  onClick: (e: React.MouseEvent) => void;
}) {
  const { dark } = useTheme();
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick(e as any); } }}
      style={{
        fontSize: 10.5,
        padding: "2px 8px",
        borderRadius: 6,
        border: `1px solid ${dark ? "rgba(255,255,255,0.14)" : C.line}`,
        color: dark ? "rgba(255,255,255,0.75)" : C.ink2,
        cursor: "pointer",
        flexShrink: 0,
      }}
    >
      {label}
    </span>
  );
}
