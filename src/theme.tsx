// 主题上下文：外观模式 / 品牌色 / 高亮方案 / 侧栏 / 字号。持久化到 localStorage。
import { createContext, useContext, useState, type ReactNode } from "react";

export type Mode = "light" | "dark" | "system";
export type Highlight = "amber" | "rose" | "blue";
export type Layout = "comfort" | "compact";
export type FontScale = "compact" | "regular" | "comfy";

export interface Theme {
  mode: Mode;
  dark: boolean; // 由 mode 推导，组件直接读这个
  accent: string;
  highlight: Highlight;
  layout: Layout;
  fontScale: FontScale;
}

const DEFAULT: Theme = {
  mode: "light",
  dark: false,
  accent: "#4F58A8",
  highlight: "amber",
  layout: "comfort",
  fontScale: "regular",
};

const STORAGE_KEY = "bidguard-theme";

function resolveDark(mode: Mode): boolean {
  if (mode === "dark") return true;
  if (mode === "light") return false;
  return typeof window !== "undefined" && window.matchMedia
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

function loadTheme(): Theme {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    if (raw) {
      const saved = JSON.parse(raw) as Partial<Theme>;
      const merged = { ...DEFAULT, ...saved };
      merged.dark = resolveDark(merged.mode);
      return merged;
    }
  } catch {
    // localStorage 不可用（隐私模式等）→ 回落默认，不阻塞
  }
  return DEFAULT;
}

function saveTheme(t: Theme): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(t));
  } catch {
    // 存储不可用时静默忽略：主题仅在本次会话内有效
  }
}

interface ThemeCtx extends Theme {
  set: (patch: Partial<Theme>) => void;
}

const ThemeContext = createContext<ThemeCtx>({ ...DEFAULT, set: () => {} });

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [t, setT] = useState<Theme>(loadTheme);
  const set = (patch: Partial<Theme>) =>
    setT((prev) => {
      const next = { ...prev, ...patch };
      if (patch.mode !== undefined) next.dark = resolveDark(patch.mode);
      saveTheme(next);
      return next;
    });
  return <ThemeContext.Provider value={{ ...t, set }}>{children}</ThemeContext.Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components
export const useTheme = () => useContext(ThemeContext);
