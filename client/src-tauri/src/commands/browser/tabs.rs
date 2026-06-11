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
    pub project_id: Option<String>,
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
    project_id: Option<String>,
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
    tabs: BTreeMap<String, TabRecord>,
    order: Vec<String>,
    active: Option<String>,
    active_by_project: BTreeMap<String, String>,
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

    pub fn active_tab_id_for_project(&self, project_id: Option<&str>) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        match normalize_project_id(project_id) {
            Some(project_id) => active_tab_id_for_project(&guard, project_id),
            None => guard.active.clone(),
        }
    }

    pub fn active_url(&self) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        let active = guard.active.as_ref()?;
        guard.tabs.get(active).and_then(|tab| tab.url.clone())
    }

    pub fn url_by_id(&self, id: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard.tabs.get(id).and_then(|tab| tab.url.clone())
    }

    pub fn url_by_label(&self, label: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .tabs
            .values()
            .find(|tab| tab.label == label)
            .and_then(|tab| tab.url.clone())
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

    pub fn tab_label_for_project(
        &self,
        id: &str,
        project_id: Option<&str>,
    ) -> CommandResult<String> {
        let guard = self.lock()?;
        let tab = guard.tabs.get(id).ok_or_else(|| {
            CommandError::user_fixable(
                "browser_tab_not_found",
                format!("Browser tab `{id}` was not found."),
            )
        })?;
        if let Some(project_id) = normalize_project_id(project_id) {
            if tab.project_id.as_deref() != Some(project_id) {
                return Err(CommandError::user_fixable(
                    "browser_tab_not_found",
                    format!("Browser tab `{id}` was not found in this project."),
                ));
            }
        }
        Ok(tab.label.clone())
    }

    pub fn new_tab_label(&self) -> (String, String) {
        let next = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let id = format!("tab-{next:x}");
        let label = format!("{BROWSER_TAB_PREFIX}{next:x}");
        (id, label)
    }

    pub fn insert(
        &self,
        id: String,
        label: String,
        project_id: Option<String>,
    ) -> CommandResult<()> {
        let mut guard = self.lock()?;
        let project_id = normalize_owned_project_id(project_id);
        let project_key = project_id.clone();
        if !guard.tabs.contains_key(&id) {
            guard.order.push(id.clone());
        }
        guard.tabs.insert(
            id.clone(),
            TabRecord {
                project_id,
                label,
                ..TabRecord::default()
            },
        );
        if guard.active.is_none() {
            guard.active = Some(id.clone());
        }
        if let Some(project_id) = project_key {
            guard
                .active_by_project
                .entry(project_id)
                .or_insert_with(|| id.clone());
        }
        Ok(())
    }

    pub fn reorder_for_project(
        &self,
        active_id: &str,
        over_id: &str,
        project_id: Option<&str>,
    ) -> CommandResult<()> {
        if active_id == over_id {
            return Ok(());
        }

        let mut guard = self.lock()?;
        ensure_tab_belongs_to_project(&guard, active_id, project_id)?;
        ensure_tab_belongs_to_project(&guard, over_id, project_id)?;

        let Some(from_index) = guard.order.iter().position(|id| id == active_id) else {
            return Err(tab_not_found_error(active_id));
        };
        let Some(to_index) = guard.order.iter().position(|id| id == over_id) else {
            return Err(tab_not_found_error(over_id));
        };
        if from_index == to_index {
            return Ok(());
        }

        let moved = guard.order.remove(from_index);
        let target_index = to_index.min(guard.order.len());
        guard.order.insert(target_index, moved);
        Ok(())
    }

    pub fn set_active(&self, id: &str) -> CommandResult<()> {
        let mut guard = self.lock()?;
        let project_id = guard.tabs.get(id).map(|tab| tab.project_id.clone());
        if project_id.is_none() && !guard.tabs.contains_key(id) {
            return Err(CommandError::user_fixable(
                "browser_tab_not_found",
                format!("Browser tab `{id}` was not found."),
            ));
        }
        guard.active = Some(id.to_string());
        if let Some(Some(project_id)) = project_id {
            guard.active_by_project.insert(project_id, id.to_string());
        }
        Ok(())
    }

    pub fn activate_project(&self, project_id: Option<&str>) -> CommandResult<Option<String>> {
        let mut guard = self.lock()?;
        let active = match normalize_project_id(project_id) {
            Some(project_id) => {
                let active = active_tab_id_for_project(&guard, project_id);
                if let Some(active) = active.as_ref() {
                    guard
                        .active_by_project
                        .insert(project_id.to_string(), active.clone());
                } else {
                    guard.active_by_project.remove(project_id);
                }
                active
            }
            None => guard.active.clone().or_else(|| last_ordered_tab_id(&guard)),
        };
        guard.active = active.clone();
        Ok(active)
    }

    pub fn tab_project_id(&self, id: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard.tabs.get(id).and_then(|tab| tab.project_id.clone())
    }

    pub fn tab_belongs_to_project(&self, id: &str, project_id: Option<&str>) -> bool {
        let Some(project_id) = normalize_project_id(project_id) else {
            return true;
        };
        self.tab_project_id(id).as_deref() == Some(project_id)
    }

    pub fn project_has_tabs(&self, project_id: Option<&str>) -> bool {
        let Ok(guard) = self.lock() else {
            return false;
        };
        match normalize_project_id(project_id) {
            Some(project_id) => guard
                .tabs
                .values()
                .any(|tab| tab.project_id.as_deref() == Some(project_id)),
            None => !guard.tabs.is_empty(),
        }
    }

    pub fn list_for_project(
        &self,
        project_id: Option<&str>,
    ) -> CommandResult<Vec<BrowserTabMetadata>> {
        let guard = self.lock()?;
        Ok(tab_metadata_for_project(&guard, project_id))
    }

    pub fn list(&self) -> CommandResult<Vec<BrowserTabMetadata>> {
        let guard = self.lock()?;
        Ok(tab_metadata_for_project(&guard, None))
    }

    pub fn find_by_label(&self, label: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .tabs
            .iter()
            .find(|(_, tab)| tab.label == label)
            .map(|(id, _)| id.clone())
    }

    pub fn project_id_by_label(&self, label: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .tabs
            .values()
            .find(|tab| tab.label == label)
            .and_then(|tab| tab.project_id.clone())
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

impl BrowserTabs {
    pub fn remove(&self, id: &str) -> CommandResult<Option<String>> {
        let mut guard = self.lock()?;
        let removed = guard.tabs.remove(id);
        let removed_project_id = removed.as_ref().and_then(|tab| tab.project_id.clone());
        let removed_was_active = guard.active.as_deref() == Some(id);

        if let Some(project_id) = removed_project_id.as_deref() {
            let project_fallback = active_tab_id_for_project(&guard, project_id);
            match project_fallback.as_ref() {
                Some(fallback) => {
                    guard
                        .active_by_project
                        .insert(project_id.to_string(), fallback.clone());
                }
                None => {
                    guard.active_by_project.remove(project_id);
                }
            }
            if removed_was_active {
                guard.active = project_fallback;
            }
        } else if removed_was_active {
            guard.active = last_ordered_tab_id(&guard);
        }

        guard.order.retain(|tab_id| tab_id != id);
        Ok(removed.map(|tab| tab.label))
    }
}

fn normalize_owned_project_id(project_id: Option<String>) -> Option<String> {
    project_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_project_id(project_id: Option<&str>) -> Option<&str> {
    project_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn active_tab_id_for_project(guard: &BrowserTabsInner, project_id: &str) -> Option<String> {
    guard
        .active_by_project
        .get(project_id)
        .filter(|id| {
            guard
                .tabs
                .get(*id)
                .is_some_and(|tab| tab.project_id.as_deref() == Some(project_id))
        })
        .cloned()
        .or_else(|| {
            guard
                .order
                .iter()
                .rev()
                .find(|id| {
                    guard
                        .tabs
                        .get(*id)
                        .is_some_and(|tab| tab.project_id.as_deref() == Some(project_id))
                })
                .cloned()
        })
}

fn last_ordered_tab_id(guard: &BrowserTabsInner) -> Option<String> {
    guard
        .order
        .iter()
        .rev()
        .find(|id| guard.tabs.contains_key(*id))
        .cloned()
}

fn tab_matches_project(tab: &TabRecord, project_id: Option<&str>) -> bool {
    match normalize_project_id(project_id) {
        Some(project_id) => tab.project_id.as_deref() == Some(project_id),
        None => true,
    }
}

fn tab_metadata_for_project(
    guard: &BrowserTabsInner,
    project_id: Option<&str>,
) -> Vec<BrowserTabMetadata> {
    let active = guard.active.clone();
    guard
        .order
        .iter()
        .filter_map(|id| guard.tabs.get(id).map(|tab| (id, tab)))
        .filter(|(_, tab)| tab_matches_project(tab, project_id))
        .map(|(id, tab)| BrowserTabMetadata {
            id: id.clone(),
            project_id: tab.project_id.clone(),
            label: tab.label.clone(),
            title: tab.title.clone(),
            url: tab.url.clone(),
            loading: tab.loading,
            can_go_back: tab.can_go_back,
            can_go_forward: tab.can_go_forward,
            active: active.as_deref() == Some(id),
        })
        .collect()
}

fn ensure_tab_belongs_to_project(
    guard: &BrowserTabsInner,
    id: &str,
    project_id: Option<&str>,
) -> CommandResult<()> {
    let tab = guard.tabs.get(id).ok_or_else(|| tab_not_found_error(id))?;
    if let Some(project_id) = normalize_project_id(project_id) {
        if tab.project_id.as_deref() != Some(project_id) {
            return Err(CommandError::user_fixable(
                "browser_tab_not_found",
                format!("Browser tab `{id}` was not found in this project."),
            ));
        }
    }
    Ok(())
}

fn tab_not_found_error(id: &str) -> CommandError {
    CommandError::user_fixable(
        "browser_tab_not_found",
        format!("Browser tab `{id}` was not found."),
    )
}
