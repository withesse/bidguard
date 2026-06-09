// 屏 6 · 重复条款聚合 —— 真实模式：按引擎聚类渲染；mock 模式：原详情设计（浏览器预览演示）。
import { Fragment, useState } from "react";
import { C, severityColor } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import type { Screen } from "../routes";
import { SECTION_LABEL, type Report, type Cluster as RealCluster } from "../engine";

const TAGS = ["甲", "乙", "丙", "丁", "戊"];
const PAL = ["#4F58A8", "#0E9A8F", "#C28430", "#B54545", "#7C3AED"];

export function Clusters({ onGo, report }: { onGo: (s: Screen) => void; report?: Report | null }) {
  if (report) return <RealClusters report={report} onGo={onGo} />;
  return <MockClusters onGo={onGo} />;
}

// ─────────────────────────────────────────────────────────────
// 真实模式
// ─────────────────────────────────────────────────────────────
function RealClusters({ report, onGo }: { report: Report; onGo: (s: Screen) => void }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const clusters = report.clusters;
  const [sel, setSel] = useState(0);
  const [flagged, setFlagged] = useState<Set<number>>(new Set());
  const cl = clusters[sel];
  const toast = useToast();

  const sus = clusters.filter((c) => c.avgScore >= 0.85).length;
  const toggleFlag = (i: number) => {
    const has = flagged.has(i);
    setFlagged((prev) => {
      const next = new Set(prev);
      if (has) next.delete(i);
      else next.add(i);
      return next;
    });
    toast.show(has ? "已移出异常报告" : "已列入异常报告", "success");
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="重复条款聚合"
        sub={`共 ${clusters.length} 组雷同条款${sus ? ` · 含 ${sus} 组围标嫌疑` : ""}`}
        actions={
          <Button kind="secondary" size="md" icon="diff" onClick={() => onGo("compare")}>
            逐对对比
          </Button>
        }
      />
      {clusters.length === 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: mute, fontSize: 13 }}>
          未发现跨文档雷同条款，各份标书差异充分。
        </div>
      ) : (
        <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "320px 1fr", gap: 16, padding: 20, overflow: "hidden" }}>
          {/* 左：聚合列表 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <div style={{ padding: "12px 14px", borderBottom: `1px solid ${border}`, display: "flex", alignItems: "center" }}>
              <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>聚合条款</span>
              <div style={{ flex: 1 }} />
              <Pill bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} fg={mute} size={10}>
                {clusters.length} 组
              </Pill>
            </div>
            <div style={{ flex: 1, overflow: "auto" }}>
              {clusters.map((c, i) => (
                <ClusterItem key={i} c={c} active={i === sel} ink={ink} border={border} accent={accent} onClick={() => setSel(i)} />
              ))}
            </div>
          </div>

          {/* 右：详情 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <ClusterDetail
              cl={cl}
              report={report}
              flagged={flagged.has(sel)}
              onFlag={() => toggleFlag(sel)}
              ink={ink}
              mute={mute}
              border={border}
              dark={dark}
              onGo={onGo}
            />
          </div>
        </div>
      )}
    </div>
  );
}

function sevOf(pct: number): { c: string; label: string } {
  if (pct >= 85) return { c: C.danger, label: "围标嫌疑" };
  if (pct >= 70) return { c: C.hi3, label: "高相似" };
  if (pct >= 50) return { c: C.hi2, label: "中相似" };
  return { c: C.hi1, label: "低相似" };
}

function ClusterItem({
  c,
  active,
  ink,
  border,
  accent,
  onClick,
}: {
  c: RealCluster;
  active: boolean;
  ink: string;
  border: string;
  accent: string;
  onClick: () => void;
}) {
  const { dark } = useTheme();
  const pct = Math.round(c.avgScore * 100);
  const s = sevOf(pct);
  const title = (c.segments[0]?.text ?? "").slice(0, 28);
  return (
    <div
      onClick={onClick}
      style={{
        padding: "12px 14px",
        borderBottom: `1px solid ${border}`,
        background: active ? (dark ? "rgba(255,255,255,0.06)" : C.brandSoft) : "transparent",
        position: "relative",
        cursor: "pointer",
      }}
    >
      {active && <div style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 2.5, background: accent, borderRadius: 2 }} />}
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ width: 6, height: 6, borderRadius: "50%", background: s.c, flexShrink: 0 }} />
        <span style={{ fontSize: 12, fontWeight: active ? 700 : 600, color: ink, flex: 1, lineHeight: 1.35, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {title}…
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 7, marginTop: 7, paddingLeft: 13 }}>
        <div style={{ display: "flex", gap: 2 }}>
          {c.docs.map((d) => (
            <div key={d} style={{ width: 16, height: 16, borderRadius: 3, background: PAL[d] ?? C.brand, color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 9.5, fontWeight: 700, fontFamily: C.serif }}>
              {TAGS[d] ?? "?"}
            </div>
          ))}
        </div>
        <Pill bg={pct >= 85 ? C.dangerSoft : C.paper2} fg={s.c} size={10}>
          {s.label}
        </Pill>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11.5, fontWeight: 700, color: s.c, fontFamily: C.mono }}>{pct}%</span>
      </div>
    </div>
  );
}

function ClusterDetail({
  cl,
  report,
  flagged,
  onFlag,
  ink,
  mute,
  border,
  dark,
  onGo,
}: {
  cl: RealCluster;
  report: Report;
  flagged: boolean;
  onFlag: () => void;
  ink: string;
  mute: string;
  border: string;
  dark: boolean;
  onGo: (s: Screen) => void;
}) {
  const pct = Math.round(cl.avgScore * 100);
  const peak = Math.round(cl.peak * 100);
  const s = sevOf(pct);
  return (
    <>
      <div style={{ padding: "16px 22px", borderBottom: `1px solid ${border}` }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <Pill bg={pct >= 85 ? C.dangerSoft : C.paper2} fg={s.c} size={11}>
            <Icon name="info" size={10} />
            {s.label}
          </Pill>
          <span style={{ fontSize: 11.5, color: mute }}>出现于 {cl.docs.length} 份标书</span>
          <div style={{ flex: 1 }} />
          <span style={{ fontSize: 11, color: mute }}>组内平均 / 峰值</span>
          <span style={{ fontSize: 15, fontWeight: 700, color: s.c, fontFamily: C.mono }}>
            {pct}% / {peak}%
          </span>
        </div>
        <div style={{ fontSize: 16, fontWeight: 700, color: ink, marginTop: 8, letterSpacing: "-0.012em", fontFamily: C.serif, lineHeight: 1.5 }}>
          {(cl.segments[0]?.text ?? "").slice(0, 48)}…
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 9 }}>
          <span style={{ fontSize: 10.5, color: mute, fontWeight: 600, letterSpacing: "0.06em", textTransform: "uppercase" }}>出现于</span>
          {cl.docs.map((d) => (
            <div key={d} style={{ display: "inline-flex", alignItems: "center", gap: 5, padding: "3px 8px 3px 3px", borderRadius: 999, background: `${PAL[d]}14`, border: `1px solid ${PAL[d]}33` }}>
              <div style={{ width: 16, height: 16, borderRadius: 4, background: PAL[d], color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 10, fontWeight: 700, fontFamily: C.serif }}>
                {TAGS[d]}
              </div>
              <span style={{ fontSize: 11, fontWeight: 600, color: PAL[d] }}>{TAGS[d]} 方</span>
            </div>
          ))}
        </div>
      </div>

      <div style={{ flex: 1, overflow: "auto", padding: 22 }}>
        <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: "0.06em", textTransform: "uppercase", marginBottom: 10 }}>
          各份标书中的雷同段落（{cl.segments.length}）
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          {cl.segments.map((seg, i) => (
            <div key={i} style={{ padding: "12px 14px", borderRadius: 10, background: dark ? "rgba(255,255,255,0.025)" : "#fff", border: `1px solid ${border}` }}>
              <div style={{ display: "flex", alignItems: "center", gap: 9, marginBottom: 8 }}>
                <div style={{ width: 20, height: 20, borderRadius: 5, background: PAL[seg.doc] ?? C.brand, color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 11, fontWeight: 700, fontFamily: C.serif }}>
                  {TAGS[seg.doc] ?? "?"}
                </div>
                <span style={{ fontSize: 12, fontWeight: 600, color: ink }}>{TAGS[seg.doc]} 方</span>
              </div>
              <div style={{ fontSize: 12.5, lineHeight: 1.8, color: ink }}>{seg.text}</div>
            </div>
          ))}
        </div>

        <SectionHeat report={report} ink={ink} mute={mute} border={border} dark={dark} />
        <SharedTerms report={report} ink={ink} mute={mute} border={border} dark={dark} />
      </div>

      <div style={{ padding: "12px 22px", borderTop: `1px solid ${border}`, display: "flex", alignItems: "center", gap: 9, background: dark ? "rgba(255,255,255,0.02)" : C.paper2 }}>
        <Button kind="secondary" size="sm" icon="diff" onClick={() => onGo("compare")}>
          查看逐对对比
        </Button>
        <div style={{ flex: 1 }} />
        <Button
          kind="primary"
          size="sm"
          accent={flagged ? C.ok : C.danger}
          icon={flagged ? "check" : "quote"}
          onClick={onFlag}
        >
          {flagged ? "已列入异常报告" : "列入异常报告"}
        </Button>
      </div>
    </>
  );
}

function SectionHeat({
  report,
  ink,
  mute,
  border,
  dark,
}: {
  report: Report;
  ink: string;
  mute: string;
  border: string;
  dark: boolean;
}) {
  const sects = ["tech", "business", "other"].filter((sec) =>
    report.sections?.some((s) => s.section === sec),
  );
  if (!report.sections?.length || sects.length === 0) return null;
  const cell = (doc: number, sec: string) =>
    report.sections!.find((s) => s.doc === doc && s.section === sec);
  return (
    <div
      style={{
        marginTop: 18,
        padding: "14px 16px",
        borderRadius: 10,
        background: dark ? "rgba(255,255,255,0.025)" : C.paper2,
        border: `1px solid ${border}`,
      }}
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          color: mute,
          letterSpacing: "0.06em",
          textTransform: "uppercase",
          marginBottom: 10,
        }}
      >
        章节热力（各标书各标段的最大跨文档雷同强度）
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: `72px repeat(${sects.length}, 1fr)`,
          gap: 4,
          alignItems: "center",
        }}
      >
        <div />
        {sects.map((sec) => (
          <div key={sec} style={{ textAlign: "center", fontSize: 10.5, color: mute, fontWeight: 600 }}>
            {SECTION_LABEL[sec] ?? sec}
          </div>
        ))}
        {report.docs.map((d, di) => (
          <Fragment key={di}>
            <div style={{ fontSize: 11, fontWeight: 700, color: ink, fontFamily: C.serif, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {TAGS[di] ?? "?"} {d.name.replace(/\.[^.]+$/, "").slice(0, 4)}
            </div>
            {sects.map((sec) => {
              const c = cell(di, sec);
              const v = c?.intensity ?? 0;
              return (
                <div
                  key={sec}
                  title={c ? `${(v * 100).toFixed(0)}% · ${c.matches} 处命中` : "无该标段内容"}
                  style={{
                    aspectRatio: "2.4 / 1",
                    borderRadius: 5,
                    background: c ? severityColor(v, C.okSoft) : dark ? "rgba(255,255,255,0.03)" : C.paper3,
                    color: v >= 0.7 ? "#fff" : ink,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 11,
                    fontWeight: 700,
                    fontFamily: C.mono,
                  }}
                >
                  {c ? (v * 100).toFixed(0) : "—"}
                </div>
              );
            })}
          </Fragment>
        ))}
      </div>
    </div>
  );
}

function SharedTerms({
  report,
  ink,
  mute,
  border,
  dark,
}: {
  report: Report;
  ink: string;
  mute: string;
  border: string;
  dark: boolean;
}) {
  const terms = report.sharedTerms ?? [];
  if (terms.length === 0) return null;
  return (
    <div
      style={{
        marginTop: 14,
        padding: "14px 16px",
        borderRadius: 10,
        background: dark ? "rgba(255,255,255,0.025)" : C.paper2,
        border: `1px solid ${border}`,
      }}
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          color: mute,
          letterSpacing: "0.06em",
          textTransform: "uppercase",
          marginBottom: 10,
        }}
      >
        共有特征词（多份标书共用的罕见词，疑似同源 / 共用笔误）
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
        {terms.slice(0, 20).map((t, i) => (
          <span
            key={i}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              padding: "4px 9px",
              borderRadius: 999,
              background: dark ? "rgba(255,255,255,0.04)" : "#fff",
              border: `1px solid ${border}`,
              fontSize: 11.5,
              color: ink,
            }}
          >
            {t.term}
            <span style={{ display: "inline-flex", gap: 2 }}>
              {t.docs.map((d) => (
                <span
                  key={d}
                  style={{
                    width: 13,
                    height: 13,
                    borderRadius: 3,
                    background: PAL[d] ?? C.brand,
                    color: "#fff",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 8.5,
                    fontWeight: 700,
                    fontFamily: C.serif,
                  }}
                >
                  {TAGS[d] ?? "?"}
                </span>
              ))}
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// mock 模式（浏览器预览演示，保留原设计）
// ─────────────────────────────────────────────────────────────
type Sev = "critical" | "high" | "mid" | "low";
interface MockCl {
  sev: Sev;
  title: string;
  docs: string[];
  colors: string[];
  pct: number;
  tpl?: boolean;
}
const CLUSTERS: MockCl[] = [
  { sev: "critical", title: "§3.2 分层架构 · 完全雷同", docs: ["甲", "乙"], colors: ["#4F58A8", "#0E9A8F"], pct: 100 },
  { sev: "critical", title: "§5.1 服务承诺 · 措辞一字不差", docs: ["甲", "乙"], colors: ["#4F58A8", "#0E9A8F"], pct: 98 },
  { sev: "high", title: "§3.3 安全合规体系", docs: ["甲", "乙"], colors: ["#4F58A8", "#0E9A8F"], pct: 88 },
  { sev: "high", title: "§7.1 项目实施总体计划", docs: ["丙", "丁"], colors: ["#C28430", "#B54545"], pct: 78 },
  { sev: "mid", title: "§4.2 项目管理体系", docs: ["丙", "丁"], colors: ["#C28430", "#B54545"], pct: 64 },
  { sev: "mid", title: "§6 售后服务标准", docs: ["甲", "乙", "丁"], colors: ["#4F58A8", "#0E9A8F", "#B54545"], pct: 58 },
  { sev: "low", title: "§2.1 公司基本介绍", docs: ["甲", "丙"], colors: ["#4F58A8", "#C28430"], pct: 38 },
  { sev: "low", title: "§9 附录 · 资质证书清单", docs: ["甲", "乙", "丙", "丁"], colors: ["#4F58A8", "#0E9A8F", "#C28430", "#B54545"], pct: 32, tpl: true },
];
const SECTIONS = ["§1", "§2", "§3", "§3.2", "§3.3", "§4", "§5", "§5.1", "§6", "§7", "§8", "§9"];
const HEAT = [0.18, 0.26, 0.88, 1.0, 0.92, 0.72, 0.95, 0.98, 0.62, 0.78, 0.42, 0.34];

function MockClusters({ onGo }: { onGo: (s: Screen) => void }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="重复条款聚合"
        sub="市政信息化平台采购 · 共 12 组雷同条款 · 含 2 组围标嫌疑"
        actions={
          <>
            <Button kind="ghost" size="md" icon="filter">
              筛选
            </Button>
            <Button kind="primary" size="md" icon="download" onClick={() => onGo("export")}>
              导出 Excel
            </Button>
          </>
        }
      />
      <div style={{ flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "300px 1fr", gap: 16, padding: 20, overflow: "hidden" }}>
        <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          <div style={{ padding: "12px 14px", borderBottom: `1px solid ${border}`, display: "flex", alignItems: "center" }}>
            <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>聚合条款</span>
            <div style={{ flex: 1 }} />
            <Pill bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} fg={mute} size={10}>
              12 组
            </Pill>
          </div>
          <div style={{ flex: 1, overflow: "auto" }}>
            {CLUSTERS.map((c, i) => (
              <MockClusterItem key={i} c={c} active={i === 0} ink={ink} border={border} accent={accent} />
            ))}
          </div>
        </div>
        <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          <div style={{ padding: "16px 22px", borderBottom: `1px solid ${border}` }}>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <Pill bg={C.dangerSoft} fg={C.danger} size={11}>
                <Icon name="info" size={10} />
                围标嫌疑
              </Pill>
              <span style={{ fontSize: 11.5, color: mute }}>聚合 #01 · 出现于 2 份标书</span>
              <div style={{ flex: 1 }} />
              <span style={{ fontSize: 15, fontWeight: 700, color: C.danger, fontFamily: C.mono }}>91%</span>
            </div>
            <div style={{ fontSize: 18, fontWeight: 700, color: ink, marginTop: 8, letterSpacing: "-0.012em", fontFamily: C.serif }}>§3.2 总体架构 · 分层解耦与 API 网关</div>
          </div>
          <div style={{ flex: 1, overflow: "auto", padding: 22 }}>
            <div style={{ padding: "14px 18px", borderRadius: 10, background: dark ? "rgba(255,255,255,0.025)" : C.paper2, border: `1px solid ${border}` }}>
              <div style={{ fontSize: 10, fontWeight: 700, color: mute, letterSpacing: "0.06em", textTransform: "uppercase", marginBottom: 7 }}>归一化文本（双方共享）</div>
              <div style={{ fontSize: 13.5, lineHeight: 1.85, color: ink }}>
                “系统自下而上划分为
                <u style={{ textDecorationColor: C.hi4, textDecorationThickness: 2, textUnderlineOffset: 3 }}>基础设施层、数据资源层、应用支撑层与业务应用层</u>
                ，各层之间通过标准化接口解耦，所有业务能力对外以 <u style={{ textDecorationColor: C.hi4, textDecorationThickness: 2, textUnderlineOffset: 3 }}>API 网关统一暴露</u>，确保横向可扩展与纵向可演进。”
              </div>
            </div>
            <div style={{ marginTop: 14, padding: "12px 14px", borderRadius: 8, background: dark ? "rgba(181,69,69,0.10)" : C.dangerSoft, border: `1px solid ${dark ? "rgba(181,69,69,0.3)" : "#E8C7C7"}` }}>
              <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
                <Icon name="info" size={13} style={{ color: C.danger }} />
                <span style={{ fontSize: 11.5, fontWeight: 700, color: C.danger }}>为何判定为围标嫌疑</span>
              </div>
              <ul style={{ fontSize: 11.5, color: ink, marginTop: 8, paddingLeft: 22, lineHeight: 1.7 }}>
                <li>双方语序、标点、错别字（如「演进」误作「演近」）完全一致</li>
                <li>该段落不属于行业通用模板，与公开模板库相似度低于 12%</li>
                <li>整体技术方案章节相似度 92%，远高于其他 5 对均值 38%</li>
              </ul>
            </div>
            <div style={{ marginTop: 22, padding: "14px 18px", borderRadius: 10, background: dark ? "rgba(255,255,255,0.025)" : C.paper2, border: `1px solid ${border}` }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: "0.06em", textTransform: "uppercase", marginBottom: 10 }}>甲 × 乙 章节相似度热力</div>
              <div style={{ display: "grid", gridTemplateColumns: `repeat(${SECTIONS.length}, 1fr)`, gap: 4 }}>
                {HEAT.map((v, i) => (
                  <div key={i} style={{ aspectRatio: "1/1.4", borderRadius: 4, background: severityColor(v, C.okSoft), display: "flex", alignItems: "center", justifyContent: "center", color: v >= 0.7 ? "#fff" : ink, fontSize: 10.5, fontWeight: 700, fontFamily: C.mono }}>
                    {(v * 100).toFixed(0)}
                  </div>
                ))}
              </div>
              <div style={{ display: "grid", gridTemplateColumns: `repeat(${SECTIONS.length}, 1fr)`, gap: 4, marginTop: 6 }}>
                {SECTIONS.map((sname, i) => (
                  <div key={i} style={{ textAlign: "center", fontSize: 9.5, color: mute, fontFamily: C.mono }}>
                    {sname}
                  </div>
                ))}
              </div>
            </div>
          </div>
          <div style={{ padding: "12px 22px", borderTop: `1px solid ${border}`, display: "flex", alignItems: "center", gap: 9, background: dark ? "rgba(255,255,255,0.02)" : C.paper2 }}>
            <Button kind="secondary" size="sm" icon="diff" onClick={() => onGo("compare")}>
              查看 甲 × 乙 逐段对比
            </Button>
            <div style={{ flex: 1 }} />
            <Button kind="primary" size="sm" accent={C.danger} icon="quote">
              列入异常报告
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function MockClusterItem({ c, active, ink, border, accent }: { c: MockCl; active: boolean; ink: string; border: string; accent: string }) {
  const { dark } = useTheme();
  const sevColor = c.sev === "critical" ? C.danger : c.sev === "high" ? C.hi3 : c.sev === "mid" ? C.hi2 : C.hi1;
  const sevLabel = c.sev === "critical" ? "围标嫌疑" : c.sev === "high" ? "高相似" : c.sev === "mid" ? "中相似" : c.tpl ? "通用模板" : "低相似";
  return (
    <div style={{ padding: "12px 14px", borderBottom: `1px solid ${border}`, background: active ? (dark ? "rgba(255,255,255,0.06)" : C.brandSoft) : "transparent", position: "relative", cursor: "pointer" }}>
      {active && <div style={{ position: "absolute", left: 0, top: 8, bottom: 8, width: 2.5, background: accent, borderRadius: 2 }} />}
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ width: 6, height: 6, borderRadius: "50%", background: sevColor }} />
        <span style={{ fontSize: 12, fontWeight: active ? 700 : 600, color: ink, flex: 1, lineHeight: 1.35 }}>{c.title}</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 7, marginTop: 7, paddingLeft: 13 }}>
        <div style={{ display: "flex", gap: 2 }}>
          {c.docs.map((d, i) => (
            <div key={i} style={{ width: 16, height: 16, borderRadius: 3, background: c.colors[i], color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 9.5, fontWeight: 700, fontFamily: C.serif }}>
              {d}
            </div>
          ))}
        </div>
        <Pill bg={c.sev === "critical" ? C.dangerSoft : C.paper2} fg={sevColor} size={10}>
          {sevLabel}
        </Pill>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11.5, fontWeight: 700, color: sevColor, fontFamily: C.mono }}>{c.pct}%</span>
      </div>
    </div>
  );
}
