use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::{CommandError, CommandResult};

pub const BROWSER_MAIN_WINDOW_LABEL: &str = "main";

/// Prefix for all browser tab webview labels, so we can identify them in Tauri's
/// global webview registry.
pub const BROWSER_TAB_PREFIX: &str = "xero-browser-tab-";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserTabMetadata {
    pub id: String,
    pub label: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub active: bool,
}

#[derive(Debug, Default)]
struct TabRecord {
    label: String,
    title: Option<String>,
    url: Option<String>,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

#[derive(Default)]
pub struct BrowserTabs {
    counter: AtomicU64,
    inner: Mutex<BrowserTabsInner>,
}

#[derive(Default)]
struct BrowserTabsInner {
    /// Preserves insertion order by using the tab id (monotonic).
    tabs: BTreeMap<String, TabRecord>,
    active: Option<String>,
}

impl BrowserTabs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_label<R: Runtime>(&self, app: &AppHandle<R>) -> CommandResult<String> {
        self.ensure_active(app)
    }

    pub fn active_webview<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CommandResult<tauri::Webview<R>> {
        let label = self.ensure_active(app)?;
        app.get_webview(&label).ok_or_else(|| {
            CommandError::user_fixable(
                "browser_not_open",
                "The in-app browser is not currently open.",
            )
        })
    }

    pub fn optional_active_webview<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Option<tauri::Webview<R>> {
        let label = self.active_label_soft()?;
        app.get_webview(&label)
    }

    pub fn active_label_soft(&self) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        let active = guard.active.as_ref()?;
        guard.tabs.get(active).map(|tab| tab.label.clone())
    }

    pub fn active_tab_id(&self) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard.active.clone()
    }

    pub fn tab_label(&self, id: &str) -> CommandResult<String> {
        let guard = self.lock()?;
        guard
            .tabs
            .get(id)
            .map(|tab| tab.label.clone())
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_tab_not_found",
                    format!("Browser tab `{id}` was not found."),
                )
            })
    }

    pub fn new_tab_label(&self) -> (String, String) {
        let next = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let id = format!("tab-{next:x}");
        let label = format!("{BROWSER_TAB_PREFIX}{next:x}");
        (id, label)
    }

    pub fn insert(&self, id: String, label: String) -> CommandResult<()> {
        let mut guard = self.lock()?;
        guard.tabs.insert(
            id.clone(),
            TabRecord {
                label,
                ..TabRecord::default()
            },
        );
        if guard.active.is_none() {
            guard.active = Some(id);
        }
        Ok(())
    }

    pub fn set_active(&self, id: &str) -> CommandResult<()> {
        let mut guard = self.lock()?;
        if !guard.tabs.contains_key(id) {
            return Err(CommandError::user_fixable(
                "browser_tab_not_found",
                format!("Browser tab `{id}` was not found."),
            ));
        }
        guard.active = Some(id.to_string());
        Ok(())
    }

    pub fn remove(&self, id: &str) -> CommandResult<Option<String>> {
        let mut guard = self.lock()?;
        let removed = guard.tabs.remove(id);
        if guard.active.as_deref() == Some(id) {
            guard.active = guard.tabs.keys().next().cloned();
        }
        Ok(removed.map(|tab| tab.label))
    }

    pub fn list(&self) -> CommandResult<Vec<BrowserTabMetadata>> {
        let guard = self.lock()?;
        let active = guard.active.clone();
        Ok(guard
            .tabs
            .iter()
            .map(|(id, tab)| BrowserTabMetadata {
                id: id.clone(),
                label: tab.label.clone(),
                title: tab.title.clone(),
                url: tab.url.clone(),
                loading: tab.loading,
                can_go_back: tab.can_go_back,
                can_go_forward: tab.can_go_forward,
                active: active.as_deref() == Some(id),
            })
            .collect())
    }

    pub fn find_by_label(&self, label: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .tabs
            .iter()
            .find(|(_, tab)| tab.label == label)
            .map(|(id, _)| id.clone())
    }

    pub fn record_page_state(
        &self,
        id: &str,
        url: Option<String>,
        title: Option<String>,
        loading: Option<bool>,
    ) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if let Some(tab) = guard.tabs.get_mut(id) {
            if url.is_some() {
                tab.url = url;
            }
            if title.is_some() {
                tab.title = title;
            }
            if let Some(loading) = loading {
                tab.loading = loading;
            }
        }
    }

    pub fn record_history_state(&self, id: &str, can_go_back: bool, can_go_forward: bool) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if let Some(tab) = guard.tabs.get_mut(id) {
            tab.can_go_back = can_go_back;
            tab.can_go_forward = can_go_forward;
        }
    }

    fn ensure_active<R: Runtime>(&self, app: &AppHandle<R>) -> CommandResult<String> {
        let Some(label) = self.active_label_soft() else {
            return Err(CommandError::user_fixable(
                "browser_not_open",
                "The in-app browser is not currently open.",
            ));
        };
        if app.get_webview(&label).is_none() {
            return Err(CommandError::user_fixable(
                "browser_not_open",
                "The in-app browser webview is not attached.",
            ));
        }
        Ok(label)
    }

    fn lock(&self) -> CommandResult<std::sync::MutexGuard<'_, BrowserTabsInner>> {
        self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_tabs_lock_poisoned",
                "Browser tabs registry lock poisoned.",
            )
        })
    }
}
