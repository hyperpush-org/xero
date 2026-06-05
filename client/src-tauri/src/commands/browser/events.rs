use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

pub const BROWSER_URL_CHANGED_EVENT: &str = "browser:url_changed";
pub const BROWSER_LOAD_STATE_EVENT: &str = "browser:load_state";
pub const BROWSER_CONSOLE_EVENT: &str = "browser:console";
pub const BROWSER_TAB_UPDATED_EVENT: &str = "browser:tab_updated";
pub const BROWSER_DIALOG_EVENT: &str = "browser:dialog";
pub const BROWSER_DOWNLOAD_EVENT: &str = "browser:download";
pub const BROWSER_RESIZE_DRAG_EVENT: &str = "browser:resize_drag";
pub const BROWSER_OCCLUSION_WHEEL_EVENT: &str = "browser:occlusion_wheel";
pub const BROWSER_OCCLUSION_CLICK_EVENT: &str = "browser:occlusion_click";
pub const BROWSER_DEV_SERVER_UNAVAILABLE_EVENT: &str = "browser:dev_server_unavailable";
pub const BROWSER_TOOL_CONTEXT_EVENT: &str = "browser:tool_context";
pub const BROWSER_TOOL_CLOSED_EVENT: &str = "browser:tool_closed";
pub const BROWSER_TOOL_STATE_EVENT: &str = "browser:tool_state";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserUrlChangedPayload {
    pub tab_id: String,
    pub url: String,
    pub title: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoadStatePayload {
    pub tab_id: String,
    pub loading: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConsolePayload {
    pub tab_id: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabUpdatedPayload {
    pub tabs: Vec<super::tabs::BrowserTabMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDevServerUnavailablePayload {
    pub tab_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDialogPayload {
    pub tab_id: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadPayload {
    pub tab_id: String,
    pub url: String,
    pub suggested_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserResizeDragPayload {
    pub tab_id: Option<String>,
    pub sidebar_width: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOcclusionClickPayload {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOcclusionWheelPayload {
    pub x: f64,
    pub y: f64,
    pub delta_x: f64,
    pub delta_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolContextPayload {
    pub tab_id: String,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolClosedPayload {
    pub tab_id: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolStatePayload {
    pub tab_id: String,
    pub mode: Option<String>,
    pub stroke_count: u64,
    pub has_drawing: bool,
}

pub fn emit<R: Runtime, T: Serialize + Clone>(app: &AppHandle<R>, event: &str, payload: &T) {
    let _ = app.emit(event, payload);
}
