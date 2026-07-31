import { describe, it, expect } from "vitest";
import type { EvaluationConfigDto, NumericPairDto } from "../api/types";
import {
  defaultPair,
  discountBand,
  evaluationError,
  flipStatement,
  identicalMatrix,
  isStrongCorrelation,
  mechanismFormulaText,
  mechanismPositionLabel,
  pairIndex,
  pairKey,
  patternLabel,
  priceSourceLabel,
  reasonLabel,
  shiftLabel,
  toSvgXY,
  MECHANISM_DISCLAIMER,
} from "./numericView";

const pair = (p: Partial<NumericPairDto> & Pick<NumericPairDto, "a" | "b">): NumericPairDto => ({
  comparable: 12,
  identical: 6,
  identicalRate: 0.5,
  alarm: false,
  reason: null,
  sharedArithErrors: [],
  ...p,
});

describe("identicalMatrix", () => {
  it("对称填充且对角线为 null（缺结论不得当 0 展示）", () => {
    const m = identicalMatrix([pair({ a: 0, b: 2, identicalRate: 0.9 })], 3);
    expect(m[0][2]).toBe(0.9);
    expect(m[2][0]).toBe(0.9);
    expect(m[1][1]).toBeNull();
    expect(m[0][1]).toBeNull();
  });

  it("不出结论的对（identicalRate=null）落 null 而非 0", () => {
    const m = identicalMatrix(
      [pair({ a: 0, b: 1, identicalRate: null, reason: "insufficient" })],
      2,
    );
    expect(m[0][1]).toBeNull();
  });
});

describe("pairKey / pairIndex / defaultPair", () => {
  it("无向键归一", () => {
    expect(pairKey(2, 0)).toBe(pairKey(0, 2));
  });

  it("按键取回文档对", () => {
    const idx = pairIndex([pair({ a: 0, b: 1 }), pair({ a: 1, b: 2 })]);
    expect(idx.get(pairKey(2, 1))?.a).toBe(1);
  });

  it("默认选中告警对，其次取雷同率最高", () => {
    const pairs = [
      pair({ a: 0, b: 1, identicalRate: 0.95 }),
      pair({ a: 0, b: 2, identicalRate: 0.82, alarm: true }),
    ];
    expect(defaultPair(pairs)?.b).toBe(2);
    const noAlarm = [pair({ a: 0, b: 1, identicalRate: 0.3 }), pair({ a: 1, b: 2, identicalRate: 0.7 })];
    expect(defaultPair(noAlarm)?.a).toBe(1);
    expect(defaultPair([])).toBeNull();
  });
});

describe("isStrongCorrelation", () => {
  it("只有 r>0.99 且比值 CV<0.5% 才算强证据", () => {
    expect(isStrongCorrelation({ n: 12, pearson: 0.995, spearman: 1, ratioCv: 0.001, note: "" })).toBe(true);
    // 天然同源导致的高相关：r 高但比值离散 → 不是强证据
    expect(isStrongCorrelation({ n: 12, pearson: 0.995, spearman: 1, ratioCv: 0.02, note: "" })).toBe(false);
    expect(isStrongCorrelation({ n: 12, pearson: 0.95, spearman: 1, ratioCv: 0.0001, note: "" })).toBe(false);
    expect(isStrongCorrelation({ n: 12, pearson: 0.999, spearman: 1, ratioCv: null, note: "" })).toBe(false);
    expect(isStrongCorrelation(null)).toBe(false);
  });
});

describe("patternLabel / reasonLabel", () => {
  it("形态标签与后端标识一一对应", () => {
    expect(patternLabel("geo_discount")).toContain("等比");
    expect(patternLabel("arith_seq")).toContain("等差");
    expect(patternLabel("affine")).toContain("仿射");
    expect(patternLabel("unknown_kind")).toBe("unknown_kind");
  });

  it("缺结论必须给出原因文案", () => {
    expect(reasonLabel("insufficient", 10)).toContain("不足 10 项");
    expect(reasonLabel(null, 10)).toContain("无可比清单项");
  });
});

describe("toSvgXY / discountBand", () => {
  it("y 轴翻转且裁剪到 [0,3]", () => {
    const { cx, cy } = toSvgXY({ alignKey: "k", name: null, x: 1, y: 1 }, 300);
    expect(cx).toBeCloseTo(100, 6);
    expect(cy).toBeCloseTo(200, 6);
    const hi = toSvgXY({ alignKey: "k", name: null, x: 9, y: -1 }, 300);
    expect(hi.cx).toBe(300);
    expect(hi.cy).toBe(300);
  });

  it("只有等比且系数明显偏离 1 才画折扣带", () => {
    expect(discountBand("geo_discount", 0.97)).toEqual({ slope: 0.97 });
    expect(discountBand("geo_discount", 1.0)).toBeNull();
    expect(discountBand("arith_seq", 0.97)).toBeNull();
    expect(discountBand("geo_discount", null)).toBeNull();
  });
});

describe("机制感知筛查（基准价敏感性）文案", () => {
  const ev = (p: Partial<EvaluationConfigDto> = {}): EvaluationConfigDto => ({
    method: "avg_benchmark",
    trimLowest: 1,
    trimHighest: 1,
    coeffMin: 0.9,
    coeffMax: 1.0,
    ...p,
  });

  it("公式全文回显含去高/去低与系数区间（人工录入需逐字核对）", () => {
    const t = mechanismFormulaText(ev());
    expect(t).toContain("去掉 1 个最高");
    expect(t).toContain("1 个最低");
    expect(t).toContain("0.9000");
    expect(t).toContain("1.0000");
    expect(t).toContain("最接近基准价者价格分最高");
    // 最低评标价法不得出现均值基准表述
    const low = mechanismFormulaText(ev({ method: "lowest" }));
    expect(low).toContain("最低评标价法");
    expect(low).not.toContain("算术平均");
  });

  it("性质声明必须写明不参与围标分级且需核对人工录入", () => {
    expect(MECHANISM_DISCLAIMER).toContain("不参与围标分级");
    expect(MECHANISM_DISCLAIMER).toContain("人工录入");
  });

  it("参数校验与后端同口径：去高去低之和须小于参评份数、系数区间须合法", () => {
    expect(evaluationError(ev(), 5)).toBeNull();
    expect(evaluationError(ev({ trimLowest: 3, trimHighest: 2 }), 5)).toContain("小于参评份数");
    expect(evaluationError(ev({ coeffMin: 1.1, coeffMax: 0.9 }), 5)).toContain("系数区间不合法");
    expect(evaluationError(ev({ coeffMin: 0 }), 5)).toContain("系数区间不合法");
    // 最低评标价法不校验均值参数
    expect(evaluationError(ev({ method: "lowest", trimLowest: 9 }), 2)).toBeNull();
  });

  it("反事实结论表述为占比而非概率", () => {
    const s = flipStatement(0.831);
    expect(s).toContain("83.1%");
    expect(s).toContain("的系数取值下改变");
    expect(s).not.toContain("概率");
  });

  it("来源打标与端点/偏移标签", () => {
    expect(priceSourceLabel("totalRow")).toBe("取自投标总价行");
    expect(priceSourceLabel("boqSum")).toBe("取自清单合计");
    expect(priceSourceLabel("heuristic")).toContain("启发式回落");
    expect(priceSourceLabel("boqSum", "后端下发标签")).toBe("后端下发标签");
    expect(mechanismPositionLabel("lowest")).toBe("报价最低端");
    expect(mechanismPositionLabel("highest")).toBe("报价最高端");
    expect(shiftLabel(1.6773)).toBe("+1.68%");
    expect(shiftLabel(-0.3458)).toBe("−0.35%");
  });
});
