use cadence_desktop_lib::{
    provider_models::{
        ProviderModelCatalog, ProviderModelCatalogDiagnostic, ProviderModelCatalogSource,
        ProviderModelRecord, ProviderModelThinkingCapability,
    },
    provider_profiles::{
        ProviderProfileCredentialLink, ProviderProfileReadinessProjection,
        ProviderProfileReadinessStatus, ProviderProfileRecord,
    },
    runtime::{
        ambient_auth_failure_diagnostic, invalid_base_url_diagnostic,
        provider_model_catalog_diagnostic, provider_profile_readiness_diagnostic,
        render_doctor_report, stale_runtime_binding_diagnostic, summarize_diagnostic_checks,
        unsupported_provider_diagnostic, validate_diagnostic_check, validate_doctor_report,
        CadenceDiagnosticCheck, CadenceDiagnosticCheckInput, CadenceDiagnosticSeverity,
        CadenceDiagnosticStatus, CadenceDiagnosticSubject, CadenceDoctorReport,
        CadenceDoctorReportInput, CadenceDoctorReportMode, CadenceDoctorReportOutputMode,
        CadenceDoctorVersionInfo, CADENCE_DIAGNOSTIC_CONTRACT_VERSION,
    },
};

fn profile(
    profile_id: &str,
    provider_id: &str,
    runtime_kind: &str,
    base_url: Option<&str>,
) -> ProviderProfileRecord {
    ProviderProfileRecord {
        profile_id: profile_id.into(),
        provider_id: provider_id.into(),
        runtime_kind: runtime_kind.into(),
        label: profile_id.into(),
        model_id: "model-1".into(),
        preset_id: Some(provider_id.into()),
        base_url: base_url.map(str::to_owned),
        api_version: None,
        region: None,
        project_id: None,
        credential_link: None,
        migrated_from_legacy: false,
        migrated_at: None,
        updated_at: "2026-04-26T12:00:00Z".into(),
    }
}

fn readiness(status: ProviderProfileReadinessStatus) -> ProviderProfileReadinessProjection {
    ProviderProfileReadinessProjection {
        ready: status == ProviderProfileReadinessStatus::Ready,
        status,
        proof: None,
        proof_updated_at: None,
    }
}

fn catalog(
    source: ProviderModelCatalogSource,
    error: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    ProviderModelCatalog {
        profile_id: "openrouter-work".into(),
        provider_id: "openrouter".into(),
        configured_model_id: "openai/o4-mini".into(),
        source,
        fetched_at: Some("2026-04-26T12:00:00Z".into()),
        last_success_at: Some("2026-04-26T12:00:00Z".into()),
        last_refresh_error: error,
        models: vec![ProviderModelRecord {
            model_id: "openai/o4-mini".into(),
            display_name: "OpenAI o4-mini".into(),
            thinking: ProviderModelThinkingCapability {
                supported: true,
                effort_options: vec![],
                default_effort: None,
            },
        }],
    }
}

#[test]
fn provider_diagnostics_normalize_readiness_profile_repair_and_redaction() {
    let missing = profile(
        "openrouter-work",
        "openrouter",
        "openrouter",
        Some("https://openrouter.ai/api/v1"),
    );
    let diagnostic = provider_profile_readiness_diagnostic(
        &missing,
        &readiness(ProviderProfileReadinessStatus::Missing),
    )
    .expect("missing readiness diagnostic");
    assert_eq!(
        diagnostic.subject,
        CadenceDiagnosticSubject::ProviderProfile
    );
    assert_eq!(diagnostic.status, CadenceDiagnosticStatus::Failed);
    assert_eq!(diagnostic.severity, CadenceDiagnosticSeverity::Error);
    assert_eq!(
        diagnostic.affected_profile_id.as_deref(),
        Some("openrouter-work")
    );
    assert_eq!(
        diagnostic.affected_provider_id.as_deref(),
        Some("openrouter")
    );
    assert_eq!(
        diagnostic
            .endpoint
            .as_ref()
            .and_then(|endpoint| endpoint.host.as_deref()),
        Some("openrouter.ai")
    );
    assert!(diagnostic
        .remediation
        .expect("remediation")
        .contains("Add credentials"));

    let mut malformed = profile("anthropic-work", "anthropic", "anthropic", None);
    malformed.credential_link = Some(ProviderProfileCredentialLink::ApiKey {
        updated_at: "2026-04-26T12:00:00Z".into(),
    });
    let malformed_diagnostic = provider_profile_readiness_diagnostic(
        &malformed,
        &readiness(ProviderProfileReadinessStatus::Malformed),
    )
    .expect("malformed diagnostic");
    assert_eq!(
        malformed_diagnostic.code,
        "provider_profile_credentials_malformed"
    );
    assert!(!malformed_diagnostic.message.contains("api key value"));

    let invalid_url = invalid_base_url_diagnostic(
        &missing,
        "https://token:sk-live-secret@example.invalid/v1?api_key=sk-another-secret",
    )
    .expect("invalid base url diagnostic");
    let serialized = serde_json::to_string(&invalid_url).expect("serialize diagnostic");
    assert!(invalid_url.redacted);
    assert!(!serialized.contains("sk-live-secret"));
    assert!(!serialized.contains("sk-another-secret"));
    assert!(serialized.contains("[redacted]"));

    let unsupported = unsupported_provider_diagnostic(Some("deepseek-work"), "deepseek")
        .expect("unsupported provider diagnostic");
    assert_eq!(unsupported.code, "provider_id_unsupported");
    assert_eq!(
        unsupported.affected_provider_id.as_deref(),
        Some("deepseek")
    );

    let stale =
        stale_runtime_binding_diagnostic(Some("openrouter-work"), "openrouter", "openrouter")
            .expect("stale binding diagnostic");
    assert_eq!(stale.subject, CadenceDiagnosticSubject::RuntimeBinding);

    let ambient = ambient_auth_failure_diagnostic(
        &profile("vertex-work", "vertex", "anthropic", None),
        "ADC failed from /Users/sn0w/.config/gcloud/application_default_credentials.json with access_token=ya29.secret",
    )
    .expect("ambient auth diagnostic");
    let ambient_json = serde_json::to_string(&ambient).expect("serialize ambient diagnostic");
    assert!(ambient.redacted);
    assert!(!ambient_json.contains("/Users/sn0w"));
    assert!(!ambient_json.contains("ya29.secret"));
}

#[test]
fn provider_diagnostics_validate_state_combinations_and_catalog_retryability() {
    let invalid_passed = CadenceDiagnosticCheck {
        contract_version: CADENCE_DIAGNOSTIC_CONTRACT_VERSION,
        check_id: "diagnostic:v1:test".into(),
        subject: CadenceDiagnosticSubject::ProviderProfile,
        status: CadenceDiagnosticStatus::Passed,
        severity: CadenceDiagnosticSeverity::Error,
        retryable: true,
        code: "bad".into(),
        message: "bad".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: None,
        redaction_class: cadence_desktop_lib::runtime::CadenceDiagnosticRedactionClass::Public,
        redacted: false,
    };
    assert_eq!(
        validate_diagnostic_check(invalid_passed)
            .expect_err("passed checks cannot be retryable errors")
            .code,
        "diagnostic_state_invalid"
    );

    let missing_remediation = CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::ModelCatalog,
        status: CadenceDiagnosticStatus::Warning,
        severity: CadenceDiagnosticSeverity::Warning,
        retryable: true,
        code: "catalog_warning".into(),
        message: "Catalog warning.".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: None,
    });
    assert_eq!(
        missing_remediation
            .expect_err("warnings need remediation")
            .code,
        "invalid_request"
    );

    let retryable_error = ProviderModelCatalogDiagnostic {
        code: "provider_catalog_timeout".into(),
        message: "Timed out while refreshing provider models.".into(),
        retryable: true,
    };
    let unavailable = ProviderModelCatalog {
        models: vec![],
        ..catalog(
            ProviderModelCatalogSource::Unavailable,
            Some(retryable_error.clone()),
        )
    };
    let unavailable_diagnostic =
        provider_model_catalog_diagnostic(&unavailable).expect("catalog diagnostic");
    assert_eq!(
        unavailable_diagnostic.status,
        CadenceDiagnosticStatus::Failed
    );
    assert!(unavailable_diagnostic.retryable);

    let stale_cache = provider_model_catalog_diagnostic(&catalog(
        ProviderModelCatalogSource::Cache,
        Some(retryable_error),
    ))
    .expect("stale cache diagnostic");
    assert_eq!(stale_cache.status, CadenceDiagnosticStatus::Warning);
    assert_eq!(stale_cache.severity, CadenceDiagnosticSeverity::Warning);

    let manual =
        provider_model_catalog_diagnostic(&catalog(ProviderModelCatalogSource::Manual, None))
            .expect("manual catalog diagnostic");
    assert_eq!(manual.status, CadenceDiagnosticStatus::Skipped);
    assert!(!manual.retryable);
}

#[test]
fn doctor_report_serializes_human_and_json_with_stable_counts_and_no_secrets() {
    let passed = CadenceDiagnosticCheck::passed(
        CadenceDiagnosticSubject::RuntimeSupervisor,
        "runtime_supervisor_ready",
        "Runtime supervisor binary is available.",
    )
    .expect("passed diagnostic");
    let skipped = CadenceDiagnosticCheck::skipped(
        CadenceDiagnosticSubject::McpRegistry,
        "mcp_registry_not_configured",
        "No MCP servers are configured, so MCP dependency checks were skipped.",
        Some("Add an MCP server before running extended dependency checks.".into()),
    )
    .expect("skipped diagnostic");
    let failed = CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::SettingsDependency,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
        retryable: false,
        code: "settings_secret_path_rejected".into(),
        message: "Settings dependency read failed at /Users/sn0w/.cadence/secrets.json with token=sk-live-secret".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: Some("Move the secret outside copied diagnostics and resave settings.".into()),
    })
    .expect("failed diagnostic");

    let report = CadenceDoctorReport::new(CadenceDoctorReportInput {
        report_id: "doctor-20260426-120000".into(),
        generated_at: "2026-04-26T12:00:00Z".into(),
        mode: CadenceDoctorReportMode::QuickLocal,
        versions: CadenceDoctorVersionInfo {
            app_version: "0.1.0".into(),
            runtime_supervisor_version: Some("0.1.0".into()),
            runtime_protocol_version: Some("diagnostics-v1".into()),
        },
        profile_checks: vec![],
        model_catalog_checks: vec![],
        runtime_supervisor_checks: vec![passed],
        mcp_dependency_checks: vec![skipped],
        settings_dependency_checks: vec![failed],
    })
    .expect("doctor report");

    assert_eq!(report.summary.total, 3);
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.skipped, 1);
    assert_eq!(
        report.summary.highest_severity,
        CadenceDiagnosticSeverity::Error
    );
    assert_eq!(
        summarize_diagnostic_checks(report.all_checks()),
        report.summary
    );

    let json = render_doctor_report(&report, CadenceDoctorReportOutputMode::Json)
        .expect("doctor report JSON");
    assert!(json.contains("\"reportId\": \"doctor-20260426-120000\""));
    assert!(!json.contains("/Users/sn0w"));
    assert!(!json.contains("sk-live-secret"));
    assert!(json.contains("[redacted-path]"));

    let human = render_doctor_report(&report, CadenceDoctorReportOutputMode::CompactHuman)
        .expect("doctor report text");
    assert!(human.contains("Summary: 1 passed, 0 warning(s), 1 failed, 1 skipped"));
    assert!(human.contains("Runtime supervisor:"));
    assert!(!human.contains("sk-live-secret"));
    assert!(!human.contains("/Users/sn0w"));

    let mut invalid_summary = report.clone();
    invalid_summary.summary.total = 99;
    assert_eq!(
        validate_doctor_report(invalid_summary)
            .expect_err("summary mismatch rejected")
            .code,
        "doctor_report_summary_invalid"
    );
}
