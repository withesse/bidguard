// 全局渲染错误边界：捕获子树异常，避免整屏白屏
import { Component, type ErrorInfo, type ReactNode } from "react";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("BidGuard 渲染异常:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (error) {
      return (
        <div
          style={{
            height: "100vh",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 16,
            background: "#15151B",
            color: "#fff",
            fontFamily:
              '-apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", sans-serif',
            padding: 40,
            textAlign: "center",
          }}
        >
          <div style={{ fontSize: 18, fontWeight: 700 }}>界面发生异常</div>
          <div style={{ fontSize: 13, color: "rgba(255,255,255,0.6)", maxWidth: 560, lineHeight: 1.6 }}>
            {error.message || "未知错误"}
          </div>
          <button
            type="button"
            onClick={() => {
              this.setState({ error: null });
              location.reload();
            }}
            style={{
              marginTop: 8,
              height: 38,
              padding: "0 20px",
              background: "#6B73C9",
              color: "#fff",
              border: "none",
              borderRadius: 8,
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            重新加载
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
