// 结构化分块（设计文档 §8.4）：三档粒度同表共存（section/paragraph/sentence），
// 标题路径（docx 标题层级 + markdown #）、技术/商务标段分类、查重源模板标记。
// 每个分块同时产出特征（tokens/实体/MinHash），一次导入全部备齐。
use crate::db::repo::chunk_repo::NewChunk;
use crate::engine::features;
use crate::engine::normalize::{self, NormalizeOptions};
use crate::engine::parse::Block;
use crate::engine::segment::{self, Section};
use crate::engine::similarity::{cosine, tokenize_lang};
use jieba_rs::Jieba;

pub struct ChunkerOptions {
    pub min_chars: usize,
    /// 查重源模板：(模板 id, 分词结果)。命中（余弦 ≥ 0.7）的分块标记 is_template，
    /// 并记录命中的模板 id（取余弦最高者），用于统计每条样板命中过多少文档。
    /// 召回阶段剔除样板段，但仍可见可解释。
    pub templates: Vec<(String, Vec<String>)>,
    pub normalize: NormalizeOptions,
    /// false 时表格行退化为普通段落文本（parser.detectTable）。
    pub detect_table: bool,
    /// false 时分块不携带页码（parser.preservePageNumber）。
    pub preserve_page_number: bool,
    /// 分词语言：auto | zh | en（compare.language）。
    pub language: String,
}

impl Default for ChunkerOptions {
    fn default() -> Self {
        Self {
            min_chars: 10,
            templates: Vec::new(),
            normalize: NormalizeOptions::default(),
            detect_table: true,
            preserve_page_number: true,
            language: "auto".into(),
        }
    }
}

const TEMPLATE_MATCH: f32 = 0.7;
/// 无标题文档的 section 级分块按此长度截断，避免整本文档一个巨块。
const SECTION_MAX_CHARS: usize = 6000;

struct Ctx<'a> {
    jieba: &'a Jieba,
    opts: &'a ChunkerOptions,
    out: Vec<NewChunk>,
    order_para: i64,
    order_sent: i64,
    order_sect: i64,
    stack: Vec<(u8, String)>,
    sect_text: String,
    sect_page: Option<u32>,
    sect_path_json: Option<String>,
}

/// 把解析段块切成三档粒度的分块。order_index 在各粒度内独立编号。
pub fn chunk(jieba: &Jieba, blocks: &[Block], opts: &ChunkerOptions) -> Vec<NewChunk> {
    let mut ctx = Ctx {
        jieba,
        opts,
        out: Vec::new(),
        order_para: 0,
        order_sent: 0,
        order_sect: 0,
        stack: Vec::new(),
        sect_text: String::new(),
        sect_page: None,
        sect_path_json: None,
    };

    for b in blocks {
        if b.is_table_row {
            if ctx.opts.detect_table {
                table_row(&mut ctx, b.text.trim(), b.page);
            } else {
                // 关闭表格识别：行文本按普通段落处理
                paragraph(&mut ctx, b.text.trim(), b.page, "paragraph");
            }
            continue;
        }
        if let Some(level) = b.heading_level {
            heading(&mut ctx, level, b.text.trim(), b.page);
            continue;
        }
        if b.is_list_item {
            // docx 编号/项目符号段落（w:numPr）
            paragraph(&mut ctx, b.text.trim(), b.page, "list_item");
            continue;
        }
        for line in b.text.split('\n') {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            // markdown 标题：# 的个数即层级
            if let Some(lvl) = md_heading_level(t) {
                let title = t.trim_start_matches('#').trim();
                if !title.is_empty() {
                    heading(&mut ctx, lvl, title, b.page);
                }
                continue;
            }
            // markdown / 纯文本表格行（| 分隔）
            if ctx.opts.detect_table {
                if let Some(row) = plain_table_row(t) {
                    if !row.is_empty() {
                        table_row(&mut ctx, &row, b.page);
                    }
                    continue; // 分隔行（|---|---|）整行丢弃
                }
            }
            let ptype = if is_list_line(t) { "list_item" } else { "paragraph" };
            paragraph(&mut ctx, t, b.page, ptype);
        }
    }
    flush_section(&mut ctx);
    ctx.out
}

/// md / 纯文本列表项：「- / * / • / ·」+ 空白，或「1.」「1、」「1)」「(1)」式编号。
/// 编号后紧跟数字不算（「3.5 系统设计」是小节号不是列表）。
fn is_list_line(line: &str) -> bool {
    let cs: Vec<char> = line.chars().take(8).collect();
    match cs.first() {
        Some('-' | '*' | '•' | '·') => cs.get(1).is_some_and(|c| c.is_whitespace()),
        Some('（' | '(') => {
            let digits = cs[1..].iter().take_while(|c| c.is_ascii_digit()).count();
            (1..=3).contains(&digits) && matches!(cs.get(1 + digits), Some(')' | '）'))
        }
        Some(c) if c.is_ascii_digit() => {
            let digits = cs.iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 3 {
                return false;
            }
            matches!(cs.get(digits), Some('.' | '、' | ')' | '）'))
                && cs.get(digits + 1).is_none_or(|c| !c.is_ascii_digit())
        }
        _ => false,
    }
}

/// md / 纯文本表格行：含 | 且拆出 ≥2 个非空单元格 → 归一为「a | b | c」。
/// 分隔行（单元格全由 -/: 组成）返回 Some("")，调用方丢弃。非表格行返回 None。
fn plain_table_row(line: &str) -> Option<String> {
    if !line.contains('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    let filled: Vec<&str> = cells.iter().copied().filter(|c| !c.is_empty()).collect();
    if filled.len() < 2 {
        return None;
    }
    if filled
        .iter()
        .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
    {
        return Some(String::new());
    }
    Some(cells.join(" | "))
}

fn md_heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

fn heading(ctx: &mut Ctx, level: u8, title: &str, page: Option<u32>) {
    // 新标题开启新 section：先冲刷累计内容
    flush_section(ctx);
    ctx.stack.retain(|(l, _)| *l < level);
    ctx.stack.push((level, title.to_string()));
    ctx.sect_path_json = path_json(&ctx.stack);
    if title.chars().count() >= 2 {
        let order = ctx.order_para;
        ctx.order_para += 1;
        let c = make(ctx, title, "heading", "paragraph", page, order);
        ctx.out.push(c);
    }
    // 标题本身计入新 section 内容
    ctx.sect_text.push_str(title);
    ctx.sect_text.push('\n');
    ctx.sect_page = ctx.sect_page.or(page);
}

/// para_type: "paragraph" | "list_item"（列表项在段落级保留结构类型，句子级照常拆句）。
fn paragraph(ctx: &mut Ctx, text: &str, page: Option<u32>, para_type: &str) {
    // section 累计（与粒度过滤无关，保持原文连贯）
    ctx.sect_text.push_str(text);
    ctx.sect_text.push('\n');
    ctx.sect_page = ctx.sect_page.or(page);
    if ctx.sect_text.chars().count() > SECTION_MAX_CHARS {
        flush_section(ctx);
    }

    if text.chars().count() >= ctx.opts.min_chars {
        let order = ctx.order_para;
        ctx.order_para += 1;
        let c = make(ctx, text, para_type, "paragraph", page, order);
        ctx.out.push(c);
    }

    for piece in split_sentences(text) {
        let s = piece.trim();
        if s.chars().count() < ctx.opts.min_chars {
            continue;
        }
        let order = ctx.order_sent;
        ctx.order_sent += 1;
        let c = make(ctx, s, "sentence", "sentence", page, order);
        ctx.out.push(c);
    }
}

/// 「必跟名字/数字」的称谓与引用缩写（小写），其后的 `.` 不视为句末。
/// 不含 Inc./Ltd./Co./etc. —— 这些常常本身就是句末，交给「后接大写」规则判断；
/// e.g./i.e./U.S.A. 等由「单字母缩写」规则兜住（每个点前都是单字母）。
const SENTENCE_ABBREVS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "st", "sr", "jr", "messrs", "gov", "sen", "rep",
    "no", "vol", "pp", "fig", "eq", "sec", "ch",
];

/// 句子切分（中英双语）：
/// 中文 。！？；与分号 ; 无歧义，直接断；
/// 英文 .!? 仅当「后接空白 + 大写/数字/CJK/引号」且前词非缩写或单字母时才断
/// （避免 Mr. / U.S. / 3.5 / e.g. 被误切）。返回原文切片。
pub fn split_sentences(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0usize;
    for i in 0..n {
        let (bi, c) = chars[i];
        let cut = if matches!(c, '。' | '！' | '？' | '；' | ';') {
            true
        } else if matches!(c, '.' | '!' | '?') {
            // 跳过句末右引号/括号后看下一个有内容的字符
            let mut j = i + 1;
            while j < n && matches!(chars[j].1, '"' | '\'' | ')' | ']' | '}' | '”' | '’') {
                j += 1;
            }
            let next_ok = if j >= n {
                true
            } else if chars[j].1.is_whitespace() {
                let mut k = j;
                while k < n && chars[k].1.is_whitespace() {
                    k += 1;
                }
                k >= n || {
                    let nc = chars[k].1;
                    nc.is_uppercase()
                        || nc.is_ascii_digit()
                        || nc as u32 >= 0x3400
                        || matches!(nc, '"' | '\'' | '“' | '‘' | '(')
                }
            } else {
                false // 后面无空白（小数 3.5 / 缩写 U.S. / 网址）→ 非句末
            };
            next_ok && (c != '.' || !abbrev_before(&chars, i))
        } else {
            false
        };
        if cut {
            let end = bi + c.len_utf8();
            out.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        let tail = &text[start..];
        if !tail.trim().is_empty() {
            out.push(tail);
        }
    }
    out
}

/// `.` 前的连续字母构成的词是否为缩写或单字母缩写（如 Mr / U / e）。
fn abbrev_before(chars: &[(usize, char)], dot: usize) -> bool {
    let mut word: Vec<char> = Vec::new();
    let mut k = dot;
    while k > 0 {
        k -= 1;
        let ch = chars[k].1;
        if ch.is_ascii_alphabetic() {
            word.push(ch);
        } else {
            break;
        }
    }
    if word.is_empty() {
        return false;
    }
    let w: String = word.iter().rev().collect();
    w.chars().count() == 1 || SENTENCE_ABBREVS.contains(&w.to_ascii_lowercase().as_str())
}

/// 表格行是原子比对单元：段落级与句子级各产出一份（不拆句），并累入 section 原文。
/// 报价表/清单的雷同与金额冲突由此进入召回-评分-事实链路。
fn table_row(ctx: &mut Ctx, text: &str, page: Option<u32>) {
    if text.is_empty() {
        return;
    }
    ctx.sect_text.push_str(text);
    ctx.sect_text.push('\n');
    ctx.sect_page = ctx.sect_page.or(page);
    if ctx.sect_text.chars().count() > SECTION_MAX_CHARS {
        flush_section(ctx);
    }

    if text.chars().count() < ctx.opts.min_chars {
        return;
    }
    let order = ctx.order_para;
    ctx.order_para += 1;
    let c = make(ctx, text, "table_row", "paragraph", page, order);
    ctx.out.push(c);

    let order = ctx.order_sent;
    ctx.order_sent += 1;
    let c = make(ctx, text, "table_row", "sentence", page, order);
    ctx.out.push(c);
}

fn flush_section(ctx: &mut Ctx) {
    let text = std::mem::take(&mut ctx.sect_text);
    let page = ctx.sect_page.take();
    let t = text.trim();
    if t.chars().count() >= ctx.opts.min_chars {
        let order = ctx.order_sect;
        ctx.order_sect += 1;
        let c = make(ctx, t, "section", "section", page, order);
        ctx.out.push(c);
    }
}

fn path_json(stack: &[(u8, String)]) -> Option<String> {
    if stack.is_empty() {
        None
    } else {
        serde_json::to_string(&stack.iter().map(|(_, t)| t).collect::<Vec<_>>()).ok()
    }
}

fn make(
    ctx: &Ctx,
    text: &str,
    chunk_type: &str,
    chunk_level: &str,
    page: Option<u32>,
    order_index: i64,
) -> NewChunk {
    let page = if ctx.opts.preserve_page_number { page } else { None };
    // 带统计归一化（W2 入口对抗层）：normalized_text/normalized_hash 与全部特征
    // （tokens/entities/ngrams/minhash）基于清洗后文本，恢复被隐形码点/同形字破坏的
    // 一致性；块级统计随 NewChunk 落 chunk_features.extra_json（定位「扰动集中在
    // 哪些块」），text 保留原始字节供取证下钻。分两步取中间产物：分词吃 sanitize
    // 产物而非原文（词内零宽/同形注入会拆碎 token，击穿 lexical 通道），也非归一
    // 终态（cn_numbers/去标点改变词面，偏离既有分词口径）；模板侧分词同口径，见
    // import_service::run_import。
    let (sanitized, evasion) = normalize::sanitize_with_stats(text);
    let normalized = normalize::normalize_sanitized(&sanitized, &ctx.opts.normalize);
    let tokens = tokenize_lang(ctx.jieba, &sanitized, &ctx.opts.language);
    // 命中余弦最高的样板（≥ 阈值）：标记 is_template 并记录其 id 供命中统计。
    let mut template_id: Option<String> = None;
    let mut best = -1.0f32;
    for (id, tt) in &ctx.opts.templates {
        let c = cosine(&tokens, tt);
        if c >= TEMPLATE_MATCH && c > best {
            best = c;
            template_id = Some(id.clone());
        }
    }
    let is_template = template_id.is_some();
    let section_kind = match segment::classify(text) {
        Section::Tech => "tech",
        Section::Business => "business",
        Section::Other => "other",
    };
    let entities = features::extract_entities(&normalized);
    let ngrams = features::char_ngrams(&normalized);
    NewChunk {
        chunk_type: chunk_type.to_string(),
        chunk_level: chunk_level.to_string(),
        section_path: ctx.sect_path_json.clone(),
        section_kind: Some(section_kind.to_string()),
        is_template,
        template_id,
        text: text.to_string(),
        normalized_text: normalized.clone(),
        page,
        order_index,
        start_offset: None,
        end_offset: None,
        exact_hash: normalize::sha256_hex(text.as_bytes()),
        normalized_hash: normalize::sha256_hex(normalized.as_bytes()),
        token_json: serde_json::to_string(&tokens).ok(),
        entity_json: serde_json::to_string(&entities).ok(),
        minhash_blob: Some(features::minhash_to_blob(&features::minhash(&ngrams))),
        evasion: if evasion.is_clean() { None } else { Some(evasion) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::similarity::tokenize;

    #[test]
    fn split_sentences_cjk_and_english() {
        // 中文按 。！？；切
        let s = split_sentences("本项目采用微服务架构。平台支持横向扩展！是否可行？");
        assert_eq!(s, vec!["本项目采用微服务架构。", "平台支持横向扩展！", "是否可行？"]);

        // 英文按 . ! ? 切（后接空格+大写）
        let s = split_sentences("The system is scalable. It supports high concurrency! Is it ready?");
        assert_eq!(s.len(), 3, "英文应切成 3 句：{s:?}");
        assert_eq!(s[0].trim(), "The system is scalable.");

        // 缩写不误切：Mr. / Inc. / e.g.
        let s = split_sentences("Mr. Smith works at Acme Inc. The project is led by Dr. Lee.");
        assert_eq!(s.len(), 2, "Mr./Inc./Dr. 不应被当句末：{s:?}");
        assert!(s[0].contains("Acme Inc."));

        // 小数与缩写点不切：3.5 / U.S.A.
        let s = split_sentences("The budget is 3.5 million USD for the U.S.A. region.");
        assert_eq!(s.len(), 1, "小数与 U.S.A. 内的点不应切：{s:?}");

        // 中英混排
        let s = split_sentences("系统采用 microservices 架构。Response time is under 300ms.");
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn english_paragraph_splits_into_sentences() {
        let jieba = Jieba::new();
        let text = "The platform adopts a microservices architecture for horizontal scaling. Each subsystem is independently deployable and observable. All capabilities are exposed through a unified API gateway.";
        let chunks = chunk(&jieba, &blocks_md(text), &ChunkerOptions::default());
        let sents: Vec<_> = chunks.iter().filter(|c| c.chunk_level == "sentence").collect();
        assert_eq!(sents.len(), 3, "英文段落应切成 3 个句子级块，而非整段一句：{}", sents.len());
        assert!(sents[0].text.contains("microservices architecture"));
    }

    fn blocks_md(text: &str) -> Vec<Block> {
        vec![Block {
            text: text.to_string(),
            heading_level: None,
            page: None,
            is_table_row: false,
            is_list_item: false,
        }]
    }

    #[test]
    fn three_levels_with_md_headings() {
        let jieba = Jieba::new();
        let text = "# 第一章 总体方案\n本项目采用分层解耦的微服务总体架构设计。平台支持横向扩展与读写分离机制。\n## 1.1 技术架构\n系统自下而上划分为基础设施层与业务应用层。";
        let chunks = chunk(&jieba, &blocks_md(text), &ChunkerOptions::default());

        let paras: Vec<_> = chunks.iter().filter(|c| c.chunk_level == "paragraph").collect();
        let sents: Vec<_> = chunks.iter().filter(|c| c.chunk_level == "sentence").collect();
        let sects: Vec<_> = chunks.iter().filter(|c| c.chunk_level == "section").collect();

        assert!(paras.iter().any(|c| c.chunk_type == "heading" && c.text == "第一章 总体方案"));
        // 段落级保留整段；句子级把两句拆开（保留句末标点，与前端着色一致）
        assert!(paras.iter().any(|c| c.text.contains("微服务总体架构设计。平台支持")));
        assert!(sents.iter().any(|c| c.text == "本项目采用分层解耦的微服务总体架构设计。"));
        assert!(sents.iter().any(|c| c.text == "平台支持横向扩展与读写分离机制。"));
        assert_eq!(sects.len(), 2, "两个标题 → 两个 section");

        // 章节路径：1.1 下的内容路径应含两级
        let deep = paras.iter().find(|c| c.text.contains("基础设施层")).unwrap();
        let path: Vec<String> = serde_json::from_str(deep.section_path.as_ref().unwrap()).unwrap();
        assert_eq!(path, vec!["第一章 总体方案", "1.1 技术架构"]);

        // 特征备齐
        assert!(deep.token_json.is_some() && deep.minhash_blob.is_some());
    }

    #[test]
    fn docx_table_row_blocks_become_atomic_chunks() {
        let jieba = Jieba::new();
        let blocks = vec![
            Block { text: "第三章 报价部分".into(), heading_level: Some(1), page: None, is_table_row: false, is_list_item: false },
            Block { text: "1 | 核心交换机。含安装调试 | 64000元 | 工期30天".into(), heading_level: None, page: Some(5), is_table_row: true, is_list_item: false },
        ];
        let chunks = chunk(&jieba, &blocks, &ChunkerOptions::default());
        let rows: Vec<_> = chunks.iter().filter(|c| c.chunk_type == "table_row").collect();
        // 段落级 + 句子级各一份，且不按「。」拆句
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|c| c.chunk_level == "paragraph"));
        assert!(rows.iter().any(|c| c.chunk_level == "sentence"));
        assert!(rows.iter().all(|c| c.text.contains("核心交换机。含安装调试")), "表格行不拆句");
        assert_eq!(rows[0].page, Some(5));
        assert!(rows[0].section_path.as_ref().unwrap().contains("报价部分"));
        // 行内金额进实体（事实冲突链路的输入）
        assert!(rows[0].entity_json.as_ref().unwrap().contains("amount"));
        // section 级累入表格内容
        let sect = chunks.iter().find(|c| c.chunk_level == "section").unwrap();
        assert!(sect.text.contains("核心交换机"));
    }

    #[test]
    fn markdown_tables_become_row_chunks() {
        let jieba = Jieba::new();
        let text = "## 报价清单\n| 序号 | 设备名称 | 单价 |\n|---|---|---|\n| 1 | 核心交换机设备 | 64000元 |\n以上报价均含税及运输费用。";
        let chunks = chunk(&jieba, &blocks_md(text), &ChunkerOptions::default());
        let rows: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == "table_row" && c.chunk_level == "paragraph")
            .collect();
        assert_eq!(rows.len(), 2, "表头 + 数据行；分隔行丢弃");
        assert_eq!(rows[0].text, "序号 | 设备名称 | 单价");
        assert_eq!(rows[1].text, "1 | 核心交换机设备 | 64000元");
        // 普通段落不受影响
        assert!(chunks.iter().any(|c| c.chunk_type == "paragraph" && c.text.contains("含税")));
    }

    #[test]
    fn detect_table_off_degrades_rows_to_paragraphs() {
        let jieba = Jieba::new();
        let blocks = vec![Block {
            text: "1 | 核心交换机及配套光模块 | 64000元".into(),
            heading_level: None,
            page: None,
            is_table_row: true,
            is_list_item: false,
        }];
        let opts = ChunkerOptions { detect_table: false, ..Default::default() };
        let chunks = chunk(&jieba, &blocks, &opts);
        assert!(chunks.iter().all(|c| c.chunk_type != "table_row"), "关闭表格识别后不应产出表格行");
        assert!(
            chunks.iter().any(|c| c.chunk_type == "paragraph" && c.text.contains("核心交换机")),
            "行文本应按普通段落处理"
        );
        // md 表格行同样不识别
        let md = chunk(&jieba, &blocks_md("| 序号 | 设备名称 | 单价 |\n|---|---|---|"), &opts);
        assert!(md.iter().all(|c| c.chunk_type != "table_row"));
    }

    #[test]
    fn preserve_page_number_off_strips_pages() {
        let jieba = Jieba::new();
        let blocks = vec![Block {
            text: "投标报价为人民币12800000元整，包含全部软硬件费用。".into(),
            heading_level: None,
            page: Some(7),
            is_table_row: false,
            is_list_item: false,
        }];
        let opts = ChunkerOptions { preserve_page_number: false, ..Default::default() };
        let chunks = chunk(&jieba, &blocks, &opts);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.page.is_none()), "关闭页码保留后分块不应带页码");
    }

    #[test]
    fn normalize_options_flow_into_chunks() {
        let jieba = Jieba::new();
        let blocks = blocks_md("投标报价为人民币壹佰万元整，ABC 系统平台。");
        // 关闭忽略大小写：normalized_text 应保留大写
        let keep_case = ChunkerOptions {
            normalize: crate::engine::normalize::NormalizeOptions {
                ignore_case: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let a = chunk(&jieba, &blocks, &keep_case);
        let b = chunk(&jieba, &blocks, &ChunkerOptions::default());
        let pa = a.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        let pb = b.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        assert!(pa.normalized_text.contains("ABC"), "保留大小写：{}", pa.normalized_text);
        assert!(pb.normalized_text.contains("abc"), "默认忽略大小写：{}", pb.normalized_text);
        assert_ne!(pa.normalized_hash, pb.normalized_hash, "不同归一配置应产出不同 hash");
    }

    #[test]
    fn list_items_get_structural_type() {
        let jieba = Jieba::new();
        let text = "服务承诺如下：\n- 提供原厂三年质保服务支持\n1. 七乘二十四小时电话响应机制\n（2）重大故障四小时内到场处理\n3.5 系统总体架构设计说明";
        let chunks = chunk(&jieba, &blocks_md(text), &ChunkerOptions::default());
        let lists: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == "list_item" && c.chunk_level == "paragraph")
            .collect();
        assert_eq!(lists.len(), 3, "三个列表项：{:?}", chunks.iter().map(|c| (&c.chunk_type, &c.text)).collect::<Vec<_>>());
        assert!(lists.iter().any(|c| c.text.contains("三年质保")));
        // 「3.5 系统…」是小节号不是列表
        assert!(chunks.iter().any(|c| c.chunk_type == "paragraph" && c.text.contains("3.5 系统")));
        // docx numPr 块
        let blocks = vec![Block {
            text: "提供原厂三年质保服务支持".into(),
            heading_level: None,
            page: None,
            is_table_row: false,
            is_list_item: true,
        }];
        let chunks = chunk(&jieba, &blocks, &ChunkerOptions::default());
        assert!(chunks.iter().any(|c| c.chunk_type == "list_item"));
    }

    #[test]
    fn is_list_line_detection() {
        for s in ["- 第一项内容", "* 第二项内容", "• 第三项", "1. 编号项", "12、编号项", "(3) 括号编号", "（3）全角括号"] {
            assert!(is_list_line(s), "{s}");
        }
        for s in ["3.5 系统设计", "2026年计划", "普通段落文本", "-连字符开头无空格", "1280万元报价"] {
            assert!(!is_list_line(s), "{s}");
        }
    }

    #[test]
    fn plain_table_row_detection() {
        assert_eq!(plain_table_row("| a | b |"), Some("a | b".into()));
        assert_eq!(plain_table_row("1 | 服务器 | 2台"), Some("1 | 服务器 | 2台".into()));
        assert_eq!(plain_table_row("|---|:---:|"), Some(String::new()), "分隔行");
        assert_eq!(plain_table_row("纯文本没有分隔"), None);
        assert_eq!(plain_table_row("| 只有一格 |"), None);
    }

    #[test]
    fn template_chunks_are_marked_not_dropped() {
        let jieba = Jieba::new();
        let tpl = "我方承诺提供7×24小时技术支持服务，质保期内免费维护，确保系统稳定运行";
        let opts = ChunkerOptions {
            templates: vec![("tpl-1".to_string(), tokenize(&jieba, tpl))],
            ..Default::default()
        };
        let text = format!("{tpl}。\n本项目采用独有的边缘计算架构与自研调度算法。");
        let chunks = chunk(&jieba, &blocks_md(&text), &opts);
        let tpl_chunk = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.text.contains("7×24"))
            .unwrap();
        assert!(tpl_chunk.is_template, "命中模板应标记");
        assert_eq!(tpl_chunk.template_id.as_deref(), Some("tpl-1"), "应记录命中的样板 id");
        let normal = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.text.contains("边缘计算"))
            .unwrap();
        assert!(!normal.is_template);
        assert!(normal.template_id.is_none());
    }

    #[test]
    fn tokens_come_from_sanitized_text() {
        // W2-1「全部特征基于清洗后文本」：token_json 也是特征列。词内零宽拆词
        // （微服\u{200B}务）与同形字替换（Pагe）不得拆碎/变形 token——否则
        // normalized_hash/MinHash 恢复了命中，权重最高的 lexical 通道（tfidf 余弦、
        // 共有词交集、模板余弦）仍被击穿
        let jieba = Jieba::new();
        let clean = "本项目采用分层解耦的微服务总体架构设计方案（Page 编号）。";
        let dirty = "本项目采用分层解耦的微服\u{200B}务总体架构设计方案\
                     （P\u{0430}\u{0433}\u{0435} 编号）。";
        let a = chunk(&jieba, &blocks_md(clean), &ChunkerOptions::default());
        let b = chunk(&jieba, &blocks_md(dirty), &ChunkerOptions::default());
        let pa = a.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        let pb = b.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        assert_eq!(pa.token_json, pb.token_json, "扰动块 tokens 应与干净文本一致");
        assert!(pb.token_json.as_ref().unwrap().contains("Page"), "同形字应折回拉丁词面");
        // 回归护栏：直接对原文分词（修复前行为）产出的词面与清洗后不同——
        // 同形词以西里尔原貌入 token，lexical 通道两侧词面失配
        let raw = serde_json::to_string(&tokenize(&jieba, dirty)).unwrap();
        assert!(!raw.contains("Page"), "原文分词不该有拉丁词面（否则本测试失去意义）");
    }

    #[test]
    fn template_match_survives_invisible_injection() {
        // 样板剔除的对抗面：雷同段落词内插零宽后，分词吃清洗产物，模板余弦不应失配
        let jieba = Jieba::new();
        let tpl = "我方承诺提供7×24小时技术支持服务，质保期内免费维护，确保系统稳定运行";
        let opts = ChunkerOptions {
            templates: vec![("tpl-1".to_string(), tokenize(&jieba, tpl))],
            ..Default::default()
        };
        let dirty = "我方承诺提供7×24小时技术支\u{200B}持服务，质\u{200B}保期内免费维\u{200B}护，确保系统稳定运行。";
        let chunks = chunk(&jieba, &blocks_md(dirty), &opts);
        let para = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.text.contains("7×24"))
            .unwrap();
        assert!(para.is_template, "零宽注入不应击穿模板匹配");
        assert_eq!(para.template_id.as_deref(), Some("tpl-1"));
    }

    #[test]
    fn evasion_stats_flow_into_chunks() {
        let jieba = Jieba::new();
        let clean = "本项目采用分层解耦的微服务总体架构设计方案。";
        let dirty = "本项目采用分层\u{200B}解耦的微服务总体架构\u{200B}设计方案。";
        let a = chunk(&jieba, &blocks_md(clean), &ChunkerOptions::default());
        let b = chunk(&jieba, &blocks_md(dirty), &ChunkerOptions::default());
        let pa = a.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        let pb = b.iter().find(|c| c.chunk_level == "paragraph").unwrap();
        // 隐形字符不破坏 normalized_hash（词面通道恢复命中），原文保留供取证
        assert_eq!(pa.normalized_hash, pb.normalized_hash);
        assert_ne!(pa.exact_hash, pb.exact_hash, "exact_hash 基于原始字节，应不同");
        assert!(pb.text.contains('\u{200B}'), "chunks.text 保留原始字节");
        // 统计只在有发现的块上携带
        assert!(pa.evasion.is_none());
        let ev = pb.evasion.as_ref().unwrap();
        assert_eq!(ev.zero_width, 2);
        assert_eq!(ev.stripped_total(), 2);
    }

    #[test]
    fn docx_heading_blocks_build_section_path() {
        let jieba = Jieba::new();
        let blocks = vec![
            Block { text: "第一章 商务部分".into(), heading_level: Some(1), page: None, is_table_row: false, is_list_item: false },
            Block { text: "投标报价为人民币12800000元整，包含全部软硬件费用。".into(), heading_level: None, page: Some(3), is_table_row: false, is_list_item: false },
        ];
        let chunks = chunk(&jieba, &blocks, &ChunkerOptions::default());
        let para = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.chunk_type == "paragraph")
            .unwrap();
        assert_eq!(para.page, Some(3));
        assert!(para.section_path.as_ref().unwrap().contains("商务部分"));
        assert_eq!(para.section_kind.as_deref(), Some("business"));
        // 实体抽取到金额
        assert!(para.entity_json.as_ref().unwrap().contains("amount"));
    }
}
