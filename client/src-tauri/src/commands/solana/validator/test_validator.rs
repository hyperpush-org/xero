//! Production `ValidatorLauncher` impl — spawns the user's installed
//! `solana-test-validator` or `surfpool` binary. Kept small on purpose;
//! argv-building lives in `process_launcher.rs` so it can be unit-tested
//! without touching the filesystem or forking a child.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde_json::json;

use crate::commands::emulator::process::ChildGuard;
use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

use super::process_launcher::{resolve_ledger_dir, surfpool_args, test_validator_args};
use super::{ClusterHandle, StartOpts, ValidatorLauncher, ValidatorSession, DEFAULT_RPC_PORT};

const READINESS_PROBE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub struct CliLauncher;

impl ValidatorLauncher for CliLauncher {
    fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
        let (tag, binary_name) = match kind {
            ClusterKind::Localnet => ("localnet", "solana-test-validator"),
            ClusterKind::MainnetFork => ("fork", "surfpool"),
            _ => {
                return Err(CommandError::user_fixable(
                    "solana_cluster_not_startable",
                    format!("Cluster {} cannot be launched locally.", kind.as_str()),
                ));
            }
        };

        let binary = resolve_binary(binary_name)?;
        let ledger_dir = resolve_ledger_dir(opts, tag);
        ensure_dir(&ledger_dir)?;

        let args = match kind {
            ClusterKind::Localnet => test_validator_args(opts, &ledger_dir),
            ClusterKind::MainnetFork => surfpool_args(opts, &ledger_dir),
            _ => unreachable!("covered above"),
        };

        let mut cmd = Command::new(&binary);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|err| {
            CommandError::system_fault(
                "solana_validator_spawn_failed",
                format!(
                    "Could not spawn {} at {}: {err}",
                    binary_name,
                    binary.display()
                ),
            )
        })?;
        let label: &'static str = match kind {
            ClusterKind::Localnet => "solana-test-validator",
            ClusterKind::MainnetFork => "surfpool",
            _ => unreachable!("covered above"),
        };
        let mut guard = ChildGuard::new(label, child);

        let rpc_port = opts.rpc_port.unwrap_or(DEFAULT_RPC_PORT);
        let ws_port = opts.ws_port.unwrap_or(rpc_port + 1);
        let rpc_url = format!("http://127.0.0.1:{rpc_port}");
        let ws_url = format!("ws://127.0.0.1:{ws_port}");

        let boot_timeout = super::clamp_boot_timeout(opts.boot_timeout_secs);
        await_rpc_ready(&mut guard, &rpc_url, boot_timeout)?;

        let handle = ClusterHandle {
            kind,
            rpc_url,
            ws_url,
            pid: guard.pid(),
            ledger_dir: ledger_dir.display().to_string(),
            started_at_ms: now_ms(),
        };

        Ok(ValidatorSession {
            kind,
            handle,
            child: guard,
            started_at: Instant::now(),
        })
    }
}

fn resolve_binary(name: &str) -> CommandResult<PathBuf> {
    let probe = toolchain::probe_tool(name, &["--version"]);
    if !probe.present {
        return Err(CommandError::user_fixable(
            "solana_toolchain_missing",
            format!("{name} not found on PATH. Install the Solana CLI or surfpool."),
        ));
    }
    match probe.path {
        Some(path) => Ok(PathBuf::from(path)),
        None => Err(CommandError::system_fault(
            "solana_toolchain_path_unavailable",
            format!("Could not resolve filesystem path for {name}."),
        )),
    }
}

fn ensure_dir(dir: &Path) -> CommandResult<()> {
    if dir.exists() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|err| {
        CommandError::system_fault(
            "solana_ledger_dir_create_failed",
            format!("Could not create ledger dir {}: {err}", dir.display()),
        )
    })
}

fn await_rpc_ready(guard: &mut ChildGuard, rpc_url: &str, timeout: Duration) -> CommandResult<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("http client should build");

    let deadline = Instant::now() + timeout;
    loop {
        match guard.try_wait() {
            Ok(Some(status)) => {
                return Err(CommandError::system_fault(
                    "solana_validator_exited",
                    format!(
                        "Validator exited before it was ready (status={status}). stderr: {}",
                        guard.stderr_tail()
                    ),
                ));
            }
            Ok(None) => {}
            Err(err) => {
                return Err(CommandError::system_fault(
                    "solana_validator_wait_failed",
                    format!("Could not poll validator child: {err}"),
                ));
            }
        }

        if probe_get_health(&client, rpc_url).is_ok() {
            return Ok(());
        }

        if Instant::now() >= deadline {
            return Err(CommandError::retryable(
                "solana_validator_boot_timeout",
                format!(
                    "Validator did not respond on {rpc_url} within {:?}",
                    timeout
                ),
            ));
        }

        std::thread::sleep(READINESS_PROBE_INTERVAL);
    }
}

fn probe_get_health(client: &Client, url: &str) -> Result<(), String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "getHealth" });
    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http {}", response.status().as_u16()));
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
