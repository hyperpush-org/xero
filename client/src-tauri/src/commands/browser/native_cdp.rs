use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::IpAddr,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use base64::Engine;
use image::{ImageBuffer, Rgba};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tungstenite::Message;
use url::Url;

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_app_data_dir_for_repo,
    runtime::redaction::redact_json_for_persistence,
};

const DEFAULT_SESSION_ID: &str = "default";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const CDP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_DIAGNOSTIC_EVENTS: usize = 600;
const MAX_DOWNLOAD_EVENTS: usize = 200;
const DEFAULT_VISUAL_DIFF_THRESHOLD_PERCENT: f64 = 0.1;

#[derive(Debug, Default)]
pub struct NativeCdpBrowserService {
    sessions: Mutex<BTreeMap<String, NativeCdpSession>>,
    next_session: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBinaryCandidate {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCdpSessionMetadata {
    pub schema: String,
    pub session_id: String,
    pub label: String,
    pub endpoint: String,
    pub endpoint_kind: String,
    pub browser_path: Option<PathBuf>,
    pub debugging_port: Option<u16>,
    pub profile_dir: PathBuf,
    pub artifact_root: PathBuf,
    pub active_page: Option<NativeCdpPage>,
    pub active_frame: Option<NativeFrameSelection>,
    pub emulation_state: JsonValue,
    pub viewer_state: NativeViewerState,
    pub launched_by_xero: bool,
    pub sensitive_mode: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCdpPage {
    pub target_id: String,
    pub title: String,
    pub url: String,
    pub websocket_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCdpActionResult {
    pub status: String,
    pub summary: String,
    pub data: JsonValue,
    pub evidence_refs: Vec<String>,
    pub current_url: Option<String>,
}

impl NativeCdpActionResult {
    pub fn success(
        summary: impl Into<String>,
        data: JsonValue,
        current_url: Option<String>,
    ) -> Self {
        Self {
            status: "success".into(),
            summary: summary.into(),
            data,
            evidence_refs: Vec::new(),
            current_url,
        }
    }

    pub fn with_evidence(mut self, evidence_refs: Vec<String>) -> Self {
        self.evidence_refs = evidence_refs;
        self
    }
}

pub fn native_cdp_capability_families() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        (
            "lifecycle",
            vec![
                "health",
                "capabilities",
                "launch",
                "attach",
                "close",
                "page_list",
            ],
        ),
        (
            "navigation",
            vec![
                "open",
                "navigate",
                "back",
                "forward",
                "reload",
                "stop",
                "wait_for_load",
            ],
        ),
        (
            "observation",
            vec![
                "current_url",
                "read_text",
                "source",
                "query",
                "accessibility_tree",
                "screenshot",
                "console_logs",
                "network_summary",
                "state_snapshot",
                "timeline",
            ],
        ),
        (
            "refs",
            vec![
                "snapshot",
                "get_ref",
                "click_ref",
                "fill_ref",
                "hover_ref",
                "stale_ref_detection",
            ],
        ),
        (
            "selectors",
            vec![
                "click",
                "type",
                "hover",
                "scroll",
                "press_key",
                "select_option",
                "set_checked",
                "drag",
                "upload_file",
                "focus",
                "paste",
            ],
        ),
        (
            "semantic",
            vec![
                "find_best",
                "act",
                "analyze_form",
                "fill_form",
                "extract",
                "action_cache",
            ],
        ),
        ("waitsAssertionsBatch", vec!["wait_for", "assert", "batch"]),
        (
            "dialogsDownloads",
            vec![
                "dialog_list",
                "dialog_accept",
                "dialog_dismiss",
                "dialog_respond",
                "download_list",
                "download_save",
                "download_clear",
            ],
        ),
        (
            "pagesFrames",
            vec![
                "page_list",
                "switch_page",
                "close_page",
                "frame_list",
                "select_frame",
                "frame_state",
            ],
        ),
        (
            "state",
            vec![
                "cookies_get",
                "storage_read",
                "state_snapshot",
                "state_restore",
                "auth_profile_save",
                "auth_profile_restore",
                "auth_profile_list",
                "auth_profile_delete",
                "vault_save",
                "vault_list",
                "vault_delete",
            ],
        ),
        (
            "networkDiagnostics",
            vec![
                "request_response_tracking",
                "failed_request_tracking",
                "network_idle",
                "har_export",
                "request_block",
                "request_mock",
            ],
        ),
        (
            "artifactsEvidence",
            vec![
                "viewport_screenshot",
                "zoom_region",
                "pdf_export",
                "har_export",
                "trace_start",
                "trace_stop",
                "trace_export",
                "trace_status",
                "visual_baseline_save",
                "visual_baseline_list",
                "visual_baseline_delete",
                "visual_diff",
                "debug_bundle",
                "export_bundle",
                "validate_bundle",
                "recording",
                "generate_test",
            ],
        ),
        (
            "emulation",
            vec![
                "set_viewport",
                "emulate_device",
                "clear_emulation",
                "emulation_state",
            ],
        ),
        (
            "collaboration",
            vec![
                "viewer_state",
                "viewer_goal",
                "takeover",
                "release_control",
                "pause",
                "resume",
                "step",
                "abort",
                "sensitive_on",
                "sensitive_off",
                "annotation",
                "recording",
            ],
        ),
        (
            "resourcesPrompts",
            vec!["browser_resource", "browser_prompt", "mcp_bridge"],
        ),
        (
            "safety",
            vec!["prompt_injection_scan", "redacted_artifacts", "audit_log"],
        ),
    ])
}

pub fn native_cdp_capability_supports_json() -> JsonValue {
    serde_json::to_value(native_cdp_capability_families()).unwrap_or(JsonValue::Null)
}

pub fn native_cdp_limitations_json() -> JsonValue {
    json!([
        "Attach requires an explicit loopback CDP endpoint by default; remote endpoints are denied unless allowRemoteEndpoint is set after explicit operator approval.",
        "Network mock responses are fulfilled while Xero is actively waiting on CDP events; long idle background mocking is not a daemon.",
        "The legacy cookie-string and direct storage mutation actions are intentionally not exposed on native CDP; use structured state_restore snapshots instead.",
        "Credential vault actions are metadata-only until encrypted credential replay is wired; vault_login returns a structured unavailable response.",
        "Cross-origin frame selection is reported with an explicit limitation unless Chrome exposes an attachable execution context for that frame.",
        "MCP bridge exposure is disabled by default and reports its disabled contract until explicitly enabled."
    ])
}

#[derive(Debug)]
struct NativeCdpSession {
    session_id: String,
    label: String,
    endpoint: String,
    browser_path: Option<PathBuf>,
    debugging_port: Option<u16>,
    profile_dir: PathBuf,
    artifact_root: PathBuf,
    active_page: Option<NativeCdpPage>,
    launched_by_xero: bool,
    sensitive_mode: bool,
    created_at: String,
    updated_at: String,
    child: Option<Child>,
    console_events: Vec<NativeConsoleEvent>,
    network_events: Vec<NativeNetworkEvent>,
    dialogs: Vec<NativeDialogEvent>,
    downloads: Vec<NativeDownloadEvent>,
    trace: NativeTraceState,
    emulation_state: JsonValue,
    active_frame: Option<NativeFrameSelection>,
    viewer: NativeViewerState,
    blocked_url_patterns: Vec<String>,
    mocks: Vec<NativeNetworkMock>,
    inflight_requests: BTreeSet<String>,
    last_network_event: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeConsoleEvent {
    sequence: u64,
    level: String,
    message: String,
    captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeNetworkEvent {
    sequence: u64,
    request_id: String,
    url: String,
    method: Option<String>,
    status: Option<u16>,
    ok: Option<bool>,
    resource_type: Option<String>,
    error: Option<String>,
    captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDialogEvent {
    sequence: u64,
    kind: String,
    message: String,
    default_prompt: Option<String>,
    captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDownloadEvent {
    pub sequence: u64,
    pub guid: String,
    pub url: String,
    pub suggested_filename: Option<String>,
    pub state: String,
    pub total_bytes: Option<u64>,
    pub received_bytes: Option<u64>,
    pub managed_path: Option<PathBuf>,
    pub saved_path: Option<PathBuf>,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeTraceState {
    status: String,
    categories: Vec<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    stream_handle: Option<String>,
    artifact_path: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
}

impl Default for NativeTraceState {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            categories: Vec::new(),
            started_at: None,
            completed_at: None,
            stream_handle: None,
            artifact_path: None,
            manifest_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeFrameSelection {
    pub frame_id: String,
    pub parent_frame_id: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub selected_at: String,
    pub limitation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeViewerState {
    pub goal: Option<String>,
    pub control_owner: Option<String>,
    pub paused: bool,
    pub aborted: bool,
    pub sensitive_mode: bool,
    pub step_budget: u32,
    pub updated_at: String,
    pub last_policy_event: Option<String>,
}

impl NativeViewerState {
    fn new(sensitive_mode: bool) -> Self {
        Self {
            goal: None,
            control_owner: None,
            paused: false,
            aborted: false,
            sensitive_mode,
            step_budget: 0,
            updated_at: now_timestamp(),
            last_policy_event: None,
        }
    }

    fn touch(&mut self) {
        self.updated_at = now_timestamp();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeNetworkMock {
    url_contains: String,
    status: u16,
    body: String,
    content_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct CdpVersion {
    #[serde(default)]
    browser: String,
    #[serde(default)]
    protocol_version: String,
    #[serde(default)]
    web_socket_debugger_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpTarget {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "type", default)]
    target_type: String,
    #[serde(default)]
    web_socket_debugger_url: Option<String>,
}

impl NativeCdpBrowserService {
    pub fn health(&self, repo_root: &Path) -> JsonValue {
        let binaries = discover_chromium_browsers();
        let sessions = self
            .sessions
            .lock()
            .ok()
            .map(|sessions| {
                sessions
                    .values()
                    .map(NativeCdpSession::metadata)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        json!({
            "schema": "xero.browser_native_cdp_health.v1",
            "healthy": true,
            "engine": "native_cdp",
            "backend": "xero_internal_cdp",
            "browserFound": !binaries.is_empty(),
            "browserCandidates": binaries,
            "sessionCount": sessions.len(),
            "sessions": sessions,
            "storageRoot": native_root(repo_root),
            "checkedAt": now_timestamp(),
        })
    }

    pub fn capability_manifest(&self, repo_root: &Path) -> JsonValue {
        let binaries = discover_chromium_browsers();
        let session_count = self
            .sessions
            .lock()
            .ok()
            .map(|sessions| sessions.len())
            .unwrap_or(0);
        json!({
            "engine": "native_cdp",
            "available": true,
            "nativeEngineCompiled": true,
            "health": if binaries.is_empty() { "ready_attach_only" } else { "ready" },
            "backend": "xero_internal_cdp",
            "browserFound": !binaries.is_empty(),
            "browserCandidates": binaries,
            "launchAvailable": !binaries.is_empty(),
            "attachAvailable": true,
            "activeSessionAvailable": session_count > 0,
            "sessionCount": session_count,
            "remoteAttachDisabledByPolicy": true,
            "supports": native_cdp_capability_supports_json(),
            "limitations": native_cdp_limitations_json(),
            "storageRoot": native_root(repo_root),
        })
    }

    pub fn launch(
        &self,
        repo_root: &Path,
        session_id: Option<String>,
        label: Option<String>,
        url: Option<String>,
        browser_path: Option<PathBuf>,
        headless: bool,
        sensitive_mode: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let session_id = normalize_session_id(session_id, || self.allocate_session_id());
        let label = label.unwrap_or_else(|| session_id.clone());
        let browser_path = match browser_path {
            Some(path) => path,
            None => discover_chromium_browsers()
                .into_iter()
                .next()
                .map(|candidate| candidate.path)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "browser_native_binary_missing",
                        "Xero native CDP is initialized, but no Chrome/Chromium-family browser binary was found. Install Chrome/Chromium or attach to an explicit CDP endpoint.",
                    )
                })?,
        };
        if !browser_path.is_file() {
            return Err(CommandError::user_fixable(
                "browser_native_binary_invalid",
                format!(
                    "Browser binary `{}` does not exist.",
                    browser_path.display()
                ),
            ));
        }

        let port = choose_free_port()?;
        let root = native_root(repo_root);
        let profile_dir =
            root.join("profiles")
                .join(format!("{}-{}", session_id, random_token(16)));
        let artifact_root = root.join("artifacts").join(&session_id);
        fs::create_dir_all(&profile_dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_profile_dir_failed",
                format!(
                    "Xero could not prepare native browser profile at {}: {error}",
                    profile_dir.display()
                ),
            )
        })?;
        fs::create_dir_all(&artifact_root).map_err(|error| {
            CommandError::retryable(
                "browser_native_artifact_dir_failed",
                format!(
                    "Xero could not prepare native browser artifacts at {}: {error}",
                    artifact_root.display()
                ),
            )
        })?;

        let mut command = Command::new(&browser_path);
        command
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-features=Translate,OptimizationHints")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if headless {
            command.arg("--headless=new");
        }
        command.arg(url.clone().unwrap_or_else(|| "about:blank".into()));

        let child = command.spawn().map_err(|error| {
            CommandError::system_fault(
                "browser_native_launch_failed",
                format!(
                    "Xero could not launch native browser `{}`: {error}",
                    browser_path.display()
                ),
            )
        })?;

        let endpoint = format!("http://127.0.0.1:{port}");
        wait_for_cdp_endpoint(&endpoint, CDP_LAUNCH_TIMEOUT)?;
        let mut session = NativeCdpSession::new(
            session_id.clone(),
            label,
            endpoint,
            Some(browser_path),
            Some(port),
            profile_dir,
            artifact_root,
            true,
            sensitive_mode,
            Some(child),
        );
        session.active_page = fetch_or_create_page(&session.endpoint, url.as_deref())?;
        persist_session_metadata(&session)?;
        let metadata = session.metadata();
        let current_url = session.active_page.as_ref().map(|page| page.url.clone());

        self.sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?
            .insert(session_id.clone(), session);

        Ok(NativeCdpActionResult::success(
            format!("Launched native CDP browser session `{session_id}`."),
            json!({ "session": metadata }),
            current_url,
        ))
    }

    pub fn attach(
        &self,
        repo_root: &Path,
        endpoint: String,
        session_id: Option<String>,
        label: Option<String>,
        sensitive_mode: bool,
        allow_remote_endpoint: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let session_id = normalize_session_id(session_id, || self.allocate_session_id());
        let label = label.unwrap_or_else(|| session_id.clone());
        let root = native_root(repo_root);
        let profile_dir = root.join("attached-profiles").join(&session_id);
        let artifact_root = root.join("artifacts").join(&session_id);
        fs::create_dir_all(&profile_dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_profile_dir_failed",
                format!(
                    "Xero could not prepare native browser attach metadata at {}: {error}",
                    profile_dir.display()
                ),
            )
        })?;
        fs::create_dir_all(&artifact_root).map_err(|error| {
            CommandError::retryable(
                "browser_native_artifact_dir_failed",
                format!(
                    "Xero could not prepare native browser artifacts at {}: {error}",
                    artifact_root.display()
                ),
            )
        })?;

        let endpoint = normalize_endpoint(&endpoint, allow_remote_endpoint)?;
        let (debugging_port, active_page) = if endpoint.starts_with("ws://") {
            (parse_ws_port(&endpoint), Some(page_from_ws_url(&endpoint)))
        } else {
            wait_for_cdp_endpoint(&endpoint, HTTP_TIMEOUT)?;
            (
                parse_http_port(&endpoint),
                fetch_or_create_page(&endpoint, None)?,
            )
        };

        let mut session = NativeCdpSession::new(
            session_id.clone(),
            label,
            endpoint,
            None,
            debugging_port,
            profile_dir,
            artifact_root,
            false,
            sensitive_mode,
            None,
        );
        session.active_page = active_page;
        persist_session_metadata(&session)?;
        let metadata = session.metadata();
        let current_url = session.active_page.as_ref().map(|page| page.url.clone());

        self.sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?
            .insert(session_id.clone(), session);

        Ok(NativeCdpActionResult::success(
            format!("Attached native CDP session `{session_id}`."),
            json!({ "session": metadata }),
            current_url,
        ))
    }

    pub fn close(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session_id = match session_id {
            Some(session_id) => session_id,
            None if sessions.len() == 1 => sessions
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| DEFAULT_SESSION_ID.into()),
            None => DEFAULT_SESSION_ID.into(),
        };
        let mut session = sessions
            .remove(&session_id)
            .ok_or_else(|| missing_session_error(&session_id))?;

        if let Some(mut child) = session.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        Ok(NativeCdpActionResult::success(
            format!("Closed native CDP browser session `{session_id}`."),
            json!({ "sessionId": session_id }),
            None,
        ))
    }

    pub fn page_list(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let pages = if session.endpoint.starts_with("http://") {
            fetch_pages(&session.endpoint)?
        } else {
            session
                .active_page
                .clone()
                .map(|page| vec![page])
                .unwrap_or_default()
        };
        if session.active_page.is_none() {
            session.active_page = pages.first().cloned();
        }
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Listed native CDP pages.",
            json!({ "sessionId": session.session_id, "pages": pages }),
            session.active_page.as_ref().map(|page| page.url.clone()),
        ))
    }

    pub fn open_or_navigate(
        &self,
        repo_root: &Path,
        url: String,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        if !self.has_session(session_id.as_deref()) {
            return self.launch(repo_root, session_id, None, Some(url), None, false, false);
        }
        self.navigate(session_id, url)
    }

    pub fn navigate(
        &self,
        session_id: Option<String>,
        url: String,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        let result = client.command(
            session,
            "Page.navigate",
            json!({ "url": url }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        wait_for_load_with_client(&mut client, session, Duration::from_secs(15))?;
        session.refresh_active_page_from_runtime(&mut client)?;
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            format!(
                "Navigated native CDP browser to `{}`.",
                current_url_from_session(session).unwrap_or(url)
            ),
            json!({ "result": result, "session": session.metadata() }),
            current_url_from_session(session),
        ))
    }

    pub fn history(
        &self,
        session_id: Option<String>,
        delta: i64,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        let expression = if delta < 0 {
            "history.back(); ({ url: location.href, title: document.title })"
        } else {
            "history.forward(); ({ url: location.href, title: document.title })"
        };
        let result = runtime_evaluate(&mut client, session, expression, CDP_RESPONSE_TIMEOUT)?;
        wait_for_load_with_client(&mut client, session, Duration::from_secs(10))?;
        session.refresh_active_page_from_runtime(&mut client)?;
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            if delta < 0 {
                "Moved native CDP browser back."
            } else {
                "Moved native CDP browser forward."
            },
            result,
            current_url_from_session(session),
        ))
    }

    pub fn reload(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        client.command(session, "Page.reload", json!({}), CDP_RESPONSE_TIMEOUT)?;
        wait_for_load_with_client(&mut client, session, Duration::from_secs(15))?;
        session.refresh_active_page_from_runtime(&mut client)?;
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Reloaded native CDP browser.",
            json!({ "session": session.metadata() }),
            current_url_from_session(session),
        ))
    }

    pub fn stop(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let result =
            client.command(session, "Page.stopLoading", json!({}), CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            "Stopped native CDP browser load.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn current_state(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let result = runtime_evaluate(
            &mut client,
            session,
            "({ url: location.href, title: document.title, readyState: document.readyState })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.update_active_page_from_state(&result);
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Read native CDP browser state.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn read_text(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let selector_json = optional_js_string(selector)?;
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const root = selector ? document.querySelector(selector) : document.body;
                if (!root) throw new Error('element not found: ' + selector);
                return {{
                    url: location.href,
                    title: document.title,
                    text: ((root.innerText || root.textContent || '').trim()).replace(/\s+/g, ' ')
                }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        session.update_active_page_from_state(&result);
        Ok(NativeCdpActionResult::success(
            "Read native CDP browser text.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn source(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let result = runtime_evaluate(
            &mut client,
            session,
            "({ url: location.href, title: document.title, html: document.documentElement ? document.documentElement.outerHTML : '' })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        Ok(NativeCdpActionResult::success(
            "Read native CDP page source.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn query(
        &self,
        session_id: Option<String>,
        selector: &str,
        limit: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let selector_json = js_string(selector)?;
        let limit = limit.unwrap_or(50).clamp(1, 200);
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 500);
                return {{
                    selector,
                    count: document.querySelectorAll(selector).length,
                    nodes: Array.from(document.querySelectorAll(selector)).slice(0, {limit}).map((el, index) => {{
                        const rect = el.getBoundingClientRect();
                        return {{
                            index,
                            tag: (el.tagName || '').toLowerCase(),
                            id: el.id || null,
                            role: el.getAttribute('role') || null,
                            name: el.getAttribute('aria-label') || el.getAttribute('name') || el.getAttribute('title') || null,
                            text: textOf(el),
                            visible: !!(el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)),
                            bounds: {{ x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) }}
                        }};
                    }})
                }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            format!("Queried native CDP selector `{selector}`."),
            result,
            current_url_from_session(session),
        ))
    }

    pub fn snapshot(
        &self,
        session_id: Option<String>,
        mode: &str,
        visible_only: bool,
        limit: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression = native_snapshot_expression(mode, visible_only, limit.unwrap_or(180));
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        session.update_active_page_from_state(&result);
        Ok(NativeCdpActionResult::success(
            "Captured native CDP browser snapshot.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn resolve_ref_selector(
        &self,
        session_id: Option<String>,
        node: &JsonValue,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression = native_ref_resolution_expression(node)?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        if result.get("ok").and_then(JsonValue::as_bool) == Some(true) {
            return Ok(NativeCdpActionResult::success(
                "Resolved native CDP browser ref.",
                result,
                current_url_from_session(session),
            ));
        }
        Err(CommandError::user_fixable(
            "browser_ref_stale",
            result
                .get("message")
                .and_then(JsonValue::as_str)
                .unwrap_or("Browser ref no longer resolves to the snapshotted element. Run snapshot again and use a fresh ref.")
                .to_owned(),
        ))
    }

    pub fn click(
        &self,
        session_id: Option<String>,
        selector: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        let bounds = selector_point(&mut client, session, selector)?;
        dispatch_mouse_click(&mut client, session, bounds.x, bounds.y)?;
        client.drain_events(session, Duration::from_millis(200));
        Ok(NativeCdpActionResult::success(
            format!("Clicked native CDP selector `{selector}`."),
            json!({ "selector": selector, "point": { "x": bounds.x, "y": bounds.y }, "bounds": bounds.raw }),
            current_url_from_session(session),
        ))
    }

    pub fn hover(
        &self,
        session_id: Option<String>,
        selector: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let bounds = selector_point(&mut client, session, selector)?;
        client.command(
            session,
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseMoved", "x": bounds.x, "y": bounds.y, "button": "none" }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        Ok(NativeCdpActionResult::success(
            format!("Hovered native CDP selector `{selector}`."),
            json!({ "selector": selector, "point": { "x": bounds.x, "y": bounds.y } }),
            current_url_from_session(session),
        ))
    }

    pub fn type_text(
        &self,
        session_id: Option<String>,
        selector: &str,
        text: &str,
        append: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let selector_json = js_string(selector)?;
        let append_literal = if append { "true" } else { "false" };
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const append = {append_literal};
                const el = document.querySelector(selector);
                if (!el) throw new Error('element not found: ' + selector);
                if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
                if (typeof el.focus === 'function') el.focus();
                if (!append) {{
                    if (typeof el.select === 'function') el.select();
                    else if (document.getSelection && el.isContentEditable) {{
                        const range = document.createRange();
                        range.selectNodeContents(el);
                        const selection = document.getSelection();
                        selection.removeAllRanges();
                        selection.addRange(range);
                    }}
                }}
                return {{ selector, tag: (el.tagName || '').toLowerCase(), active: document.activeElement === el }};
            }})()"#
        );
        let focus = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        client.command(
            session,
            "Input.insertText",
            json!({ "text": text }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        client.drain_events(session, Duration::from_millis(200));
        Ok(NativeCdpActionResult::success(
            format!("Typed into native CDP selector `{selector}`."),
            json!({ "selector": selector, "focus": focus }),
            current_url_from_session(session),
        ))
    }

    pub fn press_key(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
        key: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        if let Some(selector) = selector {
            let selector_json = js_string(selector)?;
            let expression = format!(
                r#"(() => {{
                    const el = document.querySelector({selector_json});
                    if (!el) throw new Error('element not found: ' + {selector_json});
                    if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
                    if (typeof el.focus === 'function') el.focus();
                    return true;
                }})()"#
            );
            runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        }
        let (down, up) = key_event_payloads(key);
        client.command(
            session,
            "Input.dispatchKeyEvent",
            down,
            CDP_RESPONSE_TIMEOUT,
        )?;
        client.command(session, "Input.dispatchKeyEvent", up, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            format!("Pressed native CDP key `{key}`."),
            json!({ "key": key }),
            current_url_from_session(session),
        ))
    }

    pub fn scroll(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
        x: Option<i64>,
        y: Option<i64>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let selector_json = optional_js_string(selector)?;
        let x = x.unwrap_or(0);
        let y = y.unwrap_or(0);
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const x = {x};
                const y = {y};
                if (selector) {{
                    const el = document.querySelector(selector);
                    if (!el) throw new Error('element not found: ' + selector);
                    if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
                    if (x || y) el.scrollBy ? el.scrollBy(x, y) : window.scrollBy(x, y);
                }} else {{
                    window.scrollBy(x, y);
                }}
                return {{ selector, x, y, scrollX: window.scrollX, scrollY: window.scrollY }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            "Scrolled native CDP browser.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn select_option(
        &self,
        session_id: Option<String>,
        selector: &str,
        value: Option<&str>,
        label: Option<&str>,
        index: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "select_option")?;
        let mut client = session.connect_page()?;
        let selector_json = js_string(selector)?;
        let value_json = optional_js_string(value)?;
        let label_json = optional_js_string(label)?;
        let index_json = index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".into());
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const requestedValue = {value_json};
                const requestedLabel = {label_json};
                const requestedIndex = {index_json};
                const el = document.querySelector(selector);
                if (!el) throw new Error('element not found: ' + selector);
                if ((el.tagName || '').toLowerCase() !== 'select') throw new Error('element is not a select: ' + selector);
                const options = Array.from(el.options || []);
                const option = options.find((item, idx) =>
                    (requestedValue != null && item.value === requestedValue) ||
                    (requestedLabel != null && (item.label || item.textContent || '').trim() === requestedLabel) ||
                    (requestedIndex != null && idx === requestedIndex)
                );
                if (!option) throw new Error('select option not found for selector: ' + selector);
                option.selected = true;
                el.value = option.value;
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{
                    selector,
                    value: el.value,
                    selectedIndex: el.selectedIndex,
                    selectedText: option.textContent || option.label || null
                }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            format!("Selected native CDP option for `{selector}`."),
            result,
            current_url_from_session(session),
        ))
    }

    pub fn set_checked(
        &self,
        session_id: Option<String>,
        selector: &str,
        checked: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "set_checked")?;
        let mut client = session.connect_page()?;
        let selector_json = js_string(selector)?;
        let checked_literal = if checked { "true" } else { "false" };
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const checked = {checked_literal};
                const el = document.querySelector(selector);
                if (!el) throw new Error('element not found: ' + selector);
                const role = el.getAttribute('role');
                const tag = (el.tagName || '').toLowerCase();
                const type = String(el.type || '').toLowerCase();
                if (tag === 'input' && (type === 'checkbox' || type === 'radio')) {{
                    if (type === 'radio' && checked === false) throw new Error('radio controls cannot be unchecked directly');
                    el.checked = checked;
                }} else if (role === 'checkbox' || role === 'switch' || role === 'radio') {{
                    el.setAttribute('aria-checked', checked ? 'true' : 'false');
                }} else {{
                    throw new Error('element is not a checkbox, radio, switch, or ARIA checked control: ' + selector);
                }}
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ selector, checked, ariaChecked: el.getAttribute('aria-checked'), domChecked: typeof el.checked === 'boolean' ? el.checked : null }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            format!("Set native CDP checked state for `{selector}`."),
            result,
            current_url_from_session(session),
        ))
    }

    pub fn drag(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
        target_selector: Option<&str>,
        from_x: Option<i64>,
        from_y: Option<i64>,
        to_x: Option<i64>,
        to_y: Option<i64>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "drag")?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        let start = match selector {
            Some(selector) => selector_point(&mut client, session, selector)?,
            None => SelectorPoint {
                x: from_x.unwrap_or(0) as f64,
                y: from_y.unwrap_or(0) as f64,
                raw: json!({ "source": "coordinates" }),
            },
        };
        let end = match target_selector {
            Some(selector) => selector_point(&mut client, session, selector)?,
            None => SelectorPoint {
                x: to_x.ok_or_else(|| CommandError::invalid_request("toX"))? as f64,
                y: to_y.ok_or_else(|| CommandError::invalid_request("toY"))? as f64,
                raw: json!({ "source": "coordinates" }),
            },
        };
        client.command(
            session,
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseMoved", "x": start.x, "y": start.y, "button": "none" }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        client.command(
            session,
            "Input.dispatchMouseEvent",
            json!({ "type": "mousePressed", "x": start.x, "y": start.y, "button": "left", "clickCount": 1 }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        for step in 1..=8 {
            let t = step as f64 / 8.0;
            client.command(
                session,
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseMoved",
                    "x": start.x + (end.x - start.x) * t,
                    "y": start.y + (end.y - start.y) * t,
                    "button": "left",
                }),
                CDP_RESPONSE_TIMEOUT,
            )?;
        }
        client.command(
            session,
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseReleased", "x": end.x, "y": end.y, "button": "left", "clickCount": 1 }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        client.drain_events(session, Duration::from_millis(300));
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Dragged native CDP pointer between browser targets.",
            json!({
                "from": { "x": start.x, "y": start.y, "bounds": start.raw },
                "to": { "x": end.x, "y": end.y, "bounds": end.raw },
            }),
            current_url_from_session(session),
        ))
    }

    pub fn upload_file(
        &self,
        session_id: Option<String>,
        selector: &str,
        paths: &[PathBuf],
    ) -> CommandResult<NativeCdpActionResult> {
        if paths.is_empty() {
            return Err(CommandError::invalid_request("paths"));
        }
        let files = paths
            .iter()
            .map(|path| {
                if !path.is_file() {
                    return Err(CommandError::user_fixable(
                        "browser_native_upload_file_missing",
                        format!("Upload file `{}` does not exist.", path.display()),
                    ));
                }
                Ok(path.to_string_lossy().into_owned())
            })
            .collect::<CommandResult<Vec<_>>>()?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "upload_file")?;
        let mut client = session.connect_page()?;
        let document = client.command(
            session,
            "DOM.getDocument",
            json!({ "depth": 1, "pierce": true }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let root_id = document
            .get("root")
            .and_then(|root| root.get("nodeId"))
            .and_then(JsonValue::as_i64)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_native_dom_root_missing",
                    "Native CDP DOM.getDocument did not return a root node id.",
                )
            })?;
        let query = client.command(
            session,
            "DOM.querySelector",
            json!({ "nodeId": root_id, "selector": selector }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let node_id = query
            .get("nodeId")
            .and_then(JsonValue::as_i64)
            .filter(|node_id| *node_id > 0)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_native_upload_target_missing",
                    format!("Native CDP could not find upload selector `{selector}`."),
                )
            })?;
        client.command(
            session,
            "DOM.setFileInputFiles",
            json!({ "nodeId": node_id, "files": files }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            format!("Set native CDP file input `{selector}`."),
            json!({ "selector": selector, "fileCount": paths.len(), "policy": "file_transfer_approval_required" }),
            current_url_from_session(session),
        ))
    }

    pub fn focus(
        &self,
        session_id: Option<String>,
        selector: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "focus")?;
        let mut client = session.connect_page()?;
        let selector_json = js_string(selector)?;
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const el = document.querySelector(selector);
                if (!el) throw new Error('element not found: ' + selector);
                if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
                if (typeof el.focus !== 'function') throw new Error('element cannot be focused: ' + selector);
                el.focus();
                return {{ selector, active: document.activeElement === el }};
            }})()"#
        );
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            format!("Focused native CDP selector `{selector}`."),
            result,
            current_url_from_session(session),
        ))
    }

    pub fn paste(
        &self,
        session_id: Option<String>,
        selector: &str,
        text: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "paste")?;
        let mut client = session.connect_page()?;
        let focus = self.focus_locked(session, &mut client, selector)?;
        client.command(
            session,
            "Input.insertText",
            json!({ "text": text }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            format!("Pasted text into native CDP selector `{selector}`."),
            json!({ "selector": selector, "focus": focus, "textLength": text.len(), "policy": "paste_approval_required_for_sensitive_text" }),
            current_url_from_session(session),
        ))
    }

    fn focus_locked(
        &self,
        session: &mut NativeCdpSession,
        client: &mut CdpClient,
        selector: &str,
    ) -> CommandResult<JsonValue> {
        let selector_json = js_string(selector)?;
        let expression = format!(
            r#"(() => {{
                const selector = {selector_json};
                const el = document.querySelector(selector);
                if (!el) throw new Error('element not found: ' + selector);
                if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
                if (typeof el.focus === 'function') el.focus();
                return {{ selector, active: document.activeElement === el }};
            }})()"#
        );
        runtime_evaluate(client, session, &expression, CDP_RESPONSE_TIMEOUT)
    }

    pub fn set_viewport(
        &self,
        session_id: Option<String>,
        width: u32,
        height: u32,
        device_scale_factor: Option<f64>,
        mobile: Option<bool>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "set_viewport")?;
        let mut client = session.connect_page()?;
        let state = json!({
            "schema": "xero.browser_native_emulation_state.v1",
            "active": true,
            "viewport": {
                "width": width.max(1),
                "height": height.max(1),
                "deviceScaleFactor": device_scale_factor.unwrap_or(1.0).max(0.1),
                "mobile": mobile.unwrap_or(false),
            },
            "updatedAt": now_timestamp(),
        });
        apply_emulation_state(&mut client, session, &state)?;
        session.emulation_state = state.clone();
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Updated native CDP viewport.",
            state,
            current_url_from_session(session),
        ))
    }

    pub fn zoom_region(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
        x: Option<i64>,
        y: Option<i64>,
        width: Option<u32>,
        height: Option<u32>,
        scale: Option<f64>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let clip = if let Some(selector) = selector {
            let bounds = selector_point(&mut client, session, selector)?;
            let bounds_obj = bounds.raw.get("bounds").cloned().unwrap_or(JsonValue::Null);
            json!({
                "x": bounds_obj.get("x").and_then(JsonValue::as_f64).unwrap_or(bounds.x),
                "y": bounds_obj.get("y").and_then(JsonValue::as_f64).unwrap_or(bounds.y),
                "width": bounds_obj.get("width").and_then(JsonValue::as_f64).unwrap_or(1.0).max(1.0),
                "height": bounds_obj.get("height").and_then(JsonValue::as_f64).unwrap_or(1.0).max(1.0),
                "scale": scale.unwrap_or(1.0).max(0.1),
            })
        } else {
            json!({
                "x": x.unwrap_or(0).max(0) as f64,
                "y": y.unwrap_or(0).max(0) as f64,
                "width": width.unwrap_or(400).max(1) as f64,
                "height": height.unwrap_or(300).max(1) as f64,
                "scale": scale.unwrap_or(1.0).max(0.1),
            })
        };
        let result = client.command(
            session,
            "Page.captureScreenshot",
            json!({ "format": "png", "fromSurface": true, "clip": clip }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let base64 = result
            .get("data")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_native_zoom_screenshot_invalid",
                    "Native CDP zoom-region screenshot response did not include base64 image data.",
                )
            })?;
        let path = if session.sensitive_mode {
            None
        } else {
            Some(write_base64_artifact(
                &session.artifact_root,
                "zoom-regions",
                "browser-zoom-region",
                "png",
                base64,
            )?)
        };
        let evidence = path
            .as_ref()
            .map(|path| vec![path.to_string_lossy().into_owned()])
            .unwrap_or_default();
        Ok(NativeCdpActionResult::success(
            "Captured native CDP zoom-region screenshot.",
            json!({
                "clip": clip,
                "screenshotBase64": if session.sensitive_mode { JsonValue::Null } else { JsonValue::String(base64.to_owned()) },
                "artifactPath": path.map(|path| path.to_string_lossy().into_owned()),
                "sensitiveModeSuppressed": session.sensitive_mode,
            }),
            current_url_from_session(session),
        )
        .with_evidence(evidence))
    }

    pub fn wait_for(
        &self,
        session_id: Option<String>,
        condition: &str,
        selector: Option<&str>,
        text: Option<&str>,
        url_contains: Option<&str>,
        title_contains: Option<&str>,
        count: Option<usize>,
        timeout: Duration,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;
        let started = Instant::now();
        let deadline = started + timeout;
        let mut last = JsonValue::Null;
        while Instant::now() < deadline {
            if condition == "network_idle" {
                client.drain_events(session, Duration::from_millis(120));
                let idle_for = session
                    .last_network_event
                    .map(|instant| instant.elapsed())
                    .unwrap_or_else(|| timeout);
                if session.inflight_requests.is_empty() && idle_for >= Duration::from_millis(500) {
                    let data = json!({
                        "condition": condition,
                        "waitedMs": started.elapsed().as_millis(),
                        "detail": { "inflight": 0, "idleForMs": idle_for.as_millis() }
                    });
                    return Ok(NativeCdpActionResult::success(
                        "Native CDP network idle wait was satisfied.",
                        data,
                        current_url_from_session(session),
                    ));
                }
                last = json!({
                    "inflight": session.inflight_requests.len(),
                    "idleForMs": idle_for.as_millis()
                });
                continue;
            }

            let expression = native_wait_expression(
                condition,
                selector,
                text,
                url_contains,
                title_contains,
                count.unwrap_or(0),
            )?;
            let check = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
            last = check.clone();
            if check.get("ok").and_then(JsonValue::as_bool) == Some(true) {
                let data = json!({
                    "condition": condition,
                    "waitedMs": started.elapsed().as_millis(),
                    "detail": check.get("detail").cloned().unwrap_or(JsonValue::Null)
                });
                return Ok(NativeCdpActionResult::success(
                    format!("Native CDP wait condition `{condition}` was satisfied."),
                    data,
                    current_url_from_session(session),
                ));
            }
            thread::sleep(Duration::from_millis(80));
        }
        Err(CommandError::user_fixable(
            "browser_native_wait_timeout",
            format!(
                "Native CDP wait for `{condition}` timed out after {} ms. Last check: {last}",
                timeout.as_millis()
            ),
        ))
    }

    pub fn assert_condition(
        &self,
        session_id: Option<String>,
        assertion: &str,
        selector: Option<&str>,
        expected: Option<&str>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        match assertion {
            "console_errors" => {
                let errors = session
                    .console_events
                    .iter()
                    .filter(|event| event.level == "error")
                    .count();
                if errors == 0 {
                    return Ok(NativeCdpActionResult::success(
                        "Native CDP console error assertion passed.",
                        json!({ "assertion": assertion, "pass": true, "actual": 0, "expected": 0 }),
                        current_url_from_session(session),
                    ));
                }
                return Err(CommandError::user_fixable(
                    "browser_native_assertion_failed",
                    format!("Expected no native CDP console errors, found {errors}."),
                ));
            }
            "failed_requests" => {
                let failed = session
                    .network_events
                    .iter()
                    .filter(|event| event.ok == Some(false) || event.error.is_some())
                    .count();
                if failed == 0 {
                    return Ok(NativeCdpActionResult::success(
                        "Native CDP failed-request assertion passed.",
                        json!({ "assertion": assertion, "pass": true, "actual": 0, "expected": 0 }),
                        current_url_from_session(session),
                    ));
                }
                return Err(CommandError::user_fixable(
                    "browser_native_assertion_failed",
                    format!("Expected no native CDP failed requests, found {failed}."),
                ));
            }
            "console_count" => {
                let expected = expected
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                let actual = session.console_events.len();
                if actual == expected {
                    return Ok(NativeCdpActionResult::success(
                        "Native CDP console-count assertion passed.",
                        json!({ "assertion": assertion, "pass": true, "actual": actual, "expected": expected }),
                        current_url_from_session(session),
                    ));
                }
                return Err(CommandError::user_fixable(
                    "browser_native_assertion_failed",
                    format!("Expected {expected} native CDP console events, found {actual}."),
                ));
            }
            "network_count" => {
                let expected = expected
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                let actual = session.network_events.len();
                if actual == expected {
                    return Ok(NativeCdpActionResult::success(
                        "Native CDP network-count assertion passed.",
                        json!({ "assertion": assertion, "pass": true, "actual": actual, "expected": expected }),
                        current_url_from_session(session),
                    ));
                }
                return Err(CommandError::user_fixable(
                    "browser_native_assertion_failed",
                    format!("Expected {expected} native CDP network events, found {actual}."),
                ));
            }
            _ => {}
        }

        let mut client = session.connect_page()?;
        let expression = native_assert_expression(assertion, selector, expected)?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        if result.get("pass").and_then(JsonValue::as_bool) == Some(true) {
            Ok(NativeCdpActionResult::success(
                format!("Native CDP assertion `{assertion}` passed."),
                result,
                current_url_from_session(session),
            ))
        } else {
            Err(CommandError::user_fixable(
                "browser_native_assertion_failed",
                format!("Native CDP assertion `{assertion}` failed: {result}"),
            ))
        }
    }

    pub fn screenshot(
        &self,
        session_id: Option<String>,
        full_page: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let params = if full_page {
            json!({ "format": "png", "captureBeyondViewport": true, "fromSurface": true })
        } else {
            json!({ "format": "png", "fromSurface": true })
        };
        let result = client.command(
            session,
            "Page.captureScreenshot",
            params,
            CDP_RESPONSE_TIMEOUT,
        )?;
        let base64 = result
            .get("data")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_native_screenshot_invalid",
                    "Native CDP screenshot response did not include base64 image data.",
                )
            })?;
        let path = if session.sensitive_mode {
            None
        } else {
            Some(write_base64_artifact(
                &session.artifact_root,
                "screenshots",
                "browser-screenshot",
                "png",
                base64,
            )?)
        };
        let evidence = path
            .as_ref()
            .map(|path| vec![path.to_string_lossy().into_owned()])
            .unwrap_or_default();
        Ok(NativeCdpActionResult::success(
            "Captured native CDP browser screenshot.",
            json!({
                "screenshotBase64": if session.sensitive_mode { JsonValue::Null } else { JsonValue::String(base64.to_owned()) },
                "artifactPath": path.map(|path| path.to_string_lossy().into_owned()),
                "sensitiveModeSuppressed": session.sensitive_mode,
            }),
            current_url_from_session(session),
        )
        .with_evidence(evidence))
    }

    pub fn accessibility_tree(
        &self,
        session_id: Option<String>,
        limit: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let result = client.command(
            session,
            "Accessibility.getFullAXTree",
            json!({}),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let limit = limit.unwrap_or(200).clamp(1, 1_000);
        let nodes = result
            .get("nodes")
            .and_then(JsonValue::as_array)
            .map(|nodes| nodes.iter().take(limit).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(NativeCdpActionResult::success(
            "Read native CDP accessibility tree.",
            json!({ "nodes": nodes, "truncated": result.get("nodes").and_then(JsonValue::as_array).map(|nodes| nodes.len() > limit).unwrap_or(false) }),
            current_url_from_session(session),
        ))
    }

    pub fn console_logs(
        &self,
        session_id: Option<String>,
        level: Option<&str>,
        limit: Option<usize>,
        clear: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let limit = limit.unwrap_or(100).min(MAX_DIAGNOSTIC_EVENTS);
        let mut entries = session
            .console_events
            .iter()
            .filter(|event| level.map_or(true, |level| event.level == level))
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        entries.reverse();
        if clear {
            session.console_events.clear();
        }
        Ok(NativeCdpActionResult::success(
            "Read native CDP console diagnostics.",
            json!({ "events": entries }),
            current_url_from_session(session),
        ))
    }

    pub fn network_summary(
        &self,
        session_id: Option<String>,
        limit: Option<usize>,
        clear: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let limit = limit.unwrap_or(100).min(MAX_DIAGNOSTIC_EVENTS);
        let mut events = session
            .network_events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        events.reverse();
        let failed = events
            .iter()
            .filter(|event| event.ok == Some(false) || event.error.is_some())
            .count();
        if clear {
            session.network_events.clear();
        }
        Ok(NativeCdpActionResult::success(
            "Read native CDP network diagnostics.",
            json!({
                "events": events,
                "summary": {
                    "failedRequests": failed,
                    "inflight": session.inflight_requests.len(),
                }
            }),
            current_url_from_session(session),
        ))
    }

    pub fn state_snapshot(
        &self,
        session_id: Option<String>,
        include_storage: bool,
        include_cookies: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let storage = if include_storage {
            runtime_evaluate(
                &mut client,
                session,
                r#"(() => {
                    const dump = (storage) => Object.fromEntries(Array.from({ length: storage.length }, (_, i) => {
                        const key = storage.key(i);
                        return [key, storage.getItem(key)];
                    }));
                    return { localStorage: dump(localStorage), sessionStorage: dump(sessionStorage) };
                })()"#,
                CDP_RESPONSE_TIMEOUT,
            )?
        } else {
            JsonValue::Null
        };
        let cookies = if include_cookies {
            client.command(
                session,
                "Network.getAllCookies",
                json!({}),
                CDP_RESPONSE_TIMEOUT,
            )?
        } else {
            JsonValue::Null
        };
        let state = runtime_evaluate(
            &mut client,
            session,
            "({ url: location.href, title: document.title, readyState: document.readyState })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        Ok(NativeCdpActionResult::success(
            "Captured native CDP browser state snapshot.",
            json!({
                "schema": "xero.browser_native_state_snapshot.v1",
                "manifest": { "createdAt": now_timestamp(), "engine": "native_cdp" },
                "page": state,
                "storage": storage,
                "cookies": cookies,
            }),
            current_url_from_session(session),
        ))
    }

    pub fn state_restore(
        &self,
        session_id: Option<String>,
        snapshot: JsonValue,
        navigate: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        enable_common_domains(&mut client, session)?;

        if let Some(cookies) = snapshot
            .get("cookies")
            .and_then(|value| value.get("cookies"))
            .and_then(JsonValue::as_array)
        {
            client.command(
                session,
                "Network.setCookies",
                json!({ "cookies": cookies }),
                CDP_RESPONSE_TIMEOUT,
            )?;
        }

        if navigate {
            if let Some(url) = snapshot
                .get("page")
                .and_then(|page| page.get("url"))
                .and_then(JsonValue::as_str)
            {
                client.command(
                    session,
                    "Page.navigate",
                    json!({ "url": url }),
                    CDP_RESPONSE_TIMEOUT,
                )?;
                wait_for_load_with_client(&mut client, session, Duration::from_secs(15))?;
            }
        }

        if let Some(storage) = snapshot.get("storage") {
            let storage_json = serde_json::to_string(storage).map_err(|error| {
                CommandError::system_fault(
                    "browser_native_state_encode_failed",
                    format!("Xero could not encode browser storage restore payload: {error}"),
                )
            })?;
            let expression = format!(
                r#"(() => {{
                    const snapshot = {storage_json};
                    const restore = (storage, values) => {{
                        if (!values || typeof values !== 'object') return;
                        for (const [key, value] of Object.entries(values)) storage.setItem(key, String(value));
                    }};
                    restore(localStorage, snapshot.localStorage);
                    restore(sessionStorage, snapshot.sessionStorage);
                    return {{ restored: true }};
                }})()"#
            );
            runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        }

        session.refresh_active_page_from_runtime(&mut client)?;
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Restored native CDP browser state snapshot.",
            json!({ "session": session.metadata() }),
            current_url_from_session(session),
        ))
    }

    pub fn find_best(
        &self,
        session_id: Option<String>,
        intent: &str,
        text: Option<&str>,
        role: Option<&str>,
        cached_selectors: &[String],
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression = native_find_best_expression(intent, text, role, cached_selectors)?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            format!("Found best native CDP target for `{intent}`."),
            result,
            current_url_from_session(session),
        ))
    }

    pub fn analyze_form(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression = native_analyze_form_expression(selector)?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            "Analyzed native CDP browser form.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn fill_form(
        &self,
        session_id: Option<String>,
        selector: Option<&str>,
        fields: &BTreeMap<String, String>,
        submit: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression = native_fill_form_expression(selector, fields, submit)?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        client.drain_events(session, Duration::from_millis(300));
        Ok(NativeCdpActionResult::success(
            "Filled native CDP browser form.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn frame_list(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let tree = client.command(
            session,
            "Page.getFrameTree",
            json!({}),
            CDP_RESPONSE_TIMEOUT,
        )?;
        Ok(NativeCdpActionResult::success(
            "Read native CDP frame tree.",
            tree,
            current_url_from_session(session),
        ))
    }

    pub fn dialog_list(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let _ = client.command(session, "Page.enable", json!({}), CDP_RESPONSE_TIMEOUT);
        client.drain_events(session, Duration::from_millis(150));
        Ok(NativeCdpActionResult::success(
            "Listed native CDP dialogs.",
            json!({ "dialogs": session.dialogs.clone() }),
            current_url_from_session(session),
        ))
    }

    pub fn dialog_handle(
        &self,
        session_id: Option<String>,
        accept: bool,
        prompt_text: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(
            None,
            if accept {
                "dialog_accept"
            } else {
                "dialog_dismiss"
            },
        )?;
        let mut client = session.connect_page()?;
        let params = if let Some(prompt_text) = prompt_text {
            json!({ "accept": accept, "promptText": prompt_text })
        } else {
            json!({ "accept": accept })
        };
        let result = client.command(
            session,
            "Page.handleJavaScriptDialog",
            params,
            CDP_RESPONSE_TIMEOUT,
        )?;
        client.drain_events(session, Duration::from_millis(100));
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            if accept {
                "Accepted native CDP dialog."
            } else {
                "Dismissed native CDP dialog."
            },
            json!({ "result": result, "dialogs": session.dialogs.clone() }),
            current_url_from_session(session),
        ))
    }

    pub fn download_list(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        if let Ok(mut client) = session.connect_page() {
            let _ = enable_download_events(&mut client, session);
            client.drain_events(session, Duration::from_millis(150));
        }
        Ok(NativeCdpActionResult::success(
            "Listed native CDP downloads.",
            json!({ "downloads": session.downloads.clone() }),
            current_url_from_session(session),
        ))
    }

    pub fn download_save(
        &self,
        session_id: Option<String>,
        guid: &str,
        destination: PathBuf,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "download_save")?;
        let (source, source_url, byte_size) = {
            let download = session
                .downloads
                .iter_mut()
                .rev()
                .find(|event| event.guid == guid)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "browser_native_download_missing",
                        format!("Native CDP download `{guid}` was not found."),
                    )
                })?;
            let source = download.managed_path.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_native_download_path_missing",
                    format!("Native CDP download `{guid}` does not expose a managed path yet."),
                )
            })?;
            (
                source,
                download.url.clone(),
                download.total_bytes.or(download.received_bytes),
            )
        };
        if !source.is_file() {
            return Err(CommandError::user_fixable(
                "browser_native_download_file_missing",
                format!(
                    "Native CDP download file `{}` is not available yet.",
                    source.display()
                ),
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CommandError::retryable(
                    "browser_native_download_save_dir_failed",
                    format!(
                        "Xero could not prepare download destination {}: {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        fs::copy(&source, &destination).map_err(|error| {
            CommandError::retryable(
                "browser_native_download_save_failed",
                format!(
                    "Xero could not save native browser download from {} to {}: {error}",
                    source.display(),
                    destination.display()
                ),
            )
        })?;
        if let Some(download) = session
            .downloads
            .iter_mut()
            .rev()
            .find(|event| event.guid == guid)
        {
            download.saved_path = Some(destination.clone());
            download.updated_at = now_timestamp();
        }
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Saved native CDP download.",
            json!({
                "guid": guid,
                "sourceUrl": source_url,
                "mimeType": JsonValue::Null,
                "byteSize": byte_size,
                "destination": destination,
                "policy": "file_transfer_approval_required",
                "artifactManifest": {
                    "schema": "xero.browser_download_save_manifest.v1",
                    "createdAt": now_timestamp(),
                }
            }),
            current_url_from_session(session),
        )
        .with_evidence(vec![destination.to_string_lossy().into_owned()]))
    }

    pub fn download_clear(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "download_clear")?;
        let cleared = session.downloads.len();
        session.downloads.clear();
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Cleared native CDP download metadata.",
            json!({ "cleared": cleared }),
            current_url_from_session(session),
        ))
    }

    pub fn trace_start(
        &self,
        session_id: Option<String>,
        categories: Option<Vec<String>>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "trace_start")?;
        if session.sensitive_mode {
            return Err(CommandError::user_fixable(
                "browser_native_trace_sensitive_blocked",
                "Native CDP tracing is blocked while sensitive mode is enabled.",
            ));
        }
        let mut client = session.connect_page()?;
        let categories = categories.unwrap_or_else(default_trace_categories);
        client.command(
            session,
            "Tracing.start",
            json!({
                "categories": categories.join(","),
                "transferMode": "ReturnAsStream",
            }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.trace = NativeTraceState {
            status: "recording".into(),
            categories: categories.clone(),
            started_at: Some(now_timestamp()),
            completed_at: None,
            stream_handle: None,
            artifact_path: None,
            manifest_path: None,
        };
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Started native CDP trace.",
            json!({ "trace": session.trace }),
            current_url_from_session(session),
        ))
    }

    pub fn trace_stop(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "trace_stop")?;
        if session.trace.status != "recording" {
            return Err(CommandError::user_fixable(
                "browser_native_trace_not_recording",
                "No native CDP trace is currently recording.",
            ));
        }
        let mut client = session.connect_page()?;
        client.command(session, "Tracing.end", json!({}), CDP_RESPONSE_TIMEOUT)?;
        let deadline = Instant::now() + Duration::from_secs(8);
        while session.trace.stream_handle.is_none() && Instant::now() < deadline {
            client.drain_events(session, Duration::from_millis(200));
        }
        let stream = session.trace.stream_handle.clone().ok_or_else(|| {
            CommandError::retryable(
                "browser_native_trace_stream_missing",
                "Native CDP tracing stopped but did not provide an IO stream handle.",
            )
        })?;
        let trace_text = read_cdp_stream_to_string(&mut client, session, &stream)?;
        let trace_path = write_text_artifact(
            &session.artifact_root,
            "traces",
            "browser-trace",
            "json",
            &trace_text,
        )?;
        let manifest = json!({
            "schema": "xero.browser_trace_manifest.v1",
            "createdAt": now_timestamp(),
            "engine": "native_cdp",
            "tracePath": trace_path,
            "categories": session.trace.categories,
            "redaction": "trace is local-only and blocked in sensitive mode",
        });
        let manifest_path = write_json_artifact(
            &session.artifact_root,
            "traces",
            "browser-trace-manifest",
            &manifest,
        )?;
        session.trace.status = "stopped".into();
        session.trace.artifact_path = Some(trace_path.clone());
        session.trace.manifest_path = Some(manifest_path.clone());
        session.trace.completed_at = Some(now_timestamp());
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Stopped and exported native CDP trace.",
            json!({ "trace": session.trace, "manifest": manifest }),
            current_url_from_session(session),
        )
        .with_evidence(vec![
            trace_path.to_string_lossy().into_owned(),
            manifest_path.to_string_lossy().into_owned(),
        ]))
    }

    pub fn trace_status(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        Ok(NativeCdpActionResult::success(
            "Read native CDP trace status.",
            json!({ "trace": session.trace }),
            current_url_from_session(session),
        ))
    }

    pub fn trace_export(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let Some(trace_path) = session.trace.artifact_path.clone() else {
            return Err(CommandError::user_fixable(
                "browser_native_trace_export_missing",
                "No stopped native CDP trace artifact is available to export.",
            ));
        };
        let mut evidence = vec![trace_path.to_string_lossy().into_owned()];
        if let Some(manifest) = &session.trace.manifest_path {
            evidence.push(manifest.to_string_lossy().into_owned());
        }
        Ok(NativeCdpActionResult::success(
            "Exported native CDP trace metadata.",
            json!({ "trace": session.trace }),
            current_url_from_session(session),
        )
        .with_evidence(evidence))
    }

    pub fn visual_baseline_save(
        &self,
        session_id: Option<String>,
        name: &str,
        selector: Option<&str>,
        full_page: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "visual_baseline_save")?;
        if session.sensitive_mode {
            return Err(CommandError::user_fixable(
                "browser_native_visual_sensitive_blocked",
                "Native visual baselines are blocked while sensitive mode is enabled.",
            ));
        }
        let mut client = session.connect_page()?;
        let (base64, metadata) = capture_visual_base64(&mut client, session, selector, full_page)?;
        let safe_name = safe_artifact_name(name);
        let dir = session.artifact_root.join("visual-baselines");
        fs::create_dir_all(&dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_visual_baseline_dir_failed",
                format!("Xero could not prepare visual baseline directory: {error}"),
            )
        })?;
        let path = dir.join(format!("{safe_name}.png"));
        write_base64_to_path(&path, &base64)?;
        let manifest = json!({
            "schema": "xero.browser_visual_baseline.v1",
            "name": name,
            "createdAt": now_timestamp(),
            "engine": "native_cdp",
            "baselinePath": path,
            "viewport": metadata,
            "emulationState": session.emulation_state,
            "redaction": "blocked in sensitive mode",
        });
        let manifest_path = dir.join(format!("{safe_name}.json"));
        write_json_to_path(&manifest_path, &manifest)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Saved native CDP visual baseline.",
            json!({ "name": name, "baselinePath": path, "manifestPath": manifest_path }),
            current_url_from_session(session),
        )
        .with_evidence(vec![
            path.to_string_lossy().into_owned(),
            manifest_path.to_string_lossy().into_owned(),
        ]))
    }

    pub fn visual_baseline_list(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let dir = session.artifact_root.join("visual-baselines");
        let baselines = fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<JsonValue>(&text).ok())
            .collect::<Vec<_>>();
        Ok(NativeCdpActionResult::success(
            "Listed native CDP visual baselines.",
            json!({ "baselines": baselines }),
            current_url_from_session(session),
        ))
    }

    pub fn visual_baseline_delete(
        &self,
        session_id: Option<String>,
        name: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "visual_baseline_delete")?;
        let safe_name = safe_artifact_name(name);
        let dir = session.artifact_root.join("visual-baselines");
        let png = dir.join(format!("{safe_name}.png"));
        let manifest = dir.join(format!("{safe_name}.json"));
        let removed_png = fs::remove_file(&png).is_ok();
        let removed_manifest = fs::remove_file(&manifest).is_ok();
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Deleted native CDP visual baseline.",
            json!({ "name": name, "removedPng": removed_png, "removedManifest": removed_manifest }),
            current_url_from_session(session),
        ))
    }

    pub fn visual_diff(
        &self,
        session_id: Option<String>,
        name: &str,
        threshold_percent: Option<f64>,
        selector: Option<&str>,
        full_page: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "visual_diff")?;
        if session.sensitive_mode {
            return Err(CommandError::user_fixable(
                "browser_native_visual_sensitive_blocked",
                "Native visual diff is blocked while sensitive mode is enabled.",
            ));
        }
        let safe_name = safe_artifact_name(name);
        let baseline_path = session
            .artifact_root
            .join("visual-baselines")
            .join(format!("{safe_name}.png"));
        if !baseline_path.is_file() {
            return Err(CommandError::user_fixable(
                "browser_native_visual_baseline_missing",
                format!("Native visual baseline `{name}` does not exist."),
            ));
        }
        let mut client = session.connect_page()?;
        let (current_base64, viewport) =
            capture_visual_base64(&mut client, session, selector, full_page)?;
        let current_bytes = base64::engine::general_purpose::STANDARD
            .decode(current_base64.as_bytes())
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_native_visual_decode_failed",
                    format!("Xero could not decode current screenshot: {error}"),
                )
            })?;
        let baseline_bytes = fs::read(&baseline_path).map_err(|error| {
            CommandError::retryable(
                "browser_native_visual_baseline_read_failed",
                format!(
                    "Xero could not read visual baseline {}: {error}",
                    baseline_path.display()
                ),
            )
        })?;
        let diff = visual_diff_bytes(&baseline_bytes, &current_bytes)?;
        let diff_dir = session.artifact_root.join("visual-diffs");
        fs::create_dir_all(&diff_dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_visual_diff_dir_failed",
                format!("Xero could not prepare visual diff directory: {error}"),
            )
        })?;
        let stamp = now_timestamp().replace([':', '.'], "-");
        let current_path = diff_dir.join(format!("{safe_name}-{stamp}-current.png"));
        let diff_path = diff_dir.join(format!("{safe_name}-{stamp}-diff.png"));
        fs::write(&current_path, &current_bytes).map_err(|error| {
            CommandError::retryable(
                "browser_native_visual_current_write_failed",
                format!("Xero could not write current visual diff image: {error}"),
            )
        })?;
        diff.image.save(&diff_path).map_err(|error| {
            CommandError::retryable(
                "browser_native_visual_diff_write_failed",
                format!("Xero could not write visual diff image: {error}"),
            )
        })?;
        let threshold = threshold_percent.unwrap_or(DEFAULT_VISUAL_DIFF_THRESHOLD_PERCENT);
        let pass = diff.percent_difference <= threshold;
        let manifest = json!({
            "schema": "xero.browser_visual_diff.v1",
            "createdAt": now_timestamp(),
            "name": name,
            "pass": pass,
            "pixelCount": diff.pixel_count,
            "differentPixels": diff.different_pixels,
            "percentDifference": diff.percent_difference,
            "thresholdPercent": threshold,
            "baselinePath": baseline_path,
            "currentPath": current_path,
            "diffPath": diff_path,
            "viewport": viewport,
            "emulationState": session.emulation_state,
            "redaction": "blocked in sensitive mode",
        });
        let manifest_path = diff_dir.join(format!("{safe_name}-{stamp}.json"));
        write_json_to_path(&manifest_path, &manifest)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Compared native CDP visual baseline.",
            manifest,
            current_url_from_session(session),
        )
        .with_evidence(vec![
            current_path.to_string_lossy().into_owned(),
            diff_path.to_string_lossy().into_owned(),
            manifest_path.to_string_lossy().into_owned(),
        ]))
    }

    pub fn emulate_device(
        &self,
        session_id: Option<String>,
        preset: Option<String>,
        mut fields: JsonValue,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "emulate_device")?;
        let mut preset_state = preset
            .as_deref()
            .map(device_preset_state)
            .transpose()?
            .unwrap_or_else(|| json!({}));
        merge_json_objects(&mut preset_state, &mut fields);
        preset_state["schema"] = json!("xero.browser_native_emulation_state.v1");
        preset_state["active"] = json!(true);
        preset_state["preset"] = json!(preset);
        preset_state["updatedAt"] = json!(now_timestamp());
        let mut client = session.connect_page()?;
        apply_emulation_state(&mut client, session, &preset_state)?;
        session.emulation_state = preset_state.clone();
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Applied native CDP device emulation.",
            preset_state,
            current_url_from_session(session),
        ))
    }

    pub fn clear_emulation(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "clear_emulation")?;
        let mut client = session.connect_page()?;
        client.command(
            session,
            "Emulation.clearDeviceMetricsOverride",
            json!({}),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let _ = client.command(
            session,
            "Emulation.setTouchEmulationEnabled",
            json!({ "enabled": false }),
            CDP_RESPONSE_TIMEOUT,
        );
        session.emulation_state = json!({
            "schema": "xero.browser_native_emulation_state.v1",
            "active": false,
            "updatedAt": now_timestamp(),
        });
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Cleared native CDP device emulation.",
            session.emulation_state.clone(),
            current_url_from_session(session),
        ))
    }

    pub fn emulation_state(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        Ok(NativeCdpActionResult::success(
            "Read native CDP emulation state.",
            session.emulation_state.clone(),
            current_url_from_session(session),
        ))
    }

    pub fn extract(
        &self,
        session_id: Option<String>,
        mode: &str,
        selector: Option<&str>,
        selector_map: Option<BTreeMap<String, String>>,
        limit: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression =
            native_extract_expression(mode, selector, selector_map, limit.unwrap_or(100))?;
        let extracted = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        let (redacted, redaction_changed) = redact_json_for_persistence(&extracted);
        Ok(NativeCdpActionResult::success(
            "Extracted bounded native CDP page data.",
            json!({
                "schema": "xero.browser_extract_result.v1",
                "mode": mode,
                "untrusted": true,
                "redactionChanged": redaction_changed,
                "data": redacted,
            }),
            current_url_from_session(session),
        ))
    }

    pub fn switch_page(
        &self,
        session_id: Option<String>,
        target_id: Option<String>,
        url_contains: Option<String>,
        title_contains: Option<String>,
        index: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "switch_page")?;
        let pages = if session.endpoint.starts_with("http://") {
            fetch_pages(&session.endpoint)?
        } else {
            session.active_page.clone().into_iter().collect()
        };
        let page = pages
            .iter()
            .enumerate()
            .find(|(page_index, page)| {
                target_id
                    .as_deref()
                    .is_some_and(|target| page.target_id == target)
                    || url_contains
                        .as_deref()
                        .is_some_and(|needle| page.url.contains(needle))
                    || title_contains
                        .as_deref()
                        .is_some_and(|needle| page.title.contains(needle))
                    || index.is_some_and(|wanted| wanted == *page_index)
            })
            .map(|(_, page)| page.clone())
            .or_else(|| {
                if target_id.is_none()
                    && url_contains.is_none()
                    && title_contains.is_none()
                    && index.is_none()
                {
                    pages.first().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_native_page_target_missing",
                    "No native CDP page matched the requested switch_page target.",
                )
            })?;
        session.active_page = Some(page.clone());
        session.active_frame = None;
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Switched native CDP active page.",
            json!({ "activePage": page, "pages": pages }),
            current_url_from_session(session),
        ))
    }

    pub fn close_page(
        &self,
        session_id: Option<String>,
        target_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "close_page")?;
        let target_id = target_id
            .or_else(|| {
                session
                    .active_page
                    .as_ref()
                    .map(|page| page.target_id.clone())
            })
            .ok_or_else(|| CommandError::invalid_request("targetId"))?;
        let mut client = session.connect_page()?;
        let result = client.command(
            session,
            "Target.closeTarget",
            json!({ "targetId": target_id }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.active_page = if session.endpoint.starts_with("http://") {
            fetch_pages(&session.endpoint)?.into_iter().next()
        } else {
            None
        };
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Closed native CDP page.",
            json!({ "result": result, "activePage": session.active_page }),
            current_url_from_session(session),
        ))
    }

    pub fn select_frame(
        &self,
        session_id: Option<String>,
        frame_id: Option<String>,
        name: Option<String>,
        url_contains: Option<String>,
        index: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "select_frame")?;
        let mut client = session.connect_page()?;
        let tree = client.command(
            session,
            "Page.getFrameTree",
            json!({}),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let frames = flatten_frame_tree(&tree);
        let frame = frames
            .iter()
            .enumerate()
            .find(|(frame_index, frame)| {
                frame_id
                    .as_deref()
                    .is_some_and(|wanted| frame.frame_id == wanted)
                    || name
                        .as_deref()
                        .is_some_and(|wanted| frame.name.as_deref() == Some(wanted))
                    || url_contains.as_deref().is_some_and(|needle| {
                        frame.url.as_deref().unwrap_or_default().contains(needle)
                    })
                    || index.is_some_and(|wanted| wanted == *frame_index)
            })
            .map(|(_, frame)| frame.clone())
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_native_frame_target_missing",
                    "No native CDP frame matched the requested select_frame target.",
                )
            })?;
        session.active_frame = Some(frame.clone());
        session.touch();
        persist_session_metadata(session)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Selected native CDP active frame.",
            json!({ "activeFrame": frame, "frames": frames }),
            current_url_from_session(session),
        ))
    }

    pub fn frame_state(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let tree = client.command(
            session,
            "Page.getFrameTree",
            json!({}),
            CDP_RESPONSE_TIMEOUT,
        )?;
        let frames = flatten_frame_tree(&tree);
        Ok(NativeCdpActionResult::success(
            "Read native CDP frame state.",
            json!({
                "activeFrame": session.active_frame,
                "frames": frames,
                "limitation": "Cross-origin frames may require target/session routing; Xero reports the limitation when Chrome does not expose a frame execution context."
            }),
            current_url_from_session(session),
        ))
    }

    pub fn auth_profile_save(
        &self,
        session_id: Option<String>,
        name: &str,
        include_storage: bool,
        include_cookies: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let snapshot = self.state_snapshot(session_id.clone(), include_storage, include_cookies)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "auth_profile_save")?;
        let safe_name = safe_artifact_name(name);
        let payload = json!({
            "schema": "xero.browser_auth_profile.v1",
            "name": name,
            "createdAt": now_timestamp(),
            "snapshot": snapshot.data,
            "redaction": "secret-like values are redacted before persistence",
        });
        let dir = session.artifact_root.join("auth-profiles");
        fs::create_dir_all(&dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_auth_profile_dir_failed",
                format!("Xero could not prepare auth profile directory: {error}"),
            )
        })?;
        let path = dir.join(format!("{safe_name}.json"));
        write_json_to_path(&path, &payload)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Saved native CDP auth profile.",
            json!({ "name": name, "artifactPath": path, "credentialStorage": "none" }),
            current_url_from_session(session),
        )
        .with_evidence(vec![path.to_string_lossy().into_owned()]))
    }

    pub fn auth_profile_list(
        &self,
        session_id: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let dir = session.artifact_root.join("auth-profiles");
        let profiles = fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<JsonValue>(&text).ok())
            .map(|profile| {
                json!({
                    "name": profile.get("name").cloned().unwrap_or(JsonValue::Null),
                    "createdAt": profile.get("createdAt").cloned().unwrap_or(JsonValue::Null),
                    "credentialStorage": "none",
                })
            })
            .collect::<Vec<_>>();
        Ok(NativeCdpActionResult::success(
            "Listed native CDP auth profiles.",
            json!({ "profiles": profiles }),
            current_url_from_session(session),
        ))
    }

    pub fn auth_profile_restore(
        &self,
        session_id: Option<String>,
        name: &str,
        navigate: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let profile_path = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
            let session = active_session_mut(&mut sessions, session_id.as_deref())?;
            session.control_allowed(None, "auth_profile_restore")?;
            session
                .artifact_root
                .join("auth-profiles")
                .join(format!("{}.json", safe_artifact_name(name)))
        };
        let profile = fs::read_to_string(&profile_path).map_err(|error| {
            CommandError::user_fixable(
                "browser_native_auth_profile_missing",
                format!(
                    "Xero could not read auth profile `{name}` at {}: {error}",
                    profile_path.display()
                ),
            )
        })?;
        let profile = serde_json::from_str::<JsonValue>(&profile).map_err(|error| {
            CommandError::user_fixable(
                "browser_native_auth_profile_invalid",
                format!("Auth profile `{name}` is invalid: {error}"),
            )
        })?;
        let snapshot = profile.get("snapshot").cloned().unwrap_or(JsonValue::Null);
        self.state_restore(session_id, snapshot, navigate)
    }

    pub fn auth_profile_delete(
        &self,
        session_id: Option<String>,
        name: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "auth_profile_delete")?;
        let path = session
            .artifact_root
            .join("auth-profiles")
            .join(format!("{}.json", safe_artifact_name(name)));
        let removed = fs::remove_file(&path).is_ok();
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Deleted native CDP auth profile.",
            json!({ "name": name, "removed": removed }),
            current_url_from_session(session),
        ))
    }

    pub fn vault_save(
        &self,
        session_id: Option<String>,
        name: &str,
        origin: Option<String>,
        username: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "vault_save")?;
        let payload = json!({
            "schema": "xero.browser_vault_metadata.v1",
            "name": name,
            "origin": origin,
            "username": username,
            "createdAt": now_timestamp(),
            "credentialMaterialStored": false,
            "loginReplayAvailable": false,
            "note": "This metadata-only vault record does not store passwords or tokens.",
        });
        let dir = session.artifact_root.join("vault");
        fs::create_dir_all(&dir).map_err(|error| {
            CommandError::retryable(
                "browser_native_vault_dir_failed",
                format!("Xero could not prepare browser vault metadata directory: {error}"),
            )
        })?;
        let path = dir.join(format!("{}.json", safe_artifact_name(name)));
        write_json_to_path(&path, &payload)?;
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Saved native browser vault metadata.",
            json!({ "vault": payload, "artifactPath": path }),
            current_url_from_session(session),
        ))
    }

    pub fn vault_list(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let dir = session.artifact_root.join("vault");
        let entries = fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<JsonValue>(&text).ok())
            .collect::<Vec<_>>();
        Ok(NativeCdpActionResult::success(
            "Listed native browser vault metadata.",
            json!({ "entries": entries }),
            current_url_from_session(session),
        ))
    }

    pub fn vault_delete(
        &self,
        session_id: Option<String>,
        name: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        session.control_allowed(None, "vault_delete")?;
        let path = session
            .artifact_root
            .join("vault")
            .join(format!("{}.json", safe_artifact_name(name)));
        let removed = fs::remove_file(path).is_ok();
        session.finish_control_action();
        Ok(NativeCdpActionResult::success(
            "Deleted native browser vault metadata.",
            json!({ "name": name, "removed": removed }),
            current_url_from_session(session),
        ))
    }

    pub fn vault_login(
        &self,
        session_id: Option<String>,
        name: &str,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        Ok(NativeCdpActionResult {
            status: "unavailable".into(),
            summary: "Native browser vault login replay is unavailable until encrypted credential storage, policy approval, and durable redaction are complete.".into(),
            data: json!({
                "error": {
                    "code": "browser_capability_unavailable",
                    "engine": "native_cdp",
                    "action": "vault_login",
                    "name": name,
                    "suggestedFallbacks": ["Use auth_profile_restore for browser state profiles that do not store passwords.", "Request sensitive input explicitly before any credential-bearing login flow."]
                }
            }),
            evidence_refs: Vec::new(),
            current_url: current_url_from_session(session),
        })
    }

    pub fn viewer_state(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        Ok(NativeCdpActionResult::success(
            "Read native browser viewer state.",
            json!({
                "viewer": session.viewer,
                "currentPage": session.active_page,
                "activeFrame": session.active_frame,
                "latestScreenshot": {
                    "sensitiveModeSuppressed": session.sensitive_mode,
                },
                "downloads": session.downloads,
                "trace": session.trace,
            }),
            current_url_from_session(session),
        ))
    }

    pub fn viewer_update(
        &self,
        session_id: Option<String>,
        action: &str,
        value: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        match action {
            "viewer_goal" => session.viewer.goal = value,
            "takeover" => {
                session.viewer.control_owner = Some(value.unwrap_or_else(|| "human".into()))
            }
            "release_control" => session.viewer.control_owner = None,
            "pause" => session.viewer.paused = true,
            "resume" => {
                session.viewer.paused = false;
                session.viewer.aborted = false;
            }
            "step" => {
                session.viewer.paused = true;
                session.viewer.step_budget = session.viewer.step_budget.saturating_add(1);
            }
            "abort" => session.viewer.aborted = true,
            "sensitive_on" => {
                session.sensitive_mode = true;
                session.viewer.sensitive_mode = true;
            }
            "sensitive_off" => {
                session.sensitive_mode = false;
                session.viewer.sensitive_mode = false;
            }
            other => {
                return Err(CommandError::user_fixable(
                    "browser_native_viewer_action_invalid",
                    format!("Unsupported native browser viewer action `{other}`."),
                ));
            }
        }
        session.viewer.touch();
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Updated native browser viewer state.",
            json!({ "viewer": session.viewer, "action": action }),
            current_url_from_session(session),
        ))
    }

    pub fn session_metadatas(&self) -> CommandResult<Vec<NativeCdpSessionMetadata>> {
        self.sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))
            .map(|sessions| sessions.values().map(NativeCdpSession::metadata).collect())
    }

    pub fn prompt_injection_scan(
        &self,
        session_id: Option<String>,
        include_hidden: bool,
        selector: Option<&str>,
        limit: Option<usize>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let expression =
            native_prompt_injection_scan_expression(include_hidden, selector, limit.unwrap_or(80))?;
        let result = runtime_evaluate(&mut client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
        Ok(NativeCdpActionResult::success(
            "Scanned native CDP page content for prompt-injection indicators.",
            result,
            current_url_from_session(session),
        ))
    }

    pub fn export_har(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let entries = session
            .network_events
            .iter()
            .map(|event| {
                json!({
                    "startedDateTime": event.captured_at,
                    "request": {
                        "method": event.method.clone().unwrap_or_else(|| "GET".into()),
                        "url": event.url,
                        "headers": []
                    },
                    "response": {
                        "status": event.status.unwrap_or(0),
                        "statusText": "",
                        "headers": []
                    },
                    "timings": { "send": 0, "wait": 0, "receive": 0 },
                })
            })
            .collect::<Vec<_>>();
        let entry_count = entries.len();
        let har = json!({
            "schema": "xero.browser_har_export.v1",
            "manifest": { "createdAt": now_timestamp(), "engine": "native_cdp" },
            "log": {
                "version": "1.2",
                "creator": { "name": "Xero Native CDP", "version": env!("CARGO_PKG_VERSION") },
                "entries": entries,
            }
        });
        let path = write_json_artifact(&session.artifact_root, "har", "browser", &har)?;
        let path_string = path.to_string_lossy().into_owned();
        Ok(NativeCdpActionResult::success(
            "Exported native CDP HAR artifact.",
            json!({ "artifactPath": path_string, "entryCount": entry_count }),
            current_url_from_session(session),
        )
        .with_evidence(vec![path.to_string_lossy().into_owned()]))
    }

    pub fn export_pdf(&self, session_id: Option<String>) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let result = client.command(
            session,
            "Page.printToPDF",
            json!({ "printBackground": true }),
            Duration::from_secs(30),
        )?;
        let base64 = result
            .get("data")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_native_pdf_invalid",
                    "Native CDP PDF response did not include base64 data.",
                )
            })?;
        let path =
            write_base64_artifact(&session.artifact_root, "pdf", "browser-page", "pdf", base64)?;
        let path_string = path.to_string_lossy().into_owned();
        Ok(NativeCdpActionResult::success(
            "Exported native CDP PDF artifact.",
            json!({ "artifactPath": path_string }),
            current_url_from_session(session),
        )
        .with_evidence(vec![path.to_string_lossy().into_owned()]))
    }

    pub fn network_control(
        &self,
        session_id: Option<String>,
        command: &str,
        url_contains: Option<String>,
        status: Option<u16>,
        body: Option<String>,
        content_type: Option<String>,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        match command {
            "clear" => {
                session.blocked_url_patterns.clear();
                session.mocks.clear();
            }
            "block" => {
                let pattern =
                    url_contains.ok_or_else(|| CommandError::invalid_request("urlContains"))?;
                session.blocked_url_patterns.push(pattern);
            }
            "mock" => {
                let pattern =
                    url_contains.ok_or_else(|| CommandError::invalid_request("urlContains"))?;
                session.mocks.push(NativeNetworkMock {
                    url_contains: pattern,
                    status: status.unwrap_or(200),
                    body: body.unwrap_or_default(),
                    content_type: content_type
                        .unwrap_or_else(|| "text/plain; charset=utf-8".into()),
                });
            }
            other => {
                return Err(CommandError::user_fixable(
                    "browser_native_network_control_invalid",
                    format!("Unsupported native CDP network control command `{other}`."),
                ));
            }
        }
        let mut client = session.connect_page()?;
        enable_network_controls(&mut client, session)?;
        session.touch();
        persist_session_metadata(session)?;
        Ok(NativeCdpActionResult::success(
            "Updated native CDP network controls.",
            json!({
                "blockedUrlPatterns": session.blocked_url_patterns.clone(),
                "mockCount": session.mocks.len(),
                "note": "Mocks are fulfilled while Xero is actively processing CDP events for this session."
            }),
            current_url_from_session(session),
        ))
    }

    pub fn debug_bundle(
        &self,
        session_id: Option<String>,
        include_screenshot: bool,
    ) -> CommandResult<NativeCdpActionResult> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_error("browser_native_sessions_lock_poisoned"))?;
        let session = active_session_mut(&mut sessions, session_id.as_deref())?;
        let mut client = session.connect_page()?;
        let state = runtime_evaluate(
            &mut client,
            session,
            "({ url: location.href, title: document.title, readyState: document.readyState })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        let screenshot = if include_screenshot && !session.sensitive_mode {
            client
                .command(
                    session,
                    "Page.captureScreenshot",
                    json!({ "format": "png", "fromSurface": true }),
                    CDP_RESPONSE_TIMEOUT,
                )
                .ok()
                .and_then(|value| {
                    value
                        .get("data")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned)
                })
        } else {
            None
        };
        let bundle = json!({
            "schema": "xero.browser_native_debug_bundle.v1",
            "manifest": {
                "createdAt": now_timestamp(),
                "engine": "native_cdp",
                "sensitiveMode": session.sensitive_mode,
            },
            "state": state,
            "session": session.metadata(),
            "console": session.console_events.clone(),
            "network": session.network_events.clone(),
            "dialogs": session.dialogs.clone(),
            "screenshotBase64": screenshot,
        });
        let path = write_json_artifact(
            &session.artifact_root,
            "debug-bundles",
            "debug-bundle",
            &bundle,
        )?;
        let path_string = path.to_string_lossy().into_owned();
        Ok(NativeCdpActionResult::success(
            "Created native CDP debug bundle.",
            json!({ "artifactPath": path_string, "bundle": bundle }),
            current_url_from_session(session),
        )
        .with_evidence(vec![path.to_string_lossy().into_owned()]))
    }

    fn has_session(&self, session_id: Option<&str>) -> bool {
        let wanted = session_id.unwrap_or(DEFAULT_SESSION_ID);
        self.sessions
            .lock()
            .ok()
            .is_some_and(|sessions| sessions.contains_key(wanted))
    }

    fn allocate_session_id(&self) -> String {
        let next = self.next_session.fetch_add(1, Ordering::AcqRel) + 1;
        format!("native-{next}-{}", random_token(12))
    }
}

impl NativeCdpSession {
    fn new(
        session_id: String,
        label: String,
        endpoint: String,
        browser_path: Option<PathBuf>,
        debugging_port: Option<u16>,
        profile_dir: PathBuf,
        artifact_root: PathBuf,
        launched_by_xero: bool,
        sensitive_mode: bool,
        child: Option<Child>,
    ) -> Self {
        let now = now_timestamp();
        Self {
            session_id,
            label,
            endpoint,
            browser_path,
            debugging_port,
            profile_dir,
            artifact_root,
            active_page: None,
            active_frame: None,
            emulation_state: json!({
                "schema": "xero.browser_native_emulation_state.v1",
                "active": false,
            }),
            viewer: NativeViewerState::new(sensitive_mode),
            launched_by_xero,
            sensitive_mode,
            created_at: now.clone(),
            updated_at: now,
            child,
            console_events: Vec::new(),
            network_events: Vec::new(),
            dialogs: Vec::new(),
            downloads: Vec::new(),
            trace: NativeTraceState::default(),
            blocked_url_patterns: Vec::new(),
            mocks: Vec::new(),
            inflight_requests: BTreeSet::new(),
            last_network_event: None,
        }
    }

    fn metadata(&self) -> NativeCdpSessionMetadata {
        NativeCdpSessionMetadata {
            schema: "xero.browser_native_cdp_session.v1".into(),
            session_id: self.session_id.clone(),
            label: self.label.clone(),
            endpoint: redact_cdp_endpoint(&self.endpoint),
            endpoint_kind: cdp_endpoint_kind(&self.endpoint).into(),
            browser_path: self.browser_path.clone(),
            debugging_port: self.debugging_port,
            profile_dir: self.profile_dir.clone(),
            artifact_root: self.artifact_root.clone(),
            active_page: self.active_page.clone(),
            active_frame: self.active_frame.clone(),
            emulation_state: self.emulation_state.clone(),
            viewer_state: self.viewer.clone(),
            launched_by_xero: self.launched_by_xero,
            sensitive_mode: self.sensitive_mode,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    fn connect_page(&mut self) -> CommandResult<CdpClient> {
        if self.active_page.is_none() && self.endpoint.starts_with("http://") {
            self.active_page = fetch_or_create_page(&self.endpoint, None)?;
        }
        let ws_url = self
            .active_page
            .as_ref()
            .map(|page| page.websocket_url.clone())
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_native_page_missing",
                    format!(
                        "Native CDP session `{}` has no active page.",
                        self.session_id
                    ),
                )
            })?;
        CdpClient::connect(&ws_url)
    }

    fn update_active_page_from_state(&mut self, state: &JsonValue) {
        if let Some(page) = &mut self.active_page {
            if let Some(url) = state.get("url").and_then(JsonValue::as_str) {
                page.url = url.to_owned();
            }
            if let Some(title) = state.get("title").and_then(JsonValue::as_str) {
                page.title = title.to_owned();
            }
        }
    }

    fn refresh_active_page_from_runtime(&mut self, client: &mut CdpClient) -> CommandResult<()> {
        let state = runtime_evaluate(
            client,
            self,
            "({ url: location.href, title: document.title })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        self.update_active_page_from_state(&state);
        Ok(())
    }

    fn touch(&mut self) {
        self.updated_at = now_timestamp();
    }

    fn push_console(&mut self, level: String, message: String) {
        let sequence = self
            .console_events
            .last()
            .map(|event| event.sequence + 1)
            .unwrap_or(1);
        self.console_events.push(NativeConsoleEvent {
            sequence,
            level,
            message,
            captured_at: now_timestamp(),
        });
        if self.console_events.len() > MAX_DIAGNOSTIC_EVENTS {
            let drain = self.console_events.len() - MAX_DIAGNOSTIC_EVENTS;
            self.console_events.drain(0..drain);
        }
    }

    fn push_network(&mut self, event: NativeNetworkEvent) {
        self.last_network_event = Some(Instant::now());
        self.network_events.push(event);
        if self.network_events.len() > MAX_DIAGNOSTIC_EVENTS {
            let drain = self.network_events.len() - MAX_DIAGNOSTIC_EVENTS;
            self.network_events.drain(0..drain);
        }
    }

    fn push_dialog(&mut self, kind: String, message: String, default_prompt: Option<String>) {
        let sequence = self
            .dialogs
            .last()
            .map(|event| event.sequence + 1)
            .unwrap_or(1);
        self.dialogs.push(NativeDialogEvent {
            sequence,
            kind,
            message,
            default_prompt,
            captured_at: now_timestamp(),
        });
        if self.dialogs.len() > MAX_DIAGNOSTIC_EVENTS {
            let drain = self.dialogs.len() - MAX_DIAGNOSTIC_EVENTS;
            self.dialogs.drain(0..drain);
        }
    }

    fn push_download(&mut self, guid: String, url: String, suggested_filename: Option<String>) {
        let sequence = self
            .downloads
            .last()
            .map(|event| event.sequence + 1)
            .unwrap_or(1);
        let now = now_timestamp();
        let managed_path = suggested_filename.as_ref().map(|name| {
            self.artifact_root
                .join("downloads")
                .join("managed")
                .join(name)
        });
        self.downloads.push(NativeDownloadEvent {
            sequence,
            guid,
            url,
            suggested_filename,
            state: "in_progress".into(),
            total_bytes: None,
            received_bytes: None,
            managed_path,
            saved_path: None,
            started_at: now.clone(),
            updated_at: now,
        });
        if self.downloads.len() > MAX_DOWNLOAD_EVENTS {
            let drain = self.downloads.len() - MAX_DOWNLOAD_EVENTS;
            self.downloads.drain(0..drain);
        }
    }

    fn update_download_progress(
        &mut self,
        guid: &str,
        state: Option<&str>,
        received_bytes: Option<u64>,
        total_bytes: Option<u64>,
    ) {
        let Some(download) = self
            .downloads
            .iter_mut()
            .rev()
            .find(|event| event.guid == guid)
        else {
            return;
        };
        if let Some(state) = state {
            download.state = state.to_owned();
        }
        if received_bytes.is_some() {
            download.received_bytes = received_bytes;
        }
        if total_bytes.is_some() {
            download.total_bytes = total_bytes;
        }
        download.updated_at = now_timestamp();
    }

    fn set_viewer_policy_event(&mut self, event: impl Into<String>) {
        self.viewer.last_policy_event = Some(event.into());
        self.viewer.touch();
    }

    fn control_allowed(&mut self, owner: Option<&str>, action: &str) -> CommandResult<()> {
        if self.viewer.aborted && !matches!(action, "viewer_state" | "resume" | "sensitive_off") {
            self.set_viewer_policy_event(format!("blocked:{action}:aborted"));
            return Err(CommandError::user_fixable(
                "browser_native_viewer_aborted",
                "This native browser session is aborted. Resume or start a new session before sending more control actions.",
            ));
        }
        if let Some(control_owner) = self.viewer.control_owner.clone() {
            if owner != Some(control_owner.as_str()) {
                self.set_viewer_policy_event(format!("blocked:{action}:takeover"));
                return Err(CommandError::user_fixable(
                    "browser_native_control_taken_over",
                    format!("Native browser control is currently owned by `{control_owner}`."),
                ));
            }
        }
        if self.viewer.paused
            && self.viewer.step_budget == 0
            && !matches!(action, "resume" | "step")
        {
            self.set_viewer_policy_event(format!("blocked:{action}:paused"));
            return Err(CommandError::user_fixable(
                "browser_native_session_paused",
                "This native browser session is paused. Resume it or issue a step before sending control actions.",
            ));
        }
        if self.viewer.step_budget > 0 {
            self.viewer.step_budget = self.viewer.step_budget.saturating_sub(1);
        }
        Ok(())
    }

    fn finish_control_action(&mut self) {
        if self.viewer.step_budget == 0 && self.viewer.paused {
            self.viewer.touch();
        }
    }
}

struct CdpClient {
    ws: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl CdpClient {
    fn connect(ws_url: &str) -> CommandResult<Self> {
        let url = Url::parse(ws_url).map_err(|error| {
            CommandError::user_fixable(
                "browser_native_ws_url_invalid",
                format!("Native CDP WebSocket URL `{ws_url}` is invalid: {error}"),
            )
        })?;
        let host = url.host_str().ok_or_else(|| {
            CommandError::user_fixable(
                "browser_native_ws_url_invalid",
                format!("Native CDP WebSocket URL `{ws_url}` is missing a host."),
            )
        })?;
        let port = url.port_or_known_default().ok_or_else(|| {
            CommandError::user_fixable(
                "browser_native_ws_url_invalid",
                format!("Native CDP WebSocket URL `{ws_url}` is missing a port."),
            )
        })?;
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_native_ws_resolve_failed",
                    format!("Xero could not resolve native CDP WebSocket host `{host}`: {error}"),
                )
            })?
            .next()
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_native_ws_resolve_failed",
                    format!("Xero could not resolve native CDP WebSocket host `{host}`."),
                )
            })?;
        let stream = TcpStream::connect_timeout(&addr, HTTP_TIMEOUT).map_err(|error| {
            CommandError::system_fault(
                "browser_native_ws_connect_failed",
                format!("Xero could not connect to native CDP WebSocket `{ws_url}`: {error}"),
            )
        })?;
        let _ = stream.set_read_timeout(Some(CDP_RESPONSE_TIMEOUT));
        let _ = stream.set_write_timeout(Some(CDP_RESPONSE_TIMEOUT));
        let (ws, _response) = tungstenite::client(ws_url, stream).map_err(|error| {
            CommandError::system_fault(
                "browser_native_ws_handshake_failed",
                format!("Xero could not open native CDP WebSocket `{ws_url}`: {error}"),
            )
        })?;
        Ok(Self { ws, next_id: 1 })
    }

    fn command(
        &mut self,
        session: &mut NativeCdpSession,
        method: &str,
        params: JsonValue,
        timeout: Duration,
    ) -> CommandResult<JsonValue> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let payload = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        self.ws
            .send(Message::Text(payload.to_string()))
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_native_cdp_send_failed",
                    format!("Xero could not send native CDP command `{method}`: {error}"),
                )
            })?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = self
                .ws
                .get_mut()
                .set_read_timeout(Some(remaining.min(Duration::from_secs(2))));
            match self.ws.read() {
                Ok(Message::Text(text)) => {
                    let value = serde_json::from_str::<JsonValue>(&text).map_err(|error| {
                        CommandError::system_fault(
                            "browser_native_cdp_decode_failed",
                            format!("Xero could not decode native CDP message: {error}"),
                        )
                    })?;
                    if value.get("id").and_then(JsonValue::as_u64) == Some(id) {
                        if let Some(error) = value.get("error") {
                            return Err(CommandError::user_fixable(
                                "browser_native_cdp_error",
                                format!("Native CDP command `{method}` failed: {error}"),
                            ));
                        }
                        return Ok(value.get("result").cloned().unwrap_or(JsonValue::Null));
                    }
                    if value.get("method").is_some() {
                        self.handle_event(session, &value);
                    }
                }
                Ok(Message::Binary(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                Ok(Message::Close(_)) => {
                    return Err(CommandError::system_fault(
                        "browser_native_cdp_closed",
                        format!("Native CDP WebSocket closed while waiting for `{method}`."),
                    ));
                }
                Ok(Message::Frame(_)) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(error) => {
                    return Err(CommandError::system_fault(
                        "browser_native_cdp_read_failed",
                        format!("Xero could not read native CDP response for `{method}`: {error}"),
                    ));
                }
            }
        }

        Err(CommandError::retryable(
            "browser_native_cdp_timeout",
            format!(
                "Native CDP command `{method}` did not complete within {} ms.",
                timeout.as_millis()
            ),
        ))
    }

    fn drain_events(&mut self, session: &mut NativeCdpSession, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            let _ = self
                .ws
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(50)));
            match self.ws.read() {
                Ok(Message::Text(text)) => {
                    if let Ok(value) = serde_json::from_str::<JsonValue>(&text) {
                        if value.get("method").is_some() {
                            self.handle_event(session, &value);
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = self
            .ws
            .get_mut()
            .set_read_timeout(Some(CDP_RESPONSE_TIMEOUT));
    }

    fn send_fire_and_forget(&mut self, method: &str, params: JsonValue) {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let payload = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let _ = self.ws.send(Message::Text(payload.to_string()));
    }

    fn handle_event(&mut self, session: &mut NativeCdpSession, event: &JsonValue) {
        let Some(method) = event.get("method").and_then(JsonValue::as_str) else {
            return;
        };
        let params = event.get("params").cloned().unwrap_or(JsonValue::Null);
        match method {
            "Runtime.consoleAPICalled" => {
                let level = params
                    .get("type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("log")
                    .to_owned();
                let message = params
                    .get("args")
                    .and_then(JsonValue::as_array)
                    .map(|args| {
                        args.iter()
                            .filter_map(|arg| {
                                arg.get("value")
                                    .or_else(|| arg.get("description"))
                                    .and_then(JsonValue::as_str)
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| params.to_string());
                session.push_console(level, message);
            }
            "Log.entryAdded" => {
                let entry = params.get("entry").unwrap_or(&JsonValue::Null);
                let level = entry
                    .get("level")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("log")
                    .to_owned();
                let message = entry
                    .get("text")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned();
                session.push_console(level, message);
            }
            "Network.requestWillBeSent" => {
                let request_id = params
                    .get("requestId")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if !request_id.is_empty() {
                    session.inflight_requests.insert(request_id.clone());
                }
                let request = params.get("request").unwrap_or(&JsonValue::Null);
                let sequence = session
                    .network_events
                    .last()
                    .map(|event| event.sequence + 1)
                    .unwrap_or(1);
                session.push_network(NativeNetworkEvent {
                    sequence,
                    request_id,
                    url: request
                        .get("url")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    method: request
                        .get("method")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned),
                    status: None,
                    ok: None,
                    resource_type: params
                        .get("type")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned),
                    error: None,
                    captured_at: now_timestamp(),
                });
            }
            "Network.responseReceived" => {
                let request_id = params
                    .get("requestId")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let response = params.get("response").unwrap_or(&JsonValue::Null);
                let status = response
                    .get("status")
                    .and_then(JsonValue::as_u64)
                    .map(|value| value as u16);
                let sequence = session
                    .network_events
                    .last()
                    .map(|event| event.sequence + 1)
                    .unwrap_or(1);
                session.push_network(NativeNetworkEvent {
                    sequence,
                    request_id,
                    url: response
                        .get("url")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    method: None,
                    status,
                    ok: status.map(|status| status < 400),
                    resource_type: params
                        .get("type")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned),
                    error: None,
                    captured_at: now_timestamp(),
                });
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                if let Some(request_id) = params.get("requestId").and_then(JsonValue::as_str) {
                    session.inflight_requests.remove(request_id);
                    if method == "Network.loadingFailed" {
                        let sequence = session
                            .network_events
                            .last()
                            .map(|event| event.sequence + 1)
                            .unwrap_or(1);
                        session.push_network(NativeNetworkEvent {
                            sequence,
                            request_id: request_id.to_owned(),
                            url: String::new(),
                            method: None,
                            status: None,
                            ok: Some(false),
                            resource_type: params
                                .get("type")
                                .and_then(JsonValue::as_str)
                                .map(str::to_owned),
                            error: params
                                .get("errorText")
                                .and_then(JsonValue::as_str)
                                .map(str::to_owned),
                            captured_at: now_timestamp(),
                        });
                    }
                }
            }
            "Page.javascriptDialogOpening" => {
                session.push_dialog(
                    params
                        .get("type")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("dialog")
                        .to_owned(),
                    params
                        .get("message")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    params
                        .get("defaultPrompt")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned),
                );
            }
            "Browser.downloadWillBegin" | "Page.downloadWillBegin" => {
                let guid = params
                    .get("guid")
                    .or_else(|| params.get("id"))
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if !guid.is_empty() {
                    session.push_download(
                        guid,
                        params
                            .get("url")
                            .and_then(JsonValue::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        params
                            .get("suggestedFilename")
                            .and_then(JsonValue::as_str)
                            .map(str::to_owned),
                    );
                }
            }
            "Browser.downloadProgress" | "Page.downloadProgress" => {
                if let Some(guid) = params
                    .get("guid")
                    .or_else(|| params.get("id"))
                    .and_then(JsonValue::as_str)
                {
                    session.update_download_progress(
                        guid,
                        params.get("state").and_then(JsonValue::as_str),
                        params.get("receivedBytes").and_then(JsonValue::as_u64),
                        params.get("totalBytes").and_then(JsonValue::as_u64),
                    );
                }
            }
            "Tracing.tracingComplete" => {
                session.trace.status = "stopped".into();
                session.trace.completed_at = Some(now_timestamp());
                session.trace.stream_handle = params
                    .get("stream")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned);
            }
            "Fetch.requestPaused" => self.handle_fetch_request_paused(session, &params),
            _ => {}
        }
    }

    fn handle_fetch_request_paused(&mut self, session: &NativeCdpSession, params: &JsonValue) {
        let Some(request_id) = params.get("requestId").and_then(JsonValue::as_str) else {
            return;
        };
        let url = params
            .get("request")
            .and_then(|request| request.get("url"))
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        if let Some(mock) = session
            .mocks
            .iter()
            .find(|mock| url.contains(&mock.url_contains))
        {
            let body = base64::engine::general_purpose::STANDARD.encode(mock.body.as_bytes());
            self.send_fire_and_forget(
                "Fetch.fulfillRequest",
                json!({
                    "requestId": request_id,
                    "responseCode": mock.status,
                    "responseHeaders": [{ "name": "content-type", "value": mock.content_type }],
                    "body": body,
                }),
            );
        } else {
            self.send_fire_and_forget("Fetch.continueRequest", json!({ "requestId": request_id }));
        }
    }
}

#[derive(Debug)]
struct SelectorPoint {
    x: f64,
    y: f64,
    raw: JsonValue,
}

fn selector_point(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    selector: &str,
) -> CommandResult<SelectorPoint> {
    let selector_json = js_string(selector)?;
    let expression = format!(
        r#"(() => {{
            const selector = {selector_json};
            const el = document.querySelector(selector);
            if (!el) throw new Error('element not found: ' + selector);
            if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});
            const rect = el.getBoundingClientRect();
            if (!rect.width || !rect.height) throw new Error('element has no visible bounds: ' + selector);
            return {{
                selector,
                x: rect.left + rect.width / 2,
                y: rect.top + rect.height / 2,
                bounds: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                visible: !!(el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)),
                enabled: !(el.disabled || el.getAttribute('aria-disabled') === 'true')
            }};
        }})()"#
    );
    let value = runtime_evaluate(client, session, &expression, CDP_RESPONSE_TIMEOUT)?;
    let x = value.get("x").and_then(JsonValue::as_f64).ok_or_else(|| {
        CommandError::system_fault(
            "browser_native_bounds_invalid",
            format!("Native CDP selector `{selector}` did not return an x coordinate."),
        )
    })?;
    let y = value.get("y").and_then(JsonValue::as_f64).ok_or_else(|| {
        CommandError::system_fault(
            "browser_native_bounds_invalid",
            format!("Native CDP selector `{selector}` did not return a y coordinate."),
        )
    })?;
    Ok(SelectorPoint { x, y, raw: value })
}

fn dispatch_mouse_click(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    x: f64,
    y: f64,
) -> CommandResult<()> {
    client.command(
        session,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseMoved", "x": x, "y": y, "button": "none" }),
        CDP_RESPONSE_TIMEOUT,
    )?;
    client.command(
        session,
        "Input.dispatchMouseEvent",
        json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1 }),
        CDP_RESPONSE_TIMEOUT,
    )?;
    client.command(
        session,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1 }),
        CDP_RESPONSE_TIMEOUT,
    )?;
    Ok(())
}

fn runtime_evaluate(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    expression: &str,
    timeout: Duration,
) -> CommandResult<JsonValue> {
    let result = client.command(
        session,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "userGesture": true,
        }),
        timeout,
    )?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(CommandError::user_fixable(
            "browser_native_script_error",
            format!("Native CDP script failed: {exception}"),
        ));
    }
    let remote = result.get("result").cloned().unwrap_or(JsonValue::Null);
    Ok(remote
        .get("value")
        .cloned()
        .or_else(|| remote.get("description").cloned())
        .unwrap_or(JsonValue::Null))
}

fn enable_common_domains(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
) -> CommandResult<()> {
    let _ = client.command(session, "Page.enable", json!({}), CDP_RESPONSE_TIMEOUT)?;
    let _ = client.command(session, "Runtime.enable", json!({}), CDP_RESPONSE_TIMEOUT)?;
    let _ = client.command(session, "Log.enable", json!({}), CDP_RESPONSE_TIMEOUT)?;
    let _ = client.command(session, "Network.enable", json!({}), CDP_RESPONSE_TIMEOUT)?;
    enable_download_events(client, session)?;
    enable_network_controls(client, session)?;
    Ok(())
}

fn enable_download_events(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
) -> CommandResult<()> {
    let download_dir = session.artifact_root.join("downloads").join("managed");
    fs::create_dir_all(&download_dir).map_err(|error| {
        CommandError::retryable(
            "browser_native_download_dir_failed",
            format!(
                "Xero could not prepare native CDP download directory at {}: {error}",
                download_dir.display()
            ),
        )
    })?;
    let _ = client.command(
        session,
        "Browser.setDownloadBehavior",
        json!({
            "behavior": "allow",
            "downloadPath": download_dir,
            "eventsEnabled": true,
        }),
        CDP_RESPONSE_TIMEOUT,
    );
    let _ = client.command(
        session,
        "Page.setDownloadBehavior",
        json!({
            "behavior": "allow",
            "downloadPath": download_dir,
        }),
        CDP_RESPONSE_TIMEOUT,
    );
    Ok(())
}

fn enable_network_controls(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
) -> CommandResult<()> {
    if !session.blocked_url_patterns.is_empty() || !session.mocks.is_empty() {
        let _ = client.command(session, "Network.enable", json!({}), CDP_RESPONSE_TIMEOUT)?;
    }
    if !session.blocked_url_patterns.is_empty() {
        let urls = session
            .blocked_url_patterns
            .iter()
            .map(|pattern| format!("*{pattern}*"))
            .collect::<Vec<_>>();
        let _ = client.command(
            session,
            "Network.setBlockedURLs",
            json!({ "urls": urls }),
            CDP_RESPONSE_TIMEOUT,
        )?;
    }
    if !session.mocks.is_empty() {
        let _ = client.command(
            session,
            "Fetch.enable",
            json!({ "patterns": [{ "urlPattern": "*" }] }),
            CDP_RESPONSE_TIMEOUT,
        )?;
    }
    Ok(())
}

fn wait_for_load_with_client(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    timeout: Duration,
) -> CommandResult<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let result = runtime_evaluate(
            client,
            session,
            "({ readyState: document.readyState, url: location.href, title: document.title })",
            CDP_RESPONSE_TIMEOUT,
        )?;
        session.update_active_page_from_state(&result);
        if result.get("readyState").and_then(JsonValue::as_str) == Some("complete") {
            return Ok(());
        }
        client.drain_events(session, Duration::from_millis(100));
    }
    Err(CommandError::retryable(
        "browser_native_load_timeout",
        format!(
            "Native CDP page did not reach document.readyState=complete within {} ms.",
            timeout.as_millis()
        ),
    ))
}

fn fetch_or_create_page(endpoint: &str, url: Option<&str>) -> CommandResult<Option<NativeCdpPage>> {
    let pages = fetch_pages(endpoint)?;
    if let Some(page) = pages
        .into_iter()
        .find(|page| !page.websocket_url.is_empty())
    {
        return Ok(Some(page));
    }
    create_page(endpoint, url.unwrap_or("about:blank")).map(Some)
}

fn fetch_pages(endpoint: &str) -> CommandResult<Vec<NativeCdpPage>> {
    let targets = http_get_json::<Vec<CdpTarget>>(endpoint, "/json/list")?;
    Ok(targets
        .into_iter()
        .filter(|target| target.target_type == "page")
        .filter_map(|target| {
            target
                .web_socket_debugger_url
                .map(|websocket_url| NativeCdpPage {
                    target_id: target.id,
                    title: target.title,
                    url: target.url,
                    websocket_url,
                })
        })
        .collect())
}

fn create_page(endpoint: &str, url: &str) -> CommandResult<NativeCdpPage> {
    let encoded = url::form_urlencoded::byte_serialize(url.as_bytes()).collect::<String>();
    let target = http_request_json::<CdpTarget>(endpoint, &format!("/json/new?{encoded}"), true)
        .or_else(|_| {
            http_request_json::<CdpTarget>(endpoint, &format!("/json/new?{encoded}"), false)
        })?;
    let websocket_url = target.web_socket_debugger_url.ok_or_else(|| {
        CommandError::system_fault(
            "browser_native_target_invalid",
            "Native CDP target creation did not return a page WebSocket URL.",
        )
    })?;
    Ok(NativeCdpPage {
        target_id: target.id,
        title: target.title,
        url: target.url,
        websocket_url,
    })
}

fn wait_for_cdp_endpoint(endpoint: &str, timeout: Duration) -> CommandResult<CdpVersion> {
    let started = Instant::now();
    let mut last_error = None;
    while started.elapsed() < timeout {
        match http_get_json::<CdpVersion>(endpoint, "/json/version") {
            Ok(version) => return Ok(version),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(150));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        CommandError::retryable(
            "browser_native_endpoint_unavailable",
            format!(
                "Native CDP endpoint `{}` was not ready within {} ms.",
                redact_cdp_endpoint(endpoint),
                timeout.as_millis()
            ),
        )
    }))
}

fn http_get_json<T: for<'de> Deserialize<'de>>(endpoint: &str, path: &str) -> CommandResult<T> {
    http_request_json(endpoint, path, false)
}

fn http_request_json<T: for<'de> Deserialize<'de>>(
    endpoint: &str,
    path: &str,
    put: bool,
) -> CommandResult<T> {
    let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
    let client = reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "browser_native_http_client_failed",
                format!("Xero could not build native CDP HTTP client: {error}"),
            )
        })?;
    let request = if put {
        client.put(&url)
    } else {
        client.get(&url)
    };
    let response = request.send().map_err(|error| {
        CommandError::retryable(
            "browser_native_http_failed",
            format!(
                "Xero could not reach native CDP endpoint `{}`: {error}",
                redact_cdp_endpoint(endpoint)
            ),
        )
    })?;
    if !response.status().is_success() {
        return Err(CommandError::retryable(
            "browser_native_http_status",
            format!(
                "Native CDP endpoint `{}` returned HTTP status {}.",
                redact_cdp_endpoint(endpoint),
                response.status()
            ),
        ));
    }
    response.json::<T>().map_err(|error| {
        CommandError::system_fault(
            "browser_native_http_decode_failed",
            format!(
                "Xero could not decode native CDP response from `{}`: {error}",
                redact_cdp_endpoint(endpoint)
            ),
        )
    })
}

pub fn discover_chromium_browsers() -> Vec<BrowserBinaryCandidate> {
    let mut candidates = Vec::new();
    push_browser_path(
        &mut candidates,
        "chrome",
        "Google Chrome",
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        "macos_application",
    );
    push_browser_path(
        &mut candidates,
        "chromium",
        "Chromium",
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        "macos_application",
    );
    push_browser_path(
        &mut candidates,
        "edge",
        "Microsoft Edge",
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        "macos_application",
    );
    push_browser_path(
        &mut candidates,
        "brave",
        "Brave Browser",
        PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
        "macos_application",
    );
    push_browser_path(
        &mut candidates,
        "arc",
        "Arc",
        PathBuf::from("/Applications/Arc.app/Contents/MacOS/Arc"),
        "macos_application",
    );
    if let Some(home) = dirs::home_dir() {
        push_browser_path(
            &mut candidates,
            "chrome-user",
            "Google Chrome",
            home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            "macos_user_application",
        );
        push_browser_path(
            &mut candidates,
            "chromium-user",
            "Chromium",
            home.join("Applications/Chromium.app/Contents/MacOS/Chromium"),
            "macos_user_application",
        );
    }
    for (id, name, command) in [
        ("google-chrome", "Google Chrome", "google-chrome"),
        (
            "google-chrome-stable",
            "Google Chrome",
            "google-chrome-stable",
        ),
        ("chromium", "Chromium", "chromium"),
        ("chromium-browser", "Chromium", "chromium-browser"),
        ("microsoft-edge", "Microsoft Edge", "microsoft-edge"),
        ("brave-browser", "Brave Browser", "brave-browser"),
        ("chrome-win", "Google Chrome", "chrome.exe"),
        ("msedge-win", "Microsoft Edge", "msedge.exe"),
    ] {
        if let Some(path) = find_on_path(command) {
            push_browser_path(&mut candidates, id, name, path, "path");
        }
    }

    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.path.clone()))
        .collect()
}

fn push_browser_path(
    candidates: &mut Vec<BrowserBinaryCandidate>,
    id: &str,
    name: &str,
    path: PathBuf,
    source: &str,
) {
    if path.is_file() {
        candidates.push(BrowserBinaryCandidate {
            id: id.into(),
            name: name.into(),
            path,
            source: source.into(),
        });
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|path| path.join(binary))
        .find(|path| path.is_file())
}

fn native_root(repo_root: &Path) -> PathBuf {
    project_app_data_dir_for_repo(repo_root).join("browser-automation/native-cdp")
}

fn random_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn persist_session_metadata(session: &NativeCdpSession) -> CommandResult<()> {
    let dir = session
        .profile_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| session.profile_dir.clone())
        .join("sessions")
        .join(&session.session_id);
    fs::create_dir_all(&dir).map_err(|error| {
        CommandError::retryable(
            "browser_native_session_dir_failed",
            format!(
                "Xero could not prepare native CDP session metadata at {}: {error}",
                dir.display()
            ),
        )
    })?;
    let path = dir.join("session.json");
    let bytes = serde_json::to_vec_pretty(&session.metadata()).map_err(|error| {
        CommandError::system_fault(
            "browser_native_session_encode_failed",
            format!("Xero could not encode native CDP session metadata: {error}"),
        )
    })?;
    fs::write(&path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_native_session_write_failed",
            format!(
                "Xero could not write native CDP session metadata at {}: {error}",
                path.display()
            ),
        )
    })
}

fn write_json_artifact(
    artifact_root: &Path,
    family: &str,
    prefix: &str,
    payload: &JsonValue,
) -> CommandResult<PathBuf> {
    let dir = artifact_root.join(family);
    fs::create_dir_all(&dir).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_dir_failed",
            format!(
                "Xero could not prepare native CDP artifact directory at {}: {error}",
                dir.display()
            ),
        )
    })?;
    let path = dir.join(format!(
        "{prefix}-{}.json",
        now_timestamp().replace([':', '.'], "-")
    ));
    let (redacted, _changed) = redact_json_for_persistence(payload);
    let bytes = serde_json::to_vec_pretty(&redacted).map_err(|error| {
        CommandError::system_fault(
            "browser_native_artifact_encode_failed",
            format!("Xero could not encode native CDP artifact JSON: {error}"),
        )
    })?;
    fs::write(&path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_write_failed",
            format!(
                "Xero could not write native CDP artifact at {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(path)
}

fn write_base64_artifact(
    artifact_root: &Path,
    family: &str,
    prefix: &str,
    extension: &str,
    base64_data: &str,
) -> CommandResult<PathBuf> {
    let dir = artifact_root.join(family);
    fs::create_dir_all(&dir).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_dir_failed",
            format!(
                "Xero could not prepare native CDP artifact directory at {}: {error}",
                dir.display()
            ),
        )
    })?;
    let path = dir.join(format!(
        "{prefix}-{}.{}",
        now_timestamp().replace([':', '.'], "-"),
        extension
    ));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|error| {
            CommandError::system_fault(
                "browser_native_artifact_decode_failed",
                format!("Xero could not decode native CDP base64 artifact: {error}"),
            )
        })?;
    fs::write(&path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_write_failed",
            format!(
                "Xero could not write native CDP artifact at {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(path)
}

fn write_text_artifact(
    artifact_root: &Path,
    family: &str,
    prefix: &str,
    extension: &str,
    text: &str,
) -> CommandResult<PathBuf> {
    let dir = artifact_root.join(family);
    fs::create_dir_all(&dir).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_dir_failed",
            format!(
                "Xero could not prepare native CDP artifact directory at {}: {error}",
                dir.display()
            ),
        )
    })?;
    let path = dir.join(format!(
        "{prefix}-{}.{}",
        now_timestamp().replace([':', '.'], "-"),
        extension
    ));
    fs::write(&path, text).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_write_failed",
            format!(
                "Xero could not write native CDP artifact at {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(path)
}

fn write_base64_to_path(path: &Path, base64_data: &str) -> CommandResult<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|error| {
            CommandError::system_fault(
                "browser_native_artifact_decode_failed",
                format!("Xero could not decode native CDP base64 artifact: {error}"),
            )
        })?;
    fs::write(path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_write_failed",
            format!(
                "Xero could not write native CDP artifact at {}: {error}",
                path.display()
            ),
        )
    })
}

fn write_json_to_path(path: &Path, payload: &JsonValue) -> CommandResult<()> {
    let (redacted, _changed) = redact_json_for_persistence(payload);
    let bytes = serde_json::to_vec_pretty(&redacted).map_err(|error| {
        CommandError::system_fault(
            "browser_native_artifact_encode_failed",
            format!("Xero could not encode native CDP artifact JSON: {error}"),
        )
    })?;
    fs::write(path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_native_artifact_write_failed",
            format!(
                "Xero could not write native CDP artifact at {}: {error}",
                path.display()
            ),
        )
    })
}

fn safe_artifact_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if sanitized.is_empty() {
        "default".into()
    } else {
        sanitized
    }
}

fn default_trace_categories() -> Vec<String> {
    vec![
        "devtools.timeline".into(),
        "disabled-by-default-devtools.timeline".into(),
        "blink.user_timing".into(),
        "loading".into(),
    ]
}

fn read_cdp_stream_to_string(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    stream: &str,
) -> CommandResult<String> {
    let mut output = String::new();
    loop {
        let chunk = client.command(
            session,
            "IO.read",
            json!({ "handle": stream }),
            CDP_RESPONSE_TIMEOUT,
        )?;
        if let Some(data) = chunk.get("data").and_then(JsonValue::as_str) {
            if chunk.get("base64Encoded").and_then(JsonValue::as_bool) == Some(true) {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|error| {
                        CommandError::system_fault(
                            "browser_native_trace_decode_failed",
                            format!("Xero could not decode native CDP trace stream: {error}"),
                        )
                    })?;
                output.push_str(&String::from_utf8_lossy(&bytes));
            } else {
                output.push_str(data);
            }
        }
        if chunk.get("eof").and_then(JsonValue::as_bool) == Some(true) {
            break;
        }
    }
    let _ = client.command(
        session,
        "IO.close",
        json!({ "handle": stream }),
        CDP_RESPONSE_TIMEOUT,
    );
    Ok(output)
}

fn capture_visual_base64(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    selector: Option<&str>,
    full_page: bool,
) -> CommandResult<(String, JsonValue)> {
    let mut params = if full_page {
        json!({ "format": "png", "captureBeyondViewport": true, "fromSurface": true })
    } else {
        json!({ "format": "png", "fromSurface": true })
    };
    let mut metadata = json!({ "fullPage": full_page });
    if let Some(selector) = selector {
        let bounds = selector_point(client, session, selector)?;
        let bounds_obj = bounds.raw.get("bounds").cloned().unwrap_or(JsonValue::Null);
        let clip = json!({
            "x": bounds_obj.get("x").and_then(JsonValue::as_f64).unwrap_or(bounds.x),
            "y": bounds_obj.get("y").and_then(JsonValue::as_f64).unwrap_or(bounds.y),
            "width": bounds_obj.get("width").and_then(JsonValue::as_f64).unwrap_or(1.0).max(1.0),
            "height": bounds_obj.get("height").and_then(JsonValue::as_f64).unwrap_or(1.0).max(1.0),
            "scale": 1.0,
        });
        params["clip"] = clip.clone();
        metadata["selector"] = json!(selector);
        metadata["clip"] = clip;
    }
    let result = client.command(
        session,
        "Page.captureScreenshot",
        params,
        CDP_RESPONSE_TIMEOUT,
    )?;
    let base64 = result
        .get("data")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| {
            CommandError::system_fault(
                "browser_native_visual_screenshot_invalid",
                "Native CDP screenshot response did not include base64 image data.",
            )
        })?
        .to_owned();
    Ok((base64, metadata))
}

struct VisualDiffResult {
    pixel_count: u64,
    different_pixels: u64,
    percent_difference: f64,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
}

fn visual_diff_bytes(
    baseline_bytes: &[u8],
    current_bytes: &[u8],
) -> CommandResult<VisualDiffResult> {
    let baseline = image::load_from_memory(baseline_bytes).map_err(|error| {
        CommandError::system_fault(
            "browser_native_visual_baseline_decode_failed",
            format!("Xero could not decode visual baseline image: {error}"),
        )
    })?;
    let current = image::load_from_memory(current_bytes).map_err(|error| {
        CommandError::system_fault(
            "browser_native_visual_current_decode_failed",
            format!("Xero could not decode current visual image: {error}"),
        )
    })?;
    let baseline = baseline.to_rgba8();
    let current = current.to_rgba8();
    let width = baseline.width().min(current.width());
    let height = baseline.height().min(current.height());
    let mut image = ImageBuffer::from_pixel(width.max(1), height.max(1), Rgba([0, 0, 0, 0]));
    let mut different_pixels = 0u64;
    for y in 0..height {
        for x in 0..width {
            let left = baseline.get_pixel(x, y);
            let right = current.get_pixel(x, y);
            if left != right {
                different_pixels = different_pixels.saturating_add(1);
                image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            } else {
                let dim = [
                    (right[0] as f32 * 0.35) as u8,
                    (right[1] as f32 * 0.35) as u8,
                    (right[2] as f32 * 0.35) as u8,
                    255,
                ];
                image.put_pixel(x, y, Rgba(dim));
            }
        }
    }
    let pixel_count = (width as u64).saturating_mul(height as u64);
    let dimension_delta =
        baseline.width() != current.width() || baseline.height() != current.height();
    if dimension_delta {
        different_pixels = different_pixels.saturating_add(
            (baseline.width() as i64 - current.width() as i64)
                .unsigned_abs()
                .saturating_mul(height as u64)
                .saturating_add(
                    (baseline.height() as i64 - current.height() as i64)
                        .unsigned_abs()
                        .saturating_mul(width as u64),
                ),
        );
    }
    let percent_difference = if pixel_count == 0 {
        100.0
    } else {
        (different_pixels as f64 / pixel_count as f64) * 100.0
    };
    Ok(VisualDiffResult {
        pixel_count,
        different_pixels,
        percent_difference,
        image,
    })
}

fn apply_emulation_state(
    client: &mut CdpClient,
    session: &mut NativeCdpSession,
    state: &JsonValue,
) -> CommandResult<()> {
    let viewport = state.get("viewport").unwrap_or(state);
    let width = viewport
        .get("width")
        .and_then(JsonValue::as_u64)
        .unwrap_or(1280)
        .max(1);
    let height = viewport
        .get("height")
        .and_then(JsonValue::as_u64)
        .unwrap_or(720)
        .max(1);
    let device_scale_factor = viewport
        .get("deviceScaleFactor")
        .or_else(|| viewport.get("device_scale_factor"))
        .and_then(JsonValue::as_f64)
        .unwrap_or(1.0)
        .max(0.1);
    let mobile = viewport
        .get("mobile")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    client.command(
        session,
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": width,
            "height": height,
            "deviceScaleFactor": device_scale_factor,
            "mobile": mobile,
        }),
        CDP_RESPONSE_TIMEOUT,
    )?;
    if viewport
        .get("touch")
        .or_else(|| state.get("touch"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        let _ = client.command(
            session,
            "Emulation.setTouchEmulationEnabled",
            json!({ "enabled": true, "configuration": "mobile" }),
            CDP_RESPONSE_TIMEOUT,
        );
    }
    if let Some(user_agent) = state.get("userAgent").and_then(JsonValue::as_str) {
        let _ = client.command(
            session,
            "Network.setUserAgentOverride",
            json!({ "userAgent": user_agent }),
            CDP_RESPONSE_TIMEOUT,
        );
    }
    if let Some(timezone) = state.get("timezone").and_then(JsonValue::as_str) {
        let _ = client.command(
            session,
            "Emulation.setTimezoneOverride",
            json!({ "timezoneId": timezone }),
            CDP_RESPONSE_TIMEOUT,
        );
    }
    if let Some(locale) = state.get("locale").and_then(JsonValue::as_str) {
        let _ = client.command(
            session,
            "Emulation.setLocaleOverride",
            json!({ "locale": locale }),
            CDP_RESPONSE_TIMEOUT,
        );
    }
    let mut features = Vec::new();
    if let Some(color_scheme) = state.get("colorScheme").and_then(JsonValue::as_str) {
        features.push(json!({ "name": "prefers-color-scheme", "value": color_scheme }));
    }
    if let Some(reduced_motion) = state.get("reducedMotion").and_then(JsonValue::as_str) {
        features.push(json!({ "name": "prefers-reduced-motion", "value": reduced_motion }));
    }
    if !features.is_empty() {
        let _ = client.command(
            session,
            "Emulation.setEmulatedMedia",
            json!({ "features": features }),
            CDP_RESPONSE_TIMEOUT,
        );
    }
    Ok(())
}

fn device_preset_state(preset: &str) -> CommandResult<JsonValue> {
    match preset {
        "iphone_14" | "iphone" => Ok(json!({
            "viewport": { "width": 390, "height": 844, "deviceScaleFactor": 3.0, "mobile": true, "touch": true },
            "userAgent": "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1"
        })),
        "pixel_7" | "android" => Ok(json!({
            "viewport": { "width": 412, "height": 915, "deviceScaleFactor": 2.625, "mobile": true, "touch": true },
            "userAgent": "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
        })),
        "ipad" => Ok(json!({
            "viewport": { "width": 820, "height": 1180, "deviceScaleFactor": 2.0, "mobile": true, "touch": true },
            "userAgent": "Mozilla/5.0 (iPad; CPU OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1"
        })),
        "desktop_1080p" | "desktop" => Ok(json!({
            "viewport": { "width": 1920, "height": 1080, "deviceScaleFactor": 1.0, "mobile": false, "touch": false }
        })),
        other => Err(CommandError::user_fixable(
            "browser_native_device_preset_unknown",
            format!("Unknown native CDP device preset `{other}`."),
        )),
    }
}

fn merge_json_objects(base: &mut JsonValue, overlay: &mut JsonValue) {
    let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object_mut()) else {
        return;
    };
    for (key, value) in std::mem::take(overlay) {
        match (base.get_mut(&key), value) {
            (Some(existing), mut value @ JsonValue::Object(_)) if existing.is_object() => {
                merge_json_objects(existing, &mut value);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn flatten_frame_tree(tree: &JsonValue) -> Vec<NativeFrameSelection> {
    fn walk(node: &JsonValue, out: &mut Vec<NativeFrameSelection>) {
        if let Some(frame) = node.get("frame") {
            if let Some(frame_id) = frame.get("id").and_then(JsonValue::as_str) {
                let parent_frame_id = frame
                    .get("parentId")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned);
                let url = frame
                    .get("url")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned);
                let cross_origin = parent_frame_id.is_some()
                    && url
                        .as_deref()
                        .is_some_and(|url| !url.starts_with("about:") && !url.is_empty());
                out.push(NativeFrameSelection {
                    frame_id: frame_id.to_owned(),
                    parent_frame_id,
                    name: frame.get("name").and_then(JsonValue::as_str).map(str::to_owned),
                    url,
                    selected_at: now_timestamp(),
                    limitation: cross_origin.then(|| {
                        "Frame-scoped DOM actions may require Chrome target/session routing for cross-origin content.".into()
                    }),
                });
            }
        }
        if let Some(children) = node.get("childFrames").and_then(JsonValue::as_array) {
            for child in children {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Some(root) = tree.get("frameTree") {
        walk(root, &mut out);
    }
    out
}

fn native_extract_expression(
    mode: &str,
    selector: Option<&str>,
    selector_map: Option<BTreeMap<String, String>>,
    limit: usize,
) -> CommandResult<String> {
    let mode_json = js_string(mode)?;
    let selector_json = optional_js_string(selector)?;
    let selector_map_json =
        serde_json::to_string(&selector_map.unwrap_or_default()).map_err(|error| {
            CommandError::system_fault(
                "browser_native_extract_encode_failed",
                format!("Xero could not encode extract selector map: {error}"),
            )
        })?;
    let limit = limit.clamp(1, 500);
    Ok(format!(
        r#"(() => {{
            const mode = {mode_json};
            const selector = {selector_json};
            const selectorMap = {selector_map_json};
            const limit = {limit};
            const root = selector ? document.querySelector(selector) : document;
            if (!root) throw new Error('extract root not found: ' + selector);
            const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ');
            const attr = (el, name) => el.getAttribute && el.getAttribute(name);
            const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
            const bounded = (items) => items.slice(0, limit);
            if (mode === 'summary' || mode === 'page_summary') return {{
              url: location.href, title: document.title, textPreview: textOf(root === document ? document.body : root).slice(0, 4000)
            }};
            if (mode === 'headings') return bounded(Array.from(root.querySelectorAll('h1,h2,h3,h4,h5,h6,[role="heading"]')).map((el) => ({{
              level: Number((el.tagName || '').slice(1)) || Number(attr(el, 'aria-level')) || null, text: textOf(el).slice(0, 500)
            }})));
            if (mode === 'links') return bounded(Array.from(root.querySelectorAll('a[href]')).map((el) => ({{
              text: textOf(el).slice(0, 500), href: el.href, rel: attr(el, 'rel'), target: attr(el, 'target')
            }})));
            if (mode === 'tables') return bounded(Array.from(root.querySelectorAll('table')).map((table) => ({{
              caption: textOf(table.querySelector('caption') || {{}}).slice(0, 300),
              headers: Array.from(table.querySelectorAll('thead th, tr:first-child th')).map((cell) => textOf(cell).slice(0, 200)),
              rows: Array.from(table.querySelectorAll('tr')).slice(0, limit).map((row) => Array.from(row.children).map((cell) => textOf(cell).slice(0, 500)))
            }})));
            if (mode === 'forms') return bounded(Array.from(root.querySelectorAll('form,input,textarea,select,button')).map((el) => ({{
              tag: (el.tagName || '').toLowerCase(), type: attr(el, 'type'), name: attr(el, 'name'), id: el.id || null,
              label: attr(el, 'aria-label') || attr(el, 'placeholder') || textOf(el).slice(0, 300),
              required: !!el.required || attr(el, 'aria-required') === 'true'
            }})));
            if (mode === 'metadata') return {{
              title: document.title,
              canonical: document.querySelector('link[rel="canonical"]')?.href || null,
              meta: bounded(Array.from(document.querySelectorAll('meta')).map((el) => ({{ name: attr(el, 'name') || attr(el, 'property'), content: attr(el, 'content') }})))
            }};
            if (mode === 'json_ld' || mode === 'json-ld') return bounded(Array.from(document.querySelectorAll('script[type="application/ld+json"]')).map((el) => {{
              try {{ return JSON.parse(el.textContent || 'null'); }} catch (_) {{ return {{ parseError: true, text: (el.textContent || '').slice(0, 1000) }}; }}
            }}));
            if (mode === 'selector_map') {{
              const out = {{}};
              for (const [key, css] of Object.entries(selectorMap || {{}})) {{
                out[key] = bounded(Array.from(document.querySelectorAll(css)).map((el) => ({{ text: textOf(el).slice(0, 1000), value: el.value || null, href: el.href || null }})));
              }}
              return out;
            }}
            if (mode === 'visible_text_blocks') return bounded(Array.from(root.querySelectorAll('p,li,article,section,main,div')).filter(visible).map((el) => textOf(el)).filter(Boolean).map((text) => text.slice(0, 1000)));
            throw new Error('unsupported extract mode: ' + mode);
        }})()"#
    ))
}

fn active_session_mut<'a>(
    sessions: &'a mut BTreeMap<String, NativeCdpSession>,
    session_id: Option<&str>,
) -> CommandResult<&'a mut NativeCdpSession> {
    let wanted = session_id.unwrap_or(DEFAULT_SESSION_ID);
    if sessions.contains_key(wanted) {
        return sessions
            .get_mut(wanted)
            .ok_or_else(|| missing_session_error(wanted));
    }
    if session_id.is_none() && sessions.len() == 1 {
        return sessions
            .values_mut()
            .next()
            .ok_or_else(|| missing_session_error(wanted));
    }
    Err(missing_session_error(wanted))
}

fn missing_session_error(session_id: &str) -> CommandError {
    CommandError::user_fixable(
        "browser_native_session_missing",
        format!("Native CDP session `{session_id}` does not exist. Launch or attach a native session first."),
    )
}

fn lock_error(code: &'static str) -> CommandError {
    CommandError::system_fault(code, "Native CDP browser service lock poisoned.")
}

fn normalize_session_id<F: FnOnce() -> String>(session_id: Option<String>, fallback: F) -> String {
    let raw = session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(fallback);
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
        .if_empty(DEFAULT_SESSION_ID)
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}

fn normalize_endpoint(endpoint: &str, allow_remote_endpoint: bool) -> CommandResult<String> {
    let endpoint = endpoint.trim();
    let endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_owned()
    } else if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        endpoint.to_owned()
    } else {
        format!("http://{endpoint}")
    };
    let parsed = Url::parse(&endpoint).map_err(|error| {
        CommandError::user_fixable(
            "browser_native_endpoint_invalid",
            format!("Native CDP endpoint `{endpoint}` is invalid: {error}"),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
        return Err(CommandError::user_fixable(
            "browser_native_endpoint_invalid",
            "Native CDP attach endpoint must use http://, https://, ws://, or wss://.",
        ));
    }
    if !allow_remote_endpoint && !url_is_loopback(&parsed) {
        return Err(CommandError::policy_denied(
            "Native CDP attach endpoints must be loopback by default. Use `allowRemoteEndpoint: true` only with explicit operator approval for a remote CDP endpoint.",
        ));
    }
    Ok(endpoint.trim_end_matches('/').to_owned())
}

fn url_is_loopback(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

fn redact_cdp_endpoint(endpoint: &str) -> String {
    let Ok(url) = Url::parse(endpoint) else {
        return "<redacted-cdp-endpoint>".into();
    };
    let scheme = url.scheme();
    let host = if url_is_loopback(&url) {
        "loopback"
    } else {
        "remote"
    };
    let port = url.port().map(|_| ":<redacted-port>").unwrap_or_default();
    format!("{scheme}://{host}{port}/<redacted>")
}

fn cdp_endpoint_kind(endpoint: &str) -> &'static str {
    let Ok(url) = Url::parse(endpoint) else {
        return "unknown";
    };
    match (url.scheme(), url_is_loopback(&url)) {
        ("http" | "https", true) => "loopback_http",
        ("ws" | "wss", true) => "loopback_websocket",
        ("http" | "https", false) => "remote_http",
        ("ws" | "wss", false) => "remote_websocket",
        _ => "unknown",
    }
}

fn parse_http_port(endpoint: &str) -> Option<u16> {
    Url::parse(endpoint).ok().and_then(|url| url.port())
}

fn parse_ws_port(endpoint: &str) -> Option<u16> {
    Url::parse(endpoint).ok().and_then(|url| url.port())
}

fn page_from_ws_url(ws_url: &str) -> NativeCdpPage {
    NativeCdpPage {
        target_id: Url::parse(ws_url)
            .ok()
            .and_then(|url| {
                url.path_segments()
                    .and_then(|segments| segments.last().map(str::to_owned))
            })
            .unwrap_or_else(|| "attached".into()),
        title: "Attached CDP page".into(),
        url: String::new(),
        websocket_url: ws_url.to_owned(),
    }
}

fn choose_free_port() -> CommandResult<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| {
        CommandError::system_fault(
            "browser_native_port_bind_failed",
            format!("Xero could not reserve a local native CDP port: {error}"),
        )
    })?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| {
            CommandError::system_fault(
                "browser_native_port_bind_failed",
                format!("Xero could not inspect reserved native CDP port: {error}"),
            )
        })
}

fn current_url_from_session(session: &NativeCdpSession) -> Option<String> {
    session
        .active_page
        .as_ref()
        .map(|page| page.url.clone())
        .filter(|url| !url.is_empty())
}

fn js_string(value: &str) -> CommandResult<String> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            "browser_native_script_encode_failed",
            format!("Xero could not encode native CDP script value: {error}"),
        )
    })
}

fn optional_js_string(value: Option<&str>) -> CommandResult<String> {
    match value {
        Some(value) => js_string(value),
        None => Ok("null".into()),
    }
}

fn key_event_payloads(key: &str) -> (JsonValue, JsonValue) {
    let (key_name, code, windows_code, text) = match key {
        "Enter" | "Return" => ("Enter", "Enter", 13, "\r"),
        "Tab" => ("Tab", "Tab", 9, "\t"),
        "Escape" | "Esc" => ("Escape", "Escape", 27, ""),
        "Backspace" => ("Backspace", "Backspace", 8, ""),
        "Delete" => ("Delete", "Delete", 46, ""),
        "ArrowLeft" => ("ArrowLeft", "ArrowLeft", 37, ""),
        "ArrowUp" => ("ArrowUp", "ArrowUp", 38, ""),
        "ArrowRight" => ("ArrowRight", "ArrowRight", 39, ""),
        "ArrowDown" => ("ArrowDown", "ArrowDown", 40, ""),
        value if value.chars().count() == 1 => (value, value, value.as_bytes()[0] as i64, value),
        value => (value, value, 0, ""),
    };
    let down = json!({
        "type": "keyDown",
        "key": key_name,
        "code": code,
        "windowsVirtualKeyCode": windows_code,
        "nativeVirtualKeyCode": windows_code,
        "text": text,
        "unmodifiedText": text,
    });
    let up = json!({
        "type": "keyUp",
        "key": key_name,
        "code": code,
        "windowsVirtualKeyCode": windows_code,
        "nativeVirtualKeyCode": windows_code,
    });
    (down, up)
}

fn native_wait_expression(
    condition: &str,
    selector: Option<&str>,
    text: Option<&str>,
    url_contains: Option<&str>,
    title_contains: Option<&str>,
    count: usize,
) -> CommandResult<String> {
    let condition_json = js_string(condition)?;
    let selector_json = optional_js_string(selector)?;
    let text_json = optional_js_string(text)?;
    let url_json = optional_js_string(url_contains)?;
    let title_json = optional_js_string(title_contains)?;
    Ok(format!(
        r#"(() => {{
            const condition = {condition_json};
            const selector = {selector_json};
            const text = {text_json};
            const urlContains = {url_json};
            const titleContains = {title_json};
            const expectedCount = {count};
            const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
            const pageText = () => (document.body && (document.body.innerText || document.body.textContent) || '').trim();
            if (condition === 'load') return {{ ok: document.readyState === 'complete', detail: {{ readyState: document.readyState }} }};
            if (condition === 'selector_visible') {{ const el = document.querySelector(selector || ''); return {{ ok: !!(el && visible(el)), detail: {{ selector, found: !!el, visible: !!(el && visible(el)) }} }}; }}
            if (condition === 'selector_hidden') {{ const el = document.querySelector(selector || ''); return {{ ok: !el || !visible(el), detail: {{ selector, found: !!el, visible: !!(el && visible(el)) }} }}; }}
            if (condition === 'text_visible') {{ const matched = !!(text && pageText().includes(text)); return {{ ok: matched, detail: {{ text, matched }} }}; }}
            if (condition === 'text_hidden') {{ const matched = !!(text && pageText().includes(text)); return {{ ok: !matched, detail: {{ text, matched }} }}; }}
            if (condition === 'url_contains') return {{ ok: !!(urlContains && location.href.includes(urlContains)), detail: {{ url: location.href, urlContains }} }};
            if (condition === 'title_contains') return {{ ok: !!(titleContains && document.title.includes(titleContains)), detail: {{ title: document.title, titleContains }} }};
            if (condition === 'element_count') {{ const actual = document.querySelectorAll(selector || '').length; return {{ ok: actual === expectedCount, detail: {{ selector, expectedCount, actual }} }}; }}
            if (condition === 'element_count_at_least') {{ const actual = document.querySelectorAll(selector || '').length; return {{ ok: actual >= expectedCount, detail: {{ selector, expectedCount, actual }} }}; }}
            if (condition === 'region_stable') return {{ ok: true, detail: {{ supportedBy: 'native_cdp_dom_sample' }} }};
            return {{ ok: false, detail: {{ unsupportedCondition: condition }} }};
        }})()"#
    ))
}

fn native_assert_expression(
    assertion: &str,
    selector: Option<&str>,
    expected: Option<&str>,
) -> CommandResult<String> {
    let assertion_json = js_string(assertion)?;
    let selector_json = optional_js_string(selector)?;
    let expected_json = optional_js_string(expected)?;
    Ok(format!(
        r#"(() => {{
            const assertion = {assertion_json};
            const selector = {selector_json};
            const expected = {expected_json};
            const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
            const pageText = () => (document.body && (document.body.innerText || document.body.textContent) || '').trim();
            const selected = () => selector ? document.querySelector(selector) : null;
            if (assertion === 'url') return {{ assertion, pass: expected != null && location.href === expected, actual: location.href, expected }};
            if (assertion === 'url_contains') return {{ assertion, pass: expected != null && location.href.includes(expected), actual: location.href, expected }};
            if (assertion === 'title') return {{ assertion, pass: expected != null && document.title === expected, actual: document.title, expected }};
            if (assertion === 'title_contains') return {{ assertion, pass: expected != null && document.title.includes(expected), actual: document.title, expected }};
            if (assertion === 'text') return {{ assertion, pass: expected != null && pageText().includes(expected), actual: pageText().slice(0, 1000), expected }};
            if (assertion === 'selector') {{ const el = selected(); return {{ assertion, pass: !!el, actual: !!el, expected: true, selector }}; }}
            if (assertion === 'selector_visible') {{ const el = selected(); return {{ assertion, pass: !!(el && visible(el)), actual: {{ found: !!el, visible: !!(el && visible(el)) }}, expected: true, selector }}; }}
            if (assertion === 'value') {{ const el = selected(); return {{ assertion, pass: !!el && String(el.value || '') === String(expected || ''), actual: el ? String(el.value || '') : null, expected, selector }}; }}
            if (assertion === 'checked') {{ const el = selected(); const expectedBool = expected === true || expected === 'true'; return {{ assertion, pass: !!el && Boolean(el.checked) === expectedBool, actual: el ? Boolean(el.checked) : null, expected: expectedBool, selector }}; }}
            if (assertion === 'element_count') {{ const actual = document.querySelectorAll(selector || '').length; const expectedNumber = Number(expected); return {{ assertion, pass: actual === expectedNumber, actual, expected: expectedNumber, selector }}; }}
            return {{ assertion, pass: false, unsupportedAssertion: assertion, expected }};
        }})()"#
    ))
}

fn native_snapshot_expression(mode: &str, visible_only: bool, limit: usize) -> String {
    let mode_json = serde_json::to_string(mode).unwrap_or_else(|_| "\"interactive\"".into());
    let visible = if visible_only { "true" } else { "false" };
    let limit = limit.clamp(1, 400);
    format!(
        r#"(() => {{
            const mode = {mode_json};
            const visibleOnly = {visible};
            const limit = {limit};
            const escapeCss = (value) => {{
              if (window.CSS && typeof window.CSS.escape === 'function') return window.CSS.escape(String(value));
              return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => '\\' + ch);
            }};
            const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 500);
            const attr = (el, name) => el.getAttribute && el.getAttribute(name);
            const implicitRole = (el) => {{
              const tag = (el.tagName || '').toLowerCase();
              if (tag === 'a' && el.hasAttribute('href')) return 'link';
              if (tag === 'button' || tag === 'summary') return 'button';
              if (tag === 'input') {{
                if (el.type === 'checkbox') return 'checkbox';
                if (el.type === 'radio') return 'radio';
                if (['button', 'submit', 'reset'].includes(el.type)) return 'button';
                return 'textbox';
              }}
              if (tag === 'textarea') return 'textbox';
              if (tag === 'select') return 'combobox';
              if (/^h[1-6]$/.test(tag)) return 'heading';
              if (tag === 'nav') return 'navigation';
              if (tag === 'main') return 'main';
              if (tag === 'form') return 'form';
              if (tag === 'dialog') return 'dialog';
              return null;
            }};
            const nameOf = (el) => {{
              const labelledBy = attr(el, 'aria-labelledby');
              if (labelledBy) {{
                const label = labelledBy.split(/\s+/).map((id) => document.getElementById(id)).filter(Boolean).map(textOf).join(' ').trim();
                if (label) return label.slice(0, 300);
              }}
              const id = attr(el, 'id');
              if (id) {{
                const label = document.querySelector(`label[for="${{escapeCss(id)}}"]`);
                if (label && textOf(label)) return textOf(label).slice(0, 300);
              }}
              return (attr(el, 'aria-label') || attr(el, 'alt') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el)).slice(0, 300);
            }};
            const isVisible = (el) => {{
              if (!el || el.nodeType !== 1) return false;
              const style = window.getComputedStyle ? window.getComputedStyle(el) : null;
              if (style && (style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0)) return false;
              return !!(el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length));
            }};
            const isEnabled = (el) => !(el.disabled || attr(el, 'aria-disabled') === 'true');
            const isEditable = (el) => {{
              const tag = (el.tagName || '').toLowerCase();
              return el.isContentEditable || tag === 'textarea' || tag === 'select' || (tag === 'input' && !['button', 'submit', 'reset', 'hidden', 'image'].includes(el.type || 'text'));
            }};
            const isInteractive = (el, role) => {{
              const tag = (el.tagName || '').toLowerCase();
              return isEditable(el) || ['button', 'summary', 'select', 'textarea'].includes(tag) || (tag === 'a' && el.hasAttribute('href')) || ['button', 'link', 'checkbox', 'radio', 'textbox', 'combobox', 'menuitem', 'tab', 'switch', 'slider', 'searchbox'].includes(role || '') || typeof el.onclick === 'function' || el.tabIndex >= 0;
            }};
            const nthSelector = (el) => {{
              const parts = [];
              let node = el;
              while (node && node.nodeType === 1 && node !== document.body && parts.length < 5) {{
                const tag = (node.tagName || '').toLowerCase();
                let index = 1;
                let sibling = node;
                while ((sibling = sibling.previousElementSibling)) {{
                  if ((sibling.tagName || '').toLowerCase() === tag) index += 1;
                }}
                parts.unshift(`${{tag}}:nth-of-type(${{index}})`);
                node = node.parentElement;
              }}
              return parts.length ? parts.join(' > ') : null;
            }};
            const structuralPath = (el) => {{
              const parts = [];
              let node = el;
              while (node && node.nodeType === 1 && node !== document && parts.length < 8) {{
                const tag = (node.tagName || '').toLowerCase();
                let index = 1;
                let sibling = node;
                while ((sibling = sibling.previousElementSibling)) {{
                  if ((sibling.tagName || '').toLowerCase() === tag) index += 1;
                }}
                parts.unshift(`${{tag}}:${{index}}`);
                node = node.parentElement;
              }}
              return parts.join('/');
            }};
            const stableDataAttributes = (el) => {{
              const out = {{}};
              if (!el.getAttributeNames) return out;
              for (const name of el.getAttributeNames()) {{
                if (/^(data-testid|data-test|data-cy|data-xero-ref|id|name|aria-label)$/.test(name)) {{
                  const value = attr(el, name);
                  if (value) out[name] = value;
                }}
              }}
              return out;
            }};
            const selectorCount = (selector) => {{
              try {{ return document.querySelectorAll(selector).length; }} catch (_error) {{ return 0; }}
            }};
            const selectorCandidates = (el, role) => {{
              const tag = (el.tagName || '').toLowerCase();
              const out = [];
              const add = (selector, stability, roleOnly = false) => {{
                if (!selector) return;
                const count = selectorCount(selector);
                out.push({{ selector, unique: count === 1, count, stability, roleOnly }});
              }};
              if (el.id) add(`#${{escapeCss(el.id)}}`, 'id');
              ['data-testid', 'data-test', 'data-cy', 'name', 'aria-label'].forEach((key) => {{
                const value = attr(el, key);
                if (value) add(`${{tag}}[${{key}}="${{String(value).replace(/"/g, '\\"')}}"]`, key);
              }});
              if (role && attr(el, 'role')) add(`[role="${{String(role).replace(/"/g, '\\"')}}"]`, 'role', true);
              if (role && nameOf(el)) add(`[role="${{String(role).replace(/"/g, '\\"')}}"][aria-label="${{String(nameOf(el)).replace(/"/g, '\\"')}}"]`, 'role_name');
              const path = nthSelector(el);
              if (path) add(path, 'structural');
              const seen = new Set();
              return out
                .filter((item) => {{
                  if (seen.has(item.selector)) return false;
                  seen.add(item.selector);
                  return true;
                }})
                .sort((a, b) => Number(b.unique) - Number(a.unique) || Number(a.roleOnly) - Number(b.roleOnly))
                .slice(0, 8);
            }};
            const includeForMode = (el, role, name, text, visible) => {{
              const tag = (el.tagName || '').toLowerCase();
              if (visibleOnly && !visible) return false;
              if (mode === 'interactive') return isInteractive(el, role);
              if (mode === 'form') return ['input', 'textarea', 'select', 'button', 'form', 'label'].includes(tag) || ['textbox', 'checkbox', 'radio', 'combobox', 'button', 'form'].includes(role || '');
              if (mode === 'dialog') return role === 'dialog' || tag === 'dialog' || attr(el, 'aria-modal') === 'true' || isInteractive(el, role);
              if (mode === 'navigation') return role === 'navigation' || tag === 'nav' || tag === 'a' || role === 'link' || ['button', 'tab'].includes(role || '');
              if (mode === 'errors') return attr(el, 'aria-invalid') === 'true' || attr(el, 'role') === 'alert' || /error|required|invalid|failed/i.test(`${{name}} ${{text}}`);
              if (mode === 'headings') return role === 'heading' || /^h[1-6]$/.test(tag);
              return isInteractive(el, role) || role || /^h[1-6]$/.test(tag);
            }};
            const refs = [];
            const all = Array.from(document.querySelectorAll('body, body *'));
            for (const el of all) {{
              if (refs.length >= limit) break;
              if (!el || el.nodeType !== 1) continue;
              const role = attr(el, 'role') || implicitRole(el);
              const visible = isVisible(el);
              const text = textOf(el);
              const name = nameOf(el);
              if (!includeForMode(el, role, name, text, visible)) continue;
              const rect = el.getBoundingClientRect();
              const selectorMeta = selectorCandidates(el, role);
              refs.push({{
                tag: (el.tagName || '').toLowerCase(),
                role,
                name: name || null,
                text: text || null,
                visible,
                enabled: isEnabled(el),
                editable: isEditable(el),
                checked: typeof el.checked === 'boolean' ? Boolean(el.checked) : null,
                value: isEditable(el) && typeof el.value === 'string' ? el.value.slice(0, 300) : null,
                href: attr(el, 'href'),
                form: {{ action: attr(el, 'action'), method: attr(el, 'method'), name: attr(el, 'name') }},
                structuralPath: structuralPath(el),
                stableDataAttributes: stableDataAttributes(el),
                selectorCandidates: selectorMeta.map((item) => item.selector),
                selectorMeta,
                primarySelector: selectorMeta.find((item) => item.unique && !item.roleOnly)?.selector || selectorMeta.find((item) => item.unique)?.selector || null,
                bounds: {{ x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) }},
                frame: {{ id: 'main', url: location.href }},
                page: {{ url: location.href, title: document.title }}
              }});
            }}
            return {{ url: location.href, title: document.title, readyState: document.readyState, mode, visibleOnly, refs, totalCandidates: all.length, truncated: refs.length >= limit }};
        }})()"#
    )
}

fn native_ref_resolution_expression(node: &JsonValue) -> CommandResult<String> {
    let node_json = serde_json::to_string(node).map_err(|error| {
        CommandError::system_fault(
            "browser_native_ref_encode_failed",
            format!("Xero could not encode native CDP ref fingerprint: {error}"),
        )
    })?;
    Ok(format!(
        r#"(() => {{
            const node = {node_json};
            const norm = (value, max = 500) => String(value == null ? '' : value).trim().replace(/\s+/g, ' ').slice(0, max);
            const attr = (el, name) => el && el.getAttribute ? el.getAttribute(name) : null;
            const implicitRole = (el) => {{
              const tag = (el && el.tagName || '').toLowerCase();
              if (tag === 'a' && el.hasAttribute('href')) return 'link';
              if (tag === 'button' || tag === 'summary') return 'button';
              if (tag === 'input') {{
                const type = (el.type || 'text').toLowerCase();
                if (type === 'checkbox') return 'checkbox';
                if (type === 'radio') return 'radio';
                if (['button', 'submit', 'reset'].includes(type)) return 'button';
                return 'textbox';
              }}
              if (tag === 'textarea') return 'textbox';
              if (tag === 'select') return 'combobox';
              if (/^h[1-6]$/.test(tag)) return 'heading';
              return null;
            }};
            const textOf = (el) => norm((el && (el.innerText || el.textContent)) || '', 500);
            const nameOf = (el) => norm(attr(el, 'aria-label') || attr(el, 'alt') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el), 300);
            const stableDataAttributes = (el) => {{
              const out = {{}};
              if (!el || !el.getAttributeNames) return out;
              for (const name of el.getAttributeNames()) {{
                if (/^(data-testid|data-test|data-cy|data-xero-ref|id|name|aria-label)$/.test(name)) {{
                  const value = attr(el, name);
                  if (value) out[name] = value;
                }}
              }}
              return out;
            }};
            const fingerprint = (el) => {{
              const rect = el.getBoundingClientRect ? el.getBoundingClientRect() : {{ x: 0, y: 0, width: 0, height: 0 }};
              return {{
                tag: (el && el.tagName || '').toLowerCase(),
                role: attr(el, 'role') || implicitRole(el),
                name: nameOf(el),
                text: textOf(el),
                value: typeof el.value === 'string' ? norm(el.value, 300) : null,
                checked: typeof el.checked === 'boolean' ? Boolean(el.checked) : null,
                href: attr(el, 'href'),
                stableDataAttributes: stableDataAttributes(el),
                visible: !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length))),
                bounds: {{ x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) }},
              }};
            }};
            const candidateMeta = () => {{
              if (Array.isArray(node.selectorMeta)) return node.selectorMeta;
              if (Array.isArray(node.selectorCandidates)) return node.selectorCandidates.map((selector) => ({{ selector, unique: false, roleOnly: /^\[role=/.test(String(selector || '')) }}));
              return [];
            }};
            const mismatchesFor = (el) => {{
              const current = fingerprint(el);
              const mismatches = [];
              const expectedStable = node.stableDataAttributes && typeof node.stableDataAttributes === 'object' ? node.stableDataAttributes : {{}};
              if (node.frame && node.frame.url && node.frame.url !== location.href) mismatches.push('page_url');
              if (node.tag && current.tag !== node.tag) mismatches.push('tag');
              if (node.role && current.role !== node.role) mismatches.push('role');
              if (node.name && norm(current.name, 300) !== norm(node.name, 300)) mismatches.push('name');
              if (node.text && norm(current.text, 180) !== norm(node.text, 180)) mismatches.push('text');
              if (node.href && current.href !== node.href) mismatches.push('href');
              if (node.value != null && norm(current.value, 300) !== norm(node.value, 300)) mismatches.push('value');
              if (node.checked != null && current.checked !== node.checked) mismatches.push('checked');
              for (const [key, value] of Object.entries(expectedStable)) {{
                if (attr(el, key) !== value) mismatches.push(`stable_attr:${{key}}`);
              }}
              if (!current.visible && node.visible) mismatches.push('visibility');
              return {{ current, mismatches }};
            }};
            const tried = [];
            for (const meta of candidateMeta()) {{
              const selector = String(meta.selector || '').trim();
              if (!selector) continue;
              let matches = [];
              try {{ matches = Array.from(document.querySelectorAll(selector)); }} catch (_error) {{ continue; }}
              tried.push({{ selector, count: matches.length, snapshotUnique: Boolean(meta.unique), roleOnly: Boolean(meta.roleOnly) }});
              if (matches.length !== 1) continue;
              const verified = mismatchesFor(matches[0]);
              if (verified.mismatches.length === 0) {{
                return {{ ok: true, selector, strategy: 'selector', selectorUnique: true, fingerprint: verified.current }};
              }}
            }}
            return {{
              ok: false,
              code: 'browser_ref_stale',
              message: 'Browser ref no longer resolves to the snapshotted native CDP element. Run snapshot again and use a fresh ref.',
              ref: node.ref || null,
              tried,
              currentUrl: location.href,
              snapshotUrl: node.frame && node.frame.url || null,
            }};
        }})()"#
    ))
}

fn native_find_best_expression(
    intent: &str,
    text: Option<&str>,
    role: Option<&str>,
    cached_selectors: &[String],
) -> CommandResult<String> {
    let intent_json = js_string(intent)?;
    let text_json = optional_js_string(text)?;
    let role_json = optional_js_string(role)?;
    let cached_json = serde_json::to_string(cached_selectors).map_err(|error| {
        CommandError::system_fault(
            "browser_native_script_encode_failed",
            format!("Xero could not encode native CDP selector cache: {error}"),
        )
    })?;
    Ok(format!(
        r#"(() => {{
            const intent = {intent_json};
            const requestedText = {text_json};
            const requestedRole = {role_json};
            const cachedSelectors = {cached_json};
            const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 500);
            const attr = (el, name) => el.getAttribute && el.getAttribute(name);
            const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
            const roleOf = (el) => attr(el, 'role') || (((el.tagName || '').toLowerCase() === 'button' || ['submit','button'].includes(el.type || '')) ? 'button' : ((el.tagName || '').toLowerCase() === 'a' && el.hasAttribute('href') ? 'link' : ((el.tagName || '').toLowerCase() === 'input' ? 'textbox' : null)));
            const nameOf = (el) => (attr(el, 'aria-label') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el)).slice(0, 300);
            const selectorFor = (el) => {{
              if (el.id) return '#' + (window.CSS && CSS.escape ? CSS.escape(el.id) : el.id);
              for (const key of ['data-testid', 'data-test', 'data-cy', 'name', 'aria-label']) {{
                const value = attr(el, key);
                if (value) return `${{(el.tagName || '').toLowerCase()}}[${{key}}="${{String(value).replace(/"/g, '\\"')}}"]`;
              }}
              return null;
            }};
            for (const selector of cachedSelectors || []) {{
              try {{
                const el = document.querySelector(selector);
                if (el && visible(el)) return {{ cacheHit: true, confidence: 92, intent, node: {{ tag: (el.tagName || '').toLowerCase(), role: roleOf(el), name: nameOf(el), text: textOf(el), selectorCandidates: [selector] }} }};
              }} catch (_) {{}}
            }}
            const terms = [intent, requestedText].filter(Boolean).join(' ').toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
            const candidates = Array.from(document.querySelectorAll('button, a[href], input, textarea, select, [role], [tabindex], summary')).filter(visible);
            let best = null;
            for (const el of candidates) {{
              const role = roleOf(el);
              const name = nameOf(el);
              const haystack = `${{role || ''}} ${{name}} ${{textOf(el)}} ${{(el.tagName || '').toLowerCase()}} ${{attr(el, 'type') || ''}}`.toLowerCase();
              let score = 0;
              if (requestedRole && role === requestedRole) score += 35;
              for (const term of terms) if (haystack.includes(term)) score += 12;
              if (/submit|continue|next|primary|login|sign in|search|accept|close|dismiss/.test(haystack)) score += 8;
              if (!el.disabled && attr(el, 'aria-disabled') !== 'true') score += 5;
              const selector = selectorFor(el);
              if (selector) score += 3;
              if (!best || score > best.score) best = {{ el, score, selector }};
            }}
            if (!best || best.score <= 0) throw new Error('browser find_best could not identify a target for intent: ' + intent);
            return {{
              cacheHit: false,
              confidence: Math.max(1, Math.min(99, best.score)),
              intent,
              node: {{
                tag: (best.el.tagName || '').toLowerCase(),
                role: roleOf(best.el),
                name: nameOf(best.el),
                text: textOf(best.el),
                visible: visible(best.el),
                enabled: !(best.el.disabled || attr(best.el, 'aria-disabled') === 'true'),
                selectorCandidates: best.selector ? [best.selector] : [],
              }}
            }};
        }})()"#
    ))
}

fn native_analyze_form_expression(selector: Option<&str>) -> CommandResult<String> {
    let selector_json = optional_js_string(selector)?;
    Ok(format!(
        r#"(() => {{
            const selector = {selector_json};
            const root = selector ? document.querySelector(selector) : document;
            if (!root) throw new Error('form root not found: ' + selector);
            const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 300);
            const labelFor = (field) => {{
              if (field.id) {{
                const label = root.querySelector(`label[for="${{field.id}}"]`) || document.querySelector(`label[for="${{field.id}}"]`);
                if (label && textOf(label)) return textOf(label);
              }}
              const parentLabel = field.closest && field.closest('label');
              if (parentLabel && textOf(parentLabel)) return textOf(parentLabel);
              return field.getAttribute('aria-label') || field.getAttribute('placeholder') || field.getAttribute('name') || field.id || '';
            }};
            const forms = Array.from(root.querySelectorAll ? root.querySelectorAll('form') : []).concat(root.tagName === 'FORM' ? [root] : []);
            const scanRoot = forms.length ? forms : [root];
            return {{ forms: scanRoot.map((form, index) => ({{
              index,
              name: form.getAttribute && (form.getAttribute('name') || form.getAttribute('aria-label')) || null,
              action: form.getAttribute && form.getAttribute('action') || null,
              method: form.getAttribute && form.getAttribute('method') || null,
              fields: Array.from(form.querySelectorAll('input, textarea, select')).map((field) => ({{
                tag: (field.tagName || '').toLowerCase(),
                type: field.getAttribute('type') || null,
                name: field.getAttribute('name') || null,
                id: field.id || null,
                label: labelFor(field),
                required: !!field.required || field.getAttribute('aria-required') === 'true',
                valuePresent: !!field.value,
                disabled: !!field.disabled,
              }})),
              submitCandidates: Array.from(form.querySelectorAll('button, input[type="submit"], [role="button"]')).map((button) => ({{
                tag: (button.tagName || '').toLowerCase(),
                type: button.getAttribute('type') || null,
                label: textOf(button) || button.value || button.getAttribute('aria-label') || null,
                disabled: !!button.disabled,
              }})),
            }})) }};
        }})()"#
    ))
}

fn native_fill_form_expression(
    selector: Option<&str>,
    fields: &BTreeMap<String, String>,
    submit: bool,
) -> CommandResult<String> {
    let selector_json = optional_js_string(selector)?;
    let fields_json = serde_json::to_string(fields).map_err(|error| {
        CommandError::system_fault(
            "browser_native_script_encode_failed",
            format!("Xero could not encode native CDP form fields: {error}"),
        )
    })?;
    let submit = if submit { "true" } else { "false" };
    Ok(format!(
        r#"(() => {{
            const selector = {selector_json};
            const fields = {fields_json};
            const submit = {submit};
            const root = selector ? document.querySelector(selector) : document;
            if (!root) throw new Error('form root not found: ' + selector);
            const normalize = (value) => String(value || '').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim();
            const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ');
            const labelFor = (field) => {{
              if (field.id) {{
                const label = root.querySelector(`label[for="${{field.id}}"]`) || document.querySelector(`label[for="${{field.id}}"]`);
                if (label && textOf(label)) return textOf(label);
              }}
              const parentLabel = field.closest && field.closest('label');
              if (parentLabel && textOf(parentLabel)) return textOf(parentLabel);
              return field.getAttribute('aria-label') || field.getAttribute('placeholder') || field.getAttribute('name') || field.id || '';
            }};
            const setField = (field, value) => {{
              const tag = (field.tagName || '').toLowerCase();
              const type = (field.getAttribute('type') || '').toLowerCase();
              field.focus && field.focus();
              if (type === 'checkbox' || type === 'radio') field.checked = ['true', '1', 'yes', 'on', 'checked'].includes(String(value).toLowerCase());
              else if (tag === 'select') field.value = String(value);
              else field.value = String(value);
              field.dispatchEvent(new Event('input', {{ bubbles: true }}));
              field.dispatchEvent(new Event('change', {{ bubbles: true }}));
            }};
            const candidates = Array.from(root.querySelectorAll('input, textarea, select')).filter((field) => !field.disabled);
            const matched = [];
            const unmatched = [];
            for (const [label, value] of Object.entries(fields)) {{
              const wanted = normalize(label);
              const field = candidates.find((candidate) => {{
                const haystack = normalize([labelFor(candidate), candidate.name, candidate.id, candidate.getAttribute('placeholder'), candidate.getAttribute('aria-label'), candidate.type].filter(Boolean).join(' '));
                return haystack === wanted || haystack.includes(wanted) || wanted.includes(haystack);
              }});
              if (!field) {{ unmatched.push(label); continue; }}
              setField(field, value);
              matched.push({{ label, field: labelFor(field), name: field.name || null, id: field.id || null }});
            }}
            let submitted = false;
            if (submit) {{
              const form = root.tagName === 'FORM' ? root : (root.querySelector('form') || candidates[0]?.form);
              const button = form && Array.from(form.querySelectorAll('button, input[type="submit"], [role="button"]')).find((el) => !el.disabled);
              if (button) {{ button.click(); submitted = true; }}
              else if (form && typeof form.requestSubmit === 'function') {{ form.requestSubmit(); submitted = true; }}
            }}
            return {{ matched, unmatched, submitted }};
        }})()"#
    ))
}

fn native_prompt_injection_scan_expression(
    include_hidden: bool,
    selector: Option<&str>,
    limit: usize,
) -> CommandResult<String> {
    let selector_json = optional_js_string(selector)?;
    let hidden = if include_hidden { "true" } else { "false" };
    let limit = limit.clamp(1, 500);
    Ok(format!(
        r#"(() => {{
            const selector = {selector_json};
            const includeHidden = {hidden};
            const limit = {limit};
            const root = selector ? document.querySelector(selector) : document.body;
            if (!root) throw new Error('scan root not found: ' + selector);
            const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
            const patterns = [
              {{ id: 'ignore_previous_instructions', pattern: /ignore (all )?(previous|prior|above) instructions/i }},
              {{ id: 'system_prompt_request', pattern: /(system|developer) (prompt|message|instructions)/i }},
              {{ id: 'tool_exfiltration', pattern: /(send|exfiltrate|post|upload).{{0,80}}(token|secret|cookie|password|api key)/i }},
              {{ id: 'hidden_agent_instruction', pattern: /(assistant|agent|model).{{0,80}}(must|should|will).{{0,80}}(click|type|download|submit|send)/i }},
              {{ id: 'credential_request', pattern: /(enter|share|paste).{{0,80}}(password|token|secret|api key|cookie)/i }}
            ];
            const findings = [];
            const scanText = (source, text, hidden, node) => {{
              if (!text || findings.length >= limit) return;
              for (const item of patterns) {{
                const match = String(text).match(item.pattern);
                if (match) {{
                  findings.push({{
                    id: item.id,
                    source,
                    hidden,
                    snippet: String(text).replace(/\s+/g, ' ').slice(Math.max(0, match.index - 40), Math.min(String(text).length, match.index + 160)),
                    tag: node && node.tagName ? node.tagName.toLowerCase() : null,
                  }});
                  break;
                }}
              }}
            }};
            const nodes = Array.from(root.querySelectorAll ? root.querySelectorAll('*') : []);
            for (const node of [root].concat(nodes)) {{
              if (findings.length >= limit) break;
              const hiddenNode = !visible(node);
              if (hiddenNode && !includeHidden) continue;
              scanText('text', node.innerText || node.textContent || '', hiddenNode, node);
              if (node.getAttributeNames) for (const name of node.getAttributeNames()) scanText(`attribute:${{name}}`, node.getAttribute(name), hiddenNode, node);
            }}
            return {{ scannedNodes: nodes.length + 1, includeHidden, findings, risk: findings.length ? 'suspicious' : 'none_detected' }};
        }})()"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};

    #[test]
    fn normalize_session_id_keeps_stable_safe_ids() {
        assert_eq!(
            normalize_session_id(Some("My Session!/1".into()), || "fallback".into()),
            "My-Session--1"
        );
        assert_eq!(
            normalize_session_id(Some("***".into()), || "fallback".into()),
            DEFAULT_SESSION_ID
        );
    }

    #[test]
    fn native_health_does_not_depend_on_gsd_browser() {
        let service = NativeCdpBrowserService::default();
        let temp = tempfile::tempdir().expect("tempdir");
        let health = service.health(temp.path());
        assert_eq!(health["engine"], "native_cdp");
        assert_eq!(health["backend"], "xero_internal_cdp");
        assert_eq!(health["healthy"], true);
        assert!(health.to_string().contains("browserCandidates"));
        assert!(!health.to_string().contains("gsd-browser"));
    }

    #[test]
    fn capability_manifest_is_internal_cdp_not_external_wrapper() {
        let service = NativeCdpBrowserService::default();
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = service.capability_manifest(temp.path());
        assert_eq!(manifest["available"], true);
        assert_eq!(manifest["nativeEngineCompiled"], true);
        assert_eq!(manifest["attachAvailable"], true);
        assert_eq!(manifest["remoteAttachDisabledByPolicy"], true);
        assert_eq!(manifest["backend"], "xero_internal_cdp");
        assert!(!manifest.to_string().contains("gsd-browser"));
    }

    #[test]
    fn native_attach_endpoint_is_loopback_by_default() {
        assert_eq!(
            normalize_endpoint("127.0.0.1:9222", false).expect("loopback"),
            "http://127.0.0.1:9222"
        );
        assert_eq!(
            normalize_endpoint("ws://[::1]:9222/devtools/page/1", false).expect("loopback ws"),
            "ws://[::1]:9222/devtools/page/1"
        );

        let denied = normalize_endpoint("http://203.0.113.10:9222", false)
            .expect_err("remote endpoint denied by default");
        assert_eq!(denied.code, "policy_denied");

        assert_eq!(
            normalize_endpoint("http://203.0.113.10:9222", true).expect("explicit remote"),
            "http://203.0.113.10:9222"
        );
    }

    #[test]
    fn native_metadata_redacts_control_endpoint() {
        assert_eq!(
            redact_cdp_endpoint("http://127.0.0.1:9222"),
            "http://loopback:<redacted-port>/<redacted>"
        );
        assert_eq!(
            cdp_endpoint_kind("ws://127.0.0.1:9222/devtools/page/abc"),
            "loopback_websocket"
        );
    }

    #[test]
    fn generated_native_session_ids_are_unpredictable() {
        let service = NativeCdpBrowserService::default();
        let first = service.allocate_session_id();
        let second = service.allocate_session_id();
        assert_ne!(first, second);
        assert!(first.starts_with("native-1-"));
        assert!(second.starts_with("native-2-"));
        assert!(first.len() > "native-1-".len() + 8);
    }

    #[test]
    fn capability_manifest_does_not_advertise_legacy_native_state_mutators() {
        let service = NativeCdpBrowserService::default();
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = service.capability_manifest(temp.path());
        let state = manifest["supports"]["state"]
            .as_array()
            .expect("state support list");
        assert!(state.iter().any(|value| value == "state_restore"));
        assert!(!state.iter().any(|value| value == "cookies_set"));
        assert!(!state.iter().any(|value| value == "storage_write"));
        assert!(!state.iter().any(|value| value == "storage_clear"));
    }

    #[test]
    fn capability_families_cover_native_gap_closure_actions() {
        let actions = native_cdp_capability_families()
            .into_values()
            .flatten()
            .collect::<BTreeSet<_>>();

        for action in [
            "select_option",
            "set_checked",
            "drag",
            "upload_file",
            "dialog_accept",
            "download_save",
            "trace_export",
            "visual_diff",
            "emulate_device",
            "extract",
            "switch_page",
            "select_frame",
            "auth_profile_restore",
            "viewer_goal",
            "browser_resource",
            "browser_prompt",
            "mcp_bridge",
            "generate_test",
        ] {
            assert!(
                actions.contains(action),
                "missing native CDP capability {action}"
            );
        }

        assert!(
            !actions.contains("vault_login"),
            "credential replay remains unavailable until encrypted vault replay is implemented"
        );
    }

    #[test]
    fn visual_diff_bytes_detects_identical_and_changed_pixels() {
        fn png(color: Rgba<u8>, changed: Option<(u32, u32, Rgba<u8>)>) -> Vec<u8> {
            let mut image = ImageBuffer::from_pixel(2, 2, color);
            if let Some((x, y, pixel)) = changed {
                image.put_pixel(x, y, pixel);
            }
            let mut cursor = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .expect("encode png");
            cursor.into_inner()
        }

        let baseline = png(Rgba([255, 255, 255, 255]), None);
        let identical = visual_diff_bytes(&baseline, &baseline).expect("identical diff");
        assert_eq!(identical.pixel_count, 4);
        assert_eq!(identical.different_pixels, 0);
        assert_eq!(identical.percent_difference, 0.0);

        let current = png(
            Rgba([255, 255, 255, 255]),
            Some((1, 1, Rgba([0, 0, 0, 255]))),
        );
        let changed = visual_diff_bytes(&baseline, &current).expect("changed diff");
        assert_eq!(changed.pixel_count, 4);
        assert_eq!(changed.different_pixels, 1);
        assert_eq!(changed.percent_difference, 25.0);
        assert_eq!(changed.image.get_pixel(1, 1), &Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn device_preset_state_exposes_mobile_and_desktop_contract_fields() {
        let iphone = device_preset_state("iphone_14").expect("iphone preset");
        assert_eq!(iphone["viewport"]["mobile"], true);
        assert_eq!(iphone["viewport"]["touch"], true);
        assert!(iphone["userAgent"]
            .as_str()
            .expect("user agent")
            .contains("iPhone"));

        let desktop = device_preset_state("desktop").expect("desktop preset");
        assert_eq!(desktop["viewport"]["width"], 1920);
        assert_eq!(desktop["viewport"]["mobile"], false);
    }

    #[test]
    fn native_extract_expression_supports_gap_closure_modes() {
        let expression = native_extract_expression(
            "selector_map",
            Some("main"),
            Some(BTreeMap::from([("cta".into(), "button.primary".into())])),
            20,
        )
        .expect("extract expression");

        assert!(expression.contains("selector_map"));
        assert!(expression.contains("visible_text_blocks"));
        assert!(expression.contains("application/ld+json"));
        assert!(expression.contains("button.primary"));
    }

    #[test]
    fn snapshot_expression_contains_versionable_ref_metadata_inputs() {
        let expression = native_snapshot_expression("interactive", true, 10);
        assert!(expression.contains("selectorCandidates"));
        assert!(expression.contains("bounds"));
        assert!(expression.contains("visibleOnly"));
    }

    #[test]
    fn http_discovery_reads_cdp_page_targets() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 1024];
            let read = stream.read(&mut request).expect("read");
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /json/list "));
            let body = format!(
                r#"[{{"id":"page-1","type":"page","title":"Fixture","url":"https://example.com/","webSocketDebuggerUrl":"ws://127.0.0.1:{port}/devtools/page/page-1"}}]"#
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("write");
        });

        let pages = fetch_pages(&format!("http://127.0.0.1:{port}")).expect("pages");
        server.join().expect("server");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].target_id, "page-1");
        assert_eq!(pages[0].title, "Fixture");
        assert_eq!(pages[0].url, "https://example.com/");
    }

    #[test]
    fn websocket_transport_correlates_cdp_command_responses() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut socket = tungstenite::accept(stream).expect("websocket accept");
            let message = socket.read().expect("read message");
            let Message::Text(text) = message else {
                panic!("expected text websocket message");
            };
            let payload = serde_json::from_str::<JsonValue>(&text).expect("json");
            assert_eq!(payload["method"], "Runtime.evaluate");
            let id = payload["id"].as_u64().expect("id");
            socket
                .send(Message::Text(
                    json!({
                        "method": "Runtime.consoleAPICalled",
                        "params": {
                            "type": "log",
                            "args": [{ "value": "hello from event" }]
                        }
                    })
                    .to_string(),
                ))
                .expect("event");
            socket
                .send(Message::Text(
                    json!({
                        "id": id,
                        "result": {
                            "result": {
                                "type": "object",
                                "value": { "ok": true }
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("response");
        });

        let temp = tempfile::tempdir().expect("tempdir");
        let mut session = NativeCdpSession::new(
            "test".into(),
            "Test".into(),
            format!("http://127.0.0.1:{port}"),
            None,
            Some(port),
            temp.path().join("profile"),
            temp.path().join("artifacts"),
            false,
            false,
            None,
        );
        session.active_page = Some(NativeCdpPage {
            target_id: "page-1".into(),
            title: "Fixture".into(),
            url: "https://example.com/".into(),
            websocket_url: format!("ws://127.0.0.1:{port}/devtools/page/page-1"),
        });

        let mut client = session.connect_page().expect("client");
        let result = runtime_evaluate(
            &mut client,
            &mut session,
            "({ ok: true })",
            Duration::from_secs(2),
        )
        .expect("evaluate");
        server.join().expect("server");
        assert_eq!(result["ok"], true);
        assert_eq!(session.console_events.len(), 1);
        assert_eq!(session.console_events[0].message, "hello from event");
    }
}
