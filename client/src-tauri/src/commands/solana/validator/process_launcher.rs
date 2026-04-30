//! Helpers for building argv for the `solana-test-validator` and `surfpool`
//! CLIs. Kept pure so unit tests can assert the exact argv that would be
//! handed to the child — we never spawn the real binary in tests.

use std::path::{Path, PathBuf};

use super::{StartOpts, DEFAULT_RPC_PORT, DEFAULT_WS_PORT};

/// Argv for `solana-test-validator` suitable for localnet bring-up. The
/// binary itself is looked up by the caller (from `toolchain.rs`).
pub fn test_validator_args(opts: &StartOpts, ledger_dir: &Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("--ledger".into());
    args.push(ledger_dir.display().to_string());

    let rpc_port = opts.rpc_port.unwrap_or(DEFAULT_RPC_PORT);
    args.push("--rpc-port".into());
    args.push(rpc_port.to_string());

    // `solana-test-validator` does not take --ws-port; the WS port is
    // `rpc_port + 1` by convention. We still record the override if the
    // caller set one, for surfpool's sake.

    // Default: reset localnet ledgers on start (matches `solana-test-validator`
    // behaviour). Fork mode (handled by surfpool path) prefers reset=false.
    if opts.reset.unwrap_or(true) {
        args.push("--reset".into());
    }

    if let Some(limit) = opts.limit_ledger {
        args.push("--limit-ledger-size".into());
        args.push(limit.to_string());
    }

    for program in &opts.clone_programs {
        args.push("--clone".into());
        args.push(program.clone());
    }
    for account in &opts.clone_accounts {
        args.push("--clone".into());
        args.push(account.clone());
    }

    args.push("--quiet".into());
    args
}

/// Fallback argv for fork mode when `surfpool` is unavailable. This keeps
/// fork ledgers warm by default but still forwards clone flags through
/// `solana-test-validator --clone`.
pub fn test_validator_fork_args(opts: &StartOpts, ledger_dir: &Path) -> Vec<String> {
    let mut fork_opts = opts.clone();
    if fork_opts.reset.is_none() {
        fork_opts.reset = Some(false);
    }
    test_validator_args(&fork_opts, ledger_dir)
}

/// Argv for `surfpool start`. Surfpool exposes RPC/WS on the same ports as
/// the Anza `solana-test-validator` by default, and forks mainnet
/// implicitly.
pub fn surfpool_args(opts: &StartOpts, ledger_dir: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec!["start".into()];

    args.push("--rpc-port".into());
    args.push(opts.rpc_port.unwrap_or(DEFAULT_RPC_PORT).to_string());
    args.push("--ws-port".into());
    args.push(opts.ws_port.unwrap_or(DEFAULT_WS_PORT).to_string());
    args.push("--workspace".into());
    args.push(ledger_dir.display().to_string());

    for program in &opts.clone_programs {
        args.push("--clone-program".into());
        args.push(program.clone());
    }
    for account in &opts.clone_accounts {
        args.push("--clone-account".into());
        args.push(account.clone());
    }

    if opts.reset.unwrap_or(false) {
        args.push("--reset".into());
    }

    args
}

/// Decide where the cluster's ledger lives. Prefers the caller-supplied
/// path; otherwise a stable scratch dir inside the OS temp dir so repeated
/// `start` calls keep warm state for a human debugging session.
pub fn resolve_ledger_dir(opts: &StartOpts, tag: &str) -> PathBuf {
    opts.ledger_dir
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("xero-solana-{tag}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validator_args_include_rpc_port_and_ledger() {
        let opts = StartOpts {
            rpc_port: Some(12345),
            reset: Some(true),
            ..StartOpts::default()
        };
        let ledger = PathBuf::from("/tmp/xero-test");
        let args = test_validator_args(&opts, &ledger);

        let rpc_pos = args.iter().position(|a| a == "--rpc-port").unwrap();
        assert_eq!(args[rpc_pos + 1], "12345");
        assert!(args.contains(&"--reset".to_string()));
        assert!(args.contains(&"/tmp/xero-test".to_string()));
    }

    #[test]
    fn test_validator_args_forwards_clone_program_flags_in_order() {
        let opts = StartOpts {
            clone_programs: vec!["JUP6i...".into(), "JUP7i...".into()],
            ..StartOpts::default()
        };
        let ledger = PathBuf::from("/tmp");
        let args = test_validator_args(&opts, &ledger);
        let mut cursor = 0;
        for program in ["JUP6i...", "JUP7i..."] {
            let pos = args[cursor..]
                .iter()
                .position(|a| a == program)
                .expect("program flag present");
            assert_eq!(args[cursor + pos - 1], "--clone");
            cursor += pos + 1;
        }
    }

    #[test]
    fn test_validator_args_default_reset_enabled() {
        let args = test_validator_args(&StartOpts::default(), &PathBuf::from("/tmp"));
        assert!(
            args.contains(&"--reset".to_string()),
            "localnet defaults to --reset so the ledger never leaks state"
        );
    }

    #[test]
    fn test_validator_args_reset_opt_out() {
        let opts = StartOpts {
            reset: Some(false),
            ..StartOpts::default()
        };
        let args = test_validator_args(&opts, &PathBuf::from("/tmp"));
        assert!(!args.contains(&"--reset".to_string()));
    }

    #[test]
    fn test_validator_fork_args_uses_clones_without_default_reset() {
        let opts = StartOpts {
            clone_programs: vec!["JUP6i...".into()],
            clone_accounts: vec!["So11111111111111111111111111111111111111112".into()],
            ..StartOpts::default()
        };
        let args = test_validator_fork_args(&opts, &PathBuf::from("/tmp"));
        assert!(!args.contains(&"--reset".to_string()));
        assert_eq!(args.iter().filter(|arg| *arg == "--clone").count(), 2);
    }

    #[test]
    fn test_validator_fork_args_respects_explicit_reset() {
        let opts = StartOpts {
            reset: Some(true),
            ..StartOpts::default()
        };
        let args = test_validator_fork_args(&opts, &PathBuf::from("/tmp"));
        assert!(args.contains(&"--reset".to_string()));
    }

    #[test]
    fn surfpool_args_forwards_clone_accounts() {
        let opts = StartOpts {
            clone_accounts: vec!["So11111111111111111111111111111111111111112".into()],
            rpc_port: Some(7000),
            ws_port: Some(7001),
            ..StartOpts::default()
        };
        let ledger = PathBuf::from("/tmp");
        let args = surfpool_args(&opts, &ledger);
        let idx = args
            .iter()
            .position(|a| a == "--clone-account")
            .expect("clone-account present");
        assert_eq!(args[idx + 1], "So11111111111111111111111111111111111111112");
        let rpc = args.iter().position(|a| a == "--rpc-port").unwrap();
        assert_eq!(args[rpc + 1], "7000");
        let ws = args.iter().position(|a| a == "--ws-port").unwrap();
        assert_eq!(args[ws + 1], "7001");
    }

    #[test]
    fn resolve_ledger_dir_respects_caller_path() {
        let opts = StartOpts {
            ledger_dir: Some(PathBuf::from("/custom/ledger")),
            ..StartOpts::default()
        };
        assert_eq!(
            resolve_ledger_dir(&opts, "localnet"),
            PathBuf::from("/custom/ledger")
        );
    }

    #[test]
    fn resolve_ledger_dir_falls_back_to_temp_scratch() {
        let opts = StartOpts::default();
        let path = resolve_ledger_dir(&opts, "fork");
        assert!(path.starts_with(std::env::temp_dir()));
        assert!(path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .ends_with("-fork"));
    }
}
