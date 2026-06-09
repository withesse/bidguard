import { useEffect, useState } from "react";
import { Sidebar, type NavKey } from "./components/Sidebar";
import { Home } from "./screens/Home";
import { Dashboard } from "./screens/Dashboard";
import { Tasks } from "./screens/Tasks";
import { Scan } from "./screens/Scan";
import { Library } from "./screens/Library";
import { Settings } from "./screens/Settings";
import { MatrixScreen } from "./screens/Matrix";
import { Compare } from "./screens/Compare";
import { Clusters } from "./screens/Clusters";
import { Export } from "./screens/Export";
import { useTheme } from "./theme";
import { useToast } from "./components/Toast";
import { C } from "./design/tokens";
import type { Screen } from "./routes";
import { isTauri, pickBidFiles, runAnalysis, parseMeta, saveTask, getTask, type Report, type Progress } from "./engine";
import { loadTemplates } from "./templates";
import { getSettings, setSettings, type Settings as AppSettings } from "./prefs";

const NAV_KEYS: NavKey[] = ["home", "tasks", "history", "library", "settings"];
const ACCEPT = /\.(docx|pdf|txt|md)$/i;

export type FileStatus = "parsing" | "ok" | "error";
export interface PickedFile {
  path: string;
  name: string;
  type: string;
  status: FileStatus;
  pages?: number;
  chars?: number;
  error?: string;
}

function App() {
  const [screen, setScreen] = useState<Screen>("home");
  const [report, setReport] = useState<Report | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [pending, setPending] = useState<string[]>([]);
  const [files, setFiles] = useState<PickedFile[]>([]);
  const [taskName, setTaskName] = useState("未命名查重任务");
  const [settings, setSettingsState] = useState<AppSettings>(() => getSettings());
  const { dark } = useTheme();
  const toast = useToast();

  // 异步解析单个候选文件，回填页数/字数或标记解析失败
  const parseOne = async (path: string) => {
    if (!isTauri()) return;
    try {
      const m = await parseMeta(path);
      setFiles((prev) =>
        prev.map((f) => (f.path === path ? { ...f, status: "ok", pages: m.pages, chars: m.charCount } : f)),
      );
    } catch (e) {
      setFiles((prev) =>
        prev.map((f) => (f.path === path ? { ...f, status: "error", error: String(e) } : f)),
      );
    }
  };

  const appendPaths = (paths: string[]) => {
    setFiles((prev) => {
      const seen = new Set(prev.map((f) => f.path));
      const add = paths
        .filter((p) => ACCEPT.test(p) && !seen.has(p))
        .map((p) => ({
          path: p,
          name: p.split(/[\\/]/).pop() ?? p,
          type: (p.split(".").pop() ?? "").toLowerCase(),
          status: "parsing" as const,
        }));
      const next = [...prev, ...add].slice(0, 5);
      if (prev.length + add.length > 5) toast.show("最多比对 5 份标书，多余的已忽略", "warn");
      const toParse = next.filter((f) => add.some((a) => a.path === f.path)).map((f) => f.path);
      queueMicrotask(() => toParse.forEach(parseOne));
      return next;
    });
  };

  const addFiles = async () => {
    if (!isTauri()) {
      toast.show("文件选择仅在桌面应用内可用", "info");
      return;
    }
    const picked = await pickBidFiles();
    if (picked.length) appendPaths(picked);
  };
  const removeFile = (i: number) => setFiles((prev) => prev.filter((_, k) => k !== i));
  const changeSettings = (patch: Partial<AppSettings>) => setSettingsState(setSettings(patch));

  // Tauri 原生拖拽导入：拖文件到窗口即加入候选
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "drop") {
            const dropped = event.payload.paths.filter((p) => ACCEPT.test(p));
            if (dropped.length) {
              appendPaths(dropped);
              setScreen("new");
            }
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
  }, []);

  // 选文件 → 跑引擎（实时进度）→ 进报告。浏览器预览无 Tauri，降级到演示进度态。
  const onAnalyze = async () => {
    if (!isTauri()) {
      setScreen("scan");
      return;
    }
    if (files.length < 2) {
      toast.show("请至少选择 2 份标书再开始查重", "warn");
      return;
    }
    setPending(files.map((f) => f.name));
    setProgress(null);
    setReport(null);
    setScreen("scan");
    try {
      const templates = settings.ignoreTemplates ? loadTemplates().map((t) => t.text) : [];
      const r = await runAnalysis(
        files.map((f) => f.path),
        templates,
        settings.semantic,
        settings.threshold,
        settings.scope,
        (p) => setProgress(p),
      );
      setReport(r);
      setScreen("matrix");
      const failed = r.docs.filter((d) => d.parseError).length;
      if (failed > 0) toast.show(`${failed} 份文档解析失败，已在报告中标注`, "warn");
      saveTask(taskName.trim() || `${r.docs.length} 份标书交叉比对`, r).catch(() => {});
      setFiles([]);
      setTaskName("未命名查重任务");
    } catch (e) {
      setScreen("new");
      toast.show("分析失败：" + String(e), "error");
    }
  };

  // 打开历史任务：读取其报告并进入报告屏
  const openTask = async (id: string) => {
    try {
      const r = await getTask(id);
      setReport(r);
      setScreen("matrix");
    } catch (e) {
      toast.show("打开任务失败：" + String(e), "error");
    }
  };

  // 新建任务屏归属「首页」；其余流程屏（scan/matrix/...）保持「我的任务」高亮
  const active: NavKey =
    screen === "new" ? "home" : NAV_KEYS.includes(screen as NavKey) ? (screen as NavKey) : "tasks";

  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        width: "100vw",
        background: dark ? "#15151B" : C.paper,
        overflow: "hidden",
      }}
    >
      <Sidebar active={active} onNav={setScreen} />
      {screen === "home" && <Dashboard onGo={setScreen} onOpen={openTask} />}
      {screen === "new" && (
        <Home
          files={files}
          canPick={isTauri()}
          settings={settings}
          taskName={taskName}
          onAddFiles={addFiles}
          onRemoveFile={removeFile}
          onChangeSettings={changeSettings}
          onChangeName={setTaskName}
          onAnalyze={onAnalyze}
        />
      )}
      {screen === "tasks" && <Tasks onGo={setScreen} onOpen={openTask} mode="starred" />}
      {screen === "history" && (
        <Tasks onGo={setScreen} onOpen={openTask} title="历史记录" mode="all" />
      )}
      {screen === "scan" && (
        <Scan onGo={setScreen} progress={progress} files={pending} semantic={settings.semantic} />
      )}
      {screen === "settings" && <Settings />}
      {screen === "matrix" && <MatrixScreen onGo={setScreen} report={report} />}
      {screen === "compare" && <Compare onGo={setScreen} report={report} />}
      {screen === "clusters" && <Clusters onGo={setScreen} report={report} />}
      {screen === "export" && <Export report={report} />}
      {screen === "library" && <Library />}
    </div>
  );
}

export default App;
