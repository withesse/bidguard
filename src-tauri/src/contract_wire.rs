// IPC 线协议契约（Rust 半边）：把高流量 DTO 的 serde 输出键集锁进测试。
// TS 半边（src/api/contract.test.ts）只能拦【前端】改名——其头注自述无法发现 Rust 侧改名；
// 本文件补上那个方向：Rust 字段改名/增删会在此失败，强迫同步 src/api/types.ts 与 TS 样本。
// 两边样本都是手写镜像（tauri-specta 类代码生成前的低成本护栏），改线协议时【两处一起改】。
use serde::Serialize;

/// 断言 DTO 序列化后的键集与预期完全一致（排序后比对，多键/少键/改名都会失败）。
fn assert_wire_keys<T: Serialize>(dto: &T, expect: &[&str], label: &str) {
    let v = serde_json::to_value(dto).expect("序列化不应失败");
    let obj = v.as_object().unwrap_or_else(|| panic!("{label} 应序列化为 JSON 对象"));
    let mut got: Vec<String> = obj.keys().cloned().collect();
    got.sort();
    let mut want: Vec<String> = expect.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        got, want,
        "{label} 线协议键集漂移——请同步 src/api/types.ts 与 src/api/contract.test.ts"
    );
}

#[test]
fn workspace_row_wire_shape() {
    let dto = crate::db::repo::workspace_repo::WorkspaceRow {
        id: "w1".into(),
        name: "评标项目".into(),
        created_at: "2026-08-26T00:00:00.000Z".into(),
        updated_at: "2026-08-26T00:00:00.000Z".into(),
        settings_json: Some("{}".into()), // skip_serializing_if：None 时无此键（TS 侧为可选 settingsJson?）
        document_count: 3,
        latest_job_status: Some("completed".into()),
    };
    assert_wire_keys(
        &dto,
        &["id", "name", "createdAt", "updatedAt", "settingsJson", "documentCount", "latestJobStatus"],
        "WorkspaceDto",
    );
}

#[test]
fn document_row_wire_shape() {
    let dto = crate::db::repo::document_repo::DocumentRow {
        id: "d1".into(),
        workspace_id: "w1".into(),
        file_name: "bid.docx".into(),
        file_path: "/x/bid.docx".into(),
        file_hash: "abc".into(),
        file_type: "docx".into(),
        status: "parsed".into(),
        parse_error: None,
        parse_method: Some("docx".into()),
        page_count: Some(12),
        char_count: Some(3400),
        fingerprint_json: None,
        chunk_count: 42,
        created_at: "2026-08-26T00:00:00.000Z".into(),
        updated_at: "2026-08-26T00:00:00.000Z".into(),
        truncation_notice: None,
        evasion_json: None,
        evasion_summary: None,
        doc_role: "bid".into(),
    };
    // 注：线协议含 evasionJson（原始规避统计 JSON），TS DocumentDto 刻意未声明该键
    //（前端只消费判级后的 evasionSummary）——多出的键无害，但属于协议面，记录在此。
    assert_wire_keys(
        &dto,
        &[
            "id", "workspaceId", "fileName", "filePath", "fileHash", "fileType", "status",
            "parseError", "parseMethod", "pageCount", "charCount", "fingerprintJson", "chunkCount",
            "createdAt", "updatedAt", "truncationNotice", "evasionJson", "evasionSummary", "docRole",
        ],
        "DocumentDto",
    );
}

#[test]
fn job_row_wire_shape() {
    let dto = crate::db::repo::job_repo::JobRow {
        id: "j1".into(),
        workspace_id: "w1".into(),
        job_type: "compare".into(),
        name: None,
        status: "completed".into(),
        config_json: "{}".into(),
        progress: 1.0,
        message: None,
        error_message: None,
        error_code: None,
        starred: false,
        matrix_json: None,
        collusion_level: Some("none".into()),
        created_at: "2026-08-26T00:00:00.000Z".into(),
        started_at: None,
        finished_at: None,
    };
    assert_wire_keys(
        &dto,
        &[
            "id", "workspaceId", "jobType", "name", "status", "configJson", "progress", "message",
            "errorMessage", "errorCode", "starred", "matrixJson", "collusionLevel", "createdAt",
            "startedAt", "finishedAt",
        ],
        "JobDto",
    );
}

#[test]
fn annotation_row_wire_shape() {
    let dto = crate::db::repo::annotation_repo::AnnotationRow {
        id: "a1".into(),
        workspace_id: "w1".into(),
        document_id: Some("d1".into()),
        chunk_id: None,
        cluster_id: None,
        page: Some(3),
        quote: Some("原文引用".into()),
        note: "复核意见".into(),
        created_at: "2026-08-26T00:00:00.000Z".into(),
        updated_at: "2026-08-26T00:00:00.000Z".into(),
    };
    assert_wire_keys(
        &dto,
        &[
            "id", "workspaceId", "documentId", "chunkId", "clusterId", "page", "quote", "note",
            "createdAt", "updatedAt",
        ],
        "AnnotationDto",
    );
}

#[test]
fn template_row_wire_shape() {
    let dto = crate::db::repo::template_repo::TemplateRow {
        id: "t1".into(),
        name: "法规引用".into(),
        text: "依据《招标投标法》……".into(),
        category: None,
        enabled: true,
        created_at: "2026-08-26T00:00:00.000Z".into(),
        hit_count: 2,
    };
    assert_wire_keys(
        &dto,
        &["id", "name", "text", "category", "enabled", "createdAt", "hitCount"],
        "TemplateDto",
    );
}

#[test]
fn cluster_summary_row_wire_shape() {
    let dto = crate::db::repo::compare_repo::ClusterSummaryRow {
        id: "c1".into(),
        job_id: "j1".into(),
        cluster_type: "same".into(),
        topic: None,
        summary: Some("整段一致".into()),
        severity: Some("high".into()),
        score: Some(0.98),
        section_kind: Some("tech".into()),
        review_status: "pending".into(),
        section_path: Some("第一章 › 1.1".into()),
        page: Some(5),
        document_ids: vec!["d1".into(), "d2".into()],
        member_count: 2,
        exempt_reason: None,
        multi_doc_anomaly: false,
        confidence: Some(0.87),
        band: Some("review".into()),
        rerank_score: None,
    };
    assert_wire_keys(
        &dto,
        &[
            "id", "jobId", "clusterType", "topic", "summary", "severity", "score", "sectionKind",
            "reviewStatus", "sectionPath", "page", "documentIds", "memberCount", "exemptReason",
            "multiDocAnomaly", "confidence", "band", "rerankScore",
        ],
        "ClusterSummaryDto",
    );
}

#[test]
fn segment_summary_row_wire_shape() {
    let dto = crate::db::repo::segment_repo::SegmentSummaryRow {
        id: "s1".into(),
        doc_a_id: "d1".into(),
        doc_b_id: "d2".into(),
        anchor_count: 14,
        verbatim_chars: 620,
        a_covered_chars: 1800,
        b_covered_chars: 1750,
        a_coverage: 0.82,
        b_coverage: 0.80,
        avg_score: 0.91,
        a_section_path: Some("第三章 › 3.2".into()),
        b_section_path: Some("第三章 › 3.2".into()),
        a_page_start: Some(12),
        a_page_end: Some(15),
        b_page_start: Some(11),
        b_page_end: Some(14),
    };
    assert_wire_keys(
        &dto,
        &[
            "id", "docAId", "docBId", "anchorCount", "verbatimChars", "aCoveredChars",
            "bCoveredChars", "aCoverage", "bCoverage", "avgScore", "aSectionPath", "bSectionPath",
            "aPageStart", "aPageEnd", "bPageStart", "bPageEnd",
        ],
        "AlignedSegmentDto",
    );
}

#[test]
fn tools_dtos_wire_shape() {
    let embed = crate::commands::tools::EmbedModelStatus {
        key: "bge-zh".into(),
        label: "bge-large-zh".into(),
        cached: true,
        size_bytes: 1024,
    };
    assert_wire_keys(&embed, &["key", "label", "cached", "sizeBytes"], "EmbedModelStatus");

    let rerank = crate::commands::tools::RerankModelStatus {
        key: "bge-reranker-base-int8".into(),
        label: "复核模型".into(),
        size_label: "~300MB".into(),
        cached: false,
        size_bytes: 0,
    };
    assert_wire_keys(&rerank, &["key", "label", "sizeLabel", "cached", "sizeBytes"], "RerankModelStatus");

    let status = crate::commands::tools::ModelStatus {
        ocr_present: true,
        ocr_location: Some("/models".into()),
        embed_cache_dir: None,
        embedding_models: vec![embed],
        rerank_cache_dir: None,
        rerank_models: vec![rerank],
    };
    assert_wire_keys(
        &status,
        &["ocrPresent", "ocrLocation", "embedCacheDir", "embeddingModels", "rerankCacheDir", "rerankModels"],
        "ModelStatusDto",
    );

    let storage = crate::commands::tools::StorageInfo {
        db_bytes: 1_000_000,
        embedding_rows: 42,
        document_count: 5,
        job_count: 3,
    };
    assert_wire_keys(&storage, &["dbBytes", "embeddingRows", "documentCount", "jobCount"], "StorageInfoDto");

    let diag = crate::commands::tools::DiagnosticItem {
        key: "pdfium".into(),
        label: "PDF 引擎".into(),
        ok: true,
        detail: "已就绪".into(),
    };
    assert_wire_keys(&diag, &["key", "label", "ok", "detail"], "DiagnosticItem");
}

#[test]
fn app_info_wire_shape() {
    let dto = crate::commands::settings::AppInfo {
        version: "0.6.0".into(),
        build_sha: "abc123def456".into(),
        log_dir: Some("/logs".into()),
        max_docs: 10,
        min_docs: 2,
        embedding_models: Vec::new(),
        ocr_models: Vec::new(),
        default_ocr_model: "v6-small".into(),
        calibration: Default::default(),
    };
    assert_wire_keys(
        &dto,
        &[
            "version", "buildSha", "logDir", "maxDocs", "minDocs", "embeddingModels", "ocrModels",
            "defaultOcrModel", "calibration",
        ],
        "AppInfoDto",
    );
}

#[test]
fn license_status_wire_shape() {
    let dto = crate::license::LicenseStatus {
        state: "trial".into(),
        active: true,
        plan: Some("trial".into()),
        licensee_name: None,
        expires_at: None,
        remaining_uses: Some(10),
        used_uses: Some(0),
        trial_expires_at: None,
        machine_code: "BG2-XXXXX".into(),
        clock_tamper: false,
        tamper: false,
        message: None,
    };
    assert_wire_keys(
        &dto,
        &[
            "state", "active", "plan", "licenseeName", "expiresAt", "remainingUses", "usedUses",
            "trialExpiresAt", "machineCode", "clockTamper", "tamper", "message",
        ],
        "LicenseStatusDto",
    );
}

#[test]
fn compare_summary_wire_shape() {
    let dto = crate::services::compare_service::CompareSummary::default();
    assert_wire_keys(
        &dto,
        &[
            "documentCount", "chunkCount", "clusterCount", "sameCount", "minorChangeCount",
            "rewriteCount", "changedCount", "addedCount", "deletedCount", "conflictCount",
            "uncertainCount", "highRiskCount", "semanticDegraded", "tenderRefChunkCount",
            "backgroundExemptChunkCount", "zoneLegalCount", "zonePriceCount", "zoneTechCount",
            "zoneBusinessCount", "zoneOtherCount", "boqItemCount", "boqAlignedItemCount",
            "boqAlignRate", "boqTableCount", "boqSkippedTableCount", "bandPassCount",
            "bandReviewCount", "bandFlagCount", "bandUncalibratedCount", "calibrationVersion",
            "calibrationKind", "calibrationRouting", "calibrationAlpha", "calibrationBeta",
            "rerankDegraded", "rerankReviewedCount",
        ],
        "CompareSummary",
    );
}
