// 检查更新：tauri-plugin-updater。仅在桌面端可用；签名校验由插件用打包内置公钥完成。
import { isTauri } from "../api/client";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "available"; version: string; notes?: string }
  | { kind: "downloading"; pct: number }
  | { kind: "ready" }
  | { kind: "error"; message: string };

/** 检查并（若有）下载安装更新，过程通过 onState 回报。完成后调用方负责重启。 */
export async function runUpdate(onState: (s: UpdateState) => void): Promise<boolean> {
  if (!isTauri()) {
    onState({ kind: "error", message: "检查更新仅在桌面应用内可用" });
    return false;
  }
  try {
    onState({ kind: "checking" });
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      onState({ kind: "none" });
      return false;
    }
    onState({ kind: "available", version: update.version, notes: update.body });

    let total = 0;
    let got = 0;
    await update.downloadAndInstall((ev) => {
      switch (ev.event) {
        case "Started":
          total = ev.data.contentLength ?? 0;
          onState({ kind: "downloading", pct: 0 });
          break;
        case "Progress":
          got += ev.data.chunkLength;
          onState({ kind: "downloading", pct: total > 0 ? got / total : 0 });
          break;
        case "Finished":
          onState({ kind: "ready" });
          break;
      }
    });
    onState({ kind: "ready" });
    return true;
  } catch (e) {
    onState({ kind: "error", message: String(e) });
    return false;
  }
}

/** 安装完成后重启应用以应用更新。 */
export async function relaunchApp(): Promise<void> {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}
