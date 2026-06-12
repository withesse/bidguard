// 文档原文预览：双模式——
// 「智能分块」：段落级分块虚拟滚动，?chunk=<id> 定位高亮，行内批注；
// 「原文版式」：pdf.js（文本可选中拷贝；扫描件叠 OCR 隐形文本层）/ docx-preview 保真渲染，
//             选中文本即可添加引文批注。原文件只读，修改请「用系统应用打开」改完重新导入。
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useVirtualizer } from "@tanstack/react-virtual";
import { openPath } from "@tauri-apps/plugin-opener";
import { C } from "../design/tokens";
import { Topbar } from "../components/Topbar";
import { Button, Pill, SegControl } from "../components/primitives";
import { NoteEditor } from "../components/NoteEditor";
import { PdfView } from "../components/PdfView";
import { DocxView } from "../components/DocxView";
import { MdView } from "../components/MdView";
import { TxtView } from "../components/TxtView";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import {
  useAddAnnotation,
  useAnnotations,
  useDeleteAnnotation,
  useDocumentFile,
  useDocumentOcrLayout,
  useDocumentPreview,
  useUpdateAnnotation,
} from "../queries/data";
import type { AnnotationDto, OcrLine } from "../api/types";

const METHOD_CN: Record<string, string> = {
  docx: "Word 文档",
  text: "纯文本",
  pdfium: "PDF 文本层",
  "pdf-extract": "PDF 文本层",
  ocr: "扫描件 OCR",
  xlsx: "Excel 表格",
  cache: "缓存复用",
};

export function DocPreview() {
  const { wsId, docId } = useParams();
  const [sp] = useSearchParams();
  const targetChunk = sp.get("chunk");
  const nav = useNavigate();
  const { dark } = useTheme();
  const toast = useToast();
  const { data, isLoading, error } = useDocumentPreview(docId);

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;
  const bg = dark ? "#141419" : C.paper;

  const doc = data?.document;
  const chunks = useMemo(() => data?.chunks ?? [], [data]);
  const isPdf = doc?.fileType === "pdf";
  const isDocx = doc?.fileType === "docx";
  const isMd = doc?.fileType === "md";
  const isTxt = doc?.fileType === "txt";
  // xlsx 不开版式视图：分块模式的表格行渲染即其自然形态
  const canLayout = isPdf || isDocx || isMd || isTxt;
  const [mode, setMode] = useState<"chunks" | "layout">("chunks");
  const layoutOn = mode === "layout" && canLayout;

  const fileQ = useDocumentFile(docId, layoutOn);
  const ocrQ = useDocumentOcrLayout(docId, layoutOn && isPdf && doc?.parseMethod === "ocr");
  const ocrLayout: OcrLine[][] | null = useMemo(() => {
    if (!ocrQ.data) return null;
    try {
      return JSON.parse(ocrQ.data) as OcrLine[][];
    } catch {
      return null;
    }
  }, [ocrQ.data]);

  // —— 批注 ——
  const annQ = useAnnotations(wsId);
  const docAnns = useMemo(
    () => (annQ.data ?? []).filter((a) => a.documentId === docId),
    [annQ.data, docId],
  );
  const annsOfChunk = useMemo(() => {
    const m = new Map<string, AnnotationDto[]>();
    for (const a of docAnns) {
      if (a.chunkId) {
        const arr = m.get(a.chunkId) ?? [];
        arr.push(a);
        m.set(a.chunkId, arr);
      }
    }
    return m;
  }, [docAnns]);
  const addAnn = useAddAnnotation(wsId);
  const updAnn = useUpdateAnnotation(wsId);
  const delAnn = useDeleteAnnotation(wsId);
  const [notesOpen, setNotesOpen] = useState(false);
  const [editingChunk, setEditingChunk] = useState<string | null>(null);
  // 版式模式：选中文本 → 浮动「批注选中内容」
  const [selDraft, setSelDraft] = useState<{ quote: string; page: number | null } | null>(null);
  const [selEditing, setSelEditing] = useState(false);

  const onLayoutMouseUp = () => {
    const sel = window.getSelection();
    const text = sel?.toString().trim() ?? "";
    if (text.length < 4) {
      if (!selEditing) setSelDraft(null);
      return;
    }
    let page: number | null = null;
    const node = sel?.anchorNode;
    const el = node instanceof Element ? node : node?.parentElement;
    const pageEl = el?.closest?.("[data-page]");
    if (pageEl) page = Number(pageEl.getAttribute("data-page")) || null;
    setSelDraft({ quote: text.slice(0, 300), page });
    setSelEditing(false);
  };

  const saveAnnotation = (a: {
    note: string;
    chunkId?: string;
    page?: number | null;
    quote?: string;
  }) => {
    if (!wsId || !docId) return;
    addAnn.mutate(
      {
        workspaceId: wsId,
        documentId: docId,
        note: a.note,
        chunkId: a.chunkId,
        page: a.page ?? undefined,
        quote: a.quote,
      },
      {
        onSuccess: () => toast.show("批注已保存", "success"),
        onError: (e) => toast.show("批注保存失败：" + errMsg(e), "error"),
      },
    );
  };

  // —— 分块模式虚拟列表 ——
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: chunks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 56,
    overscan: 12,
  });

  const [located, setLocated] = useState(false);
  useEffect(() => {
    if (located || mode !== "chunks" || chunks.length === 0 || !targetChunk) return;
    let idx = chunks.findIndex((c) => c.id === targetChunk);
    if (idx < 0) idx = 0; // 比对粒度与预览粒度不同时回落到开头
    virtualizer.scrollToIndex(idx, { align: "center" });
    setLocated(true);
  }, [chunks, targetChunk, located, virtualizer, mode]);

  // 版式模式定位锚点：目标分块的页码（pdf）/ 开头文本（docx）
  const targetObj = useMemo(
    () => chunks.find((c) => c.id === targetChunk) ?? null,
    [chunks, targetChunk],
  );

  const sub = doc
    ? [
        doc.pageCount != null ? `${doc.pageCount} 页` : null,
        doc.charCount != null ? `${doc.charCount.toLocaleString()} 字` : null,
        doc.parseMethod ? (METHOD_CN[doc.parseMethod] ?? doc.parseMethod) : null,
        chunks.length >= 5000 ? "超长文档，仅展示前 5000 段" : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "加载中…";

  const openOriginal = async () => {
    if (!doc) return;
    try {
      await openPath(doc.filePath);
    } catch (e) {
      toast.show("打开失败（文件可能已移动）：" + errMsg(e), "error");
    }
  };

  return (
    <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", flexDirection: "column", overflow: "hidden", background: bg, position: "relative" }}>
      <style>{`.bg-chunk-row .bg-ann-btn{opacity:0;transition:opacity .12s}.bg-chunk-row:hover .bg-ann-btn{opacity:1}`}</style>
      <Topbar
        title={doc?.fileName ?? "文档预览"}
        sub={sub}
        actions={
          <>
            {canLayout && (
              <SegControl
                options={["智能分块", "原文版式"]}
                value={mode === "chunks" ? 0 : 1}
                onChange={(i) => setMode(i === 0 ? "chunks" : "layout")}
              />
            )}
            <Button kind="ghost" size="sm" onClick={() => setNotesOpen((v) => !v)}>
              批注{docAnns.length > 0 ? ` ${docAnns.length}` : ""}
            </Button>
            <Button kind="secondary" size="sm" onClick={openOriginal} title="用系统默认应用打开原文件（修改后重新导入生效）">
              用系统应用打开
            </Button>
            <Button kind="secondary" size="sm" onClick={() => nav(-1)}>
              返回
            </Button>
          </>
        }
      />
      {error != null && (
        <div style={{ padding: 24, fontSize: 12.5, color: mute }}>原文加载失败，请确认文档仍在工作区中。</div>
      )}
      {!error && !isLoading && chunks.length === 0 && (
        <div style={{ padding: 24, fontSize: 12.5, color: mute }}>该文档没有可预览的段落（可能解析失败或全部为短噪声段）。</div>
      )}

      {/* —— 智能分块模式 —— */}
      {mode === "chunks" && (
        <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "16px 32px 32px" }}>
          <div style={{ height: virtualizer.getTotalSize(), position: "relative", maxWidth: 860, margin: "0 auto" }}>
            {virtualizer.getVirtualItems().map((vi) => {
              const c = chunks[vi.index];
              if (!c) return null;
              const hit = targetChunk != null && c.id === targetChunk;
              const anns = annsOfChunk.get(c.id) ?? [];
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
                    paddingBottom: 10,
                  }}
                >
                  <div className="bg-chunk-row" style={{ position: "relative" }}>
                    <ChunkBlock c={c} hit={hit} ink={ink} mute={mute} cardBg={cardBg} border={border} />
                    <span
                      className="bg-ann-btn"
                      onClick={() => setEditingChunk(editingChunk === c.id ? null : c.id)}
                      style={{
                        position: "absolute",
                        right: 0,
                        top: 2,
                        fontSize: 10.5,
                        fontWeight: 600,
                        color: "#6B73C9",
                        cursor: "pointer",
                        background: cardBg,
                        padding: "1px 6px",
                        borderRadius: 5,
                        border: `1px solid ${border}`,
                      }}
                    >
                      批注
                    </span>
                    {editingChunk === c.id && (
                      <div style={{ margin: "6px 0 0 44px" }}>
                        <NoteEditor
                          onCancel={() => setEditingChunk(null)}
                          onSave={(note) => {
                            saveAnnotation({ note, chunkId: c.id, page: c.page, quote: c.text.slice(0, 120) });
                            setEditingChunk(null);
                          }}
                        />
                      </div>
                    )}
                    {anns.map((a) => (
                      <AnnBubble key={a.id} a={a} mute={mute} ink={ink} border={border} cardBg={cardBg}
                        onUpdate={(note) => updAnn.mutate({ id: a.id, note })}
                        onDelete={() => delAnn.mutate(a.id)}
                      />
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* —— 原文版式模式 —— */}
      {layoutOn && (
        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }} onMouseUp={onLayoutMouseUp}>
          {fileQ.isLoading && <div style={{ padding: 24, fontSize: 12.5, color: mute }}>读取原文件…</div>}
          {fileQ.error != null && (
            <div style={{ padding: 24, fontSize: 12.5, color: mute }}>原文件读取失败：{errMsg(fileQ.error)}</div>
          )}
          {fileQ.data && isPdf && (
            <PdfView data={fileQ.data} ocrLayout={ocrLayout} targetPage={targetObj?.page ?? null} border={border} />
          )}
          {fileQ.data && isDocx && <DocxView data={fileQ.data} anchorText={targetObj?.text ?? null} />}
          {fileQ.data && isMd && <MdView data={fileQ.data} anchorText={targetObj?.text ?? null} />}
          {fileQ.data && isTxt && <TxtView data={fileQ.data} />}

          {/* 选中文本 → 引文批注 */}
          {selDraft && (
            <div
              style={{
                position: "absolute",
                left: "50%",
                bottom: 24,
                transform: "translateX(-50%)",
                width: selEditing ? 420 : "auto",
                maxWidth: "80%",
                background: cardBg,
                border: `1px solid ${border}`,
                borderRadius: 10,
                boxShadow: "0 6px 24px rgba(20,18,14,0.18)",
                padding: selEditing ? 12 : 6,
                zIndex: 10,
              }}
            >
              {!selEditing ? (
                <Button kind="primary" size="sm" onClick={() => setSelEditing(true)}>
                  批注选中内容
                </Button>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  <div style={{ fontSize: 11, color: mute, maxHeight: 60, overflow: "hidden" }}>
                    “{selDraft.quote.slice(0, 120)}{selDraft.quote.length > 120 ? "…" : ""}”
                    {selDraft.page != null && <Pill fg="#6B73C9" bg="rgba(79,88,168,0.12)" size={10}> 第 {selDraft.page} 页</Pill>}
                  </div>
                  <NoteEditor
                    onCancel={() => {
                      setSelDraft(null);
                      setSelEditing(false);
                    }}
                    onSave={(note) => {
                      saveAnnotation({ note, page: selDraft.page, quote: selDraft.quote });
                      setSelDraft(null);
                      setSelEditing(false);
                    }}
                  />
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* —— 批注侧栏 —— */}
      {notesOpen && (
        <div
          style={{
            position: "absolute",
            top: 64,
            right: 12,
            bottom: 12,
            width: 320,
            background: cardBg,
            border: `1px solid ${border}`,
            borderRadius: 12,
            boxShadow: "0 8px 28px rgba(20,18,14,0.16)",
            display: "flex",
            flexDirection: "column",
            zIndex: 20,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", padding: "10px 14px", borderBottom: `1px solid ${border}` }}>
            <span style={{ flex: 1, fontSize: 12.5, fontWeight: 700, color: ink }}>本文档批注（{docAnns.length}）</span>
            <span onClick={() => setNotesOpen(false)} style={{ cursor: "pointer", color: mute, fontSize: 12 }}>✕</span>
          </div>
          <div style={{ flex: 1, overflowY: "auto", padding: 10, display: "flex", flexDirection: "column", gap: 8 }}>
            {docAnns.length === 0 && (
              <div style={{ fontSize: 11.5, color: mute, padding: 8 }}>
                还没有批注。分块模式下悬停段落点「批注」，或在原文版式中选中文本添加引文批注。
              </div>
            )}
            {docAnns.map((a) => (
              <AnnBubble key={a.id} a={a} mute={mute} ink={ink} border={border} cardBg={dark ? "rgba(255,255,255,0.03)" : C.paper2}
                onUpdate={(note) => updAnn.mutate({ id: a.id, note })}
                onDelete={() => delAnn.mutate(a.id)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** 一条批注气泡：引文 + 内容 + 编辑/删除。 */
function AnnBubble({
  a,
  ink,
  mute,
  border,
  cardBg,
  onUpdate,
  onDelete,
}: {
  a: AnnotationDto;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
  onUpdate: (note: string) => void;
  onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  return (
    <div
      style={{
        margin: "6px 0 0 44px",
        background: cardBg,
        border: `1px solid ${border}`,
        borderLeft: "3px solid #6B73C9",
        borderRadius: 8,
        padding: "8px 10px",
      }}
    >
      {a.quote && (
        <div style={{ fontSize: 10.5, color: mute, marginBottom: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          “{a.quote}”{a.page != null ? ` · 第 ${a.page} 页` : ""}
        </div>
      )}
      {editing ? (
        <NoteEditor
          initial={a.note}
          onCancel={() => setEditing(false)}
          onSave={(note) => {
            onUpdate(note);
            setEditing(false);
          }}
        />
      ) : (
        <div style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
          <span style={{ flex: 1, fontSize: 12, lineHeight: 1.6, color: ink }}>{a.note}</span>
          <span onClick={() => setEditing(true)} style={{ fontSize: 10.5, color: "#6B73C9", cursor: "pointer", flexShrink: 0 }}>编辑</span>
          <span onClick={onDelete} style={{ fontSize: 10.5, color: mute, cursor: "pointer", flexShrink: 0 }}>删除</span>
        </div>
      )}
    </div>
  );
}

function ChunkBlock({
  c,
  hit,
  ink,
  mute,
  cardBg,
  border,
}: {
  c: { id: string; chunkType: string; text: string; page: number | null };
  hit: boolean;
  ink: string;
  mute: string;
  cardBg: string;
  border: string;
}) {
  const hitStyle = hit
    ? { background: "rgba(79,88,168,0.10)", border: "1px solid #6B73C9", borderRadius: 8, padding: "8px 10px" }
    : {};

  if (c.chunkType === "heading") {
    return (
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, paddingTop: 10, ...hitStyle }}>
        <span style={{ fontSize: 14.5, fontWeight: 800, color: ink, fontFamily: C.serif }}>{c.text}</span>
        {c.page != null && <span style={{ fontSize: 10, color: mute }}>第 {c.page} 页</span>}
      </div>
    );
  }

  if (c.chunkType === "table_row") {
    const cells = c.text.split(" | ");
    return (
      <div style={{ ...hitStyle }}>
        <div
          style={{
            display: "flex",
            background: cardBg,
            border: `1px solid ${border}`,
            borderRadius: 6,
            overflow: "hidden",
          }}
        >
          {cells.map((cell, i) => (
            <div
              key={i}
              style={{
                flex: 1,
                minWidth: 0,
                padding: "6px 10px",
                fontSize: 12,
                color: ink,
                borderLeft: i > 0 ? `1px solid ${border}` : "none",
                wordBreak: "break-all",
              }}
            >
              {cell}
            </div>
          ))}
        </div>
      </div>
    );
  }

  const isList = c.chunkType === "list_item";
  return (
    <div style={{ display: "flex", gap: 10, ...hitStyle }}>
      <span style={{ flexShrink: 0, width: 34, fontSize: 10, color: mute, textAlign: "right", paddingTop: 3 }}>
        {c.page != null ? `P${c.page}` : ""}
      </span>
      {isList && <span style={{ flexShrink: 0, color: mute, fontSize: 13, lineHeight: 1.8 }}>•</span>}
      <p
        style={{
          flex: 1,
          minWidth: 0,
          margin: 0,
          fontSize: 13,
          lineHeight: 1.8,
          color: ink,
          wordBreak: "break-word",
        }}
      >
        {c.text}
      </p>
    </div>
  );
}
