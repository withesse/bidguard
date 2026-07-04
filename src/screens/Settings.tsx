// 屏 8 · 设置 —— 检测偏好（落库，作为四层配置的「用户全局」层）/ 隐私 / 外观（本地）
import { useMemo, useState, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Logo } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Toggle, SegControl } from "../components/primitives";
import { useTheme, type FontScale, type Highlight } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import { useAppInfo, useAppSettings, useSaveAppSettings } from "../queries/data";
import { relaunchApp, runUpdate, type UpdateState } from "../utils/updater";
import { type Scope } from "../prefs";

const FONT_SCALES: FontScale[] = ["compact", "regular", "comfy", "spacious"];
const SCOPES: Scope[] = ["full", "tech", "business"];
const HIGHLIGHTS: Highlight[] = ["amber", "rose", "blue"];
const CHUNK_LEVELS = ["section", "paragraph", "sentence"];
const LANGS = ["auto", "zh", "en"];

/** 与后端 config::CompareDefaults 内置值一致的前端镜像（无 patch 时显示用）。 */
const BUILTIN_COMPARE = {
  scope: "full",
  similarityThreshold: 0.7,
  candidateTopK: 100,
  ignoreTemplates: true,
  enableSemantic: false,
  enableFactConflict: true,
  defaultChunkLevel: "paragraph",
  ignoreWhitespace: true,
  ignorePunctuation: true,
  ignoreCase: true,
  language: "auto",
  embeddingModel: "bge-zh",
};

/** 与后端 config::ParserDefaults 内置值一致的前端镜像。 */
const BUILTIN_PARSER = {
  removeHeaderFooter: true,
  preservePageNumber: true,
  detectTable: true,
  ocrDocxImages: true,
  ocrModel: "v6-small",
};

/** 与后端 config::SecurityDefaults 内置值一致的前端镜像。 */
const BUILTIN_SECURITY = {
  allowCloudModel: false,
};

export function Settings() {
  const { dark, accent, mode, set, fontScale, layout, highlight, reduceMotion } = useTheme();
  const toast = useToast();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const modeIndex = mode === "light" ? 0 : mode === "dark" ? 1 : 2;

  // 检测偏好（DB · 用户全局层）
  const { data: cfgRaw } = useAppSettings();
  const saveCfg = useSaveAppSettings();
  const { data: appInfo } = useAppInfo();
  const embeddingModels = appInfo?.embeddingModels ?? [];
  const cmp = useMemo(() => {
    const patch =
      cfgRaw && typeof cfgRaw === "object"
        ? ((cfgRaw as Record<string, unknown>).compare as Record<string, unknown> | undefined)
        : undefined;
    return { ...BUILTIN_COMPARE, ...(patch ?? {}) };
  }, [cfgRaw]);
  const changeCmp = (patch: Record<string, unknown>) => {
    const current =
      cfgRaw && typeof cfgRaw === "object" ? (cfgRaw as Record<string, unknown>) : {};
    const compare = {
      ...((current.compare as Record<string, unknown>) ?? {}),
      ...patch,
    };
    saveCfg.mutate(
      { ...current, compare },
      { onError: (e) => toast.show("保存设置失败：" + errMsg(e), "error") },
    );
  };
  const parser = useMemo(() => {
    const patch =
      cfgRaw && typeof cfgRaw === "object"
        ? ((cfgRaw as Record<string, unknown>).parser as Record<string, unknown> | undefined)
        : undefined;
    return { ...BUILTIN_PARSER, ...(patch ?? {}) };
  }, [cfgRaw]);
  const changeParser = (patch: Record<string, unknown>) => {
    const current =
      cfgRaw && typeof cfgRaw === "object" ? (cfgRaw as Record<string, unknown>) : {};
    const next = {
      ...((current.parser as Record<string, unknown>) ?? {}),
      ...patch,
    };
    saveCfg.mutate(
      { ...current, parser: next },
      { onError: (e) => toast.show("保存设置失败：" + errMsg(e), "error") },
    );
  };
  const sec = useMemo(() => {
    const patch =
      cfgRaw && typeof cfgRaw === "object"
        ? ((cfgRaw as Record<string, unknown>).security as Record<string, unknown> | undefined)
        : undefined;
    return { ...BUILTIN_SECURITY, ...(patch ?? {}) };
  }, [cfgRaw]);
  const changeSec = (patch: Record<string, unknown>) => {
    const current =
      cfgRaw && typeof cfgRaw === "object" ? (cfgRaw as Record<string, unknown>) : {};
    const next = {
      ...((current.security as Record<string, unknown>) ?? {}),
      ...patch,
    };
    saveCfg.mutate(
      { ...current, security: next },
      { onError: (e) => toast.show("保存设置失败：" + errMsg(e), "error") },
    );
  };

  const pct = Math.round((cmp.similarityThreshold as number) * 100);

  // 检查更新
  const [upd, setUpd] = useState<UpdateState>({ kind: "idle" });
  const checkUpdate = async () => {
    const ok = await runUpdate(setUpd);
    if (ok) {
      toast.show("更新已下载，正在重启应用…", "success");
      setTimeout(() => void relaunchApp(), 800);
    }
    // 未更新时的状态（检查中/已最新/有新版/错误）已由 runUpdate 经 setUpd 反映
  };
  const updLabel = (): string => {
    switch (upd.kind) {
      case "checking":
        return "检查中…";
      case "none":
        return "已是最新版本";
      case "available":
        return `发现新版本 ${upd.version}`;
      case "downloading":
        return `下载中 ${Math.round(upd.pct * 100)}%`;
      case "ready":
        return "下载完成，即将重启";
      case "error":
        return "检查失败：" + upd.message.slice(0, 40);
      default:
        return "检查更新";
    }
  };

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
                value={Math.max(0, SCOPES.indexOf(cmp.scope as Scope))}
                onChange={(i) => changeCmp({ scope: SCOPES[i] })}
              />
            </Row>
            <Row label="默认相似度阈值" sub="新任务的初始阈值，可在任务页临时调整" ink={ink} mute={mute}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <input
                  type="range"
                  min={20}
                  max={95}
                  step={5}
                  value={pct}
                  onChange={(e) => changeCmp({ similarityThreshold: Number(e.target.value) / 100 })}
                  style={{ width: 120, accentColor: accent, cursor: "pointer" }}
                />
                <span style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 32 }}>
                  {pct}%
                </span>
              </div>
            </Row>
            <Row label="默认分块粒度" sub="比对的基本单位：章节粗看 / 段落均衡 / 句子精查" ink={ink} mute={mute}>
              <SegControl
                options={["章节", "段落", "句子"]}
                value={Math.max(0, CHUNK_LEVELS.indexOf(cmp.defaultChunkLevel as string))}
                onChange={(i) => changeCmp({ defaultChunkLevel: CHUNK_LEVELS[i] })}
              />
            </Row>
            <Row label="召回深度" sub="每段保留的候选数：高=更全更慢，低=更快可能漏" ink={ink} mute={mute}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <input
                  type="range"
                  min={20}
                  max={300}
                  step={20}
                  value={(cmp.candidateTopK as number) ?? 100}
                  onChange={(e) => changeCmp({ candidateTopK: Number(e.target.value) })}
                  style={{ width: 120, accentColor: accent, cursor: "pointer" }}
                />
                <span style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 32 }}>
                  {(cmp.candidateTopK as number) ?? 100}
                </span>
              </div>
            </Row>
            <Row label="忽略通用模板段落" sub="标准条款、表头、附件目录（查重源库）" ink={ink} mute={mute}>
              <Toggle
                on={cmp.ignoreTemplates as boolean}
                onChange={() => changeCmp({ ignoreTemplates: !cmp.ignoreTemplates })}
              />
            </Row>
            <Row label="事实冲突检测" sub="同一条款金额/工期/日期不一致 → 高风险标记" ink={ink} mute={mute}>
              <Toggle
                on={cmp.enableFactConflict as boolean}
                onChange={() => changeCmp({ enableFactConflict: !cmp.enableFactConflict })}
              />
            </Row>
            <Row label="语义查重" sub="叠加 AI 语义相似度，捕捉改写式雷同（首次需下载模型 ~120MB）" ink={ink} mute={mute}>
              <Toggle
                on={cmp.enableSemantic as boolean}
                onChange={() => changeCmp({ enableSemantic: !cmp.enableSemantic })}
              />
            </Row>
            {(cmp.enableSemantic as boolean) && embeddingModels.length > 0 && (
              <Row label="语义模型" sub="切换模型后该模型的语义缓存独立重建（首次需下载）" ink={ink} mute={mute}>
                <select
                  value={(cmp.embeddingModel as string) ?? "bge-zh"}
                  onChange={(e) => changeCmp({ embeddingModel: e.target.value })}
                  style={{
                    fontSize: 12,
                    padding: "5px 8px",
                    borderRadius: 7,
                    border: `1px solid ${border}`,
                    background: cardBg,
                    color: ink,
                    cursor: "pointer",
                    maxWidth: 220,
                  }}
                >
                  {embeddingModels.map((m) => (
                    <option key={m.key} value={m.key}>
                      {m.label}
                    </option>
                  ))}
                </select>
              </Row>
            )}
          </Card>

          {/* 解析与归一（导入时生效）*/}
          <Card title="解析与归一（导入文档时生效）" cardBg={cardBg} border={border} mute={mute}>
            <Row label="忽略空白差异" sub="「每月十日前」与「每月 10 日 前」视为相同" ink={ink} mute={mute}>
              <Toggle
                on={cmp.ignoreWhitespace as boolean}
                onChange={() => changeCmp({ ignoreWhitespace: !cmp.ignoreWhitespace })}
              />
            </Row>
            <Row label="忽略标点差异" sub="全半角、中英文标点不计入差异" ink={ink} mute={mute}>
              <Toggle
                on={cmp.ignorePunctuation as boolean}
                onChange={() => changeCmp({ ignorePunctuation: !cmp.ignorePunctuation })}
              />
            </Row>
            <Row label="忽略大小写" sub="英文字母大小写不计入差异" ink={ink} mute={mute}>
              <Toggle
                on={cmp.ignoreCase as boolean}
                onChange={() => changeCmp({ ignoreCase: !cmp.ignoreCase })}
              />
            </Row>
            <Row label="识别表格" sub="报价表/清单按行比对，金额冲突可检出" ink={ink} mute={mute}>
              <Toggle
                on={parser.detectTable as boolean}
                onChange={() => changeParser({ detectTable: !parser.detectTable })}
              />
            </Row>
            <Row label="清理页眉页脚" sub="PDF 跨页重复的页眉/页脚与页码行不参与比对" ink={ink} mute={mute}>
              <Toggle
                on={parser.removeHeaderFooter as boolean}
                onChange={() => changeParser({ removeHeaderFooter: !parser.removeHeaderFooter })}
              />
            </Row>
            <Row label="识别 Word 内嵌图片" sub="OCR 截图式报价表/资质/公章里的文字，图片多时导入变慢" ink={ink} mute={mute}>
              <Toggle
                on={parser.ocrDocxImages as boolean}
                onChange={() => changeParser({ ocrDocxImages: !parser.ocrDocxImages })}
              />
            </Row>
            {(appInfo?.ocrModels ?? []).length > 0 && (
              <Row
                label="扫描件 OCR 档位"
                sub="PP-OCRv6：tiny 极速 / small 标准 / medium 高精（需在工具箱下载）"
                ink={ink}
                mute={mute}
              >
                <select
                  value={(parser.ocrModel as string) ?? "v6-small"}
                  onChange={(e) => changeParser({ ocrModel: e.target.value })}
                  style={{
                    fontSize: 12,
                    padding: "5px 8px",
                    borderRadius: 7,
                    border: `1px solid ${border}`,
                    background: cardBg,
                    color: ink,
                    cursor: "pointer",
                    maxWidth: 260,
                  }}
                >
                  {(appInfo?.ocrModels ?? []).map((m) => (
                    <option key={m.key} value={m.key}>
                      {m.label} {m.sizeLabel}
                      {m.present ? "" : " · 未下载"}
                    </option>
                  ))}
                </select>
              </Row>
            )}
            <Row label="分词语言" sub="自动按内容判定；英文标书可固定 English" ink={ink} mute={mute} last>
              <SegControl
                options={["自动", "中文", "English"]}
                value={Math.max(0, LANGS.indexOf((cmp.language as string) ?? "auto"))}
                onChange={(i) => changeCmp({ language: LANGS[i] })}
              />
            </Row>
            <div style={{ fontSize: 10.5, color: mute, padding: "8px 2px 0" }}>
              以上选项在导入文档时生效；修改后需重新导入文档才会应用到已有文档。
            </div>
          </Card>

          {/* 隐私 */}
          <Card title="隐私" cardBg={cardBg} border={border} mute={mute}>
            <Row label="本地优先模式" sub="标书不上传至服务器（本应用始终本地处理）" ink={ink} mute={mute}>
              <span style={{ fontSize: 11, color: C.ok, fontWeight: 600 }}>始终启用</span>
            </Row>
            <Row
              label="允许联网下载语义模型"
              sub="首次启用语义查重需下载模型（~120MB，仅此一次）；关闭则无缓存时自动降级词面比对"
              ink={ink}
              mute={mute}
              last
            >
              <Toggle
                on={sec.allowCloudModel as boolean}
                onChange={() => changeSec({ allowCloudModel: !sec.allowCloudModel })}
              />
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

          {/* 关于与更新 */}
          <Card title="关于" cardBg={cardBg} border={border} mute={mute}>
            <Row label="检查更新" sub="从 GitHub Releases 获取签名更新包" ink={ink} mute={mute} last>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span style={{ fontSize: 11.5, color: mute }}>{updLabel()}</span>
                <Button
                  kind="secondary"
                  size="sm"
                  onClick={checkUpdate}
                  disabled={upd.kind === "checking" || upd.kind === "downloading"}
                >
                  检查
                </Button>
              </div>
            </Row>
          </Card>

          <div style={{ fontSize: 11, color: mute, textAlign: "center", padding: "8px 0 20px" }}>
            <Logo size={20} color={accent} style={{ verticalAlign: "middle", marginRight: 6 }} />
            <span style={{ verticalAlign: "middle" }}>
              原本 · Verum · 标书查重{appInfo?.version ? ` v${appInfo.version}` : ""}
            </span>
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
