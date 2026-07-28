// Markdown 报告：文本归档 / 知识库（§14.1）。条款明细超过上限时显式注明，绝不静默截断。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, calibration_note, contrib_label, digit_clustered_cn, digit_stat,
    evasion_verdict_cn, field_cn, forensic_kind_cn, forensic_level_cn, forensic_pair_label,
    level_cn, lineage_summary, mechanism_group_cells, mechanism_lines, mechanism_price_cells,
    mechanism_support_cells, numeric_intro, numeric_item_label, numeric_pattern_cn,
    numeric_rate_cell, review_cn, section_cn, severity_cn, strength_phrase, type_cn,
    verbatim_locator, ARITH_ERROR_NOTE, DIGIT_TAIL_NOTE, EVASION_NOTE, FORENSIC_NOTE,
    MECHANISM_GROUP_HEADER, MECHANISM_GROUP_TITLE, MECHANISM_PRICE_HEADER, MECHANISM_PRICE_TITLE,
    MECHANISM_SUPPORT_HEADER, MECHANISM_SUPPORT_TITLE, MECHANISM_TITLE, SEGMENTS_NOTE,
};
use std::fmt::Write as _;

const MAX_DETAIL_CLUSTERS: usize = 1000;

/// GFM 表格单元格中和：竖线会截断列、换行会截断行——正文来自投标人（对抗方），
/// 不中和则可构造出「看起来没有证据」的错行表格。
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// 招标豁免标注（§1.5：落在招标豁免块者系合法应答，须显式标注而不是悄悄剔除）。
fn tender_badge(quote: bool) -> &'static str {
    if quote {
        "引用招标文件"
    } else {
        "—"
    }
}

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let mut m = String::new();
    let _ = writeln!(m, "# 原本 · 标书查重报告\n");
    let _ = writeln!(
        m,
        "> 任务：{} · 生成于 {} · 引擎 v{}\n",
        data.job_name.as_deref().unwrap_or("未命名比对"),
        &data.generated_at[..16].replace('T', " "),
        data.app_version
    );

    let _ = writeln!(
        m,
        "## 综合判定\n\n**{}**（证据强度：{}）\n",
        level_cn(&data.collusion.level),
        strength_phrase(&data.collusion.level)
    );
    for s in &data.collusion.signals {
        let _ = writeln!(m, "- {}（{}）", s.detail, contrib_label(s.weight));
    }
    let _ = writeln!(
        m,
        "\n> {}",
        calibration_note(
            &data.collusion.calibration_kind,
            &data.collusion.calibration_version,
            data.app_version
        )
    );

    let _ = writeln!(m, "\n## 参评标书\n\n| 编号 | 名称 | 类型 | 页数 | 字数 | 元数据风险 |");
    let _ = writeln!(m, "|---|---|---|---:|---:|---|");
    for d in &data.documents {
        let _ = writeln!(
            m,
            "| {} | {} | {} | {} | {} | {} |",
            d.tag,
            d.name,
            d.file_type,
            d.pages,
            d.char_count,
            if d.risk_flags.is_empty() { "—".to_string() } else { d.risk_flags.join("；") }
        );
    }

    if let Some(s) = &data.summary {
        let _ = writeln!(m, "\n## 总览统计\n");
        let _ = writeln!(
            m,
            "{} 份文档 · {} 个分块 · {} 组条款 · 峰值相似 {:.0}%\n",
            s.document_count,
            s.chunk_count,
            s.cluster_count,
            data.peak * 100.0
        );
        let _ = writeln!(m, "| 相同 | 轻微修改 | 修改 | 改写 | 冲突 | 待复核 | 基准缺失 | 基准独有 |");
        let _ = writeln!(m, "|---:|---:|---:|---:|---:|---:|---:|---:|");
        let _ = writeln!(
            m,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            s.same_count,
            s.minor_change_count,
            s.changed_count,
            s.rewrite_count,
            s.conflict_count,
            s.uncertain_count,
            s.added_count,
            s.deleted_count
        );
        if s.semantic_degraded {
            let _ = writeln!(m, "\n> 注：语义模型不可用，本次已降级为纯词面比对。");
        }
    }

    let n = data.documents.len();
    let _ = writeln!(m, "\n## 相似度矩阵\n");
    let header: Vec<&str> = data.documents.iter().map(|d| d.tag.as_str()).collect();
    let _ = writeln!(m, "| | {} |", header.join(" | "));
    let _ = writeln!(m, "|---|{}|", vec!["---:"; n].join("|"));
    for (i, row) in data.matrix.iter().enumerate() {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(j, v)| if i == j { "—".into() } else { format!("{:.0}%", v * 100.0) })
            .collect();
        let _ = writeln!(m, "| **{}** | {} |", data.documents[i].tag, cells.join(" | "));
    }

    // 事实冲突优先单列
    let conflicts: Vec<_> = data.clusters.iter().filter(|c| c.conflict.is_some()).collect();
    if !conflicts.is_empty() {
        let _ = writeln!(m, "\n## 事实冲突（{} 处）\n", conflicts.len());
        for c in &conflicts {
            let _ = writeln!(m, "### #{} {}\n", c.index, c.topic.as_deref().unwrap_or(""));
            if let Some(cf) = &c.conflict {
                for f in &cf.fields {
                    let vals: Vec<String> = f
                        .values
                        .iter()
                        .map(|v| format!("「{}」{}", super::shared::label(v.doc), v.value))
                        .collect();
                    let _ = writeln!(m, "- **{}**：{}", field_cn(&f.field), vals.join(" vs "));
                }
            }
            for mem in &c.members {
                let _ = writeln!(m, "> {}：{}", mem.tag, mem.text);
            }
            let _ = writeln!(m);
        }
    }

    // 复核路由三带（W6-4）：恒常驻——报告必须说明条款是按什么口径排的队。
    let _ = writeln!(m, "\n## 复核路由（三带）\n");
    for line in calibration_lines(&data.calibration) {
        let _ = writeln!(m, "- {line}");
    }

    let shown = data.clusters.len().min(MAX_DETAIL_CLUSTERS);
    let _ = writeln!(m, "\n## 雷同条款明细（{} 组）\n", data.clusters.len());
    if data.clusters.len() > MAX_DETAIL_CLUSTERS {
        let _ = writeln!(
            m,
            "> 仅列出前 {MAX_DETAIL_CLUSTERS} 组（按风险与相似度排序）；完整数据请使用 JSON 导出。\n"
        );
    }
    for c in &data.clusters[..shown] {
        let _ = writeln!(
            m,
            "### #{} [{}{}] {} · 相似 {:.0}% · {} · {} · 复核路由：{}\n",
            c.index,
            type_cn(&c.cluster_type),
            c.severity.as_deref().map(|s| format!("·{}", severity_cn(s))).unwrap_or_default(),
            c.topic.as_deref().unwrap_or(""),
            c.score.unwrap_or(0.0) * 100.0,
            section_cn(c.section_kind.as_deref().unwrap_or("other")),
            review_cn(&c.review_status),
            band_cn_of(c.band.as_deref())
        );
        for mem in &c.members {
            let page = mem.page.map(|p| format!("（第 {p} 页）")).unwrap_or_default();
            let _ = writeln!(m, "> **{}**{}：{}", mem.tag, page, mem.text);
        }
        let _ = writeln!(m);
    }

    if !data.shared_terms.is_empty() {
        let _ = writeln!(m, "\n## 共有特征词\n");
        let terms: Vec<&str> = data.shared_terms.iter().map(|t| t.term.as_str()).collect();
        let _ = writeln!(m, "{}", terms.join("、"));
    }

    // 对齐区段与逐字证据（附录 A segments 节；无区段/逐字则整章省略——§1.5 不留空表沉默背书）
    if let Some(seg) = &data.segments {
        let _ = writeln!(m, "\n## 对齐区段与逐字证据\n");
        let _ = writeln!(m, "> {SEGMENTS_NOTE}");
        for p in &seg.pairs {
            let _ = writeln!(m, "\n### {} × {}\n", p.a, p.b);
            if p.segments.is_empty() {
                let _ = writeln!(m, "无对齐区段（仅逐字铁证，见下）。");
            } else {
                let _ = writeln!(m, "对齐区段摘要（{} 段，按逐字字数排序）\n", p.segments.len());
                let _ = writeln!(
                    m,
                    "| {} 侧定位 | {} 侧定位 | 覆盖 | 锚点 | 逐字字数 | 标注 |",
                    cell(&p.a),
                    cell(&p.b)
                );
                let _ = writeln!(m, "|---|---|---:|---:|---:|---|");
                for s in &p.segments {
                    let _ = writeln!(
                        m,
                        "| {} | {} | {:.0}% | {} | {} | {} |",
                        cell(&s.a_range),
                        cell(&s.b_range),
                        s.coverage * 100.0,
                        s.anchor_count,
                        s.verbatim_chars,
                        tender_badge(s.tender_quote)
                    );
                }
            }
            if !p.verbatims.is_empty() {
                let _ = writeln!(
                    m,
                    "\n逐字雷同区间清单（{} 处 · 深红铁证 · 含双侧页码）\n",
                    p.verbatims.len()
                );
                let _ = writeln!(
                    m,
                    "| {} 侧页码/章节 | {} 侧页码/章节 | 字数 | 逐字样本 | 标注 |",
                    cell(&p.a),
                    cell(&p.b)
                );
                let _ = writeln!(m, "|---|---|---:|---|---|");
                for v in &p.verbatims {
                    let _ = writeln!(
                        m,
                        "| {} | {} | {} | {} | {} |",
                        cell(&verbatim_locator(v.a_page, v.a_section.as_deref())),
                        cell(&verbatim_locator(v.b_page, v.b_section.as_deref())),
                        v.char_len,
                        cell(&v.sample),
                        tender_badge(v.tender_quote)
                    );
                }
            }
        }
    }

    // 商务标数值证据（附录 A numeric 节；无清单数据则整章省略）
    if let Some(nm) = &data.numeric {
        let _ = writeln!(m, "\n## 商务标数值证据\n");
        let _ = writeln!(m, "> {}", numeric_intro(nm));
        for note in &nm.notes {
            let _ = writeln!(m, ">\n> {}", cell(note));
        }
        let _ = writeln!(m, "\n### 逐项单价雷同率\n");
        let _ = writeln!(m, "| 文档对 | 可比条目 | 单价相同 | 逐项雷同率 | 告警 |");
        let _ = writeln!(m, "|---|---:|---:|---:|---|");
        for p in &nm.pairs {
            let _ = writeln!(
                m,
                "| {} ↔ {} | {} | {} | {} | {} |",
                cell(&p.a),
                cell(&p.b),
                p.comparable,
                p.identical,
                numeric_rate_cell(p),
                if p.alarm { "达告警线 · 需重点核查" } else { "—" }
            );
        }
        // 规律性差异与相关性（仅列已出结论的对；§1.5 措辞随数据下发，去重后原样引用）
        if nm.pairs.iter().any(|p| p.pattern.is_some() || p.correlation.is_some()) {
            let _ = writeln!(m, "\n### 规律性差异与相关性\n");
            let _ = writeln!(m, "| 文档对 | 规律性 | 相关性 |");
            let _ = writeln!(m, "|---|---|---|");
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
                let _ = writeln!(
                    m,
                    "| {} ↔ {} | {} | {} |",
                    cell(&p.a),
                    cell(&p.b),
                    cell(&pat),
                    cell(&cor)
                );
            }
            let mut notes: Vec<&str> = Vec::new();
            for p in &nm.pairs {
                for n in [
                    p.pattern.as_ref().map(|x| x.note.as_str()),
                    p.correlation.as_ref().map(|c| c.note.as_str()),
                ]
                .into_iter()
                .flatten()
                {
                    if !n.is_empty() && !notes.contains(&n) {
                        notes.push(n);
                    }
                }
            }
            for n in notes {
                let _ = writeln!(m, "\n> {n}");
            }
        }
        // 共享算术错误清单（§1.5 人工核对提示必须随清单出现）
        let errs: Vec<(&str, &str, &super::data::NumericArithError)> = nm
            .pairs
            .iter()
            .flat_map(|p| p.shared_arith_errors.iter().map(move |x| (p.a.as_str(), p.b.as_str(), x)))
            .collect();
        if !errs.is_empty() {
            let _ = writeln!(m, "\n### 共享算术错误清单（{} 条）\n", errs.len());
            let _ = writeln!(m, "> {ARITH_ERROR_NOTE}\n");
            let _ = writeln!(
                m,
                "| 文档对 | 清单项 | 工程量 | 综合单价 | 报出合价 | 应为 | 原文锚点 |"
            );
            let _ = writeln!(m, "|---|---|---:|---:|---:|---:|---|");
            for (a, b, x) in errs {
                let _ = writeln!(
                    m,
                    "| {} ↔ {} | {} | {} | {:.2} | {:.2} | {:.2} | {} |",
                    cell(a),
                    cell(b),
                    cell(&numeric_item_label(x.name.as_deref(), &x.align_key)),
                    x.qty,
                    x.unit_price,
                    x.total,
                    x.expected_total,
                    cell(&x.chunk_ids.join(" / "))
                );
            }
        }
        // 逐文档单价尾数分布
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            let _ = writeln!(m, "\n### 逐文档单价尾数分布\n");
            let _ = writeln!(
                m,
                "| 编号 | 样本 | 分位 χ² | 角位 χ² | 临界值 | 0/5 尾占比 | 结论 |"
            );
            let _ = writeln!(m, "|---|---:|---:|---:|---:|---:|---|");
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                let _ = writeln!(
                    m,
                    "| {} | {} | {:.2} | {:.2} | {:.3} | {:.0}% | {} |",
                    d.tag,
                    digit_stat(ds, "n") as i64,
                    digit_stat(ds, "centChiSquare"),
                    digit_stat(ds, "jiaoChiSquare"),
                    digit_stat(ds, "critical"),
                    digit_stat(ds, "zeroFiveRatio") * 100.0,
                    digit_clustered_cn(ds)
                );
            }
            let _ = writeln!(m, "\n> {DIGIT_TAIL_NOTE}");
        }
        // 基准价敏感性（W5-5 机制感知筛查）：【描述性小节，不参与围标分级】
        if let Some(mc) = &nm.mechanism {
            let _ = writeln!(m, "\n### {MECHANISM_TITLE}\n");
            for line in mechanism_lines(mc) {
                let _ = writeln!(m, "> {}\n", cell(&line));
            }
            if !mc.prices.is_empty() {
                let _ = writeln!(m, "**{MECHANISM_PRICE_TITLE}**\n");
                let _ = writeln!(m, "| {} |", MECHANISM_PRICE_HEADER.join(" | "));
                let _ = writeln!(m, "|---|---:|---|");
                for p in &mc.prices {
                    let c = mechanism_price_cells(p);
                    let _ = writeln!(m, "| {} | {} | {} |", cell(&c[0]), cell(&c[1]), cell(&c[2]));
                }
            }
            if let Some(b) = &mc.benchmark {
                if b.groups.is_empty() {
                    let _ = writeln!(
                        m,
                        "\n> 未构造出候选组：参评文档间未出现可作依据的既有文档证据（文本相似峰值 / 逐项单价雷同率 / 元数据同源），故不作剔除重算。"
                    );
                } else {
                    let _ = writeln!(m, "\n**{MECHANISM_GROUP_TITLE}**\n");
                    let _ = writeln!(m, "| {} |", MECHANISM_GROUP_HEADER.join(" | "));
                    let _ = writeln!(m, "|---|---|---:|---:|---:|---|");
                    for g in &b.groups {
                        let c = mechanism_group_cells(g);
                        let cells: Vec<String> = c.iter().map(|x| cell(x)).collect();
                        let _ = writeln!(m, "| {} |", cells.join(" | "));
                    }
                }
            }
            if !mc.support_bids.is_empty() {
                let _ = writeln!(m, "\n**{MECHANISM_SUPPORT_TITLE}**\n");
                let _ = writeln!(m, "| {} |", MECHANISM_SUPPORT_HEADER.join(" | "));
                let _ = writeln!(m, "|---|---:|---|---|---:|");
                for s in &mc.support_bids {
                    let c = mechanism_support_cells(s);
                    let cells: Vec<String> = c.iter().map(|x| cell(x)).collect();
                    let _ = writeln!(m, "| {} |", cells.join(" | "));
                }
            }
        }
    }

    // 取证证据（附录 A forensic 节；无命中则整章省略）
    if let Some(f) = &data.forensic {
        let _ = writeln!(m, "\n## 取证证据\n");
        let _ = writeln!(m, "> {FORENSIC_NOTE}\n");
        let _ = writeln!(m, "| 类型 | 文档 | 强度 | 说明 |");
        let _ = writeln!(m, "|---|---|---|---|");
        for hit in &f.hits {
            let _ = writeln!(
                m,
                "| {} | {} | {} | {} |",
                forensic_kind_cn(&hit.kind),
                cell(&forensic_pair_label(&hit.doc_a, &hit.doc_b)),
                forensic_level_cn(&hit.level),
                cell(&hit.detail)
            );
        }
        let _ = writeln!(m, "\n逐文档取证指纹\n");
        let _ = writeln!(m, "| 编号 | rsid 数 | 模板 | 血缘键 |");
        let _ = writeln!(m, "|---|---:|---|---|");
        for d in &f.per_document {
            let _ = writeln!(
                m,
                "| {} | {} | {} | {} |",
                d.tag,
                d.rsid_count,
                cell(d.template_name.as_deref().unwrap_or("—")),
                cell(&lineage_summary(&d.lineage))
            );
        }
    }

    // 规避特征复核（附录 A evasion 节；仅列达判级线文档，无则整章省略）
    if let Some(ev) = &data.evasion {
        let _ = writeln!(m, "\n## 规避特征复核\n");
        let _ = writeln!(m, "> {EVASION_NOTE}\n");
        let _ = writeln!(m, "| 编号 | 判级 | 证据种类 |");
        let _ = writeln!(m, "|---|---|---|");
        for d in &ev.per_document {
            let kinds =
                if d.evidence_kinds.is_empty() { "—".to_string() } else { d.evidence_kinds.join("、") };
            let _ = writeln!(
                m,
                "| {} | {} | {} |",
                d.tag,
                evasion_verdict_cn(&d.verdict),
                cell(&kinds)
            );
        }
    }

    // 检查方法与局限（附录 A methodsAndLimitations：§1.5 无条件常驻，堵沉默背书）
    let ml = &data.methods_and_limitations;
    let _ = writeln!(m, "\n## 检查方法与局限\n");
    let _ = writeln!(m, "**本次已执行的取证 / 对抗检查项：**\n");
    for c in &ml.checks_run {
        let _ = writeln!(m, "- {c}");
    }
    let _ = writeln!(m, "\n**局限与声明：**\n");
    for d in &ml.disclaimers {
        let _ = writeln!(m, "- {d}");
    }

    let _ = writeln!(m, "\n## 附录：比对配置\n\n```json\n{}\n```\n", data.config);
    let _ = writeln!(
        m,
        "*报告由「原本 · 标书查重」v{} 本地生成（报告格式 {}），未上传任何文件。*",
        data.app_version, data.report_version
    );

    std::fs::write(path, m).map_err(|e| e.to_string())
}
