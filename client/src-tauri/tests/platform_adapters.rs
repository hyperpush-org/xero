use std::net::TcpListener;

use xero_desktop_lib::runtime::{
    bind_openai_callback_listener, resolve_openai_callback_policy,
    resolve_runtime_shell_selection_for_platform, OpenAiCallbackBindResult, RuntimePlatform,
    RuntimeShellSource,
};

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

#[test]
fn runtime_shell_selection_uses_env_when_present() {
    let selection = resolve_runtime_shell_selection_for_platform(
        RuntimePlatform::MacOs,
        Some("/usr/local/bin/fish"),
        None,
    );

    assert_eq!(selection.program, "/usr/local/bin/fish");
    assert_eq!(selection.args, vec!["-i".to_string()]);
    assert_eq!(selection.source, RuntimeShellSource::Environment);
    assert!(selection.diagnostic.is_none());
}

#[test]
fn runtime_shell_selection_falls_back_with_typed_diagnostics_for_missing_or_invalid_env() {
    let missing_windows =
        resolve_runtime_shell_selection_for_platform(RuntimePlatform::Windows, None, None);
    assert_eq!(missing_windows.program, "cmd.exe");
    assert_eq!(missing_windows.args, vec!["/Q".to_string()]);
    assert_eq!(missing_windows.source, RuntimeShellSource::Default);
    assert_eq!(
        missing_windows
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("runtime_shell_env_missing")
    );
    assert!(missing_windows
        .diagnostic
        .as_ref()
        .is_some_and(|diagnostic| diagnostic.message.contains("COMSPEC")));
    assert!(missing_windows
        .diagnostic
        .as_ref()
        .is_some_and(|diagnostic| diagnostic.message.contains("cmd.exe")));

    let invalid_linux =
        resolve_runtime_shell_selection_for_platform(RuntimePlatform::Linux, Some("   \n"), None);
    assert_eq!(invalid_linux.program, "/bin/sh");
    assert_eq!(invalid_linux.args, vec!["-i".to_string()]);
    assert_eq!(invalid_linux.source, RuntimeShellSource::Default);
    assert_eq!(
        invalid_linux
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("runtime_shell_env_invalid")
    );
    assert!(invalid_linux
        .diagnostic
        .as_ref()
        .is_some_and(|diagnostic| diagnostic.message.contains("SHELL")));
    assert!(invalid_linux
        .diagnostic
        .as_ref()
        .is_some_and(|diagnostic| diagnostic.message.contains("/bin/sh")));
}

#[test]
fn runtime_shell_selection_prefers_comspec_on_windows_even_when_shell_is_set() {
    let selection = resolve_runtime_shell_selection_for_platform(
        RuntimePlatform::Windows,
        Some("/bin/zsh"),
        Some("C:\\Windows\\System32\\cmd.exe"),
    );

    assert_eq!(selection.program, "C:\\Windows\\System32\\cmd.exe");
    assert_eq!(selection.args, vec!["/Q".to_string()]);
    assert_eq!(selection.source, RuntimeShellSource::Environment);
    assert!(selection.diagnostic.is_none());
}

#[test]
fn runtime_shell_test_helper_fails_closed_on_malformed_templates() {
    let empty_script_error = std::panic::catch_unwind(|| runtime_shell::launch_script(""))
        .expect_err("empty script template should panic");
    let empty_script_message = panic_message(empty_script_error);
    assert!(
        empty_script_message.contains("runtime_shell_test_helper_malformed_template"),
        "unexpected panic message: {empty_script_message}"
    );

    let invalid_var_error = std::panic::catch_unwind(|| {
        runtime_shell::script_prompt_read_echo_and_sleep("Enter value: ", "1value", "value=", 1)
    })
    .expect_err("invalid shell variable template should panic");
    let invalid_var_message = panic_message(invalid_var_error);
    assert!(
        invalid_var_message.contains("runtime_shell_test_helper_malformed_template"),
        "unexpected panic message: {invalid_var_message}"
    );
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "<non-string panic payload>".into()
}

#[test]
fn openai_callback_policy_rejects_malformed_host_text() {
    let error = resolve_openai_callback_policy("127.0.0.1:1455", 1455, "/auth/callback")
        .expect_err("host with inline port should fail");
    assert_eq!(error.code, "callback_listener_config_invalid");
}

#[test]
fn openai_callback_bind_falls_back_to_manual_mode_when_port_is_in_use() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).expect("occupied listener");
    let occupied_port = occupied.local_addr().expect("occupied addr").port();

    let policy = resolve_openai_callback_policy("127.0.0.1", occupied_port, "/auth/callback")
        .expect("callback policy");

    let result = bind_openai_callback_listener(&policy).expect("bind result");
    match result {
        OpenAiCallbackBindResult::ManualFallback {
            redirect_uri,
            diagnostic,
        } => {
            assert_eq!(
                redirect_uri,
                format!("http://127.0.0.1:{occupied_port}/auth/callback")
            );
            assert_eq!(diagnostic.code, "callback_listener_bind_failed");
            assert!(diagnostic
                .message
                .contains(occupied_port.to_string().as_str()));
            assert!(diagnostic.message.contains("127.0.0.1"));
        }
        OpenAiCallbackBindResult::Bound { .. } => {
            panic!("expected manual fallback when callback port is occupied")
        }
    }

    drop(occupied);
}

#[test]
fn openai_callback_bind_supports_dynamic_port_binding() {
    let policy = resolve_openai_callback_policy("127.0.0.1", 0, "/auth/callback").expect("policy");

    let result = bind_openai_callback_listener(&policy).expect("bind callback listener");
    match result {
        OpenAiCallbackBindResult::Bound {
            listener,
            redirect_uri,
        } => {
            let bound = listener.local_addr().expect("bound address");
            assert!(bound.port() > 0);
            assert!(redirect_uri.starts_with("http://127.0.0.1:"));
            assert!(redirect_uri.ends_with("/auth/callback"));
        }
        OpenAiCallbackBindResult::ManualFallback { .. } => {
            panic!("expected bound callback listener for dynamic port selection")
        }
    }
}
