// Word 报告 v2：判定 → 文档 → 八类统计 → 逐对相似 → 事实冲突 → 条款明细 → 附录。
// 最小合法 OOXML（zip + document.xml），Word/WPS/Pages 原生渲染中文。
use super::data::ExportData;
use super::shared::{docx_p, field_cn, label, level_cn, review_cn, severity_cn, type_cn, write_docx_package};

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
            "综合判定：{}（评分 {:.0}%）",
            level_cn(&data.collusion.level),
            data.collusion.score * 100.0
        ),
        true,
        26,
    );
    for s in &data.collusion.signals {
        docx_p(&mut body, &format!("· {}（权重 {:.0}%）", s.detail, s.weight * 100.0), false, 21);
    }

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
                "#{} [{}] {} · 相似 {:.0}% · 涉及 {} · {}",
                c.index,
                type_cn(&c.cluster_type),
                c.topic.as_deref().unwrap_or(""),
                c.score.unwrap_or(0.0) * 100.0,
                docs.join("·"),
                review_cn(&c.review_status)
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
