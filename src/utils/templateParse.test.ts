import { describe, expect, it } from "vitest";
import { parseTemplates, parseCsvRows } from "./templateParse";

describe("templateParse", () => {
  it("空行分段：每段一条，名称取首行", () => {
    const input = "标准售后承诺\n我方提供 7×24 小时支持。\n\n资质说明\n持有有效资质证书。";
    const rows = parseTemplates(input, "blank");
    expect(rows).toHaveLength(2);
    expect(rows[0].name).toBe("标准售后承诺");
    expect(rows[0].text).toContain("7×24");
    expect(rows[1].name).toBe("资质说明");
    expect(rows.every((r) => !r.error)).toBe(true);
  });

  it("空行分段：超长首行名称截断到 20 字", () => {
    const long = "一".repeat(30);
    const rows = parseTemplates(long, "blank");
    expect(rows[0].name.length).toBeLessThanOrEqual(21); // 20 + 省略号
    expect(rows[0].name.endsWith("…")).toBe(true);
  });

  it("逐行 分类|名称|正文：三列与两列", () => {
    const input = "法律|法规引用|根据招标投标法。\n资质目录|投标人具备资质。";
    const rows = parseTemplates(input, "pipe");
    expect(rows[0]).toMatchObject({ category: "法律", name: "法规引用" });
    expect(rows[0].text).toBe("根据招标投标法。");
    // 两列：名称|正文，未传 fallback 时分类为 null
    expect(rows[1]).toMatchObject({ category: null, name: "资质目录" });
    // 传 fallback 时两列行的分类落到 fallback
    const withFallback = parseTemplates("资质目录|投标人具备资质。", "pipe", "默认类");
    expect(withFallback[0].category).toBe("默认类");
  });

  it("逐行：缺分隔符的行标记无效", () => {
    const rows = parseTemplates("没有竖线的一行", "pipe");
    expect(rows[0].error).toBeTruthy();
  });

  it("CSV：表头 name,text,category + 引号转义", () => {
    const input = 'name,text,category\n售后,"含逗号, 的正文",承诺\n资质,正文二,';
    const rows = parseTemplates(input, "csv");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ name: "售后", category: "承诺" });
    expect(rows[0].text).toBe("含逗号, 的正文");
    expect(rows[1].category).toBeNull(); // 空 category 归一为 null
  });

  it("CSV：缺 name/text 列 → 报错行", () => {
    const rows = parseTemplates("foo,bar\na,b", "csv");
    expect(rows[0].error).toContain("name");
  });

  it("JSON：合法数组与非法输入", () => {
    const ok = parseTemplates('[{"name":"甲","text":"正文","category":"类一"}]', "json");
    expect(ok[0]).toMatchObject({ name: "甲", text: "正文", category: "类一" });

    const bad = parseTemplates("{not json", "json");
    expect(bad[0].error).toContain("JSON");

    const notArr = parseTemplates('{"name":"x"}', "json");
    expect(notArr[0].error).toContain("数组");

    const missing = parseTemplates('[{"name":"只有名"}]', "json");
    expect(missing[0].error).toBeTruthy();
  });

  it("空名或空正文统一标记无效", () => {
    const rows = parseTemplates("  |  | 只有分类", "pipe");
    expect(rows[0].error).toBeTruthy();
  });

  it("parseCsvRows：跨行引号字段", () => {
    const rows = parseCsvRows('a,"line1\nline2",c');
    expect(rows).toHaveLength(1);
    expect(rows[0][1]).toBe("line1\nline2");
  });
});
