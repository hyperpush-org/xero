//! Tests for the emulator/app automation arm of the autonomous tool runtime.
//! These exercise the executor contract without starting a real device.

use std::sync::{Arc, Mutex};

use serde_json::json;
use tempfile::TempDir;
use xero_desktop_lib::commands::CommandError;
use xero_desktop_lib::runtime::autonomous_tool_runtime::{
    AutonomousEmulatorAction, AutonomousEmulatorOutput, AutonomousEmulatorRequest,
    AutonomousToolOutput, AutonomousToolRequest, AutonomousToolRuntime, EmulatorExecutor,
    UnavailableEmulatorExecutor,
};

#[derive(Debug)]
struct RecordingEmulatorExecutor {
    calls: Mutex<Vec<(AutonomousEmulatorAction, Option<serde_json::Value>)>>,
}

impl RecordingEmulatorExecutor {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl EmulatorExecutor for RecordingEmulatorExecutor {
    fn execute(
        &self,
        action: AutonomousEmulatorAction,
        input: Option<serde_json::Value>,
    ) -> Result<AutonomousEmulatorOutput, CommandError> {
        self.calls.lock().unwrap().push((action.clone(), input));
        Ok(AutonomousEmulatorOutput {
            action: match action {
                AutonomousEmulatorAction::LaunchApp => "launch_app",
                AutonomousEmulatorAction::Tap => "tap",
                AutonomousEmulatorAction::Screenshot => "screenshot",
                _ => "other",
            }
            .to_string(),
            value_json: "null".to_string(),
        })
    }
}

fn runtime_with_executor(executor: Arc<dyn EmulatorExecutor>) -> (AutonomousToolRuntime, TempDir) {
    let temp = TempDir::new().expect("tempdir");
    let runtime = AutonomousToolRuntime::new(temp.path())
        .expect("runtime")
        .with_emulator_executor(executor);
    (runtime, temp)
}

#[test]
fn missing_emulator_executor_returns_policy_denied() {
    let temp = TempDir::new().unwrap();
    let runtime = AutonomousToolRuntime::new(temp.path()).unwrap();
    let request = AutonomousToolRequest::Emulator(AutonomousEmulatorRequest {
        action: AutonomousEmulatorAction::Screenshot,
        input: None,
    });
    let error = runtime.execute(request).expect_err("should deny");
    assert_eq!(error.code, "policy_denied");
}

#[test]
fn unavailable_emulator_executor_denies_any_action() {
    let executor: Arc<dyn EmulatorExecutor> = Arc::new(UnavailableEmulatorExecutor);
    let (runtime, _temp) = runtime_with_executor(executor);
    let request = AutonomousToolRequest::Emulator(AutonomousEmulatorRequest {
        action: AutonomousEmulatorAction::Screenshot,
        input: None,
    });
    let error = runtime.execute(request).expect_err("should deny");
    assert_eq!(error.code, "policy_denied");
}

#[test]
fn emulator_runtime_dispatches_app_lifecycle_action() {
    let recorder = Arc::new(RecordingEmulatorExecutor::new());
    let executor: Arc<dyn EmulatorExecutor> = recorder.clone();
    let (runtime, _temp) = runtime_with_executor(executor);
    let input = json!({ "bundleId": "com.example.app", "args": ["--demo"] });

    let result = runtime
        .execute(AutonomousToolRequest::Emulator(AutonomousEmulatorRequest {
            action: AutonomousEmulatorAction::LaunchApp,
            input: Some(input.clone()),
        }))
        .expect("emulator action should dispatch");
    assert_eq!(result.tool_name, "emulator");
    match result.output {
        AutonomousToolOutput::Emulator(output) => assert_eq!(output.action, "launch_app"),
        other => panic!("unexpected output: {other:?}"),
    }

    let calls = recorder.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, AutonomousEmulatorAction::LaunchApp);
    assert_eq!(calls[0].1.as_ref(), Some(&input));
}

#[test]
fn emulator_tool_request_serializes_with_action_and_input() {
    let request = AutonomousEmulatorRequest {
        action: AutonomousEmulatorAction::Tap,
        input: Some(json!({ "kind": "point", "x": 0.5, "y": 0.25 })),
    };
    let value = serde_json::to_value(&request).expect("serialize emulator request");
    assert_eq!(value["action"], "tap");
    assert_eq!(value["input"]["kind"], "point");

    let roundtrip: AutonomousEmulatorRequest =
        serde_json::from_value(value).expect("deserialize emulator request");
    assert_eq!(roundtrip, request);

    let tool_request: AutonomousToolRequest = serde_json::from_value(json!({
        "tool": "emulator",
        "input": {
            "action": "launch_app",
            "input": { "bundleId": "com.example.app" }
        }
    }))
    .expect("deserialize top-level emulator tool request");
    assert!(matches!(tool_request, AutonomousToolRequest::Emulator(_)));
}
