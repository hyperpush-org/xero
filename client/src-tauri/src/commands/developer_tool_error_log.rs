use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::CommandResult,
    developer_tool_error_log::{
        clear_tool_call_error_log, ensure_developer_tool_error_log_available,
        list_tool_call_error_log_entries, DeveloperToolErrorLogClearResponseDto,
        DeveloperToolErrorLogListRequestDto, DeveloperToolErrorLogListResponseDto,
    },
    state::DesktopState,
};

#[tauri::command]
pub fn developer_tool_error_log_list<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: Option<DeveloperToolErrorLogListRequestDto>,
) -> CommandResult<DeveloperToolErrorLogListResponseDto> {
    ensure_developer_tool_error_log_available()?;
    let app_data_dir = state.app_data_dir(&app)?;
    list_tool_call_error_log_entries(&app_data_dir, request.unwrap_or_default())
}

#[tauri::command]
pub fn developer_tool_error_log_clear<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<DeveloperToolErrorLogClearResponseDto> {
    ensure_developer_tool_error_log_available()?;
    let app_data_dir = state.app_data_dir(&app)?;
    clear_tool_call_error_log(&app_data_dir)
}
