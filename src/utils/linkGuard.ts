// 原文版式/Markdown 预览的链接导航防护：标书来自外部投标方（不可信），其内嵌链接若被点击，
// WebView 会就地导航到外部 URL——离线环境白屏需重启，联网环境是可信窗口内的钓鱼面。
// 容器级捕获 <a> 点击：一律 preventDefault，http(s) 交系统浏览器外部打开，其余 scheme 丢弃。
import type { MouseEvent } from "react";
import { isTauri } from "../api/client";

export function guardLinkClick(e: MouseEvent): void {
  const target = e.target as HTMLElement | null;
  const a = target?.closest?.("a[href]") as HTMLAnchorElement | null;
  if (!a) return;
  e.preventDefault();
  e.stopPropagation();
  const href = a.getAttribute("href") ?? "";
  if (!/^https?:\/\//i.test(href)) return; // javascript:/file:/data: 等一律丢弃
  if (isTauri()) {
    void import("@tauri-apps/plugin-opener")
      .then(({ openUrl }) => openUrl(href))
      .catch(() => {});
  } else {
    window.open(href, "_blank", "noopener,noreferrer");
  }
}
