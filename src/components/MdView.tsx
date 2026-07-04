// markdown 原文版式视图：marked 渲染 + DOMPurify 消毒。
// 消毒不可省：标书文件来自外部投标方，恶意 md 里的 <script>/onerror 一旦进 webview
// 就能摸到 Tauri IPC——只留排版标签，事件与脚本一律剥掉。
import { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { guardLinkClick } from "../utils/linkGuard";

export function MdView({ data, anchorText }: { data: ArrayBuffer; anchorText: string | null }) {
  const { dark } = useTheme();
  const [html, setHtml] = useState<string>("");
  const hostRef = useRef<HTMLDivElement>(null);

  const text = useMemo(() => decodeText(data), [data]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const raw = await marked.parse(text, { gfm: true, breaks: false });
      if (cancelled) return;
      setHtml(
        DOMPurify.sanitize(raw, {
          ALLOWED_TAGS: [
            "h1", "h2", "h3", "h4", "h5", "h6", "p", "br", "hr", "blockquote", "pre", "code",
            "ul", "ol", "li", "table", "thead", "tbody", "tr", "th", "td",
            "strong", "em", "del", "a", "span",
          ],
          ALLOWED_ATTR: ["href", "title"],
          ALLOW_DATA_ATTR: false,
        }),
      );
    })();
    return () => {
      cancelled = true;
    };
  }, [text]);

  // 渲染完成后按锚文本定位
  useEffect(() => {
    if (!html || !anchorText) return;
    const probe = anchorText.replace(/\s+/g, "").slice(0, 20);
    if (probe.length < 6) return;
    // 用本实例 ref 而非全局 querySelector：同屏多个 MdView 时避免定位串到别的实例
    const host = hostRef.current;
    if (!host) return;
    const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT);
    let node: Node | null = walker.nextNode();
    while (node) {
      if ((node.textContent ?? "").replace(/\s+/g, "").includes(probe)) {
        node.parentElement?.scrollIntoView({ block: "center" });
        return;
      }
      node = walker.nextNode();
    }
  }, [html, anchorText]);

  const ink = dark ? "rgba(255,255,255,0.92)" : C.ink;
  const border = dark ? "rgba(255,255,255,0.12)" : C.line;
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "20px 32px 40px" }} onClickCapture={guardLinkClick}>
      <style>{`
        .bg-md-host{max-width:820px;margin:0 auto;font-size:13.5px;line-height:1.85;color:${ink};font-family:${C.font};}
        .bg-md-host h1,.bg-md-host h2,.bg-md-host h3{font-family:${C.serif};line-height:1.4;}
        .bg-md-host table{border-collapse:collapse;margin:10px 0;width:100%;}
        .bg-md-host th,.bg-md-host td{border:1px solid ${border};padding:5px 10px;font-size:12.5px;text-align:left;}
        .bg-md-host pre{background:${dark ? "rgba(255,255,255,0.05)" : "#F4F1EA"};padding:10px 12px;border-radius:8px;overflow-x:auto;font-size:12px;}
        .bg-md-host code{font-family:${C.mono};font-size:12px;}
        .bg-md-host blockquote{border-left:3px solid ${border};margin:8px 0;padding:2px 14px;opacity:.85;}
        .bg-md-host a{color:var(--accent, #4F58A8);}
      `}</style>
      {/* 已经 DOMPurify 白名单消毒（剥脚本/事件/危险属性） */}
      <div ref={hostRef} className="bg-md-host" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}

/** UTF-8 优先，失败回落 GB18030（与后端 decode_text 行为一致）。 */
export function decodeText(data: ArrayBuffer): string {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(data);
  } catch {
    try {
      return new TextDecoder("gb18030").decode(data);
    } catch {
      return new TextDecoder("utf-8").decode(data); // 宽容解码兜底
    }
  }
}
