use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    time::Duration,
};

use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime};

use crate::commands::{CommandError, CommandResult};

use super::tabs::BrowserTabs;

pub const BRIDGE_DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const BRIDGE_MAX_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone)]
pub struct BridgeReply {
    pub ok: bool,
    pub value: Option<JsonValue>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct BridgeWaiters {
    counter: AtomicU64,
    waiters: Mutex<HashMap<String, SyncSender<BridgeReply>>>,
}

impl BridgeWaiters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_id(&self) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        format!("cad-{next:x}")
    }

    pub fn register(&self, id: &str) -> CommandResult<std::sync::mpsc::Receiver<BridgeReply>> {
        let (sender, receiver) = sync_channel::<BridgeReply>(1);
        let mut guard = self.waiters.lock().map_err(|_| {
            CommandError::system_fault(
                "browser_bridge_lock_poisoned",
                "Browser bridge waiter lock poisoned.",
            )
        })?;
        guard.insert(id.to_string(), sender);
        Ok(receiver)
    }

    pub fn resolve(&self, id: &str, reply: BridgeReply) {
        let Ok(mut guard) = self.waiters.lock() else {
            return;
        };
        if let Some(sender) = guard.remove(id) {
            let _ = sender.try_send(reply);
        }
    }

    pub fn cancel(&self, id: &str) {
        let Ok(mut guard) = self.waiters.lock() else {
            return;
        };
        guard.remove(id);
    }
}

/// Executes arbitrary JS in the active browser tab and returns the JSON-decoded result.
///
/// `body` is wrapped in an async IIFE; its `return` value is delivered back via the bridge.
pub fn run_script<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    body: &str,
    timeout_ms: u64,
) -> CommandResult<JsonValue> {
    let timeout = timeout_ms.clamp(100, BRIDGE_MAX_TIMEOUT_MS);
    let webview = tabs.active_webview(app)?;
    let request_id = waiters.next_id();
    let receiver = waiters.register(&request_id)?;

    let encoded_id = serde_json::to_string(&request_id).unwrap_or_else(|_| "\"\"".to_string());
    let encoded_body = serde_json::to_string(body).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        "(function(){{\
            try {{\
              if (!window.__cadenceBridge__) {{\
                window.__cadenceBridge__ = {{}};\
              }}\
              if (typeof window.__cadenceBridge__.run === 'function') {{\
                window.__cadenceBridge__.run({id}, {body});\
              }} else {{\
                throw new Error('bridge not installed');\
              }}\
            }} catch (error) {{\
              try {{\
                window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke('browser_internal_reply', {{\
                  requestId: {id},\
                  ok: false,\
                  value: null,\
                  error: (error && (error.stack || error.message)) || String(error),\
                }});\
              }} catch (_inner) {{ /* swallow */ }}\
            }}\
          }})();",
        id = encoded_id,
        body = encoded_body,
    );

    webview.eval(&script).map_err(|error| {
        waiters.cancel(&request_id);
        CommandError::system_fault(
            "browser_bridge_eval_failed",
            format!("Cadence could not evaluate the browser bridge script: {error}"),
        )
    })?;

    let reply = receiver
        .recv_timeout(Duration::from_millis(timeout))
        .map_err(|_| {
            waiters.cancel(&request_id);
            CommandError::retryable(
                "browser_bridge_timeout",
                format!("Browser script did not reply within {timeout} ms."),
            )
        })?;

    if reply.ok {
        Ok(reply.value.unwrap_or(JsonValue::Null))
    } else {
        Err(CommandError::user_fixable(
            "browser_script_error",
            reply
                .error
                .unwrap_or_else(|| "Browser script rejected without a message.".to_string()),
        ))
    }
}
