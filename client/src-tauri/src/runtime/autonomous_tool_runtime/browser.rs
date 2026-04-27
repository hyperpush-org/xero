use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Runtime};

use crate::commands::browser::{provision_browser_tab, BrowserDiagnosticReadOptions, StorageArea};
use crate::commands::{CommandError, CommandResult};
use crate::runtime::redaction::find_prohibited_persistence_content;
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
    ConsoleLogs {
        #[serde(alias = "tabId")]
        tab_id: Option<String>,
        level: Option<String>,
        limit: Option<usize>,
        clear: Option<bool>,
    },
    NetworkSummary {
        #[serde(alias = "tabId")]
        tab_id: Option<String>,
        limit: Option<usize>,
        clear: Option<bool>,
        #[serde(alias = "timeoutMs")]
        timeout_ms: Option<u64>,
    },
    AccessibilityTree {
        selector: Option<String>,
        limit: Option<usize>,
        #[serde(alias = "timeoutMs")]
        timeout_ms: Option<u64>,
    },
    StateSnapshot {
        #[serde(alias = "includeStorage")]
        include_storage: Option<bool>,
        #[serde(alias = "includeCookies")]
        include_cookies: Option<bool>,
        #[serde(alias = "timeoutMs")]
        timeout_ms: Option<u64>,
    },
    StateRestore {
        #[serde(alias = "snapshotJson")]
        snapshot_json: String,
        navigate: Option<bool>,
        #[serde(alias = "timeoutMs")]
        timeout_ms: Option<u64>,
    },
    HarnessExtensionContract,
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
        AutonomousBrowserAction::ConsoleLogs {
            tab_id,
            level,
            limit,
            clear,
        } => {
            let entries = browser_state.diagnostics().console_entries(
                BrowserDiagnosticReadOptions::console(
                    tab_id.as_deref(),
                    level.as_deref(),
                    limit,
                    clear.unwrap_or(false),
                ),
            )?;
            JsonValue::Array(
                entries
                    .into_iter()
                    .map(console_diagnostic_to_json)
                    .collect::<Vec<_>>(),
            )
        }
        AutonomousBrowserAction::NetworkSummary {
            tab_id,
            limit,
            clear,
            timeout_ms,
        } => {
            let entries = browser_state.diagnostics().network_entries(
                BrowserDiagnosticReadOptions::network(
                    tab_id.as_deref(),
                    limit,
                    clear.unwrap_or(false),
                ),
            )?;
            let performance = browser_actions::network_performance_summary(
                app, &tabs, &waiters, limit, timeout_ms,
            )?;
            json!({
                "events": entries.into_iter().map(network_diagnostic_to_json).collect::<Vec<_>>(),
                "performance": performance,
            })
        }
        AutonomousBrowserAction::AccessibilityTree {
            selector,
            limit,
            timeout_ms,
        } => browser_actions::accessibility_tree(
            app,
            &tabs,
            &waiters,
            selector.as_deref(),
            limit,
            timeout_ms,
        )?,
        AutonomousBrowserAction::StateSnapshot {
            include_storage,
            include_cookies,
            timeout_ms,
        } => browser_actions::state_snapshot(
            app,
            &tabs,
            &waiters,
            include_storage.unwrap_or(false),
            include_cookies.unwrap_or(false),
            timeout_ms,
        )?,
        AutonomousBrowserAction::StateRestore {
            snapshot_json,
            navigate,
            timeout_ms,
        } => browser_actions::state_restore(
            app,
            &tabs,
            &waiters,
            &snapshot_json,
            navigate.unwrap_or(false),
            timeout_ms,
        )?,
        AutonomousBrowserAction::HarnessExtensionContract => harness_extension_contract_json(),
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
    let output_value = redact_browser_state_output(&action_name, output_value);

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
        AutonomousBrowserAction::ConsoleLogs { .. } => "console_logs",
        AutonomousBrowserAction::NetworkSummary { .. } => "network_summary",
        AutonomousBrowserAction::AccessibilityTree { .. } => "accessibility_tree",
        AutonomousBrowserAction::StateSnapshot { .. } => "state_snapshot",
        AutonomousBrowserAction::StateRestore { .. } => "state_restore",
        AutonomousBrowserAction::HarnessExtensionContract => "harness_extension_contract",
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

fn console_diagnostic_to_json(
    entry: crate::commands::browser::BrowserConsoleDiagnosticEntry,
) -> JsonValue {
    json!({
        "sequence": entry.sequence,
        "tabId": entry.tab_id,
        "level": entry.level,
        "message": redact_browser_diagnostic_text(&entry.message),
        "capturedAt": entry.captured_at,
    })
}

fn network_diagnostic_to_json(
    entry: crate::commands::browser::BrowserNetworkDiagnosticEntry,
) -> JsonValue {
    json!({
        "sequence": entry.sequence,
        "tabId": entry.tab_id,
        "url": redact_browser_diagnostic_text(&entry.url),
        "method": entry.method,
        "status": entry.status,
        "ok": entry.ok,
        "resourceType": entry.resource_type,
        "durationMs": entry.duration_ms,
        "transferSize": entry.transfer_size,
        "error": entry.error.map(|error| redact_browser_diagnostic_text(&error)),
        "capturedAt": entry.captured_at,
    })
}

fn redact_browser_diagnostic_text(value: &str) -> String {
    if find_prohibited_persistence_content(value).is_some() {
        "[redacted browser diagnostic]".into()
    } else {
        value.to_owned()
    }
}

fn redact_browser_state_output(action_name: &str, value: JsonValue) -> JsonValue {
    if !matches!(action_name, "state_snapshot" | "state_restore") {
        return value;
    }

    redact_browser_state_json(value)
}

fn redact_browser_state_json(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::String(text) if find_prohibited_persistence_content(&text).is_some() => {
            JsonValue::String("[redacted browser state]".into())
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(redact_browser_state_json)
                .collect::<Vec<_>>(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| (key, redact_browser_state_json(value)))
                .collect(),
        ),
        other => other,
    }
}

fn harness_extension_contract_json() -> JsonValue {
    json!({
        "phase": "phase_8_browser_diagnostics_and_optional_harness_extensions",
        "schemaVersion": 1,
        "status": "contract_only",
        "toolRegistration": {
            "requiredFields": [
                "extensionId",
                "toolId",
                "description",
                "inputSchema",
                "riskLevel",
                "approvalPolicy",
                "redactionPolicy",
                "statePolicy"
            ],
            "descriptorPrefix": "extension:<extensionId>:<toolId>",
            "descriptorRequirement": "Every extension-provided tool descriptor must include source extension id, contribution id, risk level, approval requirement, and state persistence policy."
        },
        "riskLevels": [
            "observe",
            "project_read",
            "project_write",
            "run_owned",
            "network",
            "system_read",
            "os_automation",
            "signal_external"
        ],
        "approvalPolicies": [
            "never_for_observe_only",
            "required",
            "per_invocation",
            "blocked"
        ],
        "policyBoundary": {
            "filesystem": "Extension tools must call Cadence repo/system file APIs; direct path access is not part of the privileged harness contract.",
            "process": "Extension tools must call the process_manager or command policy layer for process work.",
            "network": "Network-capable extension tools must declare network risk and use approved transports.",
            "redaction": "Extension output marked durable must be redacted before persistence."
        },
        "lifecycleHooks": [
            "onRegister",
            "beforeInvoke",
            "afterInvoke",
            "onSessionResume",
            "onCompaction",
            "onShutdown"
        ],
        "statePolicy": {
            "ephemeral": "Dropped when the runtime exits.",
            "project": "Persisted in approved project-local stores.",
            "plugin": "Persisted under the extension's approved state namespace.",
            "external": "Requires explicit approval and audit metadata."
        },
        "currentImplementation": {
            "browserDiagnostics": [
                "console_logs",
                "network_summary",
                "accessibility_tree",
                "state_snapshot",
                "state_restore"
            ],
            "dynamicPrivilegedExtensionExecution": false
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_8_browser_actions_deserialize_camel_case_fields() {
        let request = serde_json::from_value::<AutonomousBrowserRequest>(json!({
            "action": "state_restore",
            "snapshotJson": "{\"url\":\"https://example.com\"}",
            "navigate": true,
            "timeoutMs": 5000
        }))
        .expect("state restore request");

        match request.action {
            AutonomousBrowserAction::StateRestore {
                snapshot_json,
                navigate,
                timeout_ms,
            } => {
                assert!(snapshot_json.contains("example.com"));
                assert_eq!(navigate, Some(true));
                assert_eq!(timeout_ms, Some(5_000));
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn harness_extension_contract_declares_policy_boundary() {
        let contract = harness_extension_contract_json();
        assert_eq!(contract["schemaVersion"], 1);
        assert_eq!(
            contract["dynamicPrivilegedExtensionExecution"],
            JsonValue::Null
        );
        assert_eq!(
            contract["currentImplementation"]["dynamicPrivilegedExtensionExecution"],
            false
        );
        assert!(contract["toolRegistration"]["requiredFields"]
            .as_array()
            .expect("required fields")
            .iter()
            .any(|value| value == "riskLevel"));
    }

    #[test]
    fn browser_state_output_redacts_prohibited_values() {
        let redacted = redact_browser_state_output(
            "state_snapshot",
            json!({
                "cookies": [{ "name": "session", "value": "sk-proj-secret" }],
                "localStorage": { "safe": "visible", "token": "Bearer sk-proj-secret" }
            }),
        );

        assert_eq!(redacted["cookies"][0]["name"], "session");
        assert_eq!(redacted["cookies"][0]["value"], "[redacted browser state]");
        assert_eq!(redacted["localStorage"]["safe"], "visible");
        assert_eq!(
            redacted["localStorage"]["token"],
            "[redacted browser state]"
        );
    }
}
