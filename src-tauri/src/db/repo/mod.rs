// 仓储层：每个领域一个文件，函数式 API（传入连接，纯 SQL，无业务逻辑）。
pub mod annotation_repo;
pub mod chunk_repo;
pub mod compare_repo;
pub mod document_repo;
pub mod embedding_repo;
pub mod fact_repo;
pub mod job_repo;
pub mod settings_repo;
pub mod template_repo;
pub mod workspace_repo;
