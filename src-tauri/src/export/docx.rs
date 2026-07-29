// Word 报告 v2：判定 → 文档 → 八类统计 → 逐对相似 → 事实冲突 → 条款明细 → 附录。
// 最小合法 OOXML（zip + document.xml），Word/WPS/Pages 原生渲染中文。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, calibration_note, contrib_label, digit_clustered_cn, digit_stat,
    docx_p, evasion_verdict_cn, field_cn, forensic_kind_cn, forensic_level_cn, forensic_pair_label,
    label, level_cn, lineage_summary, mechanism_group_cells, mechanism_lines,
    mechanism_price_cells, mechanism_support_cells, numeric_intro, numeric_pattern_cn,
    numeric_rate_cell, review_cn, severity_cn, strength_phrase, type_cn, verbatim_locator,
    write_docx_package, ARITH_ERROR_NOTE, DIGIT_TAIL_NOTE, EVASION_NOTE, FORENSIC_NOTE,
    MECHANISM_GROUP_TITLE, MECHANISM_PRICE_TITLE, MECHANISM_SUPPORT_TITLE, MECHANISM_TITLE,
    SEGMENTS_NOTE,
};

const MAX_DETAIL_CLUSTERS: usize = 500;

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let mut body = String::new();
    docx_p(&mut body, "原本 · 标书查重报告", true, 36);
    docx_p(
        &mut body,
        &format!(
            "任务：{} · 生成于 {} · 引擎 v{}",
            data.job_name.as_deref().unwrap_or("未命名比对"),
            data.generated_at[..16].replace('T', " "),
            data.app_version
        ),
        false,
        20,
    );
    docx_p(
        &mut body,
        &format!(
            "综合判定：{}（证据强度：{}）",
            level_cn(&data.collusion.level),
            strength_phrase(&data.collusion.level)
        ),
        true,
        26,
    );
    for s in &data.collusion.signals {
        docx_p(&mut body, &format!("· {}（{}）", s.detail, contrib_label(s.weight)), false, 21);
    }
    docx_p(
        &mut body,
        &calibration_note(
            &data.collusion.calibration_kind,
            &data.collusion.calibration_version,
            data.app_version,
        ),
        false,
        19,
    );

    docx_p(&mut body, "参评标书", true, 28);
    for d in &data.documents {
        let flags = if d.risk_flags.is_empty() {
            String::new()
        } else {
            format!("（{}）", d.risk_flags.join("；"))
        };
        docx_p(
            &mut body,
            &format!("{} {} · {} · {} 页{flags}", d.tag, d.name, d.file_type, d.pages),
            false,
            21,
        );
    }

    if let Some(sm) = &data.summary {
        docx_p(&mut body, "总览统计", true, 28);
        docx_p(
            &mut body,
            &format!(
                "{} 份文档 · {} 个分块 · {} 组条款 · 峰值相似 {:.0}%",
                sm.document_count,
                sm.chunk_count,
                sm.cluster_count,
                data.peak * 100.0
            ),
            false,
            21,
        );
        docx_p(
            &mut body,
            &format!(
                "相同 {} · 轻微修改 {} · 修改 {} · 改写 {} · 事实冲突 {} · 待复核 {} · 基准缺失 {} · 基准独有 {}",
                sm.same_count,
                sm.minor_change_count,
                sm.changed_count,
                sm.rewrite_count,
                sm.conflict_count,
                sm.uncertain_count,
                sm.added_count,
                sm.deleted_count
            ),
            false,
            21,
        );
        if sm.semantic_degraded {
            docx_p(&mut body, "注：语义模型不可用，本次已降级为纯词面比对。", false, 20);
        }
    }

    docx_p(&mut body, "相似度（两两）", true, 28);
    for pr in &data.pairs {
        docx_p(
            &mut body,
            &format!(
                "{} × {}：{:.0}%（{} 处雷同片段）",
                label(pr.a),
                label(pr.b),
                pr.score * 100.0,
                pr.matches.len()
            ),
            false,
            21,
        );
    }

    let conflicts: Vec<_> = data.clusters.iter().filter(|c| c.conflict.is_some()).collect();
    if !conflicts.is_empty() {
        docx_p(&mut body, &format!("事实冲突（{} 处）", conflicts.len()), true, 28);
        for c in &conflicts {
            docx_p(
                &mut body,
                &format!(
                    "#{} {}（{}）",
                    c.index,
                    c.topic.as_deref().unwrap_or(""),
                    severity_cn(c.severity.as_deref().unwrap_or("high"))
                ),
                true,
                22,
            );
            if let Some(cf) = &c.conflict {
                for f in &cf.fields {
                    let vals: Vec<String> = f
                        .values
                        .iter()
                        .map(|v| format!("「{}」{}", label(v.doc), v.value))
                        .collect();
                    docx_p(&mut body, &format!("　{}：{}", field_cn(&f.field), vals.join(" vs ")), false, 21);
                }
            }
            for m in &c.members {
                docx_p(&mut body, &format!("　{}：{}", m.tag, m.text), false, 21);
            }
        }
    }

    // 复核路由三带（W6-4）：恒常驻小节，说明条款按什么口径排队 + §1.5 强制措辞。
    docx_p(&mut body, "复核路由（三带）", true, 28);
    for line in calibration_lines(&data.calibration) {
        docx_p(&mut body, &format!("· {line}"), false, 21);
    }

    let shown = data.clusters.len().min(MAX_DETAIL_CLUSTERS);
    docx_p(&mut body, &format!("雷同条款明细（{} 组）", data.clusters.len()), true, 28);
    if data.clusters.len() > MAX_DETAIL_CLUSTERS {
        docx_p(
            &mut body,
            &format!("仅列出前 {MAX_DETAIL_CLUSTERS} 组（按风险与相似度排序）；完整数据请使用 JSON / CSV 导出。"),
            false,
            20,
        );
    }
    for c in &data.clusters[..shown] {
        let docs: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for m in &c.members {
                if !seen.contains(&m.tag.as_str()) {
                    seen.push(&m.tag);
                }
            }
            seen
        };
        docx_p(
            &mut body,
            &format!(
                "#{} [{}] {} · 相似 {:.0}% · 涉及 {} · {} · 复核路由：{}",
                c.index,
                type_cn(&c.cluster_type),
                c.topic.as_deref().unwrap_or(""),
                c.score.unwrap_or(0.0) * 100.0,
                docs.join("·"),
                review_cn(&c.review_status),
                band_cn_of(c.band.as_deref())
            ),
            true,
            22,
        );
        for m in &c.members {
            docx_p(&mut body, &format!("　{}：{}", m.tag, m.text), false, 21);
        }
    }

    if !data.shared_terms.is_empty() {
        docx_p(&mut body, "共有特征词", true, 28);
        let terms: Vec<&str> = data.shared_terms.iter().take(40).map(|t| t.term.as_str()).collect();
        docx_p(&mut body, &terms.join("、"), false, 21);
    }

    // 对齐区段与逐字证据（附录 A segments 节；§1.5 第二种正式格式。无区段/逐字则整章省略）
    if let Some(seg) = &data.segments {
        docx_p(&mut body, "对齐区段与逐字证据", true, 28);
        docx_p(&mut body, SEGMENTS_NOTE, false, 20);
        for p in &seg.pairs {
            docx_p(&mut body, &format!("{} × {}", p.a, p.b), true, 24);
            if p.segments.is_empty() {
                docx_p(&mut body, "　无对齐区段（仅逐字铁证，见下）。", false, 21);
            } else {
                docx_p(&mut body, &format!("对齐区段摘要（{} 段）", p.segments.len()), true, 22);
                for s in &p.segments {
                    let badge = if s.tender_quote { "　[引用招标文件]" } else { "" };
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} ↔ {} · 覆盖 {:.0}% · 锚点 {} · 逐字 {} 字{badge}",
                            s.a_range,
                            s.b_range,
                            s.coverage * 100.0,
                            s.anchor_count,
                            s.verbatim_chars
                        ),
                        false,
                        21,
                    );
                }
            }
            if !p.verbatims.is_empty() {
                docx_p(
                    &mut body,
                    &format!("逐字雷同区间清单（{} 处 · 含双侧页码）", p.verbatims.len()),
                    true,
                    22,
                );
                for v in &p.verbatims {
                    let badge = if v.tender_quote { "　[引用招标文件]" } else { "" };
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} ↔ {} · {} 字：{}{badge}",
                            verbatim_locator(v.a_page, v.a_section.as_deref()),
                            verbatim_locator(v.b_page, v.b_section.as_deref()),
                            v.char_len,
                            v.sample
                        ),
                        false,
                        21,
                    );
                }
            }
        }
    }

    // 商务标数值证据（附录 A numeric 节；§1.5 第二种正式格式。无清单数据则整章省略）
    if let Some(nm) = &data.numeric {
        docx_p(&mut body, "商务标数值证据", true, 28);
        docx_p(&mut body, &numeric_intro(nm), false, 20);
        for note in &nm.notes {
            docx_p(&mut body, note, false, 20);
        }
        docx_p(&mut body, "逐项单价雷同率", true, 24);
        for p in &nm.pairs {
            let rate = numeric_rate_cell(p);
            docx_p(
                &mut body,
                &format!(
                    "· {} ↔ {} · 可比 {} 项 · 单价相同 {} 项 · 雷同率 {}{}",
                    p.a,
                    p.b,
                    p.comparable,
                    p.identical,
                    rate,
                    if p.alarm { "　[达告警线 · 需重点核查]" } else { "" }
                ),
                false,
                21,
            );
        }
        // 规律性差异 / 相关性结论
        if nm.pairs.iter().any(|p| p.pattern.is_some() || p.correlation.is_some()) {
            docx_p(&mut body, "规律性差异与相关性", true, 24);
            for p in &nm.pairs {
                if let Some(x) = &p.pattern {
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} ↔ {} · 规律性：{} · a={:.4} · b={:.2} 元 · R²={:.4} · n={}{}。{}",
                            p.a,
                            p.b,
                            numeric_pattern_cn(&x.kind),
                            x.a,
                            x.b,
                            x.r2,
                            x.n,
                            if x.corroborated { " · 辅证成立" } else { "" },
                            x.note
                        ),
                        false,
                        21,
                    );
                }
                if let Some(c) = &p.correlation {
                    let cv = match c.ratio_cv {
                        Some(cv) => format!("{:.3}%", cv * 100.0),
                        None => "—".to_string(),
                    };
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} ↔ {} · 相关性：Pearson r={:.4} · Spearman ρ={:.4} · 比值 CV={} · n={}。{}",
                            p.a, p.b, c.pearson, c.spearman, cv, c.n, c.note
                        ),
                        false,
                        21,
                    );
                }
            }
        }
        // 共享算术错误清单（§1.5 提示必须随清单出现）
        let err_count: usize = nm.pairs.iter().map(|p| p.shared_arith_errors.len()).sum();
        if err_count > 0 {
            docx_p(&mut body, &format!("共享算术错误清单（{err_count} 条）"), true, 24);
            docx_p(&mut body, ARITH_ERROR_NOTE, false, 20);
            for p in &nm.pairs {
                for x in &p.shared_arith_errors {
                    let name = x.name.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("—");
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} ↔ {} · {}（{}）· 工程量 {} × 单价 {:.2} · 报出合价 {:.2}（应为 {:.2}）· 原文锚点 {}",
                            p.a,
                            p.b,
                            name,
                            x.align_key,
                            x.qty,
                            x.unit_price,
                            x.total,
                            x.expected_total,
                            x.chunk_ids.join(" / ")
                        ),
                        false,
                        21,
                    );
                }
            }
        }
        // 基准价敏感性（W5-5 机制感知筛查）：【描述性小节，不参与围标分级】
        if let Some(mc) = &nm.mechanism {
            docx_p(&mut body, MECHANISM_TITLE, true, 24);
            for line in mechanism_lines(mc) {
                docx_p(&mut body, &line, false, 20);
            }
            if !mc.prices.is_empty() {
                docx_p(&mut body, MECHANISM_PRICE_TITLE, true, 21);
                for p in &mc.prices {
                    let c = mechanism_price_cells(p);
                    docx_p(&mut body, &format!("· {} · {} 元 · {}", c[0], c[1], c[2]), false, 21);
                }
            }
            if let Some(b) = &mc.benchmark {
                if b.groups.is_empty() {
                    docx_p(
                        &mut body,
                        "未构造出候选组：参评文档间未出现可作依据的既有文档证据（文本相似峰值 / 逐项单价雷同率 / 元数据同源），故不作剔除重算。",
                        false,
                        20,
                    );
                } else {
                    docx_p(&mut body, MECHANISM_GROUP_TITLE, true, 21);
                    for g in &b.groups {
                        let c = mechanism_group_cells(g);
                        docx_p(
                            &mut body,
                            &format!(
                                "· {} · 中标人翻转比例 {} · 基准价偏移 {} · 同规模子集分位 {} · 中标人 {} · 构造依据：{}",
                                c[0], c[2], c[3], c[4], c[5], c[1]
                            ),
                            false,
                            21,
                        );
                    }
                }
            }
            if !mc.support_bids.is_empty() {
                docx_p(&mut body, MECHANISM_SUPPORT_TITLE, true, 21);
                for s in &mc.support_bids {
                    let c = mechanism_support_cells(s);
                    docx_p(
                        &mut body,
                        &format!(
                            "· {} · {} 元 · {} · 与次邻间距 {} · 偏离中位数 {}",
                            c[0], c[1], c[2], c[3], c[4]
                        ),
                        false,
                        21,
                    );
                }
            }
        }
        // 逐文档单价尾数分布
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            docx_p(&mut body, "逐文档单价尾数分布", true, 24);
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                docx_p(
                    &mut body,
                    &format!(
                        "· {} · 样本 {} · 分位 χ²={:.2} · 角位 χ²={:.2} · 临界值 {:.3} · 0/5 尾占比 {:.0}% · {}",
                        d.tag,
                        digit_stat(ds, "n") as i64,
                        digit_stat(ds, "centChiSquare"),
                        digit_stat(ds, "jiaoChiSquare"),
                        digit_stat(ds, "critical"),
                        digit_stat(ds, "zeroFiveRatio") * 100.0,
                        digit_clustered_cn(ds)
                    ),
                    false,
                    21,
                );
            }
            docx_p(&mut body, DIGIT_TAIL_NOTE, false, 20);
        }
    }

    // 取证证据（附录 A forensic 节；无命中不渲染——§1.5 不留空表沉默背书）
    if let Some(f) = &data.forensic {
        docx_p(&mut body, "取证证据", true, 28);
        docx_p(&mut body, FORENSIC_NOTE, false, 20);
        for hit in &f.hits {
            docx_p(
                &mut body,
                &format!(
                    "· {} · {} · 强度 {} — {}",
                    forensic_kind_cn(&hit.kind),
                    forensic_pair_label(&hit.doc_a, &hit.doc_b),
                    forensic_level_cn(&hit.level),
                    hit.detail
                ),
                false,
                21,
            );
        }
        docx_p(&mut body, "逐文档取证指纹", true, 22);
        for d in &f.per_document {
            docx_p(
                &mut body,
                &format!(
                    "　{}：rsid {} 个 · 模板 {} · 血缘 {}",
                    d.tag,
                    d.rsid_count,
                    d.template_name.as_deref().unwrap_or("—"),
                    lineage_summary(&d.lineage)
                ),
                false,
                20,
            );
        }
    }

    // 规避特征复核（附录 A evasion 节；仅列达判级线文档，无则不渲染）
    if let Some(ev) = &data.evasion {
        docx_p(&mut body, "规避特征复核", true, 28);
        docx_p(&mut body, EVASION_NOTE, false, 20);
        for d in &ev.per_document {
            let kinds =
                if d.evidence_kinds.is_empty() { "—".to_string() } else { d.evidence_kinds.join("、") };
            docx_p(
                &mut body,
                &format!("　{}：{} · 证据种类 {}", d.tag, evasion_verdict_cn(&d.verdict), kinds),
                false,
                21,
            );
        }
    }

    // 检查方法与局限（附录 A methodsAndLimitations：§1.5 无条件常驻，堵沉默背书——
    // 缺此节时「没有取证章节」会被读成「查过了，干净」，而 rsid/元数据/规避恰恰都可被清除）。
    let ml = &data.methods_and_limitations;
    docx_p(&mut body, "检查方法与局限", true, 28);
    docx_p(&mut body, "本次已执行的取证 / 对抗检查项：", true, 22);
    for c in &ml.checks_run {
        docx_p(&mut body, &format!("　· {c}"), false, 20);
    }
    docx_p(&mut body, "局限与声明：", true, 22);
    for d in &ml.disclaimers {
        docx_p(&mut body, &format!("　· {d}"), false, 20);
    }

    docx_p(&mut body, "附录：比对配置", true, 28);
    docx_p(&mut body, &data.config.to_string(), false, 18);
    docx_p(
        &mut body,
        &format!("报告格式 {} · 引擎 v{} · 本地生成，未上传任何文件。", data.report_version, data.app_version),
        false,
        18,
    );

    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    write_docx_package(path, &doc)
}

