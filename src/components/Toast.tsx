// 轻量 Toast（替代裸 alert，可诊断、自动消失、可点击关闭）
import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { C } from "../design/tokens";

export type ToastKind = "info" | "success" | "warn" | "error";
interface ToastItem {
  id: number;
  msg: string;
  kind: ToastKind;
}
interface ToastCtx {
  show: (msg: string, kind?: ToastKind) => void;
}

const Ctx = createContext<ToastCtx>({ show: () => {} });
export const useToast = () => useContext(Ctx);

const COLORS: Record<ToastKind, { bg: string; bar: string }> = {
  info: { bg: "#2A2A33", bar: "#6B73C9" },
  success: { bg: "#10403B", bar: "#0E9A8F" },
  warn: { bg: "#43381C", bar: "#C28430" },
  error: { bg: "#43201F", bar: "#B54545" },
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const seq = useRef(0);
  const show = useCallback((msg: string, kind: ToastKind = "info") => {
    const id = (seq.current += 1);
    setToasts((t) => [...t, { id, msg, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);
  return (
    <Ctx.Provider value={{ show }}>
      {children}
      <div
        style={{
          position: "fixed",
          bottom: 22,
          right: 22,
          display: "flex",
          flexDirection: "column",
          gap: 10,
          zIndex: 9999,
          maxWidth: 400,
        }}
      >
        <style>{`@keyframes bg-toast-in{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:none}}`}</style>
        {toasts.map((t) => {
          const c = COLORS[t.kind];
          return (
            <div
              key={t.id}
              onClick={() => setToasts((p) => p.filter((x) => x.id !== t.id))}
              style={{
                display: "flex",
                gap: 10,
                alignItems: "flex-start",
                background: c.bg,
                color: "#fff",
                borderRadius: 10,
                padding: "12px 14px",
                boxShadow: "0 8px 28px rgba(0,0,0,0.28)",
                fontSize: 12.5,
                lineHeight: 1.5,
                cursor: "pointer",
                fontFamily: C.font,
                animation: "bg-toast-in 0.18s ease",
              }}
            >
              <div style={{ width: 3, alignSelf: "stretch", borderRadius: 2, background: c.bar, flexShrink: 0 }} />
              <div>{t.msg}</div>
            </div>
          );
        })}
      </div>
    </Ctx.Provider>
  );
}
