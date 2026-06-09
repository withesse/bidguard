// 屏 5 · 逐对对比 —— 真实模式：按段落匹配对渲染（重建文本 + 高亮雷同）；mock 模式：原全页设计。
import { useMemo, useState, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { useTheme, type Highlight } from "../theme";
import type { Screen } from "../routes";
import type { Report, DiffOp } from "../engine";

type HiScheme = Record<string, string>;
const TAGS = ["甲", "乙", "丙", "丁", "戊"];
const PAL = ["#4F58A8", "#0E9A8F", "#C28430", "#B54545", "#7C3AED"];

function hiScheme(name: Highlight): HiScheme {
  if (name === "rose")
    return { hi1: "#E89FAE", hi2: "#D86E84", hi3: "#B83F5E", hi4: "#8C2444", hi1soft: "#F8D9DF", hi2soft: "#F4C5CF", hi3soft: "#EFAFBE", hi4soft: "#E89DAE" };
  if (name === "blue")
    return { hi1: "#A6BDDE", hi2: "#6B8BC4", hi3: "#3D63A8", hi4: "#1E4080", hi1soft: "#D8E2F1", hi2soft: "#BDCFE7", hi3soft: "#9FB8DA", hi4soft: "#7E9DCB" };
  return { hi1: C.hi1, hi2: C.hi2, hi3: C.hi3, hi4: C.hi4, hi1soft: C.hi1Soft, hi2soft: C.hi2Soft, hi3soft: C.hi3Soft, hi4soft: C.hi4Soft };
}

export function Compare({ onGo, report }: { onGo: (s: Screen) => void; report?: Report | null }) {
  if (report && report.pairs && report.pairs.length) return <RealCompare report={report} onGo={onGo} />;
  return <MockCompare onGo={onGo} />;
}

// ─────────────────────────────────────────────────────────────
// 真实模式
// ─────────────────────────────────────────────────────────────
function RealCompare({ report, onGo }: { report: Report; onGo: (s: Screen) => void }) {
  const { dark, accent, highlight } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const paperBg = dark ? "#22222A" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const HI = hiScheme(highlight);

  const pairs = useMemo(() => [...report.pairs].sort((a, b) => b.score - a.score), [report.pairs]);
  const [sel, setSel] = useState(0);
  const pair = pairs[sel] ?? pairs[0];
  const docName = (i: number) => report.docs[i]?.name ?? TAGS[i] ?? "?";

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="逐对对比"
        sub={`${TAGS[pair.a]} ${docName(pair.a)} × ${TAGS[pair.b]} ${docName(pair.b)} · ${pair.matches.length} 处匹配`}
        actions={
          <Button kind="primary" size="md" icon="check" onClick={() => onGo("matrix")}>
            返回报告
          </Button>
        }
      />
      {/* 配对选择 */}
      <div
        style={{
          minHeight: 56,
          flexShrink: 0,
          padding: "10px 24px",
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexWrap: "wrap",
          borderBottom: `1px solid ${border}`,
          background: dark ? "rgba(255,255,255,0.02)" : C.paper2,
        }}
      >
        <span style={{ fontSize: 11, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>
          对比组合
        </span>
        <div style={{ display: "flex", gap: 4, alignItems: "center", flexWrap: "wrap" }}>
          {pairs.map((p, i) => {
            const pct = Math.round(p.score * 100);
            const active = i === sel;
            return (
              <div
                key={`${p.a}-${p.b}`}
                onClick={() => setSel(i)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "5px 10px 5px 6px",
                  borderRadius: 7,
                  background: active ? (dark ? "rgba(255,255,255,0.08)" : "#fff") : "transparent",
                  border: `1px solid ${active ? accent : "transparent"}`,
                  boxShadow: active && !dark ? "0 1px 2px rgba(0,0,0,0.06)" : "none",
                  cursor: "pointer",
                }}
              >
                <div style={{ display: "flex", gap: 2 }}>
                  {[p.a, p.b].map((d) => (
                    <Tag key={d} idx={d} size={18} />
                  ))}
                </div>
                <span
                  style={{
                    fontSize: 11.5,
                    fontWeight: 700,
                    fontFamily: C.mono,
                    color: pct >= 80 ? C.danger : pct >= 60 ? C.hi3 : pct >= 30 ? C.hi2 : C.hi1,
                  }}
                >
                  {pct}%
                </span>
              </div>
            );
          })}
        </div>
        <div style={{ flex: 1 }} />
        <Pill bg={HI.hi3soft} fg={HI.hi3} size={11}>
          高亮 = 两份共享的雷同片段
        </Pill>
      </div>

      {/* 匹配段落列表 */}
      <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "18px 24px 40px" }}>
        {pair.matches.length === 0 ? (
          <div style={{ textAlign: "center", color: mute, fontSize: 13, padding: "60px 0" }}>
            该组合未发现达到阈值的雷同段落，差异充分。
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 1200, margin: "0 auto" }}>
            {/* 列头 */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 56px 1fr", gap: 12, alignItems: "center" }}>
              <PaneHeader idx={pair.a} ink={ink} />
              <div />
              <PaneHeader idx={pair.b} ink={ink} />
            </div>
            {pair.matches.map((m, i) => {
              const pct = Math.round(m.score * 100);
              const c = pct >= 80 ? C.danger : pct >= 60 ? HI.hi3 : HI.hi2;
              return (
                <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 56px 1fr", gap: 12, alignItems: "stretch" }}>
                  <SegPane diff={m.diff} side="a" paperBg={paperBg} border={border} ink={ink} mute={mute} HI={HI} />
                  <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 4 }}>
                    <span style={{ fontSize: 14, fontWeight: 700, color: c, fontFamily: C.mono }}>{pct}%</span>
                    <Icon name="diff" size={13} style={{ color: mute }} />
                  </div>
                  <SegPane diff={m.diff} side="b" paperBg={paperBg} border={border} ink={ink} mute={mute} HI={HI} />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function Tag({ idx, size = 18 }: { idx: number; size?: number }) {
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: 4,
        background: PAL[idx] ?? C.brand,
        color: "#fff",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: size * 0.58,
        fontWeight: 700,
        fontFamily: C.serif,
      }}
    >
      {TAGS[idx] ?? "?"}
    </div>
  );
}

function PaneHeader({ idx, ink }: { idx: number; ink: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <Tag idx={idx} size={22} />
      <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>{TAGS[idx]} 方</span>
    </div>
  );
}

// 重建一侧文本：A = eq + del；B = eq + ins。eq(共享) 高亮，独有片段淡化。
function SegPane({
  diff,
  side,
  paperBg,
  border,
  ink,
  mute,
  HI,
}: {
  diff: DiffOp[];
  side: "a" | "b";
  paperBg: string;
  border: string;
  ink: string;
  mute: string;
  HI: HiScheme;
}) {
  const uniqueOp = side === "a" ? "del" : "ins";
  return (
    <div
      style={{
        background: paperBg,
        borderRadius: 8,
        border: `1px solid ${border}`,
        padding: "14px 16px",
        fontSize: 13,
        lineHeight: 1.85,
        color: ink,
        fontFamily: C.font,
      }}
    >
      {diff
        .filter((d) => d.op === "eq" || d.op === uniqueOp)
        .map((d, i) =>
          d.op === "eq" ? (
            <span key={i} style={{ borderBottom: `2px solid ${HI.hi3}`, paddingBottom: 1 }}>
              {d.text}
            </span>
          ) : (
            <span key={i} style={{ color: mute, background: side === "a" ? HI.hi1soft : C.okSoft, borderRadius: 2 }}>
              {d.text}
            </span>
          ),
        )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// mock 模式（浏览器预览演示，保留原全页设计）
// ─────────────────────────────────────────────────────────────
const PAIRS = [
  { left: "甲", leftColor: "#4F58A8", right: "乙", rightColor: "#0E9A8F", pct: 92, active: true },
  { left: "甲", leftColor: "#4F58A8", right: "丙", rightColor: "#C28430", pct: 34 },
  { left: "甲", leftColor: "#4F58A8", right: "丁", rightColor: "#B54545", pct: 42 },
  { left: "乙", leftColor: "#0E9A8F", right: "丙", rightColor: "#C28430", pct: 31 },
  { left: "乙", leftColor: "#0E9A8F", right: "丁", rightColor: "#B54545", pct: 40 },
  { left: "丙", leftColor: "#C28430", right: "丁", rightColor: "#B54545", pct: 68 },
];

function MockCompare({ onGo }: { onGo: (s: Screen) => void }) {
  const { dark, highlight } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const paperBg = dark ? "#22222A" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const HI = hiScheme(highlight);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="逐对对比"
        sub="市政信息化平台采购 · 第 4 处 / 共 12 处雷同"
        actions={
          <>
            <Button kind="ghost" size="md" icon="quote">
              抽取条款
            </Button>
            <Button kind="primary" size="md" icon="check" onClick={() => onGo("matrix")}>
              标记为已复核
            </Button>
          </>
        }
      />
      <div
        style={{
          height: 56,
          flexShrink: 0,
          padding: "0 24px",
          display: "flex",
          alignItems: "center",
          gap: 14,
          borderBottom: `1px solid ${border}`,
          background: dark ? "rgba(255,255,255,0.02)" : C.paper2,
        }}
      >
        <span style={{ fontSize: 11, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>
          对比组合
        </span>
        <MockPairSelector />
        <div style={{ flex: 1 }} />
        <Button kind="secondary" size="sm" icon="chevL">
          上一处
        </Button>
        <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>
          <span style={{ color: ink, fontWeight: 700 }}>4</span> / 12 处
        </span>
        <Button kind="secondary" size="sm" iconRight="chevR">
          下一处
        </Button>
        <div style={{ width: 1, height: 18, background: border }} />
        <Pill bg={HI.hi3soft} fg={HI.hi3} size={11}>
          ≥60% 高
        </Pill>
        <Pill bg={HI.hi2soft} fg={HI.hi2} size={11}>
          30-60% 中
        </Pill>
      </div>

      <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "1fr 1fr", position: "relative" }}>
        <MockDocPane tag="甲" tagColor="#4F58A8" name="智慧城邦科技_技术响应文件.pdf" meta="86 页 · §3 技术方案 · 第 12 页" paperBg={paperBg} border={border} ink={ink} mute={mute}>
          <BodyA HI={HI} mute={mute} dark={dark} />
        </MockDocPane>
        <MockDocPane tag="乙" tagColor="#0E9A8F" name="启明信息_投标文件_技术标.docx" meta="72 页 · §3 技术方案 · 第 9 页" paperBg={paperBg} border={border} ink={ink} mute={mute}>
          <BodyB HI={HI} mute={mute} dark={dark} />
        </MockDocPane>
        <div style={{ position: "absolute", left: "50%", top: 0, bottom: 0, width: 1, background: border, transform: "translateX(-0.5px)" }} />
      </div>
    </div>
  );
}

function MockPairSelector() {
  const { dark, accent } = useTheme();
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
      {PAIRS.map((p, i) => (
        <div
          key={i}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "5px 10px 5px 6px",
            borderRadius: 7,
            background: p.active ? (dark ? "rgba(255,255,255,0.08)" : "#fff") : "transparent",
            border: `1px solid ${p.active ? accent : "transparent"}`,
            boxShadow: p.active && !dark ? "0 1px 2px rgba(0,0,0,0.06)" : "none",
            cursor: "pointer",
          }}
        >
          <div style={{ display: "flex", gap: 2 }}>
            {[
              { t: p.left, c: p.leftColor },
              { t: p.right, c: p.rightColor },
            ].map((x, j) => (
              <div
                key={j}
                style={{
                  width: 18,
                  height: 18,
                  borderRadius: 4,
                  background: x.c,
                  color: "#fff",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 10.5,
                  fontWeight: 700,
                  fontFamily: C.serif,
                }}
              >
                {x.t}
              </div>
            ))}
          </div>
          <span
            style={{
              fontSize: 11.5,
              fontWeight: 700,
              fontFamily: C.mono,
              color: p.pct >= 80 ? C.danger : p.pct >= 60 ? C.hi3 : p.pct >= 30 ? C.hi2 : C.hi1,
            }}
          >
            {p.pct}%
          </span>
        </div>
      ))}
    </div>
  );
}

function MockDocPane({
  tag,
  tagColor,
  name,
  meta,
  paperBg,
  border,
  ink,
  mute,
  children,
}: {
  tag: string;
  tagColor: string;
  name: string;
  meta: string;
  paperBg: string;
  border: string;
  ink: string;
  mute: string;
  children: ReactNode;
}) {
  const { dark } = useTheme();
  return (
    <div style={{ display: "flex", flexDirection: "column", minWidth: 0, background: dark ? "#181820" : C.paper }}>
      <div style={{ padding: "12px 20px", borderBottom: `1px solid ${border}`, display: "flex", alignItems: "center", gap: 10, background: dark ? "#1F1F26" : C.white }}>
        <div style={{ width: 24, height: 24, borderRadius: 6, background: tagColor, color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 13, fontWeight: 700, fontFamily: C.serif, flexShrink: 0 }}>
          {tag}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 12.5, fontWeight: 600, color: ink, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</div>
          <div style={{ fontSize: 10.5, color: mute, marginTop: 1 }}>{meta}</div>
        </div>
        <Icon name="search" size={13} style={{ color: mute }} />
        <Icon name="sliders" size={13} style={{ color: mute }} />
      </div>
      <div style={{ flex: 1, overflow: "auto", padding: "24px 30px", display: "flex", justifyContent: "center" }}>
        <div style={{ maxWidth: 540, width: "100%", background: paperBg, borderRadius: 8, border: `1px solid ${border}`, boxShadow: dark ? "none" : C.shadow.sm, padding: "36px 44px 32px", fontSize: 13, lineHeight: 1.88, color: ink, fontFamily: C.font, letterSpacing: "-0.003em" }}>
          {children}
        </div>
      </div>
    </div>
  );
}

function CHSpan({ children, level = 2, refLabel, HI }: { children: ReactNode; level?: number; refLabel?: string; HI: HiScheme }) {
  const color = HI[`hi${level}`];
  return (
    <span style={{ borderBottom: `2px solid ${color}`, paddingBottom: 1, position: "relative", cursor: "pointer" }}>
      {children}
      {refLabel && (
        <sup style={{ marginLeft: 2, padding: "0 4px", borderRadius: 3, background: color, color: "#fff", fontSize: 9, fontWeight: 700, verticalAlign: "super", fontFamily: C.mono }}>{refLabel}</sup>
      )}
    </span>
  );
}

function BodyA({ HI, mute, dark }: { HI: HiScheme; mute: string; dark: boolean }) {
  return (
    <>
      <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>§3 技术方案</div>
      <h2 style={{ fontSize: 19, fontWeight: 700, margin: "6px 0 14px", letterSpacing: "-0.014em", color: dark ? "#fff" : C.ink }}>3.2 总体架构设计</h2>
      <p>
        本项目采用「分层解耦、微服务、统一服务总线」的总体架构。
        <CHSpan HI={HI} level={4} refLabel="①">系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层，各层之间通过标准化接口解耦，所有业务能力对外以 API 网关统一暴露，确保横向可扩展与纵向可演进</CHSpan>。
      </p>
      <p>
        在数据层，
        <CHSpan HI={HI} level={4} refLabel="②">采用读写分离与多级缓存机制，关键业务数据在 PostgreSQL 主库 + 只读副本 + Redis 缓存的三级架构下，保证 99.99% 可用性与 200ms 内的端到端响应</CHSpan>。
      </p>
      <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>§3.3 安全合规</div>
      <p style={{ marginTop: 6 }}>
        <CHSpan HI={HI} level={3} refLabel="③">全平台遵循等保 2.0 三级与 ISO 27001 标准，所有数据在传输与静止状态下均通过国密 SM4 加密，密钥由本地 HSM 派生并按月轮换</CHSpan>。我们将与甲方信息部门共同建立 7×24 安全响应机制。
      </p>
      <div style={{ marginTop: 18, padding: 14, borderRadius: 10, background: dark ? "rgba(181,69,69,0.12)" : C.dangerSoft, border: `1px solid ${dark ? "rgba(181,69,69,0.4)" : "#E8C7C7"}` }}>
        <div style={{ display: "flex", alignItems: "center", gap: 7, marginBottom: 8 }}>
          <Icon name="info" size={13} style={{ color: C.danger }} />
          <span style={{ fontSize: 11, fontWeight: 700, color: C.danger, letterSpacing: "0.04em", textTransform: "uppercase" }}>围标嫌疑 · ①② 句</span>
          <span style={{ flex: 1 }} />
          <span style={{ fontSize: 11, color: mute, fontFamily: C.mono }}>与乙: <b style={{ color: C.danger }}>92%</b></span>
        </div>
        <div style={{ fontSize: 12, color: dark ? "#fff" : C.ink, lineHeight: 1.6 }}>
          ① 句在 乙 标书 §3.1 中以完全相同语序出现，且包含 2 处罕见错别字一致；② 句的「200ms 内的端到端响应」措辞与 乙 §3.2 末段一字不差。
        </div>
      </div>
    </>
  );
}

function BodyB({ HI, mute, dark }: { HI: HiScheme; mute: string; dark: boolean }) {
  return (
    <>
      <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>§3 技术方案</div>
      <h2 style={{ fontSize: 19, fontWeight: 700, margin: "6px 0 14px", letterSpacing: "-0.014em", color: dark ? "#fff" : C.ink }}>3.1 整体技术架构</h2>
      <p>
        本项目采取「分层解耦、微服务、统一服务总线」的整体架构思路。
        <CHSpan HI={HI} level={4} refLabel="①">系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层，各层之间通过标准化接口解耦，所有业务能力对外以 API 网关统一暴露，确保横向可扩展与纵向可演进</CHSpan>，以适应未来三年的业务发展需要。
      </p>
      <p>
        数据层方面，
        <CHSpan HI={HI} level={4} refLabel="②">本方案采用读写分离与多级缓存机制，关键业务数据在 PostgreSQL 主库 + 只读副本 + Redis 缓存的三级架构下，保证 99.99% 可用性与 200ms 内的端到端响应</CHSpan>，经我司在多个同类项目中验证。
      </p>
      <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: "0.06em", textTransform: "uppercase" }}>§3.2 安全体系</div>
      <p style={{ marginTop: 6 }}>
        <CHSpan HI={HI} level={3} refLabel="③">本平台严格遵循等保 2.0 三级与 ISO 27001 标准，在数据加密方面采用国密 SM4 算法，密钥由本地 HSM 派生并按月轮换</CHSpan>。
      </p>
      <p style={{ color: mute, fontSize: 12 }}>（后续 §3.3 章节为乙方独立设计的容灾与混合云架构，与甲方文档无重叠。）</p>
    </>
  );
}
