use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    runtime::redaction::redact_json_for_persistence,
};

const MAX_REFS_PER_SNAPSHOT: usize = 400;
const MAX_TIMELINE_EVENTS: usize = 500;
const MAX_ANNOTATIONS: usize = 200;
const MAX_RECORDINGS: usize = 100;

#[derive(Debug, Default)]
pub struct BrowserAutomationState {
    refs: Mutex<BrowserRefStore>,
    timeline: Mutex<Vec<BrowserTimelineEvent>>,
    annotations: Mutex<BTreeMap<String, BrowserAnnotation>>,
    recordings: Mutex<BTreeMap<String, BrowserRecording>>,
    action_cache: Mutex<BTreeMap<String, BrowserActionCacheEntry>>,
    next_annotation_id: AtomicU64,
    next_recording_id: AtomicU64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRefStore {
    pub version: u64,
    pub url: Option<String>,
    pub title: Option<String>,
    pub mode: String,
    pub created_at: Option<String>,
    pub page_signature: Option<String>,
    pub refs: Vec<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTimelineEvent {
    pub sequence: u64,
    pub action: String,
    pub engine: String,
    pub status: String,
    pub summary: String,
    pub url: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAnnotation {
    pub id: String,
    pub kind: String,
    pub note: Option<String>,
    pub target_ref: Option<String>,
    pub region: Option<JsonValue>,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecording {
    pub id: String,
    pub status: String,
    pub sensitive_mode: bool,
    pub started_at: String,
    pub updated_at: String,
    pub timeline_start_sequence: u64,
    pub timeline_end_sequence: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActionCacheEntry {
    pub key: String,
    pub url_signature: String,
    pub intent: String,
    pub selector_candidates: Vec<String>,
    pub confidence: u8,
    pub last_success_at: Option<String>,
    pub last_success: Option<JsonValue>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBrowserRef {
    pub version: u64,
    pub element_index: usize,
}

impl BrowserAutomationState {
    pub fn store_snapshot(&self, snapshot: JsonValue, mode: &str) -> CommandResult<JsonValue> {
        self.store_snapshot_for_engine(snapshot, mode, "in_app")
    }

    pub fn store_snapshot_for_engine(
        &self,
        mut snapshot: JsonValue,
        mode: &str,
        source_engine: &str,
    ) -> CommandResult<JsonValue> {
        let object = snapshot.as_object_mut().ok_or_else(|| {
            CommandError::system_fault(
                "browser_snapshot_invalid",
                "Browser snapshot script returned a non-object payload.",
            )
        })?;
        let url = object
            .get("url")
            .and_then(JsonValue::as_str)
            .map(str::to_owned);
        let title = object
            .get("title")
            .and_then(JsonValue::as_str)
            .map(str::to_owned);
        let nodes = object
            .get_mut("refs")
            .and_then(JsonValue::as_array_mut)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "browser_snapshot_invalid",
                    "Browser snapshot script did not return a refs array.",
                )
            })?;

        let mut store = self.refs.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_ref_store_lock_poisoned",
                "Browser ref store lock poisoned.",
            )
        })?;
        store.version = store.version.saturating_add(1).max(1);
        store.url = url;
        store.title = title;
        store.mode = mode.to_owned();
        store.created_at = Some(now_timestamp());
        store.page_signature = Some(page_signature(&store.url, &store.title, nodes.len()));
        store.refs.clear();

        for (index, node) in nodes.iter_mut().take(MAX_REFS_PER_SNAPSHOT).enumerate() {
            if let Some(node_object) = node.as_object_mut() {
                let ref_id = format!("@v{}:e{}", store.version, index + 1);
                node_object.insert("ref".into(), JsonValue::String(ref_id));
                node_object.insert("refIndex".into(), json!(index + 1));
                node_object.insert(
                    "sourceEngine".into(),
                    JsonValue::String(source_engine.to_owned()),
                );
                node_object.insert("snapshotVersion".into(), json!(store.version));
                node_object.insert("pageSignature".into(), json!(store.page_signature.clone()));
                store.refs.push(JsonValue::Object(node_object.clone()));
            }
        }

        object.insert("schema".into(), json!("xero.browser_snapshot.v1"));
        object.insert("version".into(), json!(store.version));
        object.insert("mode".into(), JsonValue::String(mode.to_owned()));
        object.insert("createdAt".into(), json!(store.created_at.clone()));
        object.insert("pageSignature".into(), json!(store.page_signature.clone()));
        object.insert(
            "refs".into(),
            JsonValue::Array(
                store
                    .refs
                    .iter()
                    .take(MAX_REFS_PER_SNAPSHOT)
                    .cloned()
                    .collect(),
            ),
        );
        object.insert(
            "resnapshotGuidance".into(),
            json!("Use the current refs only for this snapshot version. Re-run snapshot if the page changes or a ref is stale."),
        );

        Ok(snapshot)
    }

    pub fn latest_snapshot(&self) -> CommandResult<BrowserRefStore> {
        self.refs.lock().map(|store| store.clone()).map_err(|_| {
            CommandError::system_fault(
                "browser_ref_store_lock_poisoned",
                "Browser ref store lock poisoned.",
            )
        })
    }

    pub fn get_ref(&self, ref_id: &str) -> CommandResult<JsonValue> {
        let parsed = parse_browser_ref(ref_id)?;
        let store = self.latest_snapshot()?;
        if store.version == 0 || store.refs.is_empty() {
            return Err(CommandError::user_fixable(
                "browser_ref_snapshot_missing",
                "No browser snapshot refs are available. Run the snapshot action first.",
            ));
        }
        if parsed.version != store.version {
            return Err(CommandError::user_fixable(
                "browser_ref_stale",
                format!(
                    "Browser ref `{ref_id}` belongs to snapshot v{}, but the current snapshot is v{}. Run snapshot again and use a fresh ref.",
                    parsed.version, store.version
                ),
            ));
        }
        store
            .refs
            .get(parsed.element_index.saturating_sub(1))
            .cloned()
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_ref_not_found",
                    format!("Browser ref `{ref_id}` does not exist in the current snapshot."),
                )
            })
    }

    pub fn selector_for_ref(&self, ref_id: &str) -> CommandResult<String> {
        let node = self.get_ref(ref_id)?;
        selector_candidates_for_node(&node)
            .into_iter()
            .next()
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "browser_ref_selector_missing",
                    format!("Browser ref `{ref_id}` has no usable selector candidates."),
                )
            })
    }

    pub fn selector_candidates_for_ref(&self, ref_id: &str) -> CommandResult<Vec<String>> {
        let node = self.get_ref(ref_id)?;
        Ok(selector_candidates_for_node(&node))
    }

    pub fn push_timeline(
        &self,
        action: impl Into<String>,
        engine: impl Into<String>,
        status: impl Into<String>,
        summary: impl Into<String>,
        url: Option<String>,
        started_at: String,
        evidence_refs: Vec<String>,
    ) -> CommandResult<BrowserTimelineEvent> {
        let mut timeline = self.timeline.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_timeline_lock_poisoned",
                "Browser timeline lock poisoned.",
            )
        })?;
        let sequence = timeline
            .last()
            .map(|event| event.sequence.saturating_add(1))
            .unwrap_or(1);
        let event = BrowserTimelineEvent {
            sequence,
            action: action.into(),
            engine: engine.into(),
            status: status.into(),
            summary: summary.into(),
            url,
            started_at,
            finished_at: now_timestamp(),
            evidence_refs,
        };
        timeline.push(event.clone());
        if timeline.len() > MAX_TIMELINE_EVENTS {
            let drain = timeline.len() - MAX_TIMELINE_EVENTS;
            timeline.drain(0..drain);
        }
        Ok(event)
    }

    pub fn timeline(
        &self,
        limit: Option<usize>,
        clear: bool,
    ) -> CommandResult<Vec<BrowserTimelineEvent>> {
        let mut timeline = self.timeline.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_timeline_lock_poisoned",
                "Browser timeline lock poisoned.",
            )
        })?;
        let limit = limit.unwrap_or(100).min(MAX_TIMELINE_EVENTS);
        let selected = timeline
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        if clear {
            timeline.clear();
        }
        Ok(selected)
    }

    pub fn timeline_latest_sequence(&self) -> u64 {
        self.timeline
            .lock()
            .ok()
            .and_then(|timeline| timeline.last().map(|event| event.sequence))
            .unwrap_or(0)
    }

    pub fn cache_key(url_signature: &str, intent: &str) -> String {
        format!("{}::{}", url_signature, intent.trim().to_ascii_lowercase())
    }

    pub fn get_cached_action(
        &self,
        url_signature: &str,
        intent: &str,
    ) -> CommandResult<Option<BrowserActionCacheEntry>> {
        let key = Self::cache_key(url_signature, intent);
        self.action_cache
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_action_cache_lock_poisoned",
                    "Browser action cache lock poisoned.",
                )
            })
            .map(|cache| cache.get(&key).cloned())
    }

    pub fn put_cached_action(
        &self,
        url_signature: &str,
        intent: &str,
        selector_candidates: Vec<String>,
        confidence: u8,
    ) -> CommandResult<BrowserActionCacheEntry> {
        let key = Self::cache_key(url_signature, intent);
        let entry = BrowserActionCacheEntry {
            key: key.clone(),
            url_signature: url_signature.to_owned(),
            intent: intent.to_owned(),
            selector_candidates,
            confidence,
            last_success_at: Some(now_timestamp()),
            last_success: None,
            updated_at: now_timestamp(),
        };
        self.action_cache
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_action_cache_lock_poisoned",
                    "Browser action cache lock poisoned.",
                )
            })?
            .insert(key, entry.clone());
        Ok(entry)
    }

    pub fn action_cache_entries(&self) -> CommandResult<Vec<BrowserActionCacheEntry>> {
        self.action_cache
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_action_cache_lock_poisoned",
                    "Browser action cache lock poisoned.",
                )
            })
            .map(|cache| cache.values().cloned().collect())
    }

    pub fn clear_action_cache(&self) -> CommandResult<usize> {
        let mut cache = self.action_cache.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_action_cache_lock_poisoned",
                "Browser action cache lock poisoned.",
            )
        })?;
        let count = cache.len();
        cache.clear();
        Ok(count)
    }

    pub fn create_annotation(
        &self,
        kind: String,
        note: Option<String>,
        target_ref: Option<String>,
        region: Option<JsonValue>,
    ) -> CommandResult<BrowserAnnotation> {
        let id = format!(
            "ann-{}",
            self.next_annotation_id.fetch_add(1, Ordering::AcqRel) + 1
        );
        let annotation = BrowserAnnotation {
            id: id.clone(),
            kind,
            note,
            target_ref,
            region,
            status: "open".into(),
            created_at: now_timestamp(),
            resolved_at: None,
        };
        let mut annotations = self.annotations.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_annotations_lock_poisoned",
                "Browser annotations lock poisoned.",
            )
        })?;
        annotations.insert(id, annotation.clone());
        trim_btree_map(&mut annotations, MAX_ANNOTATIONS);
        Ok(annotation)
    }

    pub fn resolve_annotation(&self, id: &str) -> CommandResult<BrowserAnnotation> {
        let mut annotations = self.annotations.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_annotations_lock_poisoned",
                "Browser annotations lock poisoned.",
            )
        })?;
        let annotation = annotations.get_mut(id).ok_or_else(|| {
            CommandError::user_fixable(
                "browser_annotation_not_found",
                format!("Browser annotation `{id}` was not found."),
            )
        })?;
        annotation.status = "resolved".into();
        annotation.resolved_at = Some(now_timestamp());
        Ok(annotation.clone())
    }

    pub fn clear_annotations(&self) -> CommandResult<Vec<BrowserAnnotation>> {
        let mut annotations = self.annotations.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_annotations_lock_poisoned",
                "Browser annotations lock poisoned.",
            )
        })?;
        let cleared = annotations.values().cloned().collect();
        annotations.clear();
        Ok(cleared)
    }

    pub fn annotations(&self) -> CommandResult<Vec<BrowserAnnotation>> {
        self.annotations
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_annotations_lock_poisoned",
                    "Browser annotations lock poisoned.",
                )
            })
            .map(|annotations| annotations.values().cloned().collect())
    }

    pub fn start_recording(&self, sensitive_mode: bool) -> CommandResult<BrowserRecording> {
        let id = format!(
            "rec-{}",
            self.next_recording_id.fetch_add(1, Ordering::AcqRel) + 1
        );
        let now = now_timestamp();
        let recording = BrowserRecording {
            id: id.clone(),
            status: "recording".into(),
            sensitive_mode,
            started_at: now.clone(),
            updated_at: now,
            timeline_start_sequence: self.timeline_latest_sequence().saturating_add(1),
            timeline_end_sequence: None,
        };
        let mut recordings = self.recordings.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_recordings_lock_poisoned",
                "Browser recordings lock poisoned.",
            )
        })?;
        recordings.insert(id, recording.clone());
        trim_btree_map(&mut recordings, MAX_RECORDINGS);
        Ok(recording)
    }

    pub fn update_recording_status(
        &self,
        id: &str,
        status: &str,
    ) -> CommandResult<BrowserRecording> {
        let mut recordings = self.recordings.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_recordings_lock_poisoned",
                "Browser recordings lock poisoned.",
            )
        })?;
        let recording = recordings.get_mut(id).ok_or_else(|| {
            CommandError::user_fixable(
                "browser_recording_not_found",
                format!("Browser recording `{id}` was not found."),
            )
        })?;
        recording.status = status.to_owned();
        recording.updated_at = now_timestamp();
        if matches!(status, "stopped" | "paused" | "discarded") {
            recording.timeline_end_sequence = Some(self.timeline_latest_sequence());
        }
        Ok(recording.clone())
    }

    pub fn discard_recording(&self, id: &str) -> CommandResult<BrowserRecording> {
        let mut recording = self.update_recording_status(id, "discarded")?;
        self.recordings
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_recordings_lock_poisoned",
                    "Browser recordings lock poisoned.",
                )
            })?
            .remove(id);
        recording.status = "discarded".into();
        Ok(recording)
    }

    pub fn recordings(&self) -> CommandResult<Vec<BrowserRecording>> {
        self.recordings
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "browser_recordings_lock_poisoned",
                    "Browser recordings lock poisoned.",
                )
            })
            .map(|recordings| recordings.values().cloned().collect())
    }
}

pub fn parse_browser_ref(value: &str) -> CommandResult<ParsedBrowserRef> {
    let Some(rest) = value.strip_prefix("@v") else {
        return Err(invalid_ref_error(value));
    };
    let Some((version, element)) = rest.split_once(":e") else {
        return Err(invalid_ref_error(value));
    };
    let version = version
        .parse::<u64>()
        .map_err(|_| invalid_ref_error(value))?;
    let element_index = element
        .parse::<usize>()
        .map_err(|_| invalid_ref_error(value))?;
    if version == 0 || element_index == 0 {
        return Err(invalid_ref_error(value));
    }
    Ok(ParsedBrowserRef {
        version,
        element_index,
    })
}

pub fn selector_candidates_for_node(node: &JsonValue) -> Vec<String> {
    let mut selectors = Vec::new();
    if let Some(meta) = node.get("selectorMeta").and_then(JsonValue::as_array) {
        let mut entries = meta
            .iter()
            .filter_map(|entry| {
                let selector = entry.get("selector").and_then(JsonValue::as_str)?.trim();
                if selector.is_empty() {
                    return None;
                }
                Some((
                    selector.to_owned(),
                    entry
                        .get("unique")
                        .and_then(JsonValue::as_bool)
                        .unwrap_or(false),
                    entry
                        .get("roleOnly")
                        .and_then(JsonValue::as_bool)
                        .unwrap_or(false),
                ))
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_selector, unique, role_only)| (!*unique, *role_only));
        selectors.extend(entries.into_iter().map(|entry| entry.0));
    }
    selectors.extend(
        node.get("selectorCandidates")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(JsonValue::as_str)
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
            .map(str::to_owned),
    );
    let mut deduped = Vec::new();
    for selector in selectors {
        if !deduped.iter().any(|seen| seen == &selector) {
            deduped.push(selector);
        }
    }
    deduped
}

pub fn url_signature_for_cache(url: Option<&str>, title: Option<&str>) -> String {
    let url = url.unwrap_or_default();
    let title = title.unwrap_or_default();
    format!("{url}#{title}")
}

pub fn write_browser_artifact(
    artifact_root: &Path,
    family: &str,
    prefix: &str,
    payload: &JsonValue,
) -> CommandResult<PathBuf> {
    let directory = artifact_root.join(family);
    fs::create_dir_all(&directory).map_err(|error| {
        CommandError::retryable(
            "browser_artifact_dir_failed",
            format!(
                "Xero could not prepare browser artifact directory at {}: {error}",
                directory.display()
            ),
        )
    })?;
    let file_name = format!(
        "{}-{}.json",
        prefix,
        now_timestamp().replace([':', '.'], "-")
    );
    let path = directory.join(file_name);
    let (redacted, _changed) = redact_json_for_persistence(payload);
    let bytes = serde_json::to_vec_pretty(&redacted).map_err(|error| {
        CommandError::system_fault(
            "browser_artifact_encode_failed",
            format!("Xero could not encode browser artifact JSON: {error}"),
        )
    })?;
    fs::write(&path, bytes).map_err(|error| {
        CommandError::retryable(
            "browser_artifact_write_failed",
            format!(
                "Xero could not write browser artifact at {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(path)
}

pub fn validate_browser_artifact_manifest(payload: &JsonValue) -> JsonValue {
    let schema_ok = payload
        .get("schema")
        .and_then(JsonValue::as_str)
        .is_some_and(|schema| schema.starts_with("xero.browser_"));
    let has_manifest = payload.get("manifest").is_some() || payload.get("timeline").is_some();
    json!({
        "valid": schema_ok && has_manifest,
        "schemaOk": schema_ok,
        "hasManifestOrTimeline": has_manifest,
        "checkedAt": now_timestamp(),
    })
}

fn page_signature(url: &Option<String>, title: &Option<String>, ref_count: usize) -> String {
    format!(
        "{}#{}#{}",
        url.as_deref().unwrap_or_default(),
        title.as_deref().unwrap_or_default(),
        ref_count
    )
}

fn invalid_ref_error(value: &str) -> CommandError {
    CommandError::user_fixable(
        "browser_ref_invalid",
        format!("Browser ref `{value}` must use the format @v<snapshot>:e<element>, for example @v1:e1."),
    )
}

fn trim_btree_map<T>(map: &mut BTreeMap<String, T>, max_len: usize) {
    while map.len() > max_len {
        let Some(first_key) = map.keys().next().cloned() else {
            break;
        };
        map.remove(&first_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_browser_ref_accepts_versioned_refs() {
        let parsed = parse_browser_ref("@v12:e7").expect("valid ref");
        assert_eq!(parsed.version, 12);
        assert_eq!(parsed.element_index, 7);
    }

    #[test]
    fn parse_browser_ref_rejects_malformed_refs() {
        for value in ["", "v1:e1", "@v0:e1", "@v1:e0", "@v1", "@v1:n1"] {
            assert!(parse_browser_ref(value).is_err(), "{value} should fail");
        }
    }

    #[test]
    fn store_snapshot_assigns_refs_and_detects_stale_versions() {
        let state = BrowserAutomationState::default();
        let first = state
            .store_snapshot(
                json!({
                    "url": "https://example.com",
                    "title": "Example",
                    "refs": [
                        { "tag": "button", "selectorCandidates": ["#go"] }
                    ]
                }),
                "interactive",
            )
            .expect("snapshot");
        assert_eq!(first["refs"][0]["ref"], "@v1:e1");
        assert!(state.get_ref("@v1:e1").is_ok());

        let second = state
            .store_snapshot(
                json!({
                    "url": "https://example.com/next",
                    "title": "Next",
                    "refs": [
                        { "tag": "input", "selectorCandidates": ["#email"] }
                    ]
                }),
                "form",
            )
            .expect("snapshot");
        assert_eq!(second["refs"][0]["ref"], "@v2:e1");
        let stale = state.get_ref("@v1:e1").expect_err("stale ref");
        assert_eq!(stale.code, "browser_ref_stale");
    }

    #[test]
    fn artifact_validator_requires_browser_schema_and_manifest() {
        let valid = validate_browser_artifact_manifest(&json!({
            "schema": "xero.browser_artifact_bundle.v1",
            "manifest": {}
        }));
        assert_eq!(valid["valid"], true);

        let invalid = validate_browser_artifact_manifest(&json!({"schema": "other"}));
        assert_eq!(invalid["valid"], false);
    }
}
