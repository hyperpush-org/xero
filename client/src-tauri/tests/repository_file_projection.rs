use std::{
    fs,
    path::{Path, PathBuf},
};

use cadence_desktop_lib::{
    commands::{
        create_project_entry, delete_project_entry, import_repository, list_project_files,
        read_project_file, rename_project_entry, write_project_file, CommandError,
        CreateProjectEntryRequestDto, ImportRepositoryRequestDto, ProjectEntryKindDto,
        ProjectFileNodeDto, ProjectFileRequestDto, ProjectIdRequestDto,
        RenameProjectEntryRequestDto, WriteProjectFileRequestDto,
    },
    configure_builder_with_state,
    state::DesktopState,
};
use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("project-registry.json")
}

fn create_state(registry_root: &TempDir) -> DesktopState {
    DesktopState::default().with_registry_file_override(registry_path(registry_root))
}

fn import_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: impl AsRef<Path>,
) -> Result<cadence_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
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
) -> Result<cadence_desktop_lib::commands::ListProjectFilesResponseDto, CommandError> {
    list_project_files(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.to_owned(),
        },
    )
}

fn read_file_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
) -> Result<cadence_desktop_lib::commands::ReadProjectFileResponseDto, CommandError> {
    read_project_file(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
        },
    )
}

fn write_file_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
    content: &str,
) -> Result<cadence_desktop_lib::commands::WriteProjectFileResponseDto, CommandError> {
    write_project_file(
        app.handle().clone(),
        app.state::<DesktopState>(),
        WriteProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
            content: content.to_owned(),
        },
    )
}

fn create_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    parent_path: &str,
    name: &str,
    entry_type: ProjectEntryKindDto,
) -> Result<cadence_desktop_lib::commands::CreateProjectEntryResponseDto, CommandError> {
    create_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CreateProjectEntryRequestDto {
            project_id: project_id.to_owned(),
            parent_path: parent_path.to_owned(),
            name: name.to_owned(),
            entry_type,
        },
    )
}

fn rename_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
    new_name: &str,
) -> Result<cadence_desktop_lib::commands::RenameProjectEntryResponseDto, CommandError> {
    rename_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RenameProjectEntryRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
            new_name: new_name.to_owned(),
        },
    )
}

fn delete_entry_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    path: &str,
) -> Result<cadence_desktop_lib::commands::DeleteProjectEntryResponseDto, CommandError> {
    delete_project_entry(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectFileRequestDto {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
        },
    )
}

fn init_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let repository = Repository::init(temp_dir.path()).expect("git repo");

    fs::write(temp_dir.path().join("README.md"), "Cadence\n").expect("write README");
    fs::create_dir_all(temp_dir.path().join("src")).expect("create src");
    fs::write(
        temp_dir.path().join("src").join("App.tsx"),
        "export default function App() {\n  return <main>Cadence</main>\n}\n",
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
    let signature = Signature::now("Cadence", "Cadence@example.com").expect("signature");

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
    assert!(find_node(&tree.root, "/README.md").is_some());
    assert!(find_node(&tree.root, "/src").is_some());
    assert!(find_node(&tree.root, "/src/App.tsx").is_some());
    assert!(find_node(&tree.root, "/node_modules").is_none());
    assert!(find_node(&tree.root, "/.git").is_none());

    let readme = read_file_with_app(&app, &project_id, "/README.md").expect("readme loads");
    assert_eq!(readme.content, "Cadence\n");

    write_file_with_app(&app, &project_id, "/README.md", "Cadence\nUpdated\n")
        .expect("write succeeds");
    assert_eq!(
        fs::read_to_string(repository_root.path().join("README.md")).expect("read written README"),
        "Cadence\nUpdated\n"
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
    assert!(repository_root.path().join("src").join("editor.ts").is_file());

    let created_folder = create_entry_with_app(
        &app,
        &project_id,
        "/src",
        "generated",
        ProjectEntryKindDto::Folder,
    )
    .expect("create folder succeeds");
    assert_eq!(created_folder.path, "/src/generated");
    assert!(repository_root.path().join("src").join("generated").is_dir());

    let renamed = rename_entry_with_app(&app, &project_id, "/src/editor.ts", "editor-client.ts")
        .expect("rename succeeds");
    assert_eq!(renamed.path, "/src/editor-client.ts");
    assert!(repository_root
        .path()
        .join("src")
        .join("editor-client.ts")
        .is_file());
    assert!(!repository_root.path().join("src").join("editor.ts").exists());

    delete_entry_with_app(&app, &project_id, "/src/generated").expect("delete folder succeeds");
    assert!(!repository_root.path().join("src").join("generated").exists());

    let refreshed_tree = list_files_with_app(&app, &project_id).expect("refreshed tree loads");
    assert!(find_node(&refreshed_tree.root, "/src/editor-client.ts").is_some());
    assert!(find_node(&refreshed_tree.root, "/src/generated").is_none());
}
