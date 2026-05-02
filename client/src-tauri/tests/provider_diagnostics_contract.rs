use xero_desktop_lib::{
    provider_credentials::{
        ProviderApiKeyCredentialEntry, ProviderCredentialLink, ProviderCredentialProfile,
        ProviderCredentialReadinessProjection, ProviderCredentialReadinessStatus,
        ProviderCredentialsView,
    },
    provider_models::{
        ProviderModelCatalog, ProviderModelCatalogDiagnostic, ProviderModelCatalogSource,
        ProviderModelRecord, ProviderModelThinkingCapability,
    },
    runtime::{
        ambient_auth_failure_diagnostic, invalid_base_url_diagnostic,
        provider_model_catalog_diagnostic, provider_readiness_diagnostic,
        provider_validation_diagnostics, render_doctor_report, sanitize_diagnostic_text,
        stale_runtime_binding_diagnostic, summarize_diagnostic_checks,
        unsupported_provider_diagnostic, validate_diagnostic_check, validate_doctor_report,
        XeroDiagnosticCheck, XeroDiagnosticCheckInput, XeroDiagnosticEndpointMetadata,
        XeroDiagnosticRedactionClass, XeroDiagnosticSeverity, XeroDiagnosticStatus,
        XeroDiagnosticSubject, XeroDoctorReport, XeroDoctorReportInput, XeroDoctorReportMode,
        XeroDoctorReportOutputMode, XeroDoctorVersionInfo, XERO_DIAGNOSTIC_CONTRACT_VERSION,
    },
};

fn profile(
    profile_id: &str,
    provider_id: &str,
    runtime_kind: &str,
    base_url: Option<&str>,
) -> ProviderCredentialProfile {
    ProviderCredentialProfile {
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
        updated_at: "2026-04-26T12:00:00Z".into(),
    }
}

fn readiness(status: ProviderCredentialReadinessStatus) -> ProviderCredentialReadinessProjection {
    ProviderCredentialReadinessProjection {
        ready: status == ProviderCredentialReadinessStatus::Ready,
        status,
        proof: None,
        proof_updated_at: None,
    }
}

fn snapshot_for(
    active_profile_id: &str,
    profiles: Vec<ProviderCredentialProfile>,
    api_keys: Vec<ProviderApiKeyCredentialEntry>,
) -> ProviderCredentialsView {
    ProviderCredentialsView::from_projected_profiles_for_tests(
        active_profile_id.into(),
        profiles,
        api_keys,
    )
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
            context_window_tokens: None,
            max_output_tokens: None,
            context_limit_source: None,
            context_limit_confidence: None,
            context_limit_fetched_at: None,
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
    let diagnostic = provider_readiness_diagnostic(
        &missing,
        &readiness(ProviderCredentialReadinessStatus::Missing),
    )
    .expect("missing readiness diagnostic");
    assert_eq!(
        diagnostic.subject,
        XeroDiagnosticSubject::ProviderCredential
    );
    assert_eq!(diagnostic.status, XeroDiagnosticStatus::Failed);
    assert_eq!(diagnostic.severity, XeroDiagnosticSeverity::Error);
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
    malformed.credential_link = Some(ProviderCredentialLink::ApiKey {
        updated_at: "2026-04-26T12:00:00Z".into(),
    });
    let malformed_diagnostic = provider_readiness_diagnostic(
        &malformed,
        &readiness(ProviderCredentialReadinessStatus::Malformed),
    )
    .expect("malformed diagnostic");
    assert_eq!(malformed_diagnostic.code, "provider_credentials_malformed");
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
    assert_eq!(stale.subject, XeroDiagnosticSubject::RuntimeBinding);

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
fn provider_validation_reports_metadata_runtime_and_readiness_contracts() {
    let mut ready = profile("openrouter-work", "openrouter", "openrouter", None);
    ready.model_id = "openai/o4-mini".into();
    ready.credential_link = Some(ProviderCredentialLink::ApiKey {
        updated_at: "2026-04-26T12:00:00Z".into(),
    });
    let snapshot = snapshot_for(
        "openrouter-work",
        vec![ready.clone()],
        vec![ProviderApiKeyCredentialEntry {
            profile_id: "openrouter-work".into(),
            api_key: "sk-or-v1-test".into(),
            updated_at: "2026-04-26T12:00:00Z".into(),
        }],
    );

    let checks =
        provider_validation_diagnostics(&snapshot, &ready).expect("validate ready provider");
    assert!(checks
        .iter()
        .all(|check| check.status != XeroDiagnosticStatus::Failed));
    assert!(checks
        .iter()
        .any(|check| check.code == "provider_runtime_aligned"));
    assert!(checks
        .iter()
        .any(|check| check.code == "provider_metadata_ready"));
    assert!(checks.iter().any(|check| check.code == "provider_ready"));

    let malformed_metadata = profile(
        "openrouter-bad",
        "openrouter",
        "openrouter",
        Some("https://openrouter.ai/api/v1"),
    );
    let malformed_snapshot = snapshot_for(
        "openrouter-bad",
        vec![malformed_metadata.clone()],
        Vec::new(),
    );

    let repair_checks = provider_validation_diagnostics(&malformed_snapshot, &malformed_metadata)
        .expect("validate malformed provider");
    assert!(repair_checks
        .iter()
        .any(|check| check.code == "provider_metadata_unexpected"
            && check.status == XeroDiagnosticStatus::Failed));
    assert!(repair_checks
        .iter()
        .any(|check| check.code == "provider_credentials_missing"
            && check
                .remediation
                .as_deref()
                .is_some_and(|value| value.contains("Add credentials"))));
}

#[test]
fn provider_validation_accepts_supported_provider_metadata_shapes() {
    #[derive(Clone, Copy)]
    enum CredentialKind {
        OpenAiCodex,
        ApiKey,
        Local,
        Ambient,
    }

    struct Case {
        profile_id: &'static str,
        provider_id: &'static str,
        runtime_kind: &'static str,
        model_id: &'static str,
        preset_id: Option<&'static str>,
        base_url: Option<&'static str>,
        api_version: Option<&'static str>,
        region: Option<&'static str>,
        project_id: Option<&'static str>,
        credential: CredentialKind,
    }

    let cases = [
        Case {
            profile_id: "openai_codex-default",
            provider_id: "openai_codex",
            runtime_kind: "openai_codex",
            model_id: "openai_codex",
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::OpenAiCodex,
        },
        Case {
            profile_id: "openrouter-work",
            provider_id: "openrouter",
            runtime_kind: "openrouter",
            model_id: "openai/o4-mini",
            preset_id: Some("openrouter"),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "anthropic-work",
            provider_id: "anthropic",
            runtime_kind: "anthropic",
            model_id: "claude-3-7-sonnet-latest",
            preset_id: Some("anthropic"),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "github-models-work",
            provider_id: "github_models",
            runtime_kind: "openai_compatible",
            model_id: "openai/gpt-4.1",
            preset_id: Some("github_models"),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "openai-compatible-work",
            provider_id: "openai_api",
            runtime_kind: "openai_compatible",
            model_id: "gpt-4.1-mini",
            preset_id: Some("openai_api"),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "mistral-recipe-work",
            provider_id: "openai_api",
            runtime_kind: "openai_compatible",
            model_id: "mistral-large-latest",
            preset_id: Some("openai_api"),
            base_url: Some("https://api.mistral.ai/v1"),
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "ollama-work",
            provider_id: "ollama",
            runtime_kind: "openai_compatible",
            model_id: "llama3.2",
            preset_id: Some("ollama"),
            base_url: Some("http://127.0.0.1:11434/v1"),
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::Local,
        },
        Case {
            profile_id: "azure-work",
            provider_id: "azure_openai",
            runtime_kind: "openai_compatible",
            model_id: "gpt-4.1-mini",
            preset_id: Some("azure_openai"),
            base_url: Some("https://azure.example.invalid/openai/deployments/work"),
            api_version: Some("2025-04-01-preview"),
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "gemini-work",
            provider_id: "gemini_ai_studio",
            runtime_kind: "gemini",
            model_id: "gemini-2.5-pro",
            preset_id: Some("gemini_ai_studio"),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential: CredentialKind::ApiKey,
        },
        Case {
            profile_id: "bedrock-work",
            provider_id: "bedrock",
            runtime_kind: "anthropic",
            model_id: "anthropic.claude-3-7-sonnet-20250219-v1:0",
            preset_id: Some("bedrock"),
            base_url: None,
            api_version: None,
            region: Some("us-east-1"),
            project_id: None,
            credential: CredentialKind::Ambient,
        },
        Case {
            profile_id: "vertex-work",
            provider_id: "vertex",
            runtime_kind: "anthropic",
            model_id: "claude-3-7-sonnet@20250219",
            preset_id: Some("vertex"),
            base_url: None,
            api_version: None,
            region: Some("us-central1"),
            project_id: Some("vertex-project"),
            credential: CredentialKind::Ambient,
        },
    ];

    for case in cases {
        let updated_at = "2026-04-26T12:00:00Z";
        let mut record = profile(
            case.profile_id,
            case.provider_id,
            case.runtime_kind,
            case.base_url,
        );
        record.model_id = case.model_id.into();
        record.preset_id = case.preset_id.map(str::to_string);
        record.api_version = case.api_version.map(str::to_string);
        record.region = case.region.map(str::to_string);
        record.project_id = case.project_id.map(str::to_string);
        record.credential_link = Some(match case.credential {
            CredentialKind::OpenAiCodex => ProviderCredentialLink::OpenAiCodex {
                account_id: "acct-test".into(),
                session_id: "session-test".into(),
                updated_at: updated_at.into(),
            },
            CredentialKind::ApiKey => ProviderCredentialLink::ApiKey {
                updated_at: updated_at.into(),
            },
            CredentialKind::Local => ProviderCredentialLink::Local {
                updated_at: updated_at.into(),
            },
            CredentialKind::Ambient => ProviderCredentialLink::Ambient {
                updated_at: updated_at.into(),
            },
        });
        let api_keys = match case.credential {
            CredentialKind::ApiKey => vec![ProviderApiKeyCredentialEntry {
                profile_id: case.profile_id.into(),
                api_key: "test-api-key".into(),
                updated_at: updated_at.into(),
            }],
            CredentialKind::OpenAiCodex | CredentialKind::Local | CredentialKind::Ambient => {
                Vec::new()
            }
        };
        let snapshot = snapshot_for(case.profile_id, vec![record.clone()], api_keys);

        let checks = provider_validation_diagnostics(&snapshot, &record)
            .unwrap_or_else(|error| panic!("{} validation failed: {error:?}", case.provider_id));
        assert!(
            checks
                .iter()
                .all(|check| check.status != XeroDiagnosticStatus::Failed),
            "{} should not emit failed validation checks: {checks:?}",
            case.provider_id
        );
        assert!(checks
            .iter()
            .any(|check| check.code == "provider_metadata_ready"));
        assert!(checks.iter().any(|check| check.code == "provider_ready"));
    }
}

#[test]
fn provider_diagnostics_validate_state_combinations_and_catalog_retryability() {
    let invalid_passed = XeroDiagnosticCheck {
        contract_version: XERO_DIAGNOSTIC_CONTRACT_VERSION,
        check_id: "diagnostic:v1:test".into(),
        subject: XeroDiagnosticSubject::ProviderCredential,
        status: XeroDiagnosticStatus::Passed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: true,
        code: "bad".into(),
        message: "bad".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: None,
        redaction_class: xero_desktop_lib::runtime::XeroDiagnosticRedactionClass::Public,
        redacted: false,
    };
    assert_eq!(
        validate_diagnostic_check(invalid_passed)
            .expect_err("passed checks cannot be retryable errors")
            .code,
        "diagnostic_state_invalid"
    );

    let missing_remediation = XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ModelCatalog,
        status: XeroDiagnosticStatus::Warning,
        severity: XeroDiagnosticSeverity::Warning,
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
    assert_eq!(unavailable_diagnostic.status, XeroDiagnosticStatus::Failed);
    assert!(unavailable_diagnostic.retryable);

    let stale_cache = provider_model_catalog_diagnostic(&catalog(
        ProviderModelCatalogSource::Cache,
        Some(retryable_error),
    ))
    .expect("stale cache diagnostic");
    assert_eq!(stale_cache.status, XeroDiagnosticStatus::Warning);
    assert_eq!(stale_cache.severity, XeroDiagnosticSeverity::Warning);

    let manual =
        provider_model_catalog_diagnostic(&catalog(ProviderModelCatalogSource::Manual, None))
            .expect("manual catalog diagnostic");
    assert_eq!(manual.status, XeroDiagnosticStatus::Skipped);
    assert!(!manual.retryable);
}

#[test]
fn diagnostics_redact_auth_headers_cloud_paths_and_nested_report_payloads() {
    let (auth_header, auth_redacted, auth_class) = sanitize_diagnostic_text(
        "Provider returned Authorization: Bearer opaque-oauth-token-123 during refresh.",
    );
    assert!(auth_redacted);
    assert_eq!(auth_class, XeroDiagnosticRedactionClass::Secret);
    assert!(!auth_header.contains("opaque-oauth-token-123"));
    assert!(auth_header.contains("Authorization: Bearer [redacted]"));

    let (compact_auth_header, compact_auth_redacted, _) = sanitize_diagnostic_text(
        "Provider returned Authorization:Bearer compact-oauth-token-456 during refresh.",
    );
    assert!(compact_auth_redacted);
    assert!(!compact_auth_header.contains("compact-oauth-token-456"));

    let (cloud_paths, cloud_redacted, cloud_class) = sanitize_diagnostic_text(
        "ADC failed at GOOGLE_APPLICATION_CREDENTIALS=/Users/sn0w/.config/gcloud/application_default_credentials.json and AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY.",
    );
    assert!(cloud_redacted);
    assert_eq!(cloud_class, XeroDiagnosticRedactionClass::Secret);
    assert!(!cloud_paths.contains("/Users/sn0w"));
    assert!(!cloud_paths.contains("wJalrXUtnFEMI"));
    assert!(cloud_paths.contains("[redacted-path]"));

    let (windows_paths, windows_redacted, windows_class) = sanitize_diagnostic_text(
        r"Settings failed at C:\ProgramData\Xero\secrets.json and C:\Windows\Temp\xero-token.txt plus %LOCALAPPDATA%\Xero\credentials.json.",
    );
    assert!(windows_redacted);
    assert_eq!(windows_class, XeroDiagnosticRedactionClass::LocalPath);
    assert!(!windows_paths.contains("ProgramData"));
    assert!(!windows_paths.contains(r"Windows\Temp"));
    assert!(!windows_paths.contains("%LOCALAPPDATA%"));

    let raw_nested = XeroDiagnosticCheck {
        contract_version: XERO_DIAGNOSTIC_CONTRACT_VERSION,
        check_id: "diagnostic:v1:settings_dependency:global:global:nested_secret_payload".into(),
        subject: XeroDiagnosticSubject::SettingsDependency,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: "nested_secret_payload".into(),
        message: "Nested doctor payload included Authorization: Bearer opaque-nested-token-456 from /Users/sn0w/.aws/credentials.".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: Some(XeroDiagnosticEndpointMetadata {
            base_url: Some(
                "http://local-user:local-pass@127.0.0.1:4000/v1?api_key=opaque-local-key"
                    .into(),
            ),
            host: None,
            api_version: None,
            region: None,
            project_id: None,
            model_list_strategy: Some("refresh_token=rt_opaque_nested_secret".into()),
            redacted: false,
        }),
        remediation: Some(
            "Remove session_id=sess_nested_secret before copying diagnostics.".into(),
        ),
        redaction_class: XeroDiagnosticRedactionClass::Public,
        redacted: false,
    };

    let report = XeroDoctorReport::new(XeroDoctorReportInput {
        report_id: "doctor-20260426-privacy".into(),
        generated_at: "2026-04-26T12:00:00Z".into(),
        mode: XeroDoctorReportMode::QuickLocal,
        versions: XeroDoctorVersionInfo {
            app_version: "0.1.0".into(),
            runtime_supervisor_version: Some("0.1.0".into()),
            runtime_protocol_version: Some("diagnostics-v1".into()),
        },
        dictation_checks: vec![],
        profile_checks: vec![],
        model_catalog_checks: vec![],
        runtime_supervisor_checks: vec![],
        mcp_dependency_checks: vec![],
        settings_dependency_checks: vec![raw_nested],
    })
    .expect("doctor report sanitizes nested checks");

    let nested = report
        .settings_dependency_checks
        .first()
        .expect("nested diagnostic");
    assert!(nested.redacted);
    assert_eq!(nested.redaction_class, XeroDiagnosticRedactionClass::Secret);

    let json = render_doctor_report(&report, XeroDoctorReportOutputMode::Json)
        .expect("render redacted doctor report");
    for leaked in [
        "opaque-nested-token-456",
        "local-pass",
        "opaque-local-key",
        "rt_opaque_nested_secret",
        "sess_nested_secret",
        "/Users/sn0w",
    ] {
        assert!(
            !json.contains(leaked),
            "doctor report leaked secret fragment {leaked}"
        );
    }
}

#[test]
fn doctor_report_serializes_human_and_json_with_stable_counts_and_no_secrets() {
    let passed = XeroDiagnosticCheck::passed(
        XeroDiagnosticSubject::RuntimeSupervisor,
        "runtime_supervisor_ready",
        "Runtime supervisor binary is available.",
    )
    .expect("passed diagnostic");
    let skipped = XeroDiagnosticCheck::skipped(
        XeroDiagnosticSubject::McpRegistry,
        "mcp_registry_not_configured",
        "No MCP servers are configured, so MCP dependency checks were skipped.",
        Some("Add an MCP server before running extended dependency checks.".into()),
    )
    .expect("skipped diagnostic");
    let failed = XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::SettingsDependency,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: "settings_secret_path_rejected".into(),
        message: "Settings dependency read failed at /Users/sn0w/Library/Application Support/dev.sn0w.xero/secrets.json with token=sk-live-secret".into(),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: Some("Move the secret outside copied diagnostics and resave settings.".into()),
    })
    .expect("failed diagnostic");

    let report = XeroDoctorReport::new(XeroDoctorReportInput {
        report_id: "doctor-20260426-120000".into(),
        generated_at: "2026-04-26T12:00:00Z".into(),
        mode: XeroDoctorReportMode::QuickLocal,
        versions: XeroDoctorVersionInfo {
            app_version: "0.1.0".into(),
            runtime_supervisor_version: Some("0.1.0".into()),
            runtime_protocol_version: Some("diagnostics-v1".into()),
        },
        dictation_checks: vec![],
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
        XeroDiagnosticSeverity::Error
    );
    assert_eq!(
        summarize_diagnostic_checks(report.all_checks()),
        report.summary
    );

    let json = render_doctor_report(&report, XeroDoctorReportOutputMode::Json)
        .expect("doctor report JSON");
    assert!(json.contains("\"reportId\": \"doctor-20260426-120000\""));
    assert!(!json.contains("/Users/sn0w"));
    assert!(!json.contains("sk-live-secret"));
    assert!(json.contains("[redacted-path]"));

    let human = render_doctor_report(&report, XeroDoctorReportOutputMode::CompactHuman)
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
