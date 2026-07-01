// 工具箱：OCR 模型 + 语义模型（分开两卡） + 存储（DB/向量缓存/旧任务） + 环境自检。
// 资源管理动作，与「设置」的偏好分开；让 OCR/语义模型从「写死摸黑」变「可见可管」。
import { useState } from "react";
import { C } from "../design/tokens";
import { Topbar } from "../components/Topbar";
import { Button, ConfirmButton, Pill } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import {
  useAppInfo,
  useAppSettings,
  useCleanupOldJobs,
  useClearEmbeddingCache,
  useClearModel,
  useClearOcrModel,
  useDiagnostics,
  useDownloadModel,
  useDownloadOcrModel,
  useModelStatus,
  useSaveAppSettings,
  useStorageInfo,
  useVacuumDb,
} from "../queries/data";

function mb(bytes: number): string {
  if (bytes <= 0) return "0";
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function Tools() {
  const { dark, accent } = useTheme();
  const toast = useToast();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const models = useModelStatus();
  const appInfo = useAppInfo();
  // 当前选中的档位（来自设置；工具箱据此标「使用中」）
  const appCfg = useAppSettings().data as Record<string, Record<string, unknown>> | undefined;
  const activeOcr = (appCfg?.parser?.ocrModel as string) ?? "v6-small";
  const activeEmbed = (appCfg?.compare?.embeddingModel as string) ?? "bge-zh";
  const saveCfg = useSaveAppSettings();
  // 在工具箱直接切换当前档位（合并写配置，不覆盖其它项）
  const setActive = (section: "parser" | "compare", field: string, key: string) => {
    const current = (appCfg ?? {}) as Record<string, Record<string, unknown> | undefined>;
    saveCfg.mutate(
      { ...current, [section]: { ...(current[section] ?? {}), [field]: key } },
      {
        onSuccess: () => toast.show("已切换当前模型", "success"),
        onError: (e) => toast.show("切换失败：" + errMsg(e), "error"),
      },
    );
  };
  const storage = useStorageInfo();
  const download = useDownloadModel();
  const clearModel = useClearModel();
  const downloadOcr = useDownloadOcrModel();
  const clearOcr = useClearOcrModel();
  const clearCache = useClearEmbeddingCache();
  const vacuum = useVacuumDb();
  const cleanup = useCleanupOldJobs();
  const [downloading, setDownloading] = useState<string | null>(null);
  const [diagOn, setDiagOn] = useState(false);
  const diag = useDiagnostics(diagOn);

  const ms = models.data;

  const onDownload = (key: string, label: string) => {
    setDownloading(key);
    toast.show(`正在下载 ${label}，可能需要几分钟…`, "info");
    download.mutate(key, {
      onSuccess: () => toast.show(`${label} 已就绪`, "success"),
      onError: (e) => toast.show("下载失败：" + errMsg(e), "error"),
      onSettled: () => setDownloading(null),
    });
  };

  const onDownloadOcr = (key: string, label: string) => {
    setDownloading(key);
    toast.show(`正在下载 ${label}（约 132MB），可能需要几分钟…`, "info");
    downloadOcr.mutate(key, {
      onSuccess: (n) => toast.show(`${label} 已就绪（${mb(n)}）`, "success"),
      onError: (e) => toast.show("下载失败：" + errMsg(e), "error"),
      onSettled: () => setDownloading(null),
    });
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar title="工具箱" sub="模型、存储与环境自检" />
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 760, margin: "0 auto", display: "flex", flexDirection: "column", gap: 20 }}>
          {/* —— OCR 模型（扫描件识别）—— */}
          <Card title="OCR 模型 · 扫描件识别（PP-OCRv6）" cardBg={cardBg} border={border} mute={mute}>
            {(appInfo.data?.ocrModels ?? []).map((m, i, arr) => (
              <Row key={m.key} ink={ink} mute={mute} border={border} last={i === arr.length - 1}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <span style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>{m.label}</span>
                    {m.key === activeOcr && <ActivePill accent={accent} />}
                  </div>
                  <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>
                    {m.bundled
                      ? `随应用打包 · ${m.sizeLabel}`
                      : m.present
                        ? `已下载 · ${m.sizeLabel}`
                        : `未下载 · ${m.sizeLabel}`}
                  </div>
                </div>
                {m.key !== activeOcr && (
                  <Button kind="ghost" size="sm" onClick={() => setActive("parser", "ocrModel", m.key)}>
                    设为当前
                  </Button>
                )}
                {m.bundled ? (
                  <Pill fg="#0F6E56" bg="rgba(15,110,86,0.13)" size={10}>已就位</Pill>
                ) : m.present ? (
                  <ConfirmButton
                    confirmLabel="确认删除？"
                    pending={clearOcr.isPending}
                    pendingLabel="删除中…"
                    onConfirm={() =>
                      clearOcr.mutate(m.key, {
                        onSuccess: (n) => toast.show(`已释放 ${mb(n)}`, "success"),
                        onError: (e) => toast.show("删除失败：" + errMsg(e), "error"),
                      })
                    }
                  >
                    删除
                  </ConfirmButton>
                ) : (
                  <Button kind="secondary" size="sm" disabled={downloading != null} onClick={() => onDownloadOcr(m.key, m.label)}>
                    {downloading === m.key ? "下载中…" : "下载"}
                  </Button>
                )}
              </Row>
            ))}
          </Card>

          {/* —— 语义模型（向量查重）—— */}
          <Card title="语义模型 · 向量查重（按需下载）" cardBg={cardBg} border={border} mute={mute}>
            {(ms?.embeddingModels ?? []).map((m, i, arr) => (
              <Row key={m.key} ink={ink} mute={mute} border={border} last={i === arr.length - 1}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <span style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>{m.label}</span>
                    {m.key === activeEmbed && <ActivePill accent={accent} />}
                  </div>
                  <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>
                    {m.cached ? `已缓存 · ${mb(m.sizeBytes)}` : "未下载（首次语义比对时自动下载）"}
                  </div>
                </div>
                {m.key !== activeEmbed && (
                  <Button kind="ghost" size="sm" onClick={() => setActive("compare", "embeddingModel", m.key)}>
                    设为当前
                  </Button>
                )}
                {m.cached ? (
                  <ConfirmButton
                    confirmLabel="确认删除？"
                    pending={clearModel.isPending}
                    pendingLabel="删除中…"
                    onConfirm={() =>
                      clearModel.mutate(m.key, {
                        onSuccess: (n) => toast.show(`已释放 ${mb(n)}`, "success"),
                        onError: (e) => toast.show("删除失败：" + errMsg(e), "error"),
                      })
                    }
                  >
                    删除缓存
                  </ConfirmButton>
                ) : (
                  <Button kind="secondary" size="sm" disabled={downloading != null} onClick={() => onDownload(m.key, m.label)}>
                    {downloading === m.key ? "下载中…" : "下载"}
                  </Button>
                )}
              </Row>
            ))}
          </Card>

          {/* —— 存储 —— */}
          <Card title="存储" cardBg={cardBg} border={border} mute={mute}>
            <Row ink={ink} mute={mute} border={border}>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>数据库</div>
                <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>
                  {mb(storage.data?.dbBytes ?? 0)} · {storage.data?.documentCount ?? 0} 文档 · {storage.data?.jobCount ?? 0} 任务
                </div>
              </div>
              <Button
                kind="ghost"
                size="sm"
                disabled={vacuum.isPending}
                onClick={() =>
                  vacuum.mutate(undefined, {
                    onSuccess: () => toast.show("数据库已压缩", "success"),
                    onError: (e) => toast.show("压缩失败：" + errMsg(e), "error"),
                  })
                }
              >
                {vacuum.isPending ? "压缩中…" : "压缩 VACUUM"}
              </Button>
            </Row>
            <Row ink={ink} mute={mute} border={border}>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>语义向量缓存</div>
                <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>
                  {storage.data?.embeddingRows ?? 0} 条 · 换模型后旧向量可清理（下次比对按需重算）
                </div>
              </div>
              <ConfirmButton
                confirmLabel="确认清空？"
                pending={clearCache.isPending}
                pendingLabel="清空中…"
                onConfirm={() =>
                  clearCache.mutate(undefined, {
                    onSuccess: (n) => toast.show(`已清空 ${n} 条向量缓存`, "success"),
                    onError: (e) => toast.show("清理失败：" + errMsg(e), "error"),
                  })
                }
              >
                清空
              </ConfirmButton>
            </Row>
            <Row ink={ink} mute={mute} border={border} last>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>清理旧任务</div>
                <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>删除 30 天前已完结且未收藏的任务</div>
              </div>
              <ConfirmButton
                confirmLabel="确认清理？"
                pending={cleanup.isPending}
                pendingLabel="清理中…"
                onConfirm={() =>
                  cleanup.mutate(30, {
                    onSuccess: (n) =>
                      toast.show(n > 0 ? `已清理 ${n} 个旧任务` : "没有可清理的旧任务", "success"),
                    onError: (e) => toast.show("清理失败：" + errMsg(e), "error"),
                  })
                }
              >
                清理
              </ConfirmButton>
            </Row>
          </Card>

          {/* —— 环境自检 —— */}
          <Card title="环境自检" cardBg={cardBg} border={border} mute={mute}>
            {!diagOn ? (
              <Row ink={ink} mute={mute} border={border} last>
                <div style={{ flex: 1, fontSize: 12, color: mute }}>
                  检查 PDF 引擎 / OCR 模型 / 语义模型 / 数据库是否可用，自助排障。
                </div>
                <Button kind="secondary" size="sm" onClick={() => setDiagOn(true)}>开始检查</Button>
              </Row>
            ) : (
              (diag.data ?? []).map((d, i, arr) => (
                <Row key={d.key} ink={ink} mute={mute} border={border} last={i === arr.length - 1}>
                  <span style={{ fontSize: 14, marginRight: 8, color: d.ok ? "#0F6E56" : "#A32D2D" }}>
                    {d.ok ? "✓" : "✕"}
                  </span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 12.5, fontWeight: 600, color: ink }}>{d.label}</div>
                    <div style={{ fontSize: 11, color: mute, marginTop: 2 }}>{d.detail}</div>
                  </div>
                </Row>
              ))
            )}
            {diagOn && diag.isLoading && (
              <div style={{ fontSize: 12, color: mute, padding: "8px 2px" }}>检查中…</div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}

// 「使用中」徽标：标注设置里当前选中的档位。
function ActivePill({ accent }: { accent: string }) {
  return (
    <Pill fg="#fff" bg={accent} size={9.5} weight={700}>
      使用中
    </Pill>
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
  children: React.ReactNode;
  cardBg: string;
  border: string;
  mute: string;
}) {
  return (
    <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, overflow: "hidden" }}>
      <div style={{ fontSize: 11.5, fontWeight: 700, color: mute, padding: "12px 16px 6px", letterSpacing: "0.02em" }}>
        {title}
      </div>
      <div style={{ padding: "0 16px 6px" }}>{children}</div>
    </div>
  );
}

function Row({
  children,
  border,
  last,
}: {
  children: React.ReactNode;
  ink: string;
  mute: string;
  border: string;
  last?: boolean;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "11px 0",
        borderBottom: last ? "none" : `1px solid ${border}`,
      }}
    >
      {children}
    </div>
  );
}
