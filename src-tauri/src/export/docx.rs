// Word 报告 v2：判定 → 文档 → 八类统计 → 逐对相似 → 事实冲突 → 条款明细 → 附录。
// 最小合法 OOXML（zip + document.xml），Word/WPS/Pages 原生渲染中文。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, calibration_note, contrib_label, docx_p, field_cn, label,
    level_cn, review_cn, severity_cn, strength_phrase, type_cn, write_docx_package,
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
            &data.generated_at[..16].replace('T', " "),
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
        docx_p(
            &mut body,
            "对齐区段为与聚类并存的独立证据层（按 chunk 去重的真实覆盖）。深红＝逐字铁证、橙＝锚点雷同、\
             黄＝gap 细化差异；标注「引用招标文件」者落在招标豁免块，系对同一招标条款的合法应答，非串通证据。\
             未命中不构成清白证明。",
            false,
            20,
        );
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
        docx_p(
            &mut body,
            &format!(
                "报价清单逐项比对：共识别清单条目 {} 条、跨文档对齐 {} 条；雷同率告警线 {:.0}%，\
                 可比条目不足 {} 项的文档对不出结论。",
                nm.item_count,
                nm.aligned_item_count,
                nm.identical_rate_alarm * 100.0,
                nm.min_comparable
            ),
            false,
            20,
        );
        for note in &nm.notes {
            docx_p(&mut body, note, false, 20);
        }
        docx_p(&mut body, "逐项单价雷同率", true, 24);
        for p in &nm.pairs {
            let rate = match p.identical_rate {
                Some(r) => format!("{:.1}%", r * 100.0),
                None => match p.reason.as_deref() {
                    Some("insufficient") => "—（可比条目不足，不出结论）".to_string(),
                    Some(other) => format!("—（{other}）"),
                    None => "—（无可比条目）".to_string(),
                },
            };
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
                            pattern_kind_cn(&x.kind),
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
            docx_p(
                &mut body,
                "同一清单项在两份文件中工程量、综合单价与（算错的）合价三者到分全等。检测已排除可由常见\
                 舍入规则解释的差值；请核对是否源自同一计价软件舍入惯例或招标文件，单条命中不构成串通投标认定。",
                false,
                20,
            );
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
        // 逐文档单价尾数分布
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            docx_p(&mut body, "逐文档单价尾数分布", true, 24);
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                let g = |k: &str| ds.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let clustered =
                    ds.get("clustered").and_then(serde_json::Value::as_bool).unwrap_or(false);
                docx_p(
                    &mut body,
                    &format!(
                        "· {} · 样本 {} · 分位 χ²={:.2} · 角位 χ²={:.2} · 临界值 {:.3} · 0/5 尾占比 {:.0}% · {}",
                        d.tag,
                        g("n") as i64,
                        g("centChiSquare"),
                        g("jiaoChiSquare"),
                        g("critical"),
                        g("zeroFiveRatio") * 100.0,
                        if clustered { "尾数聚集" } else { "未见聚集" }
                    ),
                    false,
                    21,
                );
            }
            docx_p(
                &mut body,
                "尾数聚集反映报价的取整习惯（如统一取整到角/元），单独不构成串通认定，需结合取证类证据；\
                 本工具未做 Benford 首位检验（单价通常只跨 2–3 个数量级，前提不成立）。",
                false,
                20,
            );
        }
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

/// 规律性差异形态的中文标签（numeric.pairs[].pattern.kind）。
fn pattern_kind_cn(kind: &str) -> &str {
    match kind {
        "arith_seq" => "等差（各项差额恒定）",
        "geo_discount" => "等比 / 恒定折扣（各项系数恒定）",
        "affine" => "仿射（系数与差额均非平凡）",
        other => other,
    }
}

/// 逐字区间一侧定位串（页码 + 章节路径 → 单行；docx_p 内部再转义）。
fn verbatim_locator(page: Option<i64>, section: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(pg) = page {
        parts.push(format!("第{pg}页"));
    }
    if let Some(s) = section.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(s.to_string());
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" · ")
    }
}
