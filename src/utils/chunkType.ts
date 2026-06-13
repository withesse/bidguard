// 分块类型的展示单一来源（标签 + 配色），供预览屏类型标记与筛选条共用。
// 与后端 chunker.rs 的 chunk_type 取值对齐（预览屏只见段落级：heading/paragraph/table_row/list_item）。
export interface ChunkTypeUi {
  label: string;
  fg: string;
  bg: string;
}

export const CHUNK_TYPE_UI: Record<string, ChunkTypeUi> = {
  heading: { label: "标题", fg: "#534AB7", bg: "rgba(83,74,183,0.12)" },
  paragraph: { label: "正文", fg: "#5F5E5A", bg: "rgba(95,94,90,0.12)" },
  table_row: { label: "表格", fg: "#0F6E56", bg: "rgba(15,110,86,0.12)" },
  list_item: { label: "清单", fg: "#854F0B", bg: "rgba(133,79,11,0.13)" },
  section: { label: "章节", fg: "#185FA5", bg: "rgba(24,95,165,0.12)" },
  sentence: { label: "句子", fg: "#888780", bg: "rgba(136,135,128,0.12)" },
};

export function chunkTypeUi(t: string): ChunkTypeUi {
  return CHUNK_TYPE_UI[t] ?? { label: t, fg: "#888780", bg: "rgba(136,135,128,0.12)" };
}

/** 筛选条里的固定展示顺序（结构从上到下）。 */
export const CHUNK_TYPE_ORDER = ["heading", "paragraph", "list_item", "table_row"];

/** 句子着色的轮换底色（低透明度，浅/深色模式均可读）。 */
export const SENTENCE_TINTS = [
  "rgba(83,74,183,0.13)",
  "rgba(15,110,86,0.13)",
  "rgba(133,79,11,0.14)",
  "rgba(212,83,126,0.13)",
];

// 与后端 chunker.rs SENTENCE_ABBREVS 一致：称谓/引用缩写后的 . 不视为句末。
const SENTENCE_ABBREVS = new Set([
  "mr", "mrs", "ms", "dr", "prof", "st", "sr", "jr", "messrs", "gov", "sen", "rep",
  "no", "vol", "pp", "fig", "eq", "sec", "ch",
]);

function abbrevBefore(chars: string[], dot: number): boolean {
  let word = "";
  for (let k = dot - 1; k >= 0 && /[A-Za-z]/.test(chars[k]); k--) word = chars[k] + word;
  if (!word) return false;
  return word.length === 1 || SENTENCE_ABBREVS.has(word.toLowerCase());
}

/**
 * 句子切分（中英双语，与后端 chunker.rs::split_sentences 同逻辑）用于着色展示：
 * 中文 。！？；与分号直接断；英文 .!? 仅当「后接空白 + 大写/数字/CJK/引号」且
 * 前词非缩写/单字母时才断（Mr. / U.S. / 3.5 / e.g. 不误切）。短句也单独切出以呈现边界。
 */
export function splitSentences(text: string): string[] {
  const chars = [...text];
  const n = chars.length;
  const out: string[] = [];
  let start = 0;
  for (let i = 0; i < n; i++) {
    const c = chars[i];
    let cut = false;
    if ("。！？；;".includes(c)) {
      cut = true;
    } else if (c === "." || c === "!" || c === "?") {
      let j = i + 1;
      while (j < n && "\"')]}”’".includes(chars[j])) j++;
      let nextOk: boolean;
      if (j >= n) {
        nextOk = true;
      } else if (/\s/.test(chars[j])) {
        let k = j;
        while (k < n && /\s/.test(chars[k])) k++;
        nextOk =
          k >= n ||
          /[A-Z0-9]/.test(chars[k]) ||
          chars[k].charCodeAt(0) >= 0x3400 ||
          "\"'“‘(".includes(chars[k]);
      } else {
        nextOk = false;
      }
      cut = nextOk && (c !== "." || !abbrevBefore(chars, i));
    }
    if (cut) {
      out.push(chars.slice(start, i + 1).join(""));
      start = i + 1;
    }
  }
  if (start < n) {
    const tail = chars.slice(start).join("");
    if (tail.trim()) out.push(tail);
  }
  return out;
}
