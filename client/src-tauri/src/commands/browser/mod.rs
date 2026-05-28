pub mod actions;
pub(crate) mod bridge;
pub mod cookie_import;
mod diagnostics;
mod events;
mod screenshot;
mod script;
pub mod settings;
pub mod tabs;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

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
    objc2::rc::Retained,
    objc2_app_kit::{NSEvent, NSEventTrackingRunLoopMode, NSView, NSWindow},
    objc2_foundation::{NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSTimer},
    std::ptr::NonNull,
};

pub use actions::{StorageArea, TypingMode};
pub use diagnostics::{
    BrowserConsoleDiagnosticEntry, BrowserDiagnosticReadOptions, BrowserDiagnostics,
    BrowserNetworkDiagnosticEntry,
};
pub use events::{
    BrowserConsolePayload, BrowserDialogPayload, BrowserDownloadPayload, BrowserLoadStatePayload,
    BrowserResizeDragPayload, BrowserTabUpdatedPayload, BrowserToolClosedPayload,
    BrowserToolContextPayload, BrowserUrlChangedPayload, BROWSER_CONSOLE_EVENT,
    BROWSER_DIALOG_EVENT, BROWSER_DOWNLOAD_EVENT, BROWSER_LOAD_STATE_EVENT,
    BROWSER_RESIZE_DRAG_EVENT, BROWSER_TAB_UPDATED_EVENT, BROWSER_TOOL_CLOSED_EVENT,
    BROWSER_TOOL_CONTEXT_EVENT, BROWSER_URL_CHANGED_EVENT,
};
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
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

pub struct BrowserState {
    creation_lock: Mutex<()>,
    resize_coalescer: Arc<BrowserResizeCoalescer>,
    resize_drag: Arc<BrowserResizeDragState>,
    waiters: Arc<BridgeWaiters>,
    tabs: Arc<BrowserTabs>,
    diagnostics: Arc<BrowserDiagnostics>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            creation_lock: Mutex::new(()),
            resize_coalescer: Arc::new(BrowserResizeCoalescer::default()),
            resize_drag: Arc::new(BrowserResizeDragState::default()),
            waiters: Arc::new(BridgeWaiters::new()),
            tabs: Arc::new(BrowserTabs::new()),
            diagnostics: Arc::new(BrowserDiagnostics::default()),
        }
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

    pub fn diagnostics(&self) -> Arc<BrowserDiagnostics> {
        Arc::clone(&self.diagnostics)
    }
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
pub fn browser_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
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
        Some(BrowserViewport {
            x,
            y,
            width,
            height,
        }),
    )
}

pub fn provision_browser_tab<R: Runtime>(
    app: &AppHandle<R>,
    state: &BrowserState,
    url: &str,
    requested_tab_id: Option<&str>,
    force_new: bool,
    viewport: Option<BrowserViewport>,
) -> CommandResult<BrowserTabMetadata> {
    let target = actions::parse_url(url)?;
    let tabs = state.tabs();
    let viewport = resolve_browser_viewport(app, viewport);

    let _guard = state.creation_lock.lock().map_err(|_| {
        CommandError::system_fault("browser_lock_poisoned", "Browser state lock poisoned.")
    })?;

    let previous_active = tabs.active_tab_id();
    let mut inserted_tab = false;

    let (tab_id, label) = if force_new {
        inserted_tab = true;
        let (id, label) = tabs.new_tab_label();
        tabs.insert(id.clone(), label.clone())?;
        (id, label)
    } else {
        match requested_tab_id {
            Some(existing) => (existing.to_string(), tabs.tab_label(existing)?),
            None => {
                if let Some(active) = tabs.active_tab_id() {
                    let label = tabs
                        .active_label_soft()
                        .unwrap_or_else(|| tab_label_for_id(&active));
                    (active, label)
                } else {
                    inserted_tab = true;
                    let (id, label) = tabs.new_tab_label();
                    tabs.insert(id.clone(), label.clone())?;
                    (id, label)
                }
            }
        }
    };

    if let Err(error) = ensure_browser_webview(app, &tabs, &tab_id, &label, &target, viewport) {
        if inserted_tab {
            let _ = tabs.remove(&tab_id);
            if let Some(previous) = previous_active.as_deref() {
                let _ = tabs.set_active(previous);
            }
            hide_inactive_webviews(app, &tabs);
            emit_tab_list(app, &tabs);
        }
        return Err(error);
    }

    tabs.set_active(&tab_id)?;
    tabs.record_page_state(&tab_id, Some(target.to_string()), None, Some(true));
    hide_inactive_webviews(app, &tabs);
    emit_tab_list(app, &tabs);
    Ok(current_tab_meta(&tabs, &tab_id))
}

fn ensure_browser_webview<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
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
    let app_for_nav = app.clone();

    let tab_id_for_load = tab_id.to_string();
    let tabs_for_load = Arc::clone(tabs);
    let app_for_load = app.clone();

    let builder = WebviewBuilder::new(label.to_string(), WebviewUrl::External(target.clone()))
        .initialization_script(BROWSER_BRIDGE_INIT_SCRIPT)
        .on_navigation(move |url| {
            tabs_for_nav.record_page_state(
                &tab_id_for_nav,
                Some(url.to_string()),
                None,
                Some(true),
            );
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
            LogicalPosition::new(viewport.x, viewport.y),
            LogicalSize::new(viewport.width, viewport.height),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "browser_create_failed",
                format!("Xero could not create the browser webview: {error}"),
            )
        })?;
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
    let Some(webview) = state.tabs().optional_active_webview(&app) else {
        return Ok(None);
    };
    let url = webview.url().map_err(|error| {
        CommandError::system_fault(
            "browser_url_failed",
            format!("Xero could not read the browser URL: {error}"),
        )
    })?;
    Ok(Some(url.to_string()))
}

#[tauri::command]
pub async fn browser_screenshot<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
) -> CommandResult<String> {
    let webview = state.tabs().active_webview(&app)?;
    tauri::async_runtime::spawn_blocking(move || screenshot::capture_webview(&webview))
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "browser_screenshot_task_failed",
                format!("Xero could not capture the browser in the background: {error}"),
            )
        })?
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
            format!("Xero could not navigate the browser webview: {error}"),
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
    let current = webview.url().map_err(|error| {
        CommandError::system_fault(
            "browser_url_failed",
            format!("Xero could not read the browser URL: {error}"),
        )
    })?;
    webview.navigate(current).map_err(|error| {
        CommandError::system_fault(
            "browser_navigate_failed",
            format!("Xero could not reload the browser webview: {error}"),
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
    hide_inactive_webviews(&app, &tabs);
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

    #[test]
    fn tab_label_rejects_unknown_id() {
        let tabs = BrowserTabs::new();
        let error = tabs
            .tab_label("tab-missing")
            .expect_err("missing tab should fail");
        assert_eq!(error.code, "browser_tab_not_found");
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
