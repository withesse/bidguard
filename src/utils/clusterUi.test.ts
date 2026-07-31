import { describe, expect, it } from "vitest";
import {
  BAND_UI,
  CALIBRATION_QUALIFIER,
  RERANK_DISCLAIMER,
  REVIEW_UI,
  SEVERITY_UI,
  TYPE_UI,
  bandUi,
  rerankUi,
  routingNote,
  severityUi,
  typeUi,
} from "./clusterUi";

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

  // 复核路由三带（W6-4）：文案是产品硬约束（方案 §1.5-1），改一个字就该红。
  it("三带命名固定为「低优先级抽查 / 需人工复核 / 重点标红」", () => {
    expect(BAND_UI.pass.label).toBe("低优先级抽查");
    expect(BAND_UI.review.label).toBe("需人工复核");
    expect(BAND_UI.flag.label).toBe("重点标红");
    expect(BAND_UI.uncalibrated.label).toBe("未校准");
  });

  it("三带文案禁用「自动放行 / 漏检保证」，且 α 相关文案必须限定「合成校准语料」", () => {
    const all = [
      ...Object.values(BAND_UI).map((b) => `${b.label}${b.hint}`),
      CALIBRATION_QUALIFIER,
      routingNote("three-band", 0.05, 0.05),
      routingNote("review-all"),
      routingNote(undefined),
    ].join("\n");
    expect(all).not.toContain("自动放行");
    expect(all).not.toContain("漏检保证");
    // 三带生效时才谈 α/β，且必须写明是在合成校准语料上测得。
    const three = routingNote("three-band", 0.05, 0.05);
    expect(three).toContain("α=5%");
    expect(three).toContain("合成校准语料");
    expect(three).toContain("不是对真实标书的承诺");
    // pass 带说明必须写明「不隐藏」，避免被读成放行。
    expect(BAND_UI.pass.hint).toContain("完整保留");
  });

  it("band=null（旧任务/未校准）回落未校准档而不是留空", () => {
    expect(bandUi(null).label).toBe("未校准");
    expect(bandUi(undefined).label).toBe("未校准");
    expect(bandUi("future_band").label).toBe("未校准");
    expect(bandUi("flag").label).toBe("重点标红");
  });

  it("置信度限定语点名「不是串通概率」（§1.5-2 检察官谬误）", () => {
    expect(CALIBRATION_QUALIFIER).toContain("不是串通概率");
    expect(CALIBRATION_QUALIFIER).toContain("合成校准语料");
  });

  // 交叉复核（W6-2，§1.5-3）：cross-encoder 是黑盒且为检索相关性训练，
  // 它只给【复核排序建议】，绝不改判分类——文案里不得出现断言性措辞。
  it("复核倾向徽标写明分数与倾向，未复核时返回 null（不留空白）", () => {
    expect(rerankUi(0.83)?.label).toBe("AI 复核倾向：洗稿（0.83）");
    expect(rerankUi(0.21)?.label).toBe("AI 复核倾向：无关（0.21）");
    // null ≠「已复核且无嫌疑」：没跑复核层就不给徽标，由调用方决定怎么呈现「未复核」
    expect(rerankUi(null)).toBeNull();
    expect(rerankUi(undefined)).toBeNull();
    expect(rerankUi(Number.NaN)).toBeNull();
  });

  it("复核建议文案禁用断言性措辞，且必须写明「不改变分类、需人工确认」", () => {
    const all = [RERANK_DISCLAIMER, rerankUi(0.99)!.hint, rerankUi(0.01)!.hint].join("\n");
    for (const banned of ["判定", "认定", "确认为", "串通", "自动改判"]) {
      expect(all).not.toContain(banned);
    }
    expect(RERANK_DISCLAIMER).toContain("不改变条款分类");
    expect(RERANK_DISCLAIMER).toContain("人工确认");
    expect(RERANK_DISCLAIMER).toContain("排序建议");
  });
});
