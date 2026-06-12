// 配置分层：内置默认 < 用户全局(app_settings) < 工作区(settings_json) < 单次任务。
// 各层以 JSON patch 形式存储，resolve() 深合并后反序列化为强类型配置。
use crate::error::{AppError, AppErrorCode, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一次比对参与文档数上限（前端标签用十天干，经 get_app_info 下发，前端不得硬编码）。
pub const MAX_DOCS: usize = 10;
pub const MIN_DOCS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompareDefaults {
    pub default_chunk_level: String, // section | paragraph | sentence
    pub similarity_threshold: f32,
    pub candidate_top_k: usize,
    pub enable_semantic: bool,
    pub enable_fact_conflict: bool,
    pub ignore_whitespace: bool,
    pub ignore_punctuation: bool,
    pub ignore_case: bool,
    pub detect_moved_paragraph: bool,
    pub scope: String, // full | tech | business（比对范围，BidGuard 扩展项）
    pub ignore_templates: bool, // 剔除查重源模板段落（BidGuard 扩展项）
    /// 分词语言：auto（按 CJK 占比逐块判定）| zh（恒 jieba）| en（恒单词切分）。
    pub language: String,
}

impl Default for CompareDefaults {
    fn default() -> Self {
        Self {
            default_chunk_level: "paragraph".into(),
            similarity_threshold: 0.7,
            candidate_top_k: 100,
            enable_semantic: false,
            enable_fact_conflict: true,
            ignore_whitespace: true,
            ignore_punctuation: true,
            ignore_case: true,
            detect_moved_paragraph: true,
            scope: "full".into(),
            ignore_templates: true,
            language: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParserDefaults {
    pub remove_header_footer: bool,
    pub preserve_page_number: bool,
    pub detect_table: bool,
    pub min_paragraph_length: usize,
}

impl Default for ParserDefaults {
    fn default() -> Self {
        Self {
            remove_header_footer: true,
            preserve_page_number: true,
            detect_table: true,
            min_paragraph_length: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportDefaults {
    pub default_format: String,
    pub include_raw_text: bool,
    pub include_config: bool,
}

impl Default for ExportDefaults {
    fn default() -> Self {
        Self {
            default_format: "html".into(),
            include_raw_text: true,
            include_config: true,
        }
    }
}

// 注：日志「永不记录标书正文」是固定承诺（§15.2，tauri-plugin-log 只记任务 ID/错误码/摘要），
// 不设开关——可配置反而暗示存在记录正文的路径。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecurityDefaults {
    /// 是否允许联网下载语义模型（首次启用语义查重时需要；本地有缓存则不受限）。
    pub allow_cloud_model: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub compare: CompareDefaults,
    pub parser: ParserDefaults,
    pub export: ExportDefaults,
    pub security: SecurityDefaults,
}

/// 深合并 JSON patch：对象递归合并；null 不覆盖已有值（表示「未设置」）；其余以 patch 为准。
pub fn merge_json(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    continue;
                }
                match b.get_mut(k) {
                    Some(slot) => merge_json(slot, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => {
            if !p.is_null() {
                *b = p.clone();
            }
        }
    }
}

/// 四层合并出生效配置。任一层类型不合法（如把数字写成字符串）报 InvalidConfig，
/// 不静默回落，避免用户以为设置生效了。
pub fn resolve(
    user: Option<&Value>,
    workspace: Option<&Value>,
    task: Option<&Value>,
) -> AppResult<AppConfig> {
    let mut merged =
        serde_json::to_value(AppConfig::default()).expect("内置默认配置必然可序列化");
    for patch in [user, workspace, task].into_iter().flatten() {
        merge_json(&mut merged, patch);
    }
    serde_json::from_value(merged)
        .map_err(|e| AppError::new(AppErrorCode::InvalidConfig, "配置不合法").with_detail(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_defaults_match_design_doc() {
        let c = AppConfig::default();
        assert_eq!(c.compare.default_chunk_level, "paragraph");
        assert_eq!(c.compare.similarity_threshold, 0.7);
        assert_eq!(c.compare.candidate_top_k, 100);
        assert!(!c.compare.enable_semantic);
        assert!(c.compare.enable_fact_conflict);
        assert_eq!(c.parser.min_paragraph_length, 10);
        assert_eq!(c.export.default_format, "html");
        assert!(!c.security.allow_cloud_model);
    }

    #[test]
    fn later_layers_win() {
        let user = json!({ "compare": { "similarityThreshold": 0.5, "scope": "tech" } });
        let ws = json!({ "compare": { "similarityThreshold": 0.6 } });
        let task = json!({ "compare": { "similarityThreshold": 0.65 } });
        let c = resolve(Some(&user), Some(&ws), Some(&task)).unwrap();
        assert_eq!(c.compare.similarity_threshold, 0.65);
        // 任务层没动 scope，应保留用户层的值
        assert_eq!(c.compare.scope, "tech");
        // 其余字段保持内置默认
        assert_eq!(c.compare.candidate_top_k, 100);
    }

    #[test]
    fn null_does_not_override() {
        let user = json!({ "compare": { "scope": "business" } });
        let task = json!({ "compare": { "scope": null } });
        let c = resolve(Some(&user), None, Some(&task)).unwrap();
        assert_eq!(c.compare.scope, "business");
    }

    #[test]
    fn unknown_keys_are_ignored_but_bad_types_fail() {
        let ok = json!({ "compare": { "futureOption": true } });
        assert!(resolve(Some(&ok), None, None).is_ok());

        let bad = json!({ "compare": { "similarityThreshold": "高" } });
        let err = resolve(Some(&bad), None, None).unwrap_err();
        assert_eq!(err.code, AppErrorCode::InvalidConfig);
    }
}
