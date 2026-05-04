use std::{
    fs,
    path::{Path, PathBuf},
};

use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        create_project_entry, delete_project_entry, import_repository, list_project_files,
        read_project_file, rename_project_entry, write_project_file, CommandError,
        CreateProjectEntryRequestDto, ImportRepositoryRequestDto, ListProjectFilesRequestDto,
        ProjectAssetState, ProjectEntryKindDto, ProjectFileNodeDto, ProjectFileRendererKindDto,
        ProjectFileRequestDto, ReadProjectFileResponseDto, RenameProjectEntryRequestDto,
        WriteProjectFileRequestDto,
    },
    configure_builder_with_state,
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("xero.db")
}

fn create_state(registry_root: &TempDir) -> DesktopState {
    DesktopState::default().with_global_db_path_override(registry_path(registry_root))
}

fn import_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: impl AsRef<Path>,
) -> Result<xero_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
    import_repository(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ImportRepositoryRequestDto {
            path: path.as_ref().to_string_lossy().into_owned(),
        },
    )
}

fn list_files_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
) -> Result<xero_desktop_lib::commands::ListProjectFilesResponseDto, CommandError> {
    list_files_at_with_app(app, project_id, "/")
}

fn list_files_at_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
) -> Result<xero_desktop_lib::commands::ListProjectFilesResponseDto, CommandError> {
    tauri::async_runtime::block_on(list_project_files(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListProjectFilesRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
        },
    ))
}

fn read_file_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
) -> Result<xero_desktop_lib::commands::ReadProjectFileResponseDto, CommandError> {
    tauri::async_runtime::block_on(read_project_file(
        app.handle().clone(),
        app.state::<DesktopState>(),
        app.state::<ProjectAssetState>(),
        ProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
        },
    ))
}

fn write_file_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
    content: &str,
) -> Result<xero_desktop_lib::commands::WriteProjectFileResponseDto, CommandError> {
    tauri::async_runtime::block_on(write_project_file(
        app.handle().clone(),
        app.state::<DesktopState>(),
        WriteProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
            content: content.to_owned(),
        },
    ))
}

fn create_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    parent_path: &str,
    name: &str,
    entry_type: ProjectEntryKindDto,
) -> Result<xero_desktop_lib::commands::CreateProjectEntryResponseDto, CommandError> {
    tauri::async_runtime::block_on(create_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CreateProjectEntryRequestDto {
            project_id: project_id.to_owned(),
            parent_path: parent_path.to_owned(),
            name: name.to_owned(),
            entry_type,
        },
    ))
}

fn rename_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
    new_name: &str,
) -> Result<xero_desktop_lib::commands::RenameProjectEntryResponseDto, CommandError> {
    tauri::async_runtime::block_on(rename_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RenameProjectEntryRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
            new_name: new_name.to_owned(),
        },
    ))
}

fn delete_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
) -> Result<xero_desktop_lib::commands::DeleteProjectEntryResponseDto, CommandError> {
    tauri::async_runtime::block_on(delete_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
        },
    ))
}

fn init_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let repository = Repository::init(temp_dir.path()).expect("git repo");

    fs::write(temp_dir.path().join("README.md"), "Xero\n").expect("write README");
    fs::create_dir_all(temp_dir.path().join("src")).expect("create src");
    fs::write(
        temp_dir.path().join("src").join("App.tsx"),
        "export default function App() {\n  return <main>Xero</main>\n}\n",
    )
    .expect("write app");
    fs::create_dir_all(temp_dir.path().join("node_modules")).expect("create node_modules");
    fs::write(
        temp_dir.path().join("node_modules").join("ignored.js"),
        "console.log('ignored')\n",
    )
    .expect("write ignored file");
    fs::write(temp_dir.path().join(".gitignore"), "node_modules\n").expect("write gitignore");
    commit_all(&repository, "initial commit");

    temp_dir
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage files");
    index.write().expect("write index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repository.find_tree(tree_id).expect("find tree");
    let signature = Signature::now("Xero", "Xero@example.com").expect("signature");

    let parents = repository
        .head()
        .ok()
        .and_then(|head| head.target())
        .and_then(|oid| repository.find_commit(oid).ok())
        .into_iter()
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();

    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit");
}

fn find_node<'a>(node: &'a ProjectFileNodeDto, path: &str) -> Option<&'a ProjectFileNodeDto> {
    if node.path == path {
        return Some(node);
    }

    for child in &node.children {
        if let Some(found) = find_node(child, path) {
            return Some(found);
        }
    }

    None
}

#[test]
fn project_file_commands_list_read_write_create_rename_and_delete_real_repo_state() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let project_id = imported.project.id;

    let tree = list_files_with_app(&app, &project_id).expect("project tree loads");
    assert!(!tree.truncated);
    assert_eq!(tree.omitted_entry_count, 0);
    assert!(tree.payload_budget.is_none());
    assert!(find_node(&tree.root, "/README.md").is_some());
    assert!(find_node(&tree.root, "/src").is_some());
    assert!(find_node(&tree.root, "/node_modules").is_none());
    assert!(find_node(&tree.root, "/.git").is_none());

    let src_tree = list_files_at_with_app(&app, &project_id, "/src").expect("src tree loads");
    assert!(find_node(&src_tree.root, "/src/App.tsx").is_some());

    let readme = read_file_with_app(&app, &project_id, "/README.md").expect("readme loads");
    match readme {
        ReadProjectFileResponseDto::Text {
            text,
            renderer_kind,
            ..
        } => {
            assert_eq!(text, "Xero\n");
            assert_eq!(renderer_kind, ProjectFileRendererKindDto::Markdown);
        }
        other => panic!("expected text response, got {other:?}"),
    }

    write_file_with_app(&app, &project_id, "/README.md", "Xero\nUpdated\n")
        .expect("write succeeds");
    assert_eq!(
        fs::read_to_string(repository_root.path().join("README.md")).expect("read written README"),
        "Xero\nUpdated\n"
    );

    let created_file = create_entry_with_app(
        &app,
        &project_id,
        "/src",
        "editor.ts",
        ProjectEntryKindDto::File,
    )
    .expect("create file succeeds");
    assert_eq!(created_file.path, "/src/editor.ts");
    assert!(repository_root
        .path()
        .join("src")
        .join("editor.ts")
        .is_file());

    let created_folder = create_entry_with_app(
        &app,
        &project_id,
        "/src",
        "generated",
        ProjectEntryKindDto::Folder,
    )
    .expect("create folder succeeds");
    assert_eq!(created_folder.path, "/src/generated");
    assert!(repository_root
        .path()
        .join("src")
        .join("generated")
        .is_dir());

    let renamed = rename_entry_with_app(&app, &project_id, "/src/editor.ts", "editor-client.ts")
        .expect("rename succeeds");
    assert_eq!(renamed.path, "/src/editor-client.ts");
    assert!(repository_root
        .path()
        .join("src")
        .join("editor-client.ts")
        .is_file());
    assert!(!repository_root
        .path()
        .join("src")
        .join("editor.ts")
        .exists());

    delete_entry_with_app(&app, &project_id, "/src/generated").expect("delete folder succeeds");
    assert!(!repository_root
        .path()
        .join("src")
        .join("generated")
        .exists());

    let refreshed_src_tree =
        list_files_at_with_app(&app, &project_id, "/src").expect("refreshed src tree loads");
    assert!(find_node(&refreshed_src_tree.root, "/src/editor-client.ts").is_some());
    assert!(find_node(&refreshed_src_tree.root, "/src/generated").is_none());
}

#[test]
fn read_project_file_classifies_binary_without_utf8_error() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    fs::write(
        repository_root.path().join("payload.bin"),
        [0_u8, 0xff, 0x01, 0x02],
    )
    .expect("write binary");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let response =
        read_file_with_app(&app, &imported.project.id, "/payload.bin").expect("binary classifies");

    match response {
        ReadProjectFileResponseDto::Unsupported {
            reason, mime_type, ..
        } => {
            assert_eq!(reason, "binary");
            assert_eq!(mime_type.as_deref(), Some("application/octet-stream"));
        }
        other => panic!("expected unsupported binary response, got {other:?}"),
    }
}

#[test]
fn read_project_file_returns_preview_url_for_raster_image() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    fs::write(
        repository_root.path().join("pixel.png"),
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00],
    )
    .expect("write png");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let response =
        read_file_with_app(&app, &imported.project.id, "/pixel.png").expect("image classifies");

    match response {
        ReadProjectFileResponseDto::Renderable {
            preview_url,
            renderer_kind,
            mime_type,
            ..
        } => {
            assert!(preview_url.starts_with("project-asset://"));
            assert_eq!(renderer_kind, ProjectFileRendererKindDto::Image);
            assert_eq!(mime_type, "image/png");
        }
        other => panic!("expected renderable image response, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn read_project_file_denies_symlinked_paths() {
    use std::os::unix::fs::symlink;

    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    symlink(
        repository_root.path().join("README.md"),
        repository_root.path().join("readme-link.md"),
    )
    .expect("create symlink");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let error = read_file_with_app(&app, &imported.project.id, "/readme-link.md")
        .expect_err("symlink read should fail");

    assert_eq!(error.code, "policy_denied");
}

#[test]
fn read_project_file_denies_path_traversal_segments() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let error = read_file_with_app(&app, &imported.project.id, "/../README.md")
        .expect_err("path traversal should fail");

    assert_eq!(error.code, "policy_denied");
}
