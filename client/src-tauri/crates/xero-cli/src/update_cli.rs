use std::{
    cmp::Ordering,
    collections::BTreeMap,
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    reject_unknown_options, response, take_bool_flag, take_help, take_option, CliError,
    CliResponse, GlobalOptions,
};

const DEFAULT_MANIFEST_URL: &str = "https://xeroshell.com/downloads/tui/latest/manifest.json";
const UPDATE_SCHEMA: &str = "xero.tui.update.v1";
const UPDATE_USER_AGENT: &str = concat!("xero/", env!("CARGO_PKG_VERSION"));
const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub platform: String,
    pub manifest_url: String,
    pub asset: Option<ResolvedUpdateAsset>,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedUpdateAsset {
    pub url: String,
    pub sha256: String,
    pub binary: String,
    pub archive: String,
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    #[serde(default)]
    schema: Option<String>,
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
    assets: BTreeMap<String, UpdateAsset>,
}

#[derive(Debug, Deserialize)]
struct UpdateAsset {
    url: String,
    sha256: String,
    #[serde(default)]
    binary: Option<String>,
    #[serde(default)]
    archive: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Semver {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<String>,
}

pub(crate) fn dispatch_update(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if args.is_empty() {
        return Ok(response(
            &globals,
            "Usage: xero update check|install\nChecks for and installs xero TUI updates.",
            json!({ "command": "update" }),
        ));
    }
    match args.first().map(String::as_str) {
        Some("check") | Some("status") => command_update_check(globals, args[1..].to_vec()),
        Some("install") | Some("apply") => command_update_install(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown update command `{other}`. Use `xero update check` or `xero update install`."
        ))),
        None => command_update_check(globals, Vec::new()),
    }
}

pub(crate) fn check_for_update(manifest_url: Option<&str>) -> Result<UpdateCheck, CliError> {
    let manifest_url = manifest_url
        .map(ToOwned::to_owned)
        .or_else(|| env::var("XERO_UPDATE_MANIFEST_URL").ok())
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_owned());
    let manifest = fetch_manifest(&manifest_url)?;
    check_from_manifest(&manifest, &manifest_url, platform_key())
}

pub(crate) fn update_notice(check: &UpdateCheck) -> Option<String> {
    check.update_available.then(|| {
        format!(
            "Update available: v{} (run `xero update install`)",
            check.latest_version
        )
    })
}

fn command_update_check(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero update check [--manifest-url URL]\nChecks whether the installed xero TUI is out of date.",
            json!({ "command": "update check" }),
        ));
    }

    let manifest_url = take_option(&mut args, "--manifest-url")?;
    reject_unknown_options(&args)?;
    let check = check_for_update(manifest_url.as_deref())?;
    let text = if check.update_available {
        format!(
            "Update available: xero {} -> {}. Run `xero update install`.",
            check.current_version, check.latest_version
        )
    } else {
        format!("xero {} is up to date.", check.current_version)
    };

    Ok(response(
        &globals,
        text,
        json!({
            "kind": "updateCheck",
            "schema": UPDATE_SCHEMA,
            "currentVersion": check.current_version,
            "latestVersion": check.latest_version,
            "updateAvailable": check.update_available,
            "platform": check.platform,
            "manifestUrl": check.manifest_url,
            "asset": check.asset.as_ref().map(update_asset_json),
            "notes": check.notes,
            "pubDate": check.pub_date,
        }),
    ))
}

fn command_update_install(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero update install [--manifest-url URL] [--force] [--install-path PATH]\nDownloads, verifies, and installs the latest xero TUI binary.",
            json!({ "command": "update install" }),
        ));
    }

    let manifest_url = take_option(&mut args, "--manifest-url")?;
    let install_path = take_option(&mut args, "--install-path")?.map(PathBuf::from);
    let force = take_bool_flag(&mut args, "--force");
    reject_unknown_options(&args)?;

    let check = check_for_update(manifest_url.as_deref())?;
    if !check.update_available && !force {
        return Ok(response(
            &globals,
            format!("xero {} is already up to date.", check.current_version),
            json!({
                "kind": "updateInstall",
                "schema": UPDATE_SCHEMA,
                "status": "upToDate",
                "currentVersion": check.current_version,
                "latestVersion": check.latest_version,
                "platform": check.platform,
            }),
        ));
    }

    let asset = check.asset.as_ref().ok_or_else(|| {
        CliError::user_fixable(
            "xero_update_platform_unsupported",
            format!(
                "No xero update asset is available for platform `{}`.",
                check.platform
            ),
        )
    })?;
    let target_path = match install_path {
        Some(path) => path,
        None => current_exe_path()?,
    };
    let install_status = install_asset(asset, &target_path)?;

    Ok(response(
        &globals,
        match install_status {
            InstallStatus::Installed => format!(
                "Installed xero {}. Restart any running TUI sessions to use it.",
                check.latest_version
            ),
            #[cfg(windows)]
            InstallStatus::Scheduled => format!(
                "Downloaded xero {}. Windows will replace the binary after this process exits.",
                check.latest_version
            ),
        },
        json!({
            "kind": "updateInstall",
            "schema": UPDATE_SCHEMA,
            "status": install_status.as_str(),
            "currentVersion": check.current_version,
            "latestVersion": check.latest_version,
            "platform": check.platform,
            "installedPath": target_path.display().to_string(),
            "asset": update_asset_json(asset),
        }),
    ))
}

fn update_asset_json(asset: &ResolvedUpdateAsset) -> serde_json::Value {
    json!({
        "url": asset.url,
        "sha256": asset.sha256,
        "binary": asset.binary,
        "archive": asset.archive,
    })
}

fn fetch_manifest(url: &str) -> Result<UpdateManifest, CliError> {
    let response = http_client()?
        .get(url)
        .send()
        .map_err(|error| update_network_error("xero_update_manifest_fetch_failed", error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CliError::user_fixable(
            "xero_update_manifest_unavailable",
            format!("Xero could not fetch the update manifest: HTTP {status}."),
        ));
    }
    response.json().map_err(|error| {
        CliError::user_fixable(
            "xero_update_manifest_invalid",
            format!("Xero could not decode the update manifest: {error}"),
        )
    })
}

fn check_from_manifest(
    manifest: &UpdateManifest,
    manifest_url: &str,
    platform: &str,
) -> Result<UpdateCheck, CliError> {
    if manifest
        .schema
        .as_deref()
        .is_some_and(|schema| schema != UPDATE_SCHEMA)
    {
        return Err(CliError::user_fixable(
            "xero_update_manifest_schema_unsupported",
            format!(
                "Xero expected update manifest schema `{UPDATE_SCHEMA}`, got `{}`.",
                manifest.schema.as_deref().unwrap_or_default()
            ),
        ));
    }

    let current_version = env!("CARGO_PKG_VERSION").to_owned();
    let latest_version = normalize_version(&manifest.version);
    let update_available =
        compare_versions(&latest_version, &current_version)? == Ordering::Greater;
    let asset = manifest
        .assets
        .get(platform)
        .map(|asset| resolve_asset(asset, manifest_url))
        .transpose()?;

    Ok(UpdateCheck {
        current_version,
        latest_version,
        update_available,
        platform: platform.to_owned(),
        manifest_url: manifest_url.to_owned(),
        asset,
        notes: manifest.notes.clone(),
        pub_date: manifest.pub_date.clone(),
    })
}

fn resolve_asset(asset: &UpdateAsset, manifest_url: &str) -> Result<ResolvedUpdateAsset, CliError> {
    let url = resolve_asset_url(manifest_url, &asset.url)?;
    let archive = asset
        .archive
        .clone()
        .or_else(|| file_name_from_url(&url))
        .unwrap_or_else(|| "xero-update-archive".to_owned());
    Ok(ResolvedUpdateAsset {
        url,
        sha256: normalize_sha256(&asset.sha256)?,
        binary: asset.binary.clone().unwrap_or_else(default_binary_name),
        archive,
    })
}

fn resolve_asset_url(manifest_url: &str, value: &str) -> Result<String, CliError> {
    if value.starts_with("https://") || value.starts_with("http://") {
        return Ok(value.to_owned());
    }

    if value.starts_with('/') {
        let scheme_end = manifest_url.find("://").ok_or_else(|| {
            CliError::user_fixable(
                "xero_update_manifest_asset_url_invalid",
                format!("Update asset URL `{value}` is relative, but manifest URL is invalid."),
            )
        })?;
        let after_scheme = scheme_end + 3;
        let host_end = manifest_url[after_scheme..]
            .find('/')
            .map(|index| after_scheme + index)
            .unwrap_or(manifest_url.len());
        return Ok(format!("{}{}", &manifest_url[..host_end], value));
    }

    let base = manifest_url
        .rsplit_once('/')
        .map(|(base, _)| base)
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_update_manifest_asset_url_invalid",
                format!("Update asset URL `{value}` is relative, but manifest URL is invalid."),
            )
        })?;
    Ok(format!("{base}/{value}"))
}

fn install_asset(
    asset: &ResolvedUpdateAsset,
    target_path: &Path,
) -> Result<InstallStatus, CliError> {
    let temp_dir = unique_temp_dir()?;
    let result = install_asset_in_temp(asset, target_path, &temp_dir);
    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn install_asset_in_temp(
    asset: &ResolvedUpdateAsset,
    target_path: &Path,
    temp_dir: &Path,
) -> Result<InstallStatus, CliError> {
    fs::create_dir_all(temp_dir).map_err(|error| {
        CliError::system_fault(
            "xero_update_temp_dir_failed",
            format!(
                "Xero could not create update temp directory `{}`: {error}",
                temp_dir.display()
            ),
        )
    })?;
    let archive_path = temp_dir.join(&asset.archive);
    download_file(&asset.url, &archive_path)?;
    verify_file_sha256(&archive_path, &asset.sha256)?;
    let extract_dir = temp_dir.join("extract");
    fs::create_dir_all(&extract_dir).map_err(|error| {
        CliError::system_fault(
            "xero_update_extract_dir_failed",
            format!(
                "Xero could not create update extraction directory `{}`: {error}",
                extract_dir.display()
            ),
        )
    })?;
    let binary_path = extract_binary(&archive_path, &extract_dir, &asset.binary)?;
    replace_current_binary(&binary_path, target_path)
}

fn download_file(url: &str, path: &Path) -> Result<(), CliError> {
    let mut response = http_client()?
        .get(url)
        .send()
        .map_err(|error| update_network_error("xero_update_download_failed", error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CliError::user_fixable(
            "xero_update_download_unavailable",
            format!("Xero could not download the update archive: HTTP {status}."),
        ));
    }
    let mut file = fs::File::create(path).map_err(|error| {
        CliError::system_fault(
            "xero_update_archive_create_failed",
            format!(
                "Xero could not create update archive `{}`: {error}",
                path.display()
            ),
        )
    })?;
    io::copy(&mut response, &mut file).map_err(|error| {
        CliError::system_fault(
            "xero_update_archive_write_failed",
            format!(
                "Xero could not write update archive `{}`: {error}",
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn extract_binary(
    archive_path: &Path,
    extract_dir: &Path,
    binary_name: &str,
) -> Result<PathBuf, CliError> {
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if archive_name.ends_with(".zip") {
        extract_zip_binary(archive_path, extract_dir, binary_name)
    } else if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
        extract_tar_gz_binary(archive_path, extract_dir, binary_name)
    } else {
        Err(CliError::user_fixable(
            "xero_update_archive_type_unsupported",
            format!("Xero does not know how to extract `{archive_name}`."),
        ))
    }
}

fn extract_zip_binary(
    archive_path: &Path,
    extract_dir: &Path,
    binary_name: &str,
) -> Result<PathBuf, CliError> {
    let file = fs::File::open(archive_path).map_err(|error| {
        CliError::system_fault(
            "xero_update_archive_open_failed",
            format!(
                "Xero could not open update archive `{}`: {error}",
                archive_path.display()
            ),
        )
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        CliError::user_fixable(
            "xero_update_archive_invalid",
            format!("Xero could not read the zip update archive: {error}"),
        )
    })?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            CliError::user_fixable(
                "xero_update_archive_invalid",
                format!("Xero could not read the zip update entry: {error}"),
            )
        })?;
        if entry.is_dir() || !entry.name().ends_with(binary_name) {
            continue;
        }
        let output_path = extract_dir.join(binary_name);
        let mut output = fs::File::create(&output_path).map_err(|error| {
            CliError::system_fault(
                "xero_update_extract_failed",
                format!(
                    "Xero could not create extracted binary `{}`: {error}",
                    output_path.display()
                ),
            )
        })?;
        io::copy(&mut entry, &mut output).map_err(|error| {
            CliError::system_fault(
                "xero_update_extract_failed",
                format!(
                    "Xero could not extract binary `{}`: {error}",
                    output_path.display()
                ),
            )
        })?;
        mark_executable(&output_path)?;
        return Ok(output_path);
    }
    Err(missing_binary_error(binary_name, archive_path))
}

fn extract_tar_gz_binary(
    archive_path: &Path,
    extract_dir: &Path,
    binary_name: &str,
) -> Result<PathBuf, CliError> {
    let file = fs::File::open(archive_path).map_err(|error| {
        CliError::system_fault(
            "xero_update_archive_open_failed",
            format!(
                "Xero could not open update archive `{}`: {error}",
                archive_path.display()
            ),
        )
    })?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(extract_dir).map_err(|error| {
        CliError::user_fixable(
            "xero_update_archive_invalid",
            format!("Xero could not extract the tar.gz update archive: {error}"),
        )
    })?;
    let binary_path = find_extracted_binary(extract_dir, binary_name)?
        .ok_or_else(|| missing_binary_error(binary_name, archive_path))?;
    mark_executable(&binary_path)?;
    Ok(binary_path)
}

fn find_extracted_binary(root: &Path, binary_name: &str) -> Result<Option<PathBuf>, CliError> {
    for entry in fs::read_dir(root).map_err(|error| {
        CliError::system_fault(
            "xero_update_extract_read_failed",
            format!(
                "Xero could not inspect update extraction directory `{}`: {error}",
                root.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CliError::system_fault(
                "xero_update_extract_read_failed",
                format!("Xero could not inspect update extraction entry: {error}"),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_extracted_binary(&path, binary_name)? {
                return Ok(Some(found));
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn missing_binary_error(binary_name: &str, archive_path: &Path) -> CliError {
    CliError::user_fixable(
        "xero_update_archive_missing_binary",
        format!(
            "The update archive `{}` did not contain `{binary_name}`.",
            archive_path.display()
        ),
    )
}

fn verify_file_sha256(path: &Path, expected: &str) -> Result<(), CliError> {
    let mut file = fs::File::open(path).map_err(|error| {
        CliError::system_fault(
            "xero_update_archive_open_failed",
            format!(
                "Xero could not open update archive `{}`: {error}",
                path.display()
            ),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            CliError::system_fault(
                "xero_update_archive_read_failed",
                format!(
                    "Xero could not read update archive `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(CliError::user_fixable(
            "xero_update_checksum_mismatch",
            format!(
                "Xero rejected the update archive because its checksum was {actual}, expected {expected}."
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallStatus {
    Installed,
    #[cfg(windows)]
    Scheduled,
}

impl InstallStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            #[cfg(windows)]
            Self::Scheduled => "scheduled",
        }
    }
}

#[cfg(not(windows))]
fn replace_current_binary(source: &Path, target: &Path) -> Result<InstallStatus, CliError> {
    let parent = target.parent().ok_or_else(|| {
        CliError::user_fixable(
            "xero_update_install_path_invalid",
            format!(
                "Install path `{}` has no parent directory.",
                target.display()
            ),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError::system_fault(
            "xero_update_install_dir_failed",
            format!(
                "Xero could not create install directory `{}`: {error}",
                parent.display()
            ),
        )
    })?;
    let staged = parent.join(format!(
        ".{}.new",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("xero")
    ));
    fs::copy(source, &staged).map_err(|error| {
        CliError::system_fault(
            "xero_update_stage_failed",
            format!(
                "Xero could not stage update binary `{}`: {error}",
                staged.display()
            ),
        )
    })?;
    mark_executable(&staged)?;

    let backup = parent.join(format!(
        ".{}.old",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("xero")
    ));
    let had_existing = target.exists();
    if had_existing {
        let _ = fs::remove_file(&backup);
        fs::rename(target, &backup).map_err(|error| {
            let _ = fs::remove_file(&staged);
            CliError::user_fixable(
                "xero_update_replace_failed",
                format!(
                    "Xero could not move the existing binary `{}` aside: {error}",
                    target.display()
                ),
            )
        })?;
    }
    if let Err(error) = fs::rename(&staged, target) {
        if had_existing {
            let _ = fs::rename(&backup, target);
        }
        let _ = fs::remove_file(&staged);
        return Err(CliError::user_fixable(
            "xero_update_replace_failed",
            format!(
                "Xero could not install the update to `{}`: {error}",
                target.display()
            ),
        ));
    }
    let _ = fs::remove_file(&backup);
    Ok(InstallStatus::Installed)
}

#[cfg(windows)]
fn replace_current_binary(source: &Path, target: &Path) -> Result<InstallStatus, CliError> {
    let parent = target.parent().ok_or_else(|| {
        CliError::user_fixable(
            "xero_update_install_path_invalid",
            format!(
                "Install path `{}` has no parent directory.",
                target.display()
            ),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError::system_fault(
            "xero_update_install_dir_failed",
            format!(
                "Xero could not create install directory `{}`: {error}",
                parent.display()
            ),
        )
    })?;
    let staged = parent.join("xero.update.exe");
    fs::copy(source, &staged).map_err(|error| {
        CliError::system_fault(
            "xero_update_stage_failed",
            format!(
                "Xero could not stage update binary `{}`: {error}",
                staged.display()
            ),
        )
    })?;
    let script = parent.join("xero-update.cmd");
    let pid = process::id();
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             :wait\r\n\
             tasklist /FI \"PID eq {pid}\" | find \"{pid}\" > nul\r\n\
             if not errorlevel 1 (\r\n\
             timeout /T 1 /NOBREAK > nul\r\n\
             goto wait\r\n\
             )\r\n\
             move /Y \"{}\" \"{}\" > nul\r\n\
             del \"%~f0\" > nul\r\n",
            staged.display(),
            target.display(),
        ),
    )
    .map_err(|error| {
        CliError::system_fault(
            "xero_update_stage_failed",
            format!(
                "Xero could not write Windows updater script `{}`: {error}",
                script.display()
            ),
        )
    })?;
    process::Command::new("cmd")
        .arg("/C")
        .arg(&script)
        .spawn()
        .map_err(|error| {
            CliError::system_fault(
                "xero_update_replace_failed",
                format!("Xero could not schedule Windows binary replacement: {error}"),
            )
        })?;
    Ok(InstallStatus::Scheduled)
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            CliError::system_fault(
                "xero_update_metadata_failed",
                format!(
                    "Xero could not inspect update binary `{}`: {error}",
                    path.display()
                ),
            )
        })?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| {
        CliError::system_fault(
            "xero_update_permissions_failed",
            format!(
                "Xero could not mark update binary `{}` executable: {error}",
                path.display()
            ),
        )
    })
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

fn http_client() -> Result<reqwest::blocking::Client, CliError> {
    reqwest::blocking::Client::builder()
        .timeout(NETWORK_TIMEOUT)
        .user_agent(UPDATE_USER_AGENT)
        .build()
        .map_err(|error| {
            CliError::system_fault(
                "xero_update_http_client_failed",
                format!("Xero could not create the update HTTP client: {error}"),
            )
        })
}

fn update_network_error(code: &str, error: reqwest::Error) -> CliError {
    CliError::user_fixable(
        code,
        format!("Xero could not contact the update service: {error}"),
    )
}

fn current_exe_path() -> Result<PathBuf, CliError> {
    env::current_exe().map_err(|error| {
        CliError::system_fault(
            "xero_update_current_exe_failed",
            format!("Xero could not resolve the current executable path: {error}"),
        )
    })
}

fn unique_temp_dir() -> Result<PathBuf, CliError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            CliError::system_fault(
                "xero_update_clock_failed",
                format!("Xero could not create a unique update directory: {error}"),
            )
        })?
        .as_nanos();
    Ok(env::temp_dir().join(format!("xero-update-{}-{nanos}", process::id())))
}

fn platform_key() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        "unsupported"
    }
}

fn default_binary_name() -> String {
    if cfg!(windows) {
        "xero.exe".to_owned()
    } else {
        "xero".to_owned()
    }
}

fn file_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    without_query
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_sha256(value: &str) -> Result<String, CliError> {
    let trimmed = value.split_whitespace().next().unwrap_or(value).trim();
    if trimmed.len() != 64 || !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(CliError::user_fixable(
            "xero_update_manifest_checksum_invalid",
            "The update manifest contained an invalid SHA-256 checksum.",
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn compare_versions(left: &str, right: &str) -> Result<Ordering, CliError> {
    Ok(parse_semver(left)?.cmp(&parse_semver(right)?))
}

fn parse_semver(value: &str) -> Result<Semver, CliError> {
    let normalized = normalize_version(value);
    let without_build = normalized.split('+').next().unwrap_or(&normalized);
    let (core, pre) = without_build
        .split_once('-')
        .map(|(core, pre)| (core, Some(pre.to_owned())))
        .unwrap_or((without_build, None));
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(CliError::user_fixable(
            "xero_update_version_invalid",
            format!("Update version `{value}` is not semantic versioning."),
        ));
    }
    Ok(Semver {
        major: parse_version_part(parts[0], value)?,
        minor: parse_version_part(parts[1], value)?,
        patch: parse_version_part(parts[2], value)?,
        pre,
    })
}

fn normalize_version(value: &str) -> String {
    value.trim().trim_start_matches('v').to_owned()
}

fn parse_version_part(part: &str, original: &str) -> Result<u64, CliError> {
    if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(CliError::user_fixable(
            "xero_update_version_invalid",
            format!("Update version `{original}` is not semantic versioning."),
        ));
    }
    part.parse::<u64>().map_err(|error| {
        CliError::user_fixable(
            "xero_update_version_invalid",
            format!("Update version `{original}` is invalid: {error}"),
        )
    })
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> UpdateManifest {
        UpdateManifest {
            schema: Some(UPDATE_SCHEMA.into()),
            version: version.into(),
            notes: Some("notes".into()),
            pub_date: Some("2026-05-23T00:00:00Z".into()),
            assets: BTreeMap::from([(
                "x86_64-unknown-linux-gnu".into(),
                UpdateAsset {
                    url: "xero-x86_64-unknown-linux-gnu.tar.gz".into(),
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .into(),
                    binary: Some("xero".into()),
                    archive: None,
                },
            )]),
        }
    }

    #[test]
    fn semantic_version_compare_detects_newer_stable_release() {
        assert_eq!(
            compare_versions("0.2.0", "0.1.13").expect("compare versions"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("0.1.13", "0.1.13").expect("compare versions"),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions("1.0.0-beta.1", "1.0.0").expect("compare versions"),
            Ordering::Less
        );
    }

    #[test]
    fn manifest_check_resolves_relative_asset_url() {
        let check = check_from_manifest(
            &manifest("99.0.0"),
            "https://xeroshell.com/downloads/tui/latest/manifest.json",
            "x86_64-unknown-linux-gnu",
        )
        .expect("check manifest");
        let asset = check.asset.expect("asset");

        assert!(check.update_available);
        assert_eq!(
            asset.url,
            "https://xeroshell.com/downloads/tui/latest/xero-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(asset.binary, "xero");
    }

    #[test]
    fn manifest_check_reports_no_asset_for_unsupported_platform() {
        let check = check_from_manifest(
            &manifest(env!("CARGO_PKG_VERSION")),
            "https://xeroshell.com/downloads/tui/latest/manifest.json",
            "unsupported-target",
        )
        .expect("check manifest");

        assert!(!check.update_available);
        assert!(check.asset.is_none());
    }
}
