// Commands surfaced only when the app is launched via `pnpm start` from a
// source checkout (signalled by `XERO_LAUNCH_MODE=local-source`). They let
// the onboarding "Local environment" step display and edit the
// auto-generated `server/.env` without users opening a text editor.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

const LAUNCH_MODE_ENV: &str = "XERO_LAUNCH_MODE";
const ENV_FILE_PATH_ENV: &str = "XERO_LOCAL_ENV_FILE";
const SECRET_KEY_BASE_BYTES: usize = 64;

const EDITABLE_KEYS: &[&str] = &[
    "PHX_HOST",
    "PORT",
    "DATABASE_URL",
    "CORS_ORIGINS",
    "POOL_SIZE",
    "RATE_LIMIT_PER_MINUTE",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEnvironmentConfig {
    pub launch_mode: Option<String>,
    pub env_file_path: Option<String>,
    pub phx_host: String,
    pub port: String,
    pub database_url: String,
    pub cors_origins: String,
    pub pool_size: String,
    pub rate_limit_per_minute: String,
    pub has_secret_key_base: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocalEnvironmentConfigRequest {
    pub phx_host: String,
    pub port: String,
    pub database_url: String,
    pub cors_origins: String,
    pub pool_size: String,
    pub rate_limit_per_minute: String,
}

#[tauri::command]
pub fn get_launch_mode() -> String {
    env::var(LAUNCH_MODE_ENV).unwrap_or_default()
}

#[tauri::command]
pub fn get_local_environment_config() -> CommandResult<LocalEnvironmentConfig> {
    let env_file = env_file_path()?;
    let contents = read_env_file(&env_file)?;
    Ok(parse_config(&contents, &env_file))
}

#[tauri::command]
pub fn save_local_environment_config(
    request: SaveLocalEnvironmentConfigRequest,
) -> CommandResult<LocalEnvironmentConfig> {
    validate_save_request(&request)?;

    let env_file = env_file_path()?;
    let original = read_env_file(&env_file)?;

    let updates = [
        ("PHX_HOST", request.phx_host.as_str()),
        ("PORT", request.port.as_str()),
        ("DATABASE_URL", request.database_url.as_str()),
        ("CORS_ORIGINS", request.cors_origins.as_str()),
        ("POOL_SIZE", request.pool_size.as_str()),
        (
            "RATE_LIMIT_PER_MINUTE",
            request.rate_limit_per_minute.as_str(),
        ),
    ];

    let next = apply_updates(&original, &updates);
    write_env_file(&env_file, &next)?;
    Ok(parse_config(&next, &env_file))
}

#[tauri::command]
pub fn regenerate_secret_key_base() -> CommandResult<()> {
    let env_file = env_file_path()?;
    let original = read_env_file(&env_file)?;
    let secret = generate_secret_key_base();
    let next = apply_updates(&original, &[("SECRET_KEY_BASE", &secret)]);
    write_env_file(&env_file, &next)?;
    Ok(())
}

fn env_file_path() -> CommandResult<PathBuf> {
    let raw = env::var(ENV_FILE_PATH_ENV).map_err(|_| {
        CommandError::user_fixable(
            "local_environment_unavailable",
            "Local environment editing is only available when launched via `pnpm start`.",
        )
    })?;
    if raw.is_empty() {
        return Err(CommandError::user_fixable(
            "local_environment_unavailable",
            "Local environment editing is only available when launched via `pnpm start`.",
        ));
    }
    Ok(PathBuf::from(raw))
}

fn read_env_file(path: &Path) -> CommandResult<String> {
    fs::read_to_string(path).map_err(|err| {
        CommandError::system_fault(
            "local_env_read_failed",
            format!("Could not read {}: {err}", path.display()),
        )
    })
}

fn write_env_file(path: &Path, contents: &str) -> CommandResult<()> {
    fs::write(path, contents).map_err(|err| {
        CommandError::system_fault(
            "local_env_write_failed",
            format!("Could not write {}: {err}", path.display()),
        )
    })
}

fn validate_save_request(request: &SaveLocalEnvironmentConfigRequest) -> CommandResult<()> {
    if request.phx_host.trim().is_empty() {
        return Err(CommandError::invalid_request("request.phxHost"));
    }
    if !is_valid_port(&request.port) {
        return Err(CommandError::user_fixable(
            "local_env_invalid_port",
            "Port must be an integer between 1 and 65535.",
        ));
    }
    if request.database_url.trim().is_empty() {
        return Err(CommandError::invalid_request("request.databaseUrl"));
    }
    if !is_positive_integer(&request.pool_size) {
        return Err(CommandError::user_fixable(
            "local_env_invalid_pool_size",
            "Pool size must be a positive integer.",
        ));
    }
    if !is_positive_integer(&request.rate_limit_per_minute) {
        return Err(CommandError::user_fixable(
            "local_env_invalid_rate_limit",
            "Rate limit must be a positive integer.",
        ));
    }
    if has_disallowed_chars(&request.phx_host)
        || has_disallowed_chars(&request.database_url)
        || has_disallowed_chars(&request.cors_origins)
    {
        return Err(CommandError::user_fixable(
            "local_env_invalid_characters",
            "Values cannot contain newlines or null bytes.",
        ));
    }
    Ok(())
}

fn is_valid_port(value: &str) -> bool {
    value
        .parse::<u32>()
        .ok()
        .filter(|port| (1..=65_535).contains(port))
        .is_some()
}

fn is_positive_integer(value: &str) -> bool {
    value.parse::<u32>().is_ok_and(|n| n > 0)
}

fn has_disallowed_chars(value: &str) -> bool {
    value.contains('\n') || value.contains('\r') || value.contains('\0')
}

fn parse_env_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents
        .lines()
        .find(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].to_string())
}

fn parse_config(contents: &str, env_file: &Path) -> LocalEnvironmentConfig {
    let secret = parse_env_value(contents, "SECRET_KEY_BASE").unwrap_or_default();
    LocalEnvironmentConfig {
        launch_mode: env::var(LAUNCH_MODE_ENV).ok().filter(|s| !s.is_empty()),
        env_file_path: Some(env_file.display().to_string()),
        phx_host: parse_env_value(contents, "PHX_HOST").unwrap_or_else(|| "127.0.0.1".to_string()),
        port: parse_env_value(contents, "PORT").unwrap_or_else(|| "4000".to_string()),
        database_url: parse_env_value(contents, "DATABASE_URL").unwrap_or_default(),
        cors_origins: parse_env_value(contents, "CORS_ORIGINS").unwrap_or_default(),
        pool_size: parse_env_value(contents, "POOL_SIZE").unwrap_or_else(|| "10".to_string()),
        rate_limit_per_minute: parse_env_value(contents, "RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|| "60".to_string()),
        has_secret_key_base: !secret.is_empty(),
    }
}

fn apply_updates(original: &str, updates: &[(&str, &str)]) -> String {
    let mut updated = Vec::new();
    let mut applied = vec![false; updates.len()];

    for line in original.lines() {
        let mut replaced = false;
        for (idx, (key, value)) in updates.iter().enumerate() {
            let prefix = format!("{key}=");
            if line.starts_with(&prefix) {
                updated.push(format!("{key}={value}"));
                applied[idx] = true;
                replaced = true;
                break;
            }
        }
        if !replaced {
            updated.push(line.to_string());
        }
    }

    // Append any keys that were not present in the original file.
    for (idx, applied_flag) in applied.iter().enumerate() {
        if !applied_flag {
            let (key, value) = updates[idx];
            // EDITABLE_KEYS guards what callers may pass in; treat the
            // SECRET_KEY_BASE regenerate path the same way.
            if EDITABLE_KEYS.contains(&key) || key == "SECRET_KEY_BASE" {
                updated.push(format!("{key}={value}"));
            }
        }
    }

    let mut joined = updated.join("\n");
    if original.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

fn generate_secret_key_base() -> String {
    let mut buf = [0u8; SECRET_KEY_BASE_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    STANDARD.encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# header comment
PHX_HOST=127.0.0.1
PORT=4000
SECRET_KEY_BASE=abc123==
DATABASE_URL=ecto://postgres:postgres@localhost/xero_prod
POOL_SIZE=10
ECTO_IPV6=
DNS_CLUSTER_QUERY=
CORS_ORIGINS=http://localhost:3000,tauri://localhost
RATE_LIMIT_PER_MINUTE=60
OBAN_QUEUES=default:10,mailers:5
";

    #[test]
    fn parse_config_reads_known_keys() {
        let env_file = Path::new("/tmp/sample.env");
        let cfg = parse_config(SAMPLE, env_file);
        assert_eq!(cfg.phx_host, "127.0.0.1");
        assert_eq!(cfg.port, "4000");
        assert_eq!(cfg.pool_size, "10");
        assert_eq!(cfg.rate_limit_per_minute, "60");
        assert!(cfg.has_secret_key_base);
        assert_eq!(cfg.env_file_path.as_deref(), Some("/tmp/sample.env"));
    }

    #[test]
    fn apply_updates_preserves_unrelated_lines_and_order() {
        let updated = apply_updates(SAMPLE, &[("PORT", "4321"), ("CORS_ORIGINS", "https://x")]);
        // header comment stays put
        assert!(updated.starts_with("# header comment\n"));
        // updated keys take new values
        assert!(updated.contains("\nPORT=4321\n"));
        assert!(updated.contains("\nCORS_ORIGINS=https://x\n"));
        // other keys remain at their original values
        assert!(updated.contains("\nSECRET_KEY_BASE=abc123==\n"));
        assert!(updated.contains("\nOBAN_QUEUES=default:10,mailers:5\n"));
        // trailing newline preserved
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn apply_updates_appends_missing_editable_keys() {
        let original = "# only comment\nSECRET_KEY_BASE=keep\n";
        let updated = apply_updates(original, &[("PORT", "9000")]);
        assert!(updated.contains("\nPORT=9000"));
        assert!(updated.contains("SECRET_KEY_BASE=keep"));
    }

    #[test]
    fn regenerate_secret_key_base_updates_inplace() {
        let next = apply_updates(SAMPLE, &[("SECRET_KEY_BASE", "new-secret")]);
        assert!(next.contains("\nSECRET_KEY_BASE=new-secret\n"));
        // Other keys untouched.
        assert!(next.contains("\nPORT=4000\n"));
    }

    #[test]
    fn validate_save_request_rejects_invalid_port() {
        let result = validate_save_request(&SaveLocalEnvironmentConfigRequest {
            phx_host: "127.0.0.1".into(),
            port: "abc".into(),
            database_url: "ecto://x".into(),
            cors_origins: "*".into(),
            pool_size: "10".into(),
            rate_limit_per_minute: "60".into(),
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "local_env_invalid_port");
    }

    #[test]
    fn validate_save_request_rejects_newlines() {
        let result = validate_save_request(&SaveLocalEnvironmentConfigRequest {
            phx_host: "127.0.0.1".into(),
            port: "4000".into(),
            database_url: "ecto://x\nINJECT=1".into(),
            cors_origins: "*".into(),
            pool_size: "10".into(),
            rate_limit_per_minute: "60".into(),
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "local_env_invalid_characters");
    }

    #[test]
    fn generate_secret_key_base_is_high_entropy() {
        let a = generate_secret_key_base();
        let b = generate_secret_key_base();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }
}
