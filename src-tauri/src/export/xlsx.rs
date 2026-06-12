// Excel 报告 v2：总览 / 相似度矩阵 / 条款明细 / 事实冲突 / 逐对明细 五个工作表。
use super::data::ExportData;
use super::shared::{field_cn, label, level_cn, review_cn, section_cn, severity_cn, type_cn};
use rust_xlsxwriter::{Format, Workbook};

type R = Result<(), String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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
            &data.generated_at[..16].replace('T', " "),
            data.app_version
        )).map_err(err)?;
        r += 2;
        s.write_string_with_format(r, 0, "综合判定", &head).map_err(err)?;
        s.write_string(r, 1, format!(
            "{}（评分 {:.0}%）",
            level_cn(&data.collusion.level),
            data.collusion.score * 100.0
        )).map_err(err)?;
        r += 1;
        for sig in &data.collusion.signals {
            s.write_string(r, 1, format!("· {}（权重 {:.0}%）", sig.detail, sig.weight * 100.0))
                .map_err(err)?;
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
        for (c, h) in ["组号", "类型", "风险", "确认", "标段", "组内相似", "主题", "文档", "页码", "段落文本"]
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
                s.write_string(r, 3, review_cn(&cl.review_status)).map_err(err)?;
                s.write_string(r, 4, section_cn(cl.section_kind.as_deref().unwrap_or("other")))
                    .map_err(err)?;
                s.write_number_with_format(r, 5, cl.score.unwrap_or(0.0), &pctf).map_err(err)?;
                s.write_string(r, 6, cl.topic.as_deref().unwrap_or("")).map_err(err)?;
                s.write_string(r, 7, &m.tag).map_err(err)?;
                if let Some(p) = m.page {
                    s.write_number(r, 8, p as f64).map_err(err)?;
                }
                s.write_string(r, 9, &m.text).map_err(err)?;
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

    wb.save(path).map_err(err)
}
