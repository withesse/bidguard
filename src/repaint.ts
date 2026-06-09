// 规避 macOS WKWebView 偶发的"回到前台不重绘"（被屏保 / 其它窗口抢焦点后留白）。
// 在窗口重新获得焦点时强制一次同步重排，逼 webview 刷新缓冲。focus 事件很少，开销可忽略。
function forceRepaint() {
  const root = document.getElementById("root");
  if (!root) return;
  const prev = root.style.display;
  root.style.display = "none";
  void root.offsetHeight; // 读取触发同步 reflow（同一任务内完成，无可见闪烁）
  root.style.display = prev;
}

export function installRepaintFix() {
  window.addEventListener("focus", forceRepaint);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") forceRepaint();
  });
  // Tauri 原生窗口焦点变化（比 DOM focus 更可靠地覆盖"从别的 app 切回"）
  if ("__TAURI_INTERNALS__" in window) {
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onFocusChanged(({ payload }) => {
          if (payload) forceRepaint();
        }),
      )
      .catch(() => {});
  }
}
