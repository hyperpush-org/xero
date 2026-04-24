use std::{
    collections::BTreeSet,
    io::ErrorKind,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use reqwest::{blocking::Client, StatusCode};

use crate::auth::now_timestamp;

use super::registry::{
    McpConnectionDiagnostic, McpConnectionState, McpConnectionStatus, McpRegistry, McpServerRecord,
    McpTransport,
};

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_millis(1_200);
const STDIO_READINESS_WINDOW: Duration = Duration::from_millis(250);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(25);
const DEFAULT_MAX_PARALLEL_PROBES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpProbeConfig {
    pub timeout: Duration,
    pub max_parallel: usize,
}

impl Default for McpProbeConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_PROBE_TIMEOUT,
            max_parallel: DEFAULT_MAX_PARALLEL_PROBES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportProbeMode {
    Http,
    Sse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeOutcome {
    Connected,
    Status {
        status: McpConnectionStatus,
        diagnostic: McpConnectionDiagnostic,
    },
}

impl ProbeOutcome {
    fn failed(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Status {
            status: McpConnectionStatus::Failed,
            diagnostic: McpConnectionDiagnostic {
                code: code.into(),
                message: message.into(),
                retryable,
            },
        }
    }

    fn blocked(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Status {
            status: McpConnectionStatus::Blocked,
            diagnostic: McpConnectionDiagnostic {
                code: code.into(),
                message: message.into(),
                retryable,
            },
        }
    }

    fn misconfigured(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Status {
            status: McpConnectionStatus::Misconfigured,
            diagnostic: McpConnectionDiagnostic {
                code: code.into(),
                message: message.into(),
                retryable,
            },
        }
    }

    fn stale(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Status {
            status: McpConnectionStatus::Stale,
            diagnostic: McpConnectionDiagnostic {
                code: code.into(),
                message: message.into(),
                retryable,
            },
        }
    }
}

pub fn refresh_mcp_connection_truth(
    registry: &McpRegistry,
    selected_server_ids: Option<&BTreeSet<String>>,
) -> McpRegistry {
    refresh_mcp_connection_truth_with_config(
        registry,
        selected_server_ids,
        McpProbeConfig::default(),
    )
}

pub fn refresh_mcp_connection_truth_with_config(
    registry: &McpRegistry,
    selected_server_ids: Option<&BTreeSet<String>>,
    config: McpProbeConfig,
) -> McpRegistry {
    let target_indices = registry
        .servers
        .iter()
        .enumerate()
        .filter_map(|(index, server)| {
            let should_probe = selected_server_ids
                .map(|selection| selection.contains(&server.id))
                .unwrap_or(true);
            should_probe.then_some(index)
        })
        .collect::<Vec<_>>();

    if target_indices.is_empty() {
        return registry.clone();
    }

    let now = now_timestamp();
    let max_parallel = config.max_parallel.max(1);
    let timeout = config.timeout;

    let mut outcomes = Vec::with_capacity(target_indices.len());

    for chunk in target_indices.chunks(max_parallel) {
        let mut handles = Vec::with_capacity(chunk.len());
        for index in chunk {
            let server = registry.servers[*index].clone();
            handles.push((
                *index,
                thread::spawn(move || probe_server(&server, timeout)),
            ));
        }

        for (index, handle) in handles {
            let outcome = match handle.join() {
                Ok(outcome) => outcome,
                Err(_) => ProbeOutcome::failed(
                    "mcp_probe_runtime_panic",
                    format!(
                        "Cadence failed to evaluate MCP server `{}` because the probe task panicked.",
                        registry.servers[index].id
                    ),
                    true,
                ),
            };
            outcomes.push((index, outcome));
        }
    }

    outcomes.sort_by_key(|(index, _)| *index);

    let mut next = registry.clone();
    let mut changed = false;

    for (index, outcome) in outcomes {
        let previous_connection = &registry.servers[index].connection;
        let projected = project_connection_state(previous_connection, outcome, &now);

        if projected != *previous_connection {
            changed = true;
            next.servers[index].connection = projected;
            next.servers[index].updated_at = now.clone();
        }
    }

    if changed {
        next.updated_at = now;
    }

    next
}

pub fn stale_after_configuration_change(previous: &McpConnectionState) -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Stale,
        diagnostic: Some(McpConnectionDiagnostic {
            code: "mcp_status_recheck_required".into(),
            message: "Cadence marked this MCP server as stale because its configuration changed and must be rechecked.".into(),
            retryable: true,
        }),
        last_checked_at: None,
        last_healthy_at: preserve_last_healthy(previous),
    }
}

fn probe_server(server: &McpServerRecord, timeout: Duration) -> ProbeOutcome {
    let missing_env = server
        .env
        .iter()
        .filter(|entry| std::env::var_os(&entry.from_env).is_none())
        .map(|entry| entry.from_env.clone())
        .collect::<Vec<_>>();

    if !missing_env.is_empty() {
        return ProbeOutcome::blocked(
            "mcp_probe_env_missing",
            format!(
                "Cadence blocked MCP server `{}` because required environment references are missing: {}.",
                server.id,
                missing_env.join(", "),
            ),
            false,
        );
    }

    match &server.transport {
        McpTransport::Stdio { command, args } => {
            probe_stdio_transport(&server.id, command, args, timeout)
        }
        McpTransport::Http { url } => {
            probe_http_transport(&server.id, url, TransportProbeMode::Http, timeout)
        }
        McpTransport::Sse { url } => {
            probe_http_transport(&server.id, url, TransportProbeMode::Sse, timeout)
        }
    }
}

fn probe_stdio_transport(
    server_id: &str,
    command: &str,
    args: &[String],
    timeout: Duration,
) -> ProbeOutcome {
    let mut child = match Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return ProbeOutcome::failed(
                "mcp_probe_spawn_failed",
                format!(
                    "Cadence could not start MCP stdio server `{server_id}` for probing: {error}"
                ),
                !matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::PermissionDenied
                ),
            );
        }
    };

    let started_at = Instant::now();
    let readiness_window = timeout.min(STDIO_READINESS_WINDOW);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    ProbeOutcome::misconfigured(
                        "mcp_probe_stdio_exited_early",
                        format!(
                            "Cadence marked MCP stdio server `{server_id}` as misconfigured because it exited before becoming ready."
                        ),
                        false,
                    )
                } else {
                    ProbeOutcome::failed(
                        "mcp_probe_stdio_failed",
                        format!(
                            "Cadence marked MCP stdio server `{server_id}` as failed because the probe process exited non-zero."
                        ),
                        false,
                    )
                };
            }
            Ok(None) => {
                let elapsed = started_at.elapsed();
                if elapsed >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return ProbeOutcome::stale(
                        "mcp_probe_timeout",
                        format!(
                            "Cadence marked MCP server `{server_id}` as stale because the probe timed out."
                        ),
                        true,
                    );
                }

                if elapsed >= readiness_window {
                    let _ = child.kill();
                    let _ = child.wait();
                    return ProbeOutcome::Connected;
                }

                thread::sleep(PROBE_POLL_INTERVAL);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return ProbeOutcome::failed(
                    "mcp_probe_stdio_wait_failed",
                    format!(
                        "Cadence could not monitor MCP stdio server `{server_id}` during probing: {error}"
                    ),
                    true,
                );
            }
        }
    }
}

fn probe_http_transport(
    server_id: &str,
    url: &str,
    mode: TransportProbeMode,
    timeout: Duration,
) -> ProbeOutcome {
    let client = match Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(error) => {
            return ProbeOutcome::failed(
                "mcp_probe_client_build_failed",
                format!(
                    "Cadence could not initialize the HTTP probe client for MCP server `{server_id}`: {error}"
                ),
                true,
            );
        }
    };

    let response = match client.get(url).send() {
        Ok(response) => response,
        Err(error) => {
            if error.is_timeout() {
                return ProbeOutcome::stale(
                    "mcp_probe_timeout",
                    format!(
                        "Cadence marked MCP server `{server_id}` as stale because the probe timed out."
                    ),
                    true,
                );
            }

            return ProbeOutcome::failed(
                "mcp_probe_transport_failed",
                format!("Cadence could not reach MCP server `{server_id}` during probing: {error}"),
                true,
            );
        }
    };

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return ProbeOutcome::blocked(
            "mcp_probe_access_denied",
            format!(
                "Cadence blocked MCP server `{server_id}` because the probe received HTTP {}.",
                status.as_u16(),
            ),
            false,
        );
    }

    if status == StatusCode::NOT_FOUND {
        return ProbeOutcome::misconfigured(
            "mcp_probe_endpoint_not_found",
            format!(
                "Cadence marked MCP server `{server_id}` as misconfigured because the probe endpoint returned HTTP 404."
            ),
            false,
        );
    }

    if status.is_server_error() {
        return ProbeOutcome::failed(
            "mcp_probe_server_error",
            format!(
                "Cadence marked MCP server `{server_id}` as failed because the probe received HTTP {}.",
                status.as_u16(),
            ),
            true,
        );
    }

    if mode == TransportProbeMode::Http && status == StatusCode::METHOD_NOT_ALLOWED {
        return ProbeOutcome::Connected;
    }

    if !status.is_success() {
        return if status.is_client_error() {
            ProbeOutcome::misconfigured(
                "mcp_probe_http_client_error",
                format!(
                    "Cadence marked MCP server `{server_id}` as misconfigured because the probe received HTTP {}.",
                    status.as_u16(),
                ),
                false,
            )
        } else {
            ProbeOutcome::failed(
                "mcp_probe_http_unexpected_status",
                format!(
                    "Cadence marked MCP server `{server_id}` as failed because the probe received HTTP {}.",
                    status.as_u16(),
                ),
                true,
            )
        };
    }

    match mode {
        TransportProbeMode::Http => {
            if content_type
                .as_deref()
                .is_some_and(|value| !is_supported_http_content_type(value))
            {
                return ProbeOutcome::misconfigured(
                    "mcp_probe_http_malformed_response",
                    format!(
                        "Cadence marked MCP server `{server_id}` as misconfigured because the probe response content-type was not MCP-compatible."
                    ),
                    false,
                );
            }
            ProbeOutcome::Connected
        }
        TransportProbeMode::Sse => {
            if !content_type
                .as_deref()
                .is_some_and(is_supported_sse_content_type)
            {
                return ProbeOutcome::misconfigured(
                    "mcp_probe_sse_malformed_response",
                    format!(
                        "Cadence marked MCP server `{server_id}` as misconfigured because the probe response was not an event stream."
                    ),
                    false,
                );
            }
            ProbeOutcome::Connected
        }
    }
}

fn is_supported_http_content_type(content_type: &str) -> bool {
    let content_type = content_type.to_ascii_lowercase();
    content_type.contains("json")
        || content_type.contains("ndjson")
        || content_type.contains("application/x-ndjson")
}

fn is_supported_sse_content_type(content_type: &str) -> bool {
    content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
}

fn project_connection_state(
    previous: &McpConnectionState,
    outcome: ProbeOutcome,
    checked_at: &str,
) -> McpConnectionState {
    match outcome {
        ProbeOutcome::Connected => McpConnectionState {
            status: McpConnectionStatus::Connected,
            diagnostic: None,
            last_checked_at: Some(checked_at.to_owned()),
            last_healthy_at: Some(checked_at.to_owned()),
        },
        ProbeOutcome::Status { status, diagnostic } => McpConnectionState {
            status,
            diagnostic: Some(diagnostic),
            last_checked_at: Some(checked_at.to_owned()),
            last_healthy_at: preserve_last_healthy(previous),
        },
    }
}

fn preserve_last_healthy(previous: &McpConnectionState) -> Option<String> {
    previous.last_healthy_at.clone().or_else(|| {
        (previous.status == McpConnectionStatus::Connected)
            .then(|| previous.last_checked_at.clone())
            .flatten()
    })
}
