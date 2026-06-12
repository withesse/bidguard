// 分块仓储：chunks + chunk_features 成对写入与跨文档复制（hash 缓存复用）。
// 不在内部开事务——调用方负责把「分块写入 + 文档状态更新」包进同一个事务，
// 保证 status='parsed' 与 chunks 存在性永远一致。
use crate::db::now_iso;
use crate::error::AppResult;
use rusqlite::params;
use serde::Serialize;

/// 一条待入库的分块（与其特征）。由 engine::chunker 产出，特征在导入期一次备齐。
pub struct NewChunk {
    pub chunk_type: String,
    pub chunk_level: String,
    pub section_path: Option<String>,
    pub section_kind: Option<String>,
    pub is_template: bool,
    pub text: String,
    pub normalized_text: String,
    pub page: Option<u32>,
    pub order_index: i64,
    pub start_offset: Option<i64>,
    pub end_offset: Option<i64>,
    pub exact_hash: String,
    pub normalized_hash: String,
    pub token_json: Option<String>,
    pub entity_json: Option<String>,
    pub minhash_blob: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkRow {
    pub id: String,
    pub document_id: String,
    pub chunk_type: String,
    pub section_path: Option<String>,
    pub section_kind: Option<String>,
    pub text: String,
    pub page: Option<i64>,
    pub order_index: i64,
}

/// 写入一个文档的全部分块与特征。调用方需已开启事务。
pub fn insert_all(
    conn: &rusqlite::Connection,
    document_id: &str,
    chunks: &[NewChunk],
) -> AppResult<()> {
    let now = now_iso();
    let mut ins_chunk = conn.prepare(
        "INSERT INTO chunks (id, document_id, chunk_type, chunk_level, section_path, section_kind,
         is_template, text, normalized_text, char_count, page, order_index, start_offset,
         end_offset, exact_hash, normalized_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
    )?;
    let mut ins_feat = conn.prepare(
        "INSERT INTO chunk_features (chunk_id, token_json, entity_json, minhash_blob, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for c in chunks {
        let id = uuid::Uuid::new_v4().to_string();
        ins_chunk.execute(params![
            id,
            document_id,
            c.chunk_type,
            c.chunk_level,
            c.section_path,
            c.section_kind,
            c.is_template as i64,
            c.text,
            c.normalized_text,
            c.text.chars().count() as i64,
            c.page,
            c.order_index,
            c.start_offset,
            c.end_offset,
            c.exact_hash,
            c.normalized_hash,
            now,
        ])?;
        ins_feat.execute(params![id, c.token_json, c.entity_json, c.minhash_blob, now])?;
    }
    Ok(())
}

/// 比对期读取：某文档某粒度的全部分块 + 特征（按 order_index 排序）。
pub struct CompareChunkRow {
    pub id: String,
    pub order_index: i64,
    pub text: String,
    pub normalized_text: String,
    pub exact_hash: String,
    pub normalized_hash: String,
    pub section_path: Option<String>,
    pub section_kind: Option<String>,
    pub is_template: bool,
    pub page: Option<i64>,
    pub char_count: i64,
    pub token_json: Option<String>,
    pub entity_json: Option<String>,
    pub minhash_blob: Option<Vec<u8>>,
    pub chunk_type: String, // paragraph | sentence | section | table_row | list_item
}

pub fn load_for_compare(
    conn: &rusqlite::Connection,
    document_id: &str,
    chunk_level: &str,
) -> AppResult<Vec<CompareChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.order_index, c.text, c.normalized_text, c.exact_hash, c.normalized_hash,
         c.section_path, c.section_kind, c.is_template, c.page, c.char_count,
         f.token_json, f.entity_json, f.minhash_blob, c.chunk_type
         FROM chunks c LEFT JOIN chunk_features f ON f.chunk_id = c.id
         WHERE c.document_id = ?1 AND c.chunk_level = ?2 AND c.chunk_type != 'heading'
         ORDER BY c.order_index",
    )?;
    let rows = stmt
        .query_map(params![document_id, chunk_level], |r| {
            Ok(CompareChunkRow {
                id: r.get(0)?,
                order_index: r.get(1)?,
                text: r.get(2)?,
                normalized_text: r.get(3)?,
                exact_hash: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                normalized_hash: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                section_path: r.get(6)?,
                section_kind: r.get(7)?,
                is_template: r.get::<_, i64>(8)? != 0,
                page: r.get(9)?,
                char_count: r.get::<_, Option<i64>>(10)?.unwrap_or(0),
                token_json: r.get(11)?,
                entity_json: r.get(12)?,
                minhash_blob: r.get(13)?,
                chunk_type: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 把已解析文档的分块与特征复制给另一文档（同 file_hash 缓存复用，免重复解析）。
/// 调用方需已开启事务。
pub fn copy_all(conn: &rusqlite::Connection, from_doc: &str, to_doc: &str) -> AppResult<usize> {
    let now = now_iso();
    // 行数有限（单文档分块量级为千），读出改写 id 后重插，避免在 SQL 里造主键
    let mut sel = conn.prepare(
        "SELECT c.id, c.chunk_type, c.chunk_level, c.section_path, c.section_kind, c.is_template,
         c.text, c.normalized_text, c.char_count, c.page, c.order_index, c.start_offset,
         c.end_offset, c.exact_hash, c.normalized_hash, f.token_json, f.char_ngram_json,
         f.entity_json, f.minhash_blob, f.extra_json
         FROM chunks c LEFT JOIN chunk_features f ON f.chunk_id = c.id
         WHERE c.document_id = ?1 ORDER BY c.order_index",
    )?;
    let rows: Vec<_> = sel
        .query_map([from_doc], |r| {
            Ok((
                r.get::<_, String>(1)?,         // chunk_type
                r.get::<_, String>(2)?,         // chunk_level
                r.get::<_, Option<String>>(3)?, // section_path
                r.get::<_, Option<String>>(4)?, // section_kind
                r.get::<_, i64>(5)?,            // is_template
                r.get::<_, String>(6)?,         // text
                r.get::<_, String>(7)?,         // normalized_text
                r.get::<_, Option<i64>>(8)?,    // char_count
                r.get::<_, Option<i64>>(9)?,    // page
                r.get::<_, i64>(10)?,           // order_index
                r.get::<_, Option<i64>>(11)?,   // start_offset
                r.get::<_, Option<i64>>(12)?,   // end_offset
                r.get::<_, Option<String>>(13)?, // exact_hash
                r.get::<_, Option<String>>(14)?, // normalized_hash
                (
                    r.get::<_, Option<String>>(15)?,  // token_json
                    r.get::<_, Option<String>>(16)?,  // char_ngram_json
                    r.get::<_, Option<String>>(17)?,  // entity_json
                    r.get::<_, Option<Vec<u8>>>(18)?, // minhash_blob
                    r.get::<_, Option<String>>(19)?,  // extra_json
                ),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut ins_chunk = conn.prepare(
        "INSERT INTO chunks (id, document_id, chunk_type, chunk_level, section_path, section_kind,
         is_template, text, normalized_text, char_count, page, order_index, start_offset,
         end_offset, exact_hash, normalized_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
    )?;
    let mut ins_feat = conn.prepare(
        "INSERT INTO chunk_features (chunk_id, token_json, char_ngram_json, entity_json,
         minhash_blob, extra_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let n = rows.len();
    for row in rows {
        let id = uuid::Uuid::new_v4().to_string();
        let (ct, cl, sp, sk, tpl, text, norm, cc, page, oi, so, eo, eh, nh, feat) = row;
        ins_chunk.execute(params![
            id, to_doc, ct, cl, sp, sk, tpl, text, norm, cc, page, oi, so, eo, eh, nh, now
        ])?;
        ins_feat.execute(params![id, feat.0, feat.1, feat.2, feat.3, feat.4, now])?;
    }
    Ok(n)
}

/// 文档预览：按顺序取段落级前 limit 个分块（含标题块，反映文档结构）。
pub fn preview(
    conn: &rusqlite::Connection,
    document_id: &str,
    limit: usize,
) -> AppResult<Vec<ChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, chunk_type, section_path, section_kind, text, page, order_index
         FROM chunks WHERE document_id = ?1 AND chunk_level = 'paragraph'
         ORDER BY order_index LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![document_id, limit as i64], |r| {
            Ok(ChunkRow {
                id: r.get(0)?,
                document_id: r.get(1)?,
                chunk_type: r.get(2)?,
                section_path: r.get(3)?,
                section_kind: r.get(4)?,
                text: r.get(5)?,
                page: r.get(6)?,
                order_index: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
