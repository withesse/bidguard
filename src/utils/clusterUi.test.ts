import { describe, expect, it } from "vitest";
import { REVIEW_UI, SEVERITY_UI, TYPE_UI, severityUi, typeUi } from "./clusterUi";

describe("clusterUi", () => {
  it("八类差异类型全部有配色与标签", () => {
    const eight = ["same", "minor_change", "changed", "rewrite", "conflict", "uncertain", "added", "deleted"];
    for (const t of eight) {
      expect(TYPE_UI[t], t).toBeDefined();
      expect(TYPE_UI[t].label.length).toBeGreaterThan(0);
    }
  });

  it("未知类型回落而不崩", () => {
    const u = typeUi("future_type");
    expect(u.label).toBeTruthy();
    expect(u.fg).toMatch(/^#|^rgb/);
  });

  it("风险分级与确认状态映射齐全", () => {
    for (const s of ["high", "medium", "low", "review"]) {
      expect(SEVERITY_UI[s], s).toBeDefined();
    }
    expect(severityUi(null)).toBeNull();
    for (const r of ["pending", "confirmed", "ignored"]) {
      expect(REVIEW_UI[r], r).toBeDefined();
    }
  });
});
