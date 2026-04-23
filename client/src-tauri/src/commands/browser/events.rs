use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

pub const BROWSER_URL_CHANGED_EVENT: &str = "browser:url_changed";
pub const BROWSER_LOAD_STATE_EVENT: &str = "browser:load_state";
pub const BROWSER_CONSOLE_EVENT: &str = "browser:console";
pub const BROWSER_TAB_UPDATED_EVENT: &str = "browser:tab_updated";
pub const BROWSER_DIALOG_EVENT: &str = "browser:dialog";
pub const BROWSER_DOWNLOAD_EVENT: &str = "browser:download";

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

pub fn emit<R: Runtime, T: Serialize + Clone>(app: &AppHandle<R>, event: &str, payload: &T) {
    let _ = app.emit(event, payload);
}
