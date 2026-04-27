use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
};

const MAX_BROWSER_DIAGNOSTIC_ENTRIES_PER_TAB: usize = 500;
const MAX_BROWSER_DIAGNOSTIC_MESSAGE_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserConsoleDiagnosticEntry {
    pub sequence: u64,
    pub tab_id: String,
    pub level: String,
    pub message: String,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserNetworkDiagnosticEntry {
    pub sequence: u64,
    pub tab_id: String,
    pub url: String,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub ok: Option<bool>,
    pub resource_type: Option<String>,
    pub duration_ms: Option<u64>,
    pub transfer_size: Option<u64>,
    pub error: Option<String>,
    pub captured_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserDiagnosticReadOptions<'a> {
    pub tab_id: Option<&'a str>,
    pub level: Option<&'a str>,
    pub limit: usize,
    pub clear: bool,
}

impl<'a> BrowserDiagnosticReadOptions<'a> {
    pub fn console(
        tab_id: Option<&'a str>,
        level: Option<&'a str>,
        limit: Option<usize>,
        clear: bool,
    ) -> Self {
        Self {
            tab_id,
            level,
            limit: limit
                .unwrap_or(100)
                .clamp(1, MAX_BROWSER_DIAGNOSTIC_ENTRIES_PER_TAB),
            clear,
        }
    }

    pub fn network(tab_id: Option<&'a str>, limit: Option<usize>, clear: bool) -> Self {
        Self {
            tab_id,
            level: None,
            limit: limit
                .unwrap_or(100)
                .clamp(1, MAX_BROWSER_DIAGNOSTIC_ENTRIES_PER_TAB),
            clear,
        }
    }
}

#[derive(Debug, Default)]
pub struct BrowserDiagnostics {
    console_entries: Mutex<BTreeMap<String, Vec<BrowserConsoleDiagnosticEntry>>>,
    network_entries: Mutex<BTreeMap<String, Vec<BrowserNetworkDiagnosticEntry>>>,
    next_sequence: AtomicU64,
}

impl BrowserDiagnostics {
    pub fn push_console(&self, tab_id: &str, level: &str, message: &str) -> CommandResult<()> {
        let entry = BrowserConsoleDiagnosticEntry {
            sequence: self.next_sequence(),
            tab_id: normalized_tab_id(tab_id)?,
            level: normalized_level(level),
            message: truncate_diagnostic_text(message),
            captured_at: now_timestamp(),
        };
        let mut entries = self
            .console_entries
            .lock()
            .map_err(diagnostics_lock_error)?;
        let bucket = entries.entry(entry.tab_id.clone()).or_default();
        bucket.push(entry);
        prune_entries(bucket);
        Ok(())
    }

    pub fn push_network(&self, tab_id: &str, payload: &JsonValue) -> CommandResult<()> {
        let entry = BrowserNetworkDiagnosticEntry {
            sequence: self.next_sequence(),
            tab_id: normalized_tab_id(tab_id)?,
            url: payload
                .get("url")
                .and_then(|value| value.as_str())
                .map(sanitize_url)
                .unwrap_or_else(|| "unknown".into()),
            method: payload
                .get("method")
                .and_then(|value| value.as_str())
                .map(normalized_method),
            status: payload
                .get("status")
                .and_then(|value| value.as_u64())
                .and_then(|value| u16::try_from(value).ok()),
            ok: payload.get("ok").and_then(|value| value.as_bool()),
            resource_type: payload
                .get("type")
                .or_else(|| payload.get("resourceType"))
                .and_then(|value| value.as_str())
                .map(|value| truncate_diagnostic_text(value).to_ascii_lowercase()),
            duration_ms: payload
                .get("durationMs")
                .or_else(|| payload.get("duration"))
                .and_then(|value| value.as_f64())
                .and_then(nonnegative_u64_from_f64),
            transfer_size: payload.get("transferSize").and_then(|value| value.as_u64()),
            error: payload
                .get("error")
                .and_then(|value| value.as_str())
                .map(truncate_diagnostic_text),
            captured_at: now_timestamp(),
        };
        let mut entries = self
            .network_entries
            .lock()
            .map_err(diagnostics_lock_error)?;
        let bucket = entries.entry(entry.tab_id.clone()).or_default();
        bucket.push(entry);
        prune_entries(bucket);
        Ok(())
    }

    pub fn console_entries(
        &self,
        options: BrowserDiagnosticReadOptions<'_>,
    ) -> CommandResult<Vec<BrowserConsoleDiagnosticEntry>> {
        let mut entries = self
            .console_entries
            .lock()
            .map_err(diagnostics_lock_error)?;
        let mut selected = Vec::new();
        for (tab_id, bucket) in entries.iter() {
            if options.tab_id.is_some_and(|target| target != tab_id) {
                continue;
            }
            selected.extend(bucket.iter().filter(|entry| {
                options
                    .level
                    .map(normalized_level)
                    .map_or(true, |level| entry.level == level)
            }));
        }
        selected.sort_by_key(|entry| entry.sequence);
        let selected = selected
            .into_iter()
            .rev()
            .take(options.limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        if options.clear {
            clear_selected_console_entries(&mut entries, &selected);
        }

        Ok(selected)
    }

    pub fn network_entries(
        &self,
        options: BrowserDiagnosticReadOptions<'_>,
    ) -> CommandResult<Vec<BrowserNetworkDiagnosticEntry>> {
        let mut entries = self
            .network_entries
            .lock()
            .map_err(diagnostics_lock_error)?;
        let mut selected = Vec::new();
        for (tab_id, bucket) in entries.iter() {
            if options.tab_id.is_some_and(|target| target != tab_id) {
                continue;
            }
            selected.extend(bucket.iter());
        }
        selected.sort_by_key(|entry| entry.sequence);
        let selected = selected
            .into_iter()
            .rev()
            .take(options.limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        if options.clear {
            clear_selected_network_entries(&mut entries, &selected);
        }

        Ok(selected)
    }

    fn next_sequence(&self) -> u64 {
        self.next_sequence
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }
}

fn diagnostics_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "browser_diagnostics_lock_poisoned",
        "Browser diagnostics registry lock poisoned.",
    )
}

fn normalized_tab_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("tabId"));
    }
    Ok(trimmed.to_owned())
}

fn normalized_level(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "error" => "error".into(),
        "warn" | "warning" => "warn".into(),
        "info" => "info".into(),
        "debug" => "debug".into(),
        _ => "log".into(),
    }
}

fn normalized_method(value: &str) -> String {
    let method = value.trim().to_ascii_uppercase();
    if method.is_empty() {
        "GET".into()
    } else {
        truncate_diagnostic_text(&method)
    }
}

fn sanitize_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "unknown".into();
    }
    match url::Url::parse(trimmed) {
        Ok(mut url) => {
            url.set_query(None);
            url.set_fragment(None);
            truncate_diagnostic_text(url.as_str())
        }
        Err(_) => truncate_diagnostic_text(trimmed),
    }
}

fn truncate_diagnostic_text(value: &str) -> String {
    if value.chars().count() <= MAX_BROWSER_DIAGNOSTIC_MESSAGE_CHARS {
        return value.to_owned();
    }
    let truncated = value
        .chars()
        .take(MAX_BROWSER_DIAGNOSTIC_MESSAGE_CHARS.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}...")
}

fn nonnegative_u64_from_f64(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(value.round().min(u64::MAX as f64) as u64)
}

fn prune_entries<T>(entries: &mut Vec<T>) {
    if entries.len() > MAX_BROWSER_DIAGNOSTIC_ENTRIES_PER_TAB {
        let drop_count = entries.len() - MAX_BROWSER_DIAGNOSTIC_ENTRIES_PER_TAB;
        entries.drain(0..drop_count);
    }
}

fn clear_selected_console_entries(
    entries: &mut BTreeMap<String, Vec<BrowserConsoleDiagnosticEntry>>,
    selected: &[BrowserConsoleDiagnosticEntry],
) {
    let selected = selected
        .iter()
        .map(|entry| (entry.tab_id.clone(), entry.sequence))
        .collect::<std::collections::BTreeSet<_>>();
    for bucket in entries.values_mut() {
        bucket.retain(|entry| !selected.contains(&(entry.tab_id.clone(), entry.sequence)));
    }
}

fn clear_selected_network_entries(
    entries: &mut BTreeMap<String, Vec<BrowserNetworkDiagnosticEntry>>,
    selected: &[BrowserNetworkDiagnosticEntry],
) {
    let selected = selected
        .iter()
        .map(|entry| (entry.tab_id.clone(), entry.sequence))
        .collect::<std::collections::BTreeSet<_>>();
    for bucket in entries.values_mut() {
        bucket.retain(|entry| !selected.contains(&(entry.tab_id.clone(), entry.sequence)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_entries_are_filtered_limited_and_clearable() {
        let diagnostics = BrowserDiagnostics::default();
        diagnostics
            .push_console("tab-a", "log", "first")
            .expect("push first");
        diagnostics
            .push_console("tab-a", "warning", "second")
            .expect("push second");
        diagnostics
            .push_console("tab-b", "error", "third")
            .expect("push third");

        let warn_entries = diagnostics
            .console_entries(BrowserDiagnosticReadOptions::console(
                Some("tab-a"),
                Some("warn"),
                Some(5),
                false,
            ))
            .expect("read warning entries");
        assert_eq!(warn_entries.len(), 1);
        assert_eq!(warn_entries[0].message, "second");

        let cleared = diagnostics
            .console_entries(BrowserDiagnosticReadOptions::console(
                None,
                None,
                Some(2),
                true,
            ))
            .expect("clear entries");
        assert_eq!(cleared.len(), 2);

        let remaining = diagnostics
            .console_entries(BrowserDiagnosticReadOptions::console(
                None,
                None,
                Some(10),
                false,
            ))
            .expect("read remaining");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message, "first");
    }

    #[test]
    fn network_entries_sanitize_urls_and_payloads() {
        let diagnostics = BrowserDiagnostics::default();
        diagnostics
            .push_network(
                "tab-a",
                &serde_json::json!({
                    "url": "https://example.com/path?token=secret#frag",
                    "method": "post",
                    "status": 201,
                    "ok": true,
                    "type": "fetch",
                    "durationMs": 12.4,
                    "transferSize": 1234
                }),
            )
            .expect("push network");

        let entries = diagnostics
            .network_entries(BrowserDiagnosticReadOptions::network(
                Some("tab-a"),
                Some(10),
                false,
            ))
            .expect("read network entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://example.com/path");
        assert_eq!(entries[0].method.as_deref(), Some("POST"));
        assert_eq!(entries[0].status, Some(201));
        assert_eq!(entries[0].duration_ms, Some(12));
    }
}
