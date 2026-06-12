import { describe, expect, it } from "vitest";
import { STEMS, docColor, docTag } from "./docTag";

describe("docTag", () => {
  it("十天干覆盖 0..9", () => {
    expect(docTag(0)).toBe("甲");
    expect(docTag(4)).toBe("戊");
    expect(docTag(9)).toBe("癸");
    expect(STEMS).toHaveLength(10);
  });

  it("超界回落「文N」而非 undefined", () => {
    expect(docTag(10)).toBe("文11");
    expect(docTag(15)).toBe("文16");
  });

  it("位次配色稳定且循环", () => {
    expect(docColor(0)).toBe(docColor(10));
    expect(docColor(3)).not.toBe(docColor(4));
    for (let i = 0; i < 12; i++) {
      expect(docColor(i)).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });
});
