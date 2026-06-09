// 导出 Excel 报告：相似度矩阵 / 逐对明细 / 雷同条款 三个工作表。
use crate::engine::report::Report;
use rust_xlsxwriter::{Format, Workbook};

const LABELS: [&str; 5] = ["甲", "乙", "丙", "丁", "戊"];
fn label(i: usize) -> &'static str {
    LABELS.get(i).copied().unwrap_or("?")
}

pub fn to_xlsx(report: &Report, path: &str) -> Result<(), String> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_background_color(0xEEEFF9);
    let pctf = Format::new().set_num_format("0%");

    // ── 工作表 1：相似度矩阵 ──
    {
        let s = wb.add_worksheet();
        s.set_name("相似度矩阵").map_err(|e| e.to_string())?;
        s.write_string(0, 0, "原本 · 标书查重 · 相似度矩阵")
            .map_err(|e| e.to_string())?;
        let base = 2u32;
        for (j, d) in report.docs.iter().enumerate() {
            let title = format!("{} {}", label(j), d.name);
            s.write_string_with_format(base, (j + 1) as u16, &title, &head)
                .map_err(|e| e.to_string())?;
            s.write_string_with_format(base + 1 + j as u32, 0, &title, &head)
                .map_err(|e| e.to_string())?;
        }
        for (i, row) in report.matrix.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                s.write_number_with_format(base + 1 + i as u32, (j + 1) as u16, *v as f64, &pctf)
                    .map_err(|e| e.to_string())?;
            }
        }
        s.write_string(base + 2 + report.docs.len() as u32, 0, "峰值相似度")
            .map_err(|e| e.to_string())?;
        s.write_number_with_format(base + 2 + report.docs.len() as u32, 1, report.peak as f64, &pctf)
            .map_err(|e| e.to_string())?;
    }

    // ── 工作表 2：逐对明细 ──
    {
        let s = wb.add_worksheet();
        s.set_name("逐对明细").map_err(|e| e.to_string())?;
        for (c, h) in ["组合", "相似度", "甲方段落", "乙方段落"].iter().enumerate() {
            s.write_string_with_format(0, c as u16, *h, &bold)
                .map_err(|e| e.to_string())?;
        }
        let mut r = 1u32;
        for p in &report.pairs {
            for m in &p.matches {
                s.write_string(r, 0, &format!("{} × {}", label(p.a), label(p.b)))
                    .map_err(|e| e.to_string())?;
                s.write_number_with_format(r, 1, m.score as f64, &pctf)
                    .map_err(|e| e.to_string())?;
                s.write_string(r, 2, &m.text_a).map_err(|e| e.to_string())?;
                s.write_string(r, 3, &m.text_b).map_err(|e| e.to_string())?;
                r += 1;
            }
        }
    }

    // ── 工作表 3：雷同条款聚合 ──
    {
        let s = wb.add_worksheet();
        s.set_name("雷同条款").map_err(|e| e.to_string())?;
        for (c, h) in ["聚合#", "涉及文档", "组内平均", "段落（按文档）"].iter().enumerate() {
            s.write_string_with_format(0, c as u16, *h, &bold)
                .map_err(|e| e.to_string())?;
        }
        let mut r = 1u32;
        for (i, cl) in report.clusters.iter().enumerate() {
            let docs: Vec<&str> = cl.docs.iter().map(|&d| label(d)).collect();
            for seg in &cl.segments {
                s.write_number(r, 0, (i + 1) as f64).map_err(|e| e.to_string())?;
                s.write_string(r, 1, &docs.join("·")).map_err(|e| e.to_string())?;
                s.write_number_with_format(r, 2, cl.avg_score as f64, &pctf)
                    .map_err(|e| e.to_string())?;
                s.write_string(r, 3, &format!("{}：{}", label(seg.doc), seg.text))
                    .map_err(|e| e.to_string())?;
                r += 1;
            }
        }
    }

    wb.save(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn level_cn(l: &str) -> &'static str {
    match l {
        "high" => "围标嫌疑（高）",
        "medium" => "重点复核（中）",
        "low" => "轻度雷同（低）",
        _ => "未见明显围标",
    }
}

fn section_cn(s: &str) -> &'static str {
    match s {
        "tech" => "技术标",
        "business" => "商务标",
        _ => "其他",
    }
}

/// 导出自包含 HTML 报告（中文原生渲染；可在浏览器「打印 → 另存为 PDF」得到 PDF）。
pub fn to_html(report: &Report, path: &str) -> Result<(), String> {
    let e = xml_escape;
    let col = &report.collusion;
    let n = report.docs.len();
    let pairs = n * n.saturating_sub(1) / 2;
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>标书查重报告</title>");
    h.push_str("<style>body{font-family:-apple-system,'PingFang SC','Microsoft YaHei',sans-serif;color:#1a1a1a;max-width:920px;margin:32px auto;padding:0 24px;line-height:1.6}h1{font-size:24px}h2{font-size:17px;margin-top:28px;border-bottom:2px solid #4F58A8;padding-bottom:4px}table{border-collapse:collapse;width:100%;font-size:13px}th,td{border:1px solid #ddd;padding:6px 8px;text-align:center}th{background:#EEEFF9}.verdict{padding:12px 16px;border-radius:8px;font-weight:700;margin:12px 0}.high{background:#F7E4E4;color:#B54545}.medium{background:#F7EFE0;color:#C28430}.low{background:#EEEFF9;color:#4F58A8}.none{background:#E7F3EF;color:#0E9A8F}.seg{background:#f6f6f8;border:1px solid #eee;border-radius:6px;padding:8px 10px;margin:6px 0;font-size:13px}.muted{color:#888;font-size:12px}.tag{display:inline-block;background:#4F58A8;color:#fff;border-radius:3px;padding:0 5px;font-size:11px;margin-right:4px}@media print{body{margin:0}h2{break-after:avoid}}</style></head><body>");
    h.push_str("<h1>原本 · 标书查重报告</h1>");
    h.push_str(&format!(
        "<div class=\"verdict {}\">综合判定：{}（评分 {:.0}%）</div>",
        e(&col.level),
        level_cn(&col.level),
        col.score * 100.0
    ));
    if !col.signals.is_empty() {
        h.push_str("<ul>");
        for s in &col.signals {
            h.push_str(&format!("<li>{}（权重 {:.0}%）</li>", e(&s.detail), s.weight * 100.0));
        }
        h.push_str("</ul>");
    }
    h.push_str(&format!(
        "<p class=\"muted\">参评 {n} 份标书 · {pairs} 对比对 · 峰值相似度 {:.0}% · 全部在本地完成，未上传任何文件。</p>",
        report.peak * 100.0
    ));

    h.push_str("<h2>参评标书</h2><table><tr><th>编号</th><th>名称</th><th>类型</th><th>页数</th><th>元数据风险</th></tr>");
    for (i, d) in report.docs.iter().enumerate() {
        let flags = if d.fingerprint.risk_flags.is_empty() {
            "—".to_string()
        } else {
            d.fingerprint.risk_flags.join("；")
        };
        let nm = d
            .parse_error
            .as_ref()
            .map(|er| format!("{}（解析失败：{er}）", d.name))
            .unwrap_or_else(|| d.name.clone());
        h.push_str(&format!(
            "<tr><td>{}</td><td style=\"text-align:left\">{}</td><td>{}</td><td>{}</td><td style=\"text-align:left\">{}</td></tr>",
            label(i), e(&nm), e(&d.doc_type), d.pages, e(&flags)
        ));
    }
    h.push_str("</table>");

    h.push_str("<h2>相似度矩阵</h2><table><tr><th></th>");
    for i in 0..n {
        h.push_str(&format!("<th>{}</th>", label(i)));
    }
    h.push_str("</tr>");
    for (i, row) in report.matrix.iter().enumerate() {
        h.push_str(&format!("<tr><th>{}</th>", label(i)));
        for (j, v) in row.iter().enumerate() {
            let bg = if i != j && *v >= 0.8 {
                "#F7E4E4"
            } else if i != j && *v >= 0.6 {
                "#F7EFE0"
            } else {
                "#fff"
            };
            let cell = if i == j {
                "—".to_string()
            } else {
                format!("{:.0}%", v * 100.0)
            };
            h.push_str(&format!("<td style=\"background:{bg}\">{cell}</td>"));
        }
        h.push_str("</tr>");
    }
    h.push_str("</table>");

    if !report.sections.is_empty() {
        let present: Vec<&str> = ["tech", "business", "other"]
            .into_iter()
            .filter(|s| report.sections.iter().any(|x| x.section == *s))
            .collect();
        h.push_str("<h2>章节热力</h2><table><tr><th>标书</th>");
        for s in &present {
            h.push_str(&format!("<th>{}</th>", section_cn(s)));
        }
        h.push_str("</tr>");
        for (di, d) in report.docs.iter().enumerate() {
            let short: String = d.name.chars().take(6).collect();
            h.push_str(&format!("<tr><th>{} {}</th>", label(di), e(&short)));
            for s in &present {
                match report.sections.iter().find(|x| x.doc == di && &x.section == s) {
                    Some(st) => h.push_str(&format!("<td>{:.0}%</td>", st.intensity * 100.0)),
                    None => h.push_str("<td>—</td>"),
                }
            }
            h.push_str("</tr>");
        }
        h.push_str("</table>");
    }

    h.push_str(&format!("<h2>雷同条款（{} 组）</h2>", report.clusters.len()));
    for (i, cl) in report.clusters.iter().enumerate() {
        let docs: Vec<&str> = cl.docs.iter().map(|&d| label(d)).collect();
        h.push_str(&format!(
            "<p><b>聚合 #{} · 平均 {:.0}% / 峰值 {:.0}% · 涉及 {}</b></p>",
            i + 1,
            cl.avg_score * 100.0,
            cl.peak * 100.0,
            docs.join("·")
        ));
        for seg in &cl.segments {
            h.push_str(&format!(
                "<div class=\"seg\"><span class=\"tag\">{}</span>{}</div>",
                label(seg.doc),
                e(&seg.text)
            ));
        }
    }

    if !report.shared_terms.is_empty() {
        h.push_str("<h2>共有特征词</h2><p>");
        for t in report.shared_terms.iter().take(40) {
            let docs: Vec<&str> = t.docs.iter().map(|&d| label(d)).collect();
            h.push_str(&format!(
                "<span class=\"seg\" style=\"display:inline-block;margin:3px\">{} <span class=\"muted\">[{}]</span></span> ",
                e(&t.term),
                docs.join("")
            ));
        }
        h.push_str("</p>");
    }

    h.push_str("<p class=\"muted\" style=\"margin-top:30px\">本报告由「原本 · 标书查重」本地生成。可使用浏览器「打印 → 另存为 PDF」导出 PDF。</p>");
    h.push_str("</body></html>");
    std::fs::write(path, h).map_err(|e| e.to_string())?;
    Ok(())
}

fn docx_p(out: &mut String, text: &str, bold: bool, size: u32) {
    let mut rpr = String::new();
    if bold || size > 0 {
        rpr.push_str("<w:rPr>");
        if bold {
            rpr.push_str("<w:b/>");
        }
        if size > 0 {
            rpr.push_str(&format!("<w:sz w:val=\"{size}\"/>"));
        }
        rpr.push_str("</w:rPr>");
    }
    out.push_str(&format!(
        "<w:p><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    ));
}

const DOCX_CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
const DOCX_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";

/// 导出 Word(.docx)：最小合法 OOXML 包（zip + document.xml），Word/WPS/Pages 原生渲染中文，无需内嵌字体。
pub fn to_docx(report: &Report, path: &str) -> Result<(), String> {
    use std::io::Write;
    let col = &report.collusion;
    let n = report.docs.len();
    let mut body = String::new();
    docx_p(&mut body, "原本 · 标书查重报告", true, 36);
    docx_p(
        &mut body,
        &format!("综合判定：{}（评分 {:.0}%）", level_cn(&col.level), col.score * 100.0),
        true,
        26,
    );
    for s in &col.signals {
        docx_p(&mut body, &format!("· {}（权重 {:.0}%）", s.detail, s.weight * 100.0), false, 21);
    }
    docx_p(
        &mut body,
        &format!(
            "参评 {n} 份标书，{} 对比对，峰值相似度 {:.0}%。全部在本地完成。",
            n * n.saturating_sub(1) / 2,
            report.peak * 100.0
        ),
        false,
        21,
    );

    docx_p(&mut body, "参评标书", true, 28);
    for (i, d) in report.docs.iter().enumerate() {
        let flags = if d.fingerprint.risk_flags.is_empty() {
            String::new()
        } else {
            format!("（{}）", d.fingerprint.risk_flags.join("；"))
        };
        docx_p(
            &mut body,
            &format!("{} {} · {} · {} 页{flags}", label(i), d.name, d.doc_type, d.pages),
            false,
            21,
        );
    }

    docx_p(&mut body, "相似度（两两）", true, 28);
    for pr in &report.pairs {
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

    docx_p(&mut body, &format!("雷同条款（{} 组）", report.clusters.len()), true, 28);
    for (i, cl) in report.clusters.iter().enumerate() {
        let docs: Vec<&str> = cl.docs.iter().map(|&d| label(d)).collect();
        docx_p(
            &mut body,
            &format!("聚合 #{} · 平均 {:.0}% · 涉及 {}", i + 1, cl.avg_score * 100.0, docs.join("·")),
            true,
            22,
        );
        for seg in &cl.segments {
            docx_p(&mut body, &format!("　{}：{}", label(seg.doc), seg.text), false, 21);
        }
    }

    if !report.shared_terms.is_empty() {
        docx_p(&mut body, "共有特征词", true, 28);
        let terms: Vec<&str> = report.shared_terms.iter().take(40).map(|t| t.term.as_str()).collect();
        docx_p(&mut body, &terms.join("、"), false, 21);
    }

    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr/></w:body></w:document>"
    );

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zipw = zip::ZipWriter::new(file);
    let opt =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in [
        ("[Content_Types].xml", DOCX_CONTENT_TYPES),
        ("_rels/.rels", DOCX_RELS),
        ("word/document.xml", doc.as_str()),
    ] {
        zipw.start_file(name, opt).map_err(|e| e.to_string())?;
        zipw.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    }
    zipw.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::report::{Cluster, ClusterSeg, DiffOp, DocInfo, Fingerprint, PairDetail, SegMatch};

    #[test]
    fn exports_valid_xlsx() {
        let doc = |id: &str, name: &str| DocInfo {
            id: id.into(),
            name: name.into(),
            doc_type: "docx".into(),
            pages: 1,
            char_count: 10,
            fingerprint: Fingerprint::default(),
            parse_error: None,
        };
        let report = Report {
            docs: vec![doc("d0", "甲.docx"), doc("d1", "乙.docx")],
            matrix: vec![vec![1.0, 0.8], vec![0.8, 1.0]],
            peak: 0.8,
            pairs: vec![PairDetail {
                a: 0,
                b: 1,
                score: 0.8,
                matches: vec![SegMatch {
                    text_a: "总体架构分层解耦".into(),
                    text_b: "总体架构分层解藕".into(),
                    score: 0.9,
                    diff: vec![DiffOp { op: "eq".into(), text: "总体架构分层解".into() }],
                }],
            }],
            clusters: vec![Cluster {
                avg_score: 0.9,
                peak: 0.9,
                docs: vec![0, 1],
                segments: vec![
                    ClusterSeg { doc: 0, text: "总体架构分层解耦".into() },
                    ClusterSeg { doc: 1, text: "总体架构分层解藕".into() },
                ],
            }],
            collusion: Default::default(),
            sections: vec![],
            shared_terms: vec![],
        };
        let path = std::env::temp_dir().join("bg_export_test.xlsx");
        to_xlsx(&report, path.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(bytes.len() > 100, "xlsx 应非空，实际 {}", bytes.len());
        assert_eq!(&bytes[0..2], b"PK", "xlsx 应为 zip(PK) 格式");

        let dpath = std::env::temp_dir().join("bg_export_test.docx");
        to_docx(&report, dpath.to_str().unwrap()).unwrap();
        let db = std::fs::read(&dpath).unwrap();
        let _ = std::fs::remove_file(&dpath);
        assert_eq!(&db[0..2], b"PK", "docx 应为 zip(PK)");

        let hpath = std::env::temp_dir().join("bg_export_test.html");
        to_html(&report, hpath.to_str().unwrap()).unwrap();
        let hs = std::fs::read_to_string(&hpath).unwrap();
        let _ = std::fs::remove_file(&hpath);
        assert!(
            hs.contains("标书查重报告") && hs.contains("相似度矩阵"),
            "html 内容缺失"
        );
    }
}
