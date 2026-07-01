// 查重源批量导入：粘贴或选文件 → 四格式解析 → 预览去重 → 原子提交。
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Button, Pill, SegControl } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import { readTextFile } from "../api";
import { useBatchSaveTemplates } from "../queries/data";
import { parseTemplates, type ParseFormat, type ParsedRow } from "../utils/templateParse";
import { useFocusTrap } from "../utils/useFocusTrap";

const FORMATS: { label: string; value: ParseFormat }[] = [
  { label: "空行分段", value: "blank" },
  { label: "分类|名称|正文", value: "pipe" },
  { label: "CSV", value: "csv" },
  { label: "JSON", value: "json" },
];

const HINTS: Record<ParseFormat, string> = {
  blank: "每段一条，空行分隔；名称取首行（≤20 字），正文为整段。",
  pipe: "每行一条，按「分类|名称|正文」用竖线分隔；只有两段时按「名称|正文」。",
  csv: "首行为表头，须含 name,text 两列，可选 category；支持引号转义。",
  json: "数组：[{ \"name\": \"…\", \"text\": \"…\", \"category\": \"…\" }]",
};

export function BatchImportModal({
  existingTexts,
  presetCategory,
  onClose,
}: {
  existingTexts: Set<string>;
  presetCategory?: string;
  onClose: () => void;
}) {
  const { dark, accent } = useTheme();
  const toast = useToast();
  const batch = useBatchSaveTemplates();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1B1B22" : C.white;
  const border = dark ? "rgba(255,255,255,0.10)" : C.line;

  const [format, setFormat] = useState(0);
  const [raw, setRaw] = useState("");

  const trapRef = useFocusTrap<HTMLDivElement>();

  // Esc 关闭模态
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const fmt = FORMATS[format].value;
  const rows = useMemo(
    () => parseTemplates(raw, fmt, presetCategory),
    [raw, fmt, presetCategory],
  );

  // 标注每行状态：无效 / 重复（库内或本批内）/ 可导入
  const annotated = useMemo(() => {
    const seen = new Set(existingTexts);
    return rows.map((r) => {
      if (r.error) return { row: r, status: "invalid" as const };
      const key = r.text.trim();
      if (seen.has(key)) return { row: r, status: "dup" as const };
      seen.add(key);
      return { row: r, status: "ok" as const };
    });
  }, [rows, existingTexts]);

  const okRows = annotated.filter((a) => a.status === "ok").map((a) => a.row);
  const dupCount = annotated.filter((a) => a.status === "dup").length;
  const invalidCount = annotated.filter((a) => a.status === "invalid").length;

  const pickFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const sel = await open({
        multiple: false,
        filters: [{ name: "文本/CSV/JSON", extensions: ["txt", "csv", "json", "md"] }],
      });
      if (typeof sel === "string") {
        const content = await readTextFile(sel);
        setRaw(content);
        // 按扩展名切到对应格式；非 csv/json 一律复位为「空行分段」，避免沿用上次格式误判
        const lower = sel.toLowerCase();
        setFormat(lower.endsWith(".csv") ? 2 : lower.endsWith(".json") ? 3 : 0);
      }
    } catch (e) {
      toast.show("读取文件失败：" + errMsg(e), "error");
    }
  };

  const submit = () => {
    if (!okRows.length) return;
    batch.mutate(okRows, {
      onSuccess: (res) => {
        toast.show(`已导入 ${res.inserted} 条${res.skipped ? `，跳过 ${res.skipped} 条` : ""}`, "success");
        onClose();
      },
      onError: (e) => toast.show("导入失败：" + errMsg(e), "error"),
    });
  };

  const taStyle: CSSProperties = {
    width: "100%",
    minHeight: 120,
    padding: "10px 12px",
    borderRadius: 8,
    border: `1px solid ${border}`,
    background: dark ? "rgba(255,255,255,0.04)" : C.paper,
    color: ink,
    fontSize: 12,
    fontFamily: C.mono,
    lineHeight: 1.6,
    outline: "none",
    resize: "vertical",
    boxSizing: "border-box",
    userSelect: "text",
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 50,
        background: "rgba(10,10,14,0.45)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
      }}
    >
      <div
        ref={trapRef}
        role="dialog"
        aria-modal="true"
        aria-label="批量导入查重源"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "min(720px, 100%)",
          maxHeight: "86vh",
          display: "flex",
          flexDirection: "column",
          background: cardBg,
          border: `1px solid ${border}`,
          borderRadius: 14,
          boxShadow: "0 12px 40px rgba(0,0,0,0.3)",
          overflow: "hidden",
        }}
      >
        {/* 头 */}
        <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "14px 18px", borderBottom: `1px solid ${border}` }}>
          <Icon name="upload" size={15} style={{ color: accent }} />
          <span style={{ fontSize: 13.5, fontWeight: 700, color: ink, flex: 1 }}>批量导入查重源</span>
          <Icon name="x" size={15} style={{ color: mute, cursor: "pointer" }} onClick={onClose} />
        </div>

        <div style={{ padding: 18, overflow: "auto", display: "flex", flexDirection: "column", gap: 12 }}>
          {/* 格式 + 选文件 */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div style={{ flex: 1 }}>
              <SegControl options={FORMATS.map((f) => f.label)} value={format} onChange={setFormat} />
            </div>
            <Button kind="secondary" size="sm" icon="file" onClick={pickFile}>
              选择文件
            </Button>
          </div>
          <div style={{ fontSize: 11, color: mute, lineHeight: 1.6 }}>{HINTS[fmt]}</div>

          {/* 来源 */}
          <textarea
            style={taStyle}
            placeholder="在此粘贴内容，或点「选择文件」导入 .txt/.csv/.json…"
            value={raw}
            onChange={(e) => setRaw(e.currentTarget.value)}
          />

          {/* 预览 */}
          {rows.length > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11.5, color: mute, display: "flex", gap: 10 }}>
                <span>共解析 {rows.length} 条</span>
                <span style={{ color: "#0F6E56" }}>可导入 {okRows.length}</span>
                {dupCount > 0 && <span>重复跳过 {dupCount}</span>}
                {invalidCount > 0 && <span style={{ color: "#A32D2D" }}>无效 {invalidCount}</span>}
              </div>
              <div style={{ maxHeight: 240, overflow: "auto", border: `1px solid ${border}`, borderRadius: 8 }}>
                {annotated.map((a, i) => (
                  <PreviewRow key={i} row={a.row} status={a.status} ink={ink} mute={mute} border={border} dark={dark} last={i === annotated.length - 1} />
                ))}
              </div>
            </div>
          )}
        </div>

        {/* 底 */}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "12px 18px", borderTop: `1px solid ${border}` }}>
          <Button kind="ghost" size="md" onClick={onClose}>
            取消
          </Button>
          <Button kind="primary" size="md" icon="check" disabled={!okRows.length || batch.isPending} onClick={submit}>
            {batch.isPending ? "导入中…" : `确认导入 ${okRows.length} 条`}
          </Button>
        </div>
      </div>
    </div>
  );
}

function PreviewRow({
  row,
  status,
  ink,
  mute,
  border,
  dark,
  last,
}: {
  row: ParsedRow;
  status: "ok" | "dup" | "invalid";
  ink: string;
  mute: string;
  border: string;
  dark: boolean;
  last: boolean;
}) {
  const badge =
    status === "ok" ? (
      <Pill fg="#0F6E56" bg="rgba(15,110,86,0.13)" size={9.5}>新增</Pill>
    ) : status === "dup" ? (
      <Pill fg={mute} bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} size={9.5}>已存在·跳过</Pill>
    ) : (
      <Pill fg="#A32D2D" bg="rgba(163,45,45,0.13)" size={9.5}>{row.error ?? "无效"}</Pill>
    );
  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 8,
        padding: "8px 10px",
        borderBottom: last ? "none" : `1px solid ${border}`,
        opacity: status === "ok" ? 1 : 0.6,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          {row.category && (
            <Pill fg={mute} bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} size={9.5}>{row.category}</Pill>
          )}
          <span style={{ fontSize: 12, fontWeight: 600, color: ink, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {row.name || "（无名称）"}
          </span>
        </div>
        <div style={{ fontSize: 11, color: mute, marginTop: 3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {row.text}
        </div>
      </div>
      {badge}
    </div>
  );
}
