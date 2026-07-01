// 屏 7 · 导出报告 —— 从 DB 装配（含八类统计/事实冲突/配置快照）。
// 预览为真实任务概要；导出选项（正文全文 / 配置快照）按 per-export 覆盖传给后端。
import { useState, type ReactNode } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { C } from "../design/tokens";
import { Topbar } from "../components/Topbar";
import { Button, DocChip, Pill, Toggle } from "../components/primitives";
import { useTheme } from "../theme";
import { errMsg, isTauri } from "../api/client";
import { exportReport as exportReportV2 } from "../api";
import { useAppSettings, useCompareSummary } from "../queries/data";
import { useToast } from "../components/Toast";
import { typeUi } from "../utils/clusterUi";
import { docColor, docTag } from "../utils/docTag";
import { formatDateTime } from "../utils/formatTime";

const FORMATS: { t: string; label: string; sub: string; kind: string; ext: string }[] = [
  { t: "html", label: "网页 / PDF", sub: "浏览器可打印为 PDF", kind: "html", ext: "html" },
  { t: "docx", label: "Word", sub: "可继续编辑", kind: "docx", ext: "docx" },
  { t: "xls", label: "Excel", sub: "矩阵 + 明细 + 冲突", kind: "xlsx", ext: "xlsx" },
  { t: "json", label: "JSON", sub: "系统集成 / 二次处理", kind: "json", ext: "json" },
  { t: "md", label: "Markdown", sub: "文本归档 / 知识库", kind: "markdown", ext: "md" },
  { t: "csv", label: "CSV", sub: "条款明细表格化", kind: "csv", ext: "csv" },
];

// 报告固定包含的章节（始终生成，不可单独关闭——只作信息展示）。
const SECTIONS = [
  "封面 + 评审摘要",
  "N × N 相似度矩阵 + 章节热力",
  "围标嫌疑结论与证据链",
  "重复条款明细（按八类分组）",
  "事实冲突明细（金额/工期/日期）",
  "逐对左右对比快照",
];

export function Export({ jobId }: { jobId?: string }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const toast = useToast();

  // 默认格式与导出选项来自用户全局设置（export.*），未设置时取合理默认
  const { data: appCfg } = useAppSettings();
  const exportCfg =
    ((appCfg as Record<string, Record<string, unknown>> | undefined)?.export) ?? {};
  const defaultKind = (exportCfg.defaultFormat as string) ?? "html";
  const [fmt, setFmt] = useState(() =>
    Math.max(0, FORMATS.findIndex((f) => f.kind === defaultKind)),
  );
  const [includeRawText, setIncludeRawText] = useState<boolean>(
    (exportCfg.includeRawText as boolean) ?? true,
  );
  const [includeConfig, setIncludeConfig] = useState<boolean>(
    (exportCfg.includeConfig as boolean) ?? true,
  );
  const [lastExport, setLastExport] = useState<{ path: string; label: string } | null>(null);

  const onExport = async () => {
    if (!isTauri()) {
      toast.show("导出仅在桌面应用内可用", "warn");
      return;
    }
    if (!jobId) {
      toast.show("请先在应用内完成一次查重，再导出报告", "warn");
      return;
    }
    const f = FORMATS[fmt];
    try {
      const path = await save({
        title: `导出${f.label}报告`,
        defaultPath: `标书查重报告.${f.ext}`,
        filters: [{ name: f.label, extensions: [f.ext] }],
      });
      if (!path) return;
      await exportReportV2(jobId, f.kind, path, { includeRawText, includeConfig });
      setLastExport({ path, label: f.label });
      toast.show(`已导出 ${f.label} 报告`, "success");
    } catch (e) {
      toast.show("导出失败：" + errMsg(e), "error");
    }
  };

  const openExported = async () => {
    if (!lastExport) return;
    try {
      const { openPath } = await import("@tauri-apps/plugin-opener");
      await openPath(lastExport.path);
    } catch (e) {
      toast.show("打开失败：" + errMsg(e), "error");
    }
  };
  const revealExported = async () => {
    if (!lastExport) return;
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(lastExport.path);
    } catch (e) {
      toast.show("定位失败：" + errMsg(e), "error");
    }
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="导出报告"
        sub="选择格式与选项，导出本地查重报告（含八类统计与事实冲突）"
        actions={
          <Button kind="primary" size="md" icon="download" onClick={onExport}>
            立即导出
          </Button>
        }
      />
      {lastExport && (
        <div
          style={{
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "8px 48px",
            borderBottom: `1px solid ${border}`,
            fontSize: 12,
            color: mute,
          }}
        >
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }}>
            已导出 {lastExport.label} 报告：{lastExport.path}
          </span>
          <Button kind="secondary" size="sm" onClick={openExported}>
            打开
          </Button>
          <Button kind="ghost" size="sm" onClick={revealExported}>
            在文件夹中显示
          </Button>
        </div>
      )}
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 1100, margin: "0 auto", display: "grid", gridTemplateColumns: "420px 1fr", gap: 18 }}>
          {/* 左：选项 */}
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <CardLabel mute={mute}>文件格式</CardLabel>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 8 }}>
                {FORMATS.map((o, i) => {
                  const active = i === fmt;
                  return (
                    <div
                      key={i}
                      role="button"
                      tabIndex={0}
                      onClick={() => setFmt(i)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          setFmt(i);
                        }
                      }}
                      style={{
                        padding: 12,
                        borderRadius: 8,
                        cursor: "pointer",
                        border: `1.5px solid ${active ? accent : border}`,
                        background: active
                          ? dark
                            ? "rgba(79,88,168,0.10)"
                            : `${accent}10`
                          : dark
                            ? "rgba(255,255,255,0.02)"
                            : "#fff",
                      }}
                    >
                      <DocChip type={o.t === "xls" ? "xls" : o.t} />
                      <div style={{ fontSize: 12, fontWeight: 600, color: ink, marginTop: 8 }}>{o.label}</div>
                      <div style={{ fontSize: 10.5, color: mute, marginTop: 2 }}>{o.sub}</div>
                    </div>
                  );
                })}
              </div>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <CardLabel mute={mute}>导出选项</CardLabel>
              <Row label="包含条款正文全文" sub="关闭则正文截为前 40 字摘要（保留定位，不含全文）" ink={ink} mute={mute}>
                <Toggle on={includeRawText} onChange={() => setIncludeRawText((v) => !v)} />
              </Row>
              <Row label="附比对配置快照" sub="报告末尾附本次比对的参数（阈值/范围/模型等）" ink={ink} mute={mute} last>
                <Toggle on={includeConfig} onChange={() => setIncludeConfig((v) => !v)} />
              </Row>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              {FORMATS[fmt].kind === "csv" || FORMATS[fmt].kind === "json" ? (
                <>
                  <CardLabel mute={mute}>{FORMATS[fmt].label} 内容</CardLabel>
                  <div style={{ fontSize: 12, color: dark ? "rgba(255,255,255,0.8)" : C.ink2, lineHeight: 1.7 }}>
                    {FORMATS[fmt].kind === "csv"
                      ? "扁平表格：每行一条雷同 / 冲突条款（文档、相似度、类型、章节、正文摘要）。不含封面、矩阵图与热力图。"
                      : "结构化数据：矩阵、条款、事实冲突、围标信号与配置快照的完整 JSON，供二次处理。不含渲染版式。"}
                  </div>
                </>
              ) : (
                <>
                  <CardLabel mute={mute}>报告包含（固定章节）</CardLabel>
                  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                    {SECTIONS.map((label, i) => (
                      <div key={i} style={{ display: "flex", alignItems: "center", gap: 9 }}>
                        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" style={{ flexShrink: 0 }}>
                          <path d="M3 7.5l2.5 2.5L11 4.5" stroke={accent} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                        <span style={{ fontSize: 12, color: dark ? "rgba(255,255,255,0.8)" : C.ink2 }}>{label}</span>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          </div>

          {/* 右：真实报告概要 */}
          <ReportPreview jobId={jobId} accent={accent} mute={mute} dark={dark} border={border} />
        </div>
      </div>
    </div>
  );
}

const COLLUSION: Record<string, { label: string; danger?: boolean }> = {
  high: { label: "围标嫌疑（高）", danger: true },
  medium: { label: "疑似围标（中）", danger: true },
  low: { label: "弱信号（低）" },
  none: { label: "未发现围标信号" },
};

function ReportPreview({
  jobId,
  accent,
  mute,
  dark,
  border,
}: {
  jobId?: string;
  accent: string;
  mute: string;
  dark: boolean;
  border: string;
}) {
  const { data } = useCompareSummary(jobId);
  const panel = {
    background: dark ? "#15151B" : "#E8E5DE",
    borderRadius: 12,
    border: `1px solid ${border}`,
    padding: 24,
    display: "flex",
    flexDirection: "column" as const,
    alignItems: "center",
    gap: 16,
    overflow: "auto",
  };

  if (!jobId || !data) {
    return (
      <div style={{ ...panel, justifyContent: "center", color: mute, fontSize: 12.5 }}>
        完成一次查重后，此处显示将导出的报告概要。
      </div>
    );
  }

  const s = data.summary;
  const docs = data.documents ?? [];
  const peak = data.matrix?.peak ?? 0;
  const level = (data.job.collusionLevel as string | null) ?? "none";
  const col = COLLUSION[level] ?? COLLUSION.none;
  const pairs = docs.length >= 2 ? (docs.length * (docs.length - 1)) / 2 : 0;
  const counts: Array<[string, number]> = s
    ? [
        ["conflict", s.conflictCount],
        ["same", s.sameCount],
        ["minor_change", s.minorChangeCount],
        ["changed", s.changedCount],
        ["rewrite", s.rewriteCount],
        ["uncertain", s.uncertainCount],
        ["added", s.addedCount],
        ["deleted", s.deletedCount],
      ]
    : [];

  return (
    <div style={panel}>
      <div style={{ fontSize: 11.5, color: mute }}>报告概要 · 真实数据</div>
      <div
        style={{
          width: 380,
          padding: "30px 34px 32px",
          background: "#fff",
          boxShadow: "0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)",
          fontFamily: C.font,
          color: "#16161B",
        }}
      >
        <div style={{ borderTop: `4px solid ${accent}`, paddingTop: 16 }}>
          <div style={{ fontSize: 9.5, color: "#6B6B76", letterSpacing: "0.08em", textTransform: "uppercase", fontWeight: 700 }}>
            原本 · 标书查重评审报告
          </div>
          <div
            style={{
              fontSize: 19,
              fontWeight: 700,
              color: "#16161B",
              marginTop: 8,
              letterSpacing: "-0.014em",
              lineHeight: 1.25,
              fontFamily: C.serif,
            }}
          >
            {data.job.name || "标书交叉查重"}
          </div>
          <div style={{ fontSize: 10, color: "#6B6B76", marginTop: 8, lineHeight: 1.6 }}>
            {docs.length} 份标书 · {pairs} 组比对
            {data.job.finishedAt && <> · 生成于 {formatDateTime(data.job.finishedAt)}</>}
          </div>
        </div>

        {/* 关键结论 */}
        <div
          style={{
            marginTop: 16,
            padding: "12px 14px",
            borderRadius: 8,
            background: col.danger ? C.dangerSoft : "#F4F2EB",
            border: `1px solid ${col.danger ? "#E8C7C7" : "#E3E0D7"}`,
          }}
        >
          <div style={{ fontSize: 9, fontWeight: 700, color: col.danger ? C.danger : "#6B6B76", letterSpacing: "0.06em", textTransform: "uppercase" }}>
            围标判定
          </div>
          <div style={{ fontSize: 12.5, fontWeight: 700, color: "#16161B", marginTop: 4 }}>{col.label}</div>
        </div>

        {/* 关键指标 */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr 1fr", gap: 10, marginTop: 16 }}>
          {[
            { l: "参评标书", v: String(docs.length) },
            { l: "重复条款", v: String(s?.clusterCount ?? 0) },
            { l: "高风险", v: String(s?.highRiskCount ?? 0), danger: (s?.highRiskCount ?? 0) > 0 },
            { l: "峰值相似", v: `${Math.round(peak * 100)}%` },
          ].map((m, i) => (
            <div key={i}>
              <div style={{ fontSize: 8, fontWeight: 700, color: "#6B6B76", letterSpacing: "0.06em", textTransform: "uppercase" }}>
                {m.l}
              </div>
              <div style={{ fontSize: 17, fontWeight: 700, color: m.danger ? C.danger : "#16161B", marginTop: 3, letterSpacing: "-0.014em" }}>
                {m.v}
              </div>
            </div>
          ))}
        </div>

        {/* 八类分布 */}
        {counts.some(([, n]) => n > 0) && (
          <div style={{ display: "flex", flexWrap: "wrap", gap: 5, marginTop: 16 }}>
            {counts
              .filter(([, n]) => n > 0)
              .map(([k, n]) => {
                const t = typeUi(k);
                return (
                  <Pill key={k} fg={t.fg} bg={t.bg} size={9.5}>
                    {t.label} {n}
                  </Pill>
                );
              })}
          </div>
        )}

        {/* 文档清单 */}
        <div style={{ marginTop: 16, borderTop: "1px solid #ECE9E1", paddingTop: 12 }}>
          <div style={{ fontSize: 9, fontWeight: 700, color: "#6B6B76", letterSpacing: "0.06em", textTransform: "uppercase", marginBottom: 8 }}>
            参评标书
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {docs.map((d, i) => (
              <div key={d.id} style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span
                  style={{
                    width: 16,
                    height: 16,
                    borderRadius: 4,
                    background: docColor(i),
                    color: "#fff",
                    fontSize: 9.5,
                    fontWeight: 700,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    flexShrink: 0,
                    fontFamily: C.serif,
                  }}
                >
                  {docTag(i)}
                </span>
                <span style={{ fontSize: 11, color: "#3A3A44", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {d.fileName}
                </span>
              </div>
            ))}
          </div>
        </div>

        {s?.semanticDegraded && (
          <div style={{ fontSize: 9.5, color: "#8a6d3b", marginTop: 12 }}>
            注：本次语义模型不可用，已降级为词面比对。
          </div>
        )}
      </div>
    </div>
  );
}

function CardLabel({ children, mute }: { children: ReactNode; mute: string }) {
  return (
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
      {children}
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
