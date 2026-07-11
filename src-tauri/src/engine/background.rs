// 内置静态范本背景库（W3-4 降级版）。
//
// 【为何是降级版】原设计（§5「is_template 升级为段落级 IDF」）建增量 DF 库：每次导入
// 把文档 4-gram 计入跨工作区 background_grams 表，doc_count 累积增长。审查裁决（§2 M4 / §5
// MEDIUM）砍掉该增量库——比对结论随背景库演化漂移，破坏产品立身之本「同输入同输出、可举证」，
// 且跨项目计数外溢有保密观感。本模块保留其可复现的内核：随包版本化的【固定】静态语料 +
// 双阈值 4-gram DF。语料固定 → DF 固定 → 豁免集合逐字节可复现，且换一台机器结论一致。
//
// 【机制】静态语料是一组公开法定格式套话文档（投标函/授权委托书/廉政承诺/中小企业声明函 等
// 九部委范本式公开 formulaic 文本，非创作性作品，见 fixtures/templates/）。每篇文档按段落
// 取去重字符 4-gram 集，统计文档频率 DF：
//   · df/篇数 ≥ 60% → boilerplate 集（行业套话）；
//   · df/篇数 > 80% → legal 集（法定必备表述，既不作证据也不作嫌疑，legal ⊂ boilerplate）。
// 比对时对每块算 boiler_fraction = 命中 boilerplate 集的 4-gram 占比；≥0.6 判「行业范本套话」
// （详见 compare_service::run_inner 的 ignore_templates 接线）。
//
// 【与增量版的关键差异】不设 doc_count≥20 的启用门槛——那是增量库防冷启动漂移的护栏；
// 静态库无冷启动、内容恒定，始终生效方可复现。语料固定 5 篇即够 60%/80% 双阈值成立。
use crate::engine::features;
use crate::engine::normalize::{self, NormalizeOptions};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::OnceLock;

/// 静态语料版本戳（随包版本化）：语料内容变更须同步 bump，供 jobs 快照声明复现口径。
pub const CORPUS_VERSION: &str = "static-templates-v1";

/// 字符 n-gram 的 n（与 features::char_ngrams_n 一致）。
const NGRAM_N: usize = 4;
/// boilerplate 集阈值：4-gram 出现在 ≥ 此比例的语料篇数即计入。
const BOILER_DF_RATIO: f32 = 0.60;
/// legal 集阈值：4-gram 出现在 > 此比例的语料篇数即计入（法定必备表述）。
const LEGAL_DF_RATIO: f32 = 0.80;
/// 逐块豁免线：boiler_fraction ≥ 此值判「行业范本套话」，从残差聚类/围标信号剔除。
pub const BOILER_FRACTION_EXEMPT: f32 = 0.60;

/// 随包内置的静态背景语料（每个 &str 是一篇公开法定格式套话文档，段落以换行分隔）。
/// include_str! 在编译期嵌入二进制 → 随包版本化、离线可用、逐字节可复现。
const CORPUS: &[&str] = &[
    include_str!("../../fixtures/templates/std_forms_1.txt"),
    include_str!("../../fixtures/templates/std_forms_2.txt"),
    include_str!("../../fixtures/templates/std_forms_3.txt"),
    include_str!("../../fixtures/templates/std_forms_4.txt"),
    include_str!("../../fixtures/templates/std_forms_5.txt"),
];

/// 双阈值背景库：固定静态语料一次性算好的 boilerplate/legal 4-gram 集与语料篇数。
pub struct BackgroundLib {
    boilerplate: HashSet<u64>,
    legal: HashSet<u64>,
    doc_count: usize,
}

impl BackgroundLib {
    /// 从静态语料计算：每篇按段落取去重 4-gram（段落级，避免跨段边界 gram），
    /// 统计 DF 后按双阈值切两集。归一用默认 NormalizeOptions——与绝大多数比对配置一致；
    /// 非默认归一（大小写/标点/空白开关）下 4-gram 略有偏移但套话以中文为主，重合稳健。
    fn compute() -> Self {
        let opts = NormalizeOptions::default();
        let mut df: HashMap<u64, usize> = HashMap::new();
        let mut doc_count = 0usize;
        for doc in CORPUS {
            let mut doc_grams: HashSet<u64> = HashSet::new();
            for line in doc.lines() {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                let norm = normalize::normalize(t, &opts);
                doc_grams.extend(features::char_ngrams_n(&norm, NGRAM_N));
            }
            if doc_grams.is_empty() {
                continue;
            }
            doc_count += 1;
            for g in doc_grams {
                *df.entry(g).or_insert(0) += 1;
            }
        }
        let n = doc_count.max(1) as f32;
        let mut boilerplate = HashSet::new();
        let mut legal = HashSet::new();
        for (&g, &d) in &df {
            let ratio = d as f32 / n;
            if ratio >= BOILER_DF_RATIO {
                boilerplate.insert(g);
            }
            if ratio > LEGAL_DF_RATIO {
                legal.insert(g);
            }
        }
        BackgroundLib { boilerplate, legal, doc_count }
    }

    /// 语料篇数（复现口径快照用）。
    pub fn doc_count(&self) -> usize {
        self.doc_count
    }

    /// boilerplate 集大小（Tools/诊断展示用）。
    pub fn boiler_gram_count(&self) -> usize {
        self.boilerplate.len()
    }

    /// legal 集大小。
    pub fn legal_gram_count(&self) -> usize {
        self.legal.len()
    }

    /// 逐块背景占比：normalized_text 的字符 4-gram 中命中 boilerplate 集的比例 ∈ [0,1]。
    /// 入参须为已归一文本（比对期 chunk.normalized_text）；不足 4 字或无 gram 返回 0。
    /// 纯计数、与 HashSet 迭代序无关 → 确定性。
    pub fn boiler_fraction(&self, normalized_text: &str) -> f32 {
        let grams = features::char_ngrams_n(normalized_text, NGRAM_N);
        if grams.is_empty() {
            return 0.0;
        }
        let hit = grams.iter().filter(|g| self.boilerplate.contains(g)).count();
        hit as f32 / grams.len() as f32
    }
}

/// 进程级单例：固定语料只算一次。返回 'static 引用，比对期零额外 IO。
pub fn load() -> &'static BackgroundLib {
    static LIB: OnceLock<BackgroundLib> = OnceLock::new();
    LIB.get_or_init(BackgroundLib::compute)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 语料中出现在全部 5 篇的名词无关套话（廉政承诺句）。其 4-gram 全部 df=5 → 全在 boilerplate。
    const LEGAL_CLAUSE: &str =
        "为维护公平竞争的招标投标秩序，我方郑重承诺，在参与本次投标活动中自觉遵守国家有关法律法规和廉政建设的各项规定，不与其他投标人相互串通投标报价，自觉维护招标投标活动的正常秩序。";
    /// 语料中不存在的原创技术段落（本场私有内容的代表）。
    const NOVEL: &str =
        "本公司自主研发的智能边缘计算调度平台采用容器化微服务架构，实现全链路可观测与弹性伸缩，并通过自研的分布式一致性算法保障多活数据中心的强一致。";

    #[test]
    fn corpus_loads_with_expected_doc_count() {
        let lib = load();
        assert_eq!(lib.doc_count(), 5, "静态语料应为 5 篇");
        assert!(lib.boiler_gram_count() > 0, "boilerplate 集不应为空");
        // legal ⊂ boilerplate（阈值 0.8 > 0.6）
        assert!(lib.legal_gram_count() <= lib.boiler_gram_count());
        assert!(lib.legal_gram_count() > 0, "全篇共有的法定套话应进 legal 集");
    }

    #[test]
    fn legal_clause_is_boilerplate() {
        let lib = load();
        // 套话段（归一后）boiler_fraction 应远超豁免线。
        let norm = normalize::normalize(LEGAL_CLAUSE, &NormalizeOptions::default());
        let f = lib.boiler_fraction(&norm);
        assert!(f >= BOILER_FRACTION_EXEMPT, "法定套话段 boiler_fraction={f} 应 ≥ {BOILER_FRACTION_EXEMPT}");
    }

    #[test]
    fn novel_content_is_not_boilerplate() {
        let lib = load();
        // 库中不存在的原创段落——仅本场共享也不该被静态库豁免。
        let norm = normalize::normalize(NOVEL, &NormalizeOptions::default());
        let f = lib.boiler_fraction(&norm);
        assert!(f < BOILER_FRACTION_EXEMPT, "原创技术段 boiler_fraction={f} 应 < {BOILER_FRACTION_EXEMPT}");
    }

    #[test]
    fn deterministic_thresholds_and_fraction() {
        // 固定语料 → 两次计算集合大小与逐块占比逐字节一致（可复现内核）。
        let a = BackgroundLib::compute();
        let b = BackgroundLib::compute();
        assert_eq!(a.boiler_gram_count(), b.boiler_gram_count());
        assert_eq!(a.legal_gram_count(), b.legal_gram_count());
        assert_eq!(a.doc_count(), b.doc_count());
        let norm = normalize::normalize(LEGAL_CLAUSE, &NormalizeOptions::default());
        assert_eq!(a.boiler_fraction(&norm), b.boiler_fraction(&norm), "同输入 boiler_fraction 应逐位一致");
    }

    #[test]
    fn empty_and_short_text_safe() {
        let lib = load();
        assert_eq!(lib.boiler_fraction(""), 0.0);
        assert_eq!(lib.boiler_fraction("短"), 0.0, "不足 4 字无 gram");
    }
}
