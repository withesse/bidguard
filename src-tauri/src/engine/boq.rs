// 报价清单（BOQ / 工程量清单）识别与跨文档行对齐（W5-1，M6 商务标数值层地基）。
//
// 输入是**已原子化的表格行**：chunk_type='table_row'（xlsx 由 calamine 逐行产出、docx 表格行
// 以 " | " 连接单元格），比对期由 chunk_repo::load_table_rows 固定 paragraph 粒度加载——刻意
// 不走 cfg.chunk_level 与 scope 过滤，技术标比对时数值层仍要能跑。
//
// 本模块是纯函数层（无 DB、无 IO）：表检测 → 表头同义词典识别 → 数据行解析 → 跨文档对齐。
// 后续 W5-2/3/4 的雷同率、算术错误、规律性、相关性全部消费这里的对齐结果。
//
// 覆盖范围声明：扫描件 PDF 走 OCR 路径不产表格行块，本层遇不到表格行就返回空（静默跳过，
// 原因记在 DocExtract.skipped）——产品上数值层仅声明支持 xlsx/docx/文本 PDF 清单。
use crate::engine::features;
use crate::engine::normalize::{self, NormalizeOptions};
use std::collections::{BTreeMap, HashMap, HashSet};

/// 表头同义词典命中的规范列语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColKind {
    Code,
    Name,
    Unit,
    Qty,
    UnitPrice,
    Total,
}

/// 表头同义词典。按「长词优先」排列——`contains` 自上而下首个命中即判，
/// 保证「综合单价」不被「单价」截走、「计量单位」不被「单位」截走。
/// 词典需按真实标书迭代（措施项目表/主材表/暂估价表列名各异），未命中的表静默跳过。
const HEADER_SYNONYMS: &[(&str, ColKind)] = &[
    ("项目编码", ColKind::Code),
    ("清单编码", ColKind::Code),
    ("子目编码", ColKind::Code),
    ("定额编码", ColKind::Code),
    ("项目编号", ColKind::Code),
    ("清单编号", ColKind::Code),
    ("子目编号", ColKind::Code),
    ("定额编号", ColKind::Code),
    ("综合单价", ColKind::UnitPrice),
    ("计量单位", ColKind::Unit),
    ("项目名称", ColKind::Name),
    ("清单名称", ColKind::Name),
    ("子目名称", ColKind::Name),
    ("工程数量", ColKind::Qty),
    ("工程量", ColKind::Qty),
    ("单价", ColKind::UnitPrice),
    ("合价", ColKind::Total),
    ("金额", ColKind::Total),
    ("合计", ColKind::Total),
    ("数量", ColKind::Qty),
    ("单位", ColKind::Unit),
    ("名称", ColKind::Name),
    ("编码", ColKind::Code),
    ("编号", ColKind::Code),
];

/// 判定为清单表头所需的最少规范列数（且必须含单价或合价，见 header_map）。
const MIN_CANONICAL_COLS: usize = 3;
/// 表头在一张表内的最大扫描行数（前几行是表标题/表头/复合表头，再往后就是数据了）。
const HEADER_SCAN_ROWS: usize = 8;
/// 编码列的最小数字位数（GB50500 清单项目码 12 位；≥9 位才可能是清单码）。
/// 防误报：序号列（1/2/3…）被误认编码 —— 要求「≥9 位数字」在有码行里占多数，否则整列作废。
const CODE_MIN_DIGITS: usize = 9;
/// 数据行列数与表头一致的行占比下限；低于此值说明表被拍平错列 → 整表降级不解析。
const COLUMN_MATCH_MIN_RATIO: f64 = 0.5;
/// 名称召回的相似度下限（字符 n-gram Jaccard）。
pub const NAME_MATCH_MIN: f32 = 0.6;
/// 名称召回的规模闸门：任一文档未对齐条目超过此数则跳过名称层（避免 O(n²) 卡住比对）。
const NAME_ALIGN_MAX_ITEMS: usize = 5000;
/// 汇总行关键词：合计/小计等不是清单条目，进了对齐会污染雷同率分母。
const FOOTER_KEYWORDS: &[&str] = &["小计", "合计", "总计", "页计", "本页", "汇总"];

/// 一条待解析的表格行（比对期由 chunk_repo::load_table_rows 映射而来）。
#[derive(Debug, Clone)]
pub struct TableRowInput {
    pub chunk_id: String,
    pub text: String,
    pub page: Option<i64>,
    pub order_index: i64,
}

/// 一条解析出的清单条目。chunk_id 是原文锚点（JOIN 回 chunks 可取原文、下钻 DocPreview 举证）。
#[derive(Debug, Clone, PartialEq)]
pub struct BoqItem {
    pub code: Option<String>,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub qty: Option<f64>,
    pub unit_price: Option<f64>,
    pub total_price: Option<f64>,
    pub chunk_id: String,
    pub page: Option<i64>,
    pub order_index: i64,
    /// 解析期的降级标记（如 code_column_rejected）；无标记为 None。
    pub flags: Option<String>,
}

/// 未识别/降级的表及其原因（静默跳过，但原因要留痕——否则用户看不到「为什么没有数值证据」）。
#[derive(Debug, Clone, PartialEq)]
pub struct TableSkip {
    pub order_start: i64,
    pub rows: usize,
    /// no_header（非清单表或表头变体超出词典）| column_mismatch（合并单元格被拍平错列）|
    /// no_data_rows（有表头无可解析数据行）
    pub reason: &'static str,
}

/// 单文档的抽取结果。
#[derive(Debug, Clone, Default)]
pub struct DocExtract {
    pub items: Vec<BoqItem>,
    /// 成功识别为报价清单的表数。
    pub tables: usize,
    pub skipped: Vec<TableSkip>,
}

// —— 数字与文本解析 ——

/// 单元格 → 数值：复用 normalize 基建（NFKC + 中文数字 + 保留数字内千分位/小数点），
/// 再剥 ¥/千分位/「元」尾缀并折算残留的 万/亿。非纯数值形态（"m3"、"第 1 项"）返回 None。
/// 负号在归一化前单独取出——normalize 的去标点会吃掉 '-'（清单里出现于负数调整项）。
pub fn parse_number(cell: &str) -> Option<f64> {
    let t = cell.trim();
    let neg = t.starts_with(['-', '－', '−']);
    let v = parse_unsigned(t)?;
    Some(if neg { -v } else { v })
}

fn parse_unsigned(cell: &str) -> Option<f64> {
    let norm = normalize::normalize(cell, &NormalizeOptions::default());
    let cleaned: String = norm
        .chars()
        .filter(|c| !matches!(c, '¥' | '￥' | ',' | '，' | '元' | '圆' | '整' | ' ' | '　'))
        .collect();
    let (num, scale) = if let Some(p) = cleaned.strip_suffix('万') {
        (p, 10_000f64)
    } else if let Some(p) = cleaned.strip_suffix('亿') {
        (p, 100_000_000f64)
    } else {
        (cleaned.as_str(), 1f64)
    };
    if num.is_empty() {
        return None;
    }
    let v: f64 = num.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    // 折算后按 1e-4 取整消除 f64 倍率残差（1.005万 → 10049.9999… 应为 10050）；
    // 工程量常见 3 位小数、金额到分，4 位小数足够且保证同输入同输出。
    Some((v * scale * 10_000.0).round() / 10_000.0)
}

/// 表头单元格归一：NFKC + 去空白标点 + 去「元」计量后缀（「单价（元）」→「单价」）。
fn clean_header(cell: &str) -> String {
    let n = normalize::normalize(cell, &NormalizeOptions::default());
    n.replace(['元', '圆', '¥', '￥'], "")
}

/// 表头单元格 → 规范列语义（词典 contains 匹配，长词优先）。
fn classify_header(cell: &str) -> Option<ColKind> {
    let c = clean_header(cell);
    if c.is_empty() {
        return None;
    }
    HEADER_SYNONYMS.iter().find(|(k, _)| c.contains(k)).map(|&(_, kind)| kind)
}

/// 一行单元格是否构成清单表头：命中 ≥3 个规范列，且含单价或合价（无价列的表不是报价清单）。
/// 命中则返回（锁定的列序，命中的规范列种数）——种数用于在单行表头与两行复合表头之间取优。
fn header_map(cells: &[String]) -> Option<(Vec<Option<ColKind>>, usize)> {
    let kinds: Vec<Option<ColKind>> = cells.iter().map(|c| classify_header(c)).collect();
    let distinct: HashSet<ColKind> = kinds.iter().flatten().copied().collect();
    let has_price = distinct.contains(&ColKind::UnitPrice) || distinct.contains(&ColKind::Total);
    if distinct.len() >= MIN_CANONICAL_COLS && has_price {
        Some((kinds, distinct.len()))
    } else {
        None
    }
}

/// 拆列：docx/xlsx 表格行统一以 " | " 连接单元格。按 '|' 拆再逐格 trim（而非按 " | " 整串拆）——
/// chunker 落库前对整行做过 trim，首列为空的行（合并单元格常见）会丢掉前导空格，
/// 按整串拆会少一列并让整表被列数闸门误杀。
fn split_cells(text: &str) -> Vec<String> {
    text.split('|').map(|c| c.trim().to_string()).collect()
}

/// 按 order_index 邻接把表格行分组成表（chunker 里被 min_chars 丢弃的短行不占号，
/// 故连续表格行的 order_index 恒连续；中间夹了标题/正文段落则断开 = 换了一张表）。
fn group_tables(rows: &[TableRowInput]) -> Vec<&[TableRowInput]> {
    let mut out = Vec::new();
    if rows.is_empty() {
        return out;
    }
    let mut start = 0usize;
    for i in 1..rows.len() {
        if rows[i].order_index != rows[i - 1].order_index + 1 {
            out.push(&rows[start..i]);
            start = i;
        }
    }
    out.push(&rows[start..]);
    out
}

/// 两行复合表头合并：列数相同才逐列拼接（「金额」+「综合单价」→「金额综合单价」）。
fn merge_header_rows(a: &[String], b: &[String]) -> Option<Vec<String>> {
    if a.len() != b.len() {
        return None;
    }
    Some(a.iter().zip(b).map(|(x, y)| format!("{x}{y}")).collect())
}

/// 抽取一份文档里的全部清单条目。未识别的表静默跳过，原因记入 skipped。
pub fn extract_document(rows: &[TableRowInput]) -> DocExtract {
    let mut out = DocExtract::default();
    for table in group_tables(rows) {
        match extract_table(table) {
            Ok(mut items) => {
                out.tables += 1;
                out.items.append(&mut items);
            }
            Err(reason) => out.skipped.push(TableSkip {
                order_start: table.first().map(|r| r.order_index).unwrap_or(0),
                rows: table.len(),
                reason,
            }),
        }
    }
    out
}

fn extract_table(table: &[TableRowInput]) -> Result<Vec<BoqItem>, &'static str> {
    let cells: Vec<Vec<String>> = table.iter().map(|r| split_cells(&r.text)).collect();
    // 表头定位：单行表头与「与下一行合并」的复合表头都试，取命中规范列更多的那个
    // （复合表头的首行常是被拍平的合并单元格「金额|金额」，单看首行会把列序锁错）。
    let scan = cells.len().min(HEADER_SCAN_ROWS);
    let mut header: Option<(usize, Vec<Option<ColKind>>)> = None; // (数据起始行, 列序)
    for i in 0..scan {
        let single = header_map(&cells[i]);
        let merged = if i + 1 < cells.len() {
            merge_header_rows(&cells[i], &cells[i + 1]).and_then(|m| header_map(&m))
        } else {
            None
        };
        header = match (single, merged) {
            (Some((sk, sn)), Some((mk, mn))) => {
                Some(if mn > sn { (i + 2, mk) } else { (i + 1, sk) })
            }
            (Some((sk, _)), None) => Some((i + 1, sk)),
            (None, Some((mk, _))) => Some((i + 2, mk)),
            (None, None) => None,
        };
        if header.is_some() {
            break;
        }
    }
    let (data_start, kinds) = header.ok_or("no_header")?;
    let width = kinds.len();
    // 列序锁定：同一语义重复出现以首列为准。
    let mut col: HashMap<ColKind, usize> = HashMap::new();
    for (i, k) in kinds.iter().enumerate() {
        if let Some(k) = k {
            col.entry(*k).or_insert(i);
        }
    }

    // 合并单元格被拍平会让数据行列数与表头不一致：不一致行占多数 → 整表降级不解析。
    let candidates = cells.len().saturating_sub(data_start);
    if candidates == 0 {
        return Err("no_data_rows");
    }
    let matched = cells[data_start..].iter().filter(|c| c.len() == width).count();
    if (matched as f64) < COLUMN_MATCH_MIN_RATIO * candidates as f64 {
        return Err("column_mismatch");
    }

    let pick = |row: &[String], k: ColKind| -> Option<String> {
        col.get(&k).and_then(|&i| row.get(i)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    };
    let mut items: Vec<BoqItem> = Vec::new();
    for (ri, row) in cells.iter().enumerate().skip(data_start) {
        if row.len() != width {
            continue; // 少数错列行跳过（整表已通过占比闸门）
        }
        let name = pick(row, ColKind::Name);
        let code = pick(row, ColKind::Code);
        // 汇总行（小计/合计/本页小计）不是清单条目，进对齐会污染雷同率分母
        if name.as_deref().is_some_and(is_footer_name) {
            continue;
        }
        let qty = pick(row, ColKind::Qty).as_deref().and_then(parse_number);
        let unit_price = pick(row, ColKind::UnitPrice).as_deref().and_then(parse_number);
        let total_price = pick(row, ColKind::Total).as_deref().and_then(parse_number);
        // 表内小标题行（「一、土石方工程」独占一格）与重复表头：无任何数值 → 不是数据行
        if qty.is_none() && unit_price.is_none() && total_price.is_none() {
            continue;
        }
        if name.is_none() && code.is_none() {
            continue;
        }
        items.push(BoqItem {
            code,
            name,
            unit: pick(row, ColKind::Unit),
            qty,
            unit_price,
            total_price,
            chunk_id: table[ri].chunk_id.clone(),
            page: table[ri].page,
            order_index: table[ri].order_index,
            flags: None,
        });
    }
    if items.is_empty() {
        return Err("no_data_rows");
    }

    // 序号列防误报：编码列要求「≥9 位数字」在有码行里占多数，否则整列作废（1/2/3… 不是清单码）。
    let with_code = items.iter().filter(|i| i.code.is_some()).count();
    if with_code > 0 {
        let long_enough = items
            .iter()
            .filter(|i| i.code.as_deref().is_some_and(|c| digits_of(c).len() >= CODE_MIN_DIGITS))
            .count();
        if long_enough * 2 <= with_code {
            for i in items.iter_mut() {
                i.code = None;
                i.flags = Some("code_column_rejected".to_string());
            }
        }
    }
    Ok(items)
}

fn is_footer_name(name: &str) -> bool {
    let n = normalize::normalize(name, &NormalizeOptions::default());
    FOOTER_KEYWORDS.iter().any(|k| n.contains(k))
}

/// 取字符串里的 ASCII 数字（先 NFKC 让全角数字归位）。
fn digits_of(s: &str) -> String {
    let n = normalize::sanitize_with_stats(s).0;
    n.chars().filter(char::is_ascii_digit).collect()
}

// —— 跨文档对齐 ——

/// 一个跨文档对齐组：同一清单条目在各文档中的落点 (doc_index, item_index)。
/// 只有跨 ≥2 份文档的组才成组——单文档独有的条目不是「对齐」，align_key 留空。
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedGroup {
    pub key: String,
    pub members: Vec<(usize, usize)>,
}

/// 对齐结果。keys 与输入同形：每个条目的 align_key（未跨文档对齐 → None）。
#[derive(Debug, Clone, Default)]
pub struct AlignOutcome {
    pub keys: Vec<Vec<Option<String>>>,
    pub groups: Vec<AlignedGroup>,
}

impl AlignOutcome {
    /// 归入某个跨文档对齐组的条目数。
    pub fn aligned_item_count(&self) -> usize {
        self.groups.iter().map(|g| g.members.len()).sum()
    }

    /// 对齐率 = 对齐条目数 / 条目总数。对齐率本身即证据：连非标措施项的拆分方式都一致，
    /// 是「同一单位编制」的结构性信号。total=0 时为 0。
    pub fn align_rate(&self, total: usize) -> f64 {
        if total == 0 {
            0.0
        } else {
            self.aligned_item_count() as f64 / total as f64
        }
    }
}

/// 编码前缀键：取数字位，够 n 位则返回前 n 位（12 位=GB50500 全码，9 位=全国统一项目码）。
fn code_prefix(code: Option<&str>, n: usize) -> Option<String> {
    let d = digits_of(code?);
    if d.len() >= n {
        Some(d[..n].to_string())
    } else {
        None
    }
}

/// 跨文档行对齐：① 编码前 12 位精确 → ② 编码前 9 位精确 → ③ 名称+单位相似度贪心 1:1 召回。
/// 全程确定性（按键排序、按文档/条目次序遍历），同输入逐字节同输出。
pub fn align(docs: &[Vec<BoqItem>]) -> AlignOutcome {
    let mut keys: Vec<Vec<Option<String>>> =
        docs.iter().map(|d| vec![None; d.len()]).collect();
    let mut groups: Vec<AlignedGroup> = Vec::new();

    for &prefix in &[12usize, CODE_MIN_DIGITS] {
        align_by_code(docs, prefix, &mut keys, &mut groups);
    }
    align_by_name(docs, &mut keys, &mut groups);

    AlignOutcome { keys, groups }
}

fn align_by_code(
    docs: &[Vec<BoqItem>],
    prefix: usize,
    keys: &mut [Vec<Option<String>>],
    groups: &mut Vec<AlignedGroup>,
) {
    let mut buckets: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for (di, items) in docs.iter().enumerate() {
        for (ii, it) in items.iter().enumerate() {
            if keys[di][ii].is_some() {
                continue;
            }
            if let Some(p) = code_prefix(it.code.as_deref(), prefix) {
                buckets.entry(p).or_default().push((di, ii));
            }
        }
    }
    for (code, members) in buckets {
        // 同一文档内同码重复（如同一清单项分多次计量）按出现次序 1:1 配对，避免多对多爆炸。
        let mut seen: HashMap<usize, usize> = HashMap::new();
        let mut by_rank: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for (di, ii) in members {
            let rank = seen.entry(di).or_insert(0);
            by_rank.entry(*rank).or_default().push((di, ii));
            *rank += 1;
        }
        for (rank, mem) in by_rank {
            let docs_in: HashSet<usize> = mem.iter().map(|m| m.0).collect();
            if docs_in.len() < 2 {
                continue;
            }
            let key = format!("c{prefix}:{code}#{rank}");
            for &(di, ii) in &mem {
                keys[di][ii] = Some(key.clone());
            }
            groups.push(AlignedGroup { key, members: mem });
        }
    }
}

/// 名称+单位相似度召回：缺编码或编码失配的行（措施项目/主材/暂估价表常无统一码）。
/// 单位必须相等（双方都空亦可）+ 名称字符 n-gram Jaccard ≥ NAME_MATCH_MIN，贪心 1:1。
fn align_by_name(
    docs: &[Vec<BoqItem>],
    keys: &mut [Vec<Option<String>>],
    groups: &mut Vec<AlignedGroup>,
) {
    // 规模闸门：未对齐条目过多时跳过本层（O(n²) 会拖垮比对；编码层已覆盖标准清单）。
    if docs
        .iter()
        .zip(keys.iter())
        .any(|(d, k)| d.len().saturating_sub(k.iter().filter(|x| x.is_some()).count()) > NAME_ALIGN_MAX_ITEMS)
    {
        return;
    }
    // 预计算：(归一单位, 名称 n-gram 集)；名称缺失或过短的条目不参与。
    type NameFp = Option<(String, HashSet<u64>)>;
    let fps: Vec<Vec<NameFp>> = docs
        .iter()
        .map(|items| {
            items
                .iter()
                .map(|it| {
                    let name = it.name.as_deref().map(norm_text).unwrap_or_default();
                    if name.chars().count() < 2 {
                        return None;
                    }
                    let unit = it.unit.as_deref().map(norm_text).unwrap_or_default();
                    Some((unit, features::char_ngrams(&name)))
                })
                .collect()
        })
        .collect();

    let mut used_keys: HashSet<String> = HashSet::new();
    for seed_doc in 0..docs.len() {
        for si in 0..docs[seed_doc].len() {
            if keys[seed_doc][si].is_some() {
                continue;
            }
            let Some((sunit, sgrams)) = fps[seed_doc][si].as_ref() else {
                continue;
            };
            let mut members = vec![(seed_doc, si)];
            for other in (seed_doc + 1)..docs.len() {
                let mut best: Option<(usize, f32)> = None;
                for oi in 0..docs[other].len() {
                    if keys[other][oi].is_some() {
                        continue;
                    }
                    let Some((ounit, ograms)) = fps[other][oi].as_ref() else {
                        continue;
                    };
                    if ounit != sunit {
                        continue;
                    }
                    let j = features::jaccard(sgrams, ograms);
                    if j >= NAME_MATCH_MIN && best.is_none_or(|(_, b)| j > b) {
                        best = Some((oi, j));
                    }
                }
                if let Some((oi, _)) = best {
                    members.push((other, oi));
                }
            }
            if members.len() < 2 {
                continue;
            }
            // 内容派生的稳定键（同输入同输出）；极小概率撞键时加序号后缀区分。
            let base = format!("n:{:016x}", name_key_hash(sunit, docs, members[0]));
            let mut key = base.clone();
            let mut n = 1;
            while !used_keys.insert(key.clone()) {
                key = format!("{base}#{n}");
                n += 1;
            }
            for &(di, ii) in &members {
                keys[di][ii] = Some(key.clone());
            }
            groups.push(AlignedGroup { key, members });
        }
    }
}

/// 名称组的内容键：单位 + 种子条目名称（归一后）→ hash64。只用于生成稳定的 align_key。
fn name_key_hash(unit: &str, docs: &[Vec<BoqItem>], seed: (usize, usize)) -> u64 {
    let name = docs[seed.0][seed.1].name.as_deref().map(norm_text).unwrap_or_default();
    features::hash64(&format!("{unit}|{name}"))
}

/// 名称/单位归一（NFKC + 中文数字 + 去标点空白 + 小写）：让「m³」与「m3」、
/// 「挖一般土方（含运输）」与「挖一般土方 含运输」比得上。
fn norm_text(s: &str) -> String {
    normalize::normalize(s, &NormalizeOptions::default())
}

// —— 文档对统计：逐项雷同率 + 共享算术错误（W5-2，M6）——

/// 可比条目数下限：低于此数不出雷同率结论（只出原因）。
/// 地方雷同认定口径的 80% 线建立在成规模的清单上；分母 3 项时「2 项相同」= 66.7% 毫无意义。
pub const MIN_COMPARABLE: usize = 10;
/// 招标人给定单价的行——各家照抄本就相同，进分母会把雷同率系统性抬高。
/// 暂估价/暂列金额/暂定/信息价/甲供材：名称命中即整条剔除（分母、共享算术错误双双剔除）。
const PROVISIONAL_KEYWORDS: &[&str] = &["暂估", "暂列", "暂定", "信息价", "甲供"];
/// 行内代数校验容差：绝对 1 分 + 相对 0.5%（计价软件的进位/舍位惯例差异都在这个带内）。
const ARITH_ABS_TOL: f64 = 0.01;
const ARITH_REL_TOL: f64 = 0.005;
/// 综合单价的标准展示精度（2 位小数）。工程量大时，单价末位的四舍五入会被放大成
/// 「看似算错」的合价差 —— 半个末位 × 工程量以内的偏差一律视为可由舍入解释，不记错误。
const PRICE_DISPLAY_HALF_ULP: f64 = 0.005;

/// 一条共享算术错误：同一对齐项在两文档中 qty/单价/（算错的）合价三者全等。
/// chunk_ids 是双方原文锚点（JOIN 回 chunks 可取原文行，供人工核对与举证下钻）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedArithError {
    pub align_key: String,
    pub name: Option<String>,
    pub qty: f64,
    pub unit_price: f64,
    pub total: f64,
    /// 正确值（qty×单价），供人工一眼看出错在哪。
    pub expected_total: f64,
    pub chunk_ids: Vec<String>,
}

/// 一个文档对的数值统计。identical_rate 为 None 时 reason 必然给出原因（不出结论也要出原因）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStats {
    pub a: usize,
    pub b: usize,
    /// 可比条目数：双方均有单价、且非暂估价/信息价类的对齐项。
    pub comparable: usize,
    /// 单价按分（×100 四舍五入）相等的条目数。
    pub identical: usize,
    pub identical_rate: Option<f64>,
    /// 是否达到告警线（identical_rate 缺席时恒 false）。
    pub alarm: bool,
    /// identical_rate 缺席原因：insufficient（可比条目不足 MIN_COMPARABLE）。
    pub reason: Option<&'static str>,
    pub shared_arith_errors: Vec<SharedArithError>,
    /// 规律性差异（W5-3）：剔除双方相等项后仍 n≥10 且 R²≥0.999 才出，否则 None。
    pub pattern: Option<PatternFit>,
    /// 单价向量相关性（W5-4）：可比条目 <10 或方差为 0 时 None。
    pub correlation: Option<Correlation>,
    /// 归一化散点（W5-4）：点 =（各自单价 / 全体投标人该项中位价），裁剪 [0,3]、下采样 ≤2000。
    pub scatter: Vec<ScatterPoint>,
}

/// 金额到「分」的整数化（比较用）。工程量另用 1e-4（parse_number 已按 1e-4 取整）。
fn cents(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

fn qty_key(v: f64) -> i64 {
    (v * 10_000.0).round() as i64
}

/// 招标人给定单价行（暂估价/暂列/信息价/甲供材）——任一侧命中即整条不参与统计。
fn is_provisional(item: &BoqItem) -> bool {
    let hay = format!(
        "{}{}",
        item.name.as_deref().unwrap_or(""),
        item.unit.as_deref().unwrap_or("")
    );
    let n = normalize::normalize(&hay, &NormalizeOptions::default());
    PROVISIONAL_KEYWORDS.iter().any(|k| n.contains(k))
}

/// 差值可否由常见舍入规则解释（§1.5 硬约束：先排除舍入，再谈「算错」）。
/// 三类惯例：① 合价按 0/1/2 位小数四舍五入、进位（ceil）、舍位（trunc）；
/// ② 单价末位（2 位小数）四舍五入被工程量放大：|误差| ≤ |qty|×0.005 + 1 分。
fn rounding_explainable(qty: f64, price: f64, total: f64) -> bool {
    let exact = qty * price;
    if !exact.is_finite() {
        return false;
    }
    if (total - exact).abs() <= qty.abs() * PRICE_DISPLAY_HALF_ULP + ARITH_ABS_TOL {
        return true;
    }
    for p in 0..=2i32 {
        let scale = 10f64.powi(p);
        let scaled = exact * scale;
        for cand in [scaled.round(), scaled.ceil(), scaled.floor(), scaled.trunc()] {
            if (total - cand / scale).abs() <= ARITH_ABS_TOL {
                return true;
            }
        }
    }
    false
}

/// 行内代数校验：qty/单价/合价齐备且误差超出容差、且不可由舍入解释 → 记为算术错误。
/// 返回正确值（qty×单价）；非错误行返回 None。
fn arith_error_of(item: &BoqItem) -> Option<f64> {
    let (qty, price, total) = (item.qty?, item.unit_price?, item.total_price?);
    if !(qty.is_finite() && price.is_finite() && total.is_finite()) {
        return None;
    }
    let expected = qty * price;
    let err = total - expected;
    if err.abs() <= ARITH_ABS_TOL.max(ARITH_REL_TOL * total.abs()) {
        return None;
    }
    if rounding_explainable(qty, price, total) {
        return None;
    }
    Some(expected)
}

/// 每个文档对的逐项雷同率与共享算术错误。
/// 可比条目 = 双方均有单价的对齐项（剔除暂估价/信息价类）；相同 = 单价到分相等；
/// 共享算术错误 = 同一对齐项双方都算错、且 qty/单价/错误合价三者全等（「错得一样」）。
/// 输出按 (a,b) 升序、组内按 align_key 升序，同输入逐字节同输出。
///
/// 注意：本函数只出事实与比率，不做定性。alarm 仅表示「达到参照地方雷同认定口径的告警线」。
pub fn pair_stats(docs: &[Vec<BoqItem>], aligned: &AlignOutcome, alarm_line: f64) -> Vec<PairStats> {
    let n = docs.len();
    let mut out: Vec<PairStats> = Vec::new();
    for a in 0..n {
        for b in (a + 1)..n {
            let mut st = PairStats {
                a,
                b,
                comparable: 0,
                identical: 0,
                identical_rate: None,
                alarm: false,
                reason: None,
                shared_arith_errors: Vec::new(),
                pattern: None,
                correlation: None,
                scatter: Vec::new(),
            };
            // groups 由 align 按键有序产出；这里再按 key 排序一次，保证与 groups 内部顺序解耦。
            // 顺序固定 = 后续 OLS/相关系数的求和顺序固定 = 浮点结果逐字节可复现。
            let mut keyed: Vec<(&AlignedGroup, &BoqItem, &BoqItem)> = Vec::new();
            for g in &aligned.groups {
                let ia = g.members.iter().find(|m| m.0 == a).map(|m| &docs[a][m.1]);
                let ib = g.members.iter().find(|m| m.0 == b).map(|m| &docs[b][m.1]);
                if let (Some(x), Some(y)) = (ia, ib) {
                    if is_provisional(x) || is_provisional(y) {
                        continue;
                    }
                    keyed.push((g, x, y));
                }
            }
            keyed.sort_by(|l, r| l.0.key.cmp(&r.0.key));
            let mut vectors: Vec<(f64, f64)> = Vec::with_capacity(keyed.len());
            let mut scatter: Vec<ScatterPoint> = Vec::with_capacity(keyed.len());
            for (group, x, y) in keyed {
                let key = group.key.as_str();
                if let (Some(px), Some(py)) = (x.unit_price, y.unit_price) {
                    st.comparable += 1;
                    if cents(px) == cents(py) {
                        st.identical += 1;
                    }
                    vectors.push((px, py));
                    // 归一基准取「全体投标人该项中位价」而非某一方，图形才不偏向任何一家。
                    if let Some(med) = group_median_price(docs, group) {
                        scatter.push(ScatterPoint {
                            align_key: key.to_string(),
                            name: x.name.clone().or_else(|| y.name.clone()).map(|n| truncate_chars(&n, SCATTER_NAME_MAX_CHARS)),
                            x: round4((px / med).clamp(0.0, SCATTER_CLIP_MAX)),
                            y: round4((py / med).clamp(0.0, SCATTER_CLIP_MAX)),
                        });
                    }
                }
                if let (Some(ex), Some(_ey)) = (arith_error_of(x), arith_error_of(y)) {
                    let same = match (x.qty, y.qty, x.unit_price, y.unit_price, x.total_price, y.total_price) {
                        (Some(qa), Some(qb), Some(pa), Some(pb), Some(ta), Some(tb)) => {
                            qty_key(qa) == qty_key(qb) && cents(pa) == cents(pb) && cents(ta) == cents(tb)
                        }
                        _ => false,
                    };
                    if same {
                        st.shared_arith_errors.push(SharedArithError {
                            align_key: key.to_string(),
                            name: x.name.clone().or_else(|| y.name.clone()),
                            qty: x.qty.unwrap_or_default(),
                            unit_price: x.unit_price.unwrap_or_default(),
                            total: x.total_price.unwrap_or_default(),
                            expected_total: ex,
                            chunk_ids: vec![x.chunk_id.clone(), y.chunk_id.clone()],
                        });
                    }
                }
            }
            if st.comparable >= MIN_COMPARABLE {
                let rate = st.identical as f64 / st.comparable as f64;
                st.alarm = rate >= alarm_line;
                st.identical_rate = Some(rate);
            } else {
                st.reason = Some("insufficient");
            }
            st.pattern = regularity_of(&vectors);
            st.correlation = correlation(&vectors);
            st.scatter = downsample(scatter, SCATTER_MAX_POINTS);
            out.push(st);
        }
    }
    out
}

// —— 规律性差异 + 数字分布（W5-3）与相关性 + 归一化散点（W5-4）——
//
// 三条纪律贯穿本节：
// ① 规律性只是**线索**：对同一控制价/定额库统一下浮是合法且普遍的报价策略，同样呈等比指纹，
//    故 PatternFit 强制携带 PATTERN_NOTE，任何呈现层都丢不掉这句话。
// ② 相关性必须与**比值 CV 同屏**：投标人单价天然同源（同一定额库/信息价）会让 r 普遍 0.9+，
//    只有 r>0.99 且比值 CV≈0 才是强证据 —— 这句话同样随 Correlation 一起下发。
// ③ 浮点确定性：所有求和都按「按 align_key 升序」的固定顺序进行，同输入逐字节同输出。
//
// 另：Benford 首位卡方**已砍**（单价通常只跨 2–3 个数量级，前提太弱；2–5 份文档场景恒噪声），
// 数字检验只保留尾数（分位/角位）均匀性与 0/5 尾占比。

/// 判定规律性所需的最小拟合优度。R²≥0.999 与 n≥10 双门槛压伪规律。
const PATTERN_R2_MIN: f64 = 0.999;
/// 斜率 a 与 1 的等同判据（等差要求 a≡1）。
const PATTERN_A_EPS: f64 = 1e-6;
/// 截距 b 的零判据（元）：1 分以内视作 0（等比/恒定折扣要求 b≡0）。
const PATTERN_B_EPS: f64 = 0.01;
/// 比值向量 CV 的强佐证线（等比：0.5%）。
const RATIO_CV_STRONG: f64 = 0.005;
/// 差值向量极差的强佐证线（等差：1 分）。
const DIFF_RANGE_STRONG: f64 = 0.01;

/// §1.5 强制措辞：规律性差异只定位为线索，不得表述为「认定串通」。
pub const PATTERN_NOTE: &str = "规律性差异属线索而非认定：可能源于对同一控制价/定额库的统一下浮，需结合取证类证据综合判断。";
/// §1.5 强制措辞：相关系数须与比值 CV、散点形态同屏判读。
pub const CORRELATION_NOTE: &str = "投标人单价天然同源（同一定额库/信息价）会使相关系数普遍偏高：只有 r>0.99 且比值 CV≈0 才是强证据，须结合散点形态判读。";
/// §1.5 强制措辞：尾数聚集反映取整习惯，单独不足以定性。
pub const DIGIT_NOTE: &str =
    "尾数聚集反映报价的取整习惯（如统一取整到角/元），单独不构成串通认定，需结合取证类证据。";

/// 规律性差异的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    /// 等差：y = x + b（各项差额恒定）。
    ArithSeq,
    /// 等比 / 恒定折扣：y = a·x（各项系数恒定）。
    GeoDiscount,
    /// 仿射：y = a·x + b（系数与差额都非平凡）。
    Affine,
}

impl PatternKind {
    /// 与 serde snake_case 序列化一致的稳定标识（围标信号 detail / 导出章节共用）。
    pub fn as_str(self) -> &'static str {
        match self {
            PatternKind::ArithSeq => "arith_seq",
            PatternKind::GeoDiscount => "geo_discount",
            PatternKind::Affine => "affine",
        }
    }
}

/// 一对文档单价向量的规律性拟合结果（缺席=未达门槛，不出结论）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternFit {
    pub kind: PatternKind,
    /// 最小二乘斜率（等比时即折扣系数）。
    pub a: f64,
    /// 最小二乘截距（等差时即恒定差额，元）。
    pub b: f64,
    pub r2: f64,
    /// 参与拟合的条目数（已剔除双方单价到分相等的条目）。
    pub n: usize,
    /// 比值向量 y/x 的变异系数；<0.5% 佐证等比。任一 x 为 0 时缺席。
    pub ratio_cv: Option<f64>,
    /// 差值向量 y−x 的极差（元）；<1 分佐证等差。
    pub diff_range: f64,
    /// 辅证是否成立（等比看 ratio_cv、等差看 diff_range、仿射恒 false）。
    pub corroborated: bool,
    /// §1.5 强制文案，随数据下发，呈现层不得省略。
    pub note: &'static str,
}

/// 规律性差异检测：对齐单价向量 (x,y) 的最小二乘拟合 + 形态分类。
///
/// 先**剔除双方到分相等的条目**再判——大面积逐项雷同会把回归钉死在 y=x 上，
/// 掩盖剩余项里的等差/等比指纹；剔除后仍需 n≥10 才出结论。
pub fn regularity_of(pairs: &[(f64, f64)]) -> Option<PatternFit> {
    let kept: Vec<(f64, f64)> = pairs
        .iter()
        .copied()
        .filter(|(x, y)| x.is_finite() && y.is_finite() && cents(*x) != cents(*y))
        .collect();
    let n = kept.len();
    if n < MIN_COMPARABLE {
        return None;
    }
    let nf = n as f64;
    let mx = kept.iter().map(|p| p.0).sum::<f64>() / nf;
    let my = kept.iter().map(|p| p.1).sum::<f64>() / nf;
    let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
    for &(x, y) in &kept {
        let dx = x - mx;
        let dy = y - my;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    // 单价全等（方差为 0）→ 拟合无定义，不是「规律性」。
    if !(sxx > 0.0 && syy > 0.0) {
        return None;
    }
    let a = sxy / sxx;
    let b = my - a * mx;
    let r2 = ((sxy * sxy) / (sxx * syy)).min(1.0);
    if !r2.is_finite() || r2 < PATTERN_R2_MIN {
        return None;
    }
    let is_unit_slope = (a - 1.0).abs() < PATTERN_A_EPS;
    let is_zero_intercept = b.abs() < PATTERN_B_EPS;
    // 恒等（y≡x）不是规律性差异——差异本身已被上面的剔除步骤拿掉。
    if is_unit_slope && is_zero_intercept {
        return None;
    }
    let kind = if is_unit_slope {
        PatternKind::ArithSeq
    } else if is_zero_intercept {
        PatternKind::GeoDiscount
    } else {
        PatternKind::Affine
    };
    let ratio_cv = ratio_cv_of(&kept);
    let diffs: Vec<f64> = kept.iter().map(|(x, y)| y - x).collect();
    let diff_range = diffs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - diffs.iter().copied().fold(f64::INFINITY, f64::min);
    let corroborated = match kind {
        PatternKind::GeoDiscount => ratio_cv.is_some_and(|cv| cv < RATIO_CV_STRONG),
        PatternKind::ArithSeq => diff_range < DIFF_RANGE_STRONG,
        PatternKind::Affine => false,
    };
    Some(PatternFit {
        kind,
        a,
        b,
        r2,
        n,
        ratio_cv,
        diff_range,
        corroborated,
        note: PATTERN_NOTE,
    })
}

/// 比值向量 y/x 的变异系数（总体标准差 / 均值）。任一 x 为 0 或均值为 0 时缺席。
fn ratio_cv_of(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.is_empty() || pairs.iter().any(|(x, _)| *x == 0.0) {
        return None;
    }
    let nf = pairs.len() as f64;
    let ratios: Vec<f64> = pairs.iter().map(|(x, y)| y / x).collect();
    let mean = ratios.iter().sum::<f64>() / nf;
    if mean == 0.0 || !mean.is_finite() {
        return None;
    }
    let var = ratios.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / nf;
    Some(var.sqrt() / mean.abs())
}

// —— 尾数分布（W5-3）——

/// 尾数均匀性 χ² 的临界值：df=9、α=0.001 → 27.877（查表硬编码，不引统计库）。
const CHI2_DF9_P001: f64 = 27.877;
/// 出数字分布结论所需的最小样本量（每格期望频数 ≥2，χ² 近似才站得住）。
pub const DIGIT_MIN_N: usize = 20;
/// 0/5 尾占比的聚集判据（均匀期望 0.2）。
const ZERO_FIVE_RATIO_HIGH: f64 = 0.6;

/// 单文档单价的尾数分布检验（分位=小数第二位，角位=小数第一位）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigitStats {
    pub n: usize,
    /// 分位 0..9 频数。
    pub cent_counts: [usize; 10],
    /// 角位 0..9 频数。
    pub jiao_counts: [usize; 10],
    pub cent_chi_square: f64,
    pub jiao_chi_square: f64,
    /// df=9、α=0.001 的临界值：χ² 超过它才拒绝均匀分布假设。
    pub critical: f64,
    /// 分位为 0 或 5 的占比。
    pub zero_five_ratio: f64,
    /// 尾数聚集命中：任一位 χ² 超临界值，或 0/5 尾占比 ≥0.6。
    pub clustered: bool,
    /// §1.5 强制文案。
    pub note: &'static str,
}

/// 投标人自主报价的单价（剔除暂估价/暂列/信息价/甲供材——招标人给定价的取整习惯不是投标人的）。
pub fn bidder_unit_prices(items: &[BoqItem]) -> Vec<f64> {
    items
        .iter()
        .filter(|it| !is_provisional(it))
        .filter_map(|it| it.unit_price)
        .collect()
}

/// 单价尾数的均匀性检验 + 0/5 尾占比。样本不足 DIGIT_MIN_N 时不出结论。
/// （Benford 首位卡方已砍：单价只跨 2–3 个数量级，前提不成立。）
pub fn digit_stats(prices: &[f64]) -> Option<DigitStats> {
    let cents_list: Vec<i64> = prices
        .iter()
        .filter(|p| p.is_finite() && **p > 0.0)
        .map(|p| cents(*p).abs())
        .collect();
    let n = cents_list.len();
    if n < DIGIT_MIN_N {
        return None;
    }
    let mut cent_counts = [0usize; 10];
    let mut jiao_counts = [0usize; 10];
    for c in &cents_list {
        cent_counts[(c % 10) as usize] += 1;
        jiao_counts[((c / 10) % 10) as usize] += 1;
    }
    let expected = n as f64 / 10.0;
    let chi = |counts: &[usize; 10]| -> f64 {
        counts
            .iter()
            .map(|&o| {
                let d = o as f64 - expected;
                d * d / expected
            })
            .sum()
    };
    let cent_chi_square = chi(&cent_counts);
    let jiao_chi_square = chi(&jiao_counts);
    let zero_five_ratio = (cent_counts[0] + cent_counts[5]) as f64 / n as f64;
    let clustered = cent_chi_square > CHI2_DF9_P001
        || jiao_chi_square > CHI2_DF9_P001
        || zero_five_ratio >= ZERO_FIVE_RATIO_HIGH;
    Some(DigitStats {
        n,
        cent_counts,
        jiao_counts,
        cent_chi_square,
        jiao_chi_square,
        critical: CHI2_DF9_P001,
        zero_five_ratio,
        clustered,
        note: DIGIT_NOTE,
    })
}

// —— 相关性与归一化散点（W5-4）——

/// 一对文档单价向量的相关性。ratio_cv 必须与 pearson 同屏展示（§1.5）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Correlation {
    pub n: usize,
    pub pearson: f64,
    /// Spearman 秩相关（并列取均秩）。
    pub spearman: f64,
    /// 比值向量 y/x 的变异系数：判读强弱证据的第二个刻度。
    pub ratio_cv: Option<f64>,
    /// §1.5 强制文案。
    pub note: &'static str,
}

/// Pearson + Spearman。n<10 或任一侧方差为 0 时不出值。
pub fn correlation(pairs: &[(f64, f64)]) -> Option<Correlation> {
    if pairs.len() < MIN_COMPARABLE || pairs.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return None;
    }
    let pearson = pearson_of(pairs)?;
    let xr = ranks_of(&pairs.iter().map(|p| p.0).collect::<Vec<f64>>());
    let yr = ranks_of(&pairs.iter().map(|p| p.1).collect::<Vec<f64>>());
    let ranked: Vec<(f64, f64)> = xr.into_iter().zip(yr).collect();
    let spearman = pearson_of(&ranked)?;
    Some(Correlation {
        n: pairs.len(),
        pearson,
        spearman,
        ratio_cv: ratio_cv_of(pairs),
        note: CORRELATION_NOTE,
    })
}

/// Pearson 相关系数（固定求和顺序）。方差为 0 时无定义 → None。
fn pearson_of(pairs: &[(f64, f64)]) -> Option<f64> {
    let n = pairs.len();
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let mx = pairs.iter().map(|p| p.0).sum::<f64>() / nf;
    let my = pairs.iter().map(|p| p.1).sum::<f64>() / nf;
    let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
    for &(x, y) in pairs {
        let dx = x - mx;
        let dy = y - my;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if !(sxx > 0.0 && syy > 0.0) {
        return None;
    }
    Some((sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0))
}

/// 秩向量（秩自 1 起算，并列取均秩）。排序按值升序、同值按下标升序 → 全序，结果确定。
fn ranks_of(values: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&i, &j| {
        values[i]
            .partial_cmp(&values[j])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(i.cmp(&j))
    });
    let mut out = vec![0.0f64; values.len()];
    let mut i = 0usize;
    while i < idx.len() {
        let mut j = i + 1;
        while j < idx.len() && values[idx[j]] == values[idx[i]] {
            j += 1;
        }
        // 秩区间 [i+1, j] 的均值 = (i+1+j)/2
        let avg = (i + 1 + j) as f64 / 2.0;
        for &k in &idx[i..j] {
            out[k] = avg;
        }
        i = j;
    }
    out
}

/// 每对散点的下采样上限（前端渲染压力闸门：5 份文档 10 对）。
pub const SCATTER_MAX_POINTS: usize = 2000;
/// 归一化坐标的裁剪上界：价格/中位价 >3 的极端点压到边界，避免拉爆坐标轴。
const SCATTER_CLIP_MAX: f64 = 3.0;
/// 悬停名称的截断字符数（2000 点 × 10 对，全名会把 numeric_json 撑大）。
const SCATTER_NAME_MAX_CHARS: usize = 40;

/// 一个归一化散点：坐标 = 各自单价 / 全体投标人该项中位价，裁剪至 [0,3]。
/// 完全雷同 = 点落在对角线上；恒定折扣 = 平行于对角线的直线带。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScatterPoint {
    pub align_key: String,
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
}

/// 该对齐组在**全体文档**中的单价中位数（偶数个取中间两数均值）。
/// 剔除暂估价/信息价类；无有效正单价时缺席（该项不产生散点）。
fn group_median_price(docs: &[Vec<BoqItem>], group: &AlignedGroup) -> Option<f64> {
    let mut prices: Vec<f64> = group
        .members
        .iter()
        .filter_map(|&(di, ii)| {
            let it = docs.get(di)?.get(ii)?;
            if is_provisional(it) {
                return None;
            }
            it.unit_price.filter(|p| p.is_finite() && *p > 0.0)
        })
        .collect();
    if prices.is_empty() {
        return None;
    }
    prices.sort_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Equal));
    let m = prices.len() / 2;
    let med = if prices.len() % 2 == 1 {
        prices[m]
    } else {
        (prices[m - 1] + prices[m]) / 2.0
    };
    (med > 0.0).then_some(med)
}

/// 等距下采样至 max 点：命中下标 j·len/max（len>max 时严格递增），输出点数恒为 max。
/// 不用随机抽样——可复现是取证的前提。
fn downsample(pts: Vec<ScatterPoint>, max: usize) -> Vec<ScatterPoint> {
    let len = pts.len();
    if len <= max || max == 0 {
        return pts;
    }
    let mut out = Vec::with_capacity(max);
    let mut next = 0usize;
    for (i, p) in pts.into_iter().enumerate() {
        if next < max && i == next * len / max {
            out.push(p);
            next += 1;
        }
    }
    out
}

/// 坐标保留 4 位小数：散点渲染够用，也把 numeric_json 体积压下来。
fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// 按字符（非字节）截断，超长补省略号。
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(texts: &[&str]) -> Vec<TableRowInput> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| TableRowInput {
                chunk_id: format!("c{i}"),
                text: (*t).to_string(),
                page: Some(1),
                order_index: i as i64,
            })
            .collect()
    }

    const STD: &[&str] = &[
        "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
        "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
        "010101003001 | 挖沟槽土方 | m3 | 380.5 | 32.80 | 12480.4",
        "010401003001 | 实心砖墙 | m3 | 96 | 486.20 | 46675.2",
    ];

    #[test]
    fn standard_header_parses_all_six_fields() {
        // 验收 (1)：标准表头「项目编码|项目名称|单位|工程量|综合单价|合价」→ 全部数据行解析 6 字段
        let ex = extract_document(&rows(STD));
        assert_eq!(ex.tables, 1);
        assert!(ex.skipped.is_empty(), "标准清单表不应被跳过：{:?}", ex.skipped);
        assert_eq!(ex.items.len(), 3);
        let first = &ex.items[0];
        assert_eq!(first.code.as_deref(), Some("010101001001"));
        assert_eq!(first.name.as_deref(), Some("挖一般土方"));
        assert_eq!(first.unit.as_deref(), Some("m3"));
        assert_eq!(first.qty, Some(1200.0));
        assert_eq!(first.unit_price, Some(25.5));
        assert_eq!(first.total_price, Some(30600.0));
        assert_eq!(first.chunk_id, "c1", "chunk_id 应锚回原表格行");
        assert!(ex.items.iter().all(|i| i.flags.is_none()));
    }

    #[test]
    fn header_synonyms_and_units_are_recognized() {
        // 验收 (2)：表头同义变体（清单编码/金额/计量单位/数量/单价（元））同样识别；
        // 金额带 ¥ 与千分位、工程量带中文数字也要解析出来。
        let ex = extract_document(&rows(&[
            "清单编码 | 名称 | 计量单位 | 数量 | 单价（元） | 金额（元）",
            "010302001001 | 实心砖柱 | m³ | 120 | ¥486.20 | ¥58,344.00",
            "010401001001 | 砖基础 | m3 | 64.5 | 398.00 | 25,671.00",
        ]));
        assert_eq!(ex.items.len(), 2, "同义表头应识别：{:?}", ex.skipped);
        assert_eq!(ex.items[0].qty, Some(120.0));
        assert_eq!(ex.items[0].unit_price, Some(486.2));
        assert_eq!(ex.items[0].total_price, Some(58344.0), "¥ 与千分位应剥离");
        assert_eq!(ex.items[1].total_price, Some(25671.0));
    }

    #[test]
    fn composite_two_row_header_merges() {
        // 两行复合表头：第二行才出现「综合单价/合价」，需与第一行逐列合并后识别。
        let ex = extract_document(&rows(&[
            "项目编码 | 项目名称 | 计量单位 | 工程量 | 金额 | 金额",
            "项目编码 | 项目名称 | 计量单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            "010101003001 | 挖沟槽土方 | m3 | 380 | 32.80 | 12464",
        ]));
        assert_eq!(ex.items.len(), 2, "复合表头下的两行数据都应解析：{:?}", ex.skipped);
        assert_eq!(ex.items[0].unit_price, Some(25.5));
    }

    #[test]
    fn non_boq_table_yields_nothing() {
        // 验收 (4)：非清单表（无单价/合价列）不产出条目，且原因留痕
        let ex = extract_document(&rows(&[
            "序号 | 人员姓名 | 拟任岗位 | 职称 | 从业年限",
            "1 | 张某某 | 项目经理 | 高级工程师 | 15",
            "2 | 李某某 | 技术负责人 | 工程师 | 10",
        ]));
        assert!(ex.items.is_empty());
        assert_eq!(ex.tables, 0);
        assert_eq!(ex.skipped.len(), 1);
        assert_eq!(ex.skipped[0].reason, "no_header");
    }

    #[test]
    fn serial_number_column_is_not_taken_as_code() {
        // 防误报之一：「序号」不在编码同义词典里，整列不认（列语义为空）
        let plain = extract_document(&rows(&[
            "序号 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "1 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            "2 | 实心砖墙 | m3 | 96 | 486.20 | 46675.2",
        ]));
        assert_eq!(plain.items.len(), 2);
        assert!(plain.items.iter().all(|i| i.code.is_none()), "序号不得当作清单编码");
        assert!(plain.items.iter().all(|i| i.flags.is_none()));

        // 防误报之二：表头写作「编号」（词典命中 Code）但值是 1/2/3 —— ≥9 位数字未占多数 → 整列作废
        let numbered = extract_document(&rows(&[
            "编号 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "1 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            "2 | 实心砖墙 | m3 | 96 | 486.20 | 46675.2",
        ]));
        assert_eq!(numbered.items.len(), 2);
        assert!(numbered.items.iter().all(|i| i.code.is_none()), "序号值不得当作清单编码");
        assert!(numbered
            .items
            .iter()
            .all(|i| i.flags.as_deref() == Some("code_column_rejected")));
    }

    #[test]
    fn flattened_merged_cells_downgrade_whole_table() {
        // 合并单元格被拍平 → 数据行列数与表头不一致占多数 → 整表降级不解析（宁缺毋错）
        let ex = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50",
            "010101003001 | 挖沟槽土方 | m3 | 380",
            "010401003001 | 实心砖墙 | m3",
        ]));
        assert!(ex.items.is_empty());
        assert_eq!(ex.skipped.len(), 1);
        assert_eq!(ex.skipped[0].reason, "column_mismatch");
    }

    #[test]
    fn leading_empty_cell_row_keeps_its_columns() {
        // 首列为空（合并单元格/措施项无统一编码）：chunker 落库前 trim 掉了前导空格，
        // 列数仍须与表头一致，否则整表会被列数闸门误杀。
        let ex = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            "| 夜间施工增加费用 | 项 | 1 | 8600.00 | 8600.00",
        ]));
        assert_eq!(ex.items.len(), 2, "首列为空的行也应解析：{:?}", ex.skipped);
        assert_eq!(ex.items[1].code, None);
        assert_eq!(ex.items[1].name.as_deref(), Some("夜间施工增加费用"));
        assert_eq!(ex.items[1].unit.as_deref(), Some("项"));
        assert_eq!(ex.items[1].unit_price, Some(8600.0));
    }

    #[test]
    fn footer_rows_are_excluded() {
        let ex = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            " |  本页小计 |  |  |  | 30600",
        ]));
        assert_eq!(ex.items.len(), 1, "小计行不进条目");
    }

    #[test]
    fn adjacent_tables_split_by_order_gap() {
        // order_index 断开 = 中间夹了标题/正文 → 两张独立的表，各自找表头
        let mut rs = rows(STD);
        let mut second = rows(&[
            "清单编码 | 名称 | 单位 | 数量 | 单价 | 金额",
            "030411001001 | 配管 | m | 500 | 12.60 | 6300",
        ]);
        for (i, r) in second.iter_mut().enumerate() {
            r.order_index = 20 + i as i64;
            r.chunk_id = format!("d{i}");
        }
        rs.extend(second);
        let ex = extract_document(&rs);
        assert_eq!(ex.tables, 2);
        assert_eq!(ex.items.len(), 4);
        assert_eq!(ex.items[3].chunk_id, "d1");
    }

    #[test]
    fn scanned_pdf_without_table_rows_is_silently_skipped() {
        // 扫描件 PDF 走 OCR 不产 table_row：输入为空 → 无条目、无跳过记录、不报错
        let ex = extract_document(&[]);
        assert!(ex.items.is_empty() && ex.skipped.is_empty() && ex.tables == 0);
    }

    #[test]
    fn three_docs_align_by_code_and_name() {
        // 验收 (3)：三文档同编码 → 对齐条目数=行数；缺编码行凭名称+单位对齐
        let doc = |code_ok: bool, price: &str| {
            let mut t = vec![
                "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价".to_string(),
                format!("010101001001 | 挖一般土方 | m3 | 1200 | {price} | 30600"),
                format!("010401003001 | 实心砖墙 | m3 | 96 | {price} | 46675.2"),
            ];
            // 措施项目行无统一编码（各家拆分方式一致 → 只能靠名称+单位召回）
            t.push(if code_ok {
                "010101003001 | 夜间施工增加费 | 项 | 1 | 8600 | 8600".to_string()
            } else {
                " | 夜间施工增加费 | 项 | 1 | 8600 | 8600".to_string()
            });
            let texts: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
            extract_document(&rows(&texts)).items
        };
        let docs = vec![doc(false, "25.50"), doc(false, "25.50"), doc(false, "26.00")];
        assert!(docs.iter().all(|d| d.len() == 3));
        let out = align(&docs);
        assert_eq!(out.groups.len(), 3, "三条清单项各成一组：{:?}", out.groups);
        assert!(out.groups.iter().all(|g| g.members.len() == 3), "每组应含三份文档");
        assert_eq!(out.aligned_item_count(), 9);
        assert!((out.align_rate(9) - 1.0).abs() < 1e-9);
        // 无码行是靠名称+单位召回的
        let name_group = out.groups.iter().find(|g| g.key.starts_with("n:")).expect("名称组");
        assert_eq!(name_group.members.len(), 3);
        // 每个条目都拿到 align_key
        assert!(out.keys.iter().all(|d| d.iter().all(|k| k.is_some())));
    }

    #[test]
    fn code_prefix_nine_digits_matches_across_docs() {
        // 12 位末三位（顺序码）不同但前 9 位（全国统一项目码）相同 → 第二层命中
        let a = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
        ]))
        .items;
        let b = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001007 | 场地平整土方 | m3 | 1200 | 26.10 | 31320",
        ]))
        .items;
        let out = align(&[a, b]);
        assert_eq!(out.groups.len(), 1);
        assert!(out.groups[0].key.starts_with("c9:"), "应由 9 位前缀命中：{}", out.groups[0].key);
    }

    #[test]
    fn unmatched_items_get_no_align_key() {
        let a = extract_document(&rows(STD)).items;
        let b = extract_document(&rows(&[
            "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价",
            "010101001001 | 挖一般土方 | m3 | 1200 | 25.50 | 30600",
            "030901002001 | 消火栓钢管 | m | 240 | 88.00 | 21120",
        ]))
        .items;
        let out = align(&[a, b]);
        assert_eq!(out.groups.len(), 1, "只有挖一般土方跨文档共有");
        assert_eq!(out.aligned_item_count(), 2);
        assert!((out.align_rate(5) - 0.4).abs() < 1e-9);
        assert_eq!(out.keys[0].iter().filter(|k| k.is_some()).count(), 1);
        assert_eq!(out.keys[1].iter().filter(|k| k.is_some()).count(), 1);
    }

    #[test]
    fn align_is_deterministic() {
        let a = extract_document(&rows(STD)).items;
        let b = extract_document(&rows(STD)).items;
        let c = extract_document(&rows(STD)).items;
        let one = align(&[a.clone(), b.clone(), c.clone()]);
        let two = align(&[a, b, c]);
        assert_eq!(one.keys, two.keys);
        assert_eq!(one.groups, two.groups);
    }

    #[test]
    fn number_parsing_rejects_non_numeric() {
        assert_eq!(parse_number("1,234.56"), Some(1234.56));
        assert_eq!(parse_number("¥ 1,000,000.00"), Some(1000000.0));
        assert_eq!(parse_number("12.5万元"), Some(125000.0));
        assert_eq!(parse_number("壹佰万元整"), Some(1000000.0));
        assert_eq!(parse_number("-500"), Some(-500.0));
        assert_eq!(parse_number("m3"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("详见附表"), None);
    }

    // —— W5-2：逐项雷同率 + 共享算术错误 ——

    const ALARM: f64 = 0.80;

    /// 造 n 行标准清单：第 i 行编码 0101010010{i:02}、单价取 prices[i]（合价按 qty×单价 算准）。
    fn boq_rows(prices: &[f64]) -> Vec<BoqItem> {
        let mut lines = vec!["项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价".to_string()];
        for (i, p) in prices.iter().enumerate() {
            let qty = 100.0 + i as f64;
            lines.push(format!(
                "0101010010{:02} | 分项工程{i} | m3 | {qty} | {p:.2} | {:.2}",
                i,
                qty * p
            ));
        }
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        extract_document(&rows(&refs)).items
    }

    fn stats_of(a: &[BoqItem], b: &[BoqItem]) -> PairStats {
        let docs = vec![a.to_vec(), b.to_vec()];
        let aligned = align(&docs);
        pair_stats(&docs, &aligned, ALARM).remove(0)
    }

    #[test]
    fn identical_rate_hits_alarm_line_at_eight_of_ten() {
        // 验收 (1)：10 项中 8 项单价相同 → identicalRate=0.8 且 alarm=true（默认告警线 0.80）
        let base: Vec<f64> = (0..10).map(|i| 20.0 + i as f64).collect();
        let mut other = base.clone();
        other[3] += 5.0;
        other[7] += 5.0;
        let st = stats_of(&boq_rows(&base), &boq_rows(&other));
        assert_eq!(st.comparable, 10);
        assert_eq!(st.identical, 8);
        assert_eq!(st.identical_rate, Some(0.8));
        assert!(st.alarm);
        assert_eq!(st.reason, None);
        assert!(st.shared_arith_errors.is_empty(), "算准的行不该报算术错误");
    }

    #[test]
    fn provisional_rows_are_excluded_from_denominator() {
        // 验收 (2)：暂估价/信息价行不进分母——招标人给定单价，各家照抄本就相同
        let mut lines = vec!["项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价".to_string()];
        for i in 0..10 {
            lines.push(format!("0101010010{i:02} | 分项工程{i} | m3 | 100 | 20.00 | 2000.00"));
        }
        lines.push("010101001099 | 电梯设备（暂估价） | 台 | 2 | 300000.00 | 600000.00".into());
        lines.push("010101001098 | 主材按信息价计列 | t | 5 | 4200.00 | 21000.00".into());
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let doc = extract_document(&rows(&refs)).items;
        assert_eq!(doc.len(), 12, "解析层照收暂估价行，剔除发生在统计层");
        let st = stats_of(&doc, &doc);
        assert_eq!(st.comparable, 10, "两条暂估价/信息价行不进分母");
        assert_eq!(st.identical, 10);
        assert_eq!(st.identical_rate, Some(1.0));
    }

    #[test]
    fn shared_arithmetic_error_matches_only_when_wrong_the_same_way() {
        // 验收 (3)：qty=100、单价 25.50、两文档合价均错为 2505 → 命中且携带双方 chunk_id；
        // 错得不同（2505 vs 2510）→ 不命中。
        let head = "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价";
        let mk = |total: &str| {
            let l = [
                head.to_string(),
                format!("010101001001 | 挖一般土方 | m3 | 100 | 25.50 | {total}"),
            ];
            let refs: Vec<&str> = l.iter().map(|s| s.as_str()).collect();
            extract_document(&rows(&refs)).items
        };
        let a = mk("2505.00");
        let b = mk("2505.00");
        let st = stats_of(&a, &b);
        assert_eq!(st.shared_arith_errors.len(), 1);
        let e = &st.shared_arith_errors[0];
        assert_eq!(e.qty, 100.0);
        assert_eq!(e.unit_price, 25.5);
        assert_eq!(e.total, 2505.0);
        assert_eq!(e.expected_total, 2550.0);
        assert_eq!(e.chunk_ids, vec!["c1".to_string(), "c1".to_string()], "双方 chunk_id 都要带");
        assert_eq!(st.reason, Some("insufficient"), "1 项可比 → 不出 rate");

        let st2 = stats_of(&mk("2505.00"), &mk("2510.00"));
        assert!(st2.shared_arith_errors.is_empty(), "错得不同不算共享算术错误");
    }

    #[test]
    fn nine_comparable_items_yield_reason_instead_of_rate() {
        // 验收 (4)：可比数 9（< 10）→ 不出 rate、原因 = insufficient
        let base: Vec<f64> = (0..9).map(|i| 20.0 + i as f64).collect();
        let st = stats_of(&boq_rows(&base), &boq_rows(&base));
        assert_eq!(st.comparable, 9);
        assert_eq!(st.identical, 9);
        assert_eq!(st.identical_rate, None);
        assert!(!st.alarm);
        assert_eq!(st.reason, Some("insufficient"));
    }

    #[test]
    fn rounding_explainable_differences_are_not_arithmetic_errors() {
        // 验收 (5) 负例：可由常见舍入规则解释的差值不算算术错误（§1.5 硬约束）。
        // ① 合价按元进位/舍位：0.5×3.33=1.665 → 记 2（进位到元）
        // ② 单价末位四舍五入被大工程量放大：qty=10000、单价 0.01（实为 0.0136）→ 合价 136
        let head = "项目编码 | 项目名称 | 单位 | 工程量 | 综合单价 | 合价";
        let mk = |line: &str| {
            let l = [head.to_string(), line.to_string()];
            let refs: Vec<&str> = l.iter().map(|s| s.as_str()).collect();
            extract_document(&rows(&refs)).items
        };
        for line in [
            "010101001001 | 零星项目 | 项 | 0.5 | 3.33 | 2.00",
            "010101001001 | 零星项目 | 项 | 0.5 | 3.33 | 1.00",
            "010101001002 | 大宗材料 | kg | 10000 | 0.01 | 136.00",
            "010101001003 | 常规条目 | m3 | 100 | 25.50 | 2551.00",
        ] {
            let items = mk(line);
            assert_eq!(arith_error_of(&items[0]), None, "应视为舍入可解释：{line}");
        }
        // 对照：同样的大宗材料行，差到 300 元 —— 单价舍入解释不了，仍算错
        let bad = mk("010101001002 | 大宗材料 | kg | 10000 | 0.01 | 300.00");
        assert!(arith_error_of(&bad[0]).is_some(), "远超舍入带的差值仍是算术错误");
    }

    // —— W5-3/W5-4：规律性差异 + 尾数分布 + 相关性 + 归一化散点 ——

    /// 确定性伪随机（LCG）：取证功能禁用真随机——同输入必须同输出。
    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((*seed >> 33) as f64) / ((1u64 << 31) as f64)
    }

    /// 直接造条目（绕开解析层）：编码 12 位唯一，跨文档凭编码对齐。
    fn item(i: usize, price: f64) -> BoqItem {
        BoqItem {
            code: Some(format!("{:012}", 10_101_000_000u64 + i as u64)),
            name: Some(format!("分项工程{i}")),
            unit: Some("m3".to_string()),
            qty: Some(100.0),
            unit_price: Some(price),
            total_price: Some(100.0 * price),
            chunk_id: format!("c{i}"),
            page: Some(1),
            order_index: i as i64,
            flags: None,
        }
    }

    fn doc_of(prices: &[f64]) -> Vec<BoqItem> {
        prices.iter().enumerate().map(|(i, p)| item(i, *p)).collect()
    }

    fn base_prices(n: usize) -> Vec<f64> {
        (0..n).map(|i| 100.0 + i as f64 * 7.5).collect()
    }

    fn vectors_of(x: &[f64], y: &[f64]) -> Vec<(f64, f64)> {
        x.iter().copied().zip(y.iter().copied()).collect()
    }

    #[test]
    fn geometric_discount_pattern_is_detected() {
        // 验收 (1)：y=0.97x（噪声 <1e-9）→ kind=geo_discount、a≈0.97，比值 CV 佐证成立
        let x = base_prices(12);
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| 0.97 * v + ((i % 3) as f64 - 1.0) * 1e-10)
            .collect();
        let fit = regularity_of(&vectors_of(&x, &y)).expect("等比规律应命中");
        assert_eq!(fit.kind, PatternKind::GeoDiscount);
        assert!((fit.a - 0.97).abs() < 1e-9, "a={}", fit.a);
        assert!(fit.b.abs() < PATTERN_B_EPS);
        assert!(fit.r2 >= PATTERN_R2_MIN);
        assert_eq!(fit.n, 12);
        assert!(fit.ratio_cv.is_some_and(|cv| cv < RATIO_CV_STRONG));
        assert!(fit.corroborated);
        assert_eq!(fit.note, PATTERN_NOTE, "§1.5 线索定位文案必须随数据下发");
    }

    #[test]
    fn arithmetic_sequence_pattern_is_detected() {
        // 验收 (2)：y=x+500 → arith_seq、b≈500，差值极差佐证成立
        let x = base_prices(12);
        let y: Vec<f64> = x.iter().map(|v| v + 500.0).collect();
        let fit = regularity_of(&vectors_of(&x, &y)).expect("等差规律应命中");
        assert_eq!(fit.kind, PatternKind::ArithSeq);
        assert!((fit.a - 1.0).abs() < PATTERN_A_EPS);
        assert!((fit.b - 500.0).abs() < 1e-6, "b={}", fit.b);
        assert!(fit.diff_range < DIFF_RANGE_STRONG);
        assert!(fit.corroborated);
    }

    #[test]
    fn random_perturbation_yields_no_pattern() {
        // 验收 (3)：随机扰动 5% → R² 达不到 0.999 → 不出 pattern
        let x = base_prices(30);
        let mut seed = 42u64;
        let y: Vec<f64> = x.iter().map(|v| v * (0.95 + 0.1 * lcg(&mut seed))).collect();
        assert!(regularity_of(&vectors_of(&x, &y)).is_none());
    }

    #[test]
    fn pattern_requires_ten_items_after_dropping_equal_ones() {
        // 验收 (5)：大面积雷同不得掩盖规律性——剔除相等项后 n=5 <10 → 不判
        let x = base_prices(20);
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| if i < 15 { *v } else { 0.97 * v })
            .collect();
        assert!(regularity_of(&vectors_of(&x, &y)).is_none(), "剔除相等项后不足 10 项不出结论");
        // 恒等向量同样不出结论（差异本身为零）
        assert!(regularity_of(&vectors_of(&x, &x)).is_none());
    }

    #[test]
    fn digit_stats_flags_zero_tail_clustering_and_spares_uniform_tails() {
        // 验收 (4)：全 0 尾数 40 项 → 尾数聚集命中；尾数均匀 → 不命中
        let zeros: Vec<f64> = (0..40).map(|i| 100.0 + i as f64).collect();
        let ds = digit_stats(&zeros).expect("40 项足够出结论");
        assert_eq!(ds.n, 40);
        assert_eq!(ds.cent_counts[0], 40);
        assert!(ds.clustered);
        assert!((ds.zero_five_ratio - 1.0).abs() < 1e-12);
        assert!(ds.cent_chi_square > ds.critical);
        assert_eq!(ds.note, DIGIT_NOTE);

        // 均匀铺满 00..99 两位尾数（37 与 100 互质 → 100 个余数各命中一次）
        let uniform: Vec<f64> = (0..100).map(|i| 100.0 + ((i * 37) % 100) as f64 / 100.0).collect();
        let du = digit_stats(&uniform).expect("100 项");
        assert!(!du.clustered, "均匀尾数不应命中：{du:?}");
        assert!(du.cent_chi_square < du.critical && du.jiao_chi_square < du.critical);

        // 伪随机尾数同样不应命中
        let mut seed = 7u64;
        let random: Vec<f64> = (0..200)
            .map(|i| 100.0 + i as f64 + (lcg(&mut seed) * 100.0).floor() / 100.0)
            .collect();
        assert!(!digit_stats(&random).expect("200 项").clustered);

        // 样本不足 → 不出结论
        assert!(digit_stats(&zeros[..DIGIT_MIN_N - 1]).is_none());
    }

    #[test]
    fn pearson_and_spearman_match_hand_computed_reference() {
        // 验收：与手算参考值对拍（含并列秩），误差 <1e-9
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.0, 4.0, 5.0, 4.0, 5.0];
        let pairs = vectors_of(&x, &y);
        // Sxy=6, Sxx=10, Syy=6 → r = 6/sqrt(60)
        let expect_p = 6.0 / 60f64.sqrt();
        assert!((pearson_of(&pairs).unwrap() - expect_p).abs() < 1e-9);
        // y 的均秩：2→1、4/4→2.5、5/5→4.5
        assert_eq!(ranks_of(&y), vec![1.0, 2.5, 4.5, 2.5, 4.5]);
        let ranked = vectors_of(&ranks_of(&x), &ranks_of(&y));
        // Sxy=7, Sxx=10, Syy=9 → rs = 7/sqrt(90)
        let expect_s = 7.0 / 90f64.sqrt();
        assert!((pearson_of(&ranked).unwrap() - expect_s).abs() < 1e-9);

        // 公开入口：n<10 不出值；n≥10 出值且带 §1.5 文案与比值 CV
        assert!(correlation(&pairs).is_none(), "n<10 不出相关性");
        let bx = base_prices(12);
        let by: Vec<f64> = bx.iter().map(|v| 0.97 * v).collect();
        let c = correlation(&vectors_of(&bx, &by)).expect("n=12 应出值");
        assert_eq!(c.n, 12);
        assert!((c.pearson - 1.0).abs() < 1e-12 && (c.spearman - 1.0).abs() < 1e-12);
        assert!(c.ratio_cv.is_some_and(|cv| cv < 1e-12));
        assert_eq!(c.note, CORRELATION_NOTE);
    }

    #[test]
    fn scatter_points_are_median_normalized_clipped_and_downsampled() {
        // 验收：点数 = min(可比数, 2000)、坐标 ∈ [0,3]
        let n = 2500;
        let base = base_prices(n);
        let a = doc_of(&base);
        // 甲乙同价 → 点全落对角线 (1,1)；末项乙报 10 倍中位价 → y 被裁剪到 3
        let mut other = base.clone();
        other[n - 1] = base[n - 1] * 10.0;
        let b = doc_of(&other);
        let docs = vec![a, b];
        let aligned = align(&docs);
        let st = pair_stats(&docs, &aligned, ALARM).remove(0);
        assert_eq!(st.comparable, n);
        assert_eq!(st.scatter.len(), SCATTER_MAX_POINTS, "超过上限须下采样至恰好 2000 点");
        assert!(st
            .scatter
            .iter()
            .all(|p| (0.0..=3.0).contains(&p.x) && (0.0..=3.0).contains(&p.y)));
        assert!(st.scatter.iter().all(|p| !p.align_key.is_empty() && p.name.is_some()));

        // 小样本：点数 = 可比数，且同价项恰好落在对角线上
        let small = base_prices(12);
        let docs2 = vec![doc_of(&small), doc_of(&small)];
        let al2 = align(&docs2);
        let st2 = pair_stats(&docs2, &al2, ALARM).remove(0);
        assert_eq!(st2.scatter.len(), 12);
        assert!(st2.scatter.iter().all(|p| (p.x - 1.0).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9));
    }

    #[test]
    fn numeric_layer_output_is_byte_identical_across_runs() {
        // 验收：连跑两次输出逐字节一致（浮点求和顺序固定）
        let x = base_prices(40);
        let y: Vec<f64> = x.iter().map(|v| 0.97 * v).collect();
        let docs = vec![doc_of(&x), doc_of(&y), doc_of(&x)];
        let aligned = align(&docs);
        let one = serde_json::to_string(&pair_stats(&docs, &aligned, ALARM)).unwrap();
        let two = serde_json::to_string(&pair_stats(&docs, &aligned, ALARM)).unwrap();
        assert_eq!(one, two);
        assert!(one.contains("\"geo_discount\""), "等比形态应出现在序列化结果里");
        assert!(one.contains("需结合取证类证据"), "§1.5 文案必须随 JSON 下发");
    }

    #[test]
    fn pair_stats_covers_all_pairs_and_is_deterministic() {
        let base: Vec<f64> = (0..12).map(|i| 20.0 + i as f64).collect();
        let mut other = base.clone();
        other[0] += 3.0;
        let docs = vec![boq_rows(&base), boq_rows(&base), boq_rows(&other)];
        let aligned = align(&docs);
        let one = pair_stats(&docs, &aligned, ALARM);
        let two = pair_stats(&docs, &aligned, ALARM);
        assert_eq!(one, two, "同输入同输出");
        assert_eq!(one.len(), 3, "3 份文档 → 3 个文档对");
        assert_eq!((one[0].a, one[0].b), (0, 1));
        assert_eq!(one[0].identical_rate, Some(1.0));
        assert!(one[0].alarm);
        assert_eq!(one[2].identical, 11, "甲丙仅 1 项不同");
    }
}
