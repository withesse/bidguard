// 任务进度全局态：应用启动时订阅一次 Tauri 全局事件，任意路由都能读到实时进度。
// 深链接/刷新后由 useJob 轮询兜底（事件只在任务运行期间出现）。
import { create } from "zustand";
import type { QueryClient } from "@tanstack/react-query";
import { isTauri } from "../api/client";
import type { ProgressEvent, TerminalEvent } from "../api/types";

interface ProgressState {
  progress: Record<string, ProgressEvent>;
  terminal: Record<string, TerminalEvent>;
  onProgress: (p: ProgressEvent) => void;
  onTerminal: (t: TerminalEvent) => void;
}

export const useProgressStore = create<ProgressState>((set) => ({
  progress: {},
  terminal: {},
  onProgress: (p) =>
    set((s) => ({ progress: { ...s.progress, [p.jobId]: p } })),
  onTerminal: (t) =>
    set((s) => ({ terminal: { ...s.terminal, [t.jobId]: t } })),
}));

// 注：导出是同步 command（前端 await 结果），不经 JobManager，不发 export:* 事件
const PROGRESS_EVENTS = ["document:import:progress", "compare:progress"];
const TERMINAL_EVENTS = [
  "document:import:completed",
  "document:import:failed",
  "document:import:cancelled",
  "compare:completed",
  "compare:failed",
  "compare:cancelled",
];

/** 订阅全部任务事件；终态时让相关查询失效以刷新列表/详情。返回解绑函数。 */
export async function initJobEvents(queryClient: QueryClient): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const offs: Array<() => void> = [];
  for (const name of PROGRESS_EVENTS) {
    offs.push(
      await listen<ProgressEvent>(name, (e) => useProgressStore.getState().onProgress(e.payload)),
    );
  }
  for (const name of TERMINAL_EVENTS) {
    offs.push(
      await listen<TerminalEvent>(name, (e) => {
        useProgressStore.getState().onTerminal(e.payload);
        // 本地 SQLite 查询很便宜：终态后整体失效，保证各屏一致
        void queryClient.invalidateQueries();
      }),
    );
  }
  return () => offs.forEach((off) => off());
}
