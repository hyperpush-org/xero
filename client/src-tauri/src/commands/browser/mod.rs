mod actions;
mod bridge;
mod events;
mod screenshot;
mod script;
mod tabs;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, State, WebviewUrl,
};

use crate::commands::{CommandError, CommandResult};

pub use actions::{StorageArea, TypingMode};
pub use events::{
    BrowserConsolePayload, BrowserDialogPayload, BrowserDownloadPayload, BrowserLoadStatePayload,
    BrowserTabUpdatedPayload, BrowserUrlChangedPayload, BROWSER_CONSOLE_EVENT,
    BROWSER_DIALOG_EVENT, BROWSER_DOWNLOAD_EVENT, BROWSER_LOAD_STATE_EVENT,
    BROWSER_TAB_UPDATED_EVENT, BROWSER_URL_CHANGED_EVENT,
};
pub use tabs::{BrowserTabMetadata, BROWSER_LEGACY_LABEL, BROWSER_TAB_PREFIX};

use bridge::{BridgeReply, BridgeWaiters};
use script::BROWSER_BRIDGE_INIT_SCRIPT;
use tabs::{BrowserTabs, BROWSER_MAIN_WINDOW_LABEL};

pub const BROWSER_WEBVIEW_LABEL: &str = BROWSER_LEGACY_LABEL;
const HIDDEN_OFFSET: f64 = -32_000.0;

pub struct BrowserState {
    creation_lock: Mutex<()>,
    waiters: Arc<BridgeWaiters>,
    tabs: Arc<BrowserTabs>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            creation_lock: Mutex::new(()),
            waiters: Arc::new(BridgeWaiters::new()),
            tabs: Arc::new(BrowserTabs::new()),
        }
    }
}

impl BrowserState {
    pub fn tabs(&self) -> Arc<BrowserTabs> {
        Arc::clone(&self.tabs)
    }

    pub fn waiters(&self) -> Arc<BridgeWaiters> {
        Arc::clone(&self.waiters)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserShowRequest {
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserInternalReplyPayload {
    pub request_id: String,
    pub ok: bool,
    pub value: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserInternalEventPayload {
    pub kind: String,
    pub payload: Option<String>,
}

#[tauri::command]
pub fn browser_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tab_id: Option<String>,
) -> CommandResult<BrowserTabMetadata> {
    let target = actions::parse_url(&url)?;
    let tabs = state.tabs();

    let _guard = state.creation_lock.lock().map_err(|_| {
        CommandError::system_fault("browser_lock_poisoned", "Browser state lock poisoned.")
    })?;

    let (tab_id, label) = match tab_id {
        Some(existing) => {
            let label = tabs
                .list()?
                .into_iter()
                .find(|tab| tab.id == existing)
                .map(|tab| tab.label)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "browser_tab_not_found",
                        format!("Browser tab `{existing}` was not found."),
                    )
                })?;
            tabs.set_active(&existing)?;
            (existing, label)
        }
        None => {
            if let Some(active) = tabs.active_tab_id() {
                let label = tabs.active_label_soft().unwrap_or_else(|| BROWSER_LEGACY_LABEL.to_string());
                (active, label)
            } else {
                let (id, label) = tabs.new_tab_label();
                // First tab gets the legacy label so existing screenshot code keeps working.
                let label = if id.ends_with("-1") { BROWSER_LEGACY_LABEL.to_string() } else { label };
                tabs.insert(id.clone(), label.clone())?;
                (id, label)
            }
        }
    };

    if let Some(existing) = app.get_webview(&label) {
        existing
            .set_position(LogicalPosition::new(x, y))
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_set_position_failed",
                    format!("Cadence could not move the browser webview: {error}"),
                )
            })?;
        existing
            .set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_set_size_failed",
                    format!("Cadence could not resize the browser webview: {error}"),
                )
            })?;
        existing.navigate(target.clone()).map_err(|error| {
            CommandError::system_fault(
                "browser_navigate_failed",
                format!("Cadence could not navigate the browser webview: {error}"),
            )
        })?;
        tabs.record_page_state(&tab_id, Some(target.to_string()), None, Some(true));
        emit_tab_list(&app, &tabs);
        return Ok(current_tab_meta(&tabs, &tab_id));
    }

    let window = app.get_window(BROWSER_MAIN_WINDOW_LABEL).ok_or_else(|| {
        CommandError::system_fault(
            "browser_main_window_missing",
            "Cadence could not locate the main window to attach the browser webview.",
        )
    })?;

    let tab_id_for_nav = tab_id.clone();
    let tabs_for_nav = Arc::clone(&tabs);
    let app_for_nav = app.clone();

    let tab_id_for_load = tab_id.clone();
    let tabs_for_load = Arc::clone(&tabs);
    let app_for_load = app.clone();

    let builder = WebviewBuilder::new(label.clone(), WebviewUrl::External(target.clone()))
        .initialization_script(BROWSER_BRIDGE_INIT_SCRIPT)
        .on_navigation(move |url| {
            tabs_for_nav.record_page_state(&tab_id_for_nav, Some(url.to_string()), None, Some(true));
            events::emit(
                &app_for_nav,
                BROWSER_URL_CHANGED_EVENT,
                &BrowserUrlChangedPayload {
                    tab_id: tab_id_for_nav.clone(),
                    url: url.to_string(),
                    title: None,
                    can_go_back: false,
                    can_go_forward: false,
                },
            );
            true
        })
        .on_page_load(move |_webview, payload| {
            let url = payload.url().to_string();
            let loading = matches!(payload.event(), PageLoadEvent::Started);
            tabs_for_load.record_page_state(
                &tab_id_for_load,
                Some(url.clone()),
                None,
                Some(loading),
            );
            events::emit(
                &app_for_load,
                BROWSER_LOAD_STATE_EVENT,
                &BrowserLoadStatePayload {
                    tab_id: tab_id_for_load.clone(),
                    loading,
                    url: Some(url),
                    error: None,
                },
            );
        });

    window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(1.0), height.max(1.0)),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "browser_create_failed",
                format!("Cadence could not create the browser webview: {error}"),
            )
        })?;

    tabs.record_page_state(&tab_id, Some(target.to_string()), None, Some(true));
    emit_tab_list(&app, &tabs);
    Ok(current_tab_meta(&tabs, &tab_id))
}

#[tauri::command]
pub fn browser_resize<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let tabs = state.tabs();
    let label = resolve_label(&tabs, tab_id.as_deref())?;
    let Some(webview) = app.get_webview(&label) else {
        return Ok(());
    };

    webview
        .set_position(LogicalPosition::new(x, y))
        .map_err(|error| {
            CommandError::system_fault(
                "browser_set_position_failed",
                format!("Cadence could not move the browser webview: {error}"),
            )
        })?;
    webview
        .set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
        .map_err(|error| {
            CommandError::system_fault(
                "browser_set_size_failed",
                format!("Cadence could not resize the browser webview: {error}"),
            )
        })?;
    Ok(())
}

#[tauri::command]
pub fn browser_hide<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let tabs = state.tabs();
    let labels = match tab_id {
        Some(id) => vec![resolve_label(&tabs, Some(&id))?],
        None => tabs
            .list()?
            .into_iter()
            .map(|tab| tab.label)
            .collect::<Vec<_>>(),
    };

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            webview
                .set_position(LogicalPosition::new(HIDDEN_OFFSET, HIDDEN_OFFSET))
                .map_err(|error| {
                    CommandError::system_fault(
                        "browser_set_position_failed",
                        format!("Cadence could not hide the browser webview: {error}"),
                    )
                })?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn browser_eval<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    js: String,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    if js.trim().is_empty() {
        return Err(CommandError::invalid_request("js"));
    }
    let tabs = state.tabs();
    let waiters = state.waiters();
    let body = format!("return (function(){{ {js} }})();", js = js);
    bridge::run_script(&app, &tabs, &waiters, &body, actions::resolve_timeout(timeout_ms))
}

#[tauri::command]
pub fn browser_current_url<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<Option<String>> {
    let Some(webview) = state.tabs().optional_active_webview(&app) else {
        return Ok(None);
    };
    let url = webview.url().map_err(|error| {
        CommandError::system_fault(
            "browser_url_failed",
            format!("Cadence could not read the browser URL: {error}"),
        )
    })?;
    Ok(Some(url.to_string()))
}

#[tauri::command]
pub fn browser_screenshot<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<String> {
    let webview = state.tabs().active_webview(&app)?;
    screenshot::capture_webview(&webview)
}

#[tauri::command]
pub fn browser_navigate<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    url: String,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let target = actions::parse_url(&url)?;
    let tabs = state.tabs();
    let label = resolve_label(&tabs, tab_id.as_deref())?;
    let Some(webview) = app.get_webview(&label) else {
        return Err(CommandError::user_fixable(
            "browser_not_open",
            "The in-app browser is not currently open.",
        ));
    };
    webview.navigate(target).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Cadence could not navigate the browser webview: {error}"),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn browser_back<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<JsonValue> {
    actions::history_navigate(&app, &state.tabs(), &state.waiters(), -1)
}

#[tauri::command]
pub fn browser_forward<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<JsonValue> {
    actions::history_navigate(&app, &state.tabs(), &state.waiters(), 1)
}

#[tauri::command]
pub fn browser_reload<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let tabs = state.tabs();
    let label = resolve_label(&tabs, tab_id.as_deref())?;
    let Some(webview) = app.get_webview(&label) else {
        return Err(CommandError::user_fixable(
            "browser_not_open",
            "The in-app browser is not currently open.",
        ));
    };
    let current = webview
        .url()
        .map_err(|error| {
            CommandError::system_fault(
                "browser_url_failed",
                format!("Cadence could not read the browser URL: {error}"),
            )
        })?;
    webview.navigate(current).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Cadence could not reload the browser webview: {error}"),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn browser_stop<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<JsonValue> {
    actions::stop(&app, &state.tabs(), &state.waiters())
}

#[tauri::command]
pub fn browser_click<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: String,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::click(
        &app,
        &state.tabs(),
        &state.waiters(),
        &selector,
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_type<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: String,
    text: String,
    append: Option<bool>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let mode = if append.unwrap_or(false) {
        TypingMode::Append
    } else {
        TypingMode::Replace
    };
    actions::type_text(
        &app,
        &state.tabs(),
        &state.waiters(),
        &selector,
        &text,
        mode,
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_scroll<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::scroll_to(
        &app,
        &state.tabs(),
        &state.waiters(),
        selector.as_deref(),
        x,
        y,
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_press_key<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: Option<String>,
    key: String,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::press_key(
        &app,
        &state.tabs(),
        &state.waiters(),
        selector.as_deref(),
        &key,
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_read_text<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: Option<String>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::read_text(
        &app,
        &state.tabs(),
        &state.waiters(),
        selector.as_deref(),
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_query<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: String,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::query(
        &app,
        &state.tabs(),
        &state.waiters(),
        &selector,
        limit,
        timeout_ms,
    )
}

#[tauri::command]
pub fn browser_wait_for_selector<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    selector: String,
    timeout_ms: Option<u64>,
    visible: Option<bool>,
) -> CommandResult<JsonValue> {
    actions::wait_for_selector(
        &app,
        &state.tabs(),
        &state.waiters(),
        &selector,
        timeout_ms,
        visible.unwrap_or(true),
    )
}

#[tauri::command]
pub fn browser_wait_for_load<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    actions::wait_for_load(&app, &state.tabs(), &state.waiters(), timeout_ms)
}

#[tauri::command]
pub fn browser_history_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<JsonValue> {
    actions::history_state(&app, &state.tabs(), &state.waiters())
}

#[tauri::command]
pub fn browser_cookies_get<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<JsonValue> {
    actions::cookies_get(&app, &state.tabs(), &state.waiters())
}

#[tauri::command]
pub fn browser_cookies_set<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    cookie: String,
) -> CommandResult<JsonValue> {
    actions::cookies_set(&app, &state.tabs(), &state.waiters(), &cookie)
}

#[tauri::command]
pub fn browser_storage_read<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    area: StorageArea,
    key: Option<String>,
) -> CommandResult<JsonValue> {
    actions::storage_read(
        &app,
        &state.tabs(),
        &state.waiters(),
        area,
        key.as_deref(),
    )
}

#[tauri::command]
pub fn browser_storage_write<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    area: StorageArea,
    key: String,
    value: Option<String>,
) -> CommandResult<JsonValue> {
    actions::storage_write(
        &app,
        &state.tabs(),
        &state.waiters(),
        area,
        &key,
        value.as_deref(),
    )
}

#[tauri::command]
pub fn browser_storage_clear<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    area: StorageArea,
) -> CommandResult<JsonValue> {
    actions::storage_clear(&app, &state.tabs(), &state.waiters(), area)
}

#[tauri::command]
pub fn browser_tab_list<R: Runtime>(
    _app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    state.tabs().list()
}

#[tauri::command]
pub fn browser_tab_focus<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: String,
) -> CommandResult<BrowserTabMetadata> {
    let tabs = state.tabs();
    tabs.set_active(&tab_id)?;
    emit_tab_list(&app, &tabs);
    Ok(current_tab_meta(&tabs, &tab_id))
}

#[tauri::command]
pub fn browser_tab_close<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: String,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    let tabs = state.tabs();
    let removed_label = tabs.remove(&tab_id)?;
    if let Some(label) = removed_label {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
    }
    emit_tab_list(&app, &tabs);
    tabs.list()
}

#[tauri::command]
pub fn browser_internal_reply<R: Runtime>(
    _app: AppHandle<R>,
    state: State<'_, BrowserState>,
    request_id: String,
    ok: bool,
    value: Option<String>,
    error: Option<String>,
) -> CommandResult<()> {
    let parsed = match value {
        Some(raw) if !raw.is_empty() => match serde_json::from_str::<JsonValue>(&raw) {
            Ok(parsed) => Some(parsed),
            Err(_) => Some(JsonValue::String(raw)),
        },
        _ => None,
    };
    state.waiters().resolve(
        &request_id,
        BridgeReply {
            ok,
            value: parsed,
            error,
        },
    );
    Ok(())
}

#[tauri::command]
pub fn browser_internal_event<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    kind: String,
    payload: Option<String>,
) -> CommandResult<()> {
    let Some(tab_id) = state.tabs().active_tab_id() else {
        return Ok(());
    };
    let parsed: JsonValue = payload
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(JsonValue::Null);

    match kind.as_str() {
        "page" => {
            let url = parsed.get("url").and_then(|v| v.as_str()).map(String::from);
            let title = parsed.get("title").and_then(|v| v.as_str()).map(String::from);
            let ready_state = parsed
                .get("readyState")
                .and_then(|v| v.as_str())
                .unwrap_or("loading");
            let loading = ready_state != "complete";
            state.tabs().record_page_state(&tab_id, url.clone(), title.clone(), Some(loading));
            if let Some(url) = url {
                events::emit(
                    &app,
                    BROWSER_URL_CHANGED_EVENT,
                    &BrowserUrlChangedPayload {
                        tab_id: tab_id.clone(),
                        url: url.clone(),
                        title,
                        can_go_back: false,
                        can_go_forward: false,
                    },
                );
                events::emit(
                    &app,
                    BROWSER_LOAD_STATE_EVENT,
                    &BrowserLoadStatePayload {
                        tab_id: tab_id.clone(),
                        loading,
                        url: Some(url),
                        error: None,
                    },
                );
            }
            emit_tab_list(&app, &state.tabs());
        }
        "console" => {
            let level = parsed
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("log")
                .to_string();
            let message = parsed
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            events::emit(
                &app,
                BROWSER_CONSOLE_EVENT,
                &BrowserConsolePayload {
                    tab_id,
                    level,
                    message,
                },
            );
        }
        "error" => {
            let message = parsed
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string();
            events::emit(
                &app,
                BROWSER_CONSOLE_EVENT,
                &BrowserConsolePayload {
                    tab_id,
                    level: "error".to_string(),
                    message,
                },
            );
        }
        _ => {}
    }
    Ok(())
}

fn resolve_label(tabs: &BrowserTabs, requested: Option<&str>) -> CommandResult<String> {
    match requested {
        Some(id) => tabs
            .list()?
            .into_iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.label)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_tab_not_found",
                    format!("Browser tab `{id}` was not found."),
                )
            }),
        None => tabs.active_label_soft().ok_or_else(|| {
            CommandError::user_fixable(
                "browser_not_open",
                "The in-app browser is not currently open.",
            )
        }),
    }
}

fn current_tab_meta(tabs: &BrowserTabs, id: &str) -> BrowserTabMetadata {
    tabs.list()
        .ok()
        .and_then(|list| list.into_iter().find(|tab| tab.id == id))
        .unwrap_or(BrowserTabMetadata {
            id: id.to_string(),
            label: BROWSER_LEGACY_LABEL.to_string(),
            title: None,
            url: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            active: true,
        })
}

fn emit_tab_list<R: Runtime>(app: &AppHandle<R>, tabs: &BrowserTabs) {
    if let Ok(list) = tabs.list() {
        events::emit(
            app,
            BROWSER_TAB_UPDATED_EVENT,
            &BrowserTabUpdatedPayload { tabs: list },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_metadata_roundtrip() {
        let tabs = BrowserTabs::new();
        let (id, label) = tabs.new_tab_label();
        tabs.insert(id.clone(), label.clone()).unwrap();
        tabs.record_page_state(
            &id,
            Some("https://example.com/".to_string()),
            Some("Example".to_string()),
            Some(false),
        );
        let list = tabs.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].url.as_deref(), Some("https://example.com/"));
        assert!(list[0].active);
    }

    #[test]
    fn tab_removal_switches_active() {
        let tabs = BrowserTabs::new();
        let (id_a, label_a) = tabs.new_tab_label();
        let (id_b, label_b) = tabs.new_tab_label();
        tabs.insert(id_a.clone(), label_a).unwrap();
        tabs.insert(id_b.clone(), label_b).unwrap();
        tabs.set_active(&id_b).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(id_b.as_str()));
        tabs.remove(&id_b).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(id_a.as_str()));
    }
}
