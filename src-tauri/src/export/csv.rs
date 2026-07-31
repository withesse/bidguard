// CSV 报告：扁平多区块（条款级每成员一行 + 各类证据各一区块），UTF-8 BOM 让 Excel 直接识别中文。
// CSV 无工作表概念，故以「## 区块名」行分节、文件头给出区块清单与含义（§1.5：屏幕可见的
// 证据类型必须在正式报告格式中可引用，扁平格式也不得丢信息）。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, digit_clustered_cn, digit_stat, evasion_verdict_cn,
    forensic_kind_cn, forensic_level_cn, forensic_pair_label, lineage_summary,
    mechanism_group_cells, mechanism_lines, mechanism_price_cells, mechanism_support_cells,
    numeric_intro, numeric_item_label, numeric_pattern_cn, numeric_rate_cell, review_cn, section_cn,
    severity_cn, type_cn, verbatim_locator, ARITH_ERROR_NOTE, DIGIT_TAIL_NOTE, EVASION_NOTE,
    FORENSIC_NOTE, MECHANISM_GROUP_HEADER, MECHANISM_GROUP_TITLE, MECHANISM_PRICE_HEADER,
    MECHANISM_SUPPORT_HEADER, MECHANISM_SUPPORT_TITLE, MECHANISM_TITLE, SEGMENTS_NOTE,
};

fn esc(s: &str) -> String {
    // CWE-1236 CSV 公式注入防护：标书正文来自投标人（对抗方），若单元格以 = + - @ 或
    // TAB/CR 开头，Excel/WPS 打开时会当公式执行（可外带同表数据或诱导命令）。前置单引号中和，
    // 再做引号转义。必须先中和后转义，保证外层引号包裹逻辑不变。
    let neutralized = match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') => {
            format!("'{s}")
        }
        _ => s.to_string(),
    };
    format!("\"{}\"", neutralized.replace('"', "\"\""))
}

/// 一行 CSV（cells 由调用方决定是否 esc：自由文本走 esc，数值裸写便于 Excel 按数值列读）。
fn row(out: &mut String, cells: &[String]) {
    out.push_str(&cells.join(","));
    out.push('\n');
}

/// 区块起始：空行 +「## 区块名」+ 表头行。
fn block(out: &mut String, title: &str, header: &[&str]) {
    out.push('\n');
    row(out, &[esc(&format!("## {title}"))]);
    row(out, &header.iter().map(|h| esc(h)).collect::<Vec<_>>());
}

/// 区块内的说明行（§1.5 强制措辞：随区块出现，不得省略）。
fn note(out: &mut String, text: &str) {
    row(out, &[esc(&format!("# 说明：{text}"))]);
}

fn n(v: impl std::fmt::Display) -> String {
    v.to_string()
}

/// 招标豁免标注（落在招标豁免块者系合法应答，须显式标注而不是悄悄剔除）。
fn tender_badge(quote: bool) -> &'static str {
    if quote {
        "引用招标文件"
    } else {
        "—"
    }
}

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let mut out = String::from("\u{feff}");
    row(&mut out, &[esc("# 原本 · 标书查重报告（CSV 分区块导出）")]);
    row(
        &mut out,
        &[esc(&format!(
            "# 任务：{} · 生成于 {} · 引擎 v{} · 报告格式 {}",
            data.job_name.as_deref().unwrap_or("未命名比对"),
            data.generated_at[..16].replace('T', " "),
            data.app_version,
            data.report_version
        ))],
    );
    row(
        &mut out,
        &[esc(
            "# 本文件按证据类型分区块：每个「## 区块名」行开启一个区块，其下一行为该区块表头，\
             「# 说明：」行为该区块的口径与免责说明。",
        )],
    );
    row(
        &mut out,
        &[esc(
            "# 区块清单：条款明细 / 复核路由（三带） / 对齐区段 / 逐字雷同区间 / 逐项单价雷同率 / \
             规律性差异与相关性 / 共享算术错误清单 / 逐文档单价尾数分布 / 基准价敏感性（反事实解释性分析） / \
             候选组反事实结果 / 断崖式报价（support-bid 形态） / 取证证据 / 逐文档取证指纹 / \
             规避特征复核 / 检查方法与局限。",
        )],
    );
    row(
        &mut out,
        &[esc(
            "# 未出现的区块表示本次未发现该类证据；未命中不构成清白证明，详见「检查方法与局限」区块。",
        )],
    );

    block(
        &mut out,
        "条款明细",
        &[
            "组号",
            "类型",
            "风险",
            "复核路由",
            "确认状态",
            "标段",
            "组内相似",
            "主题",
            "涉及文档",
            "文档",
            "角色",
            "页码",
            "章节路径",
            "文本",
        ],
    );
    for c in &data.clusters {
        let docs: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for m in &c.members {
                if !seen.contains(&m.tag.as_str()) {
                    seen.push(&m.tag);
                }
            }
            seen
        };
        for m in &c.members {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                c.index,
                type_cn(&c.cluster_type),
                severity_cn(c.severity.as_deref().unwrap_or("none")),
                band_cn_of(c.band.as_deref()),
                review_cn(&c.review_status),
                section_cn(c.section_kind.as_deref().unwrap_or("other")),
                c.score.map(|s| format!("{:.0}%", s * 100.0)).unwrap_or_default(),
                esc(c.topic.as_deref().unwrap_or("")),
                docs.join("·"),
                m.tag,
                if m.role == "primary" { "主" } else { "重复" },
                m.page.map(|p| p.to_string()).unwrap_or_default(),
                esc(&m.section_path.join(" › ")),
                esc(&m.text),
            ));
        }
    }

    // 复核路由三带（W6-4）：恒常驻区块——报告必须说明条款是按什么口径排的队。
    block(&mut out, "复核路由（三带）", &["说明"]);
    for line in calibration_lines(&data.calibration) {
        row(&mut out, &[esc(&line)]);
    }

    // 对齐区段与逐字证据（附录 A segments 节；无区段则两区块整体省略）
    if let Some(seg) = &data.segments {
        let has_segments = seg.pairs.iter().any(|p| !p.segments.is_empty());
        if has_segments {
            block(
                &mut out,
                "对齐区段",
                &["文档对", "前者定位", "后者定位", "覆盖", "锚点数", "逐字字数", "标注"],
            );
            note(&mut out, SEGMENTS_NOTE);
            for p in &seg.pairs {
                for s in &p.segments {
                    row(
                        &mut out,
                        &[
                            esc(&format!("{} ↔ {}", p.a, p.b)),
                            esc(&s.a_range),
                            esc(&s.b_range),
                            n(format!("{:.0}%", s.coverage * 100.0)),
                            n(s.anchor_count),
                            n(s.verbatim_chars),
                            esc(tender_badge(s.tender_quote)),
                        ],
                    );
                }
            }
        }
        if seg.pairs.iter().any(|p| !p.verbatims.is_empty()) {
            block(
                &mut out,
                "逐字雷同区间",
                &["文档对", "前者页码/章节", "后者页码/章节", "字数", "逐字样本", "标注"],
            );
            if !has_segments {
                note(&mut out, SEGMENTS_NOTE);
            }
            for p in &seg.pairs {
                for v in &p.verbatims {
                    row(
                        &mut out,
                        &[
                            esc(&format!("{} ↔ {}", p.a, p.b)),
                            esc(&verbatim_locator(v.a_page, v.a_section.as_deref())),
                            esc(&verbatim_locator(v.b_page, v.b_section.as_deref())),
                            n(v.char_len),
                            esc(&v.sample),
                            esc(tender_badge(v.tender_quote)),
                        ],
                    );
                }
            }
        }
    }

    // 商务标数值证据（附录 A numeric 节；无清单数据则各区块整体省略）
    if let Some(nm) = &data.numeric {
        block(
            &mut out,
            "逐项单价雷同率",
            &["文档对", "可比条目", "单价相同", "逐项雷同率", "告警"],
        );
        note(&mut out, &numeric_intro(nm));
        for x in &nm.notes {
            note(&mut out, x);
        }
        for p in &nm.pairs {
            row(
                &mut out,
                &[
                    esc(&format!("{} ↔ {}", p.a, p.b)),
                    n(p.comparable),
                    n(p.identical),
                    esc(&numeric_rate_cell(p)),
                    esc(if p.alarm { "达告警线 · 需重点核查" } else { "—" }),
                ],
            );
        }
        if nm.pairs.iter().any(|p| p.pattern.is_some() || p.correlation.is_some()) {
            block(&mut out, "规律性差异与相关性", &["文档对", "规律性", "相关性", "口径说明"]);
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
                let mut notes: Vec<&str> = Vec::new();
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
                row(
                    &mut out,
                    &[
                        esc(&format!("{} ↔ {}", p.a, p.b)),
                        esc(&pat),
                        esc(&cor),
                        esc(&notes.join(" ")),
                    ],
                );
            }
        }
        let err_count: usize = nm.pairs.iter().map(|p| p.shared_arith_errors.len()).sum();
        if err_count > 0 {
            block(
                &mut out,
                "共享算术错误清单",
                &["文档对", "清单项", "工程量", "综合单价", "报出合价", "应为", "原文锚点"],
            );
            note(&mut out, ARITH_ERROR_NOTE);
            for p in &nm.pairs {
                for x in &p.shared_arith_errors {
                    row(
                        &mut out,
                        &[
                            esc(&format!("{} ↔ {}", p.a, p.b)),
                            esc(&numeric_item_label(x.name.as_deref(), &x.align_key)),
                            n(x.qty),
                            n(format!("{:.2}", x.unit_price)),
                            n(format!("{:.2}", x.total)),
                            n(format!("{:.2}", x.expected_total)),
                            esc(&x.chunk_ids.join(" / ")),
                        ],
                    );
                }
            }
        }
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            block(
                &mut out,
                "逐文档单价尾数分布",
                &["编号", "样本", "分位 χ²", "角位 χ²", "临界值", "0/5 尾占比", "结论"],
            );
            note(&mut out, DIGIT_TAIL_NOTE);
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                row(
                    &mut out,
                    &[
                        esc(&d.tag),
                        n(digit_stat(ds, "n") as i64),
                        n(format!("{:.2}", digit_stat(ds, "centChiSquare"))),
                        n(format!("{:.2}", digit_stat(ds, "jiaoChiSquare"))),
                        n(format!("{:.3}", digit_stat(ds, "critical"))),
                        n(format!("{:.0}%", digit_stat(ds, "zeroFiveRatio") * 100.0)),
                        esc(digit_clustered_cn(ds)),
                    ],
                );
            }
        }
        // 基准价敏感性（W5-5 机制感知筛查）：【描述性区块，不参与围标分级】
        if let Some(mc) = &nm.mechanism {
            block(&mut out, MECHANISM_TITLE, &MECHANISM_PRICE_HEADER);
            for line in mechanism_lines(mc) {
                note(&mut out, &line);
            }
            for p in &mc.prices {
                let c = mechanism_price_cells(p);
                row(&mut out, &[esc(&c[0]), n(&c[1]), esc(&c[2])]);
            }
            if let Some(b) = &mc.benchmark {
                if b.groups.is_empty() {
                    note(
                        &mut out,
                        "未构造出候选组：参评文档间未出现可作依据的既有文档证据（文本相似峰值 / 逐项单价雷同率 / 元数据同源），故不作剔除重算。",
                    );
                } else {
                    block(&mut out, MECHANISM_GROUP_TITLE, &MECHANISM_GROUP_HEADER);
                    for g in &b.groups {
                        let c = mechanism_group_cells(g);
                        row(&mut out, &c.iter().map(|x| esc(x)).collect::<Vec<_>>());
                    }
                }
            }
            if !mc.support_bids.is_empty() {
                block(&mut out, MECHANISM_SUPPORT_TITLE, &MECHANISM_SUPPORT_HEADER);
                for s in &mc.support_bids {
                    let c = mechanism_support_cells(s);
                    row(&mut out, &c.iter().map(|x| esc(x)).collect::<Vec<_>>());
                }
            }
        }
    }

    // 取证证据（附录 A forensic 节；无命中则两区块整体省略）
    if let Some(f) = &data.forensic {
        block(&mut out, "取证证据", &["类型", "文档", "强度", "说明"]);
        note(&mut out, FORENSIC_NOTE);
        for hit in &f.hits {
            row(
                &mut out,
                &[
                    esc(forensic_kind_cn(&hit.kind)),
                    esc(&forensic_pair_label(&hit.doc_a, &hit.doc_b)),
                    esc(forensic_level_cn(&hit.level)),
                    esc(&hit.detail),
                ],
            );
        }
        block(&mut out, "逐文档取证指纹", &["编号", "rsid 数", "模板", "血缘键"]);
        for d in &f.per_document {
            row(
                &mut out,
                &[
                    esc(&d.tag),
                    n(d.rsid_count),
                    esc(d.template_name.as_deref().unwrap_or("—")),
                    esc(&lineage_summary(&d.lineage)),
                ],
            );
        }
    }

    // 规避特征复核（附录 A evasion 节；仅列达判级线文档，无则整块省略）
    if let Some(ev) = &data.evasion {
        block(&mut out, "规避特征复核", &["编号", "判级", "证据种类"]);
        note(&mut out, EVASION_NOTE);
        for d in &ev.per_document {
            let kinds =
                if d.evidence_kinds.is_empty() { "—".to_string() } else { d.evidence_kinds.join("、") };
            row(
                &mut out,
                &[esc(&d.tag), esc(evasion_verdict_cn(&d.verdict)), esc(&kinds)],
            );
        }
    }

    // 检查方法与局限（附录 A methodsAndLimitations：§1.5 无条件常驻，堵沉默背书）
    let ml = &data.methods_and_limitations;
    block(&mut out, "检查方法与局限", &["类别", "内容"]);
    for c in &ml.checks_run {
        row(&mut out, &[esc("已执行检查项"), esc(c)]);
    }
    for d in &ml.disclaimers {
        row(&mut out, &[esc("局限与声明"), esc(d)]);
    }

    std::fs::write(path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::esc;

    #[test]
    fn esc_neutralizes_formula_injection() {
        for payload in ["=1+1", "+cmd", "-2", "@SUM(A1)", "\tx", "\rx"] {
            let out = esc(payload);
            assert!(out.starts_with("\"'"), "危险前导字符应被前置单引号中和：{payload:?} → {out}");
        }
        // 典型攻击载荷：不以裸 = 开头
        assert!(!esc("=HYPERLINK(\"http://x\",\"y\")").starts_with("\"="));
    }

    #[test]
    fn esc_preserves_normal_text_and_quote_escaping() {
        assert_eq!(esc("甲方应在十日内支付"), "\"甲方应在十日内支付\"");
        // 正常正文不加前缀；内部双引号仍转义
        assert_eq!(esc("a\"b"), "\"a\"\"b\"");
        // 中间出现的 = 不受影响（只看首字符）
        assert_eq!(esc("x=y"), "\"x=y\"");
    }
}
