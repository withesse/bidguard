// 重复条款（原生版）：分页 + 虚拟列表（万级聚合不卡）+ 类型/风险/确认状态过滤 +
// 行内人工确认。点击行进入 ClusterDetail。
import { useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { ClusterFilter, ClusterSummaryDto } from "../api/types";
import { useClustersInfinite, useCompareSummary, useSetReviewStatus } from "../queries/data";
import { docTag } from "../utils/docTag";
import { REVIEW_UI, severityUi, typeUi } from "../utils/clusterUi";

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

export function ClustersScreen() {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();
  const { dark } = useTheme();
  const [typeKey, setTypeKey] = useState<string | undefined>(undefined);
  const [onlyPending, setOnlyPending] = useState(false);

  const filter: ClusterFilter = useMemo(
    () => ({
      clusterType: typeKey,
      reviewStatus: onlyPending ? "pending" : undefined,
    }),
    [typeKey, onlyPending],
  );
  const { data: summary } = useCompareSummary(jobId);
  const q = useClustersInfinite(jobId, filter);
  const review = useSetReviewStatus(jobId);

  const items: ClusterSummaryDto[] = useMemo(
    () => (q.data?.pages ?? []).flatMap((p) => p.items),
    [q.data],
  );
  const total = q.data?.pages[0]?.total ?? 0;

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

  // 滚到底部附近自动加载下一页
  const vitems = virtualizer.getVirtualItems();
  const last = vitems.length > 0 ? vitems[vitems.length - 1] : undefined;
  if (last && last.index >= items.length - 5 && q.hasNextPage && !q.isFetchingNextPage) {
    void q.fetchNextPage();
  }

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
          <Button kind="secondary" size="sm" onClick={() => nav(`/workspace/${wsId}/job/${jobId}`)}>
            返回报告
          </Button>
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
              onClick={() => setTypeKey(f.key)}
              style={{
                fontSize: 11,
                padding: "4px 10px",
                borderRadius: 999,
                cursor: "pointer",
                background: active ? "rgba(79,88,168,0.15)" : "transparent",
                color: active ? "#6B73C9" : mute,
                border: `1px solid ${active ? "#6B73C9" : border}`,
                fontWeight: active ? 700 : 500,
              }}
            >
              {f.label}
            </span>
          );
        })}
        <span style={{ flex: 1 }} />
        <span
          onClick={() => setOnlyPending((v) => !v)}
          style={{
            fontSize: 11,
            padding: "4px 10px",
            borderRadius: 999,
            cursor: "pointer",
            color: onlyPending ? "#6B73C9" : mute,
            border: `1px solid ${onlyPending ? "#6B73C9" : border}`,
            fontWeight: onlyPending ? 700 : 500,
          }}
        >
          只看待确认
        </span>
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
  tagOf,
  cardBg,
  border,
  ink,
  mute,
  onOpen,
  onReview,
}: {
  c: ClusterSummaryDto;
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
  const review = REVIEW_UI[c.reviewStatus] ?? REVIEW_UI.pending;
  return (
    <div
      onClick={onOpen}
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 10,
        padding: "11px 14px",
        cursor: "pointer",
        display: "flex",
        flexDirection: "column",
        gap: 7,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Pill fg={t.fg} bg={t.bg} size={10.5} weight={700}>
          {t.label}
        </Pill>
        {sev && sev.label && (
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
                color: "#6B73C9",
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
      onClick={onClick}
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
