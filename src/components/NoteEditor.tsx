// 行内批注编辑器：textarea + 保存/取消（新增与编辑共用）。
import { useState } from "react";
import { Button } from "./primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";

export function NoteEditor({
  initial,
  placeholder,
  onSave,
  onCancel,
}: {
  initial?: string;
  placeholder?: string;
  onSave: (note: string) => void;
  onCancel: () => void;
}) {
  const { dark } = useTheme();
  const [text, setText] = useState(initial ?? "");
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <textarea
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={placeholder ?? "写下批注…"}
        rows={3}
        style={{
          width: "100%",
          boxSizing: "border-box",
          resize: "vertical",
          padding: "8px 10px",
          fontSize: 12.5,
          lineHeight: 1.6,
          fontFamily: C.font,
          color: dark ? "#fff" : C.ink,
          background: dark ? "rgba(255,255,255,0.05)" : C.white,
          border: `1px solid ${dark ? "rgba(255,255,255,0.12)" : C.line}`,
          borderRadius: 7,
          outline: "none",
        }}
      />
      <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
        <Button kind="ghost" size="sm" onClick={onCancel}>
          取消
        </Button>
        <Button kind="primary" size="sm" disabled={!text.trim()} onClick={() => onSave(text.trim())}>
          保存批注
        </Button>
      </div>
    </div>
  );
}
