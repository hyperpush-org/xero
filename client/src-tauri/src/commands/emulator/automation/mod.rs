//! Agent-callable automation surface.
//!
//! Everything the sidebar UI exposes to a human — tap, type, scroll, UI
//! inspection, app lifecycle, logs — is also reachable through the Tauri
//! command layer defined here. The design rule is:
//!
//!   **Every input and output is serializable JSON. No platform-specific
//!   types leak across the command boundary.**
//!
//! That lets a future MCP server, in-app agent, or external test harness
//! drive the emulator with the same payloads the frontend uses.
//!
//! Platform dispatch happens in `super` — this module only defines the
//! shared shape (`UiTree`, `Selector`, `AppDescriptor`, etc.) and per-platform
//! parsers. Android implementations are live; iOS implementations stub until
//! the idb proto is vendored.

pub mod android_ui;
pub mod apps;
pub mod ios_ui;
pub mod logs;
pub mod metro_detect;
pub mod metro_inspector;
pub mod selector;

use serde::{Deserialize, Serialize};

/// Pixel-space rectangle on the device framebuffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

/// Accessibility-tree node, normalized across Android's `uiautomator` XML
/// and iOS's idb accessibility-info JSON. Callers never need to know which
/// platform produced a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub enabled: bool,
    pub focused: bool,
    pub bounds: Bounds,
    /// Raw role string reported by the platform, for escape hatches —
    /// e.g. `android.widget.Button` or `XCUIElementTypeButton`. Consumers
    /// should prefer `role` unless they have a specific reason to
    /// fingerprint the platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_role: Option<String>,
    #[serde(default)]
    pub children: Vec<UiNode>,
}

impl UiNode {
    /// Depth-first traversal visiting self before children.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a UiNode)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTree {
    pub root: UiNode,
}

/// All fields are ANDed when present. A missing field matches anything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Substring match against `label` and `value` fields combined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    /// When set, only nodes with non-empty bounds and enabled==true match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}

/// App metadata returned by `emulator_list_apps` / `emulator_install_app`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDescriptor {
    pub bundle_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
}

/// Shape returned by `emulator_screenshot`. PNG so the agent can hand it
/// straight to a vision model without re-encoding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResponse {
    pub png_base64: String,
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f32,
}

/// Tap target — either an explicit pixel point or a selector that the
/// backend will resolve atomically (re-dump → match → tap center).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TapTarget {
    #[serde(rename = "point")]
    Point { x: f32, y: f32 },
    #[serde(rename = "element")]
    Element { selector: Selector },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SwipeRequest {
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypeRequest {
    pub text: String,
    #[serde(default)]
    pub into: Option<Selector>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HardwareKeyRequest {
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallAppRequest {
    pub source_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleIdRequest {
    pub bundle_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchAppRequest {
    pub bundle_id: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationRequest {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PushNotificationRequest {
    pub bundle_id: String,
    /// JSON payload, serialized as a string. We forward it verbatim to
    /// `simctl push`. Android devices cannot receive push via this command
    /// (the simulator has no APNS equivalent), and we return a typed error
    /// on that platform.
    pub payload: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
#[derive(Default)]
pub struct LogSubscribeRequest {
    pub filter: Option<String>,
}

/// Streamed log entry payload — emitted via `emulator:log` events.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub level: String,
    pub tag: String,
    pub message: String,
}

pub const EMULATOR_LOG_EVENT: &str = "emulator:log";

/// Handle returned by `emulator_logs_subscribe`. The caller can pass it
/// back to `emulator_logs_unsubscribe` to cancel streaming. It's also
/// implicitly cleaned up when the session stops.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionToken {
    pub id: String,
}
