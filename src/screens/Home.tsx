// 屏 1 · 新建任务 —— 检测设置 + 候选标书均由真实状态驱动
import { useState, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill, DocChip, SegControl, Toggle } from "../components/primitives";
import { BID_SLOTS, type BidSlot } from "../data/mock";
import { useTheme } from "../theme";
import type { Settings, Scope } from "../prefs";
import type { PickedFile } from "../App";

const TAGS = ["甲", "乙", "丙", "丁", "戊"];
const PALETTE = ["#4F58A8", "#0E9A8F", "#C28430", "#B54545", "#7C3AED"];
const SCOPES: Scope[] = ["full", "tech", "business"];
const SCOPE_LABEL: Record<Scope, string> = { full: "完整文档", tech: "仅技术标", business: "仅商务标" };

export function Home({
  files,
  canPick,
  settings,
  taskName,
  onAddFiles,
  onRemoveFile,
  onChangeSettings,
  onChangeName,
  onAnalyze,
}: {
  files: PickedFile[];
  canPick: boolean;
  settings: Settings;
  taskName: string;
  onAddFiles: () => void;
  onRemoveFile: (i: number) => void;
  onChangeSettings: (patch: Partial<Settings>) => void;
  onChangeName: (v: string) => void;
  onAnalyze: () => void;
}) {
  const { dark, accent } = useTheme();
  const [showHelp, setShowHelp] = useState(false);
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const demo = !canPick && files.length === 0;
  const count = demo ? 4 : files.length;
  const pairs = count >= 2 ? (count * (count - 1)) / 2 : 0;
  const pct = Math.round(settings.threshold * 100);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="新建查重任务"
        sub="选择 2 至 5 份标书进行交叉比对"
        actions={
          <Button kind="ghost" size="md" icon="info" onClick={() => setShowHelp((v) => !v)}>
            如何识别围标?
          </Button>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 1080, margin: "0 auto" }}>
          {/* 标题区 */}
          <div style={{ marginBottom: 20 }}>
            <div
              style={{
                fontSize: 11,
                color: accent,
                fontWeight: 700,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
              }}
            >
              本地交叉比对 · 不上传任何文件
            </div>
            <div
              style={{
                fontSize: 26,
                fontWeight: 700,
                color: ink,
                marginTop: 8,
                letterSpacing: "-0.018em",
                lineHeight: 1.25,
                fontFamily: C.serif,
              }}
            >
              把候选的几份标书，一起摆在桌上看。
            </div>
            <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.6 }}>
              在 2 至 5 份待审标书之间，识别条款级的雷同片段、共用模板与围标嫌疑。
              所有比对在你的电脑上本地完成，不上传任何文件。
            </div>
          </div>

          {showHelp && <HelpCard cardBg={cardBg} border={border} ink={ink} mute={mute} accent={accent} />}

          {/* 任务名称（可编辑） */}
          <div
            style={{
              background: cardBg,
              border: `1px solid ${border}`,
              borderRadius: 12,
              padding: "12px 18px",
              display: "flex",
              alignItems: "center",
              gap: 14,
              marginBottom: 16,
            }}
          >
            <div
              style={{
                fontSize: 10.5,
                fontWeight: 700,
                color: mute,
                letterSpacing: "0.06em",
                textTransform: "uppercase",
                flexShrink: 0,
              }}
            >
              任务名称
            </div>
            <input
              value={taskName}
              onChange={(e) => onChangeName(e.target.value)}
              placeholder="为本次查重命名"
              style={{
                flex: 1,
                fontSize: 14,
                fontWeight: 600,
                color: ink,
                letterSpacing: "-0.005em",
                background: "transparent",
                border: "none",
                outline: "none",
                fontFamily: C.font,
              }}
            />
            <Pill bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} fg={mute} size={11}>
              {count >= 2 ? `${pairs} 对比对` : "待选文件"}
            </Pill>
          </div>

          {/* 候选标书 */}
          <div
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: ink,
              marginBottom: 10,
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <span>候选标书</span>
            <Pill bg={C.brandSoft} fg={accent} size={10}>
              {count} / 5
            </Pill>
            <span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>
              至少 2 份，最多 5 份{canPick ? " · 可点击或拖拽导入" : ""}
            </span>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: 12 }}>
            {demo
              ? BID_SLOTS.map((s, i) => (
                  <DemoSlot key={i} s={s} idx={i} cardBg={cardBg} border={border} ink={ink} mute={mute} />
                ))
              : Array.from({ length: 5 }, (_, i) =>
                  files[i] ? (
                    <FileSlot
                      key={i}
                      f={files[i]}
                      idx={i}
                      cardBg={cardBg}
                      border={border}
                      ink={ink}
                      mute={mute}
                      onRemove={() => onRemoveFile(i)}
                    />
                  ) : (
                    <AddSlot key={i} mute={mute} onClick={onAddFiles} />
                  ),
                )}
          </div>

          {/* 检测设置 + 开始 */}
          <div style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: 16, marginTop: 22 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 14 }}>检测设置</div>
              <SettingsRow
                label="比对范围"
                sub={count >= 2 ? `对所选 ${count} 份标书两两比对，共 ${pairs} 对` : "选择参与比对的标段"}
                ink={ink}
                mute={mute}
              >
                <SegControl
                  options={["完整文档", "仅技术标", "仅商务标"]}
                  value={SCOPES.indexOf(settings.scope)}
                  onChange={(i) => onChangeSettings({ scope: SCOPES[i] })}
                />
              </SettingsRow>
              <SettingsRow label="最低相似度阈值" sub="低于此值的片段不进入报告" ink={ink} mute={mute}>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <input
                    type="range"
                    min={10}
                    max={95}
                    step={5}
                    value={pct}
                    onChange={(e) => onChangeSettings({ threshold: Number(e.target.value) / 100 })}
                    style={{ width: 140, accentColor: accent, cursor: "pointer" }}
                  />
                  <span
                    style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 32 }}
                  >
                    {pct}%
                  </span>
                </div>
              </SettingsRow>
              <SettingsRow label="忽略通用模板段落" sub="标准条款、表头、附件目录（查重源库）" ink={ink} mute={mute}>
                <Toggle
                  on={settings.ignoreTemplates}
                  onChange={() => onChangeSettings({ ignoreTemplates: !settings.ignoreTemplates })}
                />
              </SettingsRow>
              <SettingsRow label="围标嫌疑提示" sub="3 份及以上共同高相似片段触发" ink={ink} mute={mute} last>
                <Toggle
                  on={settings.flagCollusion}
                  onChange={() => onChangeSettings({ flagCollusion: !settings.flagCollusion })}
                />
              </SettingsRow>
            </div>
            <div
              style={{
                background: dark ? "rgba(79,88,168,0.10)" : `${accent}08`,
                border: `1px solid ${accent}33`,
                borderRadius: 12,
                padding: 18,
                display: "flex",
                flexDirection: "column",
              }}
            >
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  color: accent,
                  letterSpacing: "0.08em",
                  textTransform: "uppercase",
                }}
              >
                本次工作量
              </div>
              <div
                style={{
                  fontSize: 32,
                  fontWeight: 700,
                  color: ink,
                  letterSpacing: "-0.022em",
                  marginTop: 5,
                  fontFamily: C.font,
                }}
              >
                {pairs}
                <span style={{ fontSize: 14, color: mute, fontWeight: 500, marginLeft: 6 }}>对比对</span>
              </div>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  marginTop: 14,
                  fontSize: 11.5,
                  color: mute,
                }}
              >
                <span>{count} 份文档</span>
                <span style={{ fontFamily: C.mono }}>{SCOPE_LABEL[settings.scope]}</span>
              </div>
              <div style={{ flex: 1, minHeight: 14 }} />
              <Button
                kind="primary"
                size="lg"
                icon="sparkle"
                style={{
                  marginTop: 14,
                  width: "100%",
                  justifyContent: "center",
                  opacity: demo || files.length >= 2 ? 1 : 0.55,
                }}
                onClick={onAnalyze}
              >
                开始查重
              </Button>
              <div
                style={{
                  fontSize: 10.5,
                  color: mute,
                  textAlign: "center",
                  marginTop: 10,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 5,
                }}
              >
                <Icon name="lock" size={11} />
                本地完成 · 不上传任何文件
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function HelpCard({
  cardBg,
  border,
  ink,
  mute,
  accent,
}: {
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  accent: string;
}) {
  const items = [
    ["条款级雷同", "不同投标人之间出现成段、近乎逐字相同的技术或商务条款。"],
    ["共用模板与笔误", "多份标书共享同一排版、相同的错别字或异常用词，往往出自同一人之手。"],
    ["元数据同源", "文档作者、最后修改人、创建时间高度一致，提示同一台电脑制作。"],
    ["报价异常", "报价构成、让利幅度呈规律性排列（本工具聚焦文本，报价需结合开标记录）。"],
  ];
  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "16px 18px",
        marginBottom: 16,
      }}
    >
      <div style={{ fontSize: 12.5, fontWeight: 700, color: accent, marginBottom: 10 }}>
        围标的常见信号
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
        {items.map(([t, d]) => (
          <div key={t} style={{ display: "flex", gap: 8 }}>
            <Icon name="check" size={13} style={{ color: accent, marginTop: 2, flexShrink: 0 }} />
            <div>
              <div style={{ fontSize: 12, fontWeight: 600, color: ink }}>{t}</div>
              <div style={{ fontSize: 11, color: mute, marginTop: 2, lineHeight: 1.5 }}>{d}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function AddSlot({ mute, onClick }: { mute: string; onClick: () => void }) {
  const { dark } = useTheme();
  return (
    <div
      onClick={onClick}
      style={{
        background: dark ? "rgba(255,255,255,0.02)" : C.white,
        border: `1.5px dashed ${dark ? "rgba(255,255,255,0.15)" : C.ink5}`,
        borderRadius: 12,
        padding: "20px 14px",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        minHeight: 142,
        cursor: "pointer",
      }}
    >
      <div
        style={{
          width: 36,
          height: 36,
          borderRadius: 10,
          background: dark ? "rgba(255,255,255,0.04)" : C.paper2,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: mute,
        }}
      >
        <Icon name="plus" size={18} />
      </div>
      <div style={{ fontSize: 11.5, color: mute, textAlign: "center", fontWeight: 500 }}>
        添加标书
        <br />
        <span style={{ fontSize: 10.5 }}>点击或拖入文件</span>
      </div>
    </div>
  );
}

function FileSlot({
  f,
  idx,
  cardBg,
  border,
  ink,
  mute,
  onRemove,
}: {
  f: PickedFile;
  idx: number;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
  onRemove: () => void;
}) {
  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "14px 14px",
        minHeight: 142,
        display: "flex",
        flexDirection: "column",
        position: "relative",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <div
          style={{
            width: 20,
            height: 20,
            borderRadius: 5,
            background: PALETTE[idx],
            color: "#fff",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 11,
            fontWeight: 700,
            fontFamily: C.serif,
          }}
        >
          {TAGS[idx]}
        </div>
        <DocChip type={f.type || "txt"} />
        <div style={{ flex: 1 }} />
        <Icon name="x" size={12} style={{ color: mute, cursor: "pointer" }} onClick={onRemove} />
      </div>
      <div
        style={{
          fontSize: 11.5,
          fontWeight: 600,
          color: ink,
          marginTop: 9,
          lineHeight: 1.4,
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
          wordBreak: "break-all",
        }}
      >
        {f.name}
      </div>
      <div style={{ flex: 1 }} />
      <div
        style={{
          fontSize: 10.5,
          color: mute,
          marginTop: 8,
          display: "flex",
          justifyContent: "space-between",
        }}
      >
        <span>{(f.type || "?").toUpperCase()}</span>
        <span style={{ fontFamily: C.mono }}>待解析</span>
      </div>
    </div>
  );
}

function DemoSlot({
  s,
  idx,
  cardBg,
  border,
  ink,
  mute,
}: {
  s: BidSlot;
  idx: number;
  cardBg: string;
  border: string;
  ink: string;
  mute: string;
}) {
  const { dark } = useTheme();
  if (s.state === "empty") {
    return (
      <div
        style={{
          background: dark ? "rgba(255,255,255,0.02)" : C.white,
          border: `1.5px dashed ${dark ? "rgba(255,255,255,0.15)" : C.ink5}`,
          borderRadius: 12,
          padding: "20px 14px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          minHeight: 142,
        }}
      >
        <div
          style={{
            width: 36,
            height: 36,
            borderRadius: 10,
            background: dark ? "rgba(255,255,255,0.04)" : C.paper2,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: mute,
          }}
        >
          <Icon name="plus" size={18} />
        </div>
        <div style={{ fontSize: 11.5, color: mute, textAlign: "center", fontWeight: 500 }}>
          {s.label}
        </div>
      </div>
    );
  }
  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "14px 14px",
        minHeight: 142,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <div
          style={{
            width: 20,
            height: 20,
            borderRadius: 5,
            background: PALETTE[idx],
            color: "#fff",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 11,
            fontWeight: 700,
            fontFamily: C.serif,
          }}
        >
          {TAGS[idx]}
        </div>
        <DocChip type={s.type ?? "txt"} />
      </div>
      <div
        style={{
          fontSize: 11.5,
          fontWeight: 600,
          color: ink,
          marginTop: 9,
          lineHeight: 1.4,
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}
      >
        {s.name}
      </div>
      <div style={{ flex: 1 }} />
      <div
        style={{
          fontSize: 10.5,
          color: mute,
          marginTop: 8,
          display: "flex",
          justifyContent: "space-between",
        }}
      >
        <span>{s.pages} 页</span>
        <span style={{ fontFamily: C.mono }}>{s.size}</span>
      </div>
    </div>
  );
}

function SettingsRow({
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
