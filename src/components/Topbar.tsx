// 顶栏 —— 移植自 app-design/project/src/c/shell.jsx (CTopbar)
import type { ReactNode } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { useTheme } from "../theme";

export function Topbar({
  title,
  sub,
  actions,
  search,
}: {
  title: string;
  sub?: string;
  actions?: ReactNode;
  search?: { value: string; onChange: (v: string) => void; placeholder?: string };
}) {
  const { dark } = useTheme();
  return (
    <div
      style={{
        height: 60,
        flexShrink: 0,
        padding: "0 24px",
        display: "flex",
        alignItems: "center",
        gap: 14,
        borderBottom: `1px solid ${dark ? "rgba(255,255,255,0.06)" : C.line}`,
        background: dark ? "#1A1A20" : C.paper,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: 14,
            fontWeight: 700,
            color: dark ? "#fff" : C.ink,
            letterSpacing: "-0.01em",
            lineHeight: 1.2,
          }}
        >
          {title}
        </div>
        {sub && (
          <div
            style={{
              fontSize: 11.5,
              color: dark ? "rgba(255,255,255,0.55)" : C.ink3,
              marginTop: 2,
            }}
          >
            {sub}
          </div>
        )}
      </div>
      {/* 搜索（仅在提供 search 时渲染真实输入） */}
      {search && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            padding: "0 11px",
            height: 30,
            borderRadius: 7,
            background: dark ? "rgba(255,255,255,0.05)" : C.white,
            border: `1px solid ${dark ? "rgba(255,255,255,0.05)" : C.line}`,
            width: 240,
            color: dark ? "rgba(255,255,255,0.5)" : C.ink3,
          }}
        >
          <Icon name="search" size={12} />
          <input
            value={search.value}
            onChange={(e) => search.onChange(e.target.value)}
            placeholder={search.placeholder ?? "搜索"}
            style={{
              flex: 1,
              minWidth: 0,
              border: "none",
              outline: "none",
              background: "transparent",
              fontSize: 11.5,
              color: dark ? "#fff" : C.ink,
              fontFamily: C.font,
            }}
          />
        </div>
      )}
      {actions}
    </div>
  );
}
