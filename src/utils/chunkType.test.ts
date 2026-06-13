import { describe, it, expect } from "vitest";
import { splitSentences, chunkTypeUi, CHUNK_TYPE_ORDER } from "./chunkType";

describe("splitSentences", () => {
  it("按句末标点切句并保留标点", () => {
    const s = splitSentences("本项目采用微服务架构。平台支持横向扩展！是否可行？");
    expect(s).toEqual(["本项目采用微服务架构。", "平台支持横向扩展！", "是否可行？"]);
  });

  it("短句也单独切出（着色需呈现完整边界）", () => {
    const s = splitSentences("智慧水务平台实现数据汇聚共享。好。");
    expect(s).toEqual(["智慧水务平台实现数据汇聚共享。", "好。"]);
  });

  it("无句末标点的整段作一句", () => {
    expect(splitSentences("一段没有句号的文字")).toEqual(["一段没有句号的文字"]);
  });

  it("空串返回空数组", () => {
    expect(splitSentences("")).toEqual([]);
  });

  it("英文按 . ! ? 切（后接空格+大写）", () => {
    const s = splitSentences("The system is scalable. It supports HA! Ready?");
    expect(s.map((x) => x.trim())).toEqual(["The system is scalable.", "It supports HA!", "Ready?"]);
  });

  it("称谓缩写不误切，但 Inc. 句末照切", () => {
    const s = splitSentences("Mr. Smith works at Acme Inc. The lead is Dr. Lee.");
    expect(s).toHaveLength(2);
    expect(s[0]).toContain("Acme Inc.");
  });

  it("小数与字母缩写点不切", () => {
    expect(splitSentences("Budget is 3.5M for the U.S.A. region.")).toHaveLength(1);
    expect(splitSentences("Use e.g. a gateway here.")).toHaveLength(1);
  });

  it("中英混排各按其标点切", () => {
    const s = splitSentences("系统采用 microservices 架构。Response time is under 300ms.");
    expect(s).toHaveLength(2);
  });
});

describe("chunkTypeUi", () => {
  it("已知类型有中文标签", () => {
    expect(chunkTypeUi("table_row").label).toBe("表格");
    expect(chunkTypeUi("heading").label).toBe("标题");
  });
  it("未知类型回落为原值", () => {
    expect(chunkTypeUi("weird").label).toBe("weird");
  });
  it("结构顺序覆盖四种段落级类型", () => {
    expect(CHUNK_TYPE_ORDER).toContain("table_row");
    expect(CHUNK_TYPE_ORDER).toContain("list_item");
  });
});
