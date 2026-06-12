import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { router } from "./app/router";
import { ThemeProvider } from "./theme";
import { ToastProvider } from "./components/Toast";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { installRepaintFix } from "./repaint";
import { initJobEvents } from "./stores/progressStore";
import { cleanupOldJobs, getAppSettings, setAppSettings } from "./api";
import { isTauri } from "./api/client";
import { getSettings } from "./prefs";
import "./index.css";

installRepaintFix();

const queryClient = new QueryClient({
  defaultOptions: {
    // networkMode "always"：invoke 走本地 IPC 不经网络，离线机器（本产品的
    // 目标场景）上绝不能让查询因 navigator.onLine=false 被挂起。
    // retry false：本地命令的失败是确定性的（参数/状态问题），重试只会拖慢报错。
    queries: { retry: false, refetchOnWindowFocus: false, networkMode: "always" },
    mutations: { networkMode: "always" },
  },
});

/** 旧版 localStorage 检测偏好 → DB 用户全局配置（仅当 DB 还没有配置时迁移一次）。 */
async function migrateLegacyPrefs() {
  if (!isTauri()) return;
  try {
    const existing = await getAppSettings();
    if (existing) return; // DB 已有配置，不覆盖
    const raw = localStorage.getItem("bidguard-settings");
    if (!raw) return;
    const old = JSON.parse(raw) as Record<string, unknown>;
    await setAppSettings({
      compare: {
        ...(typeof old.scope === "string" ? { scope: old.scope } : {}),
        ...(typeof old.threshold === "number" ? { similarityThreshold: old.threshold } : {}),
        ...(typeof old.ignoreTemplates === "boolean"
          ? { ignoreTemplates: old.ignoreTemplates }
          : {}),
        ...(typeof old.semantic === "boolean" ? { enableSemantic: old.semantic } : {}),
      },
    });
  } catch {
    // 迁移失败不阻塞启动：用户可在设置页重设
  }
}

/** 设置页「自动清理」开启时，启动清一次 30 天前的已完结任务（收藏的保留）。 */
async function autoCleanOldJobs() {
  if (!isTauri() || !getSettings().autoClean) return;
  try {
    const n = await cleanupOldJobs(30);
    if (n > 0) void queryClient.invalidateQueries({ queryKey: ["jobs"] });
  } catch {
    // 清理失败不阻塞启动；下次启动再试
  }
}

/** 启动时订阅一次任务全局事件（StrictMode 双挂载下保证不泄漏订阅）。 */
function EventsBridge() {
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void migrateLegacyPrefs();
    void autoCleanOldJobs();
    void initJobEvents(queryClient).then((f) => {
      if (cancelled) f();
      else off = f;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ThemeProvider>
        <ToastProvider>
          <QueryClientProvider client={queryClient}>
            <EventsBridge />
            <RouterProvider router={router} />
          </QueryClientProvider>
        </ToastProvider>
      </ThemeProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
