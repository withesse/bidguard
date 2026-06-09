// 原子组件 —— 移植自 app-design/project/src/c/shell.jsx
import type { CSSProperties, ReactNode } from "react";
import { C, docChip, shadeC } from "../design/tokens";
import { Icon } from "../design/Icon";
import { useTheme } from "../theme";

// ── Pill ───────────────────────────────────────────────
export function Pill({
  children,
  fg = C.ink2,
  bg = C.paper2,
  size = 11,
  weight = 500,
  style,
}: {
  children: ReactNode;
  fg?: string;
  bg?: string;
  size?: number;
  weight?: number;
  style?: CSSProperties;
}) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 8px",
        borderRadius: 999,
        fontSize: size,
        fontWeight: weight,
        color: fg,
        background: bg,
        lineHeight: 1.4,
        ...style,
      }}
    >
      {children}
    </span>
  );
}

// ── DocChip ────────────────────────────────────────────
export function DocChip({ type = "docx", style }: { type?: string; style?: CSSProperties }) {
  const t = docChip(type);
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "1.5px 5px",
        borderRadius: 3,
        fontSize: 9.5,
        fontWeight: 700,
        letterSpacing: "0.04em",
        color: t.fg,
        background: t.bg,
        fontFamily: C.mono,
        ...style,
      }}
    >
      {t.label}
    </span>
  );
}

// ── Avatar ─────────────────────────────────────────────
export function Avatar({
  name,
  color = C.brand,
  size = 24,
}: {
  name: string;
  color?: string;
  size?: number;
}) {
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: color,
        color: "#fff",
        fontSize: size * 0.44,
        fontWeight: 700,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        letterSpacing: "-0.01em",
        fontFamily: C.font,
      }}
    >
      {name}
    </div>
  );
}

// ── Button ─────────────────────────────────────────────
type ButtonKind = "primary" | "secondary" | "ghost";
type ButtonSize = "sm" | "md" | "lg";

export function Button({
  kind = "primary",
  size = "md",
  icon,
  iconRight,
  children,
  accent: accentProp,
  dark: darkProp,
  style,
  onClick,
}: {
  kind?: ButtonKind;
  size?: ButtonSize;
  icon?: string;
  iconRight?: string;
  children?: ReactNode;
  accent?: string;
  dark?: boolean;
  style?: CSSProperties;
  onClick?: () => void;
}) {
  const th = useTheme();
  const accent = accentProp ?? th.accent;
  const dark = darkProp ?? th.dark;

  const sizes = {
    sm: { h: 26, px: 10, fs: 11.5, gap: 5 },
    md: { h: 32, px: 13, fs: 12.5, gap: 6 },
    lg: { h: 40, px: 18, fs: 13.5, gap: 8 },
  }[size];

  const kinds: Record<ButtonKind, { bg: string; color: string; border: string; shadow: string }> = {
    primary: {
      bg: accent,
      color: "#fff",
      border: "transparent",
      shadow: `0 1px 0 ${shadeC(accent, -22)} inset, 0 1px 2px rgba(20,18,14,0.08)`,
    },
    secondary: {
      bg: dark ? "rgba(255,255,255,0.05)" : C.white,
      color: dark ? "#fff" : C.ink,
      border: dark ? "rgba(255,255,255,0.10)" : C.line,
      shadow: dark ? "none" : "0 1px 2px rgba(20,18,14,0.04)",
    },
    ghost: {
      bg: "transparent",
      color: dark ? "rgba(255,255,255,0.8)" : C.ink2,
      border: "transparent",
      shadow: "none",
    },
  };
  const k = kinds[kind];

  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        height: sizes.h,
        padding: `0 ${sizes.px}px`,
        background: k.bg,
        color: k.color,
        border: k.border === "transparent" ? "none" : `1px solid ${k.border}`,
        borderRadius: 7,
        fontSize: sizes.fs,
        fontWeight: 600,
        display: "inline-flex",
        alignItems: "center",
        gap: sizes.gap,
        cursor: "pointer",
        boxShadow: k.shadow,
        fontFamily: C.font,
        letterSpacing: "-0.005em",
        ...style,
      }}
    >
      {icon && <Icon name={icon} size={sizes.fs + 1} />}
      {children}
      {iconRight && <Icon name={iconRight} size={sizes.fs + 1} />}
    </button>
  );
}

// ── Toggle ─────────────────────────────────────────────
export function Toggle({
  on,
  accent: accentProp,
  onChange,
}: {
  on: boolean;
  accent?: string;
  onChange?: () => void;
}) {
  const th = useTheme();
  const accent = accentProp ?? th.accent;
  return (
    <div
      onClick={onChange}
      style={{
        width: 28,
        height: 16,
        borderRadius: 999,
        background: on ? accent : "#C9C5CF",
        position: "relative",
        cursor: "pointer",
        transition: "background 0.15s",
        flexShrink: 0,
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 2,
          left: on ? 14 : 2,
          width: 12,
          height: 12,
          borderRadius: "50%",
          background: "#fff",
          boxShadow: "0 1px 2px rgba(0,0,0,0.2)",
          transition: "left 0.15s",
        }}
      />
    </div>
  );
}

// ── Segmented control ──────────────────────────────────
export function SegControl({
  options,
  value,
  onChange,
  dark: darkProp,
}: {
  options: string[];
  value: number;
  onChange?: (i: number) => void;
  dark?: boolean;
}) {
  const th = useTheme();
  const dark = darkProp ?? th.dark;
  return (
    <div
      style={{
        display: "flex",
        padding: 2,
        background: dark ? "rgba(255,255,255,0.05)" : C.paper2,
        border: `1px solid ${dark ? "rgba(255,255,255,0.08)" : C.line}`,
        borderRadius: 7,
      }}
    >
      {options.map((o, i) => (
        <div
          key={i}
          onClick={() => onChange?.(i)}
          style={{
            flex: 1,
            textAlign: "center",
            padding: "5px 12px",
            fontSize: 11.5,
            fontWeight: 600,
            background: i === value ? (dark ? "#2A2A33" : "#fff") : "transparent",
            color: i === value ? (dark ? "#fff" : C.ink) : dark ? "rgba(255,255,255,0.55)" : C.ink3,
            borderRadius: 5,
            cursor: "pointer",
            boxShadow: i === value && !dark ? "0 1px 2px rgba(0,0,0,0.06)" : "none",
          }}
        >
          {o}
        </div>
      ))}
    </div>
  );
}
