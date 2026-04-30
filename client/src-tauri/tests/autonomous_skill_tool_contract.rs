use serde_json::json;
use xero_desktop_lib::{
    commands::CommandError,
    runtime::{
        decide_skill_tool_access, model_may_discover_skill_source,
        skill_tool_diagnostic_from_command_error, validate_skill_tool_context_payload,
        validate_skill_tool_input, validate_skill_tool_lifecycle_event,
        AutonomousSkillSourceMetadata, XeroSkillSourceRecord, XeroSkillSourceScope,
        XeroSkillSourceState, XeroSkillToolAccessStatus, XeroSkillToolContextAsset,
        XeroSkillToolContextDocument, XeroSkillToolContextPayload, XeroSkillToolDiagnostic,
        XeroSkillToolInput, XeroSkillToolLifecycleEvent, XeroSkillToolLifecycleResult,
        XeroSkillToolOperation, XeroSkillTrustState, XERO_SKILL_TOOL_CONTRACT_VERSION,
        XERO_SKILL_TOOL_DEFAULT_LIMIT,
    },
};

const SHA256_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA256_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn github_source() -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: "vercel-labs/skills".into(),
        path: "skills/find-skills".into(),
        reference: "main".into(),
        tree_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
    }
}

fn source_record(state: XeroSkillSourceState, trust: XeroSkillTrustState) -> XeroSkillSourceRecord {
    XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::global(),
        &github_source(),
        state,
        trust,
    )
    .expect("source record")
}

fn source_id() -> String {
    source_record(XeroSkillSourceState::Enabled, XeroSkillTrustState::Trusted).source_id
}

#[test]
fn skill_tool_input_schema_accepts_model_visible_operations_and_normalizes_defaults() {
    let list_input: XeroSkillToolInput = serde_json::from_value(json!({
        "operation": "list",
        "query": "  docs  ",
        "includeUnavailable": false,
        "limit": null
    }))
    .expect("list input should decode");
    let normalized = validate_skill_tool_input(list_input).expect("list input should validate");
    assert_eq!(
        normalized,
        XeroSkillToolInput::List {
            query: Some("docs".into()),
            include_unavailable: false,
            limit: Some(XERO_SKILL_TOOL_DEFAULT_LIMIT),
        }
    );

    for operation in ["resolve", "install", "invoke", "reload"] {
        let payload = match operation {
            "resolve" => json!({
                "operation": operation,
                "sourceId": source_id(),
                "skillId": null,
                "includeUnavailable": false
            }),
            "install" => json!({
                "operation": operation,
                "sourceId": source_id(),
                "approvalGrantId": "approval-1"
            }),
            "invoke" => json!({
                "operation": operation,
                "sourceId": source_id(),
                "approvalGrantId": null,
                "includeSupportingAssets": true
            }),
            "reload" => json!({
                "operation": operation,
                "sourceId": null,
                "sourceKind": "github"
            }),
            _ => unreachable!(),
        };
        let decoded: XeroSkillToolInput =
            serde_json::from_value(payload).expect("operation input should decode");
        validate_skill_tool_input(decoded).expect("operation input should validate");
    }
}

#[test]
fn skill_tool_input_schema_rejects_unknown_fields_and_invalid_selectors() {
    let unknown_field = serde_json::from_value::<XeroSkillToolInput>(json!({
        "operation": "invoke",
        "sourceId": source_id(),
        "approvalGrantId": null,
        "includeSupportingAssets": false,
        "absolutePath": "/Users/sn0w/Library/Application Support/dev.sn0w.xero/skills/find-skills"
    }));
    assert!(unknown_field.is_err());

    let ambiguous = validate_skill_tool_input(XeroSkillToolInput::Resolve {
        source_id: Some(source_id()),
        skill_id: Some("find-skills".into()),
        include_unavailable: false,
    })
    .expect_err("resolve must use one selector");
    assert_eq!(ambiguous.code, "skill_tool_selector_ambiguous");

    let invalid_source = validate_skill_tool_input(XeroSkillToolInput::Install {
        source_id: "find-skills".into(),
        approval_grant_id: None,
    })
    .expect_err("source ids must be canonical");
    assert_eq!(invalid_source.code, "skill_tool_source_id_invalid");
}

#[test]
fn skill_tool_access_contract_exposes_discovery_and_user_approval_boundaries() {
    let discoverable = source_record(
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::ApprovalRequired,
    );
    assert!(model_may_discover_skill_source(&discoverable));

    let list_decision = decide_skill_tool_access(&discoverable, XeroSkillToolOperation::List)
        .expect("list decision");
    assert_eq!(list_decision.status, XeroSkillToolAccessStatus::Allowed);
    assert!(list_decision.model_visible);

    let invoke_requires_approval = source_record(
        XeroSkillSourceState::Enabled,
        XeroSkillTrustState::Untrusted,
    );
    let invoke_decision =
        decide_skill_tool_access(&invoke_requires_approval, XeroSkillToolOperation::Invoke)
            .expect("invoke decision");
    assert_eq!(
        invoke_decision.status,
        XeroSkillToolAccessStatus::ApprovalRequired
    );
    assert!(invoke_decision.model_visible);
    assert_eq!(
        invoke_decision.reason.expect("approval diagnostic").code,
        "skill_tool_user_approval_required"
    );

    let disabled = source_record(XeroSkillSourceState::Disabled, XeroSkillTrustState::Trusted);
    assert!(!model_may_discover_skill_source(&disabled));
    let disabled_decision = decide_skill_tool_access(&disabled, XeroSkillToolOperation::List)
        .expect("disabled decision");
    assert_eq!(disabled_decision.status, XeroSkillToolAccessStatus::Denied);
    assert!(!disabled_decision.model_visible);
}

#[test]
fn skill_tool_failures_are_redacted_before_model_projection() {
    let local_path_error = CommandError::user_fixable(
        "skill_tool_read_failed",
        "Could not read /Users/sn0w/Library/Application Support/dev.sn0w.xero/skills/find-skills/SKILL.md",
    );
    let diagnostic = skill_tool_diagnostic_from_command_error(&local_path_error);
    assert!(diagnostic.redacted);
    assert!(!diagnostic.message.contains("/Users/sn0w"));
    assert!(diagnostic.message.contains("[redacted-path]"));

    let secret_error = CommandError::retryable(
        "skill_tool_fetch_failed",
        "GitHub rejected github_pat_1234567890 from /Users/sn0w/.config/xero",
    );
    let diagnostic = skill_tool_diagnostic_from_command_error(&secret_error);
    assert!(diagnostic.redacted);
    assert!(!diagnostic.message.contains("github_pat"));
    assert!(!diagnostic.message.contains("/Users/sn0w"));
    assert!(diagnostic.message.contains("[redacted]"));
    assert!(diagnostic.message.contains("[redacted-path]"));
}

#[test]
fn skill_tool_lifecycle_events_validate_success_failure_and_approval_shapes() {
    let success = XeroSkillToolLifecycleEvent::succeeded(
        XeroSkillToolOperation::Invoke,
        Some(source_id()),
        Some("find-skills".into()),
        "Invoked skill.",
    )
    .expect("success event");
    validate_skill_tool_lifecycle_event(success).expect("success event validates");

    let failed = XeroSkillToolLifecycleEvent::failed(
        XeroSkillToolOperation::Install,
        Some(source_id()),
        Some("find-skills".into()),
        "Install failed.",
        &CommandError::retryable(
            "skill_tool_cache_failed",
            "Cache write failed at /Users/sn0w/Library/Application Support/dev.sn0w.xero/skills",
        ),
    )
    .expect("failure event");
    let failed = validate_skill_tool_lifecycle_event(failed).expect("failure event validates");
    assert_eq!(failed.result, XeroSkillToolLifecycleResult::Failed);
    assert!(failed.diagnostic.expect("failure diagnostic").redacted);

    let bad_success = XeroSkillToolLifecycleEvent {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        operation: XeroSkillToolOperation::Invoke,
        result: XeroSkillToolLifecycleResult::Succeeded,
        source_id: Some(source_id()),
        skill_id: Some("find-skills".into()),
        detail: "bad success".into(),
        diagnostic: Some(XeroSkillToolDiagnostic {
            code: "unexpected".into(),
            message: "Unexpected diagnostic.".into(),
            retryable: false,
            redacted: false,
        }),
    };
    assert_eq!(
        validate_skill_tool_lifecycle_event(bad_success)
            .expect_err("successful events must omit diagnostics")
            .code,
        "skill_tool_lifecycle_invalid"
    );
}

#[test]
fn skill_tool_context_payload_allows_markdown_and_text_assets_without_raw_paths() {
    let markdown_content =
        "---\nname: find-skills\ndescription: Find skills.\n---\n# Find Skills\n";
    let asset_content = "# Guide\nUse this skill for discovery.\n";
    let payload = XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: source_id(),
        skill_id: "find-skills".into(),
        markdown: XeroSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: SHA256_A.into(),
            bytes: markdown_content.len(),
            content: markdown_content.into(),
        },
        supporting_assets: vec![XeroSkillToolContextAsset {
            relative_path: "guide.md".into(),
            sha256: SHA256_B.into(),
            bytes: asset_content.len(),
            content: asset_content.into(),
        }],
    };

    let validated =
        validate_skill_tool_context_payload(payload).expect("context payload should validate");
    assert_eq!(validated.markdown.relative_path, "SKILL.md");
    assert_eq!(validated.supporting_assets[0].relative_path, "guide.md");

    let unknown_asset_field = serde_json::from_value::<XeroSkillToolContextPayload>(json!({
        "contractVersion": XERO_SKILL_TOOL_CONTRACT_VERSION,
        "sourceId": source_id(),
        "skillId": "find-skills",
        "markdown": {
            "relativePath": "SKILL.md",
            "sha256": SHA256_A,
            "bytes": markdown_content.len(),
            "content": markdown_content
        },
        "supportingAssets": [{
            "relativePath": "guide.md",
            "absolutePath": "/Users/sn0w/Library/Application Support/dev.sn0w.xero/skills/find-skills/guide.md",
            "sha256": SHA256_B,
            "bytes": asset_content.len(),
            "content": asset_content
        }]
    }));
    assert!(unknown_asset_field.is_err());

    let absolute_path_payload = XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: source_id(),
        skill_id: "find-skills".into(),
        markdown: XeroSkillToolContextDocument {
            relative_path:
                "/Users/sn0w/Library/Application Support/dev.sn0w.xero/skills/find-skills/SKILL.md"
                    .into(),
            sha256: SHA256_A.into(),
            bytes: markdown_content.len(),
            content: markdown_content.into(),
        },
        supporting_assets: Vec::new(),
    };
    assert_eq!(
        validate_skill_tool_context_payload(absolute_path_payload)
            .expect_err("absolute paths must not enter context payloads")
            .code,
        "autonomous_skill_source_metadata_invalid"
    );
}
