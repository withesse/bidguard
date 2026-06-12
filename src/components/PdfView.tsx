// PDF 原文版式视图：pdfjs-dist 渲染页 canvas + 文本层（可选中/拷贝/搜索）。
// 扫描件（parseMethod=ocr）无内嵌文本层，改叠我们导入期存下的 OCR 行级版面
// （归一化坐标的隐形文本），同样可选中拷贝。
import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import type { OcrLine } from "../api/types";

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "pdfjs-dist/build/pdf.worker.min.mjs",
  import.meta.url,
).toString();

// 文本层样式（pdf.js 要求容器带 --scale-factor；span 透明、可选中）
const TEXT_LAYER_CSS = `
.bg-pdf-text-layer{position:absolute;inset:0;overflow:hidden;line-height:1;}
.bg-pdf-text-layer span,.bg-pdf-text-layer br{color:transparent;position:absolute;white-space:pre;cursor:text;transform-origin:0 0;}
.bg-pdf-text-layer span::selection{background:rgba(79,88,168,0.35);}
.bg-ocr-line::selection{background:rgba(79,88,168,0.35);}
`;

export function PdfView({
  data,
  ocrLayout,
  targetPage,
  border,
}: {
  data: ArrayBuffer;
  /** 每页一组 OCR 行（扫描件）；非扫描件传 null。 */
  ocrLayout: OcrLine[][] | null;
  targetPage: number | null;
  border: string;
}) {
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [aspect, setAspect] = useState(1.414); // 首页高宽比，估算行高用
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    // pdf.js 会转移 buffer 所有权，复制一份避免缓存的 ArrayBuffer 被掏空
    const task = pdfjs.getDocument({ data: new Uint8Array(data.slice(0)) });
    task.promise
      .then(async (d) => {
        if (cancelled) return;
        const p1 = await d.getPage(1);
        const vp = p1.getViewport({ scale: 1 });
        if (!cancelled) {
          setAspect(vp.height / vp.width);
          setDoc(d);
        }
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
      // 销毁加载任务（连带释放文档与 worker 资源）
      void task.destroy();
    };
  }, [data]);

  const pageCount = doc?.numPages ?? 0;
  const width = 820; // 页面渲染宽度（容器内居中）
  const virtualizer = useVirtualizer({
    count: pageCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => width * aspect + 16,
    overscan: 2,
  });

  // 数据就绪后跳目标页
  const [jumped, setJumped] = useState(false);
  useEffect(() => {
    if (jumped || !doc || targetPage == null) return;
    const idx = Math.min(Math.max(targetPage - 1, 0), pageCount - 1);
    virtualizer.scrollToIndex(idx, { align: "start" });
    setJumped(true);
  }, [doc, targetPage, jumped, pageCount, virtualizer]);

  if (error) {
    return <div style={{ padding: 24, fontSize: 12.5 }}>PDF 渲染失败：{error}</div>;
  }

  return (
    <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "16px 0 32px" }}>
      <style>{TEXT_LAYER_CSS}</style>
      <div
        style={{
          height: virtualizer.getTotalSize(),
          position: "relative",
          width,
          margin: "0 auto",
          maxWidth: "100%",
        }}
      >
        {doc &&
          virtualizer.getVirtualItems().map((vi) => (
            <div
              key={vi.key}
              data-index={vi.index}
              ref={virtualizer.measureElement}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${vi.start}px)`,
                paddingBottom: 16,
              }}
            >
              <PdfPage
                doc={doc}
                pageNo={vi.index + 1}
                width={width}
                ocrLines={ocrLayout?.[vi.index] ?? null}
                border={border}
              />
            </div>
          ))}
      </div>
    </div>
  );
}

function PdfPage({
  doc,
  pageNo,
  width,
  ocrLines,
  border,
}: {
  doc: PDFDocumentProxy;
  pageNo: number;
  width: number;
  ocrLines: OcrLine[] | null;
  border: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(width * 1.414);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const page = await doc.getPage(pageNo);
        if (cancelled) return;
        const base = page.getViewport({ scale: 1 });
        const scale = width / base.width;
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        const vp = page.getViewport({ scale });
        setHeight(vp.height);

        const canvas = canvasRef.current;
        if (!canvas) return;
        canvas.width = Math.floor(vp.width * dpr);
        canvas.height = Math.floor(vp.height * dpr);
        canvas.style.width = `${vp.width}px`;
        canvas.style.height = `${vp.height}px`;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        await page.render({
          canvas,
          canvasContext: ctx,
          viewport: page.getViewport({ scale: scale * dpr }),
        }).promise;
        if (cancelled) return;

        // 内嵌文本层（扫描件无文本，跳过——由 OCR 层接管）
        const holder = textRef.current;
        if (holder && !ocrLines) {
          holder.innerHTML = "";
          holder.style.setProperty("--scale-factor", String(scale));
          const layer = new pdfjs.TextLayer({
            textContentSource: page.streamTextContent(),
            container: holder,
            viewport: vp,
          });
          await layer.render();
        }
      } catch {
        // 单页渲染失败不拖垮整篇（页留白）
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [doc, pageNo, width, ocrLines]);

  return (
    <div
      data-page={pageNo}
      style={{
        position: "relative",
        width,
        height,
        background: "#fff",
        border: `1px solid ${border}`,
        borderRadius: 4,
        overflow: "hidden",
      }}
    >
      <canvas ref={canvasRef} style={{ display: "block" }} />
      <div ref={textRef} className="bg-pdf-text-layer" />
      {ocrLines && (
        <div style={{ position: "absolute", inset: 0 }}>
          {ocrLines.map((l, i) => (
            <span
              key={i}
              className="bg-ocr-line"
              style={{
                position: "absolute",
                left: `${l.x * 100}%`,
                top: `${l.y * 100}%`,
                width: `${l.w * 100}%`,
                height: `${l.h * 100}%`,
                fontSize: Math.max(l.h * height * 0.78, 8),
                lineHeight: `${l.h * height}px`,
                color: "transparent",
                whiteSpace: "pre",
                overflow: "hidden",
                cursor: "text",
                userSelect: "text",
              }}
            >
              {l.t}
            </span>
          ))}
        </div>
      )}
      <span
        style={{
          position: "absolute",
          right: 8,
          bottom: 6,
          fontSize: 10,
          color: "rgba(0,0,0,0.35)",
          userSelect: "none",
        }}
      >
        {pageNo}
      </span>
    </div>
  );
}
