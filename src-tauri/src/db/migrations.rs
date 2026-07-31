// 数据库迁移：基于 PRAGMA user_version 的顺序迁移。
// 规则：MIGRATIONS[i] 把库从 version i 升到 i+1；已发布的迁移只增不改。
use crate::error::{AppError, AppErrorCode, AppResult};
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    SCHEMA_V1,
    SEED_TEMPLATES_V2,
    PARSE_OPTIONS_V3,
    CLUSTER_LOCATION_V4,
    PREVIEW_NOTES_V5,
    CATEGORY_V6,
    CHUNK_TEMPLATE_V7,
    EMBEDDINGS_RESET_V8,
    DROP_UNUSED_EDGE_INDEXES_V9,
    CLUSTER_MEMBERS_INDEX_V10,
    DOC_TRUNCATION_NOTICE_V11,
    LICENSE_USAGE_V12,
    EVASION_JSON_V13,
    DOC_ROLE_V14,
    DOCUMENT_IMAGES_V15,
    CHUNK_EXEMPTIONS_V16,
    CLUSTERS_EXEMPT_V17,
    VERBATIM_MATCHES_V18,
    ALIGNED_SEGMENTS_V19,
    SEGMENT_DIFFS_V20,
    BOQ_ITEMS_V21,
    JOB_NUMERIC_JSON_V22,
    CLUSTER_CALIBRATION_V23,
    OFFICIAL_TEMPLATES_V24,
    OFFICIAL_TEMPLATES_V25,
    OFFICIAL_TEMPLATES_V26,
];

pub fn run(conn: &mut Connection) -> AppResult<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let target = MIGRATIONS.len() as i64;
    if current > target {
        return Err(
            AppError::new(AppErrorCode::DatabaseError, "数据文件由更新版本的应用创建，请升级应用")
                .with_detail(format!("db user_version={current}，应用支持到 {target}")),
        );
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

// V1：全量建表。
// 相对设计文档 §10.2 的增改：documents 增 char_count/fingerprint_json/parse_method；
// chunks 增 chunk_level/section_kind/is_template/char_count；语义向量独立成 embeddings 表
// （按 normalized_hash+model_id 跨任务缓存）；compare_jobs 改名 jobs（import/compare/export 共用）
// 并增聚合结果列；clusters 增 section_kind/conflict_json；外键全部 ON DELETE CASCADE，
// 删工作区/任务时由 SQLite 级联清理。
const SCHEMA_V1: &str = r#"
CREATE TABLE workspaces (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  settings_json TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE TABLE documents (
  id               TEXT PRIMARY KEY,
  workspace_id     TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  file_name        TEXT NOT NULL,
  file_path        TEXT NOT NULL,
  file_hash        TEXT NOT NULL,
  file_type        TEXT NOT NULL,
  status           TEXT NOT NULL,
  parse_error      TEXT,
  parse_method     TEXT,
  page_count       INTEGER,
  char_count       INTEGER,
  fingerprint_json TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL
);
CREATE INDEX idx_documents_workspace_id ON documents(workspace_id);
CREATE INDEX idx_documents_file_hash ON documents(file_hash);

CREATE TABLE chunks (
  id              TEXT PRIMARY KEY,
  document_id     TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_type      TEXT NOT NULL,
  chunk_level     TEXT NOT NULL DEFAULT 'paragraph',
  section_path    TEXT,
  section_kind    TEXT,
  is_template     INTEGER NOT NULL DEFAULT 0,
  text            TEXT NOT NULL,
  normalized_text TEXT NOT NULL,
  char_count      INTEGER,
  page            INTEGER,
  order_index     INTEGER NOT NULL,
  start_offset    INTEGER,
  end_offset      INTEGER,
  exact_hash      TEXT,
  normalized_hash TEXT,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_chunks_document_id ON chunks(document_id);
CREATE INDEX idx_chunks_exact_hash ON chunks(exact_hash);
CREATE INDEX idx_chunks_normalized_hash ON chunks(normalized_hash);
CREATE INDEX idx_chunks_order ON chunks(document_id, order_index);

CREATE TABLE chunk_features (
  chunk_id        TEXT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
  token_json      TEXT,
  char_ngram_json TEXT,
  entity_json     TEXT,
  minhash_blob    BLOB,
  extra_json      TEXT,
  created_at      TEXT NOT NULL
);

CREATE TABLE embeddings (
  normalized_hash TEXT NOT NULL,
  model_id        TEXT NOT NULL,
  dim             INTEGER NOT NULL,
  vector          BLOB NOT NULL,
  created_at      TEXT NOT NULL,
  PRIMARY KEY (normalized_hash, model_id)
);

CREATE TABLE jobs (
  id                TEXT PRIMARY KEY,
  workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  job_type          TEXT NOT NULL,
  name              TEXT,
  status            TEXT NOT NULL,
  config_json       TEXT NOT NULL DEFAULT '{}',
  progress          REAL NOT NULL DEFAULT 0,
  message           TEXT,
  error_message     TEXT,
  error_code        TEXT,
  starred           INTEGER NOT NULL DEFAULT 0,
  summary_json      TEXT,
  matrix_json       TEXT,
  collusion_json    TEXT,
  shared_terms_json TEXT,
  sections_json     TEXT,
  created_at        TEXT NOT NULL,
  started_at        TEXT,
  finished_at       TEXT
);
CREATE INDEX idx_jobs_workspace_id ON jobs(workspace_id);
CREATE INDEX idx_jobs_status ON jobs(status);

CREATE TABLE candidate_edges (
  id               TEXT PRIMARY KEY,
  job_id           TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  source_chunk_id  TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
  target_chunk_id  TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
  lexical_score    REAL,
  char_ngram_score REAL,
  entity_score     REAL,
  structure_score  REAL,
  order_score      REAL,
  semantic_score   REAL,
  final_score      REAL NOT NULL,
  created_at       TEXT NOT NULL
);
CREATE INDEX idx_edges_job_id ON candidate_edges(job_id);
CREATE INDEX idx_edges_source ON candidate_edges(source_chunk_id);
CREATE INDEX idx_edges_target ON candidate_edges(target_chunk_id);
CREATE INDEX idx_edges_score ON candidate_edges(job_id, final_score);

CREATE TABLE clusters (
  id            TEXT PRIMARY KEY,
  job_id        TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  cluster_type  TEXT NOT NULL,
  topic         TEXT,
  summary       TEXT,
  severity      TEXT,
  score         REAL,
  section_kind  TEXT,
  conflict_json TEXT,
  review_status TEXT NOT NULL DEFAULT 'pending',
  created_at    TEXT NOT NULL
);
CREATE INDEX idx_clusters_job_id ON clusters(job_id);
CREATE INDEX idx_clusters_type ON clusters(job_id, cluster_type);
CREATE INDEX idx_clusters_severity ON clusters(job_id, severity);

CREATE TABLE cluster_members (
  cluster_id  TEXT NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_id    TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
  role        TEXT NOT NULL,
  score       REAL,
  PRIMARY KEY (cluster_id, document_id, chunk_id)
);

CREATE TABLE diffs (
  id              TEXT PRIMARY KEY,
  cluster_id      TEXT NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
  base_chunk_id   TEXT,
  target_chunk_id TEXT,
  diff_type       TEXT NOT NULL,
  diff_json       TEXT NOT NULL,
  summary         TEXT,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_diffs_cluster_id ON diffs(cluster_id);

CREATE TABLE facts (
  id              TEXT PRIMARY KEY,
  chunk_id        TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
  subject         TEXT,
  action          TEXT,
  object          TEXT,
  amount          TEXT,
  date_expr       TEXT,
  duration        TEXT,
  percentage      TEXT,
  condition_expr  TEXT,
  obligation_type TEXT,
  confidence      REAL,
  fact_json       TEXT,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_facts_chunk_id ON facts(chunk_id);

CREATE TABLE app_settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE source_templates (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  text       TEXT NOT NULL,
  enabled    INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);
"#;

// V2：内置查重源模板（与前端 templates.ts 的三条默认一致）。
// 导入时命中这些样板的段落标记 is_template，召回阶段剔除以减少误报。
const SEED_TEMPLATES_V2: &str = "
INSERT OR IGNORE INTO source_templates (id, name, text, enabled, created_at) VALUES
('t-law', '法律法规引用', '根据《中华人民共和国招标投标法》及其实施条例，以及《中华人民共和国政府采购法》的相关规定，本项目严格遵循公开、公平、公正和诚实信用的原则组织实施。', 1, '2026-06-10T00:00:00Z'),
('t-qual', '资质证书目录', '投标人具备独立法人资格，持有有效的营业执照、税务登记证及与本项目相适应的行业资质证书与质量管理体系认证，所有证照均在有效期内。', 1, '2026-06-10T00:00:00Z'),
('t-after', '标准售后承诺', '我方承诺提供 7×24 小时技术支持服务，质保期内免费维护，接到故障报修后及时响应并在约定时限内解决，确保系统稳定运行。', 1, '2026-06-10T00:00:00Z');
";

// V3：documents 增 parse_options_hash（解析期生效配置的指纹）。
// 解析参数（归一开关/表格识别/页码/页眉清理/最短段长）可配置后，跨工作区
// 「同 hash 复用分块」必须同时匹配配置指纹，否则旧配置的缓存会被错误复用。
// 旧行该列为 NULL：永不匹配任何指纹 → 保守地重新解析。
const PARSE_OPTIONS_V3: &str = "
ALTER TABLE documents ADD COLUMN parse_options_hash TEXT;
";

// V4：clusters 增 base_section_path / base_page（底版分块的位置），
// 条款列表行内直接展示「章节路径 + 页码」，不必点进详情。
const CLUSTER_LOCATION_V4: &str = "
ALTER TABLE clusters ADD COLUMN base_section_path TEXT;
ALTER TABLE clusters ADD COLUMN base_page INTEGER;
";

// V5：原文版式预览与批注。
// documents.ocr_layout_json：扫描件 OCR 行坐标（每页一组归一化 0..1 的 {t,x,y,w,h}），
// 供前端在页图上叠加隐形可选中文本层；非扫描件为 NULL。
// annotations：批注锚定到 文档(+分块/页/引文)，可选关联条款组（cluster_id 不设外键——
// 条款组随任务删除级联消失，批注作为评审记录应独立存续，由前端按存在性展示）。
const PREVIEW_NOTES_V5: &str = "
ALTER TABLE documents ADD COLUMN ocr_layout_json TEXT;
CREATE TABLE annotations (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  document_id  TEXT REFERENCES documents(id) ON DELETE CASCADE,
  chunk_id     TEXT REFERENCES chunks(id) ON DELETE CASCADE,
  cluster_id   TEXT,
  page         INTEGER,
  quote        TEXT,
  note         TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX idx_annotations_ws ON annotations(workspace_id);
CREATE INDEX idx_annotations_doc ON annotations(document_id);
";

// V6：source_templates 增 category（查重源按分类展示/筛选）。
// 旧行该列 NULL，前端统一归一为「未分类」；同时为三条内置种子补默认分类。
// 纯 ADD COLUMN，向后兼容；不触碰 enabled 列，list_enabled 契约不受影响。
const CATEGORY_V6: &str = "
ALTER TABLE source_templates ADD COLUMN category TEXT;
UPDATE source_templates SET category='法律法规' WHERE id='t-law' AND category IS NULL;
UPDATE source_templates SET category='资质证照' WHERE id='t-qual' AND category IS NULL;
UPDATE source_templates SET category='售后承诺' WHERE id='t-after' AND category IS NULL;
";

// V7：chunks 增 template_id（命中的查重源样板 id），统计「每条样板命中过多少文档」。
// 旧行 NULL（命中信息仅在重新导入后才记录）；命中数按 COUNT(DISTINCT document_id) 聚合。
// 索引加速按 template_id 的统计查询。
const CHUNK_TEMPLATE_V7: &str = "
ALTER TABLE chunks ADD COLUMN template_id TEXT;
CREATE INDEX idx_chunks_template_id ON chunks(template_id);
";

// V8：清空语义向量缓存。embed_batch 前缀策略改为按模型家族（E5→\"query: \"、BGE→无），
// 改变了喂给模型的实际文本，旧缓存（键含 model_id 但不含前缀）与新策略不一致，
// 一次性清空让下次比对按新策略重算。embeddings 仅为缓存、可再生，清空无损正确性。
const EMBEDDINGS_RESET_V8: &str = "
DELETE FROM embeddings;
";

// V9：删除 candidate_edges 上确无消费者的复合索引 idx_edges_score(job_id, final_score)——
// 无任何 SELECT 按 final_score 过滤/排序。
// 注意：idx_edges_source / idx_edges_target 不能删——source_chunk_id/target_chunk_id 是
// ON DELETE CASCADE 外键，删文档/工作区时 SQLite 靠这两个索引做级联查找，删了会退化为全表扫描。
const DROP_UNUSED_EDGE_INDEXES_V9: &str = "
DROP INDEX IF EXISTS idx_edges_score;
";

// V10：cluster_members 的 chunk_id / document_id 建索引。二者均为 ON DELETE CASCADE 外键，
// 但复合主键 (cluster_id, document_id, chunk_id) 的最左前缀是 cluster_id，删文档/工作区时
// 按 chunk_id/document_id 的级联查找无索引可用 → 每个被删 chunk 触发一次 cluster_members
// 全表扫描，大库删除退化到分钟级。与 V9 保留 idx_edges_source/target 同理（级联外键须有索引）。
const CLUSTER_MEMBERS_INDEX_V10: &str = "
CREATE INDEX IF NOT EXISTS idx_cluster_members_chunk ON cluster_members(chunk_id);
CREATE INDEX IF NOT EXISTS idx_cluster_members_doc ON cluster_members(document_id);
";

// V11：documents 增 truncation_notice（解析期「内容不完整」告知语：扫描件超 OCR 上限、
// 或 docx 正文 XML 中途出错截断）。前端以警示条展示，不进比对语料——提示文本本身若作为
// 正文参与比对，多份截断件的相同提示会被聚成假 same 雷同条款并触发假围标信号。旧行为 NULL。
const DOC_TRUNCATION_NOTICE_V11: &str = "
ALTER TABLE documents ADD COLUMN truncation_notice TEXT;
";

// V12：按次授权的使用审计表（license_usage）。
// 强制计数在 HMAC 状态文件（license::state，DB 可被直接改写不作数）；本表仅为审计与
// 「失败退款 / 启动对账」的落点：消费时插一行 consumed，任务失败/取消时置 refunded。
// 无外键到 jobs（job 删除后审计仍应留存；对账按 job 现状态判定）。
const LICENSE_USAGE_V12: &str = "
CREATE TABLE license_usage (
  id         TEXT PRIMARY KEY,
  license_id TEXT NOT NULL,
  job_id     TEXT,
  kind       TEXT NOT NULL,        -- licensed | trial
  state      TEXT NOT NULL,        -- consumed | refunded
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_license_usage_job ON license_usage(job_id);
CREATE INDEX idx_license_usage_state ON license_usage(state);
";

// V13：documents 增 evasion_json（W2 入口对抗层的文档级规避统计：隐形码点/同形字折叠/
// 混合脚本各类计数 + 受影响块数 + 最大单块浓度；块级分布在 chunk_features.extra_json）。
// 可空向后兼容：老工作区行为 NULL；新版导入无发现也保持 NULL（列非空即「有发现」）。
// 注：执行方案迁移台账原预分配 V12 给本列，但 license MVP 先行合入占用了 V12
// （license_usage 表，已发布只增不改），故顺延为 V13；台账后续编号（doc_role 等）相应顺移。
const EVASION_JSON_V13: &str = "
ALTER TABLE documents ADD COLUMN evasion_json TEXT;
";

// V14：documents 增 doc_role（'bid' 投标 | 'tender' 招标文件 | 'tender_supplement' 补遗/答疑）。
// W3 合法共享剥离层的事实基础：招标文件被误选参评会与各家对其条款的合法应答形成整片假雷同，
// 后续对减/k-共现查证也都依赖「哪些文档是招标文件」。旧行靠 DEFAULT 'bid' 向后兼容。
// 注：执行方案迁移台账原预分配 V13 给本列，因 license_usage 占用 V12 整体顺延一位（见 V13 注释）。
const DOC_ROLE_V14: &str = "
ALTER TABLE documents ADD COLUMN doc_role TEXT NOT NULL DEFAULT 'bid';
";

// V15：内嵌图片同源指纹表（W1-4 取证）。每行是一张文档内位图（docx word/media 或
// PDF 页对象），比对期两两跨文档碰撞：sha256 相等为硬命中（跨容器稳定的精确指纹），
// dhash 汉明距离小为近似命中。dhash 存 64 位 dHash 的位型（以 i64 存储，比对只做异或
// 计数不做算术，符号无关）；整页扫描图 dhash 为 NULL——只做 exact 不做 near，防「都是
// 空白页/同制式表格」误报。document_id 外键 ON DELETE CASCADE + 索引：删文档级联清理
// 图片行，与 V10 级联外键须有索引同理。旧文档无行 → 图片信号自然缺席，向后兼容。
const DOCUMENT_IMAGES_V15: &str = "
CREATE TABLE document_images (
  id          TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  idx         INTEGER NOT NULL,
  source      TEXT NOT NULL,     -- docx | pdf
  page        INTEGER,           -- PDF 页码（1 起）；docx 为 NULL
  width       INTEGER NOT NULL,
  height      INTEGER NOT NULL,
  sha256      TEXT NOT NULL,
  dhash       INTEGER            -- 64 位 dHash 位型；整页图为 NULL（只做 exact）
);
CREATE INDEX idx_document_images_doc ON document_images(document_id);
";

// V16：招标文件对减的豁免证据表（W3-2）。每行是一个投标分块「引用招标文件」的取证记录：
// coverage=命中招标 winnowing 指纹的字符覆盖率，spans_json=合并后的覆盖区间（供 UI/导出解释、
// 人工复核被剥离内容）。job 级证据（同一分块在不同任务/口径下覆盖率不同），随任务删除级联清理。
// kind 预留：'tender'（M4a）| 'background'（M4b 背景库复用）。job_id/chunk_id 双外键 ON DELETE
// CASCADE + 索引：删任务/文档时级联清理需索引（与 V10 级联外键须有索引同理）。旧库无表，向后兼容。
const CHUNK_EXEMPTIONS_V16: &str = "
CREATE TABLE chunk_exemptions (
  job_id     TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  chunk_id   TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,        -- tender | background
  coverage   REAL NOT NULL,
  spans_json TEXT,
  PRIMARY KEY (job_id, chunk_id, kind)
);
CREATE INDEX idx_chunk_exemptions_job ON chunk_exemptions(job_id);
CREATE INDEX idx_chunk_exemptions_chunk ON chunk_exemptions(chunk_id);
";

// V17：k-共现过滤升级（W3-3）——clusters 增两列：
//   · exempt_reason：≥3 家共有簇经查证命中招标文件（'tender'）或行业范本背景库（'background'）
//     的合法共享出处 → 从围标信号②/残差矩阵/high 统计剔除，但簇保留落库、UI 置灰可筛
//     （延续 is_template『标记不删除』哲学）；NULL = 未豁免。
//   · multi_doc_anomaly：两库皆查不到出处且查证质量闸门通过（招标文件已导入、非 OCR/扫描件、
//     对减覆盖率抽样达标）→ 1（『多家异常一致·待复核』，severity='review' 不自动 high、不进
//     high 统计，最终认定权属评标委员会）；0 = 非异常。
// 纯加列向后兼容：旧库 clusters 行取默认（NULL / 0），list_clusters 正常返回；豁免明细复用 V16。
const CLUSTERS_EXEMPT_V17: &str = "
ALTER TABLE clusters ADD COLUMN exempt_reason TEXT;
ALTER TABLE clusters ADD COLUMN multi_doc_anomaly INTEGER NOT NULL DEFAULT 0;
";

// V18：逐字雷同区间表（W4-1 铁证层，M5a）。每行是一对参评文档间一条「去空白后一字不差」的
// 极大公共子串证据：两侧各以（起块 id, 块内起偏移）→（止块 id, 块内止偏移(不含)）锚定原文，
// char_len=去空白后匹配字符数，sample_text=匹配文本样本。segment_id 预留（M5b 链化区段回填）。
// 迁移台账（§1 全局裁决 1）：V18=verbatim_matches。job_id/doc_a_id/doc_b_id 外键 ON DELETE
// CASCADE + 索引（级联删除需索引，与 V10 同理）；区间锚定的 chunk_id 不设 FK——块随文档删除
// 由 doc/job 级联覆盖，避免为纯锚点列多建两个级联索引。纯增表，旧工作区兼容。
const VERBATIM_MATCHES_V18: &str = "
CREATE TABLE verbatim_matches (
  id               TEXT PRIMARY KEY,
  job_id           TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  doc_a_id         TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  doc_b_id         TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  a_start_chunk_id TEXT NOT NULL,
  a_start_offset   INTEGER NOT NULL,
  a_end_chunk_id   TEXT NOT NULL,
  a_end_offset     INTEGER NOT NULL,
  b_start_chunk_id TEXT NOT NULL,
  b_start_offset   INTEGER NOT NULL,
  b_end_chunk_id   TEXT NOT NULL,
  b_end_offset     INTEGER NOT NULL,
  char_len         INTEGER NOT NULL,
  sample_text      TEXT NOT NULL,
  segment_id       TEXT,
  created_at       TEXT NOT NULL
);
CREATE INDEX idx_verbatim_matches_job ON verbatim_matches(job_id);
CREATE INDEX idx_verbatim_matches_pair ON verbatim_matches(job_id, doc_a_id, doc_b_id);
";

// V19：对齐区段与锚点表（W4-2 seed-chain-align，M5a）。aligned_segments 每行是一对参评文档间
// 一条连续对齐区段：两侧各以稠密行序区间 [start_order,end_order] + 首末 chunk 锚定，coverage=
// 被命中块字符和/区间总字符和（无重复计数覆盖率基础），verbatim_chars=区段内逐字锚点字数累计，
// avg_score=锚点均分，section_path/page 为两侧首块章节与页码范围。segment_anchors 每行是区段
// 内一条链化锚点（kind: edge 残差边 | soft 软种子 | verbatim 逐字铁证），复合主键去重、a/b_chunk_id
// 建索引供与 cluster_members 按 chunk 互查（区段↔聚类互链）。迁移台账（§1 全局裁决）：V19=
// aligned_segments(+segment_anchors)。job_id/doc_a_id/doc_b_id 外键 ON DELETE CASCADE + 索引
// （级联删除需索引，与 V10 同理）；segment_anchors.segment_id 外键级联，锚定 chunk_id 不设 FK
// （块随文档/任务级联覆盖）。纯增表，旧工作区兼容。
const ALIGNED_SEGMENTS_V19: &str = "
CREATE TABLE aligned_segments (
  id               TEXT PRIMARY KEY,
  job_id           TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  doc_a_id         TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  doc_b_id         TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  a_start_order    INTEGER NOT NULL,
  a_end_order      INTEGER NOT NULL,
  b_start_order    INTEGER NOT NULL,
  b_end_order      INTEGER NOT NULL,
  a_start_chunk_id TEXT NOT NULL,
  a_end_chunk_id   TEXT NOT NULL,
  b_start_chunk_id TEXT NOT NULL,
  b_end_chunk_id   TEXT NOT NULL,
  anchor_count     INTEGER NOT NULL,
  verbatim_chars   INTEGER NOT NULL,
  a_covered_chars  INTEGER NOT NULL,
  b_covered_chars  INTEGER NOT NULL,
  a_coverage       REAL NOT NULL,
  b_coverage       REAL NOT NULL,
  avg_score        REAL NOT NULL,
  a_section_path   TEXT,
  b_section_path   TEXT,
  a_page_start     INTEGER,
  a_page_end       INTEGER,
  b_page_start     INTEGER,
  b_page_end       INTEGER,
  created_at       TEXT NOT NULL
);
CREATE INDEX idx_aligned_segments_job ON aligned_segments(job_id);
CREATE INDEX idx_aligned_segments_pair ON aligned_segments(job_id, doc_a_id, doc_b_id);

CREATE TABLE segment_anchors (
  segment_id TEXT NOT NULL REFERENCES aligned_segments(id) ON DELETE CASCADE,
  a_chunk_id TEXT NOT NULL,
  b_chunk_id TEXT NOT NULL,
  kind       TEXT NOT NULL,        -- edge | soft | verbatim
  score      REAL NOT NULL,
  PRIMARY KEY (segment_id, a_chunk_id, b_chunk_id)
);
CREATE INDEX idx_segment_anchors_a ON segment_anchors(a_chunk_id);
CREATE INDEX idx_segment_anchors_b ON segment_anchors(b_chunk_id);
";

// V20：区段内 gap 带状字符级细化产物表（W4-3，M5a）。每行是一条对齐区段内相邻锚点之间一个 gap
// （两侧各一段连续未命中块）的细化结果：diff_json 是句级带状对齐 + 字符级细化后的 DiffOp 序列
// （eq/ins/del，过滤 ins 还原 A、过滤 del 还原 B），eq_chars 是该 gap 双方相同字符数（供区段
// 覆盖率从「锚点覆盖」升级为「细化后真实覆盖」的回填）。diff_type: gap-sentence（带状细化）|
// gap-degraded（任一侧超长降级整段句 diff）。a/b_chunk_id 是 gap 两侧首块定位（可空，供前端定位）。
// 不复用既有 diffs 表：其 cluster_id NOT NULL 且语义（底版 vs 目标）与 gap 对齐（双侧对称）不同。
// segment_id 外键 ON DELETE CASCADE：区段随任务/文档级联删除时 segment_diffs 自动清空
// （delete_job_results 显式删 aligned_segments 即触发级联，无需单独删）。纯增表，旧工作区兼容。
const SEGMENT_DIFFS_V20: &str = "
CREATE TABLE segment_diffs (
  id         TEXT PRIMARY KEY,
  segment_id TEXT NOT NULL REFERENCES aligned_segments(id) ON DELETE CASCADE,
  a_chunk_id TEXT,
  b_chunk_id TEXT,
  diff_type  TEXT NOT NULL,        -- gap-sentence | gap-degraded
  diff_json  TEXT NOT NULL,
  eq_chars   INTEGER NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_segment_diffs_segment ON segment_diffs(segment_id);
";

// V21：报价清单条目表（W5-1，M6 商务标数值层地基）。每行是一份参评文档里被识别为报价清单
// （工程量清单/BOQ）数据行的一个条目：规范六字段（编码/名称/单位/工程量/综合单价/合价）+
// chunk_id 原文锚点（供下钻 DocPreview 举证、JOIN 回 chunks 取原文）+ align_key 跨文档对齐键
// （编码前 12/9 位精确 或 名称+单位相似度召回；未跨文档对齐的条目为 NULL）。doc_index 是该
// 文档在本次任务里的位次（十天干标签口径），row_index 是文档内解析次序（稳定排序用）。
// 迁移台账（§1 全局裁决 1）：V21=boq_items 表、V22=jobs.numeric_json。
// job_id/document_id 外键均 ON DELETE CASCADE：删任务/删文档自动清空；delete_job_results
// 保留 job 行只清结果，故仍需显式 DELETE（与 chunk_exemptions/verbatim_matches 同理）。
// 纯新表，向后兼容（旧任务无行，前端按空数据隐藏数值面板）。
const BOQ_ITEMS_V21: &str = "
CREATE TABLE boq_items (
  id          TEXT PRIMARY KEY,
  job_id      TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  doc_index   INTEGER NOT NULL,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_id    TEXT NOT NULL,
  align_key   TEXT,
  code        TEXT,
  name        TEXT,
  unit        TEXT,
  qty         REAL,
  unit_price  REAL,
  total_price REAL,
  row_index   INTEGER NOT NULL,
  page        INTEGER,
  flags       TEXT,
  created_at  TEXT NOT NULL
);
CREATE INDEX idx_boq_items_job ON boq_items(job_id);
CREATE INDEX idx_boq_items_align ON boq_items(job_id, align_key);
";

// V22：jobs 增 numeric_json（W5-2，M6 商务标数值证据聚合列）。与既有五个结果 JSON 列同性质：
// 一次比对的数值层结论整体落一列（pairs[]：逐项雷同率、可比/相同条目数、告警标记、共享算术错误
// 明细含双方 chunk_id）。旧任务该列为 NULL，前端按缺省隐藏数值面板，向后兼容。
// 迁移台账（§1 全局裁决 1）：V21=boq_items 表、V22=jobs.numeric_json 列。
const JOB_NUMERIC_JSON_V22: &str = "
ALTER TABLE jobs ADD COLUMN numeric_json TEXT;
";

// V23：clusters 增三列（M7 收官里程碑的【唯一】新迁移，三列合批以免抢号）：
//   · confidence   REAL —— 校准后的置信度 p（engine/calibrate：簇分经 Platt 校准；
//                          【在合成校准语料上校准，不是串通概率】，UI 一律带限定语展示）；
//   · band         TEXT —— 复核路由三带码值 pass|review|flag（中文名由 calibrate::band_cn
//                          唯一给出：低优先级抽查 / 需人工复核 / 重点标红）；
//   · rerank_score REAL —— cross-encoder 复核建议分（W6-2）。本批先建列，M7 步骤 3 写入；
//                          与 confidence 同批落地是因为二者同属一次比对的复核路由产物，
//                          分两个迁移会让中间版本的库出现「有带无分」的半截状态。
// 迁移台账（§1 全局裁决 1）：V21=boq_items 表、V22=jobs.numeric_json、V23=clusters 三列。
// 旧行三列均为 NULL → 前端显示「未校准」，向后兼容、无回填（历史任务的分带若靠事后补算，
// 会与当时生效的校准版本不一致，反而不可举证）。
const CLUSTER_CALIBRATION_V23: &str = "
ALTER TABLE clusters ADD COLUMN confidence REAL;
ALTER TABLE clusters ADD COLUMN band TEXT;
ALTER TABLE clusters ADD COLUMN rerank_score REAL;
";

// V24：补入官方标准范本的「投标文件格式」条款作为查重源样板。
// 起因：V2 内置库只有 3 条泛用短句，实测发改委《标准施工招标文件》(2007年版)的
// 132 条条款对其最高余弦仅 0.223（阈值 0.7），命中 0 条——即全国依法必须招标项目
// 普遍适用的官方条款完全不受抑制。多份投标文件逐字照抄同一份官方表单是合法且必然的，
// 却在评分层拿满分（fixtures/corpus/template 的 verbatim 档误报率 100 %）。
//
// 选材克制：只收第八章「投标文件格式」里投标人**逐字复制**的表单（投标函/法定代表人
// 身份证明/授权委托书/联合体协议书/投标保函/资格审查附件要求）。刻意不收施工组织设计、
// 施工进度、总平面图的编制要求——它们紧邻 BidGuard 必须比对的核心内容，标为 is_template
// 会把段落**排除比对**，误压真实雷同的漏报代价高于误报。
//
// 注意：模板集进 import_service 的 templates_digest → options_hash，本迁移会使既有
// 文档的解析缓存失效，下次导入重新分块（这是正确行为：命中标记必须随模板集重算）。
const OFFICIAL_TEMPLATES_V24: &str = "
INSERT OR IGNORE INTO source_templates (id, name, text, enabled, category, created_at) VALUES
('t-ndrc-bidletter', '投标函（发改委2007范本）', '1．我方已仔细研究了 （项目名称） 标段施工招标文件的全部内容，愿意以人民币（大写） 元（¥ ）的投标总报价，工期 日历天，按合同约定实施和完成承包工程，修补工程中的任何缺陷，工程质量达到 。 2．我方承诺在投标有效期内不修改、撤销投标文件。 3．随同本投标函提交投标保证金一份，金额为人民币（大写） 元（¥ ）。 4．如我方中标： （1）我方承诺在收到中标通知书后，在中标通知书规定的期限内与你方签订合同。 （2）随同本投标函递交的投标函附录属于合同文件的组成部分。 （3）我方承诺按照招标文件规定向你方递交履约担保。 （4）我方承诺在合同约定的期限内完成并移交全部合同工程。 5．我方在此声明，所递交的投标文件及有关资料内容完整、真实和准确，且不存在第二章“投标人须知”第1.4.3项规定的任何一种情形。 6． （其他补充说明）。', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc-legalrep', '法定代表人身份证明（发改委范本）', '投标人名称： 单位性质： 地址： 成立时间： 年 月 日 经营期限： 姓名： 性别： 年龄： 职务： 系 （投标人名称）的法定代表人。 特此证明。', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc-poa', '授权委托书（发改委2007范本）', '本人 （姓名）系 （投标人名称）的法定代表人，现委托 （姓名）为我方代理人。代理人根据授权，以我方名义签署、澄清、说明、补正、递交、撤回、修改 （项目名称） 标段施工投标文件、签订合同和处理有关事宜，其法律后果由我方承担。 委托期限： 。 代理人无转委托权。 附：法定代表人身份证明', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc-consortium', '联合体协议书（发改委范本）', '（所有成员单位名称）自愿组成 （联合体名称）联合体，共同参加 （项目名称） 标段施工投标。现就联合体投标事宜订立如下协议。 1、 （某成员单位名称）为 （联合体名称）牵头人。 2、联合体牵头人合法代表联合体各成员负责本招标项目投标文件编制和合同谈判活动，并代表联合体提交和接收相关的资料、信息及指示，并处理与之有关的一切事务，负责合同实施阶段的主办、组织和协调工作。 3、联合体将严格按照招标文件的各项要求，递交投标文件，履行合同，并对外承担连带责任。 4、联合体各成员单位内部的职责分工如下： 。 5、本协议书自签署之日起生效，合同履行完毕后自动失效。 6、本协议书一式 份，联合体成员和招标人各执一份。', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc-bidbond', '投标保证金保函（发改委范本）', '鉴于 （投标人名称）（以下称“投标人”）于 年 月 日参加 （项目名称） 标段施工的投标， （担保人名称，以下简称“我方”）无条件地、不可撤销地保证：投标人在规定的投标文件有效期内撤销或修改其投标文件的，或者投标人在收到中标通知书后无正当理由拒签合同或拒交规定履约担保的，我方承担保证责任。收到你方书面通知后，在7日内无条件向你方支付人民币（大写） 元。 本保函在投标有效期内保持有效。要求我方承担保证责任的通知应在投标有效期内送达我方。', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc-qualdocs', '资格审查附件要求（发改委范本）', '“主要人员简历表”中的项目经理应附项目经理证、身份证、职称证、学历证、养老保险复印件，管理过的项目业绩须附合同协议书复印件；技术负责人应附身份证、职称证、学历证、养老保险复印件，管理过的项目业绩须附证明其所任技术职务的企业文件或用户证明；其他主要人员应附职称证（执业证或上岗证书）、养老保险复印件。', 1, '资格证照', '2026-07-31T00:00:00Z');
";

// V25：把官方表单样板从「施工」扩到「货物采购 / 工程服务」两个语域。
// V24 收的是 2007 年版《标准施工招标文件》的表单，措辞绑定施工（工期/日历天/工程质量）；
// 2017 年版五个标准招标文件（设备/材料/勘察/设计/监理，发改法规[2017]1606号）的投标函
// 另有专属清单（增值税税率、设备名称及技术服务、勘察纲要/设计方案/监理大纲等），
// 与施工版词面差异大，V16 样板匹配不上。
//
// 只收 2 条而非 5 条：实测五份投标函彼此高度重合（勘察/设计/监理 92–94%，设备/材料 87%），
// 各语域取 1 条即可覆盖同域其余（覆盖率由 official_form_templates_cover_all_domains 实测把关）。
// 模板集是分块期的逐块比对项，冗余样板只增成本不加召回。
const OFFICIAL_TEMPLATES_V25: &str = "
INSERT OR IGNORE INTO source_templates (id, name, text, enabled, category, created_at) VALUES
('t-ndrc17-bidletter-equip', '投标函·货物采购（发改委范本）', '我方已仔细研究了（项目名称）设备采购招标项目招标文件的全部 内容，愿意以人民币（大写）（¥）的投标总报价（其中，增 值税税率为）提供（设备名称及技术服务和质保期服务），并 按合同约定履行义务。2. 我方的投标文件包括下列内容：（1）投标函；（2）法定代表人（单位负责人）身份证明或授权委托书；（3）联合体协议书（如有）；（4）投标保证金（如有）；（5）商务和技术偏差表；（6）分项报价表；（7）资格审查资料；（8）投标设备技术性能指标的详细描述；（9）技术支持资料；（10）技术服务和质保期服务计划；…… 投标文件的上述组成部分如存在内容不一致的，以投标函为准。3．我方承诺除商务和技术偏差表列出的偏差外，我方响应招标文件的全部要求。4．我方承诺在招标文件规定的投标有效期内不撤销投标文件。5．如我方中标，我方承诺：（1）在收到中标通知书后，在中标通知书规定的期限内与你方签订合同；（2）在签订合同时不向你方提出附加条件；（3）按照招标文件要求提交履约保证金；（4）在合同约定的期限内完成合同规定的全部义务。6．我方在此声明，所递交的投标文件及有关资料内容完整、真实和准确，且不存在第二章 “投标人须知”第 1.4.3 项规定的任何一种情形。7．（其他补充说明）。', 1, '投标文件格式', '2026-07-31T00:00:00Z'),
('t-ndrc17-bidletter-survey', '投标函·工程服务（发改委范本）', '我方已仔细研究了（项目名称）勘察招标项目招标文件的全部内容，愿意以人民币（大写）（¥）的投标总报价（其中，增值税税 率为），勘察服务期限：日历天，按合同约定完成勘察工作。2. 我方的投标文件包括下列内容：（1）投标函及投标函附录；（2）法定代表人身份证明或授权委托书；（3）联合体协议书（如有）；（4）投标保证金（如有）；（5）勘察费用清单；（6）资格审查资料；（7）勘察纲要；…… 投标文件的上述组成部分如存在内容不一致的，以投标函为准。3．我方承诺在招标文件规定的投标有效期内不撤销投标文件。4．如我方中标，我方承诺：（1）在收到中标通知书后，在中标通知书规定的期限内与你方签订合同；（2）在签订合同时不向你方提出附加条件；（3）按照招标文件要求提交履约保证金；（4）在合同约定的期限内完成合同规定的全部义务。5．我方在此声明，所递交的投标文件及有关资料内容完整、真实和准确，且不存在第二章 “投标人须知”第 1.4.3 项规定的任何一种情形。6．（其他补充说明）。', 1, '投标文件格式', '2026-07-31T00:00:00Z');
";

// V26：补北京 2025 版授权委托书——短表单的地区变体会掉出阈值。
// 实测（regional_variant_suppression_coverage）：全国范本被各地改编后，长表单仍能被发改委版
// 样板抑制（联合体协议书：北京 0.843 / 浙江 0.863），但**短表单不行**——北京版授权委托书
// 仅 0.678，差 0.022 卡在 TEMPLATE_MATCH(0.7) 之下。原因是该表单仅百余字，token 量小，
// 北京按地方规则增加的"参加开标会/身份证号/其他事项"就足以把余弦推下阈值。
//
// 选择补样板而非调低阈值：阈值是全局的，调低会让所有文本更容易被判为样板，
// 即全局增加**漏报**；补一条样板的代价仅是每个分块多一次余弦。
// 一般规律（供后续补收参考）：长表单跨地区可复用一条，短表单需按地区各收。
const OFFICIAL_TEMPLATES_V26: &str = "
INSERT OR IGNORE INTO source_templates (id, name, text, enabled, category, created_at) VALUES
('t-bj2025-poa', '授权委托书·北京2025（地区变体）', '（投标人名称）的法定代表人，现委托我单位（姓名）身份证号：为我方代理人。代理人根据授权，就（工程 名称）以我方名义参加开标会、签署开标记录和下文载明的其他事项，其法律后果由我方承 担。其他事项：。委托期限：。代理人无转委托权。', 1, '投标文件格式', '2026-07-31T00:00:00Z');
";




/// V16 官方表单样板的 (id, text)，从迁移 SQL 现解析而来。
///
/// 供校准 harness 校验「命中照抄表单 / 不误压正文」两侧，与生产入库的是同一份字面量——
/// 测试另抄一份会随迁移修改而静默漂移，反而测不出问题。仅测试/开发工具编译。
#[cfg(any(test, feature = "dev-tools"))]
pub fn official_seed_texts() -> Vec<(String, String)> {
    // 行形如：('id', 'name', 'text', 1, 'category', 'ts'),
    [OFFICIAL_TEMPLATES_V24, OFFICIAL_TEMPLATES_V25, OFFICIAL_TEMPLATES_V26]
        .concat()
        .lines()
        .filter(|l| l.trim_start().starts_with("('t-"))
        .filter_map(|l| {
            // 按未转义的单引号切分：SQL 里内嵌单引号写作 '' ，这里先还原再取字段
            let cells: Vec<&str> = l.split('\'').collect();
            // cells: ["(", id, ", ", name, ", ", text, ", 1, ", category, ...]
            let id = cells.get(1)?.to_string();
            let text = cells.get(5)?.replace("''", "'");
            Some((id, text))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap()
    }

    #[test]
    fn migrates_fresh_db_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        // 幂等：重跑不报错、版本不变
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        // 抽查关键表可写入
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_settings (key, value_json, updated_at) VALUES ('k', '{}', 't')",
            [],
        )
        .unwrap();
        // V2 的 3 条泛用样板 + V16 的 6 条官方「投标文件格式」表单
        let tpl: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_templates WHERE enabled = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tpl, 12, "3 条默认模板 + V16 的 6 条施工表单 + V17 的 2 条货物/服务投标函");
        let official: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM source_templates WHERE id NOT IN ('t-law','t-qual','t-after') AND enabled = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(official, 9, "V16 补 6 条 + V17 补 2 条");
    }

    /// V16 可叠加到「已有 V15 的老库」上，且幂等（INSERT OR IGNORE）。
    #[test]
    fn official_templates_v16_applies_to_old_db_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..15] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 15).unwrap();
            tx.commit().unwrap();
        }
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_templates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 3, "V15 老库只有 3 条");
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_templates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 12, "升级后补齐官方表单");
        // 分类与内容非空（前端按 category 分组展示）
        let cat: String = conn
            .query_row(
                "SELECT category FROM source_templates WHERE id = 't-ndrc-bidletter'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, "投标文件格式");
        run(&mut conn).unwrap(); // 幂等
        let again: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_templates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(again, 12, "重跑不重复插入");
    }

    #[test]
    fn evasion_json_migration_applies_to_old_db_and_is_idempotent() {
        // 老库（V12：license MVP 版本的数据文件）升级：run() 只补 V13，
        // documents.evasion_json 可写；重跑幂等
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..12] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 12).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
             status, evasion_json, created_at, updated_at)
             VALUES ('d1', 'w1', 'f', 'p', 'h', 'txt', 'parsed', '{\"zeroWidth\":3}', 't', 't')",
            [],
        )
        .unwrap();
        let ev: Option<String> = conn
            .query_row("SELECT evasion_json FROM documents WHERE id = 'd1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ev.as_deref(), Some("{\"zeroWidth\":3}"));

        // 幂等：重跑不报错、版本不变
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn doc_role_migration_backfills_bid_and_is_idempotent() {
        // 老库（V13：doc_role 列出现之前）升级：既有文档行全部回填 'bid'（DEFAULT 生效），
        // 新角色可写入且同 hash 双角色可并存；重跑幂等
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..13] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 13).unwrap();
            tx.commit().unwrap();
        }
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
             status, created_at, updated_at)
             VALUES ('d1', 'w1', 'f', 'p', 'h', 'txt', 'parsed', 't', 't')",
            [],
        )
        .unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        let role: String = conn
            .query_row("SELECT doc_role FROM documents WHERE id = 'd1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(role, "bid", "既有行升级后应回填默认角色 bid");

        // 同 hash 双角色并存（去重收窄为同角色后允许的形态）
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
             status, doc_role, created_at, updated_at)
             VALUES ('d2', 'w1', 'f', 'p', 'h', 'txt', 'parsed', 'tender', 't', 't')",
            [],
        )
        .unwrap();
        let tender: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE file_hash = 'h' AND doc_role = 'tender'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tender, 1);

        // 幂等：重跑不报错、版本不变
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn document_images_migration_cascades_and_is_idempotent() {
        // 老库（V14：document_images 表出现之前）升级：补 V15，表可写；删文档级联清空图片行
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap(); // 级联删除需显式开启
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..14] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 14).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
             status, created_at, updated_at)
             VALUES ('d1', 'w1', 'f', 'p', 'h', 'pdf', 'parsed', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO document_images (id, document_id, idx, source, page, width, height, sha256, dhash)
             VALUES ('i1', 'd1', 0, 'pdf', 3, 800, 600, 'abc', 123), ('i2', 'd1', 1, 'pdf', NULL, 800, 600, 'def', NULL)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_images WHERE document_id = 'd1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        // 删文档 → 图片行级联清空
        conn.execute("DELETE FROM documents WHERE id = 'd1'", []).unwrap();
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_images", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 0, "删文档后 document_images 应级联清空");

        // 幂等：重跑不报错、版本不变
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn chunk_exemptions_migration_cascades_and_is_idempotent() {
        // 老库（V15：chunk_exemptions 表出现之前）升级：补 V16，表可写；
        // 删任务级联清空豁免行、删文档亦级联清空（双外键）。
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..15] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 15).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
             status, created_at, updated_at)
             VALUES ('d1', 'w1', 'f', 'p', 'h', 'docx', 'parsed', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, document_id, chunk_type, chunk_level, text, normalized_text,
             order_index, created_at) VALUES ('c1', 'd1', 'paragraph', 'paragraph', 't', 't', 0, 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunk_exemptions (job_id, chunk_id, kind, coverage, spans_json)
             VALUES ('j1', 'c1', 'tender', 0.95, '[[0,30]]')",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunk_exemptions WHERE job_id='j1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // 删任务 → 豁免行级联清空
        conn.execute("DELETE FROM jobs WHERE id='j1'", []).unwrap();
        let after_job: i64 =
            conn.query_row("SELECT COUNT(*) FROM chunk_exemptions", [], |r| r.get(0)).unwrap();
        assert_eq!(after_job, 0, "删任务后 chunk_exemptions 应级联清空");

        // 删文档也级联（重插一行，删 chunk 所属文档）
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j2', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunk_exemptions (job_id, chunk_id, kind, coverage, spans_json)
             VALUES ('j2', 'c1', 'tender', 0.9, '[]')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM documents WHERE id='d1'", []).unwrap();
        let after_doc: i64 =
            conn.query_row("SELECT COUNT(*) FROM chunk_exemptions", [], |r| r.get(0)).unwrap();
        assert_eq!(after_doc, 0, "删文档后 chunk_exemptions 应级联清空");

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn verbatim_matches_migration_cascades_and_is_idempotent() {
        // 老库（V17：verbatim_matches 表出现之前）升级：补 V18，表可写；
        // 删任务级联清空（job_id 外键）、删文档亦级联清空（doc_a_id/doc_b_id 外键）。
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..17] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 17).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at)
                 VALUES (?1, 'w1', 'f', 'p', 'h', 'docx', 'parsed', 't', 't')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO verbatim_matches (id, job_id, doc_a_id, doc_b_id, a_start_chunk_id,
             a_start_offset, a_end_chunk_id, a_end_offset, b_start_chunk_id, b_start_offset,
             b_end_chunk_id, b_end_offset, char_len, sample_text, created_at)
             VALUES ('v1', 'j1', 'd1', 'd2', 'c1', 0, 'c2', 40, 'c3', 0, 'c4', 40, 100, '样本', 't')",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM verbatim_matches WHERE job_id='j1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // 删任务 → 级联清空
        conn.execute("DELETE FROM jobs WHERE id='j1'", []).unwrap();
        let after_job: i64 =
            conn.query_row("SELECT COUNT(*) FROM verbatim_matches", [], |r| r.get(0)).unwrap();
        assert_eq!(after_job, 0, "删任务后 verbatim_matches 应级联清空");

        // 删文档也级联（重插一行，删其中一份文档）
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j2', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO verbatim_matches (id, job_id, doc_a_id, doc_b_id, a_start_chunk_id,
             a_start_offset, a_end_chunk_id, a_end_offset, b_start_chunk_id, b_start_offset,
             b_end_chunk_id, b_end_offset, char_len, sample_text, created_at)
             VALUES ('v2', 'j2', 'd1', 'd2', 'c1', 0, 'c2', 40, 'c3', 0, 'c4', 40, 100, '样本', 't')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM documents WHERE id='d1'", []).unwrap();
        let after_doc: i64 =
            conn.query_row("SELECT COUNT(*) FROM verbatim_matches", [], |r| r.get(0)).unwrap();
        assert_eq!(after_doc, 0, "删文档后 verbatim_matches 应级联清空");

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn aligned_segments_migration_cascades_and_is_idempotent() {
        // 老库（V18：aligned_segments/segment_anchors 出现之前）升级：补 V19，两表可写；
        // 删任务级联清空 aligned_segments（job_id 外键），锚点随区段 FK 级联；删文档亦级联。
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..18] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 18).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at)
                 VALUES (?1, 'w1', 'f', 'p', 'h', 'docx', 'parsed', 't', 't')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO aligned_segments (id, job_id, doc_a_id, doc_b_id, a_start_order, a_end_order,
             b_start_order, b_end_order, a_start_chunk_id, a_end_chunk_id, b_start_chunk_id,
             b_end_chunk_id, anchor_count, verbatim_chars, a_covered_chars, b_covered_chars,
             a_coverage, b_coverage, avg_score, created_at)
             VALUES ('s1', 'j1', 'd1', 'd2', 0, 9, 0, 9, 'a0', 'a9', 'b0', 'b9', 10, 200, 400, 400,
             1.0, 1.0, 0.9, 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO segment_anchors (segment_id, a_chunk_id, b_chunk_id, kind, score)
             VALUES ('s1', 'a0', 'b0', 'verbatim', 1.0), ('s1', 'a1', 'b1', 'edge', 0.9)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment_anchors WHERE segment_id='s1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        // 删任务 → 区段与锚点级联清空
        conn.execute("DELETE FROM jobs WHERE id='j1'", []).unwrap();
        let seg_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM aligned_segments", [], |r| r.get(0)).unwrap();
        let anc_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_anchors", [], |r| r.get(0)).unwrap();
        assert_eq!(seg_after, 0, "删任务后 aligned_segments 应级联清空");
        assert_eq!(anc_after, 0, "区段删除后 segment_anchors 应随 FK 级联清空");

        // 删文档也级联（重插，删其中一份文档）
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j2', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO aligned_segments (id, job_id, doc_a_id, doc_b_id, a_start_order, a_end_order,
             b_start_order, b_end_order, a_start_chunk_id, a_end_chunk_id, b_start_chunk_id,
             b_end_chunk_id, anchor_count, verbatim_chars, a_covered_chars, b_covered_chars,
             a_coverage, b_coverage, avg_score, created_at)
             VALUES ('s2', 'j2', 'd1', 'd2', 0, 9, 0, 9, 'a0', 'a9', 'b0', 'b9', 10, 0, 400, 400,
             1.0, 1.0, 0.9, 't')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM documents WHERE id='d1'", []).unwrap();
        let seg_doc_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM aligned_segments", [], |r| r.get(0)).unwrap();
        assert_eq!(seg_doc_after, 0, "删文档后 aligned_segments 应级联清空");

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn segment_diffs_migration_cascades_and_is_idempotent() {
        // 老库（V19：segment_diffs 出现之前）升级补 V20，表可写；删区段 → segment_diffs 随 FK 级联。
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..19] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 19).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at)
                 VALUES (?1, 'w1', 'f', 'p', 'h', 'docx', 'parsed', 't', 't')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO aligned_segments (id, job_id, doc_a_id, doc_b_id, a_start_order, a_end_order,
             b_start_order, b_end_order, a_start_chunk_id, a_end_chunk_id, b_start_chunk_id,
             b_end_chunk_id, anchor_count, verbatim_chars, a_covered_chars, b_covered_chars,
             a_coverage, b_coverage, avg_score, created_at)
             VALUES ('s1', 'j1', 'd1', 'd2', 0, 9, 0, 9, 'a0', 'a9', 'b0', 'b9', 10, 0, 400, 400,
             1.0, 1.0, 0.9, 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO segment_diffs (id, segment_id, a_chunk_id, b_chunk_id, diff_type, diff_json,
             eq_chars, created_at)
             VALUES ('df1', 's1', 'a5', 'b5', 'gap-sentence', '[{\"op\":\"eq\",\"text\":\"甲\"}]', 1, 't')",
            [],
        )
        .unwrap();
        let n: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_diffs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        // 删区段 → segment_diffs 级联清空
        conn.execute("DELETE FROM aligned_segments WHERE id='s1'", []).unwrap();
        let after: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_diffs", [], |r| r.get(0)).unwrap();
        assert_eq!(after, 0, "删区段后 segment_diffs 应随 FK 级联清空");

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn boq_items_migration_cascades_and_is_idempotent() {
        // 老库（V20：boq_items 出现之前）升级补 V21，表可写；删任务/删文档均级联清空。
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..20] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 20).unwrap();
            tx.commit().unwrap();
        }
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at)
                 VALUES (?1, 'w1', 'f', 'p', 'h', 'xlsx', 'parsed', 't', 't')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        let ins = "INSERT INTO boq_items (id, job_id, doc_index, document_id, chunk_id, align_key,
             code, name, unit, qty, unit_price, total_price, row_index, page, flags, created_at)
             VALUES (?1, 'j1', ?2, ?3, ?4, 'c12:010101001001#0', '010101001001', '挖一般土方',
             'm3', 100.0, 25.5, 2550.0, 0, 1, NULL, 't')";
        conn.execute(ins, rusqlite::params!["b1", 0, "d1", "c1"]).unwrap();
        conn.execute(ins, rusqlite::params!["b2", 1, "d2", "c2"]).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM boq_items", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);

        // 删文档 → 该文档的清单条目级联清空
        conn.execute("DELETE FROM documents WHERE id='d1'", []).unwrap();
        let after_doc: i64 =
            conn.query_row("SELECT COUNT(*) FROM boq_items", [], |r| r.get(0)).unwrap();
        assert_eq!(after_doc, 1, "删文档后其 boq_items 应级联清空");

        // 删任务 → 全部清单条目级联清空
        conn.execute("DELETE FROM jobs WHERE id='j1'", []).unwrap();
        let after_job: i64 =
            conn.query_row("SELECT COUNT(*) FROM boq_items", [], |r| r.get(0)).unwrap();
        assert_eq!(after_job, 0, "删任务后 boq_items 应级联清空");

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn numeric_json_column_migration_is_idempotent_and_defaults_null() {
        // 老库（V21：numeric_json 出现之前）升级补 V22：旧任务行该列为 NULL，新值可写可读。
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..21] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 21).unwrap();
            tx.commit().unwrap();
        }
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j_old', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();

        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        let old: Option<String> = conn
            .query_row("SELECT numeric_json FROM jobs WHERE id='j_old'", [], |r| r.get(0))
            .unwrap();
        assert!(old.is_none(), "旧任务行 numeric_json 应为 NULL（前端据此隐藏数值面板）");

        conn.execute("UPDATE jobs SET numeric_json = ?1 WHERE id='j_old'", ["{\"pairs\":[]}"])
            .unwrap();
        let now: Option<String> = conn
            .query_row("SELECT numeric_json FROM jobs WHERE id='j_old'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(now.as_deref(), Some("{\"pairs\":[]}"));

        // 幂等
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn cluster_calibration_columns_migration_defaults_null_and_is_idempotent() {
        // 老库（V22：三列出现之前）升级补 V23：旧簇行三列均为 NULL（前端显示「未校准」），
        // 新值可写可读；重复 run 幂等。验收④「V23 升级后旧任务打开不报错、band 显示未校准」。
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..22] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 22).unwrap();
            tx.commit().unwrap();
        }
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1', '测试', 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j_old', 'w1', 'compare', 'completed', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clusters (id, job_id, cluster_type, severity, score, review_status, created_at)
             VALUES ('c_old', 'j_old', 'same', 'low', 0.9, 'pending', 't')",
            [],
        )
        .unwrap();

        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);

        let (conf, band, rr): (Option<f64>, Option<String>, Option<f64>) = conn
            .query_row("SELECT confidence, band, rerank_score FROM clusters WHERE id='c_old'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert!(conf.is_none() && band.is_none() && rr.is_none(), "旧簇行三列应为 NULL（前端显示「未校准」）");

        conn.execute(
            "UPDATE clusters SET confidence = 0.87, band = 'review', rerank_score = 0.5 WHERE id='c_old'",
            [],
        )
        .unwrap();
        let band: Option<String> = conn
            .query_row("SELECT band FROM clusters WHERE id='c_old'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(band.as_deref(), Some("review"));

        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn rejects_db_from_newer_app() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = run(&mut conn).unwrap_err();
        assert_eq!(err.code, crate::error::AppErrorCode::DatabaseError);
    }
}
