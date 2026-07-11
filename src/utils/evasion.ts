// 规避特征呈现工具（§1.5 产品纪律的前端落点）：判级只驱动呈现权重，措辞统一
// 「检测到疑似规避特征，请人工复核」，机器不下「规避/串通/清白」结论。
// 判级逻辑（阈值）在 Rust engine::report::EvasionSummary，前端不重复；此处只做呈现分支与证据列举。
import type { EvasionSummaryDto } from "../api/types";

/** confirmed 才驱动 Library 文档卡徽标与 DocPreview 顶部告警条（§1.5：suspect 不打徽标不挂告警条）。 */
export function isEvasionConfirmed(e?: EvasionSummaryDto | null): boolean {
  return e?.severity === "confirmed";
}

/** 命中证据种类中文短标签（镜像 Rust EvasionSummary::evidence_kinds，供告警条列举可下钻线索）。 */
export function evasionEvidenceKinds(e: EvasionSummaryDto): string[] {
  const kinds: string[] = [];
  if (e.zeroWidth + e.bidi + e.tags + e.variation > 0) kinds.push("隐形码点");
  if (e.confusableFolds > 0) kinds.push("同形字");
  if (e.mixedScriptWords > 0) kinds.push("混合脚本");
  if (e.pdfHiddenChars > 0) kinds.push("PDF 隐藏文字");
  if (e.xcheckKind) kinds.push(e.xcheckLabel ?? "渲染-OCR 交叉验证");
  return kinds;
}
