// 屏 8 · 设置 —— 检测偏好 / 隐私 / 外观，全部接入本地持久化
import { useState, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Logo } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Toggle, SegControl } from "../components/primitives";
import { useTheme, type Mode } from "../theme";
import { useToast } from "../components/Toast";
import { getSettings, setSettings, type Settings as DetectSettings } from "../prefs";

const THEME_SWATCHES = ["#4F58A8", "#2E5BFF", "#0E9A8F", "#C84D2E", "#2B2D33"];

const VARIANTS: { key: string; label: string; accent: string; mode: Mode }[] = [
  { key: "A", label: "A · 素雅靛蓝", accent: "#4F58A8", mode: "light" },
  { key: "B", label: "B · 暗色青绿", accent: "#0E9A8F", mode: "dark" },
  { key: "C", label: "C · 暖纸红土", accent: "#C84D2E", mode: "light" },
];

export function Settings() {
  const { dark, accent, mode, set } = useTheme();
  const toast = useToast();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const modeIndex = mode === "light" ? 0 : mode === "dark" ? 1 : 2;

  const [s, setS] = useState<DetectSettings>(() => getSettings());
  const change = (patch: Partial<DetectSettings>) => setS(setSettings(patch));
  const pct = Math.round(s.threshold * 100);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar title="设置" sub="检测偏好与外观" />
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 720, margin: "0 auto", display: "flex", flexDirection: "column", gap: 20 }}>
          {/* 检测偏好 */}
          <Card title="检测偏好（新任务默认值）" cardBg={cardBg} border={border} mute={mute}>
            <Row label="默认相似度阈值" sub="新任务的初始阈值，可在任务页临时调整" ink={ink} mute={mute}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <input
                  type="range"
                  min={10}
                  max={95}
                  step={5}
                  value={pct}
                  onChange={(e) => change({ threshold: Number(e.target.value) / 100 })}
                  style={{ width: 120, accentColor: accent, cursor: "pointer" }}
                />
                <span style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 32 }}>
                  {pct}%
                </span>
              </div>
            </Row>
            <Row label="忽略通用模板段落" sub="标准条款、表头、附件目录（查重源库）" ink={ink} mute={mute}>
              <Toggle on={s.ignoreTemplates} onChange={() => change({ ignoreTemplates: !s.ignoreTemplates })} />
            </Row>
            <Row label="围标嫌疑提示" sub="3 份及以上共同高相似片段触发" ink={ink} mute={mute}>
              <Toggle on={s.flagCollusion} onChange={() => change({ flagCollusion: !s.flagCollusion })} />
            </Row>
            <Row label="语义查重" sub="叠加 AI 语义相似度，捕捉改写式雷同（首次需下载模型 ~120MB）" ink={ink} mute={mute}>
              <Toggle on={s.semantic} onChange={() => change({ semantic: !s.semantic })} />
            </Row>
            <Row
              label="联动工商关联数据"
              sub="需接入工商数据源（如企业信息 API），未配置时不参与判定"
              ink={ink}
              mute={mute}
              last
            >
              <Toggle on={s.industryLink} onChange={() => change({ industryLink: !s.industryLink })} />
            </Row>
          </Card>

          {/* 隐私 */}
          <Card title="隐私" cardBg={cardBg} border={border} mute={mute}>
            <Row label="本地优先模式" sub="标书不上传至服务器（本应用始终本地处理）" ink={ink} mute={mute}>
              <Toggle on onChange={() => toast.show("本应用全程本地处理，无需关闭", "info")} />
            </Row>
            <Row label="自动清理 30 天前的任务" sub="清理历史报告，释放空间" ink={ink} mute={mute} last>
              <Toggle on={s.autoClean} onChange={() => change({ autoClean: !s.autoClean })} />
            </Row>
          </Card>

          {/* 外观 —— 驱动主题 */}
          <Card title="外观" cardBg={cardBg} border={border} mute={mute}>
            <Row label="视觉变体" sub="一键切换设计稿三套方向（A / B / C）" ink={ink} mute={mute}>
              <div style={{ display: "flex", gap: 8 }}>
                {VARIANTS.map((v) => {
                  const active = accent === v.accent && dark === (v.mode === "dark");
                  return (
                    <button
                      key={v.key}
                      type="button"
                      onClick={() => set({ accent: v.accent, mode: v.mode })}
                      title={v.label}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                        padding: "5px 9px",
                        borderRadius: 8,
                        cursor: "pointer",
                        fontFamily: C.font,
                        fontSize: 11.5,
                        fontWeight: 600,
                        color: active ? (dark ? "#fff" : C.ink) : mute,
                        background: active ? (dark ? "rgba(255,255,255,0.06)" : C.paper2) : "transparent",
                        border: `1px solid ${active ? v.accent : border}`,
                      }}
                    >
                      <span
                        style={{
                          width: 14,
                          height: 14,
                          borderRadius: 4,
                          background: v.accent,
                          boxShadow: v.mode === "dark" ? "inset 0 0 0 2px #15151B" : "none",
                        }}
                      />
                      {v.key}
                    </button>
                  );
                })}
              </div>
            </Row>
            <Row label="主题色" sub="影响按钮、矩阵与高亮" ink={ink} mute={mute}>
              <div style={{ display: "flex", gap: 6 }}>
                {THEME_SWATCHES.map((c) => (
                  <div
                    key={c}
                    onClick={() => set({ accent: c })}
                    style={{
                      width: 22,
                      height: 22,
                      borderRadius: 6,
                      background: c,
                      cursor: "pointer",
                      boxShadow:
                        c === accent ? `0 0 0 2px ${dark ? "#15151B" : C.paper}, 0 0 0 3.5px ${c}` : "none",
                    }}
                  />
                ))}
              </div>
            </Row>
            <Row label="深色模式" sub="浅色 / 深色 / 跟随系统" ink={ink} mute={mute} last>
              <SegControl
                options={["浅色", "深色", "跟随系统"]}
                value={modeIndex}
                onChange={(i) => set({ mode: i === 0 ? "light" : i === 1 ? "dark" : "system" })}
              />
            </Row>
          </Card>

          <div style={{ fontSize: 11, color: mute, textAlign: "center", padding: "8px 0 20px" }}>
            <Logo size={20} color={accent} style={{ verticalAlign: "middle", marginRight: 6 }} />
            <span style={{ verticalAlign: "middle" }}>原本 · Verum · 标书查重 v0.1.0</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function Card({
  title,
  children,
  cardBg,
  border,
  mute,
}: {
  title: string;
  children: ReactNode;
  cardBg: string;
  border: string;
  mute: string;
}) {
  return (
    <div>
      <div
        style={{
          fontSize: 10.5,
          fontWeight: 600,
          color: mute,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
          marginBottom: 10,
          paddingLeft: 4,
        }}
      >
        {title}
      </div>
      <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: "4px 18px" }}>
        {children}
      </div>
    </div>
  );
}

function Row({
  label,
  sub,
  children,
  ink,
  mute,
  last,
}: {
  label: string;
  sub?: string;
  children: ReactNode;
  ink: string;
  mute: string;
  last?: boolean;
}) {
  const { dark } = useTheme();
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 16,
        padding: "12px 0",
        borderBottom: last ? "none" : `1px solid ${dark ? "rgba(255,255,255,0.06)" : C.line}`,
      }}
    >
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 12.5, fontWeight: 600, color: ink, letterSpacing: "-0.005em" }}>{label}</div>
        {sub && <div style={{ fontSize: 11, color: mute, marginTop: 3 }}>{sub}</div>}
      </div>
      {children}
    </div>
  );
}
