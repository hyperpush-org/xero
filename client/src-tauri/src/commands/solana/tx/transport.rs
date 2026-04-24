//! JSON-RPC transport used by the Phase 3 transaction pipeline.
//!
//! Wraps `reqwest::blocking::Client` behind a trait so integration tests can
//! substitute scripted responses without touching the network. The trait
//! takes raw `serde_json::Value` payloads rather than typed structs so the
//! individual pipeline modules (priority fee oracle, simulate, send) can
//! each encode their own RPC shape without needing transport-level
//! plumbing.

use std::fmt::Debug;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::commands::{CommandError, CommandResult};

const DEFAULT_USER_AGENT: &str = "cadence-solana-workbench/0.1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Abstract JSON-RPC transport. Every Solana RPC the pipeline needs is a
/// one-shot POST with a JSON body, so the trait surface stays tiny.
pub trait RpcTransport: Send + Sync + Debug {
    fn post(&self, url: &str, body: Value) -> CommandResult<Value>;
}

/// Production transport — `reqwest::blocking` with a permissive timeout.
#[derive(Debug)]
pub struct HttpRpcTransport {
    client: Mutex<Option<Client>>,
    timeout: Duration,
}

impl HttpRpcTransport {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn client(&self) -> CommandResult<Client> {
        let mut guard = self.client.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_tx_transport_poisoned",
                "Transport client lock poisoned.",
            )
        })?;
        if guard.is_none() {
            let client = Client::builder()
                .timeout(self.timeout)
                .user_agent(DEFAULT_USER_AGENT)
                .build()
                .map_err(|err| {
                    CommandError::system_fault(
                        "solana_tx_transport_build_failed",
                        format!("Could not build HTTP client: {err}"),
                    )
                })?;
            *guard = Some(client);
        }
        Ok(guard.as_ref().cloned().unwrap())
    }
}

impl Default for HttpRpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcTransport for HttpRpcTransport {
    fn post(&self, url: &str, body: Value) -> CommandResult<Value> {
        let client = self.client()?;
        let response = client.post(url).json(&body).send().map_err(|err| {
            CommandError::retryable(
                "solana_tx_transport_send",
                format!("RPC transport error calling {url}: {err}"),
            )
        })?;

        let status = response.status();
        let body: Value = response.json().map_err(|err| {
            CommandError::retryable(
                "solana_tx_transport_decode",
                format!("Could not decode RPC response from {url} ({status}): {err}"),
            )
        })?;

        if let Some(err) = body.get("error") {
            return Err(CommandError::retryable(
                "solana_tx_rpc_error",
                format!("RPC error from {url}: {err}"),
            ));
        }
        Ok(body)
    }
}

/// Helper: wrap a single-method JSON-RPC call in the standard envelope.
pub fn rpc_request(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::collections::HashMap;

    /// Scripted transport — returns the pre-registered response for a given
    /// `(url, method)` pair. Panics at test time if a pipeline call reaches
    /// an un-scripted endpoint, which catches forgotten mocks early.
    #[derive(Debug, Default)]
    pub struct ScriptedTransport {
        pub responses: Mutex<HashMap<(String, String), Value>>,
        pub requests: Mutex<Vec<(String, String, Value)>>,
    }

    impl ScriptedTransport {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set(&self, url: &str, method: &str, response: Value) {
            self.responses
                .lock()
                .unwrap()
                .insert((url.to_string(), method.to_string()), response);
        }

        pub fn calls_for(&self, method: &str) -> Vec<(String, Value)> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, m, _)| m == method)
                .map(|(u, _, p)| (u.clone(), p.clone()))
                .collect()
        }
    }

    impl RpcTransport for ScriptedTransport {
        fn post(&self, url: &str, body: Value) -> CommandResult<Value> {
            let method = body
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let params = body.get("params").cloned().unwrap_or(Value::Null);
            self.requests
                .lock()
                .unwrap()
                .push((url.to_string(), method.clone(), params));

            let responses = self.responses.lock().unwrap();
            match responses.get(&(url.to_string(), method.clone())) {
                Some(value) => Ok(value.clone()),
                None => Err(CommandError::system_fault(
                    "scripted_transport_unprepared",
                    format!("No scripted response for {method} @ {url}"),
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedTransport;
    use super::*;

    #[test]
    fn scripted_transport_returns_canned_response() {
        let transport = ScriptedTransport::new();
        transport.set(
            "http://rpc.test",
            "getHealth",
            json!({"jsonrpc": "2.0", "id": 1, "result": "ok"}),
        );
        let resp = transport
            .post("http://rpc.test", rpc_request("getHealth", json!([])))
            .unwrap();
        assert_eq!(resp["result"], "ok");
    }

    #[test]
    fn scripted_transport_records_method_calls() {
        let transport = ScriptedTransport::new();
        transport.set("http://rpc.test", "getSlot", json!({"result": 123}));
        let _ = transport.post("http://rpc.test", rpc_request("getSlot", json!([])));
        let calls = transport.calls_for("getSlot");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "http://rpc.test");
    }

    #[test]
    fn rpc_request_shape_is_jsonrpc_2_0() {
        let body = rpc_request("foo", json!([1, 2]));
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["method"], "foo");
        assert_eq!(body["params"], json!([1, 2]));
    }
}
