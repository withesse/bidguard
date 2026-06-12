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
    /// 查重源模板的分词结果；命中（余弦 ≥ 0.7）的分块标记 is_template，
    /// 召回阶段剔除，但仍可见可解释。
    pub template_tokens: Vec<Vec<String>>,
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
            template_tokens: Vec::new(),
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

    for piece in text.split(['。', '！', '？', '；', ';']) {
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
    let normalized = normalize::normalize(text, &ctx.opts.normalize);
    let tokens = tokenize_lang(ctx.jieba, text, &ctx.opts.language);
    let is_template = ctx
        .opts
        .template_tokens
        .iter()
        .any(|tt| cosine(&tokens, tt) >= TEMPLATE_MATCH);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::similarity::tokenize;

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
        // 段落级保留整段；句子级把两句拆开
        assert!(paras.iter().any(|c| c.text.contains("微服务总体架构设计。平台支持")));
        assert!(sents.iter().any(|c| c.text == "本项目采用分层解耦的微服务总体架构设计"));
        assert!(sents.iter().any(|c| c.text == "平台支持横向扩展与读写分离机制"));
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
            template_tokens: vec![tokenize(&jieba, tpl)],
            ..Default::default()
        };
        let text = format!("{tpl}。\n本项目采用独有的边缘计算架构与自研调度算法。");
        let chunks = chunk(&jieba, &blocks_md(&text), &opts);
        let tpl_chunk = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.text.contains("7×24"))
            .unwrap();
        assert!(tpl_chunk.is_template, "命中模板应标记");
        let normal = chunks
            .iter()
            .find(|c| c.chunk_level == "paragraph" && c.text.contains("边缘计算"))
            .unwrap();
        assert!(!normal.is_template);
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
