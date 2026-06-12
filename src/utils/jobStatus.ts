// 任务状态的展示约定（文案/配色）与点击路由——多个屏幕共用，单一来源。
import type { JobDto } from "../api/types";
import { isLive } from "../queries/data";

export const STATUS_UI: Record<string, { label: string; fg: string; bg: string }> = {
  pending: { label: "等待中", fg: "#8a6d3b", bg: "rgba(194,132,48,0.14)" },
  running: { label: "检测中", fg: "#4F58A8", bg: "rgba(79,88,168,0.12)" },
  cancelling: { label: "取消中", fg: "#8a6d3b", bg: "rgba(194,132,48,0.14)" },
  cancelled: { label: "已取消", fg: "#75646C", bg: "rgba(117,100,108,0.12)" },
  failed: { label: "失败", fg: "#B54545", bg: "rgba(181,69,69,0.12)" },
  completed: { label: "已完成", fg: "#0E9A8F", bg: "rgba(14,154,143,0.12)" },
};

export function statusUi(status: string) {
  return STATUS_UI[status] ?? { label: status, fg: "#75646C", bg: "rgba(117,100,108,0.12)" };
}

/** 点开一个任务应去哪：完成的比对看报告，进行中看进度，其余回工作区。 */
export function jobRoute(j: JobDto): string {
  if (j.jobType === "compare") {
    return j.status === "completed"
      ? `/workspace/${j.workspaceId}/job/${j.id}`
      : `/workspace/${j.workspaceId}/job/${j.id}/running`;
  }
  return `/workspace/${j.workspaceId}/new`;
}

export const STAGE_LABEL: Record<string, string> = {
  hash: "校验文件",
  parse: "解析文档",
  load: "读取分块",
  semantic: "语义向量",
  recall: "候选召回",
  score: "精排打分",
  cluster: "聚合雷同条款",
  facts: "事实冲突检测",
  persist: "保存结果",
  done: "完成",
};

export function isJobLive(j: JobDto | undefined): boolean {
  return !!j && isLive(j);
}
