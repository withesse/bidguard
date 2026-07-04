// IPC 契约测试：TS DTO 是 Rust serde(camelCase) 输出的手写镜像。这里用代表性 wire 样本 +
// `satisfies` 把样本绑定到 DTO 类型——前端任一字段重命名/漏字段/改可空性，tsc 会在此文件报错
// （`npm run build` 拦截）；运行时再断言关键 camelCase 键齐备，锁定线协议形状。
//
// 局限：无法自动发现 Rust 侧改名（那需 tauri-specta 之类的类型生成）。样本值对应当前 Rust
// serde 输出；改 Rust 字段时应同步更新对应 DTO 与此处样本。
import { describe, it, expect } from "vitest";
import type { DocumentDto, JobDto, ProgressEvent, TerminalEvent } from "./types";

const documentSample = {
  id: "d1",
  workspaceId: "w1",
  fileName: "bid.docx",
  filePath: "/x/bid.docx",
  fileHash: "abc",
  fileType: "docx",
  status: "parsed",
  parseError: null,
  parseMethod: "docx",
  pageCount: 12,
  charCount: 3400,
  fingerprintJson: null,
  chunkCount: 42,
  createdAt: "2026-07-02T00:00:00.000Z",
  updatedAt: "2026-07-02T00:00:00.000Z",
  truncationNotice: null,
} satisfies DocumentDto;

const jobSample = {
  id: "j1",
  workspaceId: "w1",
  jobType: "compare",
  name: null,
  status: "completed",
  configJson: "{}",
  progress: 1,
  message: null,
  errorMessage: null,
  errorCode: null,
  starred: false,
  matrixJson: null,
  collusionLevel: "none",
  createdAt: "2026-07-02T00:00:00.000Z",
  startedAt: null,
  finishedAt: null,
} satisfies JobDto;

const progressSample = {
  jobId: "j1",
  jobType: "import",
  stage: "parse",
  message: "解析中",
  current: 1,
  total: 3,
  percent: 0.33,
} satisfies ProgressEvent;

const terminalSample = {
  jobId: "j1",
  jobType: "import",
  status: "completed",
} satisfies TerminalEvent;

describe("IPC DTO 契约（camelCase 镜像）", () => {
  it("DocumentDto 关键字段齐备（含 truncationNotice）", () => {
    expect(Object.keys(documentSample)).toEqual(
      expect.arrayContaining([
        "id", "workspaceId", "fileName", "status", "parseError",
        "chunkCount", "truncationNotice",
      ]),
    );
  });

  it("JobDto 关键字段齐备（含 collusionLevel）", () => {
    expect(Object.keys(jobSample)).toEqual(
      expect.arrayContaining([
        "id", "workspaceId", "jobType", "status", "starred",
        "matrixJson", "collusionLevel",
      ]),
    );
  });

  it("ProgressEvent / TerminalEvent 关键字段齐备", () => {
    expect(Object.keys(progressSample)).toEqual(
      expect.arrayContaining(["jobId", "jobType", "stage", "current", "total", "percent"]),
    );
    expect(Object.keys(terminalSample)).toEqual(
      expect.arrayContaining(["jobId", "jobType", "status"]),
    );
  });
});
