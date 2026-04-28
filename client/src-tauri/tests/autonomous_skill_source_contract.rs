use cadence_desktop_lib::runtime::{
    merge_skill_source_records, validate_skill_source_state_transition,
    AutonomousSkillSourceMetadata, CadenceSkillSourceKind, CadenceSkillSourceLocator,
    CadenceSkillSourceRecord, CadenceSkillSourceScope, CadenceSkillSourceState,
    CadenceSkillTrustState, CADENCE_SKILL_SOURCE_CONTRACT_VERSION,
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
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::global(),
            CadenceSkillSourceLocator::Bundled {
                bundle_id: "cadence".into(),
                skill_id: "write-docs".into(),
                version: "2026.04.25".into(),
            },
            CadenceSkillSourceState::Enabled,
            CadenceSkillTrustState::Trusted,
        )
        .expect("bundled source"),
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::global(),
            CadenceSkillSourceLocator::Local {
                root_id: "personal-skills".into(),
                root_path: "/Users/sn0w/Library/Application Support/dev.sn0w.cadence/skills".into(),
                relative_path: "write-docs".into(),
                skill_id: "write-docs".into(),
            },
            CadenceSkillSourceState::Discoverable,
            CadenceSkillTrustState::ApprovalRequired,
        )
        .expect("local source"),
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::project("project-alpha").expect("project scope"),
            CadenceSkillSourceLocator::Project {
                relative_path: "skills/write-docs".into(),
                skill_id: "write-docs".into(),
            },
            CadenceSkillSourceState::Discoverable,
            CadenceSkillTrustState::ApprovalRequired,
        )
        .expect("project source"),
        CadenceSkillSourceRecord::github_autonomous(
            CadenceSkillSourceScope::global(),
            &github_source("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            CadenceSkillSourceState::Installed,
            CadenceSkillTrustState::Trusted,
        )
        .expect("GitHub source"),
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::project("project-alpha").expect("project scope"),
            CadenceSkillSourceLocator::Dynamic {
                run_id: "run-1".into(),
                artifact_id: "artifact-1".into(),
                skill_id: "captured-debugging".into(),
            },
            CadenceSkillSourceState::Disabled,
            CadenceSkillTrustState::Untrusted,
        )
        .expect("dynamic source"),
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::global(),
            CadenceSkillSourceLocator::Mcp {
                server_id: "docs-server".into(),
                capability_id: "prompt:write-docs".into(),
                skill_id: "write-docs".into(),
            },
            CadenceSkillSourceState::Discoverable,
            CadenceSkillTrustState::ApprovalRequired,
        )
        .expect("MCP source"),
        CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::project("project-alpha").expect("project scope"),
            CadenceSkillSourceLocator::Plugin {
                plugin_id: "com.example.skills".into(),
                contribution_id: "write-docs".into(),
                skill_path: "skills/write-docs".into(),
                skill_id: "write-docs".into(),
            },
            CadenceSkillSourceState::Discoverable,
            CadenceSkillTrustState::ApprovalRequired,
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
            CadenceSkillSourceKind::Bundled,
            CadenceSkillSourceKind::Local,
            CadenceSkillSourceKind::Project,
            CadenceSkillSourceKind::Github,
            CadenceSkillSourceKind::Dynamic,
            CadenceSkillSourceKind::Mcp,
            CadenceSkillSourceKind::Plugin,
        ]
    );

    let merged = merge_skill_source_records(sources).expect("all source ids are canonical");
    assert_eq!(merged.len(), 7);
}

#[test]
fn skill_source_contract_assigns_canonical_ids_and_round_trips_github_runtime_metadata() {
    let record = CadenceSkillSourceRecord::github_autonomous(
        CadenceSkillSourceScope::global(),
        &github_source("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        CadenceSkillSourceState::Installed,
        CadenceSkillTrustState::Trusted,
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

    let project_record = CadenceSkillSourceRecord::new(
        CadenceSkillSourceScope::project("project-alpha").expect("project scope"),
        CadenceSkillSourceLocator::Project {
            relative_path: "skills/write-docs".into(),
            skill_id: "write-docs".into(),
        },
        CadenceSkillSourceState::Discoverable,
        CadenceSkillTrustState::ApprovalRequired,
    )
    .expect("project skill source should normalize");

    assert_eq!(
        project_record.source_id,
        "skill-source:v1:project:project-alpha:project:skills/write-docs:write-docs"
    );
    assert_eq!(
        project_record.contract_version,
        CADENCE_SKILL_SOURCE_CONTRACT_VERSION
    );
}

#[test]
fn skill_source_contract_requires_kind_specific_scope_and_fields() {
    let bundled_project_error = CadenceSkillSourceRecord::new(
        CadenceSkillSourceScope::project("project-alpha").expect("project scope"),
        CadenceSkillSourceLocator::Bundled {
            bundle_id: "cadence".into(),
            skill_id: "write-docs".into(),
            version: "2026.04.25".into(),
        },
        CadenceSkillSourceState::Enabled,
        CadenceSkillTrustState::Trusted,
    )
    .expect_err("bundled skills must be global");
    assert_eq!(bundled_project_error.code, "skill_source_scope_invalid");

    let global_project_error = CadenceSkillSourceRecord::new(
        CadenceSkillSourceScope::global(),
        CadenceSkillSourceLocator::Project {
            relative_path: "skills/write-docs".into(),
            skill_id: "write-docs".into(),
        },
        CadenceSkillSourceState::Discoverable,
        CadenceSkillTrustState::ApprovalRequired,
    )
    .expect_err("project skills must be project-scoped");
    assert_eq!(global_project_error.code, "skill_source_scope_invalid");

    let bad_github_error = CadenceSkillSourceRecord::github_autonomous(
        CadenceSkillSourceScope::global(),
        &github_source("not-a-tree-hash"),
        CadenceSkillSourceState::Installed,
        CadenceSkillTrustState::Trusted,
    )
    .expect_err("GitHub sources require tree hashes for runtime compatibility");
    assert_eq!(
        bad_github_error.code,
        "autonomous_skill_source_metadata_invalid"
    );
}

#[test]
fn skill_source_contract_merges_duplicates_by_source_identity() {
    let discovered = CadenceSkillSourceRecord::github_autonomous(
        CadenceSkillSourceScope::global(),
        &github_source("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        CadenceSkillSourceState::Discoverable,
        CadenceSkillTrustState::Trusted,
    )
    .expect("GitHub source");
    let installed_new_tree = CadenceSkillSourceRecord::github_autonomous(
        CadenceSkillSourceScope::global(),
        &github_source("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        CadenceSkillSourceState::Installed,
        CadenceSkillTrustState::ApprovalRequired,
    )
    .expect("GitHub source");

    assert_eq!(discovered.source_id, installed_new_tree.source_id);

    let merged =
        merge_skill_source_records([discovered, installed_new_tree]).expect("merge sources");

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].state, CadenceSkillSourceState::Stale);
    assert_eq!(merged[0].trust, CadenceSkillTrustState::ApprovalRequired);
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
        CadenceSkillSourceState::Installed,
        CadenceSkillSourceState::Enabled,
    )
    .expect("installed sources can be enabled");
    validate_skill_source_state_transition(
        CadenceSkillSourceState::Enabled,
        CadenceSkillSourceState::Disabled,
    )
    .expect("enabled sources can be disabled");

    let discoverable_to_enabled = validate_skill_source_state_transition(
        CadenceSkillSourceState::Discoverable,
        CadenceSkillSourceState::Enabled,
    )
    .expect_err("discoverable sources must install or be reviewed first");
    assert_eq!(
        discoverable_to_enabled.code,
        "skill_source_state_transition_unsupported"
    );

    let blocked_to_enabled = validate_skill_source_state_transition(
        CadenceSkillSourceState::Blocked,
        CadenceSkillSourceState::Enabled,
    )
    .expect_err("blocked sources cannot be enabled directly");
    assert_eq!(
        blocked_to_enabled.code,
        "skill_source_state_transition_unsupported"
    );
}
