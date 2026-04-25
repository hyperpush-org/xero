//! Tests for the browser arm of the autonomous tool runtime. These exercise the
//! executor contract without spinning up a real Tauri runtime.

use std::sync::{Arc, Mutex};

use cadence_desktop_lib::commands::CommandError;
use cadence_desktop_lib::runtime::autonomous_tool_runtime::{
    AutonomousBrowserAction, AutonomousBrowserOutput, AutonomousBrowserRequest,
    AutonomousToolOutput, AutonomousToolRequest, AutonomousToolRuntime, BrowserExecutor,
    UnavailableBrowserExecutor,
};
use tempfile::TempDir;

#[derive(Debug)]
struct RecordingExecutor {
    calls: Mutex<Vec<AutonomousBrowserAction>>,
}

impl RecordingExecutor {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl BrowserExecutor for RecordingExecutor {
    fn execute(
        &self,
        action: AutonomousBrowserAction,
    ) -> Result<AutonomousBrowserOutput, CommandError> {
        let name = match &action {
            AutonomousBrowserAction::Open { .. } => "open",
            AutonomousBrowserAction::TabOpen { .. } => "tab_open",
            AutonomousBrowserAction::Navigate { .. } => "navigate",
            AutonomousBrowserAction::Click { .. } => "click",
            AutonomousBrowserAction::ReadText { .. } => "read_text",
            AutonomousBrowserAction::Screenshot => "screenshot",
            _ => "other",
        }
        .to_string();
        let url = match &action {
            AutonomousBrowserAction::Open { url }
            | AutonomousBrowserAction::TabOpen { url }
            | AutonomousBrowserAction::Navigate { url } => Some(url.clone()),
            _ => None,
        };
        self.calls.lock().unwrap().push(action);
        Ok(AutonomousBrowserOutput {
            action: name,
            url,
            value_json: "null".to_string(),
        })
    }
}

fn runtime_with_executor(executor: Arc<dyn BrowserExecutor>) -> (AutonomousToolRuntime, TempDir) {
    let temp = TempDir::new().expect("tempdir");
    let runtime = AutonomousToolRuntime::new(temp.path())
        .expect("runtime")
        .with_browser_executor(executor);
    (runtime, temp)
}

#[test]
fn missing_executor_returns_policy_denied() {
    let temp = TempDir::new().unwrap();
    let runtime = AutonomousToolRuntime::new(temp.path()).unwrap();
    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Navigate {
            url: "https://example.com/".to_string(),
        },
    });
    let error = runtime.execute(request).expect_err("should deny");
    assert_eq!(error.code, "policy_denied");
}

#[test]
fn unavailable_executor_denies_any_action() {
    let executor: Arc<dyn BrowserExecutor> = Arc::new(UnavailableBrowserExecutor);
    let (runtime, _temp) = runtime_with_executor(executor);
    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::CurrentUrl,
    });
    let error = runtime.execute(request).expect_err("should deny");
    assert_eq!(error.code, "policy_denied");
}

#[test]
fn recording_executor_dispatches_open_and_propagates_result() {
    let recorder = Arc::new(RecordingExecutor::new());
    let executor: Arc<dyn BrowserExecutor> = recorder.clone();
    let (runtime, _temp) = runtime_with_executor(executor);

    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Open {
            url: "https://example.com/".to_string(),
        },
    });
    let result = runtime.execute(request).expect("success");
    assert_eq!(result.tool_name, "browser");
    match result.output {
        AutonomousToolOutput::Browser(output) => {
            assert_eq!(output.action, "open");
            assert_eq!(output.url.as_deref(), Some("https://example.com/"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let calls = recorder.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        AutonomousBrowserAction::Open { url } => assert_eq!(url, "https://example.com/"),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn recording_executor_dispatches_navigate_and_propagates_result() {
    let recorder = Arc::new(RecordingExecutor::new());
    let executor: Arc<dyn BrowserExecutor> = recorder.clone();
    let (runtime, _temp) = runtime_with_executor(executor);

    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Navigate {
            url: "https://example.com/".to_string(),
        },
    });
    let result = runtime.execute(request).expect("success");
    assert_eq!(result.tool_name, "browser");
    match result.output {
        AutonomousToolOutput::Browser(output) => {
            assert_eq!(output.action, "navigate");
            assert_eq!(output.url.as_deref(), Some("https://example.com/"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let calls = recorder.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        AutonomousBrowserAction::Navigate { url } => assert_eq!(url, "https://example.com/"),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn recording_executor_dispatches_tab_open() {
    let recorder = Arc::new(RecordingExecutor::new());
    let executor: Arc<dyn BrowserExecutor> = recorder.clone();
    let (runtime, _temp) = runtime_with_executor(executor);

    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::TabOpen {
            url: "https://example.com/new".to_string(),
        },
    });
    let result = runtime.execute(request).expect("success");
    match result.output {
        AutonomousToolOutput::Browser(output) => {
            assert_eq!(output.action, "tab_open");
            assert_eq!(output.url.as_deref(), Some("https://example.com/new"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn recording_executor_handles_click() {
    let recorder = Arc::new(RecordingExecutor::new());
    let executor: Arc<dyn BrowserExecutor> = recorder.clone();
    let (runtime, _temp) = runtime_with_executor(executor);

    let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Click {
            selector: "button.submit".to_string(),
            timeout_ms: Some(5_000),
        },
    });
    let result = runtime.execute(request).expect("success");
    match result.output {
        AutonomousToolOutput::Browser(output) => assert_eq!(output.action, "click"),
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn browser_action_serializes_with_action_tag() {
    // Serialization contract — what the autonomous orchestrator encodes into
    // supervisor events should round-trip cleanly.
    let open = AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Open {
            url: "https://example.com/".to_string(),
        },
    };
    let open_json = serde_json::to_value(&open).unwrap();
    assert_eq!(open_json["action"], "open");
    assert_eq!(open_json["url"], "https://example.com/");
    let open_roundtrip: AutonomousBrowserRequest = serde_json::from_value(open_json).unwrap();
    assert_eq!(open_roundtrip, open);

    let tab_open = AutonomousBrowserRequest {
        action: AutonomousBrowserAction::TabOpen {
            url: "https://example.com/new".to_string(),
        },
    };
    let tab_open_json = serde_json::to_value(&tab_open).unwrap();
    assert_eq!(tab_open_json["action"], "tab_open");
    assert_eq!(tab_open_json["url"], "https://example.com/new");
    let tab_open_roundtrip: AutonomousBrowserRequest =
        serde_json::from_value(tab_open_json).unwrap();
    assert_eq!(tab_open_roundtrip, tab_open);

    let click = AutonomousBrowserRequest {
        action: AutonomousBrowserAction::Click {
            selector: "#login".to_string(),
            timeout_ms: None,
        },
    };
    let click_json = serde_json::to_value(&click).unwrap();
    assert_eq!(click_json["action"], "click");
    assert_eq!(click_json["selector"], "#login");

    let click_roundtrip: AutonomousBrowserRequest = serde_json::from_value(click_json).unwrap();
    assert_eq!(click_roundtrip, click);
}

#[test]
fn browser_action_rejects_unknown_action_tag() {
    let payload = serde_json::json!({
        "action": "open_in_new_window",
        "url": "https://example.com/"
    });
    let error = serde_json::from_value::<AutonomousBrowserRequest>(payload)
        .expect_err("unknown action tags should fail closed");
    assert!(error.to_string().contains("unknown variant"));
}
