// 路由表：Hash 路由（Tauri 生产环境用自定义协议加载，BrowserRouter 刷新会丢路径）。
// 结果屏 Matrix/Compare/Export 原生消费 DTO（useCompareSummary / 按需 getPairDetail），适配层已移除。
import { createHashRouter, Outlet, useLocation, useNavigate, useParams } from "react-router-dom";
import { Sidebar, type NavKey } from "../components/Sidebar";
import { Pill } from "../components/primitives";
import { C } from "../design/tokens";
import { useTheme } from "../theme";
import type { Screen } from "../routes";
import { useCompareSummary } from "../queries/data";
import { typeUi } from "../utils/clusterUi";
import { WorkspaceList } from "../screens/WorkspaceList";
import { CompareSetup } from "../screens/CompareSetup";
import { Running } from "../screens/Running";
import { JobsList } from "../screens/JobsList";
import { ClustersScreen } from "../screens/ClustersScreen";
import { ClusterDetail } from "../screens/ClusterDetail";
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
      <Outlet />
    </div>
  );
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

export const router = createHashRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <WorkspaceList /> },
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
      { path: "/workspace/:wsId/job/:jobId/cluster/:cid", element: <ClusterDetail /> },
      { path: "/workspace/:wsId/job/:jobId/export", element: <JobView view="export" /> },
      { path: "*", element: <WorkspaceList /> },
    ],
  },
]);
