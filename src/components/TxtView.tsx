// 纯文本原文视图：原样全文（保留原始换行与空白——分块视图会滤掉空行与短行）。
import { useMemo } from "react";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { decodeText } from "./MdView";

export function TxtView({ data }: { data: ArrayBuffer }) {
  const { dark } = useTheme();
  const text = useMemo(() => decodeText(data), [data]);
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
        {text}
      </pre>
    </div>
  );
}
