// 全局渲染错误边界：捕获子树异常，避免整屏白屏。
// 位于 ThemeProvider 之外，故通过 html.dark 类（theme.tsx 切换）判断暗色，
// 用 var(--accent) 跟随主色；并提供可复制的错误详情便于排障。
import { Component, type ErrorInfo, type ReactNode } from "react";

interface State {
  error: Error | null;
  stack: string;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null, stack: "" };

  static getDerivedStateFromError(error: Error): State {
    return { error, stack: "" };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("BidGuard 渲染异常:", error, info.componentStack);
    this.setState({ stack: info.componentStack ?? "" });
  }

  render() {
    const { error, stack } = this.state;
    if (!error) return this.props.children;

    const dark =
      typeof document !== "undefined" && document.documentElement.classList.contains("dark");
    const bg = dark ? "#15151B" : "#FAFAF7";
    const fg = dark ? "#fff" : "#16161B";
    const mute = dark ? "rgba(255,255,255,0.6)" : "#6B6B76";
    const cardBg = dark ? "rgba(255,255,255,0.05)" : "#FFFFFF";
    const border = dark ? "rgba(255,255,255,0.10)" : "#ECEAE3";
    const detail = `${error.message}\n\n${error.stack ?? ""}\n\n${stack}`.trim();

    return (
      <div
        style={{
          height: "100vh",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 14,
          background: bg,
          color: fg,
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", sans-serif',
          padding: 40,
          textAlign: "center",
        }}
      >
        <div style={{ fontSize: 18, fontWeight: 700 }}>界面发生异常</div>
        <div style={{ fontSize: 13, color: mute, maxWidth: 560, lineHeight: 1.6 }}>
          {error.message || "未知错误"}
        </div>
        {detail && (
          <details style={{ maxWidth: 620, width: "100%", textAlign: "left" }}>
            <summary style={{ fontSize: 12, color: mute, cursor: "pointer" }}>错误详情</summary>
            <pre
              style={{
                marginTop: 8,
                maxHeight: 220,
                overflow: "auto",
                background: cardBg,
                border: `1px solid ${border}`,
                borderRadius: 8,
                padding: "10px 12px",
                fontSize: 11,
                lineHeight: 1.5,
                color: mute,
                whiteSpace: "pre-wrap",
                userSelect: "text",
              }}
            >
              {detail}
            </pre>
          </details>
        )}
        <div style={{ display: "flex", gap: 10, marginTop: 4 }}>
          <button
            type="button"
            onClick={() => void navigator.clipboard?.writeText(detail).catch(() => {})}
            style={{
              height: 36,
              padding: "0 16px",
              background: "transparent",
              color: fg,
              border: `1px solid ${border}`,
              borderRadius: 8,
              fontSize: 12.5,
              fontWeight: 600,
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            复制错误
          </button>
          <button
            type="button"
            onClick={() => {
              this.setState({ error: null, stack: "" });
              location.reload();
            }}
            style={{
              height: 36,
              padding: "0 20px",
              background: "var(--accent, #4F58A8)",
              color: "#fff",
              border: "none",
              borderRadius: 8,
              fontSize: 12.5,
              fontWeight: 600,
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            重新加载
          </button>
        </div>
      </div>
    );
  }
}
