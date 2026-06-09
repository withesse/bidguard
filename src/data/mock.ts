// 演示数据 —— 移植自 app-design/project/src/c/bid-a.jsx
// 后续将由 Rust 引擎（analyze_paths）的真实结果替换。

export interface BidSlot {
  state: "filled" | "empty";
  name?: string;
  size?: string;
  pages?: number;
  type?: string;
  label?: string;
}

export const BID_SLOTS: BidSlot[] = [
  { state: "filled", name: "智慧城邦科技_技术响应文件.pdf", size: "12.4 MB", pages: 86, type: "pdf" },
  { state: "filled", name: "启明信息_投标文件_技术标.docx", size: "8.6 MB", pages: 72, type: "docx" },
  { state: "filled", name: "鸿信科技_市政平台投标书.pdf", size: "14.2 MB", pages: 92, type: "pdf" },
  { state: "filled", name: "蓝信电子_技术标响应.docx", size: "6.3 MB", pages: 38, type: "docx" },
  { state: "empty", label: "可选 · 第 5 份" },
];

export type TaskStatus = "running" | "done" | "review";

export interface BidTask {
  name: string;
  sub: string;
  docs: string[];
  matrix: number[][];
  peak: number;
  hint: string;
  status: TaskStatus;
  progress: number;
  time: string;
}

export const BID_TASKS: BidTask[] = [
  {
    name: "市政信息化平台采购 · 5 家供应商围标核查",
    sub: "4 份标书 · 6 对比对 · 进行中",
    docs: ["甲", "乙", "丙", "丁"],
    matrix: [
      [1, 0.92, 0.34, 0.42],
      [0.92, 1, 0.31, 0.4],
      [0.34, 0.31, 1, 0.68],
      [0.42, 0.4, 0.68, 1],
    ],
    peak: 92,
    hint: "甲乙疑似围标",
    status: "running",
    progress: 0.62,
    time: "2 分钟前",
  },
  {
    name: "高校实验室设备采购 · 三厂家技术响应",
    sub: "3 份标书 · 3 对比对 · 已完成",
    docs: ["甲", "乙", "丙"],
    matrix: [
      [1, 0.41, 0.28],
      [0.41, 1, 0.35],
      [0.28, 0.35, 1],
    ],
    peak: 41,
    hint: "相似度正常",
    status: "done",
    progress: 1,
    time: "昨日 16:18",
  },
  {
    name: "智慧园区集成项目 · 4 家集成商",
    sub: "4 份标书 · 6 对比对 · 需复核",
    docs: ["甲", "乙", "丙", "丁"],
    matrix: [
      [1, 0.58, 0.74, 0.41],
      [0.58, 1, 0.62, 0.38],
      [0.74, 0.62, 1, 0.46],
      [0.41, 0.38, 0.46, 1],
    ],
    peak: 74,
    hint: "甲丙高相似",
    status: "review",
    progress: 1,
    time: "5 月 22 日",
  },
  {
    name: "政府云一期项目 · 双供应商技术对比",
    sub: "2 份标书 · 1 对比对 · 已完成",
    docs: ["甲", "乙"],
    matrix: [
      [1, 0.18],
      [0.18, 1],
    ],
    peak: 18,
    hint: "差异充分",
    status: "done",
    progress: 1,
    time: "5 月 18 日",
  },
  {
    name: "数据中心建设 · 3 家集成商投标",
    sub: "3 份标书 · 3 对比对 · 已完成",
    docs: ["甲", "乙", "丙"],
    matrix: [
      [1, 0.86, 0.81],
      [0.86, 1, 0.79],
      [0.81, 0.79, 1],
    ],
    peak: 86,
    hint: "三方共用模板",
    status: "done",
    progress: 1,
    time: "5 月 14 日",
  },
];
