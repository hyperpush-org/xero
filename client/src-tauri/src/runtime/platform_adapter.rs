use std::net::{IpAddr, TcpListener};

use crate::{
    auth::{AuthDiagnostic, AuthFlowError},
    commands::RuntimeAuthPhase,
};

const WINDOWS_DEFAULT_SHELL: &str = "cmd.exe";
const UNIX_DEFAULT_SHELL: &str = "/bin/sh";
const WINDOWS_DEFAULT_SHELL_ARGS: [&str; 1] = ["/Q"];
const UNIX_DEFAULT_SHELL_ARGS: [&str; 1] = ["-i"];

pub const OPENAI_DEFAULT_CALLBACK_HOST: &str = "localhost";
pub const OPENAI_DEFAULT_CALLBACK_PORT: u16 = 1455;
pub const OPENAI_DEFAULT_CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlatform {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl RuntimePlatform {
    pub fn detect() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            _ => Self::Other,
        }
    }

    pub const fn is_windows(self) -> bool {
        matches!(self, Self::Windows)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeShellSource {
    Environment,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAdapterDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeShellSelection {
    pub platform: RuntimePlatform,
    pub program: String,
    pub args: Vec<String>,
    pub source: RuntimeShellSource,
    pub diagnostic: Option<RuntimeAdapterDiagnostic>,
}

pub fn resolve_runtime_shell_selection() -> RuntimeShellSelection {
    let shell = std::env::var("SHELL").ok();
    let comspec = std::env::var("COMSPEC").ok();

    resolve_runtime_shell_selection_for_platform(
        RuntimePlatform::detect(),
        shell.as_deref(),
        comspec.as_deref(),
    )
}

pub fn resolve_runtime_shell_selection_for_platform(
    platform: RuntimePlatform,
    shell_env: Option<&str>,
    comspec_env: Option<&str>,
) -> RuntimeShellSelection {
    let env_name = if platform.is_windows() {
        "COMSPEC"
    } else {
        "SHELL"
    };
    let env_value = if platform.is_windows() {
        comspec_env
    } else {
        shell_env
    };

    let default_program = if platform.is_windows() {
        WINDOWS_DEFAULT_SHELL
    } else {
        UNIX_DEFAULT_SHELL
    };
    let default_args = if platform.is_windows() {
        WINDOWS_DEFAULT_SHELL_ARGS
    } else {
        UNIX_DEFAULT_SHELL_ARGS
    }
    .iter()
    .map(|value| (*value).to_owned())
    .collect::<Vec<_>>();

    match env_value {
        Some(value) if is_valid_shell_program(value) => RuntimeShellSelection {
            platform,
            program: value.trim().to_owned(),
            args: default_args,
            source: RuntimeShellSource::Environment,
            diagnostic: None,
        },
        Some(value) => RuntimeShellSelection {
            platform,
            program: default_program.into(),
            args: default_args,
            source: RuntimeShellSource::Default,
            diagnostic: Some(RuntimeAdapterDiagnostic {
                code: "runtime_shell_env_invalid".into(),
                message: format!(
                    "Xero ignored the `{env_name}` shell override (`{}`) because it was blank or malformed and fell back to `{default_program}`.",
                    value.replace('\n', "\\n")
                ),
            }),
        },
        None => RuntimeShellSelection {
            platform,
            program: default_program.into(),
            args: default_args,
            source: RuntimeShellSource::Default,
            diagnostic: Some(RuntimeAdapterDiagnostic {
                code: "runtime_shell_env_missing".into(),
                message: format!(
                    "Xero did not find `{env_name}` and fell back to `{default_program}`."
                ),
            }),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCallbackPolicy {
    pub host: String,
    pub preferred_port: u16,
    pub path: String,
}

impl OpenAiCallbackPolicy {
    pub fn redirect_uri_for_port(&self, port: u16) -> String {
        format!("http://{}:{port}{}", host_for_uri(&self.host), self.path)
    }

    pub fn fallback_redirect_uri(&self) -> String {
        self.redirect_uri_for_port(self.preferred_port)
    }
}

#[derive(Debug)]
pub enum OpenAiCallbackBindResult {
    Bound {
        listener: TcpListener,
        redirect_uri: String,
    },
    ManualFallback {
        redirect_uri: String,
        diagnostic: AuthDiagnostic,
    },
}

pub fn default_openai_callback_policy() -> OpenAiCallbackPolicy {
    OpenAiCallbackPolicy {
        host: OPENAI_DEFAULT_CALLBACK_HOST.into(),
        preferred_port: OPENAI_DEFAULT_CALLBACK_PORT,
        path: OPENAI_DEFAULT_CALLBACK_PATH.into(),
    }
}

pub fn resolve_openai_callback_policy(
    callback_host: &str,
    callback_port: u16,
    callback_path: &str,
) -> Result<OpenAiCallbackPolicy, AuthFlowError> {
    let host = callback_host.trim();
    if host.is_empty() {
        return Err(invalid_callback_config(
            "Xero requires a non-empty OpenAI callback host.",
        ));
    }
    if !is_supported_callback_host(host) {
        return Err(invalid_callback_config(format!(
            "Xero rejected OpenAI callback host `{host}`. Use `localhost` or a literal IP address without an inline port."
        )));
    }

    let path = callback_path.trim();
    if path.is_empty() {
        return Err(invalid_callback_config(
            "Xero requires a non-empty OpenAI callback path.",
        ));
    }
    if !path.starts_with('/') {
        return Err(invalid_callback_config(format!(
            "Xero rejected OpenAI callback path `{path}` because callback paths must start with '/'."
        )));
    }
    if path.chars().any(char::is_whitespace) {
        return Err(invalid_callback_config(format!(
            "Xero rejected OpenAI callback path `{path}` because callback paths must not contain whitespace."
        )));
    }

    Ok(OpenAiCallbackPolicy {
        host: host.into(),
        preferred_port: callback_port,
        path: path.into(),
    })
}

pub fn bind_openai_callback_listener(
    policy: &OpenAiCallbackPolicy,
) -> Result<OpenAiCallbackBindResult, AuthFlowError> {
    match TcpListener::bind((policy.host.as_str(), policy.preferred_port)) {
        Ok(listener) => {
            let bound_port = listener
                .local_addr()
                .map_err(|error| {
                    AuthFlowError::terminal(
                        "callback_listener_address_unavailable",
                        RuntimeAuthPhase::Starting,
                        format!(
                            "Xero could not resolve the OpenAI callback listener address: {error}"
                        ),
                    )
                })?
                .port();

            Ok(OpenAiCallbackBindResult::Bound {
                listener,
                redirect_uri: policy.redirect_uri_for_port(bound_port),
            })
        }
        Err(error) => Ok(OpenAiCallbackBindResult::ManualFallback {
            redirect_uri: policy.fallback_redirect_uri(),
            diagnostic: AuthDiagnostic {
                code: "callback_listener_bind_failed".into(),
                message: format!(
                    "Xero could not bind the OpenAI callback listener on {}:{}: {error}",
                    policy.host, policy.preferred_port
                ),
                retryable: false,
            },
        }),
    }
}

fn is_valid_shell_program(value: &str) -> bool {
    let candidate = value.trim();
    !candidate.is_empty() && !candidate.contains('\0')
}

fn invalid_callback_config(message: impl Into<String>) -> AuthFlowError {
    AuthFlowError::terminal(
        "callback_listener_config_invalid",
        RuntimeAuthPhase::Starting,
        message,
    )
}

fn is_supported_callback_host(value: &str) -> bool {
    if value.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let candidate = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value);
    candidate.parse::<IpAddr>().is_ok()
}

fn host_for_uri(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.into()
    }
}
