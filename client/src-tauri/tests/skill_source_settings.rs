use std::fs;

use cadence_desktop_lib::runtime::{
    load_skill_source_settings_from_path, persist_skill_source_settings, SkillLocalRootSetting,
    SkillSourceSettings,
};

fn settings_path(root: &tempfile::TempDir) -> std::path::PathBuf {
    root.path().join("skill-sources.json")
}

#[test]
fn skill_source_settings_persist_global_project_and_github_sources() {
    let root = tempfile::tempdir().expect("temp dir");
    let local_root = root.path().join("team-skills");
    fs::create_dir_all(&local_root).expect("local root");

    let settings = SkillSourceSettings {
        local_roots: Vec::new(),
        projects: Vec::new(),
        ..SkillSourceSettings::default()
    }
    .upsert_local_root(
        Some("team-skills".into()),
        local_root.to_string_lossy().into_owned(),
        true,
    )
    .expect("local root setting")
    .update_project("project-1".into(), false)
    .expect("project setting")
    .update_github(
        "acme/skills".into(),
        "stable".into(),
        "catalog".into(),
        false,
    )
    .expect("github setting");

    let saved =
        persist_skill_source_settings(&settings_path(&root), settings).expect("persist settings");
    assert_eq!(saved.local_roots.len(), 1);
    assert_eq!(saved.local_roots[0].root_id, "team-skills");
    assert_eq!(
        saved.local_roots[0].path,
        fs::canonicalize(&local_root)
            .expect("canonical root")
            .to_string_lossy()
            .into_owned()
    );
    assert!(!saved.project_discovery_enabled("project-1"));
    assert!(saved.project_discovery_enabled("project-2"));
    assert_eq!(saved.github.repo, "acme/skills");
    assert_eq!(saved.github.reference, "stable");
    assert_eq!(saved.github.root, "catalog");
    assert!(!saved.github.enabled);

    let loaded =
        load_skill_source_settings_from_path(&settings_path(&root)).expect("load persisted");
    assert_eq!(loaded, saved);

    let removed = loaded
        .remove_local_root("team-skills")
        .expect("remove local root");
    let reloaded = persist_skill_source_settings(&settings_path(&root), removed)
        .expect("persist removed root");
    assert!(reloaded.local_roots.is_empty());
}

#[test]
fn skill_source_settings_reject_unsafe_and_duplicate_local_roots() {
    let root = tempfile::tempdir().expect("temp dir");
    let local_root = root.path().join("team-skills");
    fs::create_dir_all(&local_root).expect("local root");
    let canonical = fs::canonicalize(&local_root)
        .expect("canonical root")
        .to_string_lossy()
        .into_owned();

    let unsafe_path_error = SkillSourceSettings {
        local_roots: Vec::new(),
        ..SkillSourceSettings::default()
    }
    .upsert_local_root(None, "relative/skills".into(), true)
    .expect_err("relative roots are rejected");
    assert_eq!(unsafe_path_error.code, "skill_source_path_unsafe");
    assert!(
        unsafe_path_error.message.contains("absolute paths"),
        "unexpected message: {}",
        unsafe_path_error.message
    );

    let duplicate_error = SkillSourceSettings {
        local_roots: vec![
            SkillLocalRootSetting {
                root_id: "team-a".into(),
                path: canonical.clone(),
                enabled: true,
                updated_at: "2026-04-24T05:00:00Z".into(),
            },
            SkillLocalRootSetting {
                root_id: "team-b".into(),
                path: canonical,
                enabled: true,
                updated_at: "2026-04-24T05:00:00Z".into(),
            },
        ],
        ..SkillSourceSettings::default()
    }
    .validate()
    .expect_err("duplicate canonical roots are rejected");

    assert_eq!(duplicate_error.code, "skill_source_settings_duplicate_root");
}
