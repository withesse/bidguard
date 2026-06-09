// 屏 7 · 导出报告 —— 移植自 app-design/project/src/c/bid-c.jsx (BidScrExport)
import { Fragment, useState, type ReactNode } from "react";
import { C, severityColor } from "../design/tokens";
import { Topbar } from "../components/Topbar";
import { Button, DocChip, Toggle } from "../components/primitives";
import { useTheme } from "../theme";
import { isTauri, exportReport, type ExportKind, type Report } from "../engine";
import { useToast } from "../components/Toast";

const FORMATS: { t: string; label: string; sub: string; kind: ExportKind }[] = [
  { t: "html", label: "网页 / PDF", sub: "浏览器可打印为 PDF", kind: "html" },
  { t: "docx", label: "Word", sub: "可继续编辑", kind: "docx" },
  { t: "xls", label: "Excel", sub: "矩阵 + 明细", kind: "xlsx" },
];

const INCLUDE = [
  "封面 + 评审摘要",
  "N × N 相似度矩阵",
  "围标嫌疑结论与证据链",
  "逐对左右对比快照",
  "重复条款明细清单（全部 12 组）",
  "章节级热力图",
  "工商关联辅助参考（若启用）",
];

export function Export({ report }: { report?: Report | null }) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const toast = useToast();

  const [fmt, setFmt] = useState(0);
  const [checks, setChecks] = useState<boolean[]>(INCLUDE.map((_, i) => i < 6));
  const [security, setSecurity] = useState<boolean[]>([true, true, true]);

  const toggleCheck = (i: number) => setChecks((c) => c.map((v, j) => (j === i ? !v : v)));
  const toggleSec = (i: number) => setSecurity((s) => s.map((v, j) => (j === i ? !v : v)));

  const onExport = async () => {
    if (!report || !isTauri()) {
      toast.show("请先在应用内完成一次查重，再导出报告", "warn");
      return;
    }
    const f = FORMATS[fmt];
    try {
      const p = await exportReport(report, f.kind);
      if (p) toast.show(`已导出 ${f.label} 报告：${p}`, "success");
    } catch (e) {
      toast.show("导出失败：" + String(e), "error");
    }
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="导出报告"
        sub="市政信息化平台采购 · 4 份标书 · 6 对比对"
        actions={
          <Button
            kind="primary"
            size="md"
            icon="download"
            onClick={onExport}
          >
            立即导出
          </Button>
        }
      />
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
                      onClick={() => setFmt(i)}
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
              <CardLabel mute={mute}>包含内容</CardLabel>
              <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                {INCLUDE.map((label, i) => (
                  <Check key={i} label={label} checked={checks[i]} onClick={() => toggleCheck(i)} />
                ))}
              </div>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <CardLabel mute={mute}>安全与签发</CardLabel>
              <Row label="添加评审水印" sub="评审人 · 时间 · 文件编号" ink={ink} mute={mute}>
                <Toggle on={security[0]} onChange={() => toggleSec(0)} />
              </Row>
              <Row label="文件密码保护" sub="打开报告时需输入密码" ink={ink} mute={mute}>
                <Toggle on={security[1]} onChange={() => toggleSec(1)} />
              </Row>
              <Row label="附带源文件清单" sub="不包含标书原文" ink={ink} mute={mute} last>
                <Toggle on={security[2]} onChange={() => toggleSec(2)} />
              </Row>
            </div>
          </div>

          {/* 右：预览 */}
          <div
            style={{
              background: dark ? "#15151B" : "#E8E5DE",
              borderRadius: 12,
              border: `1px solid ${border}`,
              padding: 24,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 16,
              overflow: "auto",
            }}
          >
            <div style={{ fontSize: 11.5, color: mute }}>报告预览 · 共 32 页</div>
            <ReportPageCover accent={accent} />
            <ReportPageMatrix />
          </div>
        </div>
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

function Check({ checked, label, onClick }: { checked: boolean; label: string; onClick: () => void }) {
  const { dark, accent } = useTheme();
  return (
    <label onClick={onClick} style={{ display: "flex", alignItems: "center", gap: 9, cursor: "pointer" }}>
      <div
        style={{
          width: 14,
          height: 14,
          borderRadius: 4,
          flexShrink: 0,
          border: `1.5px solid ${checked ? accent : dark ? "rgba(255,255,255,0.2)" : C.ink5}`,
          background: checked ? accent : "transparent",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {checked && (
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path d="M2 5l2 2 4-4" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </div>
      <span style={{ fontSize: 12, color: dark ? "rgba(255,255,255,0.85)" : C.ink2 }}>{label}</span>
    </label>
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

function ReportPageCover({ accent }: { accent: string }) {
  return (
    <div
      style={{
        width: 360,
        padding: "34px 38px 36px",
        background: "#fff",
        boxShadow: "0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)",
        fontFamily: C.font,
        color: "#16161B",
      }}
    >
      <div style={{ borderTop: `4px solid ${accent}`, paddingTop: 18 }}>
        <div style={{ fontSize: 9.5, color: "#6B6B76", letterSpacing: "0.08em", textTransform: "uppercase", fontWeight: 700 }}>
          原本 · 标书查重评审报告
        </div>
        <div
          style={{
            fontSize: 22,
            fontWeight: 700,
            color: "#16161B",
            marginTop: 10,
            letterSpacing: "-0.014em",
            lineHeight: 1.2,
            fontFamily: C.serif,
          }}
        >
          市政信息化平台采购
          <br />5 家供应商围标核查
        </div>
        <div style={{ fontSize: 10, color: "#6B6B76", marginTop: 12, lineHeight: 1.6 }}>
          评审编号 · 047 / 2026
          <br />
          生成时间 · 2026-05-26 14:32
        </div>
      </div>
      <div style={{ marginTop: 18, padding: "14px 16px", borderRadius: 8, background: C.dangerSoft, border: "1px solid #E8C7C7" }}>
        <div style={{ fontSize: 9, fontWeight: 700, color: C.danger, letterSpacing: "0.06em", textTransform: "uppercase" }}>
          关键结论
        </div>
        <div style={{ fontSize: 11.5, fontWeight: 700, color: "#16161B", marginTop: 5, lineHeight: 1.5 }}>
          甲、乙两份标书存在 12 组高度雷同条款，整体相似度 92%，其中 2 组属于围标嫌疑，建议依据采购法及实施细则进行处理。
        </div>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr 1fr", gap: 10, marginTop: 16 }}>
        {[
          { l: "参评标书", v: "4" },
          { l: "比对数", v: "6" },
          { l: "雷同条款", v: "12" },
          { l: "围标嫌疑", v: "2", c: C.danger },
        ].map((s, i) => (
          <div key={i}>
            <div style={{ fontSize: 8, fontWeight: 700, color: "#6B6B76", letterSpacing: "0.06em", textTransform: "uppercase" }}>
              {s.l}
            </div>
            <div style={{ fontSize: 18, fontWeight: 700, color: s.c || "#16161B", marginTop: 3, letterSpacing: "-0.014em", fontFamily: C.font }}>
              {s.v}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

const PREVIEW_DOCS = [
  { tag: "甲" },
  { tag: "乙" },
  { tag: "丙" },
  { tag: "丁" },
];
const PREVIEW_MATRIX = [
  [1, 0.92, 0.34, 0.42],
  [0.92, 1, 0.31, 0.4],
  [0.34, 0.31, 1, 0.68],
  [0.42, 0.4, 0.68, 1],
];

function ReportPageMatrix() {
  return (
    <div
      style={{
        width: 360,
        padding: "28px 32px 30px",
        background: "#fff",
        boxShadow: "0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)",
        fontFamily: C.font,
        color: "#16161B",
      }}
    >
      <div style={{ fontSize: 12.5, fontWeight: 700, color: "#16161B", fontFamily: C.serif }}>2. 标书相似度矩阵</div>
      <div style={{ fontSize: 10, color: "#6B6B76", marginTop: 5, lineHeight: 1.6 }}>
        基于语义级段落比对，数值表示两份标书之间的整体相似程度。
      </div>
      <div style={{ marginTop: 14, display: "grid", gridTemplateColumns: "24px repeat(4, 1fr)", gap: 3 }}>
        <div />
        {PREVIEW_DOCS.map((d) => (
          <div key={d.tag} style={{ textAlign: "center", fontSize: 9, fontWeight: 700, color: "#16161B", fontFamily: C.serif }}>
            {d.tag}
          </div>
        ))}
        {PREVIEW_MATRIX.map((row, r) => (
          <Fragment key={r}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "flex-end",
                fontSize: 9,
                fontWeight: 700,
                color: "#16161B",
                fontFamily: C.serif,
              }}
            >
              {PREVIEW_DOCS[r].tag}
            </div>
            {row.map((v, c) => {
              const diag = r === c;
              return (
                <div
                  key={c}
                  style={{
                    aspectRatio: "1.3 / 1",
                    borderRadius: 3,
                    background: diag ? "#F4F2EB" : severityColor(v, C.okSoft),
                    color: !diag && v >= 0.7 ? "#fff" : "#16161B",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 9.5,
                    fontWeight: 700,
                    fontFamily: C.mono,
                  }}
                >
                  {diag ? "—" : (v * 100).toFixed(0)}
                </div>
              );
            })}
          </Fragment>
        ))}
      </div>
      <div style={{ marginTop: 16, fontSize: 10, fontWeight: 700, color: "#16161B", fontFamily: C.serif }}>2.1 主要发现</div>
      <ul style={{ fontSize: 9.5, color: "#3A3A44", paddingLeft: 16, lineHeight: 1.7, marginTop: 4 }}>
        <li>
          <b>甲 × 乙: 92%</b> · 5 个核心章节高度同源，且存在共有错别字、关联工商记录，判定为围标嫌疑
        </li>
        <li>
          <b>丙 × 丁: 68%</b> · 在项目管理与售后服务两节出现模板雷同，但其他章节差异充分
        </li>
        <li>其余 4 组的相似度均在 30%-42% 区间，主要落在通用条款</li>
      </ul>
    </div>
  );
}
