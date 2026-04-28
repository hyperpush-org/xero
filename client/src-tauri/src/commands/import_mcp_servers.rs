use std::path::PathBuf;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        CommandError, CommandResult, ImportMcpServersRequestDto, ImportMcpServersResponseDto,
    },
    mcp::{
        apply_mcp_registry_import, load_mcp_registry_from_path, parse_mcp_registry_import_file,
        persist_mcp_registry,
    },
    state::DesktopState,
};

use super::list_mcp_servers::{
    mcp_import_diagnostic_dto_from_record, mcp_registry_dto_from_snapshot,
};

#[tauri::command]
pub fn import_mcp_servers<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ImportMcpServersRequestDto,
) -> CommandResult<ImportMcpServersResponseDto> {
    let import_path = request.path.trim();
    if import_path.is_empty() {
        return Err(CommandError::invalid_request("path"));
    }

    let import_path = PathBuf::from(import_path);
    let registry_path = state.global_db_path(&app)?;

    let current = load_mcp_registry_from_path(&registry_path)?;
    let entries = parse_mcp_registry_import_file(&import_path)?;
    let import_result = apply_mcp_registry_import(&current, entries, &import_path);

    let snapshot = if import_result.registry == current {
        current
    } else {
        persist_mcp_registry(&registry_path, &import_result.registry)?
    };

    Ok(ImportMcpServersResponseDto {
        registry: mcp_registry_dto_from_snapshot(&snapshot),
        diagnostics: import_result
            .diagnostics
            .iter()
            .map(mcp_import_diagnostic_dto_from_record)
            .collect::<Vec<_>>(),
    })
}
