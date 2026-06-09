import type { NavKey } from "./components/Sidebar";

// 首页(home=仪表盘) + 新建任务(new) + 任务流程屏
export type Screen = NavKey | "new" | "scan" | "matrix" | "compare" | "clusters" | "export";
