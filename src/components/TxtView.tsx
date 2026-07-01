// 纯文本原文视图：原样全文（保留原始换行与空白——分块视图会滤掉空行与短行）。
// 支持锚文本定位：把命中片段包成 <mark> 滚动到中部并短暂高亮，与 md/docx 四格式一致。
import { useEffect, useMemo, useRef } from "react";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { decodeText } from "./MdView";

export function TxtView({ data, anchorText }: { data: ArrayBuffer; anchorText?: string | null }) {
  const { dark } = useTheme();
  const text = useMemo(() => decodeText(data), [data]);
  const markRef = useRef<HTMLElement>(null);

  // 在原文里定位锚文本前缀（txt 的锚文本即原文片段，可直接 indexOf）
  const parts = useMemo(() => {
    const needle = anchorText?.trim().slice(0, 40) ?? "";
    if (needle.length < 6) return null;
    const idx = text.indexOf(needle);
    if (idx < 0) return null;
    return {
      before: text.slice(0, idx),
      match: text.slice(idx, idx + needle.length),
      after: text.slice(idx + needle.length),
    };
  }, [text, anchorText]);

  useEffect(() => {
    if (parts && markRef.current) markRef.current.scrollIntoView({ block: "center" });
  }, [parts]);

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "20px 32px 40px" }}>
      <pre
        style={{
          maxWidth: 860,
          margin: "0 auto",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontSize: 12.5,
          lineHeight: 1.8,
          color: dark ? "rgba(255,255,255,0.92)" : C.ink,
          fontFamily: C.font,
        }}
      >
        {parts ? (
          <>
            {parts.before}
            <mark ref={markRef} style={{ background: "rgba(79,88,168,0.18)", color: "inherit" }}>
              {parts.match}
            </mark>
            {parts.after}
          </>
        ) : (
          text
        )}
      </pre>
    </div>
  );
}
