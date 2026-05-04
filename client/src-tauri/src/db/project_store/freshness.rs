use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::commands::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessState {
    Current,
    SourceUnknown,
    Stale,
    SourceMissing,
    Superseded,
    Blocked,
}

impl FreshnessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::SourceUnknown => "source_unknown",
            Self::Stale => "stale",
            Self::SourceMissing => "source_missing",
            Self::Superseded => "superseded",
            Self::Blocked => "blocked",
        }
    }
}

pub const SOURCE_FINGERPRINTS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFingerprintInput {
    pub path: String,
    pub source: String,
    pub source_item_id: Option<String>,
    pub operation: Option<String>,
}

impl SourceFingerprintInput {
    pub fn related_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: "related_path".into(),
            source_item_id: None,
            operation: None,
        }
    }

    pub fn agent_file_change(
        path: impl Into<String>,
        source_item_id: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            source: "agent_file_change".into(),
            source_item_id: Some(source_item_id.into()),
            operation: Some(operation.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSourceFingerprints {
    pub freshness_state: FreshnessState,
    pub freshness_checked_at: Option<String>,
    pub stale_reason: Option<String>,
    pub source_fingerprints_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFingerprintDocument {
    pub schema_version: u32,
    pub fingerprints: Vec<SourceFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFingerprint {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    pub exists: bool,
    pub captured_at: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_hash: Option<String>,
}

pub fn source_fingerprints_empty_json() -> String {
    format!(r#"{{"schemaVersion":{SOURCE_FINGERPRINTS_SCHEMA_VERSION},"fingerprints":[]}}"#)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessUpdate {
    pub freshness_state: FreshnessState,
    pub freshness_checked_at: Option<String>,
    pub stale_reason: Option<String>,
    pub source_fingerprints_json: String,
    pub invalidated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersessionUpdate {
    pub superseded_by_id: Option<String>,
    pub supersedes_id: Option<String>,
    pub fact_key: Option<String>,
    pub invalidated_at: Option<String>,
    pub stale_reason: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshnessMetadata<'a> {
    pub freshness_state: &'a str,
    pub freshness_checked_at: Option<&'a str>,
    pub stale_reason: Option<&'a str>,
    pub source_fingerprints_json: &'a str,
    pub supersedes_id: Option<&'a str>,
    pub superseded_by_id: Option<&'a str>,
    pub invalidated_at: Option<&'a str>,
    pub fact_key: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreshnessRefreshSummary {
    pub inspected_count: usize,
    pub updated_count: usize,
    pub current_count: usize,
    pub source_unknown_count: usize,
    pub stale_count: usize,
    pub source_missing_count: usize,
    pub superseded_count: usize,
    pub blocked_count: usize,
}

impl FreshnessRefreshSummary {
    pub fn record_state(&mut self, state: FreshnessState, changed: bool) {
        self.inspected_count += 1;
        if changed {
            self.updated_count += 1;
        }
        match state {
            FreshnessState::Current => self.current_count += 1,
            FreshnessState::SourceUnknown => self.source_unknown_count += 1,
            FreshnessState::Stale => self.stale_count += 1,
            FreshnessState::SourceMissing => self.source_missing_count += 1,
            FreshnessState::Superseded => self.superseded_count += 1,
            FreshnessState::Blocked => self.blocked_count += 1,
        }
    }

    pub fn merge(&mut self, other: FreshnessRefreshSummary) {
        self.inspected_count += other.inspected_count;
        self.updated_count += other.updated_count;
        self.current_count += other.current_count;
        self.source_unknown_count += other.source_unknown_count;
        self.stale_count += other.stale_count;
        self.source_missing_count += other.source_missing_count;
        self.superseded_count += other.superseded_count;
        self.blocked_count += other.blocked_count;
    }

    pub fn as_json(&self) -> JsonValue {
        json!({
            "inspectedCount": self.inspected_count,
            "updatedCount": self.updated_count,
            "currentCount": self.current_count,
            "sourceUnknownCount": self.source_unknown_count,
            "staleCount": self.stale_count,
            "sourceMissingCount": self.source_missing_count,
            "supersededCount": self.superseded_count,
            "blockedCount": self.blocked_count,
            "blockedExcludedCount": self.blocked_count,
        })
    }
}

pub fn parse_freshness_state(value: &str) -> FreshnessState {
    match value {
        "current" => FreshnessState::Current,
        "stale" => FreshnessState::Stale,
        "source_missing" => FreshnessState::SourceMissing,
        "superseded" => FreshnessState::Superseded,
        "blocked" => FreshnessState::Blocked,
        _ => FreshnessState::SourceUnknown,
    }
}

pub fn evaluate_freshness(
    repo_root: &Path,
    previous_state: FreshnessState,
    previous_invalidated_at: Option<&str>,
    source_fingerprints_json: &str,
    checked_at: &str,
    redaction_blocked: bool,
) -> Result<FreshnessUpdate, CommandError> {
    let mut document = decode_source_fingerprints(source_fingerprints_json)?;
    if redaction_blocked || previous_state == FreshnessState::Blocked {
        return Ok(FreshnessUpdate {
            freshness_state: FreshnessState::Blocked,
            freshness_checked_at: Some(checked_at.into()),
            stale_reason: None,
            source_fingerprints_json: encode_source_fingerprints(&document)?,
            invalidated_at: None,
        });
    }
    if previous_state == FreshnessState::Superseded {
        return Ok(FreshnessUpdate {
            freshness_state: FreshnessState::Superseded,
            freshness_checked_at: Some(checked_at.into()),
            stale_reason: Some("Superseded by newer durable context.".into()),
            source_fingerprints_json: encode_source_fingerprints(&document)?,
            invalidated_at: previous_invalidated_at
                .map(str::to_string)
                .or_else(|| Some(checked_at.into())),
        });
    }
    if document.fingerprints.is_empty() {
        return Ok(FreshnessUpdate {
            freshness_state: FreshnessState::SourceUnknown,
            freshness_checked_at: Some(checked_at.into()),
            stale_reason: Some("No checkable source fingerprints are available.".into()),
            source_fingerprints_json: encode_source_fingerprints(&document)?,
            invalidated_at: None,
        });
    }

    let mut missing_paths = Vec::new();
    let mut changed_paths = Vec::new();
    let mut checkable_count = 0_usize;
    for fingerprint in &mut document.fingerprints {
        fingerprint.current_hash = None;
        let Some(relative_path) = normalize_repo_relative_path(repo_root, &fingerprint.path) else {
            continue;
        };
        fingerprint.path = relative_path.clone();
        let absolute_path = repo_root.join(relative_path_to_path_buf(&relative_path));
        if absolute_path.is_file() {
            checkable_count += 1;
            fingerprint.exists = true;
            let current_hash = sha256_file(&absolute_path)?;
            if fingerprint.hash.as_deref() != Some(current_hash.as_str()) {
                fingerprint.current_hash = Some(current_hash);
                changed_paths.push(relative_path);
            }
        } else {
            fingerprint.exists = false;
            missing_paths.push(relative_path);
        }
    }

    let (freshness_state, stale_reason) = if !missing_paths.is_empty() {
        (
            FreshnessState::SourceMissing,
            Some(format!("Source path missing: {}", missing_paths.join(", "))),
        )
    } else if !changed_paths.is_empty() {
        (
            FreshnessState::Stale,
            Some(format!(
                "Source hash changed for {}",
                changed_paths.join(", ")
            )),
        )
    } else if checkable_count == 0 {
        (
            FreshnessState::SourceUnknown,
            Some("No checkable source fingerprints are available.".into()),
        )
    } else {
        (FreshnessState::Current, None)
    };
    let invalidated_at = match freshness_state {
        FreshnessState::Stale | FreshnessState::SourceMissing => previous_invalidated_at
            .map(str::to_string)
            .or_else(|| Some(checked_at.into())),
        _ => None,
    };

    Ok(FreshnessUpdate {
        freshness_state,
        freshness_checked_at: Some(checked_at.into()),
        stale_reason,
        source_fingerprints_json: encode_source_fingerprints(&document)?,
        invalidated_at,
    })
}

pub fn freshness_update_changed(
    state: &str,
    checked_at: Option<&str>,
    stale_reason: Option<&str>,
    source_fingerprints_json: &str,
    invalidated_at: Option<&str>,
    update: &FreshnessUpdate,
) -> bool {
    parse_freshness_state(state) != update.freshness_state
        || checked_at != update.freshness_checked_at.as_deref()
        || stale_reason != update.stale_reason.as_deref()
        || source_fingerprints_json != update.source_fingerprints_json
        || invalidated_at != update.invalidated_at.as_deref()
}

pub fn freshness_metadata_json(metadata: FreshnessMetadata<'_>) -> Result<JsonValue, CommandError> {
    let document = decode_source_fingerprints(metadata.source_fingerprints_json)?;
    Ok(json!({
        "state": metadata.freshness_state,
        "checkedAt": metadata.freshness_checked_at,
        "staleReason": metadata.stale_reason,
        "sourceFingerprints": document.fingerprints.into_iter().map(source_fingerprint_json).collect::<Vec<_>>(),
        "supersedesId": metadata.supersedes_id,
        "supersededById": metadata.superseded_by_id,
        "invalidatedAt": metadata.invalidated_at,
        "factKey": metadata.fact_key,
    }))
}

pub fn source_fingerprint_paths(
    source_fingerprints_json: &str,
) -> Result<Vec<String>, CommandError> {
    Ok(decode_source_fingerprints(source_fingerprints_json)?
        .fingerprints
        .into_iter()
        .map(|fingerprint| fingerprint.path)
        .collect())
}

pub fn source_fingerprint_paths_overlap(
    left_source_fingerprints_json: &str,
    right_source_fingerprints_json: &str,
) -> Result<bool, CommandError> {
    let left_paths = source_fingerprint_paths(left_source_fingerprints_json)?;
    let right_paths = source_fingerprint_paths(right_source_fingerprints_json)?;
    if left_paths.is_empty() || right_paths.is_empty() {
        return Ok(true);
    }
    let left_paths = left_paths.into_iter().collect::<BTreeSet<_>>();
    Ok(right_paths
        .iter()
        .any(|right_path| left_paths.contains(right_path)))
}

fn source_fingerprint_json(fingerprint: SourceFingerprint) -> JsonValue {
    json!({
        "path": fingerprint.path,
        "hash": fingerprint.hash,
        "exists": fingerprint.exists,
        "capturedAt": fingerprint.captured_at,
        "source": fingerprint.source,
        "sourceItemId": fingerprint.source_item_id,
        "operation": fingerprint.operation,
        "currentHash": fingerprint.current_hash,
    })
}

fn decode_source_fingerprints(
    source_fingerprints_json: &str,
) -> Result<SourceFingerprintDocument, CommandError> {
    serde_json::from_str(source_fingerprints_json).map_err(|error| {
        CommandError::system_fault(
            "source_fingerprints_decode_failed",
            format!("Xero could not decode source fingerprints: {error}"),
        )
    })
}

fn encode_source_fingerprints(
    document: &SourceFingerprintDocument,
) -> Result<String, CommandError> {
    serde_json::to_string(document).map_err(|error| {
        CommandError::system_fault(
            "source_fingerprints_serialize_failed",
            format!("Xero could not serialize source fingerprints: {error}"),
        )
    })
}

pub fn capture_source_fingerprints(
    repo_root: &Path,
    sources: impl IntoIterator<Item = SourceFingerprintInput>,
    captured_at: &str,
) -> Result<CapturedSourceFingerprints, CommandError> {
    let mut fingerprints_by_path = BTreeMap::new();

    for source in sources {
        let Some(relative_path) = normalize_repo_relative_path(repo_root, &source.path) else {
            continue;
        };
        let absolute_path = repo_root.join(relative_path_to_path_buf(&relative_path));
        let fingerprint = if absolute_path.is_file() {
            SourceFingerprint {
                path: relative_path.clone(),
                hash: Some(sha256_file(&absolute_path)?),
                exists: true,
                captured_at: captured_at.into(),
                source: source.source,
                source_item_id: source.source_item_id,
                operation: source.operation,
                current_hash: None,
            }
        } else if absolute_path.exists() {
            continue;
        } else {
            SourceFingerprint {
                path: relative_path.clone(),
                hash: None,
                exists: false,
                captured_at: captured_at.into(),
                source: source.source,
                source_item_id: source.source_item_id,
                operation: source.operation,
                current_hash: None,
            }
        };
        fingerprints_by_path.insert(relative_path, fingerprint);
    }

    let fingerprints = fingerprints_by_path.into_values().collect::<Vec<_>>();
    let freshness_state = if fingerprints.is_empty() {
        FreshnessState::SourceUnknown
    } else if fingerprints.iter().any(|fingerprint| !fingerprint.exists) {
        FreshnessState::SourceMissing
    } else {
        FreshnessState::Current
    };
    let stale_reason = if freshness_state == FreshnessState::SourceMissing {
        let missing_paths = fingerprints
            .iter()
            .filter(|fingerprint| !fingerprint.exists)
            .map(|fingerprint| fingerprint.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Source path missing when context was captured: {missing_paths}"
        ))
    } else {
        None
    };
    let freshness_checked_at = match freshness_state {
        FreshnessState::Current | FreshnessState::SourceMissing => Some(captured_at.into()),
        FreshnessState::SourceUnknown
        | FreshnessState::Stale
        | FreshnessState::Superseded
        | FreshnessState::Blocked => None,
    };
    let document = SourceFingerprintDocument {
        schema_version: SOURCE_FINGERPRINTS_SCHEMA_VERSION,
        fingerprints,
    };
    let source_fingerprints_json = encode_source_fingerprints(&document)?;

    Ok(CapturedSourceFingerprints {
        freshness_state,
        freshness_checked_at,
        stale_reason,
        source_fingerprints_json,
    })
}

fn sha256_file(path: &Path) -> Result<String, CommandError> {
    let bytes = fs::read(path).map_err(|error| {
        CommandError::system_fault(
            "source_fingerprint_file_hash_failed",
            format!(
                "Xero could not hash source file {}: {error}",
                path.display()
            ),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn normalize_repo_relative_path(repo_root: &Path, raw_path: &str) -> Option<String> {
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return None;
    }
    let path = Path::new(raw_path);
    let relative = if path.is_absolute() {
        path.strip_prefix(repo_root).ok()?.to_path_buf()
    } else {
        normalize_relative_path(path)?
    };
    let parts = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn normalize_relative_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if normalized.as_os_str().is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn relative_path_to_path_buf(path: &str) -> PathBuf {
    path.split('/').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_source_fingerprints_hashes_existing_files() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path();
        fs::create_dir_all(repo_root.join("src")).expect("src dir");
        fs::write(repo_root.join("src/main.rs"), "fn main() {}\n").expect("write source");

        let captured = capture_source_fingerprints(
            repo_root,
            [SourceFingerprintInput::related_path("src/main.rs")],
            "2026-05-03T00:00:00Z",
        )
        .expect("capture fingerprints");
        let document: SourceFingerprintDocument =
            serde_json::from_str(&captured.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(captured.freshness_state, FreshnessState::Current);
        assert_eq!(document.fingerprints.len(), 1);
        assert_eq!(document.fingerprints[0].path, "src/main.rs");
        assert!(document.fingerprints[0].hash.is_some());
    }

    #[test]
    fn capture_source_fingerprints_marks_missing_paths() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path();

        let captured = capture_source_fingerprints(
            repo_root,
            [SourceFingerprintInput::related_path("src/missing.rs")],
            "2026-05-03T00:00:00Z",
        )
        .expect("capture fingerprints");

        assert_eq!(captured.freshness_state, FreshnessState::SourceMissing);
        assert!(captured
            .stale_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("src/missing.rs")));
    }

    #[test]
    fn evaluate_freshness_marks_changed_hash_stale() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path();
        fs::create_dir_all(repo_root.join("src")).expect("src dir");
        fs::write(repo_root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n")
            .expect("write source");
        let captured = capture_source_fingerprints(
            repo_root,
            [SourceFingerprintInput::related_path("src/lib.rs")],
            "2026-05-03T00:00:00Z",
        )
        .expect("capture fingerprints");
        fs::write(repo_root.join("src/lib.rs"), "pub fn value() -> u8 { 2 }\n")
            .expect("rewrite source");

        let update = evaluate_freshness(
            repo_root,
            FreshnessState::Current,
            None,
            &captured.source_fingerprints_json,
            "2026-05-03T00:01:00Z",
            false,
        )
        .expect("evaluate freshness");
        let document: SourceFingerprintDocument =
            serde_json::from_str(&update.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(update.freshness_state, FreshnessState::Stale);
        assert!(update
            .stale_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("hash changed")));
        assert!(document.fingerprints[0].current_hash.is_some());
    }

    #[test]
    fn evaluate_freshness_marks_missing_current_source() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path();
        fs::create_dir_all(repo_root.join("src")).expect("src dir");
        fs::write(repo_root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n")
            .expect("write source");
        let captured = capture_source_fingerprints(
            repo_root,
            [SourceFingerprintInput::related_path("src/lib.rs")],
            "2026-05-03T00:00:00Z",
        )
        .expect("capture fingerprints");
        fs::remove_file(repo_root.join("src/lib.rs")).expect("delete source");

        let update = evaluate_freshness(
            repo_root,
            FreshnessState::Current,
            None,
            &captured.source_fingerprints_json,
            "2026-05-03T00:01:00Z",
            false,
        )
        .expect("evaluate freshness");
        let document: SourceFingerprintDocument =
            serde_json::from_str(&update.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(update.freshness_state, FreshnessState::SourceMissing);
        assert!(!document.fingerprints[0].exists);
        assert!(update.invalidated_at.is_some());
    }
}
