// Markdown 报告：文本归档 / 知识库（§14.1）。条款明细超过上限时显式注明，绝不静默截断。
use super::data::ExportData;
use super::shared::{field_cn, level_cn, review_cn, section_cn, severity_cn, type_cn};
use std::fmt::Write as _;

const MAX_DETAIL_CLUSTERS: usize = 1000;

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
        "## 综合判定\n\n**{}**（评分 {:.0}%）\n",
        level_cn(&data.collusion.level),
        data.collusion.score * 100.0
    );
    for s in &data.collusion.signals {
        let _ = writeln!(m, "- {}（权重 {:.0}%）", s.detail, s.weight * 100.0);
    }

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
            "### #{} [{}{}] {} · 相似 {:.0}% · {} · {}\n",
            c.index,
            type_cn(&c.cluster_type),
            c.severity.as_deref().map(|s| format!("·{}", severity_cn(s))).unwrap_or_default(),
            c.topic.as_deref().unwrap_or(""),
            c.score.unwrap_or(0.0) * 100.0,
            section_cn(c.section_kind.as_deref().unwrap_or("other")),
            review_cn(&c.review_status)
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

    let _ = writeln!(m, "\n## 附录：比对配置\n\n```json\n{}\n```\n", data.config);
    let _ = writeln!(
        m,
        "*报告由「原本 · 标书查重」v{} 本地生成（报告格式 {}），未上传任何文件。*",
        data.app_version, data.report_version
    );

    std::fs::write(path, m).map_err(|e| e.to_string())
}
