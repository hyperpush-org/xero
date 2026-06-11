pub mod actions;
pub mod automation;
pub(crate) mod bridge;
pub mod cookie_import;
mod diagnostics;
mod events;
pub mod native_cdp;
mod screenshot;
mod script;
pub mod settings;
pub mod tabs;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::OpenOptions,
    io::{Read, Write},
    net::{IpAddr, TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(any(
    windows,
    target_os = "macos",
    all(unix, not(any(target_os = "linux", target_os = "macos")))
))]
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, Rect, Runtime, State, Webview, WebviewUrl,
};
use url::Url;

use crate::commands::{CommandError, CommandResult};

#[cfg(target_os = "macos")]
use {
    block2::{Block, RcBlock},
    objc2::{rc::Retained, runtime::AnyObject, ClassType},
    objc2_app_kit::{NSEvent, NSEventMask, NSEventTrackingRunLoopMode, NSView, NSWindow},
    objc2_core_graphics::CGMutablePath,
    objc2_foundation::{NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSTimer},
    objc2_quartz_core::{kCAFillRuleEvenOdd, CAShapeLayer, CATransaction},
    std::ptr::NonNull,
};

pub use actions::{StorageArea, TypingMode};
pub use automation::{
    validate_browser_artifact_manifest, write_browser_artifact, BrowserActionCacheEntry,
    BrowserAnnotation, BrowserAutomationState, BrowserRecording, BrowserRefStore,
    BrowserTimelineEvent,
};
pub use diagnostics::{
    BrowserConsoleDiagnosticEntry, BrowserDiagnosticReadOptions, BrowserDiagnostics,
    BrowserNetworkDiagnosticEntry,
};
pub use events::{
    BrowserConsolePayload, BrowserDevServerUnavailablePayload, BrowserDialogPayload,
    BrowserDownloadPayload, BrowserLoadStatePayload, BrowserOcclusionClickPayload,
    BrowserOcclusionWheelPayload, BrowserResizeDragPayload, BrowserTabUpdatedPayload,
    BrowserToolClosedPayload, BrowserToolContextPayload, BrowserToolDictationTogglePayload,
    BrowserToolNotePayload, BrowserToolStatePayload, BrowserUrlChangedPayload,
    BROWSER_CONSOLE_EVENT, BROWSER_DEV_SERVER_UNAVAILABLE_EVENT, BROWSER_DIALOG_EVENT,
    BROWSER_DOWNLOAD_EVENT, BROWSER_LOAD_STATE_EVENT, BROWSER_OCCLUSION_CLICK_EVENT,
    BROWSER_OCCLUSION_WHEEL_EVENT, BROWSER_RESIZE_DRAG_EVENT, BROWSER_TAB_UPDATED_EVENT,
    BROWSER_TOOL_CLOSED_EVENT, BROWSER_TOOL_CONTEXT_EVENT, BROWSER_TOOL_DICTATION_TOGGLE_EVENT,
    BROWSER_TOOL_NOTE_EVENT, BROWSER_TOOL_STATE_EVENT, BROWSER_URL_CHANGED_EVENT,
};
pub use native_cdp::{NativeCdpActionResult, NativeCdpBrowserService};
pub use screenshot::capture_webview as screenshot_webview;
pub(crate) use settings::load_browser_control_settings;
pub use settings::{
    browser_control_settings, browser_control_update_settings, BrowserControlPreferenceDto,
    BrowserControlSettingsDto, UpsertBrowserControlSettingsRequestDto,
};
pub use tabs::{BrowserTabMetadata, BROWSER_TAB_PREFIX};

use bridge::{BridgeReply, BridgeWaiters};
use script::BROWSER_BRIDGE_INIT_SCRIPT;
use tabs::{BrowserTabs, BROWSER_MAIN_WINDOW_LABEL};

const HIDDEN_OFFSET: f64 = -32_000.0;
const DEFAULT_AUTONOMOUS_BROWSER_WIDTH: f64 = 1_280.0;
const DEFAULT_AUTONOMOUS_BROWSER_HEIGHT: f64 = 720.0;
const RESIZE_DRAG_MAX_DURATION: Duration = Duration::from_secs(30);
const DEV_SERVER_MONITOR_INITIAL_DELAY: Duration = Duration::from_secs(2);
const DEV_SERVER_MONITOR_INTERVAL: Duration = Duration::from_secs(2);
const DEV_SERVER_MONITOR_CONNECT_TIMEOUT: Duration = Duration::from_millis(350);
const DEV_SERVER_LIVENESS_CONNECT_TIMEOUT: Duration = Duration::from_millis(180);
const DEV_SERVER_LIST_HTTP_PROBE_TIMEOUT: Duration = Duration::from_millis(220);
const DEV_SERVER_MONITOR_FAILURE_THRESHOLD: u8 = 3;
const DEV_SERVER_RECONCILER_INTERVAL: Duration = Duration::from_secs(2);
const BROWSER_RESIZE_NUDGE_SCRIPT: &str = r#"
(() => {
  try {
    window.dispatchEvent(new Event("resize"));
    if (window.visualViewport) {
      window.visualViewport.dispatchEvent(new Event("resize"));
    }
    requestAnimationFrame(() => {
      try {
        window.dispatchEvent(new Event("resize"));
      } catch (_) {}
    });
  } catch (_) {}
})();
"#;
#[cfg(debug_assertions)]
const BROWSER_NATIVE_PROBE_SCRIPT: &str = r#"
(() => {
  const read = (fn) => {
    try {
      return fn();
    } catch (error) {
      return { error: error && (error.stack || error.message) ? String(error.stack || error.message) : String(error) };
    }
  };

  return {
    href: read(() => location.href),
    readyState: read(() => document.readyState),
    title: read(() => document.title),
    hasBody: read(() => Boolean(document.body)),
    bodyText: read(() => document.body ? document.body.innerText.slice(0, 500) : null),
    bodyBg: read(() => document.body ? getComputedStyle(document.body).backgroundColor : null),
    rootChildren: read(() => {
      const root = document.getElementById('root');
      return root ? root.childElementCount : null;
    }),
    rootText: read(() => {
      const root = document.getElementById('root');
      return root ? root.innerText.slice(0, 500) : null;
    }),
    htmlSample: read(() => document.documentElement ? document.documentElement.outerHTML.slice(0, 1000) : null),
    tauriInternals: read(() => Boolean(window.__TAURI_INTERNALS__)),
    xeroBridge: read(() => Boolean(window.__xeroBridge__)),
    bridgeErrors: read(() => {
      const state = window.__xeroBridgeState__;
      return state && Array.isArray(state.errors) ? state.errors.slice(-8) : null;
    }),
    resources: read(() => performance.getEntriesByType('resource').slice(-12).map((entry) => ({
      name: entry.name,
      initiatorType: entry.initiatorType,
      duration: Math.round(entry.duration),
      transferSize: entry.transferSize || 0,
    }))),
  };
})()
"#;

#[cfg(debug_assertions)]
fn append_browser_probe_log(message: impl AsRef<str>) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let path = std::env::temp_dir().join("xero-browser-webview-probe.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{timestamp}] {}", message.as_ref());
    }
}

#[cfg(debug_assertions)]
fn schedule_browser_webview_probe<R: Runtime + 'static>(
    app: &AppHandle<R>,
    tab_id: &str,
    label: &str,
    target: &Url,
    reason: &str,
) {
    append_browser_probe_log(format!(
        "schedule reason={reason} tab={tab_id} label={label} target={target}"
    ));

    for delay_ms in [0_u64, 250, 1_000, 3_000, 6_000] {
        let app = app.clone();
        let tab_id = tab_id.to_string();
        let label = label.to_string();
        let target = target.to_string();
        let reason = reason.to_string();
        thread::spawn(move || {
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }

            let Some(webview) = app.get_webview(&label) else {
                append_browser_probe_log(format!(
                    "probe reason={reason} delay={delay_ms}ms tab={tab_id} label={label} target={target} missing-webview"
                ));
                return;
            };

            let prefix = format!(
                "probe reason={reason} delay={delay_ms}ms tab={tab_id} label={label} target={target}"
            );
            append_browser_probe_log(&prefix);
            let callback_prefix = prefix.clone();
            if let Err(error) =
                webview.eval_with_callback(BROWSER_NATIVE_PROBE_SCRIPT, move |raw| {
                    append_browser_probe_log(format!("{callback_prefix} eval={raw}"));
                })
            {
                append_browser_probe_log(format!("{prefix} eval-error={error}"));
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRunningDevServerDto {
    pub cwd: Option<String>,
    pub detected_at: u64,
    pub label: String,
    pub local_addr: String,
    pub pid: Option<u32>,
    pub port: u16,
    pub process_name: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone)]
struct BrowserSystemPortInfo {
    cwd: Option<String>,
    local_addr: String,
    local_port: u16,
    pid: Option<u32>,
    process_name: Option<String>,
}

impl BrowserViewport {
    fn sanitize(self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            width: self.width.max(1.0),
            height: self.height.max(1.0),
        }
    }

    fn as_rect(self) -> Rect {
        let viewport = self.sanitize();
        Rect {
            position: LogicalPosition::new(viewport.x, viewport.y).into(),
            size: LogicalSize::new(viewport.width, viewport.height).into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOcclusionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl BrowserOcclusionRect {
    fn sanitize(self, viewport: BrowserViewport) -> Option<Self> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return None;
        }

        let left = self.x.max(0.0).min(viewport.width);
        let top = self.y.max(0.0).min(viewport.height);
        let right = (self.x + self.width).max(0.0).min(viewport.width);
        let bottom = (self.y + self.height).max(0.0).min(viewport.height);
        if right <= left || bottom <= top {
            return None;
        }

        Some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

pub struct BrowserState {
    creation_lock: Mutex<()>,
    resize_coalescer: Arc<BrowserResizeCoalescer>,
    resize_drag: Arc<BrowserResizeDragState>,
    #[cfg(target_os = "macos")]
    occlusion_wheel: Arc<BrowserOcclusionWheelState>,
    dev_server_monitor: Arc<BrowserDevServerMonitorState>,
    dev_server_reconciler_started: AtomicBool,
    waiters: Arc<BridgeWaiters>,
    tabs: Arc<BrowserTabs>,
    diagnostics: Arc<BrowserDiagnostics>,
    automation: Arc<BrowserAutomationState>,
    native_cdp: Arc<NativeCdpBrowserService>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            creation_lock: Mutex::new(()),
            resize_coalescer: Arc::new(BrowserResizeCoalescer::default()),
            resize_drag: Arc::new(BrowserResizeDragState::default()),
            #[cfg(target_os = "macos")]
            occlusion_wheel: Arc::new(BrowserOcclusionWheelState::default()),
            dev_server_monitor: Arc::new(BrowserDevServerMonitorState::default()),
            dev_server_reconciler_started: AtomicBool::new(false),
            waiters: Arc::new(BridgeWaiters::new()),
            tabs: Arc::new(BrowserTabs::new()),
            diagnostics: Arc::new(BrowserDiagnostics::default()),
            automation: Arc::new(BrowserAutomationState::default()),
            native_cdp: Arc::new(NativeCdpBrowserService::default()),
        }
    }
}

#[derive(Default)]
struct BrowserDevServerMonitorState {
    generations: Mutex<HashMap<String, u64>>,
    next_generation: AtomicU64,
}

impl BrowserDevServerMonitorState {
    fn start(&self, tab_id: &str) -> CommandResult<u64> {
        let generation = self.next_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let mut generations = self.generations.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_dev_server_monitor_lock_poisoned",
                "Browser dev-server monitor lock poisoned.",
            )
        })?;
        generations.insert(tab_id.to_string(), generation);
        Ok(generation)
    }

    fn cancel(&self, tab_id: &str) {
        if let Ok(mut generations) = self.generations.lock() {
            generations.remove(tab_id);
        }
    }

    fn is_current(&self, tab_id: &str, generation: u64) -> bool {
        self.generations
            .lock()
            .ok()
            .and_then(|generations| generations.get(tab_id).copied())
            == Some(generation)
    }

    fn clear_if_current(&self, tab_id: &str, generation: u64) -> bool {
        let Ok(mut generations) = self.generations.lock() else {
            return false;
        };
        if generations.get(tab_id).copied() != Some(generation) {
            return false;
        }
        generations.remove(tab_id);
        true
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct BrowserResizeDragSession {
    id: u64,
    labels: Vec<String>,
    tab_id: Option<String>,
    start_client_x: f64,
    start_width: f64,
    right: f64,
    top: f64,
    height: f64,
    min_width: f64,
    max_width: f64,
    inset: f64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl BrowserResizeDragSession {
    fn width_for_cursor(&self, cursor_client_x: f64) -> f64 {
        let delta = self.start_client_x - cursor_client_x;
        (self.start_width + delta).clamp(self.min_width, self.max_width)
    }

    fn viewport_for_width(&self, width: f64) -> BrowserViewport {
        BrowserViewport {
            x: self.right - width + self.inset,
            y: self.top,
            width: (width - self.inset).max(1.0),
            height: self.height,
        }
    }

    fn viewport_for_cursor(&self, cursor_client_x: f64) -> BrowserViewport {
        self.viewport_for_width(self.width_for_cursor(cursor_client_x))
    }
}

#[cfg(target_os = "macos")]
type BrowserNativeResizeDragTimerHandler = dyn Fn(NonNull<NSTimer>);

#[cfg(target_os = "macos")]
struct BrowserNativeResizeDragMonitor {
    timer: usize,
    block: usize,
}

#[cfg(target_os = "macos")]
type BrowserNativeOcclusionWheelHandler = dyn Fn(NonNull<NSEvent>) -> *mut NSEvent;

#[cfg(target_os = "macos")]
type BrowserNativeOcclusionClickHandler = dyn Fn(NonNull<NSEvent>) -> *mut NSEvent;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
struct BrowserNativeOcclusionWheelRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
impl BrowserNativeOcclusionWheelRect {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

#[cfg(target_os = "macos")]
struct BrowserNativeOcclusionWheelMonitor {
    event_monitor: usize,
    block: usize,
}

#[cfg(target_os = "macos")]
struct BrowserNativeOcclusionClickMonitor {
    event_monitor: usize,
    block: usize,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct BrowserOcclusionWheelState {
    native_monitor: Mutex<Option<BrowserNativeOcclusionWheelMonitor>>,
    native_click_monitor: Mutex<Option<BrowserNativeOcclusionClickMonitor>>,
}

#[cfg(target_os = "macos")]
impl BrowserOcclusionWheelState {
    fn replace_native_monitor(
        &self,
        monitor: BrowserNativeOcclusionWheelMonitor,
    ) -> Option<BrowserNativeOcclusionWheelMonitor> {
        match self.native_monitor.lock() {
            Ok(mut active) => active.replace(monitor),
            Err(_) => Some(monitor),
        }
    }

    fn take_native_monitor(&self) -> Option<BrowserNativeOcclusionWheelMonitor> {
        self.native_monitor
            .lock()
            .ok()
            .and_then(|mut active| active.take())
    }

    fn replace_native_click_monitor(
        &self,
        monitor: BrowserNativeOcclusionClickMonitor,
    ) -> Option<BrowserNativeOcclusionClickMonitor> {
        match self.native_click_monitor.lock() {
            Ok(mut active) => active.replace(monitor),
            Err(_) => Some(monitor),
        }
    }

    fn take_native_click_monitor(&self) -> Option<BrowserNativeOcclusionClickMonitor> {
        self.native_click_monitor
            .lock()
            .ok()
            .and_then(|mut active| active.take())
    }
}

#[derive(Default)]
struct BrowserResizeDragState {
    active: Mutex<Option<BrowserResizeDragSession>>,
    #[cfg(target_os = "macos")]
    native_monitor: Mutex<Option<BrowserNativeResizeDragMonitor>>,
    next_id: AtomicU64,
}

impl BrowserResizeDragState {
    fn begin(&self, mut session: BrowserResizeDragSession) -> CommandResult<u64> {
        let id = self.next_id.fetch_add(1, Ordering::AcqRel) + 1;
        session.id = id;
        let mut active = self.active.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_resize_drag_lock_poisoned",
                "Browser resize drag state lock poisoned.",
            )
        })?;
        *active = Some(session);
        Ok(id)
    }

    fn end(&self) -> CommandResult<Option<BrowserResizeDragSession>> {
        let mut active = self.active.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_resize_drag_lock_poisoned",
                "Browser resize drag state lock poisoned.",
            )
        })?;
        Ok(active.take())
    }

    fn take_if_current(&self, id: u64) -> Option<BrowserResizeDragSession> {
        if let Ok(mut active) = self.active.lock() {
            if active.as_ref().is_some_and(|session| session.id == id) {
                return active.take();
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    fn replace_native_monitor_if_current(
        &self,
        id: u64,
        monitor: BrowserNativeResizeDragMonitor,
    ) -> Option<BrowserNativeResizeDragMonitor> {
        let is_current = self
            .active
            .lock()
            .map(|active| active.as_ref().is_some_and(|session| session.id == id))
            .unwrap_or(false);
        if !is_current {
            return Some(monitor);
        }

        match self.native_monitor.lock() {
            Ok(mut active) => active.replace(monitor),
            Err(_) => Some(monitor),
        }
    }

    #[cfg(target_os = "macos")]
    fn take_native_monitor(&self) -> Option<BrowserNativeResizeDragMonitor> {
        self.native_monitor
            .lock()
            .ok()
            .and_then(|mut active| active.take())
    }
}

impl BrowserState {
    pub fn tabs(&self) -> Arc<BrowserTabs> {
        Arc::clone(&self.tabs)
    }

    pub fn waiters(&self) -> Arc<BridgeWaiters> {
        Arc::clone(&self.waiters)
    }

    fn dev_server_monitor(&self) -> Arc<BrowserDevServerMonitorState> {
        Arc::clone(&self.dev_server_monitor)
    }

    pub fn diagnostics(&self) -> Arc<BrowserDiagnostics> {
        Arc::clone(&self.diagnostics)
    }

    pub fn automation(&self) -> Arc<BrowserAutomationState> {
        Arc::clone(&self.automation)
    }

    pub fn native_cdp(&self) -> Arc<NativeCdpBrowserService> {
        Arc::clone(&self.native_cdp)
    }
}

pub fn start_browser_dev_server_reconciler<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: &BrowserState,
) {
    if state
        .dev_server_reconciler_started
        .swap(true, Ordering::AcqRel)
    {
        return;
    }

    let tabs = state.tabs();
    let monitor = state.dev_server_monitor();
    thread::spawn(move || {
        run_browser_dev_server_reconciler(app, tabs, monitor);
    });
}

#[derive(Debug, Clone)]
struct BrowserResizeJob {
    labels: Vec<String>,
    viewport: BrowserViewport,
}

#[derive(Default)]
struct BrowserResizeCoalescer {
    pending: Mutex<Option<BrowserResizeJob>>,
    scheduled: AtomicBool,
}

impl BrowserResizeCoalescer {
    fn schedule<R: Runtime>(
        self: &Arc<Self>,
        app: AppHandle<R>,
        job: BrowserResizeJob,
    ) -> CommandResult<()> {
        {
            let mut pending = self.pending.lock().map_err(|_| {
                CommandError::system_fault(
                    "browser_resize_lock_poisoned",
                    "Browser resize state lock poisoned.",
                )
            })?;
            *pending = Some(job);
        }

        if self.scheduled.swap(true, Ordering::AcqRel) {
            return Ok(());
        }

        let coalescer = Arc::clone(self);
        app.clone()
            .run_on_main_thread(move || coalescer.flush_on_main(app))
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_resize_schedule_failed",
                    format!("Xero could not schedule the browser webview resize: {error}"),
                )
            })?;
        Ok(())
    }

    fn flush_on_main<R: Runtime>(self: Arc<Self>, app: AppHandle<R>) {
        loop {
            let job = self
                .pending
                .lock()
                .ok()
                .and_then(|mut pending| pending.take());

            let Some(job) = job else {
                self.scheduled.store(false, Ordering::Release);
                let has_pending = self
                    .pending
                    .lock()
                    .map(|pending| pending.is_some())
                    .unwrap_or(false);
                if has_pending && !self.scheduled.swap(true, Ordering::AcqRel) {
                    continue;
                }
                return;
            };

            for label in job.labels {
                if let Some(webview) = app.get_webview(&label) {
                    let _ = set_browser_webview_bounds(&webview, job.viewport);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserShowRequest {
    pub project_id: Option<String>,
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
#[allow(clippy::too_many_arguments)]
pub fn browser_show<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    project_id: Option<String>,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tab_id: Option<String>,
    new_tab: Option<bool>,
) -> CommandResult<BrowserTabMetadata> {
    provision_browser_tab(
        &app,
        &state,
        &url,
        tab_id.as_deref(),
        new_tab.unwrap_or(false),
        project_id.as_deref(),
        Some(BrowserViewport {
            x,
            y,
            width,
            height,
        }),
    )
}

pub fn provision_browser_tab<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &BrowserState,
    url: &str,
    requested_tab_id: Option<&str>,
    force_new: bool,
    project_id: Option<&str>,
    viewport: Option<BrowserViewport>,
) -> CommandResult<BrowserTabMetadata> {
    let target = actions::parse_url(url)?;
    let tabs = state.tabs();
    let dev_server_monitor = state.dev_server_monitor();
    let viewport = resolve_browser_viewport(app, viewport);

    let _guard = state.creation_lock.lock().map_err(|_| {
        CommandError::system_fault("browser_lock_poisoned", "Browser state lock poisoned.")
    })?;

    let previous_active = tabs.active_tab_id();
    let mut inserted_tab = false;

    let (tab_id, label) = if force_new {
        inserted_tab = true;
        let (id, label) = tabs.new_tab_label();
        tabs.insert(id.clone(), label.clone(), project_id.map(str::to_string))?;
        (id, label)
    } else {
        match requested_tab_id {
            Some(existing) => (
                existing.to_string(),
                tabs.tab_label_for_project(existing, project_id)?,
            ),
            None => {
                if let Some(active) = tabs.active_tab_id_for_project(project_id) {
                    let label = tabs
                        .tab_label_for_project(&active, project_id)
                        .unwrap_or_else(|_| tab_label_for_id(&active));
                    (active, label)
                } else {
                    inserted_tab = true;
                    let (id, label) = tabs.new_tab_label();
                    tabs.insert(id.clone(), label.clone(), project_id.map(str::to_string))?;
                    (id, label)
                }
            }
        }
    };

    tabs.set_active(&tab_id)?;
    tabs.record_page_state(&tab_id, Some(target.to_string()), None, Some(true));

    if let Err(error) = ensure_browser_webview(
        app,
        &tabs,
        &dev_server_monitor,
        &tab_id,
        &label,
        &target,
        viewport,
    ) {
        dev_server_monitor.cancel(&tab_id);
        if inserted_tab {
            let _ = tabs.remove(&tab_id);
        }
        if let Some(previous) = previous_active.as_deref() {
            let _ = tabs.set_active(previous);
        }
        hide_inactive_webviews(app, &tabs);
        emit_tab_list(app, &tabs);
        return Err(error);
    }

    sync_browser_dev_server_monitor(app, &tabs, &dev_server_monitor, &tab_id, &target);
    hide_inactive_webviews(app, &tabs);
    emit_tab_list(app, &tabs);
    Ok(current_tab_meta(&tabs, &tab_id))
}

fn ensure_browser_webview<R: Runtime + 'static>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    dev_server_monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    label: &str,
    target: &Url,
    viewport: BrowserViewport,
) -> CommandResult<()> {
    let viewport = viewport.sanitize();
    if let Some(existing) = app.get_webview(label) {
        set_browser_webview_bounds(&existing, viewport).map_err(|error| {
            CommandError::system_fault(
                "browser_set_bounds_failed",
                format!("Xero could not position the browser webview: {error}"),
            )
        })?;
        existing.navigate(target.clone()).map_err(|error| {
            CommandError::system_fault(
                "browser_navigate_failed",
                format!("Xero could not navigate the browser webview: {error}"),
            )
        })?;
        #[cfg(debug_assertions)]
        schedule_browser_webview_probe(app, tab_id, label, target, "existing-navigate");
        return Ok(());
    }

    let window = app.get_window(BROWSER_MAIN_WINDOW_LABEL).ok_or_else(|| {
        CommandError::system_fault(
            "browser_main_window_missing",
            "Xero could not locate the main window to attach the browser webview.",
        )
    })?;

    let tab_id_for_nav = tab_id.to_string();
    let tabs_for_nav = Arc::clone(tabs);
    let monitor_for_nav = Arc::clone(dev_server_monitor);
    let app_for_nav = app.clone();

    let tab_id_for_load = tab_id.to_string();
    let tabs_for_load = Arc::clone(tabs);
    let monitor_for_load = Arc::clone(dev_server_monitor);
    let app_for_load = app.clone();

    let about_blank = Url::parse("about:blank").map_err(|error| {
        CommandError::system_fault(
            "browser_blank_url_failed",
            format!("Xero could not prepare the browser webview: {error}"),
        )
    })?;

    let builder = WebviewBuilder::new(label.to_string(), WebviewUrl::External(about_blank))
        .initialization_script(BROWSER_BRIDGE_INIT_SCRIPT)
        .on_navigation(move |url| {
            let url = url.to_string();
            #[cfg(debug_assertions)]
            append_browser_probe_log(format!("on_navigation tab={tab_id_for_nav} url={url}"));
            tabs_for_nav.record_page_state(&tab_id_for_nav, Some(url.clone()), None, Some(true));
            if let Ok(target) = actions::parse_url(&url) {
                sync_browser_dev_server_monitor(
                    &app_for_nav,
                    &tabs_for_nav,
                    &monitor_for_nav,
                    &tab_id_for_nav,
                    &target,
                );
            } else {
                monitor_for_nav.cancel(&tab_id_for_nav);
            }
            events::emit(
                &app_for_nav,
                BROWSER_URL_CHANGED_EVENT,
                &BrowserUrlChangedPayload {
                    tab_id: tab_id_for_nav.clone(),
                    url,
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
            #[cfg(debug_assertions)]
            {
                let event_name = if loading { "started" } else { "finished" };
                append_browser_probe_log(format!(
                    "on_page_load tab={tab_id_for_load} event={event_name} url={url}"
                ));
            }
            tabs_for_load.record_page_state(
                &tab_id_for_load,
                Some(url.clone()),
                None,
                Some(loading),
            );
            if let Ok(target) = actions::parse_url(&url) {
                sync_browser_dev_server_monitor(
                    &app_for_load,
                    &tabs_for_load,
                    &monitor_for_load,
                    &tab_id_for_load,
                    &target,
                );
            } else {
                monitor_for_load.cancel(&tab_id_for_load);
            }
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
            emit_tab_list(&app_for_load, &tabs_for_load);
        });

    window
        .add_child(
            builder,
            LogicalPosition::new(viewport.x, viewport.y),
            LogicalSize::new(viewport.width, viewport.height),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "browser_create_failed",
                format!("Xero could not create the browser webview: {error}"),
            )
        })?;

    let webview = app.get_webview(label).ok_or_else(|| {
        CommandError::system_fault(
            "browser_create_missing",
            "Xero created the browser webview but could not attach to it.",
        )
    })?;
    set_browser_webview_bounds(&webview, viewport).map_err(|error| {
        CommandError::system_fault(
            "browser_set_bounds_failed",
            format!("Xero could not position the browser webview: {error}"),
        )
    })?;
    webview.navigate(target.clone()).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Xero could not navigate the browser webview: {error}"),
        )
    })?;
    #[cfg(debug_assertions)]
    schedule_browser_webview_probe(app, tab_id, label, target, "created");
    Ok(())
}

fn set_browser_webview_bounds<R: Runtime>(
    webview: &Webview<R>,
    viewport: BrowserViewport,
) -> tauri::Result<()> {
    let viewport = viewport.sanitize();
    webview.set_bounds(viewport.as_rect())?;

    #[cfg(target_os = "macos")]
    {
        let _ = webview.with_webview(move |platform_webview| {
            let raw_webview = platform_webview.inner();
            let ns_view = unsafe { &*(raw_webview.cast::<NSView>()) };
            apply_macos_browser_webview_frame(ns_view, viewport);
        });
    }

    let _ = webview.eval(BROWSER_RESIZE_NUDGE_SCRIPT);

    Ok(())
}

fn set_browser_webview_occlusion_regions<R: Runtime>(
    webview: &Webview<R>,
    rects: &[BrowserOcclusionRect],
) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let rects = rects.to_vec();
        webview.with_webview(move |platform_webview| {
            let raw_webview = platform_webview.inner();
            if raw_webview.is_null() {
                return;
            }

            let ns_view = unsafe { &*(raw_webview.cast::<NSView>()) };
            apply_macos_browser_webview_occlusion_regions(ns_view, &rects);
        })?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = rects;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_macos_browser_occlusion_wheel_monitor<R: Runtime + 'static>(
    app: &AppHandle<R>,
    webview: &Webview<R>,
    state: Arc<BrowserOcclusionWheelState>,
    rects: &[BrowserOcclusionRect],
) -> tauri::Result<()> {
    if rects.is_empty() {
        cleanup_macos_browser_occlusion_wheel_monitor(app, &state);
        return Ok(());
    }

    let app = app.clone();
    let rects = rects.to_vec();
    webview.with_webview(move |platform_webview| {
        let raw_webview = platform_webview.inner();
        let raw_window = platform_webview.ns_window();
        if raw_webview.is_null() || raw_window.is_null() {
            if let Some(previous) = state.take_native_monitor() {
                unsafe { remove_macos_browser_occlusion_wheel_monitor(previous) };
            }
            if let Some(previous) = state.take_native_click_monitor() {
                unsafe { remove_macos_browser_occlusion_click_monitor(previous) };
            }
            return;
        }

        let ns_view = unsafe { &*(raw_webview.cast::<NSView>()) };
        let native_rects = macos_browser_occlusion_wheel_rects(ns_view, &rects);
        if native_rects.is_empty() {
            if let Some(previous) = state.take_native_monitor() {
                unsafe { remove_macos_browser_occlusion_wheel_monitor(previous) };
            }
            if let Some(previous) = state.take_native_click_monitor() {
                unsafe { remove_macos_browser_occlusion_click_monitor(previous) };
            }
            return;
        }

        let ns_view_ptr = raw_webview.cast::<NSView>() as usize;
        let ns_window = unsafe { &*(raw_window.cast::<NSWindow>()) };
        let ns_window_number = ns_window.windowNumber();
        let app_for_wheel = app.clone();
        let native_rects_for_wheel = native_rects.clone();
        let block =
            RcBlock::<BrowserNativeOcclusionWheelHandler>::new(move |event: NonNull<NSEvent>| {
                let event_ref = unsafe { event.as_ref() };
                let Some((x, y)) = macos_browser_occlusion_wheel_event_location(
                    event_ref,
                    ns_view_ptr,
                    ns_window_number,
                    &native_rects_for_wheel,
                ) else {
                    return event.as_ptr();
                };

                let (delta_x, delta_y) = macos_browser_occlusion_wheel_delta(event_ref);
                if delta_x != 0.0 || delta_y != 0.0 {
                    events::emit(
                        &app_for_wheel,
                        BROWSER_OCCLUSION_WHEEL_EVENT,
                        &BrowserOcclusionWheelPayload {
                            x,
                            y,
                            delta_x,
                            delta_y,
                        },
                    );
                }
                std::ptr::null_mut()
            });

        let Some(event_monitor) = (unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::ScrollWheel, &block)
        }) else {
            return;
        };
        let monitor = BrowserNativeOcclusionWheelMonitor {
            event_monitor: Retained::into_raw(event_monitor) as usize,
            block: RcBlock::into_raw(block) as usize,
        };

        if let Some(previous) = state.replace_native_monitor(monitor) {
            unsafe { remove_macos_browser_occlusion_wheel_monitor(previous) };
        }

        let app_for_click = app.clone();
        let native_rects_for_click = native_rects;
        let click_block =
            RcBlock::<BrowserNativeOcclusionClickHandler>::new(move |event: NonNull<NSEvent>| {
                let event_ref = unsafe { event.as_ref() };
                let Some((x, y)) = macos_browser_occlusion_wheel_event_location(
                    event_ref,
                    ns_view_ptr,
                    ns_window_number,
                    &native_rects_for_click,
                ) else {
                    return event.as_ptr();
                };

                events::emit(
                    &app_for_click,
                    BROWSER_OCCLUSION_CLICK_EVENT,
                    &BrowserOcclusionClickPayload { x, y },
                );
                std::ptr::null_mut()
            });

        let click_mask = NSEventMask(NSEventMask::LeftMouseDown.0);
        let Some(click_event_monitor) = (unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(click_mask, &click_block)
        }) else {
            if let Some(previous) = state.take_native_click_monitor() {
                unsafe { remove_macos_browser_occlusion_click_monitor(previous) };
            }
            return;
        };
        let click_monitor = BrowserNativeOcclusionClickMonitor {
            event_monitor: Retained::into_raw(click_event_monitor) as usize,
            block: RcBlock::into_raw(click_block) as usize,
        };

        if let Some(previous) = state.replace_native_click_monitor(click_monitor) {
            unsafe { remove_macos_browser_occlusion_click_monitor(previous) };
        }
    })
}

#[cfg(target_os = "macos")]
fn cleanup_macos_browser_occlusion_wheel_monitor<R: Runtime>(
    app: &AppHandle<R>,
    state: &BrowserOcclusionWheelState,
) {
    let monitor = state.take_native_monitor();
    let click_monitor = state.take_native_click_monitor();
    if monitor.is_none() && click_monitor.is_none() {
        return;
    }

    let _ = app.run_on_main_thread(move || unsafe {
        if let Some(monitor) = monitor {
            remove_macos_browser_occlusion_wheel_monitor(monitor);
        }
        if let Some(click_monitor) = click_monitor {
            remove_macos_browser_occlusion_click_monitor(click_monitor);
        }
    });
}

#[cfg(target_os = "macos")]
fn macos_browser_occlusion_wheel_rects(
    ns_view: &NSView,
    rects: &[BrowserOcclusionRect],
) -> Vec<BrowserNativeOcclusionWheelRect> {
    let bounds = ns_view.bounds();
    let viewport = BrowserViewport {
        x: 0.0,
        y: 0.0,
        width: bounds.size.width.max(1.0),
        height: bounds.size.height.max(1.0),
    }
    .sanitize();

    rects
        .iter()
        .filter_map(|rect| rect.sanitize(viewport))
        .map(|rect| BrowserNativeOcclusionWheelRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_browser_occlusion_wheel_event_location(
    event: &NSEvent,
    ns_view_ptr: usize,
    ns_window_number: isize,
    rects: &[BrowserNativeOcclusionWheelRect],
) -> Option<(f64, f64)> {
    if event.windowNumber() != ns_window_number {
        return None;
    }

    let ns_view = unsafe { &*(ns_view_ptr as *const NSView) };
    let bounds = ns_view.bounds();
    let point = ns_view.convertPoint_fromView(event.locationInWindow(), None);
    let local_x = point.x;
    let local_y = if ns_view.isFlipped() {
        point.y
    } else {
        bounds.size.height - point.y
    };

    if local_x < 0.0 || local_x > bounds.size.width || local_y < 0.0 || local_y > bounds.size.height
    {
        return None;
    }

    rects
        .iter()
        .any(|rect| rect.contains(local_x, local_y))
        .then_some((local_x, local_y))
}

#[cfg(target_os = "macos")]
fn macos_browser_occlusion_wheel_delta(event: &NSEvent) -> (f64, f64) {
    let scale = if event.hasPreciseScrollingDeltas() {
        1.0
    } else {
        16.0
    };
    (
        -event.scrollingDeltaX() * scale,
        -event.scrollingDeltaY() * scale,
    )
}

#[cfg(target_os = "macos")]
unsafe fn remove_macos_browser_occlusion_wheel_monitor(
    monitor: BrowserNativeOcclusionWheelMonitor,
) {
    let event_monitor_ptr = monitor.event_monitor as *mut AnyObject;
    if !event_monitor_ptr.is_null() {
        if let Some(event_monitor) = unsafe { Retained::from_raw(event_monitor_ptr) } {
            unsafe { NSEvent::removeMonitor(&event_monitor) };
        }
    }

    let block_ptr = monitor.block as *mut Block<BrowserNativeOcclusionWheelHandler>;
    let _ = unsafe { RcBlock::<BrowserNativeOcclusionWheelHandler>::from_raw(block_ptr) };
}

#[cfg(target_os = "macos")]
unsafe fn remove_macos_browser_occlusion_click_monitor(
    monitor: BrowserNativeOcclusionClickMonitor,
) {
    let event_monitor_ptr = monitor.event_monitor as *mut AnyObject;
    if !event_monitor_ptr.is_null() {
        if let Some(event_monitor) = unsafe { Retained::from_raw(event_monitor_ptr) } {
            unsafe { NSEvent::removeMonitor(&event_monitor) };
        }
    }

    let block_ptr = monitor.block as *mut Block<BrowserNativeOcclusionClickHandler>;
    let _ = unsafe { RcBlock::<BrowserNativeOcclusionClickHandler>::from_raw(block_ptr) };
}

#[cfg(target_os = "macos")]
fn install_native_resize_drag_monitor<R: Runtime>(
    app: AppHandle<R>,
    webview: &Webview<R>,
    resize_drag: Arc<BrowserResizeDragState>,
    session_id: u64,
    session: BrowserResizeDragSession,
) -> tauri::Result<()> {
    webview.with_webview(move |platform_webview| {
        let raw_webview = platform_webview.inner();
        let raw_window = platform_webview.ns_window();
        if raw_webview.is_null() || raw_window.is_null() {
            return;
        }

        let app_for_drag = app.clone();
        let ns_view_ptr = raw_webview.cast::<NSView>() as usize;
        let ns_window_ptr = raw_window.cast::<NSWindow>() as usize;
        let monitor_session = session.clone();
        let block =
            RcBlock::<BrowserNativeResizeDragTimerHandler>::new(move |timer: NonNull<NSTimer>| {
                let ns_window = unsafe { &*(ns_window_ptr as *const NSWindow) };
                let location = ns_window.mouseLocationOutsideOfEventStream();
                let sidebar_width = monitor_session.width_for_cursor(location.x);
                let viewport = monitor_session.viewport_for_cursor(location.x);
                let ns_view = unsafe { &*(ns_view_ptr as *const NSView) };
                apply_macos_browser_webview_frame(ns_view, viewport);

                let complete = (NSEvent::pressedMouseButtons() & 1) == 0;
                events::emit(
                    &app_for_drag,
                    BROWSER_RESIZE_DRAG_EVENT,
                    &BrowserResizeDragPayload {
                        tab_id: monitor_session.tab_id.clone(),
                        sidebar_width,
                        x: viewport.x,
                        y: viewport.y,
                        width: viewport.width,
                        height: viewport.height,
                        complete,
                    },
                );

                if complete {
                    let timer = unsafe { timer.as_ref() };
                    timer.invalidate();
                }
            });
        let timer =
            unsafe { NSTimer::timerWithTimeInterval_repeats_block(1.0 / 120.0, true, &block) };
        let run_loop = NSRunLoop::mainRunLoop();
        unsafe {
            run_loop.addTimer_forMode(&timer, NSEventTrackingRunLoopMode);
            run_loop.addTimer_forMode(&timer, NSRunLoopCommonModes);
        }
        timer.fire();
        let monitor = BrowserNativeResizeDragMonitor {
            timer: Retained::into_raw(timer) as usize,
            block: RcBlock::into_raw(block) as usize,
        };

        if let Some(previous) = resize_drag.replace_native_monitor_if_current(session_id, monitor) {
            unsafe { remove_native_resize_drag_monitor(previous) };
        }
    })
}

#[cfg(target_os = "macos")]
fn cleanup_native_resize_drag_monitor<R: Runtime>(
    app: &AppHandle<R>,
    resize_drag: &BrowserResizeDragState,
) {
    let Some(monitor) = resize_drag.take_native_monitor() else {
        return;
    };

    let _ = app.run_on_main_thread(move || unsafe {
        remove_native_resize_drag_monitor(monitor);
    });
}

#[cfg(not(target_os = "macos"))]
fn cleanup_native_resize_drag_monitor<R: Runtime>(
    _app: &AppHandle<R>,
    _resize_drag: &BrowserResizeDragState,
) {
}

#[cfg(target_os = "macos")]
unsafe fn remove_native_resize_drag_monitor(monitor: BrowserNativeResizeDragMonitor) {
    let timer_ptr = monitor.timer as *mut NSTimer;
    if !timer_ptr.is_null() {
        if let Some(timer) = unsafe { Retained::from_raw(timer_ptr) } {
            timer.invalidate();
        }
    }

    let block_ptr = monitor.block as *mut Block<BrowserNativeResizeDragTimerHandler>;
    let _ = unsafe { RcBlock::<BrowserNativeResizeDragTimerHandler>::from_raw(block_ptr) };
}

#[cfg(target_os = "macos")]
fn apply_macos_browser_webview_frame(ns_view: &NSView, viewport: BrowserViewport) {
    let Some(parent_view) = (unsafe { ns_view.superview() }) else {
        return;
    };

    let frame = NSRect::new(
        macos_browser_webview_origin(&parent_view, viewport),
        NSSize::new(viewport.width.round(), viewport.height.round()),
    );

    ns_view.setFrame(frame);
    ns_view.setNeedsLayout(true);
    ns_view.layoutSubtreeIfNeeded();
    ns_view.setNeedsDisplay(true);
    ns_view.displayIfNeeded();

    parent_view.setNeedsLayout(true);
    parent_view.layoutSubtreeIfNeeded();
    parent_view.setNeedsDisplay(true);
}

#[cfg(target_os = "macos")]
fn apply_macos_browser_webview_occlusion_regions(ns_view: &NSView, rects: &[BrowserOcclusionRect]) {
    ns_view.setWantsLayer(true);
    let Some(layer) = ns_view.layer() else {
        return;
    };

    if rects.is_empty() {
        unsafe { layer.setMask(None) };
        return;
    }

    let bounds = ns_view.bounds();
    let viewport = BrowserViewport {
        x: 0.0,
        y: 0.0,
        width: bounds.size.width.max(1.0),
        height: bounds.size.height.max(1.0),
    }
    .sanitize();

    let sanitized: Vec<_> = rects
        .iter()
        .filter_map(|rect| rect.sanitize(viewport))
        .collect();
    if sanitized.is_empty() {
        unsafe { layer.setMask(None) };
        return;
    }

    let path = CGMutablePath::new();
    let mask_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(viewport.width, viewport.height),
    );

    unsafe {
        CGMutablePath::add_rect(Some(&path), std::ptr::null(), mask_frame);
    }

    for rect in sanitized {
        let y = if ns_view.isFlipped() {
            rect.y
        } else {
            viewport.height - rect.y - rect.height
        };
        let occlusion = NSRect::new(
            NSPoint::new(rect.x, y),
            NSSize::new(rect.width, rect.height),
        );
        unsafe {
            CGMutablePath::add_rect(Some(&path), std::ptr::null(), occlusion);
        }
    }

    CATransaction::begin();
    CATransaction::setDisableActions(true);

    let mask = CAShapeLayer::layer();
    mask.as_super().setFrame(mask_frame);
    mask.setFillRule(unsafe { kCAFillRuleEvenOdd });
    mask.setPath(
        CGMutablePath::new_copy(Some(&path))
            .as_deref()
            .map(|path| &**path),
    );
    unsafe {
        layer.setMask(Some(mask.as_super()));
    }

    CATransaction::commit();
}

#[cfg(target_os = "macos")]
fn macos_browser_webview_origin(parent_view: &NSView, viewport: BrowserViewport) -> NSPoint {
    let x = viewport.x.round();
    let y = viewport.y.round();
    if parent_view.isFlipped() {
        return NSPoint::new(x, y);
    }

    let parent_frame = parent_view.frame();
    NSPoint::new(x, parent_frame.size.height - y - viewport.height.round())
}

fn resolve_browser_viewport<R: Runtime>(
    app: &AppHandle<R>,
    viewport: Option<BrowserViewport>,
) -> BrowserViewport {
    if let Some(viewport) = viewport {
        return viewport.sanitize();
    }

    if let Some(window) = app.get_window(BROWSER_MAIN_WINDOW_LABEL) {
        if let Ok(size) = window.inner_size() {
            return BrowserViewport {
                x: 0.0,
                y: 0.0,
                width: size.width as f64,
                height: size.height as f64,
            }
            .sanitize();
        }
    }

    BrowserViewport {
        x: 0.0,
        y: 0.0,
        width: DEFAULT_AUTONOMOUS_BROWSER_WIDTH,
        height: DEFAULT_AUTONOMOUS_BROWSER_HEIGHT,
    }
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
    let labels = resolve_resize_labels(&app, &tabs, tab_id.as_deref())?;
    let viewport = BrowserViewport {
        x,
        y,
        width,
        height,
    };
    state
        .resize_coalescer
        .schedule(app, BrowserResizeJob { labels, viewport })
}

#[tauri::command]
pub fn browser_set_occlusion_regions<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    rects: Vec<BrowserOcclusionRect>,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let tabs = state.tabs();
    let labels = resolve_resize_labels(&app, &tabs, tab_id.as_deref()).or_else(|error| {
        if rects.is_empty() {
            Ok(Vec::new())
        } else {
            Err(error)
        }
    })?;

    #[cfg(target_os = "macos")]
    let monitor_label = labels.first().cloned();

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            set_browser_webview_occlusion_regions(&webview, &rects).map_err(|error| {
                CommandError::system_fault(
                    "browser_set_occlusion_regions_failed",
                    format!("Xero could not update the browser webview overlay mask: {error}"),
                )
            })?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(webview) = monitor_label.and_then(|label| app.get_webview(&label)) {
            sync_macos_browser_occlusion_wheel_monitor(
                &app,
                &webview,
                Arc::clone(&state.occlusion_wheel),
                &rects,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "browser_set_occlusion_regions_failed",
                    format!(
                        "Xero could not update the browser webview overlay input mask: {error}"
                    ),
                )
            })?;
        } else if rects.is_empty() {
            cleanup_macos_browser_occlusion_wheel_monitor(&app, &state.occlusion_wheel);
        }
    }

    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn browser_resize_drag_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    start_client_x: f64,
    start_width: f64,
    right: f64,
    top: f64,
    height: f64,
    min_width: f64,
    max_width: f64,
    inset: Option<f64>,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let tabs = state.tabs();
    let labels = resolve_resize_labels(&app, &tabs, tab_id.as_deref())?;
    validate_resize_drag_number("startClientX", start_client_x)?;
    validate_resize_drag_number("startWidth", start_width)?;
    validate_resize_drag_number("right", right)?;
    validate_resize_drag_number("top", top)?;
    validate_resize_drag_number("height", height)?;
    validate_resize_drag_number("minWidth", min_width)?;
    validate_resize_drag_number("maxWidth", max_width)?;
    let inset = inset.unwrap_or(0.0);
    validate_resize_drag_number("inset", inset)?;

    let min_width = min_width.max(1.0);
    let max_width = max_width.max(min_width);
    let session = BrowserResizeDragSession {
        id: 0,
        labels,
        tab_id,
        start_client_x,
        start_width: start_width.clamp(min_width, max_width),
        right,
        top,
        height: height.max(1.0),
        min_width,
        max_width,
        inset: inset.max(0.0),
    };
    let id = state.resize_drag.begin(session.clone())?;

    #[cfg(target_os = "macos")]
    if let Some(label) = session.labels.first() {
        if let Some(webview) = app.get_webview(label) {
            let _ = install_native_resize_drag_monitor(
                app.clone(),
                &webview,
                Arc::clone(&state.resize_drag),
                id,
                session.clone(),
            );
        }
    }

    let resize_drag = Arc::clone(&state.resize_drag);
    let app_for_timeout = app.clone();

    std::thread::spawn(move || {
        std::thread::sleep(RESIZE_DRAG_MAX_DURATION);
        if resize_drag.take_if_current(id).is_some() {
            cleanup_native_resize_drag_monitor(&app_for_timeout, &resize_drag);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn browser_resize_drag_end<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    tab_id: Option<String>,
) -> CommandResult<()> {
    let session = state.resize_drag.end()?;
    cleanup_native_resize_drag_monitor(&app, &state.resize_drag);
    let labels = match session {
        Some(session) => session.labels,
        None => {
            let tabs = state.tabs();
            resolve_resize_labels(&app, &tabs, tab_id.as_deref())?
        }
    };
    let viewport = match (x, y, width, height) {
        (Some(x), Some(y), Some(width), Some(height)) => {
            validate_resize_drag_number("x", x)?;
            validate_resize_drag_number("y", y)?;
            validate_resize_drag_number("width", width)?;
            validate_resize_drag_number("height", height)?;
            Some(BrowserViewport {
                x,
                y,
                width,
                height,
            })
        }
        (None, None, None, None) => None,
        _ => {
            return Err(CommandError::user_fixable(
                "invalid_browser_resize_drag_end",
                "Browser resize drag end must include a complete viewport.",
            ));
        }
    };

    let Some(viewport) = viewport else {
        return Ok(());
    };

    let mut first_error: Option<tauri::Error> = None;
    for label in labels {
        let Some(webview) = app.get_webview(&label) else {
            continue;
        };
        if let Err(error) = set_browser_webview_bounds(&webview, viewport) {
            first_error.get_or_insert(error);
        }
    }

    match first_error {
        Some(error) => Err(CommandError::system_fault(
            "browser_resize_drag_end_failed",
            format!("Xero could not finish resizing the browser webview: {error}"),
        )),
        None => Ok(()),
    }
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
                        format!("Xero could not hide the browser webview: {error}"),
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
    bridge::run_script(
        &app,
        &tabs,
        &waiters,
        &body,
        actions::resolve_timeout(timeout_ms),
    )
}

#[tauri::command]
pub fn browser_eval_fire_and_forget<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    js: String,
) -> CommandResult<()> {
    if js.trim().is_empty() {
        return Err(CommandError::invalid_request("js"));
    }
    let webview = state.tabs().active_webview(&app)?;
    webview.eval(&js).map_err(|error| {
        CommandError::system_fault(
            "browser_eval_fire_and_forget_failed",
            format!("Xero could not evaluate the browser script: {error}"),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn browser_current_url<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<Option<String>> {
    let tabs = state.tabs();
    let Some(_) = tabs.optional_active_webview(&app) else {
        return Ok(None);
    };
    Ok(tabs.active_url())
}

#[tauri::command]
pub async fn browser_dev_server_running(url: String) -> CommandResult<bool> {
    let target = match actions::parse_url(&url) {
        Ok(target) => target,
        Err(_) => return Ok(false),
    };
    if browser_dev_server_origin_key(&target).is_none() {
        return Ok(false);
    }

    tauri::async_runtime::spawn_blocking(move || {
        browser_dev_server_accepts_connections(&target, DEV_SERVER_LIVENESS_CONNECT_TIMEOUT)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "browser_dev_server_liveness_task_failed",
            format!("Xero could not check project app availability: {error}"),
        )
    })
}

#[tauri::command]
pub async fn browser_list_running_dev_servers() -> CommandResult<Vec<BrowserRunningDevServerDto>> {
    tauri::async_runtime::spawn_blocking(list_running_browser_dev_servers_blocking)
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "browser_running_dev_servers_task_failed",
                format!("Xero could not list running local dev servers: {error}"),
            )
        })?
}

#[tauri::command]
pub async fn browser_screenshot<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<String> {
    let webview = state.tabs().active_webview(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _perf = crate::perf::PerfSpan::new("browser_screenshot");
        screenshot::capture_webview(&webview)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_task_failed",
            format!("Xero could not capture the browser in the background: {error}"),
        )
    })?
}

#[tauri::command]
pub fn browser_navigate<R: Runtime + 'static>(
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
    webview.navigate(target.clone()).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Xero could not navigate the browser webview: {error}"),
        )
    })?;
    #[cfg(debug_assertions)]
    if let Some(tab_id) = tabs.find_by_label(&label) {
        schedule_browser_webview_probe(&app, &tab_id, &label, &target, "command-navigate");
    }
    if let Some(tab_id) = tabs.find_by_label(&label) {
        sync_browser_dev_server_monitor(&app, &tabs, &state.dev_server_monitor, &tab_id, &target);
    }
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
pub fn browser_reload<R: Runtime + 'static>(
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
    let current = tabs.url_by_label(&label).ok_or_else(|| {
        CommandError::user_fixable(
            "browser_url_unavailable",
            "The in-app browser does not have a URL to reload yet.",
        )
    })?;
    let current = actions::parse_url(&current)?;
    if browser_dev_server_origin_key(&current).is_some()
        && !browser_dev_server_accepts_connections(&current, DEV_SERVER_MONITOR_CONNECT_TIMEOUT)
    {
        if let Some(tab_id) = tabs.find_by_label(&label) {
            state.dev_server_monitor.cancel(&tab_id);
            emit_browser_dev_server_unavailable(&app, &tab_id, &current);
            close_browser_tab(&app, &tabs, &tab_id)?;
        }
        return Ok(());
    }
    webview.navigate(current.clone()).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Xero could not reload the browser webview: {error}"),
        )
    })?;
    #[cfg(debug_assertions)]
    if let Some(tab_id) = tabs.find_by_label(&label) {
        schedule_browser_webview_probe(&app, &tab_id, &label, &current, "command-reload");
    }
    if let Some(tab_id) = tabs.find_by_label(&label) {
        sync_browser_dev_server_monitor(&app, &tabs, &state.dev_server_monitor, &tab_id, &current);
    }
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
    actions::click(&app, &state.tabs(), &state.waiters(), &selector, timeout_ms)
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
    actions::storage_read(&app, &state.tabs(), &state.waiters(), area, key.as_deref())
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
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    project_id: Option<String>,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    let tabs = state.tabs();
    prune_unavailable_dev_server_tabs(
        &app,
        &tabs,
        &state.dev_server_monitor,
        DEV_SERVER_LIVENESS_CONNECT_TIMEOUT,
    );
    tabs.activate_project(project_id.as_deref())?;
    hide_inactive_webviews(&app, &tabs);
    emit_tab_list(&app, &tabs);
    tabs.list_for_project(project_id.as_deref())
}

#[tauri::command]
pub fn browser_tab_focus<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: String,
    project_id: Option<String>,
) -> CommandResult<BrowserTabMetadata> {
    let tabs = state.tabs();
    tabs.tab_label_for_project(&tab_id, project_id.as_deref())?;
    tabs.set_active(&tab_id)?;
    hide_inactive_webviews(&app, &tabs);
    emit_tab_list(&app, &tabs);
    Ok(current_tab_meta(&tabs, &tab_id))
}

#[tauri::command]
pub fn browser_tab_close<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    tab_id: String,
    project_id: Option<String>,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    let tabs = state.tabs();
    tabs.tab_label_for_project(&tab_id, project_id.as_deref())?;
    state.dev_server_monitor.cancel(&tab_id);
    close_browser_tab(&app, &tabs, &tab_id)?;
    tabs.activate_project(project_id.as_deref())?;
    hide_inactive_webviews(&app, &tabs);
    emit_tab_list(&app, &tabs);
    tabs.list_for_project(project_id.as_deref())
}

#[tauri::command]
pub fn browser_tab_reorder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    active_tab_id: String,
    over_tab_id: String,
    project_id: Option<String>,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    let tabs = state.tabs();
    tabs.reorder_for_project(&active_tab_id, &over_tab_id, project_id.as_deref())?;
    emit_tab_list(&app, &tabs);
    tabs.list_for_project(project_id.as_deref())
}

fn close_browser_tab<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    tab_id: &str,
) -> CommandResult<Vec<BrowserTabMetadata>> {
    let removed_label = tabs.remove(tab_id)?;
    if let Some(label) = removed_label {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
    }
    hide_inactive_webviews(&app, &tabs);
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
    let parsed: JsonValue = payload
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(JsonValue::Null);
    let Some(tab_id) = state.tabs().active_tab_id() else {
        return Ok(());
    };

    match kind.as_str() {
        "page" => {
            let url = parsed.get("url").and_then(|v| v.as_str()).map(String::from);
            let title = parsed
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from);
            let ready_state = parsed
                .get("readyState")
                .and_then(|v| v.as_str())
                .unwrap_or("loading");
            let loading = ready_state != "complete";
            state
                .tabs()
                .record_page_state(&tab_id, url.clone(), title.clone(), Some(loading));
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
            state
                .diagnostics()
                .push_console(&tab_id, &level, &message)?;
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
            state
                .diagnostics()
                .push_console(&tab_id, "error", &message)?;
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
        "network" => {
            state.diagnostics().push_network(&tab_id, &parsed)?;
        }
        "tool_context" => {
            events::emit(
                &app,
                BROWSER_TOOL_CONTEXT_EVENT,
                &BrowserToolContextPayload {
                    tab_id,
                    context: parsed,
                },
            );
        }
        "tool_closed" => {
            let mode = parsed
                .get("mode")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            events::emit(
                &app,
                BROWSER_TOOL_CLOSED_EVENT,
                &BrowserToolClosedPayload { tab_id, mode },
            );
        }
        "tool_state" => {
            let mode = parsed
                .get("mode")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let stroke_count = parsed
                .get("strokeCount")
                .or_else(|| parsed.get("stroke_count"))
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let has_drawing = parsed
                .get("hasDrawing")
                .or_else(|| parsed.get("has_drawing"))
                .and_then(|value| value.as_bool())
                .unwrap_or(stroke_count > 0);
            events::emit(
                &app,
                BROWSER_TOOL_STATE_EVENT,
                &BrowserToolStatePayload {
                    tab_id,
                    mode,
                    stroke_count,
                    has_drawing,
                },
            );
        }
        "tool_note" => {
            let mode = parsed
                .get("mode")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let note = parsed
                .get("note")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let active = parsed
                .get("active")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            events::emit(
                &app,
                BROWSER_TOOL_NOTE_EVENT,
                &BrowserToolNotePayload {
                    tab_id,
                    mode,
                    note,
                    active,
                },
            );
        }
        "tool_dictation_toggle" => {
            let mode = parsed
                .get("mode")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let note = parsed
                .get("note")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            events::emit(
                &app,
                BROWSER_TOOL_DICTATION_TOGGLE_EVENT,
                &BrowserToolDictationTogglePayload { tab_id, mode, note },
            );
        }
        _ => {}
    }
    Ok(())
}

fn resolve_label(tabs: &BrowserTabs, requested: Option<&str>) -> CommandResult<String> {
    match requested {
        Some(id) => tabs.tab_label(id),
        None => tabs.active_label_soft().ok_or_else(|| {
            CommandError::user_fixable(
                "browser_not_open",
                "The in-app browser is not currently open.",
            )
        }),
    }
}

fn resolve_resize_labels<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &BrowserTabs,
    requested: Option<&str>,
) -> CommandResult<Vec<String>> {
    let mut labels = Vec::new();
    if let Ok(label) = resolve_label(tabs, requested) {
        push_unique_label(&mut labels, label);
    }
    if let Some(label) = tabs.active_label_soft() {
        push_unique_label(&mut labels, label);
    }
    for label in visible_browser_webview_labels(app) {
        push_unique_label(&mut labels, label);
    }

    if labels.is_empty() {
        return Err(CommandError::user_fixable(
            "browser_not_open",
            "The in-app browser is not currently open.",
        ));
    }

    Ok(labels)
}

fn push_unique_label(labels: &mut Vec<String>, label: String) {
    if labels.iter().any(|candidate| candidate == &label) {
        return;
    }
    labels.push(label);
}

fn visible_browser_webview_labels<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    let Some(window) = app.get_window(BROWSER_MAIN_WINDOW_LABEL) else {
        return Vec::new();
    };
    let scale_factor = window
        .scale_factor()
        .ok()
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0);

    app.webviews()
        .into_iter()
        .filter_map(|(label, webview)| {
            if !label.starts_with(BROWSER_TAB_PREFIX) {
                return None;
            }
            let bounds = webview.bounds().ok()?;
            let position = bounds.position.to_logical::<f64>(scale_factor);
            if position.x <= HIDDEN_OFFSET / 2.0 || position.y <= HIDDEN_OFFSET / 2.0 {
                return None;
            }
            Some(label)
        })
        .collect()
}

fn validate_resize_drag_number(field: &'static str, value: f64) -> CommandResult<()> {
    if value.is_finite() {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "invalid_browser_resize_drag",
        format!("Field `{field}` must be a finite number."),
    ))
}

fn current_tab_meta(tabs: &BrowserTabs, id: &str) -> BrowserTabMetadata {
    tabs.list()
        .ok()
        .and_then(|list| list.into_iter().find(|tab| tab.id == id))
        .unwrap_or(BrowserTabMetadata {
            id: id.to_string(),
            project_id: tabs.tab_project_id(id),
            label: tab_label_for_id(id),
            title: None,
            url: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            active: true,
        })
}

fn tab_label_for_id(id: &str) -> String {
    let suffix = id.strip_prefix("tab-").unwrap_or(id);
    format!("{BROWSER_TAB_PREFIX}{suffix}")
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

fn emit_browser_dev_server_unavailable<R: Runtime>(app: &AppHandle<R>, tab_id: &str, url: &Url) {
    events::emit(
        app,
        BROWSER_DEV_SERVER_UNAVAILABLE_EVENT,
        &BrowserDevServerUnavailablePayload {
            tab_id: tab_id.to_string(),
            url: url.to_string(),
        },
    );
}

#[derive(Debug, Clone)]
struct BrowserDevServerTabSnapshot {
    tab_id: String,
    url: Url,
}

fn browser_dev_server_tab_snapshots(tabs: &BrowserTabs) -> Vec<BrowserDevServerTabSnapshot> {
    let Ok(list) = tabs.list() else {
        return Vec::new();
    };

    list.into_iter()
        .filter_map(|tab| {
            let url = tab.url.as_deref()?;
            let url = actions::parse_url(url).ok()?;
            browser_dev_server_origin_key(&url)?;
            Some(BrowserDevServerTabSnapshot {
                tab_id: tab.id,
                url,
            })
        })
        .collect()
}

fn record_browser_dev_server_probe_result(
    failures: &mut HashMap<String, u8>,
    tab_id: &str,
    running: bool,
    threshold: u8,
) -> bool {
    if running {
        failures.remove(tab_id);
        return false;
    }

    let count = failures.entry(tab_id.to_string()).or_insert(0);
    *count = count.saturating_add(1);
    *count >= threshold
}

fn prune_unavailable_dev_server_tabs<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    timeout: Duration,
) {
    for snapshot in browser_dev_server_tab_snapshots(tabs) {
        if browser_dev_server_accepts_connections(&snapshot.url, timeout) {
            continue;
        }
        close_unavailable_dev_server_tab(app, tabs, monitor, &snapshot.tab_id, &snapshot.url);
    }
}

fn close_unavailable_dev_server_tab<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    unavailable_url: &Url,
) {
    monitor.cancel(tab_id);
    emit_browser_dev_server_unavailable(app, tab_id, unavailable_url);
    let _ = close_browser_tab(app, tabs, tab_id);
}

fn close_dev_server_tab_if_current_url<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    unavailable_url: &Url,
) {
    let Some(expected_origin) = browser_dev_server_origin_key(unavailable_url) else {
        return;
    };
    let current_matches = tabs
        .url_by_id(tab_id)
        .and_then(|url| actions::parse_url(&url).ok())
        .and_then(|url| browser_dev_server_origin_key(&url))
        .as_deref()
        == Some(expected_origin.as_str());
    if !current_matches {
        return;
    }

    close_unavailable_dev_server_tab(app, tabs, monitor, tab_id, unavailable_url);
}

fn run_browser_dev_server_reconciler<R: Runtime + 'static>(
    app: AppHandle<R>,
    tabs: Arc<BrowserTabs>,
    monitor: Arc<BrowserDevServerMonitorState>,
) {
    let mut failures: HashMap<String, u8> = HashMap::new();
    loop {
        thread::sleep(DEV_SERVER_RECONCILER_INTERVAL);

        let snapshots = browser_dev_server_tab_snapshots(&tabs);
        let live_tab_ids = snapshots
            .iter()
            .map(|snapshot| snapshot.tab_id.as_str())
            .collect::<HashSet<_>>();
        failures.retain(|tab_id, _| live_tab_ids.contains(tab_id.as_str()));

        for snapshot in snapshots {
            let running = browser_dev_server_accepts_connections(
                &snapshot.url,
                DEV_SERVER_MONITOR_CONNECT_TIMEOUT,
            );
            if !record_browser_dev_server_probe_result(
                &mut failures,
                &snapshot.tab_id,
                running,
                DEV_SERVER_MONITOR_FAILURE_THRESHOLD,
            ) {
                continue;
            }
            failures.remove(&snapshot.tab_id);
            close_dev_server_tab_from_background(
                &app,
                &tabs,
                &monitor,
                &snapshot.tab_id,
                &snapshot.url,
            );
        }
    }
}

fn close_dev_server_tab_from_background<R: Runtime + 'static>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    unavailable_url: &Url,
) {
    let app_for_main = app.clone();
    let tabs_for_main = Arc::clone(tabs);
    let monitor_for_main = Arc::clone(monitor);
    let tab_id_for_main = tab_id.to_string();
    let unavailable_url_for_main = unavailable_url.clone();

    let app_for_fallback = app.clone();
    let tabs_for_fallback = Arc::clone(tabs);
    let monitor_for_fallback = Arc::clone(monitor);
    let tab_id_for_fallback = tab_id.to_string();
    let unavailable_url_for_fallback = unavailable_url.clone();

    if app
        .run_on_main_thread(move || {
            close_dev_server_tab_if_current_url(
                &app_for_main,
                &tabs_for_main,
                &monitor_for_main,
                &tab_id_for_main,
                &unavailable_url_for_main,
            );
        })
        .is_err()
    {
        close_dev_server_tab_if_current_url(
            &app_for_fallback,
            &tabs_for_fallback,
            &monitor_for_fallback,
            &tab_id_for_fallback,
            &unavailable_url_for_fallback,
        );
    }
}

fn sync_browser_dev_server_monitor<R: Runtime + 'static>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    url: &Url,
) {
    if browser_dev_server_origin_key(url).is_none() {
        monitor.cancel(tab_id);
        return;
    }

    let Ok(generation) = monitor.start(tab_id) else {
        return;
    };
    let app = app.clone();
    let tabs = Arc::clone(tabs);
    let monitor = Arc::clone(monitor);
    let tab_id = tab_id.to_string();
    let target = url.clone();

    thread::spawn(move || {
        monitor_browser_dev_server_tab(app, tabs, monitor, tab_id, target, generation);
    });
}

fn monitor_browser_dev_server_tab<R: Runtime + 'static>(
    app: AppHandle<R>,
    tabs: Arc<BrowserTabs>,
    monitor: Arc<BrowserDevServerMonitorState>,
    tab_id: String,
    target: Url,
    generation: u64,
) {
    let Some(origin_key) = browser_dev_server_origin_key(&target) else {
        monitor.cancel(&tab_id);
        return;
    };
    let mut failures = 0_u8;
    let mut delay = DEV_SERVER_MONITOR_INITIAL_DELAY;

    loop {
        thread::sleep(delay);
        delay = DEV_SERVER_MONITOR_INTERVAL;

        if !monitor.is_current(&tab_id, generation) {
            return;
        }

        let Some(current_url) = tabs.url_by_id(&tab_id) else {
            monitor.cancel(&tab_id);
            return;
        };
        let Ok(current) = actions::parse_url(&current_url) else {
            monitor.cancel(&tab_id);
            return;
        };
        if browser_dev_server_origin_key(&current).as_deref() != Some(origin_key.as_str()) {
            monitor.cancel(&tab_id);
            return;
        }

        if browser_dev_server_accepts_connections(&current, DEV_SERVER_MONITOR_CONNECT_TIMEOUT) {
            failures = 0;
            continue;
        }

        failures = failures.saturating_add(1);
        if failures >= DEV_SERVER_MONITOR_FAILURE_THRESHOLD {
            close_browser_tab_if_monitor_current(
                &app, &tabs, &monitor, &tab_id, generation, &current,
            );
            return;
        }
    }
}

fn close_browser_tab_if_monitor_current<R: Runtime + 'static>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    monitor: &Arc<BrowserDevServerMonitorState>,
    tab_id: &str,
    generation: u64,
    unavailable_url: &Url,
) {
    let app_for_main = app.clone();
    let tabs_for_main = Arc::clone(tabs);
    let monitor_for_main = Arc::clone(monitor);
    let tab_id_for_main = tab_id.to_string();
    let unavailable_url_for_main = unavailable_url.clone();

    let app_for_fallback = app.clone();
    let tabs_for_fallback = Arc::clone(tabs);
    let monitor_for_fallback = Arc::clone(monitor);
    let tab_id_for_fallback = tab_id.to_string();
    let unavailable_url_for_fallback = unavailable_url.clone();

    if app
        .run_on_main_thread(move || {
            if monitor_for_main.clear_if_current(&tab_id_for_main, generation) {
                emit_browser_dev_server_unavailable(
                    &app_for_main,
                    &tab_id_for_main,
                    &unavailable_url_for_main,
                );
                let _ = close_browser_tab(&app_for_main, &tabs_for_main, &tab_id_for_main);
            }
        })
        .is_err()
        && monitor_for_fallback.clear_if_current(&tab_id_for_fallback, generation)
    {
        emit_browser_dev_server_unavailable(
            &app_for_fallback,
            &tab_id_for_fallback,
            &unavailable_url_for_fallback,
        );
        let _ = close_browser_tab(&app_for_fallback, &tabs_for_fallback, &tab_id_for_fallback);
    }
}

fn list_running_browser_dev_servers_blocking() -> CommandResult<Vec<BrowserRunningDevServerDto>> {
    let detected_at = browser_detected_at_millis();
    let mut seen = BTreeSet::new();
    let mut servers = Vec::new();

    for port in list_browser_system_ports()? {
        let Some(url) = browser_system_port_url(&port) else {
            continue;
        };
        let Ok(target) = actions::parse_url(&url) else {
            continue;
        };
        let Some(origin_key) = browser_dev_server_origin_key(&target) else {
            continue;
        };
        if !seen.insert(origin_key) {
            continue;
        }
        if !browser_dev_server_responds_like_http(&target, DEV_SERVER_LIST_HTTP_PROBE_TIMEOUT) {
            continue;
        }

        servers.push(BrowserRunningDevServerDto {
            cwd: port.cwd.clone(),
            detected_at,
            label: browser_running_dev_server_label(&port),
            local_addr: port.local_addr.clone(),
            pid: port.pid,
            port: port.local_port,
            process_name: port.process_name.clone(),
            url,
        });
    }

    servers.sort_by(|left, right| {
        left.port
            .cmp(&right.port)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.url.cmp(&right.url))
    });
    Ok(servers)
}

fn browser_detected_at_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn browser_running_dev_server_label(port: &BrowserSystemPortInfo) -> String {
    let host = browser_system_port_display_host(&port.local_addr);
    match port
        .process_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(name) => format!("{name} · {host}:{}", port.local_port),
        None => format!("{host}:{}", port.local_port),
    }
}

fn browser_system_port_display_host(local_addr: &str) -> &'static str {
    match browser_system_port_url_host(local_addr).as_deref() {
        Some("[::1]") => "[::1]",
        _ => "127.0.0.1",
    }
}

fn browser_system_port_url(port: &BrowserSystemPortInfo) -> Option<String> {
    let host = browser_system_port_url_host(&port.local_addr)?;
    Some(format!("http://{host}:{}/", port.local_port))
}

fn browser_system_port_url_host(local_addr: &str) -> Option<String> {
    let normalized = local_addr
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    match normalized.as_str() {
        "" | "*" | "0.0.0.0" | "::" | "localhost" | "127.0.0.1" => {
            return Some("127.0.0.1".into());
        }
        "::1" => return Some("[::1]".into()),
        _ => {}
    }

    let ip = normalized.parse::<IpAddr>().ok()?;
    if !ip.is_loopback() {
        return None;
    }
    match ip {
        IpAddr::V4(addr) => Some(addr.to_string()),
        IpAddr::V6(addr) => Some(format!("[{addr}]")),
    }
}

fn browser_dev_server_responds_like_http(url: &Url, timeout: Duration) -> bool {
    let Some((host, port)) = browser_dev_server_probe_target(url) else {
        return false;
    };
    let Ok(addrs) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };
    let host_header = match url.host_str() {
        Some(host) if !host.is_empty() => format!("{host}:{port}"),
        _ => format!("127.0.0.1:{port}"),
    };

    for addr in addrs {
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        let request = format!(
            "GET / HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nAccept: */*\r\n\r\n"
        );
        if stream.write_all(request.as_bytes()).is_err() {
            continue;
        }
        let mut buffer = [0_u8; 16];
        let Ok(bytes_read) = stream.read(&mut buffer) else {
            continue;
        };
        if bytes_read >= 5 && buffer[..bytes_read].starts_with(b"HTTP/") {
            return true;
        }
    }

    false
}

fn list_browser_system_ports() -> CommandResult<Vec<BrowserSystemPortInfo>> {
    #[cfg(target_os = "linux")]
    {
        linux_browser_system_ports()
    }

    #[cfg(target_os = "macos")]
    {
        lsof_browser_system_ports()
    }

    #[cfg(windows)]
    {
        windows_browser_system_ports()
    }

    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        lsof_browser_system_ports()
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(CommandError::system_fault(
            "browser_running_dev_servers_unsupported",
            "Xero cannot list running local dev servers on this platform yet.",
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_browser_system_ports() -> CommandResult<Vec<BrowserSystemPortInfo>> {
    let mut ports = Vec::new();
    ports.extend(linux_browser_tcp_ports("/proc/net/tcp", false)?);
    ports.extend(linux_browser_tcp_ports("/proc/net/tcp6", true)?);
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_browser_tcp_ports(path: &str, ipv6: bool) -> CommandResult<Vec<BrowserSystemPortInfo>> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        CommandError::system_fault(
            "browser_running_dev_servers_failed",
            format!("Xero could not read {path} for listening ports: {error}"),
        )
    })?;
    let mut ports = Vec::new();
    for line in content.lines().skip(1) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 4 || columns[3] != "0A" {
            continue;
        }
        let Some((addr_hex, port_hex)) = columns[1].split_once(':') else {
            continue;
        };
        let Ok(local_port) = u16::from_str_radix(port_hex, 16) else {
            continue;
        };
        ports.push(BrowserSystemPortInfo {
            cwd: None,
            local_addr: if ipv6 {
                linux_browser_ipv6_addr(addr_hex)
            } else {
                linux_browser_ipv4_addr(addr_hex)
            },
            local_port,
            pid: None,
            process_name: None,
        });
    }
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_browser_ipv4_addr(value: &str) -> String {
    let Ok(raw) = u32::from_str_radix(value, 16) else {
        return value.into();
    };
    let bytes = raw.to_le_bytes();
    format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

#[cfg(target_os = "linux")]
fn linux_browser_ipv6_addr(value: &str) -> String {
    if value.len() != 32 {
        return value.into();
    }
    let mut segments = Vec::new();
    for chunk in value.as_bytes().chunks(8) {
        let chunk = String::from_utf8_lossy(chunk);
        let Ok(raw) = u32::from_str_radix(&chunk, 16) else {
            return value.into();
        };
        for segment in raw.to_le_bytes().chunks(2) {
            segments.push(u16::from_be_bytes([segment[0], segment[1]]));
        }
    }
    segments
        .chunks(1)
        .map(|chunk| format!("{:x}", chunk[0]))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(any(
    target_os = "macos",
    all(unix, not(any(target_os = "linux", target_os = "macos")))
))]
fn lsof_browser_system_ports() -> CommandResult<Vec<BrowserSystemPortInfo>> {
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "browser_running_dev_servers_failed",
                format!("Xero could not execute lsof for listening ports: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(CommandError::system_fault(
            "browser_running_dev_servers_failed",
            format!("lsof exited with status {}.", output.status),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = Vec::new();
    let mut pid = None;
    let mut process_name = None;
    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let (tag, value) = line.split_at(1);
        match tag {
            "p" => {
                pid = value.parse::<u32>().ok();
                process_name = None;
            }
            "c" => process_name = Some(value.to_owned()),
            "n" => {
                if let Some((local_addr, local_port)) = parse_browser_lsof_address(value) {
                    ports.push(BrowserSystemPortInfo {
                        cwd: None,
                        local_addr,
                        local_port,
                        pid,
                        process_name: process_name.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    hydrate_lsof_browser_system_port_cwds(&mut ports);
    Ok(ports)
}

#[cfg(any(
    target_os = "macos",
    all(unix, not(any(target_os = "linux", target_os = "macos")))
))]
fn hydrate_lsof_browser_system_port_cwds(ports: &mut [BrowserSystemPortInfo]) {
    let mut cwd_by_pid: HashMap<u32, Option<String>> = HashMap::new();
    for port in ports {
        let Some(pid) = port.pid else {
            continue;
        };
        if let Some(cwd) = cwd_by_pid.get(&pid) {
            port.cwd = cwd.clone();
            continue;
        }

        let cwd = lsof_process_cwd(pid);
        port.cwd = cwd.clone();
        cwd_by_pid.insert(pid, cwd);
    }
}

#[cfg(any(
    target_os = "macos",
    all(unix, not(any(target_os = "linux", target_os = "macos")))
))]
fn lsof_process_cwd(pid: u32) -> Option<String> {
    let pid_arg = pid.to_string();
    let output = Command::new("lsof")
        .args(["-nP", "-a", "-p", pid_arg.as_str(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(any(
    target_os = "macos",
    all(unix, not(any(target_os = "linux", target_os = "macos")))
))]
fn parse_browser_lsof_address(value: &str) -> Option<(String, u16)> {
    let without_state = value.split(" (").next().unwrap_or(value);
    let (addr, port) = if let Some(end) = without_state.rfind("]:") {
        let addr = without_state[..=end]
            .trim_start_matches('[')
            .trim_end_matches(']');
        (addr.to_owned(), &without_state[end + 2..])
    } else {
        let (addr, port) = without_state.rsplit_once(':')?;
        (addr.to_owned(), port)
    };
    Some((addr, port.parse::<u16>().ok()?))
}

#[cfg(windows)]
fn windows_browser_system_ports() -> CommandResult<Vec<BrowserSystemPortInfo>> {
    let output = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "browser_running_dev_servers_failed",
                format!("Xero could not execute netstat for listening ports: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(CommandError::system_fault(
            "browser_running_dev_servers_failed",
            format!("netstat exited with status {}.", output.status),
        ));
    }
    parse_browser_windows_netstat(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        CommandError::system_fault(
            "browser_running_dev_servers_failed",
            format!("Xero could not parse netstat output: {error}"),
        )
    })
}

#[cfg(windows)]
fn parse_browser_windows_netstat(text: &str) -> Result<Vec<BrowserSystemPortInfo>, String> {
    let mut ports = Vec::new();
    for line in text.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 5 || !columns[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !columns[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some((local_addr, local_port)) = parse_browser_windows_addr_port(columns[1]) else {
            continue;
        };
        ports.push(BrowserSystemPortInfo {
            cwd: None,
            local_addr,
            local_port,
            pid: columns[4].parse::<u32>().ok(),
            process_name: None,
        });
    }
    Ok(ports)
}

#[cfg(windows)]
fn parse_browser_windows_addr_port(value: &str) -> Option<(String, u16)> {
    if let Some(end) = value.rfind("]:") {
        let addr = value[..=end]
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        return Some((addr, value[end + 2..].parse::<u16>().ok()?));
    }
    let (addr, port) = value.rsplit_once(':')?;
    Some((addr.to_owned(), port.parse::<u16>().ok()?))
}

fn browser_dev_server_accepts_connections(url: &Url, timeout: Duration) -> bool {
    let Some((host, port)) = browser_dev_server_probe_target(url) else {
        return false;
    };
    let Ok(addrs) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };

    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
}

fn browser_dev_server_probe_target(url: &Url) -> Option<(String, u16)> {
    browser_dev_server_origin_key(url)?;
    let port = url.port_or_known_default()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let host = match host.as_str() {
        "localhost" | "0.0.0.0" => "127.0.0.1".to_string(),
        "::1" => "::1".to_string(),
        _ => host,
    };
    Some((host, port))
}

fn browser_dev_server_origin_key(url: &Url) -> Option<String> {
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let is_loopback = match host.as_str() {
        "localhost" | "0.0.0.0" | "::1" => true,
        _ => host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()),
    };
    if !is_loopback {
        return None;
    }
    let port = url.port_or_known_default()?;
    let normalized_host = match host.as_str() {
        "localhost" | "0.0.0.0" => "127.0.0.1",
        other => other,
    };
    Some(format!("{}://{}:{}", url.scheme(), normalized_host, port))
}

/// Move every non-active tab's webview off-screen so only the active one is
/// visible. Native child webviews paint on top of all HTML regardless of
/// CSS z-index, so without this all created tabs would stack over one
/// another at the same viewport rect.
fn hide_inactive_webviews<R: Runtime>(app: &AppHandle<R>, tabs: &BrowserTabs) {
    let Ok(list) = tabs.list() else {
        return;
    };
    for tab in list {
        if tab.active {
            continue;
        }
        if let Some(webview) = app.get_webview(&tab.label) {
            let _ = webview.set_position(LogicalPosition::new(HIDDEN_OFFSET, HIDDEN_OFFSET));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_metadata_roundtrip() {
        let tabs = BrowserTabs::new();
        let (id, label) = tabs.new_tab_label();
        tabs.insert(id.clone(), label.clone(), Some("project-a".to_string()))
            .unwrap();
        tabs.record_page_state(
            &id,
            Some("https://example.com/".to_string()),
            Some("Example".to_string()),
            Some(false),
        );
        let list = tabs.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].project_id.as_deref(), Some("project-a"));
        assert_eq!(list[0].url.as_deref(), Some("https://example.com/"));
        assert_eq!(tabs.url_by_id(&id).as_deref(), Some("https://example.com/"));
        assert!(list[0].active);
    }

    #[test]
    fn tab_removal_switches_active() {
        let tabs = BrowserTabs::new();
        let (id_a, label_a) = tabs.new_tab_label();
        let (id_b, label_b) = tabs.new_tab_label();
        tabs.insert(id_a.clone(), label_a, None).unwrap();
        tabs.insert(id_b.clone(), label_b, None).unwrap();
        tabs.set_active(&id_b).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(id_b.as_str()));
        tabs.remove(&id_b).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(id_a.as_str()));
    }

    #[test]
    fn tab_lists_and_active_tabs_are_project_scoped() {
        let tabs = BrowserTabs::new();
        let (project_a_first, project_a_first_label) = tabs.new_tab_label();
        let (project_b, project_b_label) = tabs.new_tab_label();
        let (project_a_second, project_a_second_label) = tabs.new_tab_label();
        tabs.insert(
            project_a_first.clone(),
            project_a_first_label,
            Some("project-a".to_string()),
        )
        .unwrap();
        tabs.insert(
            project_b.clone(),
            project_b_label,
            Some("project-b".to_string()),
        )
        .unwrap();
        tabs.insert(
            project_a_second.clone(),
            project_a_second_label,
            Some("project-a".to_string()),
        )
        .unwrap();

        tabs.set_active(&project_a_second).unwrap();
        assert_eq!(
            tabs.list_for_project(Some("project-a"))
                .unwrap()
                .into_iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![project_a_first.clone(), project_a_second.clone()],
        );
        assert_eq!(
            tabs.list_for_project(Some("project-b"))
                .unwrap()
                .into_iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![project_b.clone()],
        );

        tabs.set_active(&project_b).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(project_b.as_str()));
        tabs.activate_project(Some("project-a")).unwrap();
        assert_eq!(
            tabs.active_tab_id().as_deref(),
            Some(project_a_second.as_str()),
        );
        tabs.activate_project(Some("project-b")).unwrap();
        assert_eq!(tabs.active_tab_id().as_deref(), Some(project_b.as_str()));
    }

    #[test]
    fn tab_reorder_is_project_scoped() {
        let tabs = BrowserTabs::new();
        let (project_a_first, project_a_first_label) = tabs.new_tab_label();
        let (project_b, project_b_label) = tabs.new_tab_label();
        let (project_a_second, project_a_second_label) = tabs.new_tab_label();
        tabs.insert(
            project_a_first.clone(),
            project_a_first_label,
            Some("project-a".to_string()),
        )
        .unwrap();
        tabs.insert(
            project_b.clone(),
            project_b_label,
            Some("project-b".to_string()),
        )
        .unwrap();
        tabs.insert(
            project_a_second.clone(),
            project_a_second_label,
            Some("project-a".to_string()),
        )
        .unwrap();
        tabs.set_active(&project_a_second).unwrap();

        tabs.reorder_for_project(&project_a_second, &project_a_first, Some("project-a"))
            .unwrap();

        assert_eq!(
            tabs.list_for_project(Some("project-a"))
                .unwrap()
                .into_iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![project_a_second.clone(), project_a_first.clone()],
        );
        assert_eq!(
            tabs.list()
                .unwrap()
                .into_iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![
                project_a_second.clone(),
                project_a_first.clone(),
                project_b.clone(),
            ],
        );
        assert_eq!(
            tabs.active_tab_id().as_deref(),
            Some(project_a_second.as_str()),
        );

        let error = tabs
            .reorder_for_project(&project_a_second, &project_b, Some("project-a"))
            .expect_err("cross-project reorder should fail");
        assert_eq!(error.code, "browser_tab_not_found");
    }

    #[test]
    fn tab_label_rejects_unknown_id() {
        let tabs = BrowserTabs::new();
        let error = tabs
            .tab_label("tab-missing")
            .expect_err("missing tab should fail");
        assert_eq!(error.code, "browser_tab_not_found");
    }

    #[test]
    fn dev_server_origin_key_normalizes_loopback_only() {
        let local = actions::parse_url("http://localhost:5173/app").unwrap();
        let any_addr = actions::parse_url("http://0.0.0.0:3000/").unwrap();
        let google = actions::parse_url("https://google.com/").unwrap();

        assert_eq!(
            browser_dev_server_origin_key(&local).as_deref(),
            Some("http://127.0.0.1:5173"),
        );
        assert_eq!(
            browser_dev_server_origin_key(&any_addr).as_deref(),
            Some("http://127.0.0.1:3000"),
        );
        assert_eq!(browser_dev_server_origin_key(&google), None);
    }

    #[test]
    fn dev_server_liveness_probe_detects_open_loopback_port() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = actions::parse_url(&format!("http://127.0.0.1:{port}/")).unwrap();

        assert!(browser_dev_server_accepts_connections(
            &url,
            Duration::from_millis(100),
        ));
    }

    #[test]
    fn running_dev_server_url_normalizes_local_listeners() {
        let wildcard = BrowserSystemPortInfo {
            cwd: None,
            local_addr: "*".into(),
            local_port: 4100,
            pid: Some(123),
            process_name: Some("node".into()),
        };
        let loopback_v6 = BrowserSystemPortInfo {
            cwd: None,
            local_addr: "::1".into(),
            local_port: 5173,
            pid: None,
            process_name: None,
        };
        let remote = BrowserSystemPortInfo {
            cwd: None,
            local_addr: "192.168.1.12".into(),
            local_port: 3000,
            pid: None,
            process_name: None,
        };

        assert_eq!(
            browser_system_port_url(&wildcard).as_deref(),
            Some("http://127.0.0.1:4100/")
        );
        assert_eq!(
            browser_running_dev_server_label(&wildcard),
            "node · 127.0.0.1:4100"
        );
        assert_eq!(
            browser_system_port_url(&loopback_v6).as_deref(),
            Some("http://[::1]:5173/")
        );
        assert_eq!(browser_system_port_url(&remote), None);
    }

    #[test]
    fn running_dev_server_http_probe_requires_http_response() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let url = actions::parse_url(&format!("http://127.0.0.1:{port}/")).unwrap();

        assert!(browser_dev_server_responds_like_http(
            &url,
            Duration::from_millis(500),
        ));
        handle.join().unwrap();
    }

    #[test]
    fn dev_server_tab_snapshots_include_only_loopback_http_tabs() {
        let tabs = BrowserTabs::new();
        let (local_id, local_label) = tabs.new_tab_label();
        tabs.insert(local_id.clone(), local_label, Some("project-a".to_string()))
            .unwrap();
        tabs.record_page_state(
            &local_id,
            Some("http://localhost:5173/dashboard".to_string()),
            None,
            Some(false),
        );
        let (remote_id, remote_label) = tabs.new_tab_label();
        tabs.insert(
            remote_id.clone(),
            remote_label,
            Some("project-a".to_string()),
        )
        .unwrap();
        tabs.record_page_state(
            &remote_id,
            Some("https://example.com/".to_string()),
            None,
            Some(false),
        );

        let snapshots = browser_dev_server_tab_snapshots(&tabs);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].tab_id, local_id);
        assert_eq!(snapshots[0].url.as_str(), "http://127.0.0.1:5173/dashboard");
    }

    #[test]
    fn dev_server_failure_tracker_waits_for_threshold_and_resets_on_success() {
        let mut failures = HashMap::new();

        assert!(!record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            false,
            3,
        ));
        assert!(!record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            false,
            3,
        ));
        assert!(!record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            true,
            3,
        ));
        assert!(!failures.contains_key("tab-1"));
        assert!(!record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            false,
            3,
        ));
        assert!(!record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            false,
            3,
        ));
        assert!(record_browser_dev_server_probe_result(
            &mut failures,
            "tab-1",
            false,
            3,
        ));
    }

    #[test]
    fn dev_server_monitor_generation_guards_stale_threads() {
        let monitor = BrowserDevServerMonitorState::default();
        let first = monitor.start("tab-1").unwrap();
        assert!(monitor.is_current("tab-1", first));

        let second = monitor.start("tab-1").unwrap();
        assert!(!monitor.is_current("tab-1", first));
        assert!(!monitor.clear_if_current("tab-1", first));
        assert!(monitor.clear_if_current("tab-1", second));
        assert!(!monitor.is_current("tab-1", second));
    }

    #[test]
    fn resize_drag_viewport_tracks_cursor_against_fixed_right_edge() {
        let session = BrowserResizeDragSession {
            id: 1,
            labels: vec!["xero-browser-tab-1".to_string()],
            tab_id: Some("tab-1".to_string()),
            start_client_x: 760.0,
            start_width: 640.0,
            right: 1400.0,
            top: 100.0,
            height: 800.0,
            min_width: 320.0,
            max_width: 1400.0,
            inset: 6.0,
        };

        let viewport = session.viewport_for_cursor(680.0);

        assert_eq!(viewport.x, 686.0);
        assert_eq!(viewport.y, 100.0);
        assert_eq!(viewport.width, 714.0);
        assert_eq!(viewport.height, 800.0);
    }
}
