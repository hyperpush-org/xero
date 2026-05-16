//! Run-execution mode for the TUI.
//!
//! The plan calls for an NDJSON stream consumer reading from
//! `xero agent exec --stream`. The streaming surface is not yet on the
//! headless runtime — until it lands we keep the existing polled snapshot
//! flow and probe for support at startup so the redesign ships without
//! waiting on the backend phase. When `--stream` becomes available the
//! probe flips to `Streaming` automatically and the rest of the TUI gains
//! per-token rendering with no further changes.

use std::sync::OnceLock;

use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::app::invoke_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    /// Read the run via `xero conversation show` until the worker thread
    /// reports completion. Tool-pill durations tick from the local
    /// `Instant` captured when the job started.
    Polled,
    /// Consume NDJSON frames from `xero agent exec --stream`. Reserved for
    /// when the backend lands the flag.
    #[allow(dead_code)]
    Streaming,
}

static MODE_CACHE: OnceLock<RuntimeMode> = OnceLock::new();

/// Decide which mode to use for this TUI session. Cached so the probe
/// fires exactly once per process.
pub fn mode(globals: &GlobalOptions) -> RuntimeMode {
    *MODE_CACHE.get_or_init(|| probe_mode(globals))
}

fn probe_mode(globals: &GlobalOptions) -> RuntimeMode {
    // Operators can force a mode via env var — useful for local dev when
    // the streaming surface ships behind a feature flag.
    if let Ok(value) = std::env::var("XERO_TUI_RUNTIME_MODE") {
        match value.as_str() {
            "stream" | "streaming" => return RuntimeMode::Streaming,
            "poll" | "polled" => return RuntimeMode::Polled,
            _ => {}
        }
    }
    if supports_streaming(globals) {
        RuntimeMode::Streaming
    } else {
        RuntimeMode::Polled
    }
}

fn supports_streaming(globals: &GlobalOptions) -> bool {
    // `xero agent exec --help` returns a JSON `command` object; once the
    // backend advertises a `--stream` flag the help payload will include
    // it. Until then this probe consistently picks `Polled`.
    let Ok(value) = invoke_json(globals, &["agent", "exec", "--help"]) else {
        return false;
    };
    help_text_mentions_stream(&value)
}

fn help_text_mentions_stream(value: &JsonValue) -> bool {
    fn search(value: &JsonValue) -> bool {
        match value {
            JsonValue::String(text) => text.contains("--stream"),
            JsonValue::Array(items) => items.iter().any(search),
            JsonValue::Object(map) => map.values().any(search),
            _ => false,
        }
    }
    search(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_stream_flag_in_help_payload() {
        let payload = json!({
            "command": "agent exec",
            "help": "Usage: xero agent exec --stream [PROMPT] ...",
        });
        assert!(help_text_mentions_stream(&payload));
    }

    #[test]
    fn no_stream_flag_means_polled() {
        let payload = json!({
            "command": "agent exec",
            "help": "Usage: xero agent exec [PROMPT] --provider ID",
        });
        assert!(!help_text_mentions_stream(&payload));
    }
}
