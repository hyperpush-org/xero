use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime};

use crate::commands::browser::{provision_browser_tab, StorageArea};
use crate::commands::{CommandError, CommandResult};
use crate::state::DesktopState;

pub const AUTONOMOUS_TOOL_BROWSER: &str = "browser";

pub const DEFAULT_BROWSER_ACTION_TIMEOUT_MS: u64 = 10_000;
pub const MAX_BROWSER_ACTION_TIMEOUT_MS: u64 = 60_000;

pub const BROWSER_NOT_OPEN_ERROR_CODE: &str = "browser_not_open";
pub const BROWSER_POLICY_DENIED_CODE: &str = "policy_denied";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousBrowserAction {
    Open {
        url: String,
    },
    TabOpen {
        url: String,
    },
    Navigate {
        url: String,
    },
    Back,
    Forward,
    Reload,
    Stop,
    Click {
        selector: String,
        timeout_ms: Option<u64>,
    },
    Type {
        selector: String,
        text: String,
        append: Option<bool>,
        timeout_ms: Option<u64>,
    },
    Scroll {
        selector: Option<String>,
        x: Option<i64>,
        y: Option<i64>,
        timeout_ms: Option<u64>,
    },
    PressKey {
        selector: Option<String>,
        key: String,
        timeout_ms: Option<u64>,
    },
    ReadText {
        selector: Option<String>,
        timeout_ms: Option<u64>,
    },
    Query {
        selector: String,
        limit: Option<usize>,
        timeout_ms: Option<u64>,
    },
    WaitForSelector {
        selector: String,
        timeout_ms: Option<u64>,
        visible: Option<bool>,
    },
    WaitForLoad {
        timeout_ms: Option<u64>,
    },
    CurrentUrl,
    HistoryState,
    Screenshot,
    CookiesGet,
    CookiesSet {
        cookie: String,
    },
    StorageRead {
        area: StorageArea,
        key: Option<String>,
    },
    StorageWrite {
        area: StorageArea,
        key: String,
        value: Option<String>,
    },
    StorageClear {
        area: StorageArea,
    },
    TabList,
    TabClose {
        tab_id: String,
    },
    TabFocus {
        tab_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousBrowserRequest {
    #[serde(flatten)]
    pub action: AutonomousBrowserAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousBrowserOutput {
    pub action: String,
    pub url: Option<String>,
    /// JSON-serialized result of the action. Held as a string so that the
    /// overall tool output remains `Eq`-derivable (JSON values aren't).
    pub value_json: String,
}

pub trait BrowserExecutor: Send + Sync + std::fmt::Debug {
    fn execute(&self, action: AutonomousBrowserAction) -> CommandResult<AutonomousBrowserOutput>;
}

pub fn execute_action_with_app<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    action: AutonomousBrowserAction,
) -> CommandResult<AutonomousBrowserOutput> {
    use tauri::Manager;
    let browser_state = app
        .try_state::<crate::commands::browser::BrowserState>()
        .ok_or_else(|| {
            CommandError::system_fault(
                "browser_executor_state_missing",
                "Browser state is not registered on the app handle.",
            )
        })?;
    let tabs = browser_state.tabs();
    let waiters = browser_state.waiters();
    let action_name = action_tool_name(&action);
    let _ = state; // reserved for future policy hooks

    use crate::commands::browser::actions as browser_actions;

    let output_value = match action {
        AutonomousBrowserAction::Open { url } => {
            let tab = provision_browser_tab(app, browser_state.inner(), &url, None, false, None)?;
            tab_to_json(tab)
        }
        AutonomousBrowserAction::TabOpen { url } => {
            let tab = provision_browser_tab(app, browser_state.inner(), &url, None, true, None)?;
            tab_to_json(tab)
        }
        AutonomousBrowserAction::Navigate { url } => {
            let target = browser_actions::parse_url(&url)?;
            let label = tabs.active_label_soft().ok_or_else(require_open_error)?;
            let webview = app.get_webview(&label).ok_or_else(require_open_error)?;
            webview.navigate(target.clone()).map_err(|error| {
                CommandError::system_fault(
                    "browser_navigate_failed",
                    format!("Cadence could not navigate the browser webview: {error}"),
                )
            })?;
            JsonValue::String(target.to_string())
        }
        AutonomousBrowserAction::Back => {
            browser_actions::history_navigate(app, &tabs, &waiters, -1)?
        }
        AutonomousBrowserAction::Forward => {
            browser_actions::history_navigate(app, &tabs, &waiters, 1)?
        }
        AutonomousBrowserAction::Reload => {
            let label = tabs.active_label_soft().ok_or_else(require_open_error)?;
            let webview = app.get_webview(&label).ok_or_else(require_open_error)?;
            let current = webview.url().map_err(|error| {
                CommandError::system_fault(
                    "browser_url_failed",
                    format!("Cadence could not read the browser URL: {error}"),
                )
            })?;
            webview.navigate(current.clone()).map_err(|error| {
                CommandError::system_fault(
                    "browser_navigate_failed",
                    format!("Cadence could not reload the browser webview: {error}"),
                )
            })?;
            JsonValue::String(current.to_string())
        }
        AutonomousBrowserAction::Stop => browser_actions::stop(app, &tabs, &waiters)?,
        AutonomousBrowserAction::Click {
            selector,
            timeout_ms,
        } => browser_actions::click(app, &tabs, &waiters, &selector, timeout_ms)?,
        AutonomousBrowserAction::Type {
            selector,
            text,
            append,
            timeout_ms,
        } => {
            let mode = if append.unwrap_or(false) {
                crate::commands::browser::TypingMode::Append
            } else {
                crate::commands::browser::TypingMode::Replace
            };
            browser_actions::type_text(app, &tabs, &waiters, &selector, &text, mode, timeout_ms)?
        }
        AutonomousBrowserAction::Scroll {
            selector,
            x,
            y,
            timeout_ms,
        } => browser_actions::scroll_to(
            app,
            &tabs,
            &waiters,
            selector.as_deref(),
            x.map(|value| value as f64),
            y.map(|value| value as f64),
            timeout_ms,
        )?,
        AutonomousBrowserAction::PressKey {
            selector,
            key,
            timeout_ms,
        } => {
            browser_actions::press_key(app, &tabs, &waiters, selector.as_deref(), &key, timeout_ms)?
        }
        AutonomousBrowserAction::ReadText {
            selector,
            timeout_ms,
        } => browser_actions::read_text(app, &tabs, &waiters, selector.as_deref(), timeout_ms)?,
        AutonomousBrowserAction::Query {
            selector,
            limit,
            timeout_ms,
        } => browser_actions::query(app, &tabs, &waiters, &selector, limit, timeout_ms)?,
        AutonomousBrowserAction::WaitForSelector {
            selector,
            timeout_ms,
            visible,
        } => browser_actions::wait_for_selector(
            app,
            &tabs,
            &waiters,
            &selector,
            timeout_ms,
            visible.unwrap_or(true),
        )?,
        AutonomousBrowserAction::WaitForLoad { timeout_ms } => {
            browser_actions::wait_for_load(app, &tabs, &waiters, timeout_ms)?
        }
        AutonomousBrowserAction::CurrentUrl => match tabs.optional_active_webview(app) {
            Some(webview) => {
                let url = webview.url().map_err(|error| {
                    CommandError::system_fault(
                        "browser_url_failed",
                        format!("Cadence could not read the browser URL: {error}"),
                    )
                })?;
                JsonValue::String(url.to_string())
            }
            None => JsonValue::Null,
        },
        AutonomousBrowserAction::HistoryState => {
            browser_actions::history_state(app, &tabs, &waiters)?
        }
        AutonomousBrowserAction::Screenshot => {
            let webview = tabs.active_webview(app)?;
            let base64 = crate::commands::browser::screenshot_webview(&webview)?;
            JsonValue::String(base64)
        }
        AutonomousBrowserAction::CookiesGet => browser_actions::cookies_get(app, &tabs, &waiters)?,
        AutonomousBrowserAction::CookiesSet { cookie } => {
            browser_actions::cookies_set(app, &tabs, &waiters, &cookie)?
        }
        AutonomousBrowserAction::StorageRead { area, key } => browser_actions::storage_read(
            app,
            &tabs,
            &waiters,
            map_storage_area(area),
            key.as_deref(),
        )?,
        AutonomousBrowserAction::StorageWrite { area, key, value } => {
            browser_actions::storage_write(
                app,
                &tabs,
                &waiters,
                map_storage_area(area),
                &key,
                value.as_deref(),
            )?
        }
        AutonomousBrowserAction::StorageClear { area } => {
            browser_actions::storage_clear(app, &tabs, &waiters, map_storage_area(area))?
        }
        AutonomousBrowserAction::TabList => JsonValue::Array(
            tabs.list()?
                .into_iter()
                .map(tab_to_json)
                .collect::<Vec<_>>(),
        ),
        AutonomousBrowserAction::TabClose { tab_id } => {
            let removed_label = tabs.remove(&tab_id)?;
            if let Some(label) = removed_label {
                if let Some(webview) = app.get_webview(&label) {
                    let _ = webview.close();
                }
            }
            JsonValue::Array(
                tabs.list()?
                    .into_iter()
                    .map(tab_to_json)
                    .collect::<Vec<_>>(),
            )
        }
        AutonomousBrowserAction::TabFocus { tab_id } => {
            tabs.set_active(&tab_id)?;
            JsonValue::String(tab_id)
        }
    };

    let current_url = tabs
        .optional_active_webview(app)
        .and_then(|webview| webview.url().ok().map(|u| u.to_string()));

    let value_json = serde_json::to_string(&output_value).unwrap_or_else(|_| "null".to_string());
    Ok(AutonomousBrowserOutput {
        action: action_name,
        url: current_url,
        value_json,
    })
}

fn action_tool_name(action: &AutonomousBrowserAction) -> String {
    match action {
        AutonomousBrowserAction::Open { .. } => "open",
        AutonomousBrowserAction::TabOpen { .. } => "tab_open",
        AutonomousBrowserAction::Navigate { .. } => "navigate",
        AutonomousBrowserAction::Back => "back",
        AutonomousBrowserAction::Forward => "forward",
        AutonomousBrowserAction::Reload => "reload",
        AutonomousBrowserAction::Stop => "stop",
        AutonomousBrowserAction::Click { .. } => "click",
        AutonomousBrowserAction::Type { .. } => "type",
        AutonomousBrowserAction::Scroll { .. } => "scroll",
        AutonomousBrowserAction::PressKey { .. } => "press_key",
        AutonomousBrowserAction::ReadText { .. } => "read_text",
        AutonomousBrowserAction::Query { .. } => "query",
        AutonomousBrowserAction::WaitForSelector { .. } => "wait_for_selector",
        AutonomousBrowserAction::WaitForLoad { .. } => "wait_for_load",
        AutonomousBrowserAction::CurrentUrl => "current_url",
        AutonomousBrowserAction::HistoryState => "history_state",
        AutonomousBrowserAction::Screenshot => "screenshot",
        AutonomousBrowserAction::CookiesGet => "cookies_get",
        AutonomousBrowserAction::CookiesSet { .. } => "cookies_set",
        AutonomousBrowserAction::StorageRead { .. } => "storage_read",
        AutonomousBrowserAction::StorageWrite { .. } => "storage_write",
        AutonomousBrowserAction::StorageClear { .. } => "storage_clear",
        AutonomousBrowserAction::TabList => "tab_list",
        AutonomousBrowserAction::TabClose { .. } => "tab_close",
        AutonomousBrowserAction::TabFocus { .. } => "tab_focus",
    }
    .to_string()
}

fn require_open_error() -> CommandError {
    CommandError::user_fixable(
        BROWSER_NOT_OPEN_ERROR_CODE,
        "The in-app browser is not currently open.",
    )
}

fn map_storage_area(area: StorageArea) -> crate::commands::browser::StorageArea {
    match area {
        StorageArea::Local => crate::commands::browser::StorageArea::Local,
        StorageArea::Session => crate::commands::browser::StorageArea::Session,
    }
}

fn tab_to_json(tab: crate::commands::browser::BrowserTabMetadata) -> JsonValue {
    serde_json::to_value(tab).unwrap_or(JsonValue::Null)
}

/// A no-op executor used when the browser backend is unreachable (e.g. unit tests
/// without a Tauri runtime). Returns `policy_denied` for every action so the
/// autonomous loop records a useful error rather than panicking.
#[derive(Debug, Default)]
pub struct UnavailableBrowserExecutor;

impl BrowserExecutor for UnavailableBrowserExecutor {
    fn execute(&self, _action: AutonomousBrowserAction) -> CommandResult<AutonomousBrowserOutput> {
        Err(CommandError::policy_denied(
            "Browser actions require the desktop runtime and an open in-app browser.",
        ))
    }
}

/// Produces a browser executor bound to the given Tauri app handle. Safe to clone.
pub fn tauri_browser_executor<R: Runtime>(
    app: AppHandle<R>,
    desktop_state: DesktopState,
) -> Arc<dyn BrowserExecutor> {
    Arc::new(TauriBrowserExecutor { app, desktop_state })
}

struct TauriBrowserExecutor<R: Runtime> {
    app: AppHandle<R>,
    desktop_state: DesktopState,
}

impl<R: Runtime> std::fmt::Debug for TauriBrowserExecutor<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriBrowserExecutor").finish()
    }
}

impl<R: Runtime> BrowserExecutor for TauriBrowserExecutor<R> {
    fn execute(&self, action: AutonomousBrowserAction) -> CommandResult<AutonomousBrowserOutput> {
        execute_action_with_app(&self.app, &self.desktop_state, action)
    }
}
