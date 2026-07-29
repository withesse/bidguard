// Excel 报告 v2：总览 / 相似度矩阵 / 条款明细 / 事实冲突 / 逐对明细，
// 加各类证据一表——对齐区段与逐字证据 / 数值证据 / 取证证据 / 规避特征 / 复核路由 / 检查方法与局限。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, calibration_note, contrib_label, digit_clustered_cn, digit_stat,
    evasion_verdict_cn, field_cn, forensic_kind_cn, forensic_level_cn, forensic_pair_label, label,
    level_cn, lineage_summary, mechanism_group_cells, mechanism_lines, mechanism_price_cells,
    mechanism_support_cells, numeric_intro, numeric_item_label, numeric_pattern_cn,
    numeric_rate_cell, review_cn, section_cn, severity_cn, strength_phrase, type_cn,
    verbatim_locator, ARITH_ERROR_NOTE, DIGIT_TAIL_NOTE, EVASION_NOTE, FORENSIC_NOTE,
    MECHANISM_GROUP_HEADER, MECHANISM_GROUP_TITLE, MECHANISM_PRICE_HEADER, MECHANISM_PRICE_TITLE,
    MECHANISM_SUPPORT_HEADER, MECHANISM_SUPPORT_TITLE, MECHANISM_TITLE, SEGMENTS_NOTE,
};
use rust_xlsxwriter::{Format, Workbook, Worksheet};

type R = Result<(), String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// 写一行表头（返回下一行行号）。
fn header_row(s: &mut Worksheet, r: u32, cols: &[&str], head: &Format) -> Result<u32, String> {
    for (c, h) in cols.iter().enumerate() {
        s.write_string_with_format(r, c as u16, *h, head).map_err(err)?;
    }
    Ok(r + 1)
}

/// 写一行文本单元格（返回下一行行号）。
fn text_row(s: &mut Worksheet, r: u32, cells: &[String]) -> Result<u32, String> {
    for (c, v) in cells.iter().enumerate() {
        s.write_string(r, c as u16, v).map_err(err)?;
    }
    Ok(r + 1)
}

/// 区块小标题（同一工作表内分块，返回下一行行号）。
fn block_title(s: &mut Worksheet, r: u32, title: &str, bold: &Format) -> Result<u32, String> {
    s.write_string_with_format(r, 0, title, bold).map_err(err)?;
    Ok(r + 1)
}

/// §1.5 强制说明行（随区块出现，不得省略）。
fn note_row(s: &mut Worksheet, r: u32, text: &str) -> Result<u32, String> {
    s.write_string(r, 0, text).map_err(err)?;
    Ok(r + 1)
}

/// 招标豁免标注（落在招标豁免块者系合法应答，须显式标注而不是悄悄剔除）。
fn tender_badge(quote: bool) -> &'static str {
    if quote {
        "引用招标文件"
    } else {
        "—"
    }
}

pub fn write(data: &ExportData, path: &str) -> R {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_background_color(0xEEEFF9);
    let pctf = Format::new().set_num_format("0%");

    // ── 总览 ──
    {
        let s = wb.add_worksheet();
        s.set_name("总览").map_err(err)?;
        let mut r = 0u32;
        s.write_string_with_format(r, 0, "原本 · 标书查重报告", &bold).map_err(err)?;
        r += 1;
        s.write_string(r, 0, format!(
            "任务：{} · 生成于 {} · 引擎 v{}",
            data.job_name.as_deref().unwrap_or("未命名比对"),
            data.generated_at[..16].replace('T', " "),
            data.app_version
        )).map_err(err)?;
        r += 2;
        s.write_string_with_format(r, 0, "综合判定", &head).map_err(err)?;
        s.write_string(r, 1, format!(
            "{}（证据强度：{}）",
            level_cn(&data.collusion.level),
            strength_phrase(&data.collusion.level)
        )).map_err(err)?;
        r += 1;
        for sig in &data.collusion.signals {
            s.write_string(r, 1, format!("· {}（{}）", sig.detail, contrib_label(sig.weight)))
                .map_err(err)?;
            r += 1;
        }
        s.write_string(r, 1, calibration_note(
            &data.collusion.calibration_kind,
            &data.collusion.calibration_version,
            data.app_version,
        )).map_err(err)?;
        r += 2;
        // 复核路由三带（W6-4）：恒常驻小节。
        s.write_string_with_format(r, 0, "复核路由（三带）", &head).map_err(err)?;
        for line in calibration_lines(&data.calibration) {
            s.write_string(r, 1, line).map_err(err)?;
            r += 1;
        }
        if let Some(sm) = &data.summary {
            r += 1;
            s.write_string_with_format(r, 0, "八类统计", &head).map_err(err)?;
            r += 1;
            for (name, v) in [
                ("相同", sm.same_count),
                ("轻微修改", sm.minor_change_count),
                ("修改", sm.changed_count),
                ("改写", sm.rewrite_count),
                ("事实冲突", sm.conflict_count),
                ("待复核", sm.uncertain_count),
                ("基准缺失", sm.added_count),
                ("基准独有", sm.deleted_count),
            ] {
                s.write_string(r, 0, name).map_err(err)?;
                s.write_number(r, 1, v as f64).map_err(err)?;
                r += 1;
            }
        }
        r += 1;
        s.write_string_with_format(r, 0, "比对配置", &head).map_err(err)?;
        r += 1;
        if let Some(obj) = data.config.as_object() {
            for (k, v) in obj {
                s.write_string(r, 0, k).map_err(err)?;
                s.write_string(r, 1, v.to_string()).map_err(err)?;
                r += 1;
            }
        }
    }

    // ── 相似度矩阵 ──
    {
        let s = wb.add_worksheet();
        s.set_name("相似度矩阵").map_err(err)?;
        for (j, d) in data.documents.iter().enumerate() {
            let title = format!("{} {}", d.tag, d.name);
            s.write_string_with_format(0, (j + 1) as u16, &title, &head).map_err(err)?;
            s.write_string_with_format(1 + j as u32, 0, &title, &head).map_err(err)?;
        }
        for (i, row) in data.matrix.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                s.write_number_with_format(1 + i as u32, (j + 1) as u16, *v as f64, &pctf)
                    .map_err(err)?;
            }
        }
        let base = data.documents.len() as u32 + 2;
        s.write_string(base, 0, "峰值相似度").map_err(err)?;
        s.write_number_with_format(base, 1, data.peak as f64, &pctf).map_err(err)?;
    }

    // ── 条款明细（每成员一行）──
    {
        let s = wb.add_worksheet();
        s.set_name("条款明细").map_err(err)?;
        for (c, h) in ["组号", "类型", "风险", "复核路由", "确认", "标段", "组内相似", "主题", "文档", "页码", "段落文本"]
            .iter()
            .enumerate()
        {
            s.write_string_with_format(0, c as u16, *h, &head).map_err(err)?;
        }
        let mut r = 1u32;
        for cl in &data.clusters {
            for m in &cl.members {
                s.write_number(r, 0, cl.index as f64).map_err(err)?;
                s.write_string(r, 1, type_cn(&cl.cluster_type)).map_err(err)?;
                s.write_string(r, 2, severity_cn(cl.severity.as_deref().unwrap_or("none")))
                    .map_err(err)?;
                s.write_string(r, 3, band_cn_of(cl.band.as_deref())).map_err(err)?;
                s.write_string(r, 4, review_cn(&cl.review_status)).map_err(err)?;
                s.write_string(r, 5, section_cn(cl.section_kind.as_deref().unwrap_or("other")))
                    .map_err(err)?;
                s.write_number_with_format(r, 6, cl.score.unwrap_or(0.0), &pctf).map_err(err)?;
                s.write_string(r, 7, cl.topic.as_deref().unwrap_or("")).map_err(err)?;
                s.write_string(r, 8, &m.tag).map_err(err)?;
                if let Some(p) = m.page {
                    s.write_number(r, 9, p as f64).map_err(err)?;
                }
                s.write_string(r, 10, &m.text).map_err(err)?;
                r += 1;
            }
        }
    }

    // ── 事实冲突 ──
    {
        let s = wb.add_worksheet();
        s.set_name("事实冲突").map_err(err)?;
        for (c, h) in ["组号", "主题", "风险", "字段", "文档", "值"].iter().enumerate() {
            s.write_string_with_format(0, c as u16, *h, &head).map_err(err)?;
        }
        let mut r = 1u32;
        for cl in data.clusters.iter().filter(|c| c.conflict.is_some()) {
            let cf = cl.conflict.as_ref().unwrap();
            for f in &cf.fields {
                for v in &f.values {
                    s.write_number(r, 0, cl.index as f64).map_err(err)?;
                    s.write_string(r, 1, cl.topic.as_deref().unwrap_or("")).map_err(err)?;
                    s.write_string(r, 2, severity_cn(&cf.risk)).map_err(err)?;
                    s.write_string(r, 3, field_cn(&f.field)).map_err(err)?;
                    s.write_string(r, 4, label(v.doc)).map_err(err)?;
                    s.write_string(r, 5, &v.value).map_err(err)?;
                    r += 1;
                }
            }
        }
    }

    // ── 逐对明细 ──
    {
        let s = wb.add_worksheet();
        s.set_name("逐对明细").map_err(err)?;
        for (c, h) in ["组合", "相似度", "甲方段落", "乙方段落"].iter().enumerate() {
            s.write_string_with_format(0, c as u16, *h, &head).map_err(err)?;
        }
        let mut r = 1u32;
        for p in &data.pairs {
            for m in &p.matches {
                s.write_string(r, 0, format!("{} × {}", label(p.a), label(p.b))).map_err(err)?;
                s.write_number_with_format(r, 1, m.score as f64, &pctf).map_err(err)?;
                s.write_string(r, 2, &m.text_a).map_err(err)?;
                s.write_string(r, 3, &m.text_b).map_err(err)?;
                r += 1;
            }
        }
    }

    // ── 对齐区段与逐字证据（附录 A segments 节；无区段则整表省略）──
    if let Some(seg) = &data.segments {
        let s = wb.add_worksheet();
        s.set_name("对齐区段与逐字证据").map_err(err)?;
        s.set_column_width(1, 34).map_err(err)?;
        s.set_column_width(2, 34).map_err(err)?;
        let mut r = note_row(s, 0, SEGMENTS_NOTE)?;
        r += 1;
        if seg.pairs.iter().any(|p| !p.segments.is_empty()) {
            r = block_title(s, r, "对齐区段摘要", &bold)?;
            r = header_row(
                s,
                r,
                &["文档对", "前者定位", "后者定位", "覆盖", "锚点数", "逐字字数", "标注"],
                &head,
            )?;
            for p in &seg.pairs {
                for x in &p.segments {
                    s.write_string(r, 0, format!("{} ↔ {}", p.a, p.b)).map_err(err)?;
                    s.write_string(r, 1, &x.a_range).map_err(err)?;
                    s.write_string(r, 2, &x.b_range).map_err(err)?;
                    s.write_number_with_format(r, 3, x.coverage, &pctf).map_err(err)?;
                    s.write_number(r, 4, x.anchor_count as f64).map_err(err)?;
                    s.write_number(r, 5, x.verbatim_chars as f64).map_err(err)?;
                    s.write_string(r, 6, tender_badge(x.tender_quote)).map_err(err)?;
                    r += 1;
                }
            }
            r += 1;
        }
        if seg.pairs.iter().any(|p| !p.verbatims.is_empty()) {
            r = block_title(s, r, "逐字雷同区间清单（深红铁证 · 含双侧页码）", &bold)?;
            r = header_row(
                s,
                r,
                &["文档对", "前者页码/章节", "后者页码/章节", "字数", "逐字样本", "标注"],
                &head,
            )?;
            for p in &seg.pairs {
                for v in &p.verbatims {
                    s.write_string(r, 0, format!("{} ↔ {}", p.a, p.b)).map_err(err)?;
                    s.write_string(r, 1, verbatim_locator(v.a_page, v.a_section.as_deref()))
                        .map_err(err)?;
                    s.write_string(r, 2, verbatim_locator(v.b_page, v.b_section.as_deref()))
                        .map_err(err)?;
                    s.write_number(r, 3, v.char_len as f64).map_err(err)?;
                    s.write_string(r, 4, &v.sample).map_err(err)?;
                    s.write_string(r, 5, tender_badge(v.tender_quote)).map_err(err)?;
                    r += 1;
                }
            }
        }
    }

    // ── 商务标数值证据（附录 A numeric 节；无清单数据则整表省略）──
    if let Some(nm) = &data.numeric {
        let s = wb.add_worksheet();
        s.set_name("数值证据").map_err(err)?;
        s.set_column_width(1, 40).map_err(err)?;
        s.set_column_width(2, 40).map_err(err)?;
        let mut r = note_row(s, 0, &numeric_intro(nm))?;
        for x in &nm.notes {
            r = note_row(s, r, x)?;
        }
        r += 1;
        r = block_title(s, r, "逐项单价雷同率", &bold)?;
        r = header_row(s, r, &["文档对", "可比条目", "单价相同", "逐项雷同率", "告警"], &head)?;
        for p in &nm.pairs {
            s.write_string(r, 0, format!("{} ↔ {}", p.a, p.b)).map_err(err)?;
            s.write_number(r, 1, p.comparable as f64).map_err(err)?;
            s.write_number(r, 2, p.identical as f64).map_err(err)?;
            s.write_string(r, 3, numeric_rate_cell(p)).map_err(err)?;
            s.write_string(r, 4, if p.alarm { "达告警线 · 需重点核查" } else { "—" })
                .map_err(err)?;
            r += 1;
        }
        r += 1;
        if nm.pairs.iter().any(|p| p.pattern.is_some() || p.correlation.is_some()) {
            r = block_title(s, r, "规律性差异与相关性", &bold)?;
            r = header_row(s, r, &["文档对", "规律性", "相关性"], &head)?;
            let mut notes: Vec<&str> = Vec::new();
            for p in &nm.pairs {
                if p.pattern.is_none() && p.correlation.is_none() {
                    continue;
                }
                let pat = match &p.pattern {
                    Some(x) => format!(
                        "{}（a={:.4}、b={:.2} 元、R²={:.4}、n={}{}）",
                        numeric_pattern_cn(&x.kind),
                        x.a,
                        x.b,
                        x.r2,
                        x.n,
                        if x.corroborated { "、辅证成立" } else { "" }
                    ),
                    None => "—（未达门槛）".to_string(),
                };
                let cor = match &p.correlation {
                    Some(c) => format!(
                        "Pearson r={:.4}、Spearman ρ={:.4}、比值 CV={}",
                        c.pearson,
                        c.spearman,
                        match c.ratio_cv {
                            Some(cv) => format!("{:.3}%", cv * 100.0),
                            None => "—".to_string(),
                        }
                    ),
                    None => "—（可比条目不足或方差为 0）".to_string(),
                };
                for x in [
                    p.pattern.as_ref().map(|x| x.note.as_str()),
                    p.correlation.as_ref().map(|c| c.note.as_str()),
                ]
                .into_iter()
                .flatten()
                {
                    if !x.is_empty() && !notes.contains(&x) {
                        notes.push(x);
                    }
                }
                r = text_row(s, r, &[format!("{} ↔ {}", p.a, p.b), pat, cor])?;
            }
            for x in notes {
                r = note_row(s, r, x)?;
            }
            r += 1;
        }
        let err_count: usize = nm.pairs.iter().map(|p| p.shared_arith_errors.len()).sum();
        if err_count > 0 {
            r = block_title(s, r, &format!("共享算术错误清单（{err_count} 条）"), &bold)?;
            r = note_row(s, r, ARITH_ERROR_NOTE)?;
            r = header_row(
                s,
                r,
                &["文档对", "清单项", "工程量", "综合单价", "报出合价", "应为", "原文锚点"],
                &head,
            )?;
            for p in &nm.pairs {
                for x in &p.shared_arith_errors {
                    s.write_string(r, 0, format!("{} ↔ {}", p.a, p.b)).map_err(err)?;
                    s.write_string(r, 1, numeric_item_label(x.name.as_deref(), &x.align_key))
                        .map_err(err)?;
                    s.write_number(r, 2, x.qty).map_err(err)?;
                    s.write_number(r, 3, x.unit_price).map_err(err)?;
                    s.write_number(r, 4, x.total).map_err(err)?;
                    s.write_number(r, 5, x.expected_total).map_err(err)?;
                    s.write_string(r, 6, x.chunk_ids.join(" / ")).map_err(err)?;
                    r += 1;
                }
            }
            r += 1;
        }
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            r = block_title(s, r, "逐文档单价尾数分布", &bold)?;
            r = note_row(s, r, DIGIT_TAIL_NOTE)?;
            r = header_row(
                s,
                r,
                &["编号", "样本", "分位 χ²", "角位 χ²", "临界值", "0/5 尾占比", "结论"],
                &head,
            )?;
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                s.write_string(r, 0, &d.tag).map_err(err)?;
                s.write_number(r, 1, digit_stat(ds, "n")).map_err(err)?;
                s.write_number(r, 2, digit_stat(ds, "centChiSquare")).map_err(err)?;
                s.write_number(r, 3, digit_stat(ds, "jiaoChiSquare")).map_err(err)?;
                s.write_number(r, 4, digit_stat(ds, "critical")).map_err(err)?;
                s.write_number_with_format(r, 5, digit_stat(ds, "zeroFiveRatio"), &pctf)
                    .map_err(err)?;
                s.write_string(r, 6, digit_clustered_cn(ds)).map_err(err)?;
                r += 1;
            }
            r += 1;
        }
        // 基准价敏感性（W5-5 机制感知筛查）：【描述性区块，不参与围标分级】
        if let Some(mc) = &nm.mechanism {
            r = block_title(s, r, MECHANISM_TITLE, &bold)?;
            for line in mechanism_lines(mc) {
                r = note_row(s, r, &line)?;
            }
            if !mc.prices.is_empty() {
                r = block_title(s, r, MECHANISM_PRICE_TITLE, &bold)?;
                r = header_row(s, r, &MECHANISM_PRICE_HEADER, &head)?;
                for p in &mc.prices {
                    r = text_row(s, r, &mechanism_price_cells(p))?;
                }
                r += 1;
            }
            if let Some(b) = &mc.benchmark {
                if b.groups.is_empty() {
                    r = note_row(
                        s,
                        r,
                        "未构造出候选组：参评文档间未出现可作依据的既有文档证据（文本相似峰值 / 逐项单价雷同率 / 元数据同源），故不作剔除重算。",
                    )?;
                } else {
                    r = block_title(s, r, MECHANISM_GROUP_TITLE, &bold)?;
                    r = header_row(s, r, &MECHANISM_GROUP_HEADER, &head)?;
                    for g in &b.groups {
                        r = text_row(s, r, &mechanism_group_cells(g))?;
                    }
                    r += 1;
                }
            }
            if !mc.support_bids.is_empty() {
                r = block_title(s, r, MECHANISM_SUPPORT_TITLE, &bold)?;
                r = header_row(s, r, &MECHANISM_SUPPORT_HEADER, &head)?;
                for sb in &mc.support_bids {
                    r = text_row(s, r, &mechanism_support_cells(sb))?;
                }
            }
        }
    }

    // ── 取证证据（附录 A forensic 节；无命中则整表省略）──
    if let Some(f) = &data.forensic {
        let s = wb.add_worksheet();
        s.set_name("取证证据").map_err(err)?;
        s.set_column_width(3, 60).map_err(err)?;
        let mut r = note_row(s, 0, FORENSIC_NOTE)?;
        r += 1;
        r = header_row(s, r, &["类型", "文档", "强度", "说明"], &head)?;
        for hit in &f.hits {
            r = text_row(
                s,
                r,
                &[
                    forensic_kind_cn(&hit.kind).to_string(),
                    forensic_pair_label(&hit.doc_a, &hit.doc_b),
                    forensic_level_cn(&hit.level).to_string(),
                    hit.detail.clone(),
                ],
            )?;
        }
        r += 1;
        r = block_title(s, r, "逐文档取证指纹", &bold)?;
        r = header_row(s, r, &["编号", "rsid 数", "模板", "血缘键"], &head)?;
        for d in &f.per_document {
            s.write_string(r, 0, &d.tag).map_err(err)?;
            s.write_number(r, 1, d.rsid_count as f64).map_err(err)?;
            s.write_string(r, 2, d.template_name.as_deref().unwrap_or("—")).map_err(err)?;
            s.write_string(r, 3, lineage_summary(&d.lineage)).map_err(err)?;
            r += 1;
        }
    }

    // ── 规避特征复核（附录 A evasion 节；仅列达判级线文档，无则整表省略）──
    if let Some(ev) = &data.evasion {
        let s = wb.add_worksheet();
        s.set_name("规避特征").map_err(err)?;
        s.set_column_width(2, 48).map_err(err)?;
        let mut r = note_row(s, 0, EVASION_NOTE)?;
        r += 1;
        r = header_row(s, r, &["编号", "判级", "证据种类"], &head)?;
        for d in &ev.per_document {
            let kinds = if d.evidence_kinds.is_empty() {
                "—".to_string()
            } else {
                d.evidence_kinds.join("、")
            };
            r = text_row(
                s,
                r,
                &[d.tag.clone(), evasion_verdict_cn(&d.verdict).to_string(), kinds],
            )?;
        }
    }

    // ── 复核路由（三带）：恒常驻工作表（W6-4：说明条款按什么口径排的队）──
    {
        let s = wb.add_worksheet();
        s.set_name("复核路由").map_err(err)?;
        s.set_column_width(0, 100).map_err(err)?;
        let mut r = header_row(s, 0, &["说明"], &head)?;
        for line in calibration_lines(&data.calibration) {
            r = note_row(s, r, &line)?;
        }
    }

    // ── 检查方法与局限（附录 A methodsAndLimitations：§1.5 无条件常驻，堵沉默背书）──
    {
        let s = wb.add_worksheet();
        s.set_name("检查方法与局限").map_err(err)?;
        s.set_column_width(1, 100).map_err(err)?;
        let ml = &data.methods_and_limitations;
        let mut r = header_row(s, 0, &["类别", "内容"], &head)?;
        for c in &ml.checks_run {
            r = text_row(s, r, &["已执行检查项".to_string(), c.clone()])?;
        }
        for d in &ml.disclaimers {
            r = text_row(s, r, &["局限与声明".to_string(), d.clone()])?;
        }
    }

    wb.save(path).map_err(err)
}
