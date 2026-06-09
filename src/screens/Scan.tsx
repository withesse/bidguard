// 屏 3 · 检测中 —— 真实模式：Channel 进度 + 真实文件驱动；mock 模式：原动画演示。
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill } from "../components/primitives";
import { BuildingMatrix } from "../components/Matrix";
import { useTheme } from "../theme";
import type { Screen } from "../routes";
import type { Progress } from "../engine";

export function Scan({
  onGo,
  progress,
  files,
  semantic = false,
}: {
  onGo: (s: Screen) => void;
  progress?: Progress | null;
  files?: string[];
  semantic?: boolean;
}) {
  if (files && files.length)
    return <RealScan onGo={onGo} progress={progress} files={files} semantic={semantic} />;
  return <MockScan onGo={onGo} />;
}

// ─────────────────────────────────────────────────────────────
// 真实模式
// ─────────────────────────────────────────────────────────────
const STAGE_LABEL: Record<string, string> = {
  parse: "解析文档 + 分词分段",
  semantic: "语义比对 · AI 向量",
  compare: "两两段落对齐比对",
  cluster: "聚合 & 围标识别",
  done: "完成",
};
const stageList = (semantic: boolean) =>
  semantic ? ["parse", "semantic", "compare", "cluster"] : ["parse", "compare", "cluster"];

function overallPct(p: Progress | null | undefined, semantic: boolean): number {
  if (!p) return 2;
  const within = p.total > 0 ? p.done / p.total : 0;
  if (p.stage === "done") return 100;
  if (p.stage === "cluster") return 90 + within * 8;
  if (semantic) {
    if (p.stage === "parse") return within * 20;
    if (p.stage === "semantic") return 20 + within * 20;
    if (p.stage === "compare") return 40 + within * 50;
  } else {
    if (p.stage === "parse") return within * 35;
    if (p.stage === "compare") return 35 + within * 55;
  }
  return 30;
}

function RealScan({
  onGo,
  progress,
  files,
  semantic,
}: {
  onGo: (s: Screen) => void;
  progress?: Progress | null;
  files: string[];
  semantic: boolean;
}) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const TAGS = ["甲", "乙", "丙", "丁", "戊"];
  const PAL = ["#4F58A8", "#0E9A8F", "#C28430", "#B54545", "#7C3AED"];

  const STAGES = stageList(semantic);
  const pct = Math.round(overallPct(progress, semantic));
  const cur = STAGES.indexOf(progress?.stage ?? "parse");
  const parsedDocs = progress?.stage === "parse" ? progress.done : files.length;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="正在比对"
        sub={`${files.length} 份标书 · 本地完成`}
        actions={
          <Button kind="secondary" size="md" onClick={() => onGo("home")}>
            取消
          </Button>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "24px 48px 40px" }}>
        <div style={{ maxWidth: 760, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
          {/* 文件读取 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 14 }}>各份标书读取</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
              {files.map((f, i) => {
                const done = i < parsedDocs || cur > 0;
                return (
                  <div key={i} style={{ display: "flex", alignItems: "center", gap: 9 }}>
                    <div style={{ width: 18, height: 18, borderRadius: 5, background: PAL[i] ?? C.brand, color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 10, fontWeight: 700, fontFamily: C.serif }}>
                      {TAGS[i] ?? "?"}
                    </div>
                    <span style={{ fontSize: 12, color: ink, fontWeight: 500, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f}</span>
                    <span style={{ fontSize: 11.5, color: done ? C.ok : mute, fontWeight: 700, fontFamily: C.mono }}>{done ? "✓" : "…"}</span>
                  </div>
                );
              })}
            </div>
          </div>

          {/* 处理阶段 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>处理阶段</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {STAGES.map((stage, i) => {
                const status = cur > i ? "done" : cur === i ? "running" : "pending";
                return (
                  <div key={stage} style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 12px", borderRadius: 8, background: dark ? "rgba(255,255,255,0.02)" : "#fff", border: `1px solid ${border}` }}>
                    <div style={{ width: 16, height: 16, borderRadius: "50%", flexShrink: 0, background: status === "done" ? C.ok : status === "running" ? `${accent}22` : "transparent", border: status === "done" ? "none" : `1.5px solid ${status === "running" ? accent : dark ? "rgba(255,255,255,0.2)" : C.ink5}`, display: "flex", alignItems: "center", justifyContent: "center" }}>
                      {status === "done" && <Icon name="check" size={10} style={{ color: "#fff" }} strokeWidth={2.5} />}
                      {status === "running" && <span style={{ width: 7, height: 7, borderRadius: "50%", background: accent, animation: "cpulse 1.4s ease-in-out infinite" }} />}
                    </div>
                    <span style={{ fontSize: 12.5, fontWeight: 600, color: status === "pending" ? mute : ink, flex: 1 }}>{STAGE_LABEL[stage]}</span>
                    {status === "running" && progress && (
                      <span style={{ fontSize: 11, color: accent, fontFamily: C.mono, fontWeight: 700 }}>{progress.note}</span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* 总进度 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: "16px 22px", display: "flex", alignItems: "center", gap: 22 }}>
            <div>
              <div style={{ fontSize: 10.5, color: mute, letterSpacing: "0.06em", textTransform: "uppercase", fontWeight: 700 }}>总体进度</div>
              <div style={{ fontSize: 22, fontWeight: 700, color: ink, marginTop: 3, fontFamily: C.font, letterSpacing: "-0.014em" }}>
                {pct}
                <span style={{ fontSize: 12, color: mute, fontWeight: 500 }}>%</span>
              </div>
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ height: 4, background: dark ? "rgba(255,255,255,0.08)" : C.paper2, borderRadius: 2, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${pct}%`, background: accent, transition: "width 0.3s" }} />
              </div>
              <div style={{ marginTop: 8, fontSize: 11, color: mute }}>{progress?.note ?? "准备中…"}</div>
            </div>
            <Pill bg={C.brandSoft} fg={accent} size={11}>
              <Icon name="lock" size={10} />
              本地比对
            </Pill>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// mock 模式（浏览器预览演示）
// ─────────────────────────────────────────────────────────────
const BUILDING_MATRIX: (number | null)[][] = [
  [null, 0.92, 0.34, 0.42],
  [0.92, null, 0.31, null],
  [0.34, 0.31, null, null],
  [0.42, null, null, null],
];
const DOC_PROGRESS = [
  { tag: "甲", name: "智慧城邦科技_技术响应文件.pdf", pct: 100, color: "#4F58A8" },
  { tag: "乙", name: "启明信息_投标文件_技术标.docx", pct: 100, color: "#0E9A8F" },
  { tag: "丙", name: "鸿信科技_市政平台投标书.pdf", pct: 84, color: "#C28430" },
  { tag: "丁", name: "蓝信电子_技术标响应.docx", pct: 38, color: "#B54545" },
];
const STAGES: { label: string; status: "done" | "running" | "pending"; pct?: number }[] = [
  { label: "解析文档", status: "done" },
  { label: "段落语义化", status: "done" },
  { label: "两两交叉比对", status: "running", pct: 62 },
  { label: "聚合 & 围标识别", status: "pending" },
];
const FINDINGS = [
  { pair: "甲 × 乙", text: "§3 技术方案 高度同源", pct: 92, sev: "high" },
  { pair: "甲 × 乙", text: "§5 服务承诺 措辞完全一致", pct: 88, sev: "high" },
  { pair: "丙 × 丁", text: "§7 实施计划 模板雷同", pct: 68, sev: "mid" },
];

function MockScan({ onGo }: { onGo: (s: Screen) => void }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="正在比对"
        sub="市政信息化平台采购 · 5 家供应商围标核查"
        actions={
          <>
            <Button kind="ghost" size="md">
              暂停
            </Button>
            <Button kind="secondary" size="md" onClick={() => onGo("tasks")}>
              取消
            </Button>
          </>
        }
      />
      <div style={{ flex: 1, overflow: "auto", padding: "24px 48px 40px" }}>
        <div style={{ maxWidth: 1080, margin: "0 auto", display: "grid", gridTemplateColumns: "1.3fr 1fr", gap: 16 }}>
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 16 }}>各份标书读取进度</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              {DOC_PROGRESS.map((d, i) => (
                <div key={i}>
                  <div style={{ display: "flex", alignItems: "center", gap: 9, marginBottom: 6 }}>
                    <div style={{ width: 18, height: 18, borderRadius: 5, background: d.color, color: "#fff", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 10, fontWeight: 700, fontFamily: C.serif }}>{d.tag}</div>
                    <span style={{ fontSize: 12, color: ink, fontWeight: 500, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{d.name}</span>
                    <span style={{ fontSize: 11.5, color: d.pct === 100 ? C.ok : ink, fontWeight: 700, fontFamily: C.mono, minWidth: 36, textAlign: "right" }}>{d.pct === 100 ? "✓" : `${d.pct}%`}</span>
                  </div>
                  <div style={{ height: 4, background: dark ? "rgba(255,255,255,0.05)" : C.paper2, borderRadius: 2, overflow: "hidden" }}>
                    <div style={{ height: "100%", width: `${d.pct}%`, background: d.pct === 100 ? C.ok : accent, transition: "width 0.4s" }} />
                  </div>
                </div>
              ))}
            </div>
            <div style={{ marginTop: 22, paddingTop: 18, borderTop: `1px solid ${border}` }}>
              <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>处理阶段</div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                {STAGES.map((s, i) => (
                  <ScanStep key={i} s={s} />
                ))}
              </div>
            </div>
          </div>
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ display: "flex", alignItems: "baseline" }}>
              <span style={{ fontSize: 13, fontWeight: 700, color: ink }}>相似度矩阵 · 正在生成</span>
              <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>已完成 4 / 6 对</span>
            </div>
            <div style={{ marginTop: 16 }}>
              <BuildingMatrix m={BUILDING_MATRIX} />
            </div>
            <div style={{ marginTop: 16, padding: "12px 14px", borderRadius: 10, background: dark ? "rgba(255,255,255,0.025)" : C.paper2, border: `1px solid ${border}` }}>
              <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: "0.06em", textTransform: "uppercase", marginBottom: 8 }}>已发现 · 14 处提示</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                {FINDINGS.map((f, i) => (
                  <div key={i} style={{ display: "flex", alignItems: "center", gap: 9, fontSize: 11.5 }}>
                    <span style={{ color: mute, fontFamily: C.serif, fontWeight: 700, width: 60, flexShrink: 0 }}>{f.pair}</span>
                    <span style={{ color: ink, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f.text}</span>
                    <span style={{ fontSize: 12, fontWeight: 700, color: f.sev === "high" ? C.hi3 : C.hi2, fontFamily: C.mono }}>{f.pct}%</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ScanStep({ s }: { s: { label: string; status: "done" | "running" | "pending"; pct?: number } }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const done = s.status === "done";
  const running = s.status === "running";
  return (
    <div style={{ padding: 12, borderRadius: 8, background: dark ? "rgba(255,255,255,0.02)" : "#fff", border: `1px solid ${border}` }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <div style={{ width: 16, height: 16, borderRadius: "50%", flexShrink: 0, background: done ? C.ok : running ? `${accent}22` : "transparent", border: done ? "none" : `1.5px solid ${running ? accent : dark ? "rgba(255,255,255,0.2)" : C.ink5}`, display: "flex", alignItems: "center", justifyContent: "center" }}>
          {done && <Icon name="check" size={10} style={{ color: "#fff" }} strokeWidth={2.5} />}
          {running && <span style={{ width: 7, height: 7, borderRadius: "50%", background: accent, animation: "cpulse 1.4s ease-in-out infinite" }} />}
        </div>
        <span style={{ fontSize: 11.5, fontWeight: 600, color: done ? mute : ink, flex: 1 }}>{s.label}</span>
        {running && <span style={{ fontSize: 11, color: accent, fontFamily: C.mono, fontWeight: 700 }}>{s.pct}%</span>}
      </div>
    </div>
  );
}
