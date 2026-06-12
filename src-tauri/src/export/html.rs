// HTML 报告 v2（§14.2 全结构）：判定 → 文档 → 总览八类 → 矩阵 → 章节热力 →
// 事实冲突 → 条款明细（按风险排序，超限显式注明）→ 共有特征词 → 配置与版本附录。
// 自包含单文件，可「打印 → 另存为 PDF」。
use super::data::ExportData;
use super::shared::{field_cn, label, level_cn, review_cn, section_cn, severity_cn, type_cn, xml_escape};
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
        "<div class=\"verdict {}\">综合判定：{}（评分 {:.0}%）</div>",
        e(&col.level),
        level_cn(&col.level),
        col.score * 100.0
    );
    if !col.signals.is_empty() {
        h.push_str("<ul>");
        for s in &col.signals {
            let _ = write!(h, "<li>{}（权重 {:.0}%）</li>", e(&s.detail), s.weight * 100.0);
        }
        h.push_str("</ul>");
    }

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
            "<p><b>#{} [{}{}] {} · 相似 {:.0}% · 涉及 {} · {}</b></p>",
            c.index,
            type_cn(&c.cluster_type),
            c.severity.as_deref().map(|s| format!("·{}", severity_cn(s))).unwrap_or_default(),
            e(c.topic.as_deref().unwrap_or("")),
            c.score.unwrap_or(0.0) * 100.0,
            docs.join("·"),
            review_cn(&c.review_status)
        );
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
