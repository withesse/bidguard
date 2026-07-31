// 路由表：Hash 路由（Tauri 生产环境用自定义协议加载，BrowserRouter 刷新会丢路径）。
// 结果屏 Matrix/Compare/Export 原生消费 DTO（useCompareSummary / 按需 getPairDetail），适配层已移除。
import type { ReactNode } from "react";
import {
  createHashRouter,
  Navigate,
  Outlet,
  useLocation,
  useNavigate,
  useParams,
  useRouteError,
} from "react-router-dom";
import { Sidebar, type NavKey } from "../components/Sidebar";
import { Button, Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { Screen } from "../routes";
import { useCompareSummary, useLicenseStatus } from "../queries/data";
import { Activate } from "../screens/Activate";
import { typeUi } from "../utils/clusterUi";
import { WorkspaceList } from "../screens/WorkspaceList";
import { CompareSetup } from "../screens/CompareSetup";
import { Running } from "../screens/Running";
import { JobsList } from "../screens/JobsList";
import { ClustersScreen } from "../screens/ClustersScreen";
import { ClusterDetail } from "../screens/ClusterDetail";
import { PairSegments } from "../screens/PairSegments";
import { BusinessNumeric } from "../screens/BusinessNumeric";
import { DocPreview } from "../screens/DocPreview";
import { Library } from "../screens/Library";
import { Tools } from "../screens/Tools";
import { Settings } from "../screens/Settings";
import { MatrixScreen } from "../screens/Matrix";
import { Compare } from "../screens/Compare";
import { Export } from "../screens/Export";

const NAV_PATH: Record<NavKey, string> = {
  home: "/",
  tasks: "/starred",
  history: "/history",
  library: "/library",
  tools: "/tools",
  settings: "/settings",
};

function activeKey(path: string): NavKey {
  if (path === "/") return "home";
  if (path.startsWith("/starred")) return "tasks";
  if (path.startsWith("/history")) return "history";
  if (path.startsWith("/library")) return "library";
  if (path.startsWith("/tools")) return "tools";
  if (path.startsWith("/settings")) return "settings";
  if (path.includes("/job/")) return "tasks";
  return "home";
}

function Layout() {
  const nav = useNavigate();
  const { pathname } = useLocation();
  const { dark } = useTheme();
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
      <Sidebar active={activeKey(pathname)} onNav={(k) => nav(NAV_PATH[k as NavKey] ?? "/")} />
      <LicenseGate>
        <Outlet />
      </LicenseGate>
    </div>
  );
}

/** 授权守卫：未激活/到期/用尽（active=false）时，除激活页外一律拦到 /activate。
 *  授权判定在 Rust 层强制，这里仅为 UX 引导（前端不可信）。状态未加载时不拦截，避免闪烁。 */
function LicenseGate({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  const { data, isLoading } = useLicenseStatus();
  if (!isLoading && data && !data.active && pathname !== "/activate") {
    return <Navigate to="/activate" replace />;
  }
  return <>{children}</>;
}

/** 结果屏壳：各屏原生消费 DTO（useCompareSummary / 按需 getPairDetail），onGo 映射为路由跳转。 */
function JobView({ view }: { view: "matrix" | "compare" | "export" }) {
  const { wsId, jobId } = useParams<{ wsId: string; jobId: string }>();
  const nav = useNavigate();

  const base = `/workspace/${wsId}/job/${jobId}`;
  const go = (s: Screen) => {
    const map: Partial<Record<Screen, string>> = {
      matrix: base,
      compare: `${base}/compare`,
      clusters: `${base}/clusters`,
      segments: `${base}/segments`,
      numeric: `${base}/numeric`,
      export: `${base}/export`,
      scan: `${base}/running`,
      tasks: "/starred",
      history: "/history",
      home: "/",
      new: `/workspace/${wsId}/new`,
    };
    nav(map[s] ?? "/");
  };

  switch (view) {
    case "matrix":
      return (
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          <SummaryBanner jobId={jobId} onClusters={() => go("clusters")} />
          <MatrixScreen onGo={go} jobId={jobId} />
        </div>
      );
    case "compare":
      return <Compare onGo={go} jobId={jobId} />;
    case "export":
      return <Export jobId={jobId} />;
  }
}

/** 八类差异统计条（设计文档 §13.1）：挂在报告总览顶部，点击类型直达条款列表。 */
function SummaryBanner({ jobId, onClusters }: { jobId: string | undefined; onClusters: () => void }) {
  const { dark } = useTheme();
  const { data } = useCompareSummary(jobId);
  const s = data?.summary;
  if (!s) return null;
  const counts: Array<[string, number]> = [
    ["conflict", s.conflictCount],
    ["same", s.sameCount],
    ["minor_change", s.minorChangeCount],
    ["changed", s.changedCount],
    ["rewrite", s.rewriteCount],
    ["uncertain", s.uncertainCount],
    ["added", s.addedCount],
    ["deleted", s.deletedCount],
  ];
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  return (
    <div
      style={{
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "8px 24px",
        borderBottom: `1px solid ${dark ? "rgba(255,255,255,0.06)" : C.line}`,
        flexWrap: "wrap",
        cursor: "pointer",
      }}
      onClick={onClusters}
      title="点击查看重复条款明细"
    >
      <span style={{ fontSize: 11, color: mute }}>
        {s.documentCount} 份 · {s.chunkCount} 段 · {s.clusterCount} 组条款
      </span>
      {(s.highRiskCount ?? 0) > 0 && (
        <Pill fg="#B54545" bg="rgba(181,69,69,0.14)" size={10.5} weight={700}>
          高风险 {s.highRiskCount}
        </Pill>
      )}
      {counts
        .filter(([, n]) => n > 0)
        .map(([k, n]) => {
          const t = typeUi(k);
          return (
            <Pill key={k} fg={t.fg} bg={t.bg} size={10.5}>
              {t.label} {n}
            </Pill>
          );
        })}
      {s.semanticDegraded && (
        <Pill fg="#8a6d3b" bg="rgba(194,132,48,0.14)" size={10.5}>
          语义模型不可用，已降级词面比对
        </Pill>
      )}
    </div>
  );
}

/** 路由级错误兜底：loader/渲染错误显示中文页面而非 React Router 默认英文错误页。 */
function RouteError() {
  const { dark } = useTheme();
  const err = useRouteError();
  const msg =
    err instanceof Error ? err.message : typeof err === "string" ? err : "页面加载时发生未知错误";
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 14,
        padding: 24,
        textAlign: "center",
        background: dark ? "#15151B" : C.paper,
      }}
    >
      <div style={{ fontSize: 15, fontWeight: 700, color: dark ? "#fff" : C.ink }}>这个页面出错了</div>
      <div
        style={{
          fontSize: 12.5,
          maxWidth: 480,
          lineHeight: 1.6,
          color: dark ? "rgba(255,255,255,0.6)" : C.ink3,
        }}
      >
        {msg}
      </div>
      <div style={{ display: "flex", gap: 10 }}>
        <Button kind="secondary" size="md" icon="home" onClick={() => (window.location.hash = "#/")}>
          返回首页
        </Button>
        <Button kind="primary" size="md" onClick={() => window.location.reload()}>
          重新加载
        </Button>
      </div>
    </div>
  );
}

export const router = createHashRouter([
  {
    element: <Layout />,
    errorElement: <RouteError />,
    children: [
      { path: "/", element: <WorkspaceList /> },
      { path: "/activate", element: <Activate /> },
      { path: "/starred", element: <JobsList title="我的任务" mode="starred" /> },
      { path: "/history", element: <JobsList title="历史记录" mode="all" /> },
      { path: "/library", element: <Library /> },
      { path: "/tools", element: <Tools /> },
      { path: "/settings", element: <Settings /> },
      { path: "/workspace/:wsId/new", element: <CompareSetup /> },
      { path: "/workspace/:wsId/doc/:docId", element: <DocPreview /> },
      { path: "/workspace/:wsId/job/:jobId/running", element: <Running /> },
      { path: "/workspace/:wsId/job/:jobId", element: <JobView view="matrix" /> },
      { path: "/workspace/:wsId/job/:jobId/compare", element: <JobView view="compare" /> },
      { path: "/workspace/:wsId/job/:jobId/clusters", element: <ClustersScreen /> },
      { path: "/workspace/:wsId/job/:jobId/segments", element: <PairSegments /> },
      { path: "/workspace/:wsId/job/:jobId/numeric", element: <BusinessNumeric /> },
      { path: "/workspace/:wsId/job/:jobId/cluster/:cid", element: <ClusterDetail /> },
      { path: "/workspace/:wsId/job/:jobId/export", element: <JobView view="export" /> },
      { path: "*", element: <WorkspaceList /> },
    ],
  },
]);
