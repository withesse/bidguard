// 侧栏 —— 移植自 app-design/project/src/c/bid-a.jsx (BidSidebar)
// 任务导向，个人版（无团队/工作区）。
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { useTheme } from "../theme";
import type { Screen } from "../routes";

export type NavKey = "home" | "tasks" | "history" | "library" | "settings";

export function Sidebar({ active, onNav }: { active: NavKey; onNav: (s: Screen) => void }) {
  const { dark, layout } = useTheme();
  const compact = layout === "compact";
  const bg = dark ? "#15151B" : C.paper2;
  const border = dark ? "rgba(255,255,255,0.06)" : C.line;
  const inkMute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const inkStrong = dark ? "#fff" : C.ink;

  const nav = (k: NavKey, label: string, icon: string) => (
    <NavRow
      key={k}
      active={active === k}
      label={compact ? "" : label}
      icon={icon}
      compact={compact}
      onClick={() => onNav(k)}
    />
  );

  return (
    <div
      style={{
        width: compact ? 60 : 216,
        flexShrink: 0,
        background: bg,
        borderRight: `1px solid ${border}`,
        display: "flex",
        flexDirection: "column",
        color: inkStrong,
      }}
    >
      {/* 品牌 */}
      <div
        style={{
          padding: compact ? "16px 0 14px" : "18px 16px 14px",
          display: "flex",
          alignItems: "center",
          gap: 10,
          justifyContent: compact ? "center" : "flex-start",
        }}
      >
        <img
          src="/icon.png"
          alt="原本"
          width={36}
          height={36}
          style={{ borderRadius: 9, display: "block", flexShrink: 0 }}
        />
        {!compact && (
          <div style={{ lineHeight: 1.1 }}>
            <div
              style={{
                fontSize: 15,
                fontWeight: 700,
                color: inkStrong,
                letterSpacing: "0.02em",
                fontFamily: C.serif,
              }}
            >
              原本
            </div>
            <div style={{ fontSize: 10, color: inkMute, marginTop: 1, letterSpacing: "0.14em" }}>
              标书查重
            </div>
          </div>
        )}
      </div>

      <div style={{ padding: "8px 8px 0", display: "flex", flexDirection: "column", gap: 1 }}>
        {nav("home", "首页", "home")}
        {nav("tasks", "我的任务", "folder")}
        {nav("history", "历史记录", "history")}
      </div>
      <SidebarLabel collapsed={compact}>工具</SidebarLabel>
      <div style={{ padding: "0 8px", display: "flex", flexDirection: "column", gap: 1 }}>
        {nav("library", "查重源", "book")}
      </div>

      <div style={{ flex: 1 }} />

      {/* 底部：设置 */}
      <div style={{ padding: "8px 8px 10px" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          {nav("settings", "设置", "cog")}
        </div>
      </div>
    </div>
  );
}

function NavRow({
  active,
  label,
  icon,
  compact,
  onClick,
}: {
  active: boolean;
  label: string;
  icon: string;
  compact: boolean;
  onClick: () => void;
}) {
  const { dark, accent } = useTheme();
  const activeBg = dark ? "rgba(255,255,255,0.06)" : C.white;
  return (
    <div
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 9,
        height: 30,
        padding: compact ? 0 : "0 10px",
        borderRadius: 7,
        justifyContent: compact ? "center" : "flex-start",
        background: active ? activeBg : "transparent",
        boxShadow: active && !dark ? `0 1px 0 ${C.line}` : "none",
        color: active ? (dark ? "#fff" : C.ink) : dark ? "rgba(255,255,255,0.7)" : C.ink2,
        cursor: "pointer",
        position: "relative",
      }}
    >
      <Icon
        name={icon}
        size={14}
        style={{ color: active ? accent : dark ? "rgba(255,255,255,0.55)" : C.ink3 }}
      />
      {label && <span style={{ flex: 1, fontSize: 12.5, fontWeight: active ? 600 : 500 }}>{label}</span>}
    </div>
  );
}

function SidebarLabel({ children, collapsed }: { children: React.ReactNode; collapsed: boolean }) {
  const { dark } = useTheme();
  if (collapsed) return <div style={{ height: 10 }} />;
  return (
    <div
      style={{
        padding: "14px 16px 4px",
        fontSize: 9.5,
        fontWeight: 600,
        letterSpacing: "0.07em",
        textTransform: "uppercase",
        color: dark ? "rgba(255,255,255,0.38)" : C.ink4,
      }}
    >
      {children}
    </div>
  );
}
