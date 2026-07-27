// 逐字雷同区间检测（W4-1 铁证层）：手写后缀自动机（SAM）求跨文档极大公共子串。
// 高精度低召回——刻意用「原文仅剥空白」的逐字语义（非 normalized），给评标报告提供
// 带起止锚点、可直接引用的零概率误报文本证据（「甲 3.2 节与乙 3.2 节 800 字逐字相同」）。
//
// 与语义/n-gram 层的分工：本层只报「一字不差（去空白后）」的长串，全角/半角标点差异、
// 洗稿改写不在此层召回，由召回/聚类/对齐层兜底。SAM 构建 O(n)、匹配 O(m) 摊还，
// 确定性构造（无随机源），满足取证可复现。
//
// W3 残差桥接（§9 排期审查 HIGH）：完全落在「引用招标文件」豁免块（tender_coverage≥0.8）
// 或样板块（ignore_templates 开启时）内的区间被丢弃——两份标书对同一招标条款的合法逐字
// 应答不得以「铁证」形态还魂。豁免判定由调用方按块预置到 VbChunk.exempt。
use std::collections::HashMap;

/// 一份参评文档的一个 paragraph 级分块（逐字层输入）。text 为原文（仅剥空白后进 SAM）。
pub struct VbChunk {
    pub id: String,
    /// 原文（保留全部字符；SAM 前只跳过 Unicode 空白）。
    pub text: String,
    /// 豁免块：引用招标文件（tender_coverage≥0.8）或样板块（ignore_templates 时）。
    /// 完全落在豁免块内的区间被丢弃（W3 桥接）。
    pub exempt: bool,
}

/// 一份参评文档（按 order_index 有序的 paragraph 分块）。文档在 find_pairwise 输入切片中的
/// 位置即其参评序号，产出区间的 doc_a/doc_b 用该位置标识（与 compare_service 的 docs 同序）。
pub struct VbDoc {
    pub chunks: Vec<VbChunk>,
}

/// 一条逐字雷同区间。doc_a < doc_b 恒成立（按参评序号规范化），两侧锚点各自指向
/// 「起块内起始字符偏移（含）→ 止块内结束字符偏移（不含）」，偏移按原文 char 计。
/// 单块内区间时 chunk.text 的 char 切片 [start_offset, end_offset) 即匹配文本。
#[derive(Debug, Clone, PartialEq)]
pub struct VerbatimMatch {
    pub doc_a: usize,
    pub doc_b: usize,
    pub a_start_chunk_id: String,
    pub a_start_offset: usize,
    pub a_end_chunk_id: String,
    pub a_end_offset: usize,
    pub b_start_chunk_id: String,
    pub b_start_offset: usize,
    pub b_end_chunk_id: String,
    pub b_end_offset: usize,
    /// 去空白后的逐字匹配字符数（== 两侧区间归一长度）。
    pub char_len: usize,
    /// 匹配文本样本（去空白）。超 SAMPLE_CAP 截断并加省略号，char_len 仍为完整长度。
    pub sample_text: String,
}

/// sample_text 存储上限（字符）：防超长逐字段撑爆库；char_len 保留完整长度。
pub const SAMPLE_CAP: usize = 2000;

// —— 后缀自动机（SAM）——
// 每状态存 len/link/转移（HashMap）与 first_end=该状态某子串在源串中首次出现的结束下标。
// first_end 供把「B 侧匹配」锚回 A 侧原文位置（只记首次出现，同串多处只锚一个——设计已知取舍）。
struct Sam {
    len: Vec<i32>,
    link: Vec<i32>,
    next: Vec<HashMap<char, usize>>,
    first_end: Vec<i32>,
    last: usize,
}

impl Sam {
    fn build(s: &[char]) -> Self {
        let mut sam = Sam {
            len: Vec::with_capacity(2 * s.len() + 2),
            link: Vec::with_capacity(2 * s.len() + 2),
            next: Vec::with_capacity(2 * s.len() + 2),
            first_end: Vec::with_capacity(2 * s.len() + 2),
            last: 0,
        };
        // 根状态
        sam.len.push(0);
        sam.link.push(-1);
        sam.next.push(HashMap::new());
        sam.first_end.push(-1);
        for (pos, &c) in s.iter().enumerate() {
            sam.extend(c, pos);
        }
        sam
    }

    fn add_state(&mut self, len: i32, link: i32, first_end: i32) -> usize {
        self.len.push(len);
        self.link.push(link);
        self.next.push(HashMap::new());
        self.first_end.push(first_end);
        self.len.len() - 1
    }

    fn extend(&mut self, c: char, pos: usize) {
        let cur = self.add_state(self.len[self.last] + 1, -1, pos as i32);
        let mut p = self.last as i32;
        while p != -1 && !self.next[p as usize].contains_key(&c) {
            self.next[p as usize].insert(c, cur);
            p = self.link[p as usize];
        }
        if p == -1 {
            self.link[cur] = 0;
        } else {
            let q = self.next[p as usize][&c];
            if self.len[p as usize] + 1 == self.len[q] {
                self.link[cur] = q as i32;
            } else {
                // 分裂：clone 继承 q 的转移与首次出现位置（clone 的子串出现不晚于 q）
                let clone = self.add_state(self.len[p as usize] + 1, self.link[q], self.first_end[q]);
                self.next[clone] = self.next[q].clone();
                while p != -1 && self.next[p as usize].get(&c) == Some(&q) {
                    self.next[p as usize].insert(c, clone);
                    p = self.link[p as usize];
                }
                self.link[q] = clone as i32;
                self.link[cur] = clone as i32;
            }
        }
        self.last = cur;
    }
}

/// 一条原始极大匹配（归一坐标，半开右端在下游转半开区间）：
/// sam 侧 [sam_start, sam_end]、stream 侧 [stream_start, stream_end]、长度 len。
struct RawMatch {
    sam_start: usize,
    sam_end: usize,
    stream_start: usize,
    stream_end: usize,
    len: usize,
}

/// 以 sam 串建自动机，流式匹配 stream 串，收集长度 ≥ min 的极大公共子串。
/// 极大性：仅在「右端不能再延伸一字」处记一条（stream 位置 i 满足 len[i] ≥ min 且
/// len[i+1] ≠ len[i]+1），保证一条连续匹配走廊只产一条，无重复计数。
fn matches_for_pair(sam_chars: &[char], stream: &[char], min: usize) -> Vec<RawMatch> {
    if sam_chars.is_empty() || stream.is_empty() || min == 0 {
        return Vec::new();
    }
    let sam = Sam::build(sam_chars);
    // 逐位匹配统计：lens[i] = stream[..=i] 的最长后缀在 sam 中出现的长度；aend[i] = 其在 sam 的结束下标
    let mut lens: Vec<usize> = Vec::with_capacity(stream.len());
    let mut aends: Vec<usize> = Vec::with_capacity(stream.len());
    let mut v = 0usize;
    let mut l = 0i32;
    for &c in stream {
        if let Some(&nx) = sam.next[v].get(&c) {
            v = nx;
            l += 1;
        } else {
            while v != 0 && !sam.next[v].contains_key(&c) {
                v = sam.link[v] as usize;
            }
            if let Some(&nx) = sam.next[v].get(&c) {
                l = sam.len[v] + 1;
                v = nx;
            } else {
                v = 0;
                l = 0;
            }
        }
        lens.push(l as usize);
        aends.push(if l > 0 { sam.first_end[v] as usize } else { 0 });
    }
    let mut out = Vec::new();
    for i in 0..stream.len() {
        let li = lens[i];
        if li < min {
            continue;
        }
        // 右端可再延伸（下一位在同一走廊上更长）→ 交给 i+1 记录，避免重复
        let extends = i + 1 < stream.len() && lens[i + 1] == li + 1;
        if extends {
            continue;
        }
        let a_end = aends[i];
        out.push(RawMatch {
            sam_start: a_end + 1 - li,
            sam_end: a_end,
            stream_start: i + 1 - li,
            stream_end: i,
            len: li,
        });
    }
    out
}

/// 一份文档的归一视图：去空白后的字符序列 + 每字符 →(块下标, 块内原文 char 偏移) 映射。
struct DocNorm<'a> {
    chunks: &'a [VbChunk],
    norm: Vec<char>,
    map: Vec<(usize, usize)>,
}

impl<'a> DocNorm<'a> {
    fn build(doc: &'a VbDoc) -> Self {
        let mut norm = Vec::new();
        let mut map = Vec::new();
        for (ci, ch) in doc.chunks.iter().enumerate() {
            for (oi, c) in ch.text.chars().enumerate() {
                if c.is_whitespace() {
                    continue;
                }
                norm.push(c);
                map.push((ci, oi));
            }
        }
        DocNorm { chunks: &doc.chunks, norm, map }
    }

    /// 归一区间 [start, end]（含）→（起块 id, 起偏移, 止块 id, 止偏移(不含), 是否整段落豁免）。
    fn anchor(&self, start: usize, end: usize) -> (String, usize, String, usize, bool) {
        let (sc, so) = self.map[start];
        let (ec, eo) = self.map[end];
        let all_exempt = (sc..=ec).all(|k| self.chunks[k].exempt);
        (self.chunks[sc].id.clone(), so, self.chunks[ec].id.clone(), eo + 1, all_exempt)
    }
}

fn sample_of(chars: &[char]) -> String {
    if chars.len() > SAMPLE_CAP {
        let mut s: String = chars[..SAMPLE_CAP].iter().collect();
        s.push('…');
        s
    } else {
        chars.iter().collect()
    }
}

/// 跨全部文档对（≤C(n,2)）求逐字雷同区间。min = verbatim_min_chars。
/// 每对以较短文档建 SAM（内存/时间上界），结果按 doc_a<doc_b 规范化。确定性：
/// 文档序、对序、对内 stream 位置序均固定，无随机源。
pub fn find_pairwise(docs: &[VbDoc], min: usize) -> Vec<VerbatimMatch> {
    let norms: Vec<DocNorm> = docs.iter().map(DocNorm::build).collect();
    let mut out = Vec::new();
    for i in 0..docs.len() {
        for j in (i + 1)..docs.len() {
            let (ni, nj) = (&norms[i], &norms[j]);
            // 以较短串建 SAM；等长以低序 i 为 SAM（确定性）。
            let sam_is_i = ni.norm.len() <= nj.norm.len();
            let (sam, stream) = if sam_is_i { (ni, nj) } else { (nj, ni) };
            for m in matches_for_pair(&sam.norm, &stream.norm, min) {
                let (sam_sc, sam_so, sam_ec, sam_eo, sam_exempt) =
                    sam.anchor(m.sam_start, m.sam_end);
                let (st_sc, st_so, st_ec, st_eo, st_exempt) =
                    stream.anchor(m.stream_start, m.stream_end);
                // W3 桥接：任一侧完全落在豁免块内 → 合法共享，不作铁证。
                if sam_exempt || st_exempt {
                    continue;
                }
                // 规范化 a=低序文档、b=高序文档；sample 取 a 侧（两侧逐字相同，取低序确定性）。
                let (a_side, a_slice_dn, a_slice_start, a_slice_end, b_side) = if sam_is_i {
                    (
                        (sam_sc, sam_so, sam_ec, sam_eo),
                        sam,
                        m.sam_start,
                        m.sam_end,
                        (st_sc, st_so, st_ec, st_eo),
                    )
                } else {
                    (
                        (st_sc, st_so, st_ec, st_eo),
                        stream,
                        m.stream_start,
                        m.stream_end,
                        (sam_sc, sam_so, sam_ec, sam_eo),
                    )
                };
                out.push(VerbatimMatch {
                    doc_a: i,
                    doc_b: j,
                    a_start_chunk_id: a_side.0,
                    a_start_offset: a_side.1,
                    a_end_chunk_id: a_side.2,
                    a_end_offset: a_side.3,
                    b_start_chunk_id: b_side.0,
                    b_start_offset: b_side.1,
                    b_end_chunk_id: b_side.2,
                    b_end_offset: b_side.3,
                    char_len: m.len,
                    sample_text: sample_of(&a_slice_dn.norm[a_slice_start..=a_slice_end]),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(_idx: usize, chunks: &[(&str, &str, bool)]) -> VbDoc {
        VbDoc {
            chunks: chunks
                .iter()
                .map(|(id, text, exempt)| VbChunk {
                    id: (*id).into(),
                    text: (*text).into(),
                    exempt: *exempt,
                })
                .collect(),
        }
    }

    /// 生成 n 个连续 CJK 字符（U+4E00 起），保证块内无重复、跨块可拼接。
    fn cjk(start: u32, n: usize) -> String {
        (0..n).map(|k| char::from_u32(start + k as u32).unwrap()).collect()
    }

    #[test]
    fn cross_paragraph_100_chars_one_merged_interval() {
        // 共享 100 字逐字文本跨两个段落（60 + 40），前后段落两侧均不同 → 恰好 1 条 char_len=100
        let shared1 = cjk(0x4E00, 60);
        let shared2 = cjk(0x4E00 + 60, 40);
        let a = doc(
            0,
            &[
                ("a0", "甲方前言只此一句", false),
                ("a1", &shared1, false),
                ("a2", &shared2, false),
                ("a3", "甲方尾段收束于此", false),
            ],
        );
        let b = doc(
            1,
            &[
                ("b0", "乙方开场别样文字", false),
                ("b1", &shared1, false),
                ("b2", &shared2, false),
                ("b3", "乙方结尾另作他述", false),
            ],
        );
        let ms = find_pairwise(&[a, b], 30);
        assert_eq!(ms.len(), 1, "跨段 100 字应合成恰好 1 条区间");
        let m = &ms[0];
        assert_eq!(m.char_len, 100);
        assert_eq!((m.doc_a, m.doc_b), (0, 1));
        assert_eq!(m.a_start_chunk_id, "a1");
        assert_eq!(m.a_start_offset, 0);
        assert_eq!(m.a_end_chunk_id, "a2");
        assert_eq!(m.a_end_offset, 40, "止块内 40 字全命中 → 结束偏移(不含)=40");
        assert_eq!(m.b_start_chunk_id, "b1");
        assert_eq!(m.b_end_chunk_id, "b2");
        assert_eq!(m.b_end_offset, 40);
        assert_eq!(m.sample_text.chars().count(), 100);
        assert_eq!(m.sample_text, format!("{shared1}{shared2}"));
    }

    #[test]
    fn below_threshold_yields_nothing() {
        // 共享 29 字 < 默认阈值 30 → 0 条
        let shared = cjk(0x4E00, 29);
        let a = doc(0, &[("a0", "唯甲独有此前缀", false), ("a1", &shared, false), ("a2", "唯甲独有此后缀", false)]);
        let b = doc(1, &[("b0", "乙方别样起头文", false), ("b1", &shared, false), ("b2", "乙方别样收尾文", false)]);
        let ms = find_pairwise(&[a, b], 30);
        assert!(ms.is_empty(), "29 字应低于阈值不输出，实际 {} 条", ms.len());
    }

    #[test]
    fn deterministic_across_runs() {
        let shared1 = cjk(0x4E00, 60);
        let shared2 = cjk(0x4E00 + 60, 40);
        let mk = || {
            let a = doc(0, &[("a0", "甲方前言只此一句", false), ("a1", &shared1, false), ("a2", &shared2, false), ("a3", "甲方尾段收束于此", false)]);
            let b = doc(1, &[("b0", "乙方开场别样文字", false), ("b1", &shared1, false), ("b2", &shared2, false), ("b3", "乙方结尾另作他述", false)]);
            find_pairwise(&[a, b], 30)
        };
        assert_eq!(mk(), mk(), "同输入两遍逐字段一致（无 id/时间字段，纯函数确定性）");
    }

    #[test]
    fn interval_wholly_in_exempt_block_dropped() {
        // E(50 字) 落在豁免块（引用招标）→ 丢弃；K(40 字) 非豁免 → 保留。中段两侧首末字均不同以断开。
        let e = cjk(0x4E00, 50);
        let k = cjk(0x4E00 + 50, 40);
        let a = doc(0, &[("ae", &e, true), ("amid", "甲隔断文字甚长以断丙", false), ("ak", &k, false)]);
        let b = doc(1, &[("be", &e, true), ("bmid", "乙另段隔断彼此区分丁", false), ("bk", &k, false)]);
        let ms = find_pairwise(&[a, b], 30);
        assert_eq!(ms.len(), 1, "只应保留非豁免的 K 区间");
        assert_eq!(ms[0].char_len, 40);
        assert_eq!(ms[0].a_start_chunk_id, "ak");
        assert_eq!(ms[0].b_start_chunk_id, "bk");
    }

    #[test]
    fn shorter_doc_used_as_sam_and_anchors_map_back() {
        // A 短 B 长：SAM 建在 A，锚点仍按 a=低序(0)/b=高序(1) 规范化回填
        let shared = cjk(0x4E00, 40);
        let a = doc(0, &[("a0", &shared, false)]);
        let b = doc(1, &[("b0", "乙冗长前缀内容甚多以致更长", false), ("b1", &shared, false), ("b2", "乙冗长后缀内容亦多", false)]);
        let ms = find_pairwise(&[a, b], 30);
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.char_len, 40);
        assert_eq!(m.a_start_chunk_id, "a0");
        assert_eq!(m.a_start_offset, 0);
        assert_eq!(m.b_start_chunk_id, "b1");
    }

    #[test]
    fn perf_guard_large_repetitive_input() {
        // 病态输入：两份 2 万字高度周期串（经典 SAM 最坏形态）——线性构造，秒级完成，产 1 条全长匹配
        let big: String = "阿弥陀佛观自在菩萨行深般若".chars().cycle().take(20000).collect();
        let a = doc(0, &[("a0", &big, false)]);
        let b = doc(1, &[("b0", &big, false)]);
        let t0 = std::time::Instant::now();
        let ms = find_pairwise(&[a, b], 30);
        let dt = t0.elapsed();
        assert_eq!(ms.len(), 1, "全同串应产恰好 1 条极大匹配");
        assert_eq!(ms[0].char_len, 20000);
        assert!(ms[0].sample_text.chars().count() <= SAMPLE_CAP + 1, "样本应受 SAMPLE_CAP 约束");
        assert!(dt.as_secs() < 10, "2 万字病态输入应秒级完成，实际 {dt:?}");
    }

    #[test]
    fn empty_and_single_doc_are_safe() {
        assert!(find_pairwise(&[], 30).is_empty());
        let a = doc(0, &[("a0", "只有一份文档没有对手", false)]);
        assert!(find_pairwise(&[a], 30).is_empty(), "单文档无对不产区间");
    }
}
