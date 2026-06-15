// 批量导入查重源样板的四格式解析（纯函数，便于单测）。
// 空行分段 / 分类|名称|正文 逐行 / CSV / JSON。
import type { NewTemplateDto } from "../api/types";

export type ParseFormat = "blank" | "pipe" | "csv" | "json";

export interface ParsedRow extends NewTemplateDto {
  /** 无效原因；有值则该行不可导入。 */
  error?: string;
}

const NAME_MAX = 20;

function nameFrom(firstLine: string): string {
  const s = firstLine.trim();
  return s.length > NAME_MAX ? s.slice(0, NAME_MAX) + "…" : s;
}

function normCat(c: string | null | undefined): string | null {
  const s = (c ?? "").trim();
  return s.length ? s : null;
}

/** 空行分段：每段一条，名称取首行（≤20 字），正文为整段。 */
function parseBlank(input: string, fallbackCategory: string | null): ParsedRow[] {
  return input
    .split(/\n[ \t]*\n+/)
    .map((b) => b.trim())
    .filter(Boolean)
    .map((block) => {
      const firstLine = block.split("\n")[0] ?? "";
      const name = nameFrom(firstLine);
      return { category: fallbackCategory, name, text: block };
    });
}

/** 逐行「分类|名称|正文」：3 列；2 列时按 名称|正文（分类落 fallback）。 */
function parsePipe(input: string, fallbackCategory: string | null): ParsedRow[] {
  return input
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .map((line) => {
      const parts = line.split("|").map((p) => p.trim());
      if (parts.length >= 3) {
        const [category, name, ...rest] = parts;
        return { category: normCat(category) ?? fallbackCategory, name, text: rest.join("|") };
      }
      if (parts.length === 2) {
        return { category: fallbackCategory, name: parts[0], text: parts[1] };
      }
      return { category: fallbackCategory, name: "", text: line, error: "缺少分隔符「|」" };
    });
}

/** 最小 CSV 解析：支持双引号包裹、""转义、字段内逗号/换行。 */
export function parseCsvRows(input: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;
  let started = false;
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    started = true;
    if (inQuotes) {
      if (ch === '"') {
        if (input[i + 1] === '"') {
          field += '"';
          i++;
        } else inQuotes = false;
      } else field += ch;
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ",") {
      row.push(field);
      field = "";
    } else if (ch === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else if (ch !== "\r") {
      field += ch;
    }
  }
  if (started && (field.length > 0 || row.length > 0)) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

function parseCsv(input: string, fallbackCategory: string | null): ParsedRow[] {
  const rows = parseCsvRows(input).filter((r) => r.some((c) => c.trim().length));
  if (rows.length < 1) return [];
  const header = rows[0].map((h) => h.trim().toLowerCase());
  const iName = header.indexOf("name");
  const iText = header.indexOf("text");
  const iCat = header.indexOf("category");
  if (iName < 0 || iText < 0) {
    return [{ category: fallbackCategory, name: "", text: "", error: "CSV 表头需包含 name,text 两列" }];
  }
  return rows.slice(1).map((r) => ({
    category: iCat >= 0 ? normCat(r[iCat]) ?? fallbackCategory : fallbackCategory,
    name: (r[iName] ?? "").trim(),
    text: (r[iText] ?? "").trim(),
  }));
}

function parseJson(input: string, fallbackCategory: string | null): ParsedRow[] {
  let data: unknown;
  try {
    data = JSON.parse(input);
  } catch (e) {
    return [{ category: fallbackCategory, name: "", text: "", error: "JSON 解析失败：" + (e as Error).message }];
  }
  if (!Array.isArray(data)) {
    return [{ category: fallbackCategory, name: "", text: "", error: "JSON 顶层须为数组 [{name,text,category?}]" }];
  }
  return data.map((it) => {
    const o = (it ?? {}) as Record<string, unknown>;
    const name = typeof o.name === "string" ? o.name.trim() : "";
    const text = typeof o.text === "string" ? o.text.trim() : "";
    const category = typeof o.category === "string" ? normCat(o.category) ?? fallbackCategory : fallbackCategory;
    if (!name || !text) return { category, name, text, error: "缺少 name 或 text" };
    return { category, name, text };
  });
}

/** 解析为待导入行。fallbackCategory 用于未指定分类的行（通常为当前选中分类）。 */
export function parseTemplates(
  input: string,
  format: ParseFormat,
  fallbackCategory?: string | null,
): ParsedRow[] {
  const fc = normCat(fallbackCategory);
  if (!input.trim()) return [];
  const rows =
    format === "blank"
      ? parseBlank(input, fc)
      : format === "pipe"
        ? parsePipe(input, fc)
        : format === "csv"
          ? parseCsv(input, fc)
          : parseJson(input, fc);
  // 统一校验空名/空正文
  return rows.map((r) =>
    r.error ? r : !r.name.trim() || !r.text.trim() ? { ...r, error: "名称或正文为空" } : r,
  );
}
