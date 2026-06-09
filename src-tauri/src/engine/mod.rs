// 引擎编排：解析 → 分词/分段 → 文档级矩阵 + 逐对段落对齐 + 跨文档聚类 + 元数据指纹
pub mod collusion;
pub mod embed;
pub mod fingerprint;
pub mod ocr;
pub mod parse;
pub mod report;
pub mod segment;
pub mod similarity;

use jieba_rs::Jieba;
use report::{DocInfo, Progress, Report};
use std::path::Path;

/// 分析 2-5 份标书。`progress` 在各阶段回调，用于驱动「检测中」实时进度。
pub fn analyze(
    paths: Vec<String>,
    templates: Vec<String>,
    semantic: bool,
    threshold: f32,
    scope: String,
    progress: &dyn Fn(Progress),
) -> Result<Report, String> {
    if paths.len() < 2 {
        return Err("至少需要 2 份标书进行交叉比对".into());
    }
    if paths.len() > 5 {
        return Err("一次最多比对 5 份标书".into());
    }

    let jieba = Jieba::new();
    // 查重源 / 通用模板的 token 集，用于剔除样板段落
    let template_tokens: Vec<Vec<String>> = templates
        .iter()
        .map(|t| similarity::tokenize(&jieba, t))
        .filter(|t| !t.is_empty())
        .collect();
    let total = paths.len();
    let mut docs: Vec<DocInfo> = Vec::with_capacity(total);
    let mut seg_docs: Vec<Vec<segment::Segment>> = Vec::with_capacity(total);
    let mut doc_texts: Vec<String> = Vec::with_capacity(total);

    // 1) 解析 + 分词 + 分段
    for (i, p) in paths.iter().enumerate() {
        let path = Path::new(p);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(p)
            .to_string();
        progress(Progress {
            stage: "parse".into(),
            done: i,
            total,
            note: name.clone(),
        });
        let doc_type = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match parse::parse_file(path) {
            Ok(pd) => {
                seg_docs.push(segment::segment(&jieba, &pd.text, &template_tokens));
                docs.push(DocInfo {
                    id: format!("doc{i}"),
                    name,
                    doc_type,
                    pages: pd.pages,
                    char_count: pd.text.chars().count(),
                    fingerprint: pd.fingerprint,
                    parse_error: None,
                });
                doc_texts.push(pd.text);
            }
            Err(e) => {
                seg_docs.push(Vec::new());
                docs.push(DocInfo {
                    id: format!("doc{i}"),
                    name,
                    doc_type,
                    pages: 0,
                    char_count: 0,
                    fingerprint: report::Fingerprint::default(),
                    parse_error: Some(e),
                });
                doc_texts.push(String::new());
            }
        }
    }
    progress(Progress {
        stage: "parse".into(),
        done: total,
        total,
        note: "解析完成".into(),
    });

    // 2) 比对范围过滤（仅技术标 / 仅商务标）：tech 去掉商务段、business 去掉技术段，Other 保留
    let scoped: Vec<Vec<segment::Segment>> = match scope.as_str() {
        "tech" => seg_docs
            .into_iter()
            .map(|segs| {
                segs.into_iter()
                    .filter(|s| s.section != segment::Section::Business)
                    .collect()
            })
            .collect(),
        "business" => seg_docs
            .into_iter()
            .map(|segs| {
                segs.into_iter()
                    .filter(|s| s.section != segment::Section::Tech)
                    .collect()
            })
            .collect(),
        _ => seg_docs,
    };
    // 由（过滤后）段落派生文档级 token，使比对范围真正影响矩阵
    let token_docs: Vec<Vec<String>> = scoped
        .iter()
        .map(|segs| segs.iter().flat_map(|s| s.tokens.clone()).collect())
        .collect();

    // 文档级相似度矩阵（可叠加语义）
    let (mut matrix, mut peak) = similarity::matrix(&token_docs);
    let n = docs.len();

    // 2.5) 语义查重：embedding 余弦叠加到矩阵（取较大者），捕捉改写式雷同
    if semantic {
        progress(Progress {
            stage: "semantic".into(),
            done: 0,
            total: 1,
            note: "语义比对".into(),
        });
        let texts: Vec<String> = doc_texts
            .iter()
            .map(|t| t.chars().take(1500).collect::<String>())
            .collect();
        if let Some(embs) = embed::embed(&texts) {
            for i in 0..n {
                for j in (i + 1)..n {
                    if docs[i].parse_error.is_some() || docs[j].parse_error.is_some() {
                        continue;
                    }
                    let sem = embed::cosine(&embs[i], &embs[j]);
                    if sem > matrix[i][j] {
                        matrix[i][j] = sem;
                        matrix[j][i] = sem;
                        if sem > peak {
                            peak = sem;
                        }
                    }
                }
            }
        }
    }

    // 3) 逐对段落对齐（带进度）
    let pair_total = n * (n - 1) / 2;
    let mut pairs = Vec::with_capacity(pair_total);
    let mut done = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            pairs.push(segment::align_pair(i, j, matrix[i][j], &scoped[i], &scoped[j], threshold));
            done += 1;
            progress(Progress {
                stage: "compare".into(),
                done,
                total: pair_total,
                note: format!("已比对 {done} / {pair_total} 对"),
            });
        }
    }

    // 4) 跨文档聚类
    progress(Progress {
        stage: "cluster".into(),
        done: 0,
        total: 1,
        note: "聚合雷同条款".into(),
    });
    let clusters = segment::cluster_segments(&scoped, threshold);

    // 5) 元数据指纹交叉标记
    fingerprint::cross_flags(&mut docs);

    // 6) 共有特征词 / 章节热力 / 围标综合判定
    let shared_terms = segment::shared_terms(&scoped);
    let sections = segment::section_stats(&scoped);
    let collusion = collusion::assess(peak, &clusters, &docs, &shared_terms);

    progress(Progress {
        stage: "done".into(),
        done: 1,
        total: 1,
        note: "完成".into(),
    });

    Ok(Report {
        docs,
        matrix,
        peak,
        pairs,
        clusters,
        collusion,
        sections,
        shared_terms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop(_: Progress) {}

    #[test]
    fn similar_docs_score_higher_than_different() {
        let dir = std::env::temp_dir();
        let p1 = dir.join("bg_test_a.txt");
        let p2 = dir.join("bg_test_b.txt");
        let p3 = dir.join("bg_test_c.txt");
        let common = "本项目采用分层解耦的微服务总体架构，系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层，所有业务能力对外以统一接口网关暴露，保证横向可扩展与纵向可演进。";
        std::fs::write(&p1, format!("{common}甲方在实施计划中补充了里程碑安排与质量保证措施。")).unwrap();
        std::fs::write(&p2, format!("{common}乙方在实施计划中补充了里程碑安排与质量保证措施。")).unwrap();
        std::fs::write(&p3, "本方案聚焦数据治理与隐私合规，强调本地化部署、最小权限与全链路审计，组织方式与技术选型均独立设计。").unwrap();

        let paths = vec![
            p1.to_string_lossy().into_owned(),
            p2.to_string_lossy().into_owned(),
            p3.to_string_lossy().into_owned(),
        ];
        let r = analyze(paths, vec![], false, 0.35, "full".into(), &noop).unwrap();

        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
        let _ = std::fs::remove_file(&p3);

        assert_eq!(r.docs.len(), 3);
        let ab = r.matrix[0][1];
        let ac = r.matrix[0][2];
        assert!(ab > 0.6, "甲乙相似度应较高，实际 {ab}");
        assert!(ac < ab, "甲丙应低于甲乙：ac={ac} ab={ab}");
        assert!((r.matrix[0][0] - 1.0).abs() < 1e-6, "对角线应为 1");
        assert!(r.peak > 0.6, "峰值应较高，实际 {}", r.peak);

        // 段落引擎：甲乙这对应有匹配段落；应聚出至少一组跨文档雷同条款
        let ab_pair = r.pairs.iter().find(|p| p.a == 0 && p.b == 1).expect("应有甲乙这对");
        assert!(!ab_pair.matches.is_empty(), "甲乙应有匹配段落");
        assert!(
            ab_pair.matches[0].diff.iter().any(|d| d.op == "eq"),
            "匹配段落应含相同片段"
        );
        assert!(!r.clusters.is_empty(), "应聚出跨文档雷同条款");
        assert!(r.clusters[0].docs.len() >= 2, "聚合需跨 ≥2 份文档");
    }

    #[test]
    fn rejects_single_doc() {
        assert!(analyze(vec!["only.txt".into()], vec![], false, 0.35, "full".into(), &noop).is_err());
    }

    #[test]
    fn parses_pdf_fixture() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return; // 无夹具时跳过
        }
        let parsed = parse::parse_file(&fixture).expect("应能解析样例 PDF");
        assert!(!parsed.text.trim().is_empty(), "PDF 抽取文本不应为空");
        let lower = parsed.text.to_lowercase();
        assert!(
            lower.contains("bidguard") || lower.contains("gateway"),
            "应抽取到已知英文文本，实际：{:?}",
            parsed.text
        );
    }

    // 端到端：自动生成标书 → 跑完整引擎 → 验证围标判定 / 聚类 / 章节，并做负向对照。
    #[test]
    fn collusion_pipeline_on_generated_bids() {
        let dir = std::env::temp_dir().join("bidguard_gen_bids");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, content: String| -> String {
            let p = dir.join(name);
            std::fs::write(&p, content).unwrap();
            p.to_string_lossy().into_owned()
        };

        // —— 围标组：甲乙技术+商务条款近乎逐字雷同；甲乙丙共有合规声明与工期条款 ——
        let tech = "系统采用分层解耦的微服务架构设计自下而上划分为基础设施层数据资源层应用支撑层与业务应用层\n\
各层之间通过标准化接口解耦所有业务能力对外以统一接口网关暴露确保横向可扩展与纵向可演进\n\
平台采用读写分离与多级缓存机制保证高可用性与毫秒级的端到端响应";
        let compliance = "本项目严格遵循国家信息安全等级保护三级标准与相关行业规范要求";
        let schedule = "本工程建设周期为一百八十个日历日完成全部交付与验收工作";
        let qual = "我公司具备信息系统集成及服务一级资质与软件企业认定证书";

        let jia = write(
            "甲_智慧城邦.txt",
            format!("本技术方案由智慧城邦科技有限公司编制\n{tech}\n{compliance}\n{schedule}\n投标报价为人民币一千两百八十万元整包含全部软硬件与三年运维服务费用\n{qual}"),
        );
        let yi = write(
            "乙_启明信息.txt",
            format!("本技术方案由启明信息技术股份公司编制\n{tech}\n{compliance}\n{schedule}\n投标报价为人民币一千两百九十万元整包含全部软硬件与三年运维服务费用\n{qual}"),
        );
        let bing = write(
            "丙_鸿信科技.txt",
            format!("本技术方案由鸿信科技集团独立编写完成\n我们基于云原生容器编排技术构建弹性可伸缩的整体解决方案\n采用事件驱动与消息队列实现各子系统之间的异步协同与削峰填谷\n数据治理方面引入数据中台统一汇聚清洗与共享交换各类政务数据资源\n{compliance}\n{schedule}\n投标报价为人民币一千一百五十万元整\n我公司持有建筑智能化工程专业承包资质"),
        );

        let r = analyze(vec![jia, yi, bing], vec![], false, 0.5, "full".into(), &noop).unwrap();
        eprintln!(
            "[围标组] 峰值={:.2} 判定={}({:.2}) 信号={} 聚类={}(其中{}组跨≥3份) 共有特征词={}",
            r.peak,
            r.collusion.level,
            r.collusion.score,
            r.collusion.signals.len(),
            r.clusters.len(),
            r.clusters.iter().filter(|c| c.docs.len() >= 3).count(),
            r.shared_terms.len()
        );
        for s in &r.collusion.signals {
            eprintln!("   · {} (权重{:.2})", s.detail, s.weight);
        }

        assert!(r.peak >= 0.75, "甲乙应高度同源，实际峰值 {}", r.peak);
        let p01 = r.pairs.iter().find(|p| p.a == 0 && p.b == 1).expect("应有甲乙对");
        assert!(p01.matches.len() >= 4, "甲乙应有多处雷同段落，实际 {}", p01.matches.len());
        assert!(
            r.clusters.iter().any(|c| c.docs.len() >= 3),
            "应存在跨 3 份文档的雷同条款"
        );
        assert!(r.sections.iter().any(|s| s.section == "tech"), "应识别出技术标段");
        assert!(r.sections.iter().any(|s| s.section == "business"), "应识别出商务标段");
        assert!(
            matches!(r.collusion.level.as_str(), "high" | "medium"),
            "围标组应判定为需复核(high/medium)，实际 {}",
            r.collusion.level
        );

        // —— 负向对照：三份业务领域完全不同的独立标书，不应误判 ——
        let a = write(
            "独A.txt",
            "本公司专注于城市轨道交通信号系统的设计集成与现场实施工作\n依托自主研发的列车自动控制平台保障线路运行安全与准点率".into(),
        );
        let b = write(
            "独B.txt",
            "我司主营医院信息化与电子病历平台的建设运营服务\n凭借多年三甲医院项目经验提供稳定的临床数据与诊疗支撑".into(),
        );
        let c = write(
            "独C.txt",
            "团队从事智慧农业物联网传感终端的研发生产与销售\n通过田间环境监测与作物长势分析帮助种植户增产增收".into(),
        );
        let r2 = analyze(vec![a, b, c], vec![], false, 0.5, "full".into(), &noop).unwrap();
        eprintln!(
            "[独立组] 峰值={:.2} 判定={}({:.2}) 聚类={}",
            r2.peak,
            r2.collusion.level,
            r2.collusion.score,
            r2.clusters.len()
        );

        assert!(
            matches!(r2.collusion.level.as_str(), "none" | "low"),
            "独立标书不应判围标，实际 {}",
            r2.collusion.level
        );
        assert!(
            r2.clusters.iter().all(|c| c.docs.len() < 3),
            "独立标书不应出现跨 3 份的雷同条款"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
