// 屏 8 · 设置 —— 检测偏好 / 隐私 / 外观，全部接入本地持久化
import { useState, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Logo } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Toggle, SegControl } from "../components/primitives";
import { useTheme, type FontScale, type Highlight } from "../theme";
import { useToast } from "../components/Toast";
import { getSettings, setSettings, type Settings as DetectSettings, type Scope } from "../prefs";

const FONT_SCALES: FontScale[] = ["compact", "regular", "comfy", "spacious"];
const SCOPES: Scope[] = ["full", "tech", "business"];
const HIGHLIGHTS: Highlight[] = ["amber", "rose", "blue"];

export function Settings() {
  const { dark, accent, mode, set, fontScale, layout, highlight, reduceMotion } = useTheme();
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
            <Row label="默认比对范围" sub="新任务默认比对的标段" ink={ink} mute={mute}>
              <SegControl
                options={["完整文档", "仅技术标", "仅商务标"]}
                value={Math.max(0, SCOPES.indexOf(s.scope))}
                onChange={(i) => change({ scope: SCOPES[i] })}
              />
            </Row>
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
            <Row label="界面字号" sub="整体放大或缩小界面与文字" ink={ink} mute={mute}>
              <SegControl
                options={["小", "标准", "大", "特大"]}
                value={Math.max(0, FONT_SCALES.indexOf(fontScale))}
                onChange={(i) => set({ fontScale: FONT_SCALES[i] })}
              />
            </Row>
            <Row label="雷同高亮配色" sub="逐对对比中雷同片段的标注颜色" ink={ink} mute={mute}>
              <SegControl
                options={["琥珀", "玫红", "靛蓝"]}
                value={Math.max(0, HIGHLIGHTS.indexOf(highlight))}
                onChange={(i) => set({ highlight: HIGHLIGHTS[i] })}
              />
            </Row>
            <Row label="紧凑侧栏" sub="折叠侧栏文字只留图标，腾出更多内容空间" ink={ink} mute={mute}>
              <Toggle
                on={layout === "compact"}
                onChange={() => set({ layout: layout === "compact" ? "comfort" : "compact" })}
              />
            </Row>
            <Row label="减少动效" sub="关闭进度脉冲与过渡动画" ink={ink} mute={mute}>
              <Toggle on={reduceMotion} onChange={() => set({ reduceMotion: !reduceMotion })} />
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
