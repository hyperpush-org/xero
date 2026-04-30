use xero_desktop_lib::runtime::{
    merge_skill_source_records, validate_skill_source_state_transition,
    AutonomousSkillSourceMetadata, XeroSkillSourceKind, XeroSkillSourceLocator,
    XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState, XeroSkillTrustState,
    XERO_SKILL_SOURCE_CONTRACT_VERSION,
};

fn github_source(tree_hash: &str) -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: "vercel-labs/skills".into(),
        path: "skills/find-skills".into(),
        reference: "main".into(),
        tree_hash: tree_hash.into(),
    }
}

#[test]
fn skill_source_contract_names_every_taxonomy_source_and_scope() {
    let sources = [
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::global(),
            XeroSkillSourceLocator::Bundled {
                bundle_id: "xero".into(),
                skill_id: "write-docs".into(),
                version: "2026.04.25".into(),
            },
            XeroSkillSourceState::Enabled,
            XeroSkillTrustState::Trusted,
        )
        .expect("bundled source"),
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::global(),
            XeroSkillSourceLocator::Local {
                root_id: "personal-skills".into(),
                root_path: "/Users/sn0w/Library/Application Support/dev.sn0w.xero/skills".into(),
                relative_path: "write-docs".into(),
                skill_id: "write-docs".into(),
            },
            XeroSkillSourceState::Discoverable,
            XeroSkillTrustState::ApprovalRequired,
        )
        .expect("local source"),
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::project("project-alpha").expect("project scope"),
            XeroSkillSourceLocator::Project {
                relative_path: "skills/write-docs".into(),
                skill_id: "write-docs".into(),
            },
            XeroSkillSourceState::Discoverable,
            XeroSkillTrustState::ApprovalRequired,
        )
        .expect("project source"),
        XeroSkillSourceRecord::github_autonomous(
            XeroSkillSourceScope::global(),
            &github_source("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            XeroSkillSourceState::Installed,
            XeroSkillTrustState::Trusted,
        )
        .expect("GitHub source"),
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::project("project-alpha").expect("project scope"),
            XeroSkillSourceLocator::Dynamic {
                run_id: "run-1".into(),
                artifact_id: "artifact-1".into(),
                skill_id: "captured-debugging".into(),
            },
            XeroSkillSourceState::Disabled,
            XeroSkillTrustState::Untrusted,
        )
        .expect("dynamic source"),
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::global(),
            XeroSkillSourceLocator::Mcp {
                server_id: "docs-server".into(),
                capability_id: "prompt:write-docs".into(),
                skill_id: "write-docs".into(),
            },
            XeroSkillSourceState::Discoverable,
            XeroSkillTrustState::ApprovalRequired,
        )
        .expect("MCP source"),
        XeroSkillSourceRecord::new(
            XeroSkillSourceScope::project("project-alpha").expect("project scope"),
            XeroSkillSourceLocator::Plugin {
                plugin_id: "com.example.skills".into(),
                contribution_id: "write-docs".into(),
                skill_path: "skills/write-docs".into(),
                skill_id: "write-docs".into(),
            },
            XeroSkillSourceState::Discoverable,
            XeroSkillTrustState::ApprovalRequired,
        )
        .expect("plugin source"),
    ];

    let kinds = sources
        .iter()
        .map(|source| source.locator.kind())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            XeroSkillSourceKind::Bundled,
            XeroSkillSourceKind::Local,
            XeroSkillSourceKind::Project,
            XeroSkillSourceKind::Github,
            XeroSkillSourceKind::Dynamic,
            XeroSkillSourceKind::Mcp,
            XeroSkillSourceKind::Plugin,
        ]
    );

    let merged = merge_skill_source_records(sources).expect("all source ids are canonical");
    assert_eq!(merged.len(), 7);
}

#[test]
fn skill_source_contract_assigns_canonical_ids_and_round_trips_github_runtime_metadata() {
    let record = XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::global(),
        &github_source("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        XeroSkillSourceState::Installed,
        XeroSkillTrustState::Trusted,
    )
    .expect("GitHub autonomous skill source should normalize");

    assert_eq!(
        record.source_id,
        "skill-source:v1:global:github:vercel-labs/skills:main:skills/find-skills"
    );
    assert_eq!(
        record
            .locator
            .to_autonomous_github_source()
            .expect("GitHub locator should round-trip"),
        github_source("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );

    let project_record = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::project("project-alpha").expect("project scope"),
        XeroSkillSourceLocator::Project {
            relative_path: "skills/write-docs".into(),
            skill_id: "write-docs".into(),
        },
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::ApprovalRequired,
    )
    .expect("project skill source should normalize");

    assert_eq!(
        project_record.source_id,
        "skill-source:v1:project:project-alpha:project:skills/write-docs:write-docs"
    );
    assert_eq!(
        project_record.contract_version,
        XERO_SKILL_SOURCE_CONTRACT_VERSION
    );
}

#[test]
fn skill_source_contract_requires_kind_specific_scope_and_fields() {
    let bundled_project_error = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::project("project-alpha").expect("project scope"),
        XeroSkillSourceLocator::Bundled {
            bundle_id: "xero".into(),
            skill_id: "write-docs".into(),
            version: "2026.04.25".into(),
        },
        XeroSkillSourceState::Enabled,
        XeroSkillTrustState::Trusted,
    )
    .expect_err("bundled skills must be global");
    assert_eq!(bundled_project_error.code, "skill_source_scope_invalid");

    let global_project_error = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::global(),
        XeroSkillSourceLocator::Project {
            relative_path: "skills/write-docs".into(),
            skill_id: "write-docs".into(),
        },
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::ApprovalRequired,
    )
    .expect_err("project skills must be project-scoped");
    assert_eq!(global_project_error.code, "skill_source_scope_invalid");

    let bad_github_error = XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::global(),
        &github_source("not-a-tree-hash"),
        XeroSkillSourceState::Installed,
        XeroSkillTrustState::Trusted,
    )
    .expect_err("GitHub sources require tree hashes for runtime compatibility");
    assert_eq!(
        bad_github_error.code,
        "autonomous_skill_source_metadata_invalid"
    );
}

#[test]
fn skill_source_contract_merges_duplicates_by_source_identity() {
    let discovered = XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::global(),
        &github_source("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::Trusted,
    )
    .expect("GitHub source");
    let installed_new_tree = XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::global(),
        &github_source("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        XeroSkillSourceState::Installed,
        XeroSkillTrustState::ApprovalRequired,
    )
    .expect("GitHub source");

    assert_eq!(discovered.source_id, installed_new_tree.source_id);

    let merged =
        merge_skill_source_records([discovered, installed_new_tree]).expect("merge sources");

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].state, XeroSkillSourceState::Stale);
    assert_eq!(merged[0].trust, XeroSkillTrustState::ApprovalRequired);
    assert_eq!(
        merged[0]
            .locator
            .to_autonomous_github_source()
            .expect("GitHub locator")
            .tree_hash,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
}

#[test]
fn skill_source_contract_rejects_unsupported_state_transitions() {
    validate_skill_source_state_transition(
        XeroSkillSourceState::Installed,
        XeroSkillSourceState::Enabled,
    )
    .expect("installed sources can be enabled");
    validate_skill_source_state_transition(
        XeroSkillSourceState::Enabled,
        XeroSkillSourceState::Disabled,
    )
    .expect("enabled sources can be disabled");

    let discoverable_to_enabled = validate_skill_source_state_transition(
        XeroSkillSourceState::Discoverable,
        XeroSkillSourceState::Enabled,
    )
    .expect_err("discoverable sources must install or be reviewed first");
    assert_eq!(
        discoverable_to_enabled.code,
        "skill_source_state_transition_unsupported"
    );

    let blocked_to_enabled = validate_skill_source_state_transition(
        XeroSkillSourceState::Blocked,
        XeroSkillSourceState::Enabled,
    )
    .expect_err("blocked sources cannot be enabled directly");
    assert_eq!(
        blocked_to_enabled.code,
        "skill_source_state_transition_unsupported"
    );
}
