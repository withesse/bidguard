// HTML 报告 v2（§14.2 全结构）：判定 → 文档 → 总览八类 → 矩阵 → 章节热力 →
// 事实冲突 → 条款明细（按风险排序，超限显式注明）→ 共有特征词 → 配置与版本附录。
// 自包含单文件，可「打印 → 另存为 PDF」。
use super::data::ExportData;
use super::shared::{
    band_cn_of, calibration_lines, calibration_note, contrib_label, field_cn, label, level_cn,
    review_cn, section_cn, severity_cn, strength_phrase, type_cn, xml_escape,
};
use std::fmt::Write as _;

const MAX_DETAIL_CLUSTERS: usize = 800;

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let e = xml_escape;
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>标书查重报告</title>");
    h.push_str("<style>body{font-family:-apple-system,'PingFang SC','Microsoft YaHei',sans-serif;color:#1a1a1a;max-width:960px;margin:32px auto;padding:0 24px;line-height:1.65}h1{font-size:24px}h2{font-size:17px;margin-top:30px;border-bottom:2px solid #4F58A8;padding-bottom:4px}table{border-collapse:collapse;width:100%;font-size:13px}th,td{border:1px solid #ddd;padding:6px 8px;text-align:center}th{background:#EEEFF9}.verdict{padding:12px 16px;border-radius:8px;font-weight:700;margin:12px 0}.high{background:#F7E4E4;color:#B54545}.medium{background:#F7EFE0;color:#C28430}.low{background:#EEEFF9;color:#4F58A8}.none{background:#E7F3EF;color:#0E9A8F}.seg{background:#f6f6f8;border:1px solid #eee;border-radius:6px;padding:8px 10px;margin:6px 0;font-size:13px}.muted{color:#888;font-size:12px}.tag{display:inline-block;background:#4F58A8;color:#fff;border-radius:3px;padding:0 5px;font-size:11px;margin-right:4px}.chip{display:inline-block;border-radius:999px;padding:2px 10px;font-size:12px;margin:2px 4px 2px 0;background:#EEEFF9;color:#4F58A8}.chip.red{background:#F7E4E4;color:#B54545}.conf{border:1px solid #ECC;background:#FDF6F6;border-radius:8px;padding:10px 14px;margin:8px 0}.cl{margin:14px 0 18px}.meta{font-size:12px;color:#666}@media print{body{margin:0}h2{break-after:avoid}.cl{break-inside:avoid}}</style></head><body>");
    h.push_str("<h1>原本 · 标书查重报告</h1>");
    let _ = write!(
        h,
        "<p class=\"muted\">任务：{} · 生成于 {} · 引擎 v{} · 全部在本地完成，未上传任何文件</p>",
        e(data.job_name.as_deref().unwrap_or("未命名比对")),
        &data.generated_at[..16].replace('T', " "),
        data.app_version
    );

    // 判定
    let col = &data.collusion;
    let _ = write!(
        h,
        "<div class=\"verdict {}\">综合判定：{}（证据强度：{}）</div>",
        e(&col.level),
        level_cn(&col.level),
        e(strength_phrase(&col.level))
    );
    if !col.signals.is_empty() {
        h.push_str("<ul>");
        for s in &col.signals {
            let _ = write!(h, "<li>{}（{}）</li>", e(&s.detail), e(&contrib_label(s.weight)));
        }
        h.push_str("</ul>");
    }
    let _ = write!(
        h,
        "<p class=\"muted\">{}</p>",
        e(&calibration_note(&col.calibration_kind, &col.calibration_version, data.app_version))
    );

    // 文档
    h.push_str("<h2>参评标书</h2><table><tr><th>编号</th><th>名称</th><th>类型</th><th>页数</th><th>字数</th><th>解析</th><th>元数据风险</th></tr>");
    for d in &data.documents {
        let flags = if d.risk_flags.is_empty() { "—".to_string() } else { d.risk_flags.join("；") };
        let _ = write!(
            h,
            "<tr><td>{}</td><td style=\"text-align:left\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td style=\"text-align:left\">{}</td></tr>",
            d.tag, e(&d.name), e(&d.file_type), d.pages, d.char_count,
            e(d.parse_method.as_deref().unwrap_or("—")), e(&flags)
        );
    }
    h.push_str("</table>");

    // 总览八类
    if let Some(s) = &data.summary {
        h.push_str("<h2>总览统计</h2><p>");
        let _ = write!(
            h,
            "<span class=\"chip\">{} 份文档</span><span class=\"chip\">{} 个分块</span><span class=\"chip\">{} 组条款</span><span class=\"chip\">峰值 {:.0}%</span>",
            s.document_count, s.chunk_count, s.cluster_count, data.peak * 100.0
        );
        for (k, v) in [
            ("conflict", s.conflict_count),
            ("same", s.same_count),
            ("minor_change", s.minor_change_count),
            ("changed", s.changed_count),
            ("rewrite", s.rewrite_count),
            ("uncertain", s.uncertain_count),
            ("added", s.added_count),
            ("deleted", s.deleted_count),
        ] {
            if v > 0 {
                let cls = if k == "conflict" { " red" } else { "" };
                let _ = write!(h, "<span class=\"chip{cls}\">{} {v}</span>", type_cn(k));
            }
        }
        h.push_str("</p>");
        if s.semantic_degraded {
            h.push_str("<p class=\"muted\">注：语义模型不可用，本次已降级为纯词面比对。</p>");
        }
    }

    // 矩阵
    h.push_str("<h2>相似度矩阵</h2><table><tr><th></th>");
    for d in &data.documents {
        let _ = write!(h, "<th>{}</th>", d.tag);
    }
    h.push_str("</tr>");
    for (i, row) in data.matrix.iter().enumerate() {
        let _ = write!(h, "<tr><th>{}</th>", data.documents.get(i).map(|d| d.tag.as_str()).unwrap_or("?"));
        for (j, v) in row.iter().enumerate() {
            let bg = if i != j && *v >= 0.8 { "#F7E4E4" } else if i != j && *v >= 0.6 { "#F7EFE0" } else { "#fff" };
            let cell = if i == j { "—".to_string() } else { format!("{:.0}%", v * 100.0) };
            let _ = write!(h, "<td style=\"background:{bg}\">{cell}</td>");
        }
        h.push_str("</tr>");
    }
    h.push_str("</table>");

    // 章节热力
    if !data.sections.is_empty() {
        let present: Vec<&str> = ["tech", "business", "other"]
            .into_iter()
            .filter(|s| data.sections.iter().any(|x| x.section == *s))
            .collect();
        h.push_str("<h2>章节热力</h2><table><tr><th>标书</th>");
        for s in &present {
            let _ = write!(h, "<th>{}</th>", section_cn(s));
        }
        h.push_str("</tr>");
        for (di, d) in data.documents.iter().enumerate() {
            let short: String = d.name.chars().take(6).collect();
            let _ = write!(h, "<tr><th>{} {}</th>", d.tag, e(&short));
            for s in &present {
                match data.sections.iter().find(|x| x.doc == di && &x.section == s) {
                    Some(st) => {
                        let _ = write!(h, "<td>{:.0}%</td>", st.intensity * 100.0);
                    }
                    None => h.push_str("<td>—</td>"),
                }
            }
            h.push_str("</tr>");
        }
        h.push_str("</table>");
    }

    // 事实冲突
    let conflicts: Vec<_> = data.clusters.iter().filter(|c| c.conflict.is_some()).collect();
    if !conflicts.is_empty() {
        let _ = write!(h, "<h2>事实冲突（{} 处）</h2>", conflicts.len());
        for c in &conflicts {
            h.push_str("<div class=\"conf\">");
            let _ = write!(
                h,
                "<b>#{} {}</b><span class=\"meta\">（{}）</span>",
                c.index,
                e(c.topic.as_deref().unwrap_or("")),
                severity_cn(c.severity.as_deref().unwrap_or("high"))
            );
            if let Some(cf) = &c.conflict {
                h.push_str("<ul style=\"margin:6px 0\">");
                for f in &cf.fields {
                    let vals: Vec<String> = f
                        .values
                        .iter()
                        .map(|v| format!("「{}」{}", label(v.doc), e(&v.value)))
                        .collect();
                    let _ = write!(h, "<li><b>{}</b>：{}</li>", field_cn(&f.field), vals.join(" vs "));
                }
                h.push_str("</ul>");
            }
            for m in &c.members {
                let _ = write!(h, "<div class=\"seg\"><span class=\"tag\">{}</span>{}</div>", m.tag, e(&m.text));
            }
            h.push_str("</div>");
        }
    }

    // 多家异常一致清单（W3-3）：≥3 家共有且招标文件与行业范本库均查不到出处的段落。
    // §1.5：强制「涉嫌」措辞 +「需评标委员会依法认定」脚注，独立「待复核」，不并入高风险统计。
    let anomalies: Vec<_> = data.clusters.iter().filter(|c| c.multi_doc_anomaly).collect();
    if !anomalies.is_empty() {
        let _ = write!(h, "<h2>多家异常一致清单（{} 处·待复核）</h2>", anomalies.len());
        h.push_str(
            "<p class=\"muted\">下列段落在 3 家及以上投标间高度雷同，且招标文件与行业范本库均未查得出处，\
             涉嫌《招标投标法实施条例》第四十条『投标文件异常一致』情形。此为线索级提示、非定性结论，\
             未自动判为高风险，需评标委员会结合原文依法认定，未命中不代表清白。</p>",
        );
        for c in &anomalies {
            h.push_str("<div class=\"conf\">");
            let _ = write!(
                h,
                "<b>#{} {}</b><span class=\"meta\">（涉嫌多家异常一致·待复核）</span>",
                c.index,
                e(c.topic.as_deref().unwrap_or(""))
            );
            for m in &c.members {
                let _ = write!(h, "<div class=\"seg\"><span class=\"tag\">{}</span>{}</div>", m.tag, e(&m.text));
            }
            h.push_str("</div>");
        }
    }

    // 复核路由三带（W6-4）：恒常驻小节，说明条款按什么口径排队 + §1.5 强制措辞。
    h.push_str("<h2>复核路由（三带）</h2>");
    for line in calibration_lines(&data.calibration) {
        let _ = write!(h, "<p class=\"muted\">{}</p>", e(&line));
    }

    // 条款明细
    let shown = data.clusters.len().min(MAX_DETAIL_CLUSTERS);
    let _ = write!(h, "<h2>雷同条款明细（{} 组）</h2>", data.clusters.len());
    if data.clusters.len() > MAX_DETAIL_CLUSTERS {
        let _ = write!(
            h,
            "<p class=\"muted\">仅展示前 {MAX_DETAIL_CLUSTERS} 组（按风险与相似度排序）；完整数据请使用 JSON / CSV 导出。</p>"
        );
    }
    for c in &data.clusters[..shown] {
        h.push_str("<div class=\"cl\">");
        let docs: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for m in &c.members {
                if !seen.contains(&m.tag.as_str()) {
                    seen.push(&m.tag);
                }
            }
            seen
        };
        let _ = write!(
            h,
            "<p><b>#{} [{}{}] {} · 相似 {:.0}% · 涉及 {} · {} · 复核路由：{}</b></p>",
            c.index,
            type_cn(&c.cluster_type),
            c.severity.as_deref().map(|s| format!("·{}", severity_cn(s))).unwrap_or_default(),
            e(c.topic.as_deref().unwrap_or("")),
            c.score.unwrap_or(0.0) * 100.0,
            docs.join("·"),
            review_cn(&c.review_status),
            band_cn_of(c.band.as_deref())
        );
        // k-共现查证标记（W3-3）：豁免簇标注合法共享出处、异常簇标注待复核。
        match c.exempt_reason.as_deref() {
            Some("tender") => h.push_str("<p class=\"muted\">· 已核为引用招标文件的合法共享，不计入风险统计</p>"),
            Some("background") => h.push_str("<p class=\"muted\">· 已核为行业范本套话的合法共享，不计入风险统计</p>"),
            _ if c.multi_doc_anomaly => h.push_str(
                "<p class=\"muted\">· 涉嫌多家异常一致（招标/范本库均无出处）·待复核，需评标委员会依法认定</p>",
            ),
            _ => {}
        }
        // 分区标注（§5 W3-5）：标注条款所属五区；legal 区附阈值上调口径、price 区附证据主体说明。
        if let Some(sk) = c.section_kind.as_deref() {
            let note = match sk {
                "legal" => "（法定格式文本，阈值已上调，仅压套话雷同）",
                "price" => "（证据主体为金额事实冲突，非文字雷同）",
                _ => "",
            };
            let _ = write!(h, "<p class=\"muted\">· 分区：{}{}</p>", section_cn(sk), note);
        }
        for m in &c.members {
            let page = m.page.map(|p| format!("<span class=\"meta\">（第 {p} 页）</span>")).unwrap_or_default();
            let _ = write!(h, "<div class=\"seg\"><span class=\"tag\">{}</span>{}{}</div>", m.tag, e(&m.text), page);
        }
        h.push_str("</div>");
    }

    // 共有特征词
    if !data.shared_terms.is_empty() {
        h.push_str("<h2>共有特征词</h2><p>");
        for t in data.shared_terms.iter().take(40) {
            let docs: Vec<&str> = t.docs.iter().map(|&d| label(d)).collect();
            let _ = write!(
                h,
                "<span class=\"seg\" style=\"display:inline-block;margin:3px\">{} <span class=\"muted\">[{}]</span></span> ",
                e(&t.term),
                docs.join("")
            );
        }
        h.push_str("</p>");
    }

    // 对齐区段与逐字证据（附录 A segments 节；无区段/逐字不渲染——§1.5 屏幕可见证据须在正式报告可引用）
    if let Some(seg) = &data.segments {
        h.push_str("<h2>对齐区段与逐字证据</h2>");
        h.push_str(
            "<p class=\"muted\">对齐区段是与聚类并存的独立证据层：按 chunk 去重后的真实覆盖，\
             与区段视图/矩阵区段口径同源。三级视觉语义——\
             <span style=\"background:#F7D4D4;color:#8B2E2E;padding:0 5px;border-radius:3px\">深红＝逐字铁证（去空白一字不差）</span> \
             <span style=\"background:#F6DFC6;color:#9A5B18;padding:0 5px;border-radius:3px\">橙＝锚点雷同</span> \
             <span style=\"background:#F5EEC2;color:#7E6E12;padding:0 5px;border-radius:3px\">黄＝gap 细化差异</span>。\
             标注「引用招标文件」的区段/区间落在招标豁免块，系对同一招标条款的合法应答，非串通证据；\
             未命中不构成清白证明。</p>",
        );
        for p in &seg.pairs {
            let _ =
                write!(h, "<h3 style=\"font-size:15px;margin-top:22px\">{} × {}</h3>", e(&p.a), e(&p.b));
            // 区段摘要表
            if p.segments.is_empty() {
                h.push_str("<p class=\"muted\">无对齐区段（仅逐字铁证，见下）。</p>");
            } else {
                let _ = write!(
                    h,
                    "<p class=\"muted\">对齐区段摘要（{} 段，按逐字字数排序）</p>",
                    p.segments.len()
                );
                let _ = write!(
                    h,
                    "<table><tr><th>{} 侧定位</th><th>{} 侧定位</th><th>覆盖</th><th>锚点</th><th>逐字字数</th><th>标注</th></tr>",
                    e(&p.a),
                    e(&p.b)
                );
                for s in &p.segments {
                    let badge = if s.tender_quote {
                        "<span class=\"chip\">引用招标文件</span>"
                    } else {
                        "—"
                    };
                    let _ = write!(
                        h,
                        "<tr><td style=\"text-align:left\">{}</td><td style=\"text-align:left\">{}</td><td>{:.0}%</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        e(&s.a_range),
                        e(&s.b_range),
                        s.coverage * 100.0,
                        s.anchor_count,
                        s.verbatim_chars,
                        badge
                    );
                }
                h.push_str("</table>");
            }
            // 逐字雷同区间清单（深红铁证 + 双侧页码）
            if !p.verbatims.is_empty() {
                let _ = write!(
                    h,
                    "<p class=\"muted\">逐字雷同区间清单（{} 处 · 深红铁证 · 含双侧页码）</p>",
                    p.verbatims.len()
                );
                let _ = write!(
                    h,
                    "<table><tr><th>{} 侧页码/章节</th><th>{} 侧页码/章节</th><th>字数</th><th>逐字样本</th><th>标注</th></tr>",
                    e(&p.a),
                    e(&p.b)
                );
                for v in &p.verbatims {
                    let badge = if v.tender_quote {
                        "<span class=\"chip\">引用招标文件</span>"
                    } else {
                        "—"
                    };
                    let _ = write!(
                        h,
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td style=\"text-align:left;background:#F7D4D4;color:#8B2E2E\">{}</td><td>{}</td></tr>",
                        e(&verbatim_locator(v.a_page, v.a_section.as_deref())),
                        e(&verbatim_locator(v.b_page, v.b_section.as_deref())),
                        v.char_len,
                        e(&v.sample),
                        badge
                    );
                }
                h.push_str("</table>");
            }
        }
    }

    // 商务标数值证据（附录 A numeric 节；无清单数据不渲染——§1.5 不留空表沉默背书）
    if let Some(nm) = &data.numeric {
        h.push_str("<h2>商务标数值证据</h2>");
        let _ = write!(
            h,
            "<p class=\"muted\">报价清单逐项比对：共识别清单条目 {} 条、跨文档对齐 {} 条；\
             雷同率告警线 {:.0}%，可比条目不足 {} 项的文档对不出结论。</p>",
            nm.item_count, nm.aligned_item_count, nm.identical_rate_alarm * 100.0, nm.min_comparable
        );
        for note in &nm.notes {
            let _ = write!(h, "<p class=\"muted\">{}</p>", e(note));
        }
        // 逐项单价雷同率表
        h.push_str(
            "<table><tr><th>文档对</th><th>可比条目</th><th>单价相同</th><th>逐项雷同率</th><th>告警</th></tr>",
        );
        for p in &nm.pairs {
            let rate = match p.identical_rate {
                Some(r) => format!("{:.1}%", r * 100.0),
                None => format!("—（{}）", numeric_reason_cn(p.reason.as_deref())),
            };
            let flag = if p.alarm {
                "<span class=\"chip\">达告警线 · 需重点核查</span>"
            } else {
                "—"
            };
            let _ = write!(
                h,
                "<tr><td>{} ↔ {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                e(&p.a), e(&p.b), p.comparable, p.identical, rate, flag
            );
        }
        h.push_str("</table>");
        // 规律性差异 / 相关性结论（逐对，仅列已出结论者）
        let has_stat = nm.pairs.iter().any(|p| p.pattern.is_some() || p.correlation.is_some());
        if has_stat {
            h.push_str("<p style=\"margin-top:14px\"><b>规律性差异与相关性</b></p>");
            h.push_str("<table><tr><th>文档对</th><th>规律性</th><th>相关性</th></tr>");
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
                let _ = write!(
                    h,
                    "<tr><td>{} ↔ {}</td><td style=\"text-align:left\">{}</td><td style=\"text-align:left\">{}</td></tr>",
                    e(&p.a), e(&p.b), e(&pat), e(&cor)
                );
            }
            h.push_str("</table>");
            // §1.5 强制文案（随数据下发，去重后原样引用）：规律性只是线索、相关性须与比值 CV 同屏
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
                let _ = write!(h, "<p class=\"muted\">{}</p>", e(n));
            }
        }
        // 共享算术错误清单（逐条 + §1.5 人工核对提示）
        let errs: Vec<(&str, &str, &crate::export::data::NumericArithError)> = nm
            .pairs
            .iter()
            .flat_map(|p| p.shared_arith_errors.iter().map(move |x| (p.a.as_str(), p.b.as_str(), x)))
            .collect();
        if !errs.is_empty() {
            let _ = write!(h, "<p style=\"margin-top:14px\"><b>共享算术错误清单（{} 条）</b></p>", errs.len());
            h.push_str(
                "<p class=\"muted\">同一清单项在两份文件中工程量、综合单价与（算错的）合价三者到分全等。\
                 检测已排除可由常见舍入规则解释的差值；请核对是否源自同一计价软件舍入惯例或招标文件，\
                 单条命中不构成串通投标认定。</p>",
            );
            h.push_str(
                "<table><tr><th>文档对</th><th>清单项</th><th>工程量</th><th>综合单价</th><th>报出合价</th><th>应为</th><th>原文锚点</th></tr>",
            );
            for (a, b, x) in errs {
                let _ = write!(
                    h,
                    "<tr><td>{} ↔ {}</td><td style=\"text-align:left\">{}</td><td>{}</td><td>{:.2}</td><td style=\"background:#F7D4D4;color:#8B2E2E\">{:.2}</td><td>{:.2}</td><td class=\"muted\" style=\"text-align:left;font-size:11px\">{}</td></tr>",
                    e(a),
                    e(b),
                    e(&numeric_item_label(x.name.as_deref(), &x.align_key)),
                    x.qty,
                    x.unit_price,
                    x.total,
                    x.expected_total,
                    e(&x.chunk_ids.join(" / "))
                );
            }
            h.push_str("</table>");
        }
        // 逐文档单价尾数分布（Benford 首位检验已砍，仅分位/角位均匀性 + 0/5 尾占比）
        if nm.docs.iter().any(|d| d.digit_stats.is_some()) {
            h.push_str("<p style=\"margin-top:14px\"><b>逐文档单价尾数分布</b></p>");
            h.push_str(
                "<table><tr><th>编号</th><th>样本</th><th>分位 χ²</th><th>角位 χ²</th><th>临界值</th><th>0/5 尾占比</th><th>结论</th></tr>",
            );
            for d in &nm.docs {
                let Some(ds) = &d.digit_stats else { continue };
                let g = |k: &str| ds.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let clustered =
                    ds.get("clustered").and_then(serde_json::Value::as_bool).unwrap_or(false);
                let _ = write!(
                    h,
                    "<tr><td>{}</td><td>{}</td><td>{:.2}</td><td>{:.2}</td><td>{:.3}</td><td>{:.0}%</td><td>{}</td></tr>",
                    d.tag,
                    g("n") as i64,
                    g("centChiSquare"),
                    g("jiaoChiSquare"),
                    g("critical"),
                    g("zeroFiveRatio") * 100.0,
                    if clustered { "尾数聚集" } else { "未见聚集" }
                );
            }
            h.push_str("</table>");
            h.push_str(
                "<p class=\"muted\">尾数聚集反映报价的取整习惯（如统一取整到角/元），单独不构成串通认定，\
                 需结合取证类证据；本工具未做 Benford 首位检验（单价通常只跨 2–3 个数量级，前提不成立）。</p>",
            );
        }
    }

    // 取证证据（附录 A forensic 节；无命中不渲染——§1.5 不留空表沉默背书）
    if let Some(f) = &data.forensic {
        h.push_str("<h2>取证证据</h2>");
        h.push_str("<p class=\"muted\">取证信号为线索级同源证据（rsid / PDF 血缘 / 图片同源 / 共同错误）。请核对是否源自招标方统一模板或同一代理机构；未命中不构成清白证明（另存为 / 元数据清洗可消除痕迹）。</p>");
        h.push_str("<table><tr><th>类型</th><th>文档</th><th>强度</th><th>说明</th></tr>");
        for hit in &f.hits {
            let pair = if hit.doc_a.is_empty() && hit.doc_b.is_empty() {
                "见说明".to_string()
            } else {
                format!("{} ↔ {}", e(&hit.doc_a), e(&hit.doc_b))
            };
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td style=\"text-align:left\">{}</td></tr>",
                forensic_kind_cn(&hit.kind),
                pair,
                forensic_level_cn(&hit.level),
                e(&hit.detail)
            );
        }
        h.push_str("</table>");
        h.push_str("<p style=\"margin-top:14px\"><b>逐文档取证指纹</b></p>");
        h.push_str("<table><tr><th>编号</th><th>rsid 数</th><th>模板</th><th>血缘键</th></tr>");
        for d in &f.per_document {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td style=\"text-align:left\">{}</td><td style=\"text-align:left\">{}</td></tr>",
                d.tag,
                d.rsid_count,
                e(d.template_name.as_deref().unwrap_or("—")),
                e(&lineage_summary(&d.lineage))
            );
        }
        h.push_str("</table>");
    }

    // 规避特征复核（附录 A evasion 节；仅列达判级线文档，无则不渲染）
    if let Some(ev) = &data.evasion {
        h.push_str("<h2>规避特征复核</h2>");
        h.push_str("<p class=\"muted\">检测到疑似规避特征，请人工复核；本工具不作「规避 / 串通」定性结论。未命中不构成清白证明。</p>");
        h.push_str("<table><tr><th>编号</th><th>判级</th><th>证据种类</th></tr>");
        for d in &ev.per_document {
            let kinds = if d.evidence_kinds.is_empty() { "—".to_string() } else { d.evidence_kinds.join("、") };
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td style=\"text-align:left\">{}</td></tr>",
                d.tag,
                evasion_verdict_cn(&d.verdict),
                e(&kinds)
            );
        }
        h.push_str("</table>");
    }

    // 检查方法与局限（附录 A methodsAndLimitations：§1.5 无条件常驻，堵沉默背书）
    let ml = &data.methods_and_limitations;
    h.push_str("<h2>检查方法与局限</h2>");
    h.push_str("<p><b>本次已执行的取证 / 对抗检查项：</b></p><ul>");
    for c in &ml.checks_run {
        let _ = write!(h, "<li>{}</li>", e(c));
    }
    h.push_str("</ul><p><b>局限与声明：</b></p><ul>");
    for d in &ml.disclaimers {
        let _ = write!(h, "<li>{}</li>", e(d));
    }
    h.push_str("</ul>");

    // 附录
    h.push_str("<h2>附录：比对配置与版本</h2>");
    let _ = write!(
        h,
        "<pre style=\"background:#f6f6f8;border-radius:6px;padding:10px;font-size:12px;overflow:auto\">{}</pre>",
        e(&serde_json::to_string_pretty(&data.config).unwrap_or_default())
    );
    let _ = write!(
        h,
        "<p class=\"muted\">报告格式 {} · 引擎 v{} · 由「原本 · 标书查重」本地生成。可使用浏览器「打印 → 另存为 PDF」导出 PDF。</p>",
        data.report_version, data.app_version
    );
    h.push_str("</body></html>");
    std::fs::write(path, h).map_err(|e| e.to_string())
}

/// 规律性差异形态的中文标签（numeric.pairs[].pattern.kind）。
fn numeric_pattern_cn(kind: &str) -> &str {
    match kind {
        "arith_seq" => "等差（各项差额恒定）",
        "geo_discount" => "等比 / 恒定折扣（各项系数恒定）",
        "affine" => "仿射（系数与差额均非平凡）",
        other => other,
    }
}

/// 雷同率缺席原因的中文标签（numeric.pairs[].reason）。
fn numeric_reason_cn(reason: Option<&str>) -> &str {
    match reason {
        Some("insufficient") => "可比条目不足，不出结论",
        Some(other) => other,
        None => "无可比条目",
    }
}

/// 清单项可读标签：优先项目名称，回落对齐键（编码/名称+单位）。
fn numeric_item_label(name: Option<&str>, align_key: &str) -> String {
    match name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => format!("{n}（{align_key}）"),
        None => align_key.to_string(),
    }
}

/// 取证命中类型中文标签（forensic.hits[].kind）。
fn forensic_kind_cn(kind: &str) -> &str {
    match kind {
        "rsid" => "docx 修订标识（rsid）",
        "pdfLineage" => "PDF 血缘",
        "imageReuse" => "内嵌图片同源",
        "sharedErrors" => "共同错误指纹",
        other => other,
    }
}

/// 取证命中强度中文标签（forensic.hits[].level）。
fn forensic_level_cn(level: &str) -> &str {
    match level {
        "hard" => "硬命中",
        "mid" => "中命中",
        "weak" => "弱命中",
        other => other,
    }
}

/// 规避判级中文标签（evasion.perDocument[].verdict）——§1.5：措辞不下定性结论。
fn evasion_verdict_cn(verdict: &str) -> &str {
    match verdict {
        "confirmed" => "需人工复核",
        "suspect" => "疑似（弱信号）",
        other => other,
    }
}

/// 逐字区间一侧定位串（页码 + 章节路径 → 单行；调用方再 xml_escape）。
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

/// 逐文档血缘键摘要（forensic.perDocument[].lineage → 单行可读串）。
fn lineage_summary(lineage: &serde_json::Value) -> String {
    let get = |k: &str| lineage.get(k).and_then(serde_json::Value::as_str).filter(|s| !s.is_empty());
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = get("documentId") {
        parts.push(format!("GUID {v}"));
    }
    if let Some(v) = get("idFirst") {
        parts.push(format!("trailer ID {v}"));
    }
    if let Some(v) = get("derivedFrom") {
        parts.push(format!("派生自 {v}"));
    }
    let tags = lineage
        .get("fontSubsetTags")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    if tags > 0 {
        parts.push(format!("字体子集 {tags} 个"));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join("；")
    }
}
