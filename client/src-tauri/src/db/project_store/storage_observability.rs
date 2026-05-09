use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value as JsonValue};

use crate::{
    commands::CommandError,
    db::{database_path_for_repo, project_app_data_dir_for_repo},
};

use super::{
    agent_memory_lance, export_agent_runtime_audit,
    lance_health::{LanceTableFreshnessCounts, LanceTableHealthReport},
    list_active_agent_capability_revocations, list_cross_store_outbox_by_status,
    open_runtime_database, project_record_lance, validate_non_empty_text,
};

const LANCE_HEALTH_STATS_LATENCY_WARNING_MS: u64 = 500;
const PROJECT_PERFORMANCE_BUDGETS: &[(&str, u64, &str, &str)] = &[
    (
        "project_open",
        1_500,
        "startup diagnostics and project-state open",
        "warning",
    ),
    (
        "agent_selection",
        250,
        "agent definition lookup and activation preflight",
        "warning",
    ),
    (
        "custom_detail_load",
        300,
        "custom agent definition hydration",
        "warning",
    ),
    (
        "retrieval_latency",
        750,
        "project-record and approved-memory retrieval",
        "warning",
    ),
    (
        "memory_review_query",
        500,
        "memory candidate and approved-memory support queries",
        "warning",
    ),
    (
        "handoff_preparation",
        2_000,
        "handoff bundle build, record write, and lineage update",
        "blocker",
    ),
    (
        "startup_diagnostics",
        1_000,
        "SQLite quick_check, migration status, and Lance health",
        "warning",
    ),
];
const PROJECT_PERFORMANCE_BENCHMARK_SCHEMA: &str = "xero.project_performance_benchmark.v1";
const PROJECT_PERFORMANCE_REGRESSION_GRACE_MS: u64 = 25;
const PROJECT_PERFORMANCE_REGRESSION_GRACE_PERCENT: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStorageObservabilityReport {
    pub project_id: String,
    pub database_path: PathBuf,
    pub app_data_dir: PathBuf,
    pub lance_dir: PathBuf,
    pub migration_version: i64,
    pub state_file_bytes: u64,
    pub app_data_bytes: u64,
    pub(crate) project_record_health: LanceTableHealthReport,
    pub(crate) agent_memory_health: LanceTableHealthReport,
    pub project_record_health_status: String,
    pub agent_memory_health_status: String,
    pub retrieval_health_status: String,
    pub pending_outbox_count: usize,
    pub failed_reconciliation_count: usize,
    pub last_successful_maintenance_at: Option<String>,
    pub diagnostics: Vec<ProjectStorageDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStorageDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPerformanceBudgetReport {
    pub project_id: String,
    pub budgets: Vec<ProjectPerformanceBudget>,
    pub diagnostics: Vec<ProjectStorageDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPerformanceBudget {
    pub operation: String,
    pub budget_ms: u64,
    pub measurement_source: String,
    pub enforcement: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPerformanceBenchmarkMeasurement {
    pub operation: String,
    pub observed_ms: u64,
    pub sample_count: u64,
    pub measured_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPerformanceBenchmarkObservation {
    pub operation: String,
    pub budget_ms: u64,
    pub observed_ms: Option<u64>,
    pub sample_count: Option<u64>,
    pub over_budget_ms: u64,
    pub failure_threshold_ms: u64,
    pub measurement_source: String,
    pub enforcement: String,
    pub status: String,
    pub measured_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPerformanceBenchmarkReport {
    pub schema: String,
    pub project_id: String,
    pub generated_at: String,
    pub regression_grace_ms: u64,
    pub regression_grace_percent: u64,
    pub observations: Vec<ProjectPerformanceBenchmarkObservation>,
    pub diagnostics: Vec<ProjectStorageDiagnostic>,
    pub failed_operations: Vec<String>,
}

pub(crate) fn record_project_storage_maintenance_success(
    repo_root: &Path,
    project_id: &str,
    maintenance_kind: &str,
    completed_at: &str,
    diagnostic: Option<JsonValue>,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId", "project_storage_project_required")?;
    validate_non_empty_text(
        maintenance_kind,
        "maintenanceKind",
        "project_storage_maintenance_kind_required",
    )?;
    validate_non_empty_text(
        completed_at,
        "completedAt",
        "project_storage_maintenance_completed_at_required",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let run_id = format!(
        "storage-maintenance-{}-{}",
        maintenance_kind,
        completed_at
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
    );
    let diagnostic_json = diagnostic
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "project_storage_maintenance_diagnostic_encode_failed",
                format!("Xero could not encode project storage maintenance diagnostic: {error}"),
            )
        })?;
    connection
        .execute(
            r#"
            INSERT INTO project_storage_maintenance_runs (
                run_id,
                project_id,
                maintenance_kind,
                status,
                diagnostic_json,
                started_at,
                completed_at
            )
            VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5, ?5)
            ON CONFLICT(project_id, run_id)
            DO UPDATE SET
                status = 'succeeded',
                diagnostic_json = excluded.diagnostic_json,
                completed_at = excluded.completed_at
            "#,
            params![
                run_id,
                project_id,
                maintenance_kind,
                diagnostic_json,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "project_storage_maintenance_record_failed",
                format!(
                    "Xero could not record project storage maintenance in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

pub fn load_project_storage_observability(
    repo_root: &Path,
    project_id: &str,
) -> Result<ProjectStorageObservabilityReport, CommandError> {
    validate_non_empty_text(project_id, "projectId", "project_storage_project_required")?;
    let database_path = database_path_for_repo(repo_root);
    let app_data_dir = project_app_data_dir_for_repo(repo_root);
    let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let migration_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| {
            CommandError::retryable(
                "project_storage_migration_version_failed",
                format!(
                    "Xero could not read project-state migration version at {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let last_successful_maintenance_at: Option<String> = connection
        .query_row(
            r#"
            SELECT completed_at
            FROM project_storage_maintenance_runs
            WHERE project_id = ?1
              AND status = 'succeeded'
              AND completed_at IS NOT NULL
            ORDER BY completed_at DESC, id DESC
            LIMIT 1
            "#,
            params![project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                "project_storage_maintenance_read_failed",
                format!(
                    "Xero could not read project storage maintenance state at {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    drop(connection);

    let project_record_health =
        project_record_lance::open_for_database_path(&database_path, project_id).health_report()?;
    let agent_memory_health =
        agent_memory_lance::open_for_database_path(&database_path, project_id).health_report()?;
    let pending_outbox_count =
        list_cross_store_outbox_by_status(repo_root, project_id, "pending")?.len();
    let failed_reconciliation_count =
        list_cross_store_outbox_by_status(repo_root, project_id, "failed")?.len();
    let state_file_bytes = fs::metadata(&database_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let app_data_bytes = directory_bytes(&app_data_dir)?;
    let retrieval_health_status = retrieval_health_status(
        &project_record_health.status,
        &agent_memory_health.status,
        pending_outbox_count,
        failed_reconciliation_count,
    );

    let mut diagnostics = Vec::new();
    if pending_outbox_count > 0 {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_outbox_pending".into(),
            message: format!("{pending_outbox_count} cross-store outbox operation(s) are pending."),
            severity: "warning".into(),
        });
    }
    if failed_reconciliation_count > 0 {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_outbox_failed".into(),
            message: format!(
                "{failed_reconciliation_count} cross-store outbox operation(s) failed reconciliation."
            ),
            severity: "error".into(),
        });
    }
    for (table, status) in [
        ("project_records", &project_record_health.status),
        ("agent_memories", &agent_memory_health.status),
    ] {
        if status != "healthy" {
            diagnostics.push(ProjectStorageDiagnostic {
                code: "project_storage_lance_degraded".into(),
                message: format!("Lance table `{table}` reported `{status}`."),
                severity: "warning".into(),
            });
        }
    }
    push_lance_health_diagnostics(&mut diagnostics, &project_record_health);
    push_lance_health_diagnostics(&mut diagnostics, &agent_memory_health);

    Ok(ProjectStorageObservabilityReport {
        project_id: project_id.to_string(),
        database_path,
        app_data_dir,
        lance_dir,
        migration_version,
        state_file_bytes,
        app_data_bytes,
        project_record_health_status: project_record_health.status.clone(),
        agent_memory_health_status: agent_memory_health.status.clone(),
        project_record_health,
        agent_memory_health,
        retrieval_health_status,
        pending_outbox_count,
        failed_reconciliation_count,
        last_successful_maintenance_at,
        diagnostics,
    })
}

pub fn load_project_performance_budgets(
    repo_root: &Path,
    project_id: &str,
) -> Result<ProjectPerformanceBudgetReport, CommandError> {
    let storage = load_project_storage_observability(repo_root, project_id)?;
    let mut diagnostics = Vec::new();
    if storage.retrieval_health_status != "healthy" {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_performance_budget_retrieval_at_risk".into(),
            message: format!(
                "Retrieval performance budgets are at risk because retrieval health is `{}`.",
                storage.retrieval_health_status
            ),
            severity: "warning".into(),
        });
    }
    if storage
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
    {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_performance_budget_storage_error".into(),
            message:
                "Storage errors may invalidate project-open, retrieval, and startup diagnostics budgets."
                    .into(),
            severity: "error".into(),
        });
    }

    let budgets = PROJECT_PERFORMANCE_BUDGETS
        .iter()
        .map(|(operation, budget_ms, measurement_source, enforcement)| {
            let status = if *operation == "retrieval_latency"
                && storage.retrieval_health_status != "healthy"
            {
                "at_risk"
            } else if *operation == "startup_diagnostics"
                && storage
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == "error")
            {
                "at_risk"
            } else {
                "defined"
            };
            ProjectPerformanceBudget {
                operation: (*operation).into(),
                budget_ms: *budget_ms,
                measurement_source: (*measurement_source).into(),
                enforcement: (*enforcement).into(),
                status: status.into(),
            }
        })
        .collect();

    Ok(ProjectPerformanceBudgetReport {
        project_id: project_id.to_string(),
        budgets,
        diagnostics,
    })
}

pub fn evaluate_project_performance_benchmark(
    repo_root: &Path,
    project_id: &str,
    generated_at: &str,
    measurements: &[ProjectPerformanceBenchmarkMeasurement],
) -> Result<ProjectPerformanceBenchmarkReport, CommandError> {
    validate_non_empty_text(
        generated_at,
        "generatedAt",
        "project_performance_benchmark_generated_at_required",
    )?;
    for measurement in measurements {
        validate_non_empty_text(
            &measurement.operation,
            "operation",
            "project_performance_benchmark_operation_required",
        )?;
        if measurement.sample_count == 0 {
            return Err(CommandError::user_fixable(
                "project_performance_benchmark_sample_count_required",
                format!(
                    "Performance benchmark measurement `{}` must include at least one sample.",
                    measurement.operation
                ),
            ));
        }
    }

    let budget_report = load_project_performance_budgets(repo_root, project_id)?;
    let mut diagnostics = budget_report.diagnostics.clone();
    let mut failed_operations = Vec::new();
    let mut observations = Vec::new();

    if measurements.is_empty() {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_performance_benchmark_unmeasured".into(),
            message: "No benchmark measurements were supplied for the project performance budgets."
                .into(),
            severity: "warning".into(),
        });
    }

    for measurement in measurements {
        if !budget_report
            .budgets
            .iter()
            .any(|budget| budget.operation == measurement.operation)
        {
            diagnostics.push(ProjectStorageDiagnostic {
                code: "project_performance_benchmark_unknown_measurement".into(),
                message: format!(
                    "Benchmark measurement `{}` does not match a tracked project performance budget.",
                    measurement.operation
                ),
                severity: "warning".into(),
            });
        }
    }

    for budget in &budget_report.budgets {
        let measurement = measurements
            .iter()
            .filter(|measurement| measurement.operation == budget.operation)
            .max_by_key(|measurement| measurement.observed_ms);
        let failure_threshold_ms = project_performance_regression_threshold_ms(budget.budget_ms);
        let (observed_ms, sample_count, over_budget_ms, status, measured_at) =
            if let Some(measurement) = measurement {
                let over_budget_ms = measurement.observed_ms.saturating_sub(budget.budget_ms);
                let status = if measurement.observed_ms <= budget.budget_ms {
                    "within_budget"
                } else if measurement.observed_ms > failure_threshold_ms {
                    failed_operations.push(budget.operation.clone());
                    diagnostics.push(ProjectStorageDiagnostic {
                        code: "project_performance_benchmark_regression".into(),
                        message: format!(
                            "`{}` took {}ms, exceeding its {}ms budget and {}ms failure threshold.",
                            budget.operation,
                            measurement.observed_ms,
                            budget.budget_ms,
                            failure_threshold_ms
                        ),
                        severity: if budget.enforcement == "blocker" {
                            "error".into()
                        } else {
                            "warning".into()
                        },
                    });
                    "meaningful_regression"
                } else {
                    diagnostics.push(ProjectStorageDiagnostic {
                        code: "project_performance_benchmark_over_budget_within_grace".into(),
                        message: format!(
                        "`{}` took {}ms against a {}ms budget, within the {}ms failure threshold.",
                        budget.operation,
                        measurement.observed_ms,
                        budget.budget_ms,
                        failure_threshold_ms
                    ),
                        severity: "warning".into(),
                    });
                    "over_budget_within_grace"
                };
                (
                    Some(measurement.observed_ms),
                    Some(measurement.sample_count),
                    over_budget_ms,
                    status,
                    measurement.measured_at.clone(),
                )
            } else {
                (None, None, 0, "unmeasured", None)
            };

        observations.push(ProjectPerformanceBenchmarkObservation {
            operation: budget.operation.clone(),
            budget_ms: budget.budget_ms,
            observed_ms,
            sample_count,
            over_budget_ms,
            failure_threshold_ms,
            measurement_source: budget.measurement_source.clone(),
            enforcement: budget.enforcement.clone(),
            status: status.into(),
            measured_at,
        });
    }

    Ok(ProjectPerformanceBenchmarkReport {
        schema: PROJECT_PERFORMANCE_BENCHMARK_SCHEMA.into(),
        project_id: budget_report.project_id,
        generated_at: generated_at.into(),
        regression_grace_ms: PROJECT_PERFORMANCE_REGRESSION_GRACE_MS,
        regression_grace_percent: PROJECT_PERFORMANCE_REGRESSION_GRACE_PERCENT,
        observations,
        diagnostics,
        failed_operations,
    })
}

pub fn enforce_project_performance_benchmark(
    repo_root: &Path,
    project_id: &str,
    generated_at: &str,
    measurements: &[ProjectPerformanceBenchmarkMeasurement],
) -> Result<ProjectPerformanceBenchmarkReport, CommandError> {
    let report =
        evaluate_project_performance_benchmark(repo_root, project_id, generated_at, measurements)?;
    if !report.failed_operations.is_empty() {
        return Err(CommandError::system_fault(
            "project_performance_budget_regression",
            format!(
                "Project performance benchmark exceeded meaningful budgets for: {}.",
                report.failed_operations.join(", ")
            ),
        ));
    }
    Ok(report)
}

pub fn load_project_support_diagnostics_bundle(
    repo_root: &Path,
    project_id: &str,
    run_id: Option<&str>,
    generated_at: &str,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(project_id, "projectId", "project_support_project_required")?;
    validate_non_empty_text(
        generated_at,
        "generatedAt",
        "project_support_generated_at_required",
    )?;
    if let Some(run_id) = run_id {
        validate_non_empty_text(run_id, "runId", "project_support_run_required")?;
    }

    let storage = load_project_storage_observability(repo_root, project_id)?;
    let performance = load_project_performance_budgets(repo_root, project_id)?;
    let active_revocations = list_active_agent_capability_revocations(repo_root, project_id)?;
    let runtime_audit = match run_id {
        Some(run_id) => match export_agent_runtime_audit(repo_root, project_id, run_id) {
            Ok(export) => json!({
                "status": "available",
                "runId": run_id,
                "agentSessionId": export.agent_session_id,
                "runtimeAgentId": export.runtime_agent_id,
                "providerId": export.provider_id,
                "modelId": export.model_id,
                "agentDefinitionId": export.agent_definition_id,
                "agentDefinitionVersion": export.agent_definition_version,
                "contextManifestIds": export.context_manifest_ids,
                "toolPolicy": export.tool_policy,
                "memoryPolicy": export.memory_policy,
                "retrievalPolicy": export.retrieval_policy,
                "outputContract": export.output_contract,
                "handoffPolicy": export.handoff_policy,
                "capabilityPermissionExplanations": export.capability_permission_explanations,
                "riskyCapabilityApprovalCount": export.risky_capability_approvals.len(),
                "auditEvents": export.audit_events.iter().map(|event| json!({
                    "auditId": event.audit_id,
                    "actionKind": event.action_kind,
                    "subjectKind": event.subject_kind,
                    "subjectId": event.subject_id,
                    "riskClass": event.risk_class,
                    "approvalActionId": event.approval_action_id,
                    "createdAt": event.created_at,
                    "payload": event.payload,
                })).collect::<Vec<_>>(),
            }),
            Err(error) => json!({
                "status": "unavailable",
                "runId": run_id,
                "code": error.code,
                "message": error.message,
            }),
        },
        None => json!({
            "status": "not_requested",
            "runId": JsonValue::Null,
        }),
    };

    let bundle = json!({
        "schema": "xero.agent_support_diagnostics_bundle.v1",
        "projectId": project_id,
        "generatedAt": generated_at,
        "redactionState": "pending",
        "ui": {
            "newUiImplemented": false,
            "reason": "Backend diagnostics bundle only; visible support UI remains deferred by plan policy.",
            "deferredSurfaces": support_deferred_ui_surfaces(),
        },
        "storage": storage_report_json(&storage),
        "performanceBudgets": performance_budget_report_json(&performance),
        "failureAreas": support_failure_area_summary(
            &storage,
            &performance,
            active_revocations.len(),
            &runtime_audit,
        ),
        "capabilityRevocations": {
            "activeCount": active_revocations.len(),
            "active": active_revocations.iter().map(|revocation| json!({
                "revocationId": revocation.revocation_id,
                "subjectKind": revocation.subject_kind,
                "subjectId": revocation.subject_id,
                "status": revocation.status,
                "reason": revocation.reason,
                "createdBy": revocation.created_by,
                "createdAt": revocation.created_at,
                "scope": revocation.scope,
            })).collect::<Vec<_>>(),
        },
        "runtimeAudit": runtime_audit,
    });
    let (mut bundle, redacted) = crate::runtime::redaction::redact_json_for_persistence(&bundle);
    if let Some(fields) = bundle.as_object_mut() {
        fields.insert(
            "redactionState".into(),
            json!(if redacted { "redacted" } else { "clean" }),
        );
    }
    Ok(bundle)
}

fn support_deferred_ui_surfaces() -> JsonValue {
    json!([
        {
            "surface": "visual_authoring",
            "slices": ["S04", "S07-S13", "S25", "S62", "S63"],
            "status": "deferred_no_new_ui",
            "backendEvidence": [
                "authoring_catalog",
                "preview_agent_definition",
                "get_agent_definition_version_diff"
            ]
        },
        {
            "surface": "runtime_transparency",
            "slices": ["S15", "S46", "S52", "S64", "S66"],
            "status": "deferred_no_new_ui",
            "backendEvidence": [
                "get_agent_database_touchpoint_explanation",
                "get_agent_handoff_context_summary",
                "get_capability_permission_explanation",
                "get_agent_knowledge_inspection",
                "get_agent_knowledge_inspection(runId)",
                "get_agent_run_start_explanation"
            ]
        },
        {
            "surface": "user_control",
            "slices": ["S28", "S43", "S61", "S65"],
            "status": "deferred_no_new_ui",
            "backendEvidence": [
                "get_session_memory_review_queue",
                "update_session_memory",
                "correct_session_memory",
                "delete_session_memory",
                "create_project_state_backup",
                "restore_project_state_backup",
                "repair_project_state",
                "get_agent_support_diagnostics_bundle",
                "delete_project_context_record",
                "supersede_project_context_record"
            ]
        },
        {
            "surface": "representative_dogfood",
            "slices": ["S70"],
            "status": "blocked_until_ui_or_backend_only_acceptance",
            "backendEvidence": ["docs/agent-system-dogfood-notes.md"]
        }
    ])
}

fn support_failure_area_summary(
    storage: &ProjectStorageObservabilityReport,
    performance: &ProjectPerformanceBudgetReport,
    active_revocation_count: usize,
    runtime_audit: &JsonValue,
) -> JsonValue {
    let runtime_audit_status = runtime_audit
        .get("status")
        .and_then(JsonValue::as_str)
        .unwrap_or("unknown");
    json!({
        "visualBuilder": {
            "status": "ui_deferred",
            "signals": [
                "visible_builder_support_report_not_implemented",
                "custom_definition_audit_available_when_run_requested"
            ],
        },
        "runtimePolicy": {
            "status": support_runtime_policy_status(runtime_audit_status, active_revocation_count),
            "runtimeAuditStatus": runtime_audit_status,
            "activeRevocationCount": active_revocation_count,
            "signals": [
                "tool_policy",
                "memory_policy",
                "retrieval_policy",
                "handoff_policy",
                "capability_permission_explanations"
            ],
        },
        "storage": {
            "status": support_storage_area_status(storage),
            "diagnosticCount": storage.diagnostics.len(),
            "pendingOutboxCount": storage.pending_outbox_count,
            "failedReconciliationCount": storage.failed_reconciliation_count,
            "startupBudgetStatus": performance_budget_status(performance, "startup_diagnostics"),
            "projectOpenBudgetStatus": performance_budget_status(performance, "project_open"),
        },
        "retrieval": {
            "status": storage.retrieval_health_status,
            "projectRecordHealthStatus": storage.project_record_health_status,
            "agentMemoryHealthStatus": storage.agent_memory_health_status,
            "retrievalBudgetStatus": performance_budget_status(performance, "retrieval_latency"),
        },
        "memory": {
            "status": storage.agent_memory_health_status,
            "memoryReviewBudgetStatus": performance_budget_status(performance, "memory_review_query"),
            "freshness": lance_freshness_counts_json(&storage.agent_memory_health.freshness_counts),
        },
        "handoff": {
            "status": support_handoff_area_status(runtime_audit_status),
            "runtimeAuditStatus": runtime_audit_status,
            "handoffBudgetStatus": performance_budget_status(performance, "handoff_preparation"),
            "signals": [
                "handoff_policy",
                "context_manifest_ids",
                "handoff_preparation_budget"
            ],
        },
    })
}

fn support_runtime_policy_status(
    runtime_audit_status: &str,
    active_revocation_count: usize,
) -> &'static str {
    if active_revocation_count > 0 {
        "revoked_capabilities_present"
    } else {
        match runtime_audit_status {
            "available" => "available",
            "not_requested" => "not_requested",
            "unavailable" => "unavailable",
            _ => "unknown",
        }
    }
}

fn support_storage_area_status(storage: &ProjectStorageObservabilityReport) -> &'static str {
    if storage
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
        || storage.failed_reconciliation_count > 0
    {
        "failed"
    } else if storage.diagnostics.is_empty() && storage.pending_outbox_count == 0 {
        "healthy"
    } else {
        "warning"
    }
}

fn support_handoff_area_status(runtime_audit_status: &str) -> &'static str {
    match runtime_audit_status {
        "available" => "available",
        "not_requested" => "not_requested",
        "unavailable" => "unavailable",
        _ => "unknown",
    }
}

fn performance_budget_status(
    report: &ProjectPerformanceBudgetReport,
    operation: &str,
) -> JsonValue {
    report
        .budgets
        .iter()
        .find(|budget| budget.operation == operation)
        .map(|budget| json!(budget.status.clone()))
        .unwrap_or(JsonValue::Null)
}

fn storage_report_json(report: &ProjectStorageObservabilityReport) -> JsonValue {
    json!({
        "projectId": report.project_id,
        "migrationVersion": report.migration_version,
        "stateFileBytes": report.state_file_bytes,
        "appDataBytes": report.app_data_bytes,
        "projectRecordHealthStatus": report.project_record_health_status,
        "agentMemoryHealthStatus": report.agent_memory_health_status,
        "retrievalHealthStatus": report.retrieval_health_status,
        "pendingOutboxCount": report.pending_outbox_count,
        "failedReconciliationCount": report.failed_reconciliation_count,
        "lastSuccessfulMaintenanceAt": report.last_successful_maintenance_at,
        "lanceHealth": {
            "projectRecords": lance_health_report_json(&report.project_record_health),
            "agentMemories": lance_health_report_json(&report.agent_memory_health),
        },
        "diagnostics": report.diagnostics.iter().map(storage_diagnostic_json).collect::<Vec<_>>(),
    })
}

fn lance_health_report_json(report: &LanceTableHealthReport) -> JsonValue {
    json!({
        "tableName": report.table_name,
        "status": report.status,
        "schemaCurrent": report.schema_current,
        "version": report.version,
        "rowCount": report.row_count,
        "totalBytes": report.total_bytes,
        "indexCount": report.index_count,
        "fragmentCount": report.fragment_count,
        "smallFragmentCount": report.small_fragment_count,
        "statsLatencyMs": report.stats_latency_ms,
        "maintenanceRecommended": report.maintenance_recommended,
        "quarantineTableCount": report.quarantine_table_count,
        "diagnosticMarkerCount": report.diagnostic_marker_count,
        "freshness": lance_freshness_counts_json(&report.freshness_counts),
    })
}

fn lance_freshness_counts_json(counts: &LanceTableFreshnessCounts) -> JsonValue {
    json!({
        "inspectedRowCount": counts.inspected_row_count,
        "currentRowCount": counts.current_row_count,
        "sourceUnknownRowCount": counts.source_unknown_row_count,
        "staleRowCount": counts.stale_row_count,
        "sourceMissingRowCount": counts.source_missing_row_count,
        "supersededRowCount": counts.superseded_row_count,
        "blockedRowCount": counts.blocked_row_count,
        "retrievalDegradedRowCount": counts.retrieval_degraded_row_count(),
    })
}

fn performance_budget_report_json(report: &ProjectPerformanceBudgetReport) -> JsonValue {
    json!({
        "projectId": report.project_id,
        "budgets": report.budgets.iter().map(|budget| json!({
            "operation": budget.operation,
            "budgetMs": budget.budget_ms,
            "measurementSource": budget.measurement_source,
            "enforcement": budget.enforcement,
            "status": budget.status,
        })).collect::<Vec<_>>(),
        "diagnostics": report.diagnostics.iter().map(storage_diagnostic_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
fn performance_benchmark_report_json(report: &ProjectPerformanceBenchmarkReport) -> JsonValue {
    json!({
        "schema": report.schema,
        "projectId": report.project_id,
        "generatedAt": report.generated_at,
        "regressionGraceMs": report.regression_grace_ms,
        "regressionGracePercent": report.regression_grace_percent,
        "failureCount": report.failed_operations.len(),
        "failedOperations": report.failed_operations,
        "observations": report.observations.iter().map(|observation| json!({
            "operation": observation.operation,
            "budgetMs": observation.budget_ms,
            "observedMs": observation.observed_ms,
            "sampleCount": observation.sample_count,
            "overBudgetMs": observation.over_budget_ms,
            "failureThresholdMs": observation.failure_threshold_ms,
            "measurementSource": observation.measurement_source,
            "enforcement": observation.enforcement,
            "status": observation.status,
            "measuredAt": observation.measured_at,
        })).collect::<Vec<_>>(),
        "diagnostics": report.diagnostics.iter().map(storage_diagnostic_json).collect::<Vec<_>>(),
    })
}

fn storage_diagnostic_json(diagnostic: &ProjectStorageDiagnostic) -> JsonValue {
    json!({
        "code": diagnostic.code,
        "message": diagnostic.message,
        "severity": diagnostic.severity,
    })
}

fn retrieval_health_status(
    project_record_status: &str,
    agent_memory_status: &str,
    pending_outbox_count: usize,
    failed_reconciliation_count: usize,
) -> String {
    if failed_reconciliation_count > 0 {
        "degraded".into()
    } else if project_record_status == "healthy"
        && agent_memory_status == "healthy"
        && pending_outbox_count == 0
    {
        "healthy".into()
    } else {
        "maintenance_recommended".into()
    }
}

fn push_lance_health_diagnostics(
    diagnostics: &mut Vec<ProjectStorageDiagnostic>,
    health: &LanceTableHealthReport,
) {
    if health.maintenance_recommended {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_lance_maintenance_recommended".into(),
            message: format!(
                "Lance table `{}` has {} small fragment(s) across {} fragment(s); compaction/index maintenance is recommended.",
                health.table_name, health.small_fragment_count, health.fragment_count
            ),
            severity: "warning".into(),
        });
    }
    if health.stats_latency_ms > LANCE_HEALTH_STATS_LATENCY_WARNING_MS {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_lance_stats_slow".into(),
            message: format!(
                "Lance table `{}` statistics took {}ms, above the {}ms diagnostics budget.",
                health.table_name, health.stats_latency_ms, LANCE_HEALTH_STATS_LATENCY_WARNING_MS
            ),
            severity: "warning".into(),
        });
    }
    if health.quarantine_table_count > 0 || health.diagnostic_marker_count > 0 {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_lance_quarantine_artifacts".into(),
            message: format!(
                "Lance table `{}` has {} quarantined table(s) and {} schema-drift diagnostic marker(s).",
                health.table_name, health.quarantine_table_count, health.diagnostic_marker_count
            ),
            severity: "warning".into(),
        });
    }
    let degraded_row_count = health.freshness_counts.retrieval_degraded_row_count();
    if degraded_row_count > 0 {
        diagnostics.push(ProjectStorageDiagnostic {
            code: "project_storage_lance_stale_rows".into(),
            message: format!(
                "Lance table `{}` has {} stale retrieval row(s): {} stale, {} source-missing, {} superseded, and {} blocked.",
                health.table_name,
                degraded_row_count,
                health.freshness_counts.stale_row_count,
                health.freshness_counts.source_missing_row_count,
                health.freshness_counts.superseded_row_count,
                health.freshness_counts.blocked_row_count
            ),
            severity: "warning".into(),
        });
    }
}

fn project_performance_regression_threshold_ms(budget_ms: u64) -> u64 {
    let percentage_grace_ms =
        budget_ms.saturating_mul(PROJECT_PERFORMANCE_REGRESSION_GRACE_PERCENT) / 100;
    budget_ms.saturating_add(percentage_grace_ms.max(PROJECT_PERFORMANCE_REGRESSION_GRACE_MS))
}

fn directory_bytes(path: &Path) -> Result<u64, CommandError> {
    if !path.exists() {
        return Ok(0);
    }
    let mut byte_count = 0;
    for entry in fs::read_dir(path).map_err(|error| {
        CommandError::retryable(
            "project_storage_directory_read_failed",
            format!(
                "Xero could not inspect project storage directory {}: {error}",
                path.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "project_storage_directory_entry_failed",
                format!(
                    "Xero could not inspect an entry in project storage directory {}: {error}",
                    path.display()
                ),
            )
        })?;
        let metadata = entry.metadata().map_err(|error| {
            CommandError::retryable(
                "project_storage_metadata_failed",
                format!(
                    "Xero could not inspect project storage entry {}: {error}",
                    entry.path().display()
                ),
            )
        })?;
        if metadata.is_dir() {
            byte_count += directory_bytes(&entry.path())?;
        } else {
            byte_count += metadata.len();
        }
    }
    Ok(byte_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::{params, Connection};
    use serde_json::json;

    use crate::db::{
        configure_connection,
        migrations::migrations,
        project_store::{revoke_agent_capability, NewAgentCapabilityRevocationRecord},
        register_project_database_path,
    };

    fn create_project_database(repo_root: &Path, project_id: &str) -> PathBuf {
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent")).expect("database dir");
        let mut connection = Connection::open(&database_path).expect("open database");
        configure_connection(&connection).expect("configure database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        register_project_database_path(repo_root, &database_path);
        database_path
    }

    #[test]
    fn s44_storage_observability_reports_state_health_and_maintenance_time() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-storage-observability";
        let database_path = create_project_database(&repo_root, project_id);
        record_project_storage_maintenance_success(
            &repo_root,
            project_id,
            "repair",
            "2026-05-09T00:02:00Z",
            None,
        )
        .expect("record maintenance");

        let report = load_project_storage_observability(&repo_root, project_id)
            .expect("load storage observability");
        assert_eq!(report.project_id, project_id);
        assert_eq!(report.database_path, database_path);
        assert!(report.migration_version > 0);
        assert!(report.state_file_bytes > 0);
        assert_eq!(report.pending_outbox_count, 0);
        assert_eq!(report.failed_reconciliation_count, 0);
        assert_eq!(
            report.last_successful_maintenance_at.as_deref(),
            Some("2026-05-09T00:02:00Z")
        );
        assert!(!repo_root.join(".xero").exists());
    }

    #[test]
    fn s37_lance_health_diagnostics_explain_degraded_retrieval_storage() {
        let health = LanceTableHealthReport {
            table_name: "project_records".into(),
            status: "degraded".into(),
            schema_current: true,
            version: 7,
            row_count: 42,
            total_bytes: 2048,
            index_count: 1,
            fragment_count: 30,
            small_fragment_count: 25,
            stats_latency_ms: 750,
            maintenance_recommended: true,
            quarantine_table_count: 2,
            diagnostic_marker_count: 1,
            freshness_counts: LanceTableFreshnessCounts {
                inspected_row_count: 42,
                current_row_count: 36,
                source_unknown_row_count: 1,
                stale_row_count: 2,
                source_missing_row_count: 1,
                superseded_row_count: 1,
                blocked_row_count: 1,
            },
        };
        let mut diagnostics = Vec::new();
        push_lance_health_diagnostics(&mut diagnostics, &health);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "project_storage_lance_maintenance_recommended"
                && diagnostic.message.contains("25 small fragment")
        }));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "project_storage_lance_stats_slow"));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "project_storage_lance_quarantine_artifacts"
                && diagnostic.message.contains("2 quarantined")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "project_storage_lance_stale_rows"
                && diagnostic.message.contains("5 stale retrieval row")
        }));
        let health_json = lance_health_report_json(&health);
        assert_eq!(health_json["rowCount"], json!(42));
        assert_eq!(health_json["indexCount"], json!(1));
        assert_eq!(health_json["statsLatencyMs"], json!(750));
        assert_eq!(health_json["maintenanceRecommended"], json!(true));
        assert_eq!(health_json["freshness"]["staleRowCount"], json!(2));
        assert_eq!(
            health_json["freshness"]["retrievalDegradedRowCount"],
            json!(5)
        );
    }

    #[test]
    fn s60_project_performance_budgets_define_storage_and_retrieval_gates() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-performance-budgets";
        create_project_database(&repo_root, project_id);

        let report = load_project_performance_budgets(&repo_root, project_id)
            .expect("load performance budgets");
        assert_eq!(report.project_id, project_id);
        assert!(report
            .budgets
            .iter()
            .any(|budget| budget.operation == "project_open" && budget.budget_ms == 1_500));
        assert!(report
            .budgets
            .iter()
            .any(|budget| budget.operation == "retrieval_latency" && budget.budget_ms == 750));
        assert!(report
            .budgets
            .iter()
            .any(|budget| budget.operation == "handoff_preparation"
                && budget.enforcement == "blocker"));

        let benchmark = evaluate_project_performance_benchmark(
            &repo_root,
            project_id,
            "2026-05-09T00:05:00Z",
            &[ProjectPerformanceBenchmarkMeasurement {
                operation: "project_open".into(),
                observed_ms: 1_250,
                sample_count: 5,
                measured_at: Some("2026-05-09T00:05:00Z".into()),
            }],
        )
        .expect("evaluate performance benchmark");
        assert_eq!(benchmark.schema, PROJECT_PERFORMANCE_BENCHMARK_SCHEMA);
        assert_eq!(benchmark.failed_operations, Vec::<String>::new());
        assert_eq!(benchmark.observations.len(), report.budgets.len());
        let project_open = benchmark
            .observations
            .iter()
            .find(|observation| observation.operation == "project_open")
            .expect("project open observation");
        assert_eq!(project_open.status, "within_budget");
        assert_eq!(project_open.observed_ms, Some(1_250));
        let agent_selection = benchmark
            .observations
            .iter()
            .find(|observation| observation.operation == "agent_selection")
            .expect("agent selection observation");
        assert_eq!(agent_selection.status, "unmeasured");

        let benchmark_json = performance_benchmark_report_json(&benchmark);
        assert_eq!(
            benchmark_json["schema"],
            json!(PROJECT_PERFORMANCE_BENCHMARK_SCHEMA)
        );
        assert_eq!(benchmark_json["failureCount"], json!(0));
        assert_eq!(
            benchmark_json["observations"]
                .as_array()
                .expect("observations array")
                .len(),
            report.budgets.len()
        );
    }

    #[test]
    fn s60_project_performance_benchmark_fails_meaningful_regressions() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-performance-budget-regressions";
        create_project_database(&repo_root, project_id);

        let measurements = vec![
            ProjectPerformanceBenchmarkMeasurement {
                operation: "retrieval_latency".into(),
                observed_ms: 800,
                sample_count: 7,
                measured_at: Some("2026-05-09T00:06:00Z".into()),
            },
            ProjectPerformanceBenchmarkMeasurement {
                operation: "handoff_preparation".into(),
                observed_ms: 2_300,
                sample_count: 3,
                measured_at: Some("2026-05-09T00:06:00Z".into()),
            },
        ];
        let report = evaluate_project_performance_benchmark(
            &repo_root,
            project_id,
            "2026-05-09T00:06:00Z",
            &measurements,
        )
        .expect("evaluate regression benchmark");
        assert_eq!(
            report.failed_operations,
            vec!["handoff_preparation".to_string()]
        );
        let retrieval = report
            .observations
            .iter()
            .find(|observation| observation.operation == "retrieval_latency")
            .expect("retrieval observation");
        assert_eq!(retrieval.status, "over_budget_within_grace");
        let handoff = report
            .observations
            .iter()
            .find(|observation| observation.operation == "handoff_preparation")
            .expect("handoff observation");
        assert_eq!(handoff.status, "meaningful_regression");
        assert_eq!(handoff.over_budget_ms, 300);
        assert_eq!(handoff.failure_threshold_ms, 2_200);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "project_performance_benchmark_regression"));

        let output = performance_benchmark_report_json(&report);
        assert_eq!(output["failureCount"], json!(1));
        assert_eq!(output["failedOperations"], json!(["handoff_preparation"]));
        let error = enforce_project_performance_benchmark(
            &repo_root,
            project_id,
            "2026-05-09T00:06:00Z",
            &measurements,
        )
        .expect_err("meaningful regression should fail enforcement");
        assert_eq!(error.code, "project_performance_budget_regression");
    }

    #[test]
    fn s61_support_diagnostics_bundle_is_backend_only_and_redacted() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-support-diagnostics";
        create_project_database(&repo_root, project_id);
        revoke_agent_capability(
            &repo_root,
            &NewAgentCapabilityRevocationRecord {
                revocation_id: "revocation-support-diagnostics".into(),
                project_id: project_id.into(),
                subject_kind: "tool_pack".into(),
                subject_id: "external-network-tools".into(),
                scope: json!({ "api_key": "sk-test-secret-value" }),
                reason: "Disable after api_key=sk-test-secret-value appeared in a report.".into(),
                created_by: "support-user".into(),
                created_at: "2026-05-09T00:03:00Z".into(),
            },
        )
        .expect("record revocation");

        let bundle = load_project_support_diagnostics_bundle(
            &repo_root,
            project_id,
            None,
            "2026-05-09T00:04:00Z",
        )
        .expect("load support diagnostics");
        assert_eq!(
            bundle["schema"],
            json!("xero.agent_support_diagnostics_bundle.v1")
        );
        assert_eq!(bundle["ui"]["newUiImplemented"], json!(false));
        assert_eq!(
            bundle["ui"]["deferredSurfaces"][0]["surface"],
            json!("visual_authoring")
        );
        assert!(bundle["ui"]["deferredSurfaces"]
            .as_array()
            .expect("deferred UI surfaces")
            .iter()
            .any(|surface| surface["surface"] == json!("user_control")
                && surface["backendEvidence"]
                    .as_array()
                    .expect("backend evidence")
                    .contains(&json!("get_agent_support_diagnostics_bundle"))));
        assert_eq!(bundle["redactionState"], json!("redacted"));
        assert_eq!(bundle["capabilityRevocations"]["activeCount"], json!(1));
        assert_eq!(
            bundle["failureAreas"]["visualBuilder"]["status"],
            json!("ui_deferred")
        );
        assert_eq!(
            bundle["failureAreas"]["runtimePolicy"]["activeRevocationCount"],
            json!(1)
        );
        assert!(bundle["failureAreas"]["storage"]["status"].is_string());
        assert!(bundle["failureAreas"]["retrieval"]["status"].is_string());
        assert!(bundle["failureAreas"]["memory"]["freshness"]["inspectedRowCount"].is_number());
        assert!(bundle["failureAreas"]["handoff"]["handoffBudgetStatus"].is_string());
        assert!(!bundle.to_string().contains("sk-test-secret-value"));
    }
}
