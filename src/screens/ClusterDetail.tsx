// 条款详情：多文档并排（十天干列头）+ 分级 diff 高亮 + 事实字段差异表 +
// 冲突风险解释 + 章节路径/页码定位 + 人工确认。
import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { DiffOp } from "../engine";
import type { FactRowDto, MemberDetailDto } from "../api/types";
import {
  useAddAnnotation,
  useAnnotations,
  useClusterDetail,
  useCompareSummary,
  useDeleteAnnotation,
  useSetReviewStatus,
  useUpdateAnnotation,
} from "../queries/data";
import type { AnnotationDto } from "../api/types";
import { NoteEditor } from "../components/NoteEditor";
import { docTag } from "../utils/docTag";
import { REVIEW_UI, severityUi, typeUi } from "../utils/clusterUi";

interface ConflictJson {
  risk: string;
  fields: Array<{ field: string; values: Array<{ doc: number; value: string }> }>;
}

const FIELD_LABEL: Record<string, string> = {
  amount: "金额",
  duration: "工期",
  date: "日期",
  percentage: "比例",
  subject: "责任主体",
};

export function ClusterDetail() {
  const { wsId, jobId, cid } = useParams<{ wsId: string; jobId: string; cid: string }>();
  const nav = useNavigate();
  const { dark } = useTheme();
  const { data, isLoading } = useClusterDetail(cid);
  const { data: summary } = useCompareSummary(jobId);
  const review = useSetReviewStatus(jobId);
  // 批注：按成员 chunk 锚定（评审记录入库，导出与重启后仍在）
  const { data: anns } = useAnnotations(wsId);
  const addAnn = useAddAnnotation(wsId);
  const updAnn = useUpdateAnnotation(wsId);
  const delAnn = useDeleteAnnotation(wsId);
  const annsOfChunk = useMemo(() => {
    const m = new Map<string, AnnotationDto[]>();
    for (const a of anns ?? []) {
      if (a.chunkId) {
        const arr = m.get(a.chunkId) ?? [];
        arr.push(a);
        m.set(a.chunkId, arr);
      }
    }
    return m;
  }, [anns]);

  const docOrder: string[] = (summary?.matrix?.documentIds as string[]) ?? [];
  const idxOf = (docId: string) => docOrder.indexOf(docId);

  const conflict: ConflictJson | null = useMemo(() => {
    try {
      return data?.conflictJson ? (JSON.parse(data.conflictJson) as ConflictJson) : null;
    } catch {
      return null;
    }
  }, [data?.conflictJson]);

  // diff 按 target chunk 索引（成员渲染时套用自己的 diff 高亮）
  const diffOfChunk = useMemo(() => {
    const m = new Map<string, DiffOp[]>();
    for (const d of data?.diffs ?? []) {
      if (d.targetChunkId) {
        try {
          m.set(d.targetChunkId, JSON.parse(d.diffJson) as DiffOp[]);
        } catch {
          // 坏 diff 行不阻塞渲染
        }
      }
    }
    return m;
  }, [data?.diffs]);

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;

  if (isLoading || !data) {
    return (
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: mute, fontSize: 13 }}>
        {isLoading ? "正在加载条款详情…" : "条款不存在"}
      </div>
    );
  }

  const c = data.cluster;
  const t = typeUi(c.clusterType);
  const sev = severityUi(c.severity);
  const rv = REVIEW_UI[c.reviewStatus] ?? REVIEW_UI.pending;
  // 成员按文档位次排序，primary 优先
  const members = [...data.members].sort((a, b) => {
    const d = idxOf(a.documentId) - idxOf(b.documentId);
    return d !== 0 ? d : a.role === "primary" ? -1 : 1;
  });

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar
        title={c.topic ?? "条款详情"}
        sub={c.summary ?? ""}
        actions={
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <span style={{ fontSize: 11.5, color: rv.fg, fontWeight: 600 }}>{rv.label}</span>
            <Button kind="secondary" size="sm" onClick={() => review.mutate({ clusterId: c.id, status: "confirmed" })}>
              确认
            </Button>
            <Button kind="secondary" size="sm" onClick={() => review.mutate({ clusterId: c.id, status: "ignored" })}>
              忽略
            </Button>
            <Button kind="secondary" size="sm" onClick={() => nav(`/workspace/${wsId}/job/${jobId}/clusters`)}>
              返回列表
            </Button>
          </div>
        }
      />
      <div style={{ flex: 1, overflowY: "auto", padding: 24, display: "flex", flexDirection: "column", gap: 16 }}>
        {/* 标签行 */}
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <Pill fg={t.fg} bg={t.bg} size={11} weight={700}>
            {t.label}
          </Pill>
          {sev && sev.label && (
            <Pill fg={sev.fg} bg={sev.bg} size={11}>
              {sev.label}
            </Pill>
          )}
          {c.score != null && (
            <span style={{ fontSize: 12, color: mute }}>组内平均相似 {Math.round(c.score * 100)}%</span>
          )}
        </div>

        {/* 冲突解释 */}
        {conflict && (
          <div
            style={{
              background: "rgba(181,69,69,0.07)",
              border: "1px solid rgba(181,69,69,0.35)",
              borderRadius: 12,
              padding: "13px 16px",
            }}
          >
            <div style={{ fontSize: 12.5, fontWeight: 700, color: "#B54545", marginBottom: 8 }}>
              同一条款关键数字不一致
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              {conflict.fields.map((f) => (
                <div key={f.field} style={{ fontSize: 12, color: ink, display: "flex", gap: 10, flexWrap: "wrap" }}>
                  <span style={{ fontWeight: 700, minWidth: 32 }}>{FIELD_LABEL[f.field] ?? f.field}</span>
                  {f.values.map((v) => (
                    <span key={v.doc} style={{ color: mute }}>
                      「{docTag(v.doc)}」{v.value}
                    </span>
                  ))}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* 多文档并排 */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: `repeat(${Math.min(members.length, 3)}, minmax(240px, 1fr))`,
            gap: 12,
          }}
        >
          {members.map((m) => (
            <MemberPane
              key={m.chunkId}
              m={m}
              tag={docTag(idxOf(m.documentId))}
              diff={diffOfChunk.get(m.chunkId)}
              cardBg={cardBg}
              border={border}
              ink={ink}
              mute={mute}
              onSource={() => nav(`/workspace/${wsId}/doc/${m.documentId}?chunk=${m.chunkId}`)}
              anns={annsOfChunk.get(m.chunkId) ?? []}
              onAddNote={(note) =>
                addAnn.mutate({
                  workspaceId: wsId!,
                  documentId: m.documentId,
                  chunkId: m.chunkId,
                  clusterId: cid,
                  page: m.page ?? undefined,
                  quote: m.text.slice(0, 120),
                  note,
                })
              }
              onUpdateNote={(id, note) => updAnn.mutate({ id, note })}
              onDeleteNote={(id) => delAnn.mutate(id)}
            />
          ))}
        </div>

        {/* 事实字段表 */}
        {data.facts.length > 0 && (
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 16 }}>
            <div style={{ fontSize: 12, fontWeight: 700, color: ink, marginBottom: 10 }}>抽取的事实字段</div>
            <FactTable facts={data.facts} idxOf={idxOf} ink={ink} mute={mute} border={border} />
          </div>
        )}
      </div>
    </div>
  );
}

function MemberPane({
  m,
  tag,
  diff,
  cardBg,
  border,
  ink,
  mute,
  onSource,
  anns,
  onAddNote,
  onUpdateNote,
  onDeleteNote,
}: {
  m: MemberDetailDto;
  tag: string;
  diff: DiffOp[] | undefined;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  onSource: () => void;
  anns: AnnotationDto[];
  onAddNote: (note: string) => void;
  onUpdateNote: (id: string, note: string) => void;
  onDeleteNote: (id: string) => void;
}) {
  const [noting, setNoting] = useState(false);
  let sectionPath: string[] = [];
  try {
    sectionPath = m.sectionPath ? (JSON.parse(m.sectionPath) as string[]) : [];
  } catch {
    // 路径损坏只影响面包屑
  }
  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "12px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
        opacity: m.role === "duplicate_candidate" ? 0.8 : 1,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{
            width: 20,
            height: 20,
            borderRadius: 6,
            background: "rgba(79,88,168,0.13)",
            color: "#6B73C9",
            fontSize: 11,
            fontWeight: 700,
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {tag}
        </span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 11.5,
            fontWeight: 600,
            color: ink,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          title={m.documentName}
        >
          {m.documentName}
        </span>
        {m.role === "duplicate_candidate" && (
          <span style={{ fontSize: 10, color: mute }}>同文档重复</span>
        )}
        <span
          onClick={onSource}
          title="在原文中查看此段"
          style={{ fontSize: 10.5, color: "#6B73C9", cursor: "pointer", fontWeight: 600, flexShrink: 0 }}
        >
          原文
        </span>
        <span
          onClick={() => setNoting((v) => !v)}
          title="给这段加批注"
          style={{ fontSize: 10.5, color: anns.length > 0 ? "#C28430" : "#6B73C9", cursor: "pointer", fontWeight: 600, flexShrink: 0 }}
        >
          批注{anns.length > 0 ? ` ${anns.length}` : ""}
        </span>
      </div>
      {noting && (
        <NoteEditor
          onCancel={() => setNoting(false)}
          onSave={(note) => {
            onAddNote(note);
            setNoting(false);
          }}
        />
      )}
      {anns.map((a) => (
        <MemberNote key={a.id} a={a} ink={ink} mute={mute} border={border}
          onUpdate={(note) => onUpdateNote(a.id, note)} onDelete={() => onDeleteNote(a.id)} />
      ))}
      {(sectionPath.length > 0 || m.page != null) && (
        <div style={{ fontSize: 10.5, color: mute }}>
          {sectionPath.join(" › ")}
          {m.page != null && ` · 第 ${m.page} 页`}
          {` · 段 ${m.orderIndex + 1}`}
        </div>
      )}
      <div style={{ fontSize: 12.5, lineHeight: 1.8, color: ink, userSelect: "text" }}>
        {diff ? <DiffText ops={diff} side="target" /> : m.text}
      </div>
    </div>
  );
}

/** 渲染 diff ops：side=target 显示 eq+ins（del 不属于本侧文本）。 */
function DiffText({ ops, side }: { ops: DiffOp[]; side: "base" | "target" }) {
  return (
    <>
      {ops.map((op, i) => {
        if (op.op === "eq") {
          return (
            <span key={i} style={{ background: "rgba(194,132,48,0.16)", borderRadius: 2 }}>
              {op.text}
            </span>
          );
        }
        if (side === "target" && op.op === "ins") {
          return (
            <span key={i} style={{ background: "rgba(14,154,143,0.14)", borderRadius: 2 }}>
              {op.text}
            </span>
          );
        }
        if (side === "base" && op.op === "del") {
          return (
            <span key={i} style={{ background: "rgba(181,69,69,0.12)", textDecoration: "line-through", borderRadius: 2 }}>
              {op.text}
            </span>
          );
        }
        return null;
      })}
    </>
  );
}

function FactTable({
  facts,
  idxOf,
  ink,
  mute,
  border,
}: {
  facts: FactRowDto[];
  idxOf: (id: string) => number;
  ink: string;
  mute: string;
  border: string;
}) {
  const cols: Array<{ key: keyof FactRowDto; label: string }> = [
    { key: "subject", label: "主体" },
    { key: "action", label: "动作" },
    { key: "object", label: "对象" },
    { key: "amount", label: "金额" },
    { key: "duration", label: "工期" },
    { key: "date", label: "日期" },
    { key: "percentage", label: "比例" },
    { key: "obligationType", label: "义务类型" },
  ];
  const sorted = [...facts].sort((a, b) => idxOf(a.documentId) - idxOf(b.documentId));
  return (
    <div style={{ overflowX: "auto" }}>
      <table style={{ borderCollapse: "collapse", width: "100%", fontSize: 11.5 }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left", color: mute, fontWeight: 600, padding: "4px 10px 6px 0" }}>文档</th>
            {cols.map((c) => (
              <th key={c.key} style={{ textAlign: "left", color: mute, fontWeight: 600, padding: "4px 10px 6px 0" }}>
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((f) => (
            <tr key={f.chunkId} style={{ borderTop: `1px solid ${border}` }}>
              <td style={{ padding: "6px 10px 6px 0", color: ink, fontWeight: 700 }}>
                {docTag(idxOf(f.documentId))}
              </td>
              {cols.map((c) => (
                <td key={c.key} style={{ padding: "6px 10px 6px 0", color: ink, whiteSpace: "nowrap" }}>
                  {(f[c.key] as string | null) ?? "—"}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** 成员卡内的一条批注（评审记录，可编辑/删除）。 */
function MemberNote({
  a,
  ink,
  mute,
  border,
  onUpdate,
  onDelete,
}: {
  a: AnnotationDto;
  ink: string;
  mute: string;
  border: string;
  onUpdate: (note: string) => void;
  onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  if (editing) {
    return (
      <NoteEditor
        initial={a.note}
        onCancel={() => setEditing(false)}
        onSave={(note) => {
          onUpdate(note);
          setEditing(false);
        }}
      />
    );
  }
  return (
    <div
      style={{
        borderLeft: "3px solid #C28430",
        border: `1px solid ${border}`,
        borderRadius: 7,
        padding: "6px 8px",
        display: "flex",
        gap: 8,
        alignItems: "baseline",
      }}
    >
      <span style={{ flex: 1, fontSize: 11.5, lineHeight: 1.6, color: ink }}>{a.note}</span>
      <span onClick={() => setEditing(true)} style={{ fontSize: 10, color: "#6B73C9", cursor: "pointer", flexShrink: 0 }}>编辑</span>
      <span onClick={onDelete} style={{ fontSize: 10, color: mute, cursor: "pointer", flexShrink: 0 }}>删除</span>
    </div>
  );
}
