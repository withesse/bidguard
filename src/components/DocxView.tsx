// docx 原文版式视图：docx-preview 渲染为 HTML（表格/图片/样式保真），文本天然可选中拷贝。
import { useEffect, useRef, useState } from "react";
import { renderAsync } from "docx-preview";

export function DocxView({
  data,
  anchorText,
}: {
  data: ArrayBuffer;
  /** 定位锚文本（来自条款分块的开头片段），渲染完成后滚动到首个命中处。 */
  anchorText: string | null;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let cancelled = false;
    host.innerHTML = "";
    renderAsync(data.slice(0), host, undefined, {
      inWrapper: true,
      breakPages: true,
      ignoreLastRenderedPageBreak: false,
    })
      .then(() => {
        if (cancelled || !anchorText) return;
        scrollToText(host, anchorText);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [data, anchorText]);

  if (error) {
    return <div style={{ padding: 24, fontSize: 12.5 }}>docx 渲染失败：{error}</div>;
  }
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "16px 0 32px" }}>
      <div ref={hostRef} style={{ margin: "0 auto", maxWidth: "100%", width: "fit-content" }} />
    </div>
  );
}

/** 在渲染结果里找锚文本首个命中节点并滚动+短暂高亮（找不到则静默）。 */
function scrollToText(host: HTMLElement, anchor: string) {
  const probe = anchor.replace(/\s+/g, "").slice(0, 24);
  if (probe.length < 6) return;
  const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT);
  let node: Node | null = walker.nextNode();
  while (node) {
    const t = (node.textContent ?? "").replace(/\s+/g, "");
    if (t.includes(probe.slice(0, Math.min(probe.length, Math.max(t.length, 6))))) {
      const el = node.parentElement;
      if (el) {
        el.scrollIntoView({ block: "center" });
        const old = el.style.background;
        el.style.background = "rgba(79,88,168,0.18)";
        setTimeout(() => {
          el.style.background = old;
        }, 1800);
      }
      return;
    }
    node = walker.nextNode();
  }
}
