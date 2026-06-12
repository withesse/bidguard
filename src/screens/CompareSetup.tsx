// 比对配置页：导入文档（选择/拖拽，任务化带进度）→ 勾选参评文档（2-10，十天干位次）
// → 检测设置 → 发起比对。文档与解析状态都在 DB 里，刷新/深链接不丢。
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Topbar } from "../components/Topbar";
import { Button, DocChip, Pill, SegControl, Toggle } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg, isTauri } from "../api/client";
import { setWorkspaceSettings } from "../api";
import { pickBidFiles } from "../engine";
import type { DocumentDto } from "../api/types";
import {
  useAppSettings,
  useDocuments,
  useImportDocuments,
  useJobs,
  useRemoveDocument,
  useStartCompare,
  useWorkspace,
} from "../queries/data";
import { useProgressStore } from "../stores/progressStore";
import { docTag } from "../utils/docTag";
import { isJobLive } from "../utils/jobStatus";

const ACCEPT = /\.(docx|pdf|txt|md|xlsx|xls)$/i;
const MAX_PICK = 10;

export function CompareSetup() {
  const { wsId } = useParams<{ wsId: string }>();
  const nav = useNavigate();
  const toast = useToast();
  const { dark } = useTheme();
  const { data: ws } = useWorkspace(wsId);
  const { data: documents } = useDocuments(wsId);
  const { data: jobs } = useJobs(wsId);
  const importDocs = useImportDocuments(wsId!);
  const removeDoc = useRemoveDocument(wsId!);
  const startCompare = useStartCompare(wsId!);
  const progress = useProgressStore((s) => s.progress);

  const { data: cfgRaw } = useAppSettings();
  const [taskName, setTaskName] = useState("");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [baseDocId, setBaseDocId] = useState<string>("");
  const [semantic, setSemantic] = useState(false);
  const [factConflict, setFactConflict] = useState(true);
  const [ignoreTemplates, setIgnoreTemplates] = useState(true);
  const [scopeIdx, setScopeIdx] = useState(0);
  const [levelIdx, setLevelIdx] = useState(1); // section/paragraph/sentence
  const [threshold, setThreshold] = useState(0.7);
  const [cfgApplied, setCfgApplied] = useState(false);

  // 用户全局默认值（DB）就绪后填充一次；之后用户改动优先
  useEffect(() => {
    if (cfgApplied || cfgRaw === undefined) return;
    const cmp =
      cfgRaw && typeof cfgRaw === "object"
        ? ((cfgRaw as Record<string, unknown>).compare as Record<string, unknown> | undefined)
        : undefined;
    if (cmp) {
      if (typeof cmp.enableSemantic === "boolean") setSemantic(cmp.enableSemantic);
      if (typeof cmp.enableFactConflict === "boolean") setFactConflict(cmp.enableFactConflict);
      if (typeof cmp.ignoreTemplates === "boolean") setIgnoreTemplates(cmp.ignoreTemplates);
      if (typeof cmp.similarityThreshold === "number") setThreshold(cmp.similarityThreshold);
      const si = ["full", "tech", "business"].indexOf(String(cmp.scope ?? ""));
      if (si >= 0) setScopeIdx(si);
      const li = ["section", "paragraph", "sentence"].indexOf(String(cmp.defaultChunkLevel ?? ""));
      if (li >= 0) setLevelIdx(li);
    }
    setCfgApplied(true);
  }, [cfgRaw, cfgApplied]);

  const parsed = useMemo(() => (documents ?? []).filter((d) => d.status === "parsed"), [documents]);

  // 首批解析完成后默认全选（≤10）
  useEffect(() => {
    if (chosen.size === 0 && parsed.length > 0) {
      setChosen(new Set(parsed.slice(0, MAX_PICK).map((d) => d.id)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parsed.length]);

  // Tauri 原生拖拽导入
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "drop") {
            const dropped = event.payload.paths.filter((p) => ACCEPT.test(p));
            if (dropped.length) doImport(dropped);
          }
        }),
      )
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wsId]);

  const doImport = (paths: string[]) => {
    importDocs.mutate(paths, {
      onError: (e) => toast.show("导入失败：" + errMsg(e), "error"),
    });
  };

  const onPick = async () => {
    if (!isTauri()) {
      toast.show("文件选择仅在桌面应用内可用", "info");
      return;
    }
    const picked = await pickBidFiles();
    if (picked.length) doImport(picked);
  };

  const toggle = (id: string) => {
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        if (next.size >= MAX_PICK) {
          toast.show(`一次最多比对 ${MAX_PICK} 份标书`, "warn");
          return prev;
        }
        next.add(id);
      }
      return next;
    });
  };

  const onStart = async () => {
    const ids = parsed.filter((d) => chosen.has(d.id)).map((d) => d.id);
    if (ids.length < 2) {
      toast.show("请至少勾选 2 份解析成功的标书", "warn");
      return;
    }
    try {
      const job = await startCompare.mutateAsync({
        documentIds: ids,
        name: taskName.trim() || undefined,
        baseDocumentId: baseDocId && ids.includes(baseDocId) ? baseDocId : undefined,
        chunkLevel: (["section", "paragraph", "sentence"] as const)[levelIdx] ?? "paragraph",
        enableSemantic: semantic,
        enableFactConflict: factConflict,
        ignoreTemplates,
        similarityThreshold: threshold,
        scope: (["full", "tech", "business"] as const)[scopeIdx] ?? "full",
      });
      nav(`/workspace/${wsId}/job/${job.id}/running`);
    } catch (e) {
      toast.show("发起比对失败：" + errMsg(e), "error");
    }
  };

  // 当前检测设置存为工作区层配置（覆盖用户全局，被单次任务设置覆盖）
  const saveAsWorkspaceDefault = async () => {
    if (!wsId) return;
    try {
      const patch = {
        compare: {
          scope: (["full", "tech", "business"] as const)[scopeIdx] ?? "full",
          defaultChunkLevel: (["section", "paragraph", "sentence"] as const)[levelIdx] ?? "paragraph",
          similarityThreshold: threshold,
          enableSemantic: semantic,
          enableFactConflict: factConflict,
          ignoreTemplates,
        },
      };
      await setWorkspaceSettings(wsId, JSON.stringify(patch));
      toast.show("已保存为本工作区默认设置", "success");
    } catch (e) {
      toast.show("保存失败：" + errMsg(e), "error");
    }
  };

  // 进行中的导入任务进度
  const liveImport = (jobs ?? []).find((j) => j.jobType === "import" && isJobLive(j));
  const importProg = liveImport ? progress[liveImport.id] : undefined;

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = dark ? "rgba(255,255,255,0.07)" : C.line;
  // 勾选位次 → 十天干（按解析列表顺序）
  const orderOf = (id: string) =>
    parsed.filter((d) => chosen.has(d.id)).findIndex((d) => d.id === id);

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <Topbar
        title={ws?.name ?? "新建查重"}
        sub="导入 2-10 份标书，勾选后发起交叉比对"
        actions={
          <Button
            kind="primary"
            icon="diff"
            onClick={onStart}
          >
            开始交叉比对（{chosen.size}）
          </Button>
        }
      />
      <div style={{ flex: 1, overflowY: "auto", padding: 24, display: "flex", flexDirection: "column", gap: 18 }}>
        {/* 任务名 */}
        <input
          value={taskName}
          onChange={(e) => setTaskName(e.target.value)}
          placeholder={`任务名称（默认「${chosen.size || "N"} 份标书交叉比对」）`}
          style={{
            background: cardBg,
            border: `1px solid ${border}`,
            borderRadius: 10,
            padding: "10px 14px",
            fontSize: 13,
            color: ink,
            outline: "none",
            fontFamily: C.font,
          }}
        />

        {/* 导入进度 */}
        {liveImport && (
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 10, padding: "10px 14px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: mute, marginBottom: 6 }}>
              <span>正在导入… {importProg?.message ?? ""}</span>
              <span>{Math.round((importProg?.percent ?? 0) * 100)}%</span>
            </div>
            <div style={{ height: 4, background: dark ? "rgba(255,255,255,0.08)" : C.paper2, borderRadius: 2 }}>
              <div
                style={{
                  height: "100%",
                  width: `${Math.round((importProg?.percent ?? 0) * 100)}%`,
                  background: "#4F58A8",
                  borderRadius: 2,
                  transition: "width 0.25s ease",
                }}
              />
            </div>
          </div>
        )}

        {/* 文档卡片 */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
            gap: 12,
          }}
        >
          {(documents ?? []).map((d) => (
            <DocCard
              key={d.id}
              doc={d}
              order={orderOf(d.id)}
              chosen={chosen.has(d.id)}
              onToggle={() => d.status === "parsed" && toggle(d.id)}
              onPreview={() => nav(`/workspace/${wsId}/doc/${d.id}`)}
              onRemove={() =>
                removeDoc.mutate(d.id, {
                  onError: (e) => toast.show("删除失败：" + errMsg(e), "error"),
                })
              }
              onRetry={() =>
                importDocs.mutate([d.filePath], {
                  onError: (e) => toast.show("重试失败：" + errMsg(e), "error"),
                })
              }
            />
          ))}
          <div
            onClick={onPick}
            style={{
              border: `1.5px dashed ${border}`,
              borderRadius: 12,
              minHeight: 92,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: mute,
              fontSize: 12.5,
              cursor: "pointer",
              textAlign: "center",
              padding: 12,
            }}
          >
            ＋ 选择标书文件，或直接拖入窗口
            <br />
          </div>
        </div>

        {/* 检测设置 */}
        <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 16 }}>
          <div style={{ fontSize: 12, fontWeight: 700, color: ink, marginBottom: 12 }}>检测设置</div>
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SettingRow label="比对范围" hint="技术标/商务标按段落关键词识别">
              <SegControl options={["完整标书", "仅技术标", "仅商务标"]} value={scopeIdx < 0 ? 0 : scopeIdx} onChange={setScopeIdx} />
            </SettingRow>
            <SettingRow label="分块粒度" hint="章节粗看 / 段落均衡（推荐）/ 句子精查">
              <SegControl options={["章节", "段落", "句子"]} value={levelIdx} onChange={setLevelIdx} />
            </SettingRow>
            <SettingRow label="基准文档" hint="设定后可识别「基准缺失/独有」内容；不设则各文档平等比对">
              <select
                value={baseDocId}
                onChange={(e) => setBaseDocId(e.target.value)}
                style={{
                  background: cardBg,
                  border: `1px solid ${border}`,
                  borderRadius: 8,
                  padding: "6px 10px",
                  fontSize: 12,
                  color: ink,
                  fontFamily: C.font,
                  maxWidth: 220,
                }}
              >
                <option value="">不设基准</option>
                {parsed
                  .filter((d) => chosen.has(d.id))
                  .map((d, i) => (
                    <option key={d.id} value={d.id}>
                      {docTag(i)} · {d.fileName}
                    </option>
                  ))}
              </select>
            </SettingRow>
            <SettingRow label={`相似度阈值 ${Math.round(threshold * 100)}%`} hint="低于此值的段落对不进入报告">
              <input
                type="range"
                min={20}
                max={95}
                value={Math.round(threshold * 100)}
                onChange={(e) => setThreshold(Number(e.target.value) / 100)}
                style={{ width: 180 }}
              />
            </SettingRow>
            <SettingRow label="语义查重" hint="识别改写式雷同（首次启用需下载模型）">
              <Toggle on={semantic} onChange={() => setSemantic((v) => !v)} />
            </SettingRow>
            <SettingRow label="事实冲突检测" hint="同一条款金额/工期/日期不一致 → 风险标记">
              <Toggle on={factConflict} onChange={() => setFactConflict((v) => !v)} />
            </SettingRow>
            <SettingRow label="忽略通用模板" hint="命中查重源样板的段落不参与比对">
              <Toggle on={ignoreTemplates} onChange={() => setIgnoreTemplates((v) => !v)} />
            </SettingRow>
            <div style={{ display: "flex", justifyContent: "flex-end", paddingTop: 4 }}>
              <Button kind="ghost" size="sm" onClick={saveAsWorkspaceDefault}>
                保存为本工作区默认
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  const { dark } = useTheme();
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 12.5, fontWeight: 600, color: dark ? "#fff" : C.ink }}>{label}</div>
        {hint && (
          <div style={{ fontSize: 11, color: dark ? "rgba(255,255,255,0.45)" : C.ink3, marginTop: 2 }}>{hint}</div>
        )}
      </div>
      {children}
    </div>
  );
}

function DocCard({
  doc,
  order,
  chosen,
  onToggle,
  onPreview,
  onRemove,
  onRetry,
}: {
  doc: DocumentDto;
  order: number;
  chosen: boolean;
  onToggle: () => void;
  onPreview: () => void;
  onRemove: () => void;
  onRetry: () => void;
}) {
  const { dark, accent } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const cardBg = dark ? "#1E1E25" : C.white;
  const border = chosen ? accent : dark ? "rgba(255,255,255,0.07)" : C.line;

  return (
    <div
      onClick={onToggle}
      style={{
        background: cardBg,
        border: `1.5px solid ${border}`,
        borderRadius: 12,
        padding: "12px 14px",
        cursor: doc.status === "parsed" ? "pointer" : "default",
        opacity: doc.status === "failed" ? 0.75 : 1,
        display: "flex",
        flexDirection: "column",
        gap: 7,
        position: "relative",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        {chosen && order >= 0 && (
          <span
            style={{
              width: 20,
              height: 20,
              borderRadius: 6,
              background: accent,
              color: "#fff",
              fontSize: 11,
              fontWeight: 700,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              flexShrink: 0,
            }}
          >
            {docTag(order)}
          </span>
        )}
        <DocChip type={doc.fileType} />
        <div
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12.5,
            fontWeight: 600,
            color: ink,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          title={doc.fileName}
        >
          {doc.fileName}
        </div>
        {doc.status === "parsed" && (
          <span
            onClick={(e) => {
              e.stopPropagation();
              onPreview();
            }}
            style={{ color: "#6B73C9", fontSize: 10.5, fontWeight: 600, padding: 2, flexShrink: 0 }}
            title="预览原文"
          >
            预览
          </span>
        )}
        <span
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          style={{ color: mute, fontSize: 11, padding: 2, flexShrink: 0 }}
          title="移除文档"
        >
          ✕
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11, color: mute }}>
        {doc.status === "parsed" && (
          <>
            <span>{doc.pageCount ?? "?"} 页</span>
            <span>{((doc.charCount ?? 0) / 1000).toFixed(1)}k 字</span>
            <span>{doc.chunkCount} 段</span>
            {doc.parseMethod === "cache" && (
              <Pill fg="#0E9A8F" bg="rgba(14,154,143,0.12)" size={10}>
                缓存命中
              </Pill>
            )}
            {doc.parseMethod === "ocr" && (
              <Pill fg="#8a6d3b" bg="rgba(194,132,48,0.14)" size={10}>
                OCR
              </Pill>
            )}
          </>
        )}
        {doc.status === "parsing" && <Pill fg="#4F58A8" bg="rgba(79,88,168,0.12)" size={10}>解析中…</Pill>}
        {doc.status === "failed" && (
          <>
            <span style={{ color: "#B54545", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }} title={doc.parseError ?? ""}>
              解析失败：{(doc.parseError ?? "未知原因").slice(0, 24)}
            </span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                onRetry();
              }}
              style={{ color: "#6B73C9", fontWeight: 600, flexShrink: 0, cursor: "pointer" }}
              title="用同一文件重新解析"
            >
              重试
            </span>
          </>
        )}
      </div>
    </div>
  );
}
