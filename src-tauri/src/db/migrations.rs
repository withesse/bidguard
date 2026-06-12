// 数据库迁移：基于 PRAGMA user_version 的顺序迁移。
// 规则：MIGRATIONS[i] 把库从 version i 升到 i+1；已发布的迁移只增不改。
use crate::error::{AppError, AppErrorCode, AppResult};
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[SCHEMA_V1, SEED_TEMPLATES_V2, PARSE_OPTIONS_V3, CLUSTER_LOCATION_V4, PREVIEW_NOTES_V5];

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
        // V2：默认查重源模板已就位
        let tpl: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_templates WHERE enabled = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tpl, 3, "应内置 3 条默认模板");
    }

    #[test]
    fn rejects_db_from_newer_app() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = run(&mut conn).unwrap_err();
        assert_eq!(err.code, crate::error::AppErrorCode::DatabaseError);
    }
}
