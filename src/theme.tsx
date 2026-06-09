// 主题上下文：外观模式 / 品牌色 / 高亮方案 / 侧栏 / 字号。持久化到 localStorage。
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Mode = "light" | "dark" | "system";
export type Highlight = "amber" | "rose" | "blue";
export type Layout = "comfort" | "compact";
export type FontScale = "compact" | "regular" | "comfy" | "spacious";

export interface Theme {
  mode: Mode;
  dark: boolean; // 由 mode 推导，组件直接读这个
  accent: string;
  highlight: Highlight;
  layout: Layout;
  fontScale: FontScale;
  reduceMotion: boolean;
}

const DEFAULT: Theme = {
  mode: "light",
  dark: false,
  accent: "#4F58A8",
  highlight: "amber",
  layout: "comfort",
  fontScale: "regular",
  reduceMotion: false,
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

// 界面字号 = webview 缩放（等比放大字体+界面，不撑破布局）
const ZOOM: Record<FontScale, number> = { compact: 0.9, regular: 1.0, comfy: 1.15, spacious: 1.35 };
function applyZoom(scale: FontScale) {
  const z = ZOOM[scale] ?? 1;
  if (typeof window === "undefined") return;
  if ("__TAURI_INTERNALS__" in window) {
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(z))
      .catch(() => {});
  } else {
    (document.documentElement.style as unknown as { zoom: string }).zoom = String(z);
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [t, setT] = useState<Theme>(loadTheme);
  useEffect(() => {
    applyZoom(t.fontScale);
  }, [t.fontScale]);
  useEffect(() => {
    if (typeof document !== "undefined")
      document.documentElement.classList.toggle("reduce-motion", t.reduceMotion);
  }, [t.reduceMotion]);
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
