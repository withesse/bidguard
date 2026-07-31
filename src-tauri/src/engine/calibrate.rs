// 概率校准 + 共形三带（执行方案 §8 W6-4 / M7）。
//
// 解决什么问题：簇的 score（组内平均相似度）是【排序分】不是【概率】——0.8 不代表 80%
// 的把握。本模块把排序分过一层单调校准（Platt 两参数起步，小样本最稳），得到「在合成校准
// 语料上校准过的同源概率」，再用 split conformal 把「转人工」的边界从拍脑袋的 0.55/0.70
// 换成有限样本保证的阈值：
//   · t_low：以【正样本】的 (1−p) 为不合格分，取 ceil((n+1)(1−α))/n 分位 → 低于 t_low 的
//     簇在校准语料分布上漏检率 ≤α；
//   · t_high：以【负样本】的 p 为不合格分，同式取分位 → 高于 t_high 的簇误报率 ≤β。
//
// §1.5-1 产品纪律（硬约束，改文案前先读方案 §1.5）：
//  ① 三带命名固定为「低优先级抽查 / 需人工复核 / 重点标红」。【禁用「自动放行」「漏检保证」】
//     ——共形保证只在交换性成立的校准语料分布上成立，真实标书分布漂移时承诺失效，
//     在监管场景把统计假设讲成对评标方的承诺有法律暴露。
//  ② pass 带（低优先级抽查）【只做排序与折叠，不隐藏任何簇】：list_clusters 把 pass 排到
//     最后，前端默认折叠，但计数、筛选、导出一律完整可达。
//  ③ 所有 α/β 相关文案强制限定「在合成校准语料上测得」。
//  ④ 三带是【复核路由的正交维度】：八类 cluster_type 与 severity 语义完全不动，band 不参与
//     围标融合，也不改任何既有分级。
//
// α/β 与阈值随包固化在 fixtures/calibration/score_calib.json（改 α 即改承诺语义 → 走版本
// 发布，不开放运行时调整）；设置页只读展示版本与语料 hash。
//
// 回退路径（§1.5-6）：文件缺失/损坏/未过审查 → active_calibration() 返回 None，
// compare_service 写 NULL confidence/band，前端与旧任务同路径显示「未校准」，
// 【不静默编造一个校准】。

/// 三带的落库码值（DB clusters.band / DTO band 字段）。UI 与导出一律经 band_cn 取中文，
/// 避免各处硬编码文案漂移（§1.5-1 命名是硬约束）。
pub const BAND_PASS: &str = "pass";
pub const BAND_REVIEW: &str = "review";
pub const BAND_FLAG: &str = "flag";

/// 三带中文名【唯一来源】。禁止出现「自动放行 / 漏检保证」字样（§1.5-1）。
pub fn band_cn(band: &str) -> &'static str {
    match band {
        BAND_PASS => "低优先级抽查",
        BAND_REVIEW => "需人工复核",
        BAND_FLAG => "重点标红",
        _ => "未校准",
    }
}

/// 三带释义（UI 提示与导出章节共用）。同样受 §1.5-1 约束：pass 带写明「不隐藏、可随时展开」。
pub fn band_hint(band: &str) -> &'static str {
    match band {
        BAND_PASS => "校准概率低于低位阈值：默认折叠、排在最后，仍完整保留可展开与导出，建议抽查",
        BAND_REVIEW => "校准概率位于两条阈值之间：判读不确定，需人工复核",
        BAND_FLAG => "校准概率高于高位阈值：建议优先重点复核",
        _ => "本次比对未启用校准（或为旧任务），三带不可用，按既有风险等级复核",
    }
}

/// 分流开关（拟合侧的退化守卫写入，运行时只读）。
///
/// 为什么需要它：三带分流只有在「校准概率在【簇实际存在的分数区间】里有分辨力」时才成立。
/// M7 落地时对现有合成语料实测（见 corpusgen::fit_calibration 的守卫与 score_calib.json 台账）：
///   · pairs 语料的 unrelated 与正类【完全可分】（unrelated 最大 0.188 < rewrite 最小 0.211），
///     校准曲线的拐点落在 s≈0.26，而簇的分数恒 ≥ 相似阈值（默认 0.70）；
///   · docsets 的【独立集】里同样存在 avg=1.000 的簇（各家共享的范本/法定格式段落）。
/// 两条合起来的结论是：相似度分本身无法区分「同源编制」与「合法共享」，硬按概率阈值分流
/// 会把几乎所有簇推进同一条带（实测运行域内 100% 落重点标红）——那是告警疲劳，不是分流。
/// 故本版随包文件声明 review-all：三带机器全部就位、置信度照常展示，但【不做分流断言】，
/// 全部条目按需人工复核处理（§1.5-1「如实展示」；acceptance③ 明示的回退路径）。
/// 真实脱敏语料（含「独立编制但表面相似」的难负样本）到位后重跑 fit-calib 即自动启用三带。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    /// 三带分流生效（拟合侧确认运行域内有分辨力）。
    ThreeBand,
    /// 不做分流：全部条目落「需人工复核」，置信度仍展示并标注限定语。
    ReviewAll,
}

impl Routing {
    pub fn as_str(self) -> &'static str {
        match self {
            Routing::ThreeBand => "three-band",
            Routing::ReviewAll => "review-all",
        }
    }
}

/// 分流状态的说明文案（设置页 / 报告脚注 / 三带 chips 提示共用，禁止各处另写一份）。
pub fn routing_note(routing: Routing, alpha: f32, beta: f32) -> String {
    match routing {
        Routing::ThreeBand => format!(
            "三带分流已启用：低优先级抽查带的漏检率目标 α={:.0}%、重点标红带的误报率目标 β={:.0}%，\
             均为【在合成校准语料上测得】的带内错误率，不是对真实标书的承诺；\
             低优先级抽查带只做排序与折叠，不隐藏任何条款。",
            alpha * 100.0,
            beta * 100.0
        ),
        Routing::ReviewAll => "三带分流未启用：本版校准语料不含「独立编制但表面相似」的难负样本，\
             相似度分在簇的分数区间内无分辨力，据此分流会把几乎所有条款推进同一条带。\
             故全部条款按【需人工复核】处理，置信度仅作参考（在合成校准语料上校准，非串通概率）。"
            .to_string(),
    }
}

/// 校准器。Platt = 两参数 sigmoid（小样本最稳）；Isotonic = 单调分段线性（语料够后无缝切换，
/// 文件格式已前向兼容：只换 type 与参数块，运行时与落库口径不变）。
#[derive(Debug, Clone, PartialEq)]
pub enum Calibrator {
    /// p = σ(a·s + b)，要求 a ≥ 0（单调不减：分越高概率不得反而更低）。
    Platt { a: f32, b: f32 },
    /// 断点 (s, p) 升序，段内线性插值，两端外推为端点值。
    Isotonic { breakpoints: Vec<(f32, f32)> },
}

impl Calibrator {
    /// 排序分 → 校准概率（值域恒 [0,1]）。
    pub fn probability(&self, s: f32) -> f32 {
        let s = if s.is_finite() { s.clamp(0.0, 1.0) } else { 0.0 };
        let p = match self {
            Calibrator::Platt { a, b } => 1.0 / (1.0 + (-(a * s + b)).exp()),
            Calibrator::Isotonic { breakpoints } => interpolate(breakpoints, s),
        };
        if p.is_finite() {
            p.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// 文件里的 type 值（写回与展示用）。
    pub fn kind_str(&self) -> &'static str {
        match self {
            Calibrator::Platt { .. } => "platt",
            Calibrator::Isotonic { .. } => "isotonic",
        }
    }
}

fn interpolate(bp: &[(f32, f32)], s: f32) -> f32 {
    match bp.first() {
        None => 0.0,
        Some(&(x0, y0)) if s <= x0 => y0,
        _ => {
            let last = bp[bp.len() - 1];
            if s >= last.0 {
                return last.1;
            }
            for w in bp.windows(2) {
                let ((x0, y0), (x1, y1)) = (w[0], w[1]);
                if s <= x1 {
                    let span = x1 - x0;
                    return if span <= f32::EPSILON { y1 } else { y0 + (y1 - y0) * (s - x0) / span };
                }
            }
            last.1
        }
    }
}

/// 生效的校准模型（随包固化，运行时只读）。
#[derive(Debug, Clone)]
pub struct CalibrationModel {
    /// §1.5-6 实验性标签：experimental-synthetic = 合成语料拟合，真实判例回测前不摘。
    pub calibration_kind: String,
    pub version: String,
    pub calibrator: Calibrator,
    /// 低优先级抽查带在【合成校准语料】上的目标漏检率。
    pub alpha: f32,
    /// 重点标红带在【合成校准语料】上的目标误报率。
    pub beta: f32,
    pub t_low: f32,
    pub t_high: f32,
    /// 分流开关：review-all 时忽略两条阈值，全部落「需人工复核」（见 Routing 文档）。
    pub routing: Routing,
    /// 训练语料 hash（设置页只读展示，报告脚注可追溯）。
    pub corpus_hash: String,
}

impl CalibrationModel {
    pub fn probability(&self, s: f32) -> f32 {
        self.calibrator.probability(s)
    }

    /// 校准概率 → 三带码值（受分流开关约束）。
    pub fn band_of(&self, p: f32) -> &'static str {
        match self.routing {
            Routing::ThreeBand => band_of(p, self.t_low, self.t_high),
            Routing::ReviewAll => BAND_REVIEW,
        }
    }

    /// 排序分 →（校准概率，三带码值）。落库与导出共用同一通道，保证同输入逐字节一致。
    pub fn evaluate(&self, s: f32) -> (f32, &'static str) {
        let p = round6(self.probability(s));
        (p, self.band_of(p))
    }

    pub fn routing_note(&self) -> String {
        routing_note(self.routing, self.alpha, self.beta)
    }
}

/// 六位定点：落库/导出/两次比对之间的逐字节一致靠它（f32 求值本身确定，定点只是消除
/// 展示与序列化层的末位噪声）。
fn round6(v: f32) -> f32 {
    ((v as f64 * 1e6).round() / 1e6) as f32
}

/// 三带判定（纯函数，边界闭合口径：p < t_low → pass；p ≥ t_high → flag；其余 review）。
pub fn band_of(p: f32, t_low: f32, t_high: f32) -> &'static str {
    if !p.is_finite() {
        return BAND_REVIEW;
    }
    if p < t_low {
        BAND_PASS
    } else if p >= t_high {
        BAND_FLAG
    } else {
        BAND_REVIEW
    }
}

/// 校准输入分：M7 的管线顺序【固定】为 rerank 改判建议在前、校准在后（§8 风险④：
/// 顺序不固定则同配置结果不可复现）。rerank_avg = 复核层给出的簇级倾向分（W6-2，
/// 默认关闭 → None）。融合取凸组合而非替换：cross-encoder 是检索相关性模型，
/// 「相关 ≠ 同源改写」，不能单独决定判读（§1.5-3）。
pub const RERANK_BLEND: f32 = 0.5;

pub fn calibration_input(avg: f32, rerank_avg: Option<f32>) -> f32 {
    match rerank_avg {
        Some(r) if r.is_finite() => {
            round6((RERANK_BLEND * r.clamp(0.0, 1.0) + (1.0 - RERANK_BLEND) * avg).clamp(0.0, 1.0))
        }
        _ => avg,
    }
}

// —— 磁盘格式（前向兼容 isotonic）——

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalibFile {
    calibration_kind: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    platt: Option<PlattParams>,
    #[serde(default)]
    isotonic: Option<IsotonicParams>,
    alpha: f32,
    beta: f32,
    thresholds: Thresholds,
    /// 缺省即【不分流】：没有显式声明分辨力的文件一律不做分流断言（保守缺省，
    /// 手工编辑的文件不会因为漏写一个字段就悄悄开始给条款分带）。
    #[serde(default)]
    routing: Option<String>,
    #[serde(default)]
    fit: FitLedger,
}

#[derive(serde::Deserialize)]
struct PlattParams {
    a: f32,
    b: f32,
}

#[derive(serde::Deserialize)]
struct IsotonicParams {
    breakpoints: Vec<[f32; 2]>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thresholds {
    t_low: f32,
    t_high: f32,
}

/// 台账段：运行时只读 pairsHash（设置页展示），其余字段供评审与追溯。
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FitLedger {
    #[serde(default)]
    pairs_hash: String,
}

/// 允许的最大 α/β：超过一半的目标错误率没有产品意义，多半是文件写错。
const RATE_MAX: f32 = 0.5;
/// Platt 斜率上限（防病态拟合把概率钉死在 0/1）。
const SLOPE_MAX: f32 = 60.0;

/// 解析 + 审查（与 collusion::parse_lr_model 同纪律：宁可回退也不用可疑参数）。
/// 抽成纯函数便于单测「损坏 → 回退未校准」而无需篡改磁盘文件。
pub fn parse_calibration(raw: &str) -> Result<CalibrationModel, String> {
    let f: CalibFile = serde_json::from_str(raw).map_err(|e| format!("JSON 解析失败：{e}"))?;
    if f.calibration_kind.trim().is_empty() {
        return Err("缺少 calibrationKind 标签".into());
    }
    let calibrator = match f.kind.as_str() {
        "platt" => {
            let p = f.platt.ok_or("type=platt 但缺少 platt 参数块")?;
            if !p.a.is_finite() || !p.b.is_finite() {
                return Err(format!("Platt 参数非有限值：a={} b={}", p.a, p.b));
            }
            if p.a < 0.0 {
                return Err(format!("Platt 斜率 a={} 为负：校准必须单调不减", p.a));
            }
            if p.a > SLOPE_MAX {
                return Err(format!("Platt 斜率 a={} 超上限 {SLOPE_MAX}（疑似病态拟合）", p.a));
            }
            Calibrator::Platt { a: p.a, b: p.b }
        }
        "isotonic" => {
            let i = f.isotonic.ok_or("type=isotonic 但缺少 isotonic 参数块")?;
            if i.breakpoints.len() < 2 {
                return Err("isotonic 断点少于 2 个".into());
            }
            let mut bp: Vec<(f32, f32)> = Vec::with_capacity(i.breakpoints.len());
            for [x, y] in i.breakpoints {
                if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                    return Err(format!("isotonic 断点越界：({x}, {y})"));
                }
                if let Some(&(px, py)) = bp.last() {
                    if x < px || y < py {
                        return Err("isotonic 断点非单调不减".into());
                    }
                }
                bp.push((x, y));
            }
            Calibrator::Isotonic { breakpoints: bp }
        }
        other => return Err(format!("未知校准类型 {other}（支持 platt|isotonic）")),
    };
    for (name, v) in [("alpha", f.alpha), ("beta", f.beta)] {
        if !v.is_finite() || v <= 0.0 || v > RATE_MAX {
            return Err(format!("{name}={v} 越界（须在 (0, {RATE_MAX}]）"));
        }
    }
    let (lo, hi) = (f.thresholds.t_low, f.thresholds.t_high);
    if !lo.is_finite() || !hi.is_finite() || !(0.0..=1.0).contains(&lo) || !(0.0..=1.0).contains(&hi) {
        return Err(format!("三带阈值越界：tLow={lo} tHigh={hi}"));
    }
    if lo > hi {
        return Err(format!("三带阈值倒挂：tLow={lo} > tHigh={hi}"));
    }
    let routing = match f.routing.as_deref() {
        None | Some("review-all") => Routing::ReviewAll,
        Some("three-band") => Routing::ThreeBand,
        Some(other) => return Err(format!("未知分流模式 {other}（支持 three-band|review-all）")),
    };
    Ok(CalibrationModel {
        calibration_kind: f.calibration_kind,
        version: f.version,
        calibrator,
        alpha: f.alpha,
        beta: f.beta,
        t_low: lo,
        t_high: hi,
        routing,
        corpus_hash: f.fit.pairs_hash,
    })
}

/// 随包固化的校准文件（不可运行时热换：同一安装包对同一输入恒定产出，结果可举证）。
const CALIB_JSON: &str = include_str!("../../fixtures/calibration/score_calib.json");

/// 生效校准模型；文件不可用 → None（写 NULL confidence/band，前端显示「未校准」）+ 一次性 warn。
pub fn active_calibration() -> Option<&'static CalibrationModel> {
    static MODEL: std::sync::OnceLock<Option<CalibrationModel>> = std::sync::OnceLock::new();
    MODEL
        .get_or_init(|| match parse_calibration(CALIB_JSON) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!(
                    "概率校准文件不可用（{e}），本次比对不产出置信度与三带（簇按既有风险等级复核）；\
                     比对本身照常进行"
                );
                None
            }
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platt() -> CalibrationModel {
        CalibrationModel {
            calibration_kind: "test".into(),
            version: "t".into(),
            calibrator: Calibrator::Platt { a: 10.0, b: -5.0 },
            alpha: 0.05,
            beta: 0.05,
            t_low: 0.3,
            t_high: 0.8,
            routing: Routing::ThreeBand,
            corpus_hash: String::new(),
        }
    }

    #[test]
    fn band_boundaries_are_half_open() {
        // p < t_low → pass；t_low ≤ p < t_high → review；p ≥ t_high → flag（边界归属钉死）。
        assert_eq!(band_of(0.2999, 0.3, 0.8), BAND_PASS);
        assert_eq!(band_of(0.3, 0.3, 0.8), BAND_REVIEW);
        assert_eq!(band_of(0.7999, 0.3, 0.8), BAND_REVIEW);
        assert_eq!(band_of(0.8, 0.3, 0.8), BAND_FLAG);
        assert_eq!(band_of(1.0, 0.3, 0.8), BAND_FLAG);
        assert_eq!(band_of(f32::NAN, 0.3, 0.8), BAND_REVIEW);
    }

    #[test]
    fn band_names_never_say_auto_pass() {
        // §1.5-1 铁律：三带命名固定，且禁用「自动放行 / 漏检保证」字样。
        assert_eq!(band_cn(BAND_PASS), "低优先级抽查");
        assert_eq!(band_cn(BAND_REVIEW), "需人工复核");
        assert_eq!(band_cn(BAND_FLAG), "重点标红");
        assert_eq!(band_cn("whatever"), "未校准");
        for b in [BAND_PASS, BAND_REVIEW, BAND_FLAG, "x"] {
            let text = format!("{}{}", band_cn(b), band_hint(b));
            assert!(!text.contains("自动放行"), "三带文案禁用「自动放行」：{text}");
            assert!(!text.contains("漏检保证"), "三带文案禁用「漏检保证」：{text}");
        }
    }

    #[test]
    fn platt_is_monotone_and_bounded() {
        let m = platt();
        let mut prev = -1.0f32;
        for i in 0..=100 {
            let p = m.probability(i as f32 / 100.0);
            assert!((0.0..=1.0).contains(&p));
            assert!(p >= prev, "校准必须单调不减");
            prev = p;
        }
        assert_eq!(m.probability(f32::NAN), m.probability(0.0));
    }

    #[test]
    fn isotonic_interpolates_and_clamps() {
        let c = Calibrator::Isotonic { breakpoints: vec![(0.2, 0.1), (0.6, 0.5), (0.9, 0.95)] };
        assert!((c.probability(0.0) - 0.1).abs() < 1e-6, "左端外推取端点值");
        assert!((c.probability(1.0) - 0.95).abs() < 1e-6, "右端外推取端点值");
        assert!((c.probability(0.4) - 0.3).abs() < 1e-6, "段内线性插值");
        assert!((c.probability(0.6) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn evaluate_is_deterministic_and_six_decimal_fixed() {
        let m = platt();
        for i in 0..=50 {
            let s = i as f32 / 50.0;
            let (p1, b1) = m.evaluate(s);
            let (p2, b2) = m.evaluate(s);
            assert_eq!(p1.to_bits(), p2.to_bits(), "同输入必须逐字节一致");
            assert_eq!(b1, b2);
            assert_eq!(p1, round6(p1));
        }
    }

    #[test]
    fn rerank_blend_only_applies_when_present() {
        assert_eq!(calibration_input(0.6, None), 0.6);
        assert!((calibration_input(0.6, Some(0.8)) - 0.7).abs() < 1e-6);
        assert_eq!(calibration_input(0.6, Some(f32::NAN)), 0.6);
    }

    #[test]
    fn review_all_routing_ignores_thresholds_but_keeps_confidence() {
        // 分流未启用时：三带一律「需人工复核」，置信度照常给出（§1.5-1 如实展示）。
        let m = CalibrationModel { routing: Routing::ReviewAll, ..platt() };
        for s in [0.0f32, 0.2, 0.5, 0.9, 1.0] {
            let (p, band) = m.evaluate(s);
            assert_eq!(band, BAND_REVIEW, "review-all 下不得分流：s={s}");
            assert_eq!(p, round6(m.probability(s)));
        }
        let note = m.routing_note();
        assert!(note.contains("未启用"));
        assert!(!note.contains("自动放行") && !note.contains("漏检保证"));
    }

    #[test]
    fn parse_rejects_broken_files_and_accepts_both_types() {
        let ok = r#"{"calibrationKind":"experimental-synthetic","version":"v","type":"platt",
            "platt":{"a":9.0,"b":-4.5},"alpha":0.05,"beta":0.05,"routing":"three-band",
            "thresholds":{"tLow":0.2,"tHigh":0.9},"fit":{"pairsHash":"abc"}}"#;
        let m = parse_calibration(ok).unwrap();
        assert_eq!(m.corpus_hash, "abc");
        assert_eq!(m.calibrator.kind_str(), "platt");
        assert_eq!(m.routing, Routing::ThreeBand);
        // 缺 routing 字段 → 保守缺省为不分流。
        let no_routing = ok.replace("\"routing\":\"three-band\",", "");
        assert_eq!(parse_calibration(&no_routing).unwrap().routing, Routing::ReviewAll);
        assert!(parse_calibration(&ok.replace("three-band", "yolo")).is_err());

        let iso = r#"{"calibrationKind":"k","version":"v","type":"isotonic",
            "isotonic":{"breakpoints":[[0.0,0.0],[0.5,0.4],[1.0,1.0]]},"alpha":0.05,"beta":0.05,
            "thresholds":{"tLow":0.2,"tHigh":0.9}}"#;
        assert_eq!(parse_calibration(iso).unwrap().calibrator.kind_str(), "isotonic");

        for bad in [
            r#"{"calibrationKind":"","version":"v","type":"platt","platt":{"a":1.0,"b":-1.0},"alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"platt","platt":{"a":-1.0,"b":-1.0},"alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"platt","alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"platt","platt":{"a":1.0,"b":-1.0},"alpha":0.9,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"platt","platt":{"a":1.0,"b":-1.0},"alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.95,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"isotonic","isotonic":{"breakpoints":[[0.5,0.5],[0.2,0.6]]},"alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
            r#"{"calibrationKind":"k","version":"v","type":"quantum","alpha":0.05,"beta":0.05,"thresholds":{"tLow":0.2,"tHigh":0.9}}"#,
        ] {
            assert!(parse_calibration(bad).is_err(), "应拒绝：{bad}");
        }
    }

    #[test]
    fn shipped_calibration_file_passes_review() {
        // 随包文件必须自洽：解析通过、三带阈值不倒挂、概率单调。
        let m = parse_calibration(CALIB_JSON).expect("随包校准文件应通过审查");
        assert!(m.t_low <= m.t_high);
        assert!(m.probability(0.9) >= m.probability(0.1));
    }
}
