use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{
    webview::{cookie, Cookie},
    AppHandle, Manager, Runtime, State,
};
use time::{Duration, OffsetDateTime};

use crate::commands::{CommandError, CommandResult};

use super::tabs::BrowserTabs;
use super::BrowserState;

const HELPER_BIN_NAME: &str = "xero-cookie-importer";

#[derive(Debug, Clone, Copy)]
enum BrowserSource {
    Chrome,
    Chromium,
    Brave,
    Edge,
    Opera,
    OperaGx,
    Vivaldi,
    Arc,
    Firefox,
    LibreWolf,
    Zen,
    #[cfg(target_os = "macos")]
    Safari,
}

impl BrowserSource {
    fn label(self) -> &'static str {
        match self {
            BrowserSource::Chrome => "Google Chrome",
            BrowserSource::Chromium => "Chromium",
            BrowserSource::Brave => "Brave",
            BrowserSource::Edge => "Microsoft Edge",
            BrowserSource::Opera => "Opera",
            BrowserSource::OperaGx => "Opera GX",
            BrowserSource::Vivaldi => "Vivaldi",
            BrowserSource::Arc => "Arc",
            BrowserSource::Firefox => "Firefox",
            BrowserSource::LibreWolf => "LibreWolf",
            BrowserSource::Zen => "Zen",
            #[cfg(target_os = "macos")]
            BrowserSource::Safari => "Safari",
        }
    }

    fn id(self) -> &'static str {
        match self {
            BrowserSource::Chrome => "chrome",
            BrowserSource::Chromium => "chromium",
            BrowserSource::Brave => "brave",
            BrowserSource::Edge => "edge",
            BrowserSource::Opera => "opera",
            BrowserSource::OperaGx => "opera_gx",
            BrowserSource::Vivaldi => "vivaldi",
            BrowserSource::Arc => "arc",
            BrowserSource::Firefox => "firefox",
            BrowserSource::LibreWolf => "librewolf",
            BrowserSource::Zen => "zen",
            #[cfg(target_os = "macos")]
            BrowserSource::Safari => "safari",
        }
    }

    fn all() -> Vec<BrowserSource> {
        let mut sources = vec![
            BrowserSource::Chrome,
            BrowserSource::Chromium,
            BrowserSource::Brave,
            BrowserSource::Edge,
            BrowserSource::Opera,
            BrowserSource::OperaGx,
            BrowserSource::Vivaldi,
            BrowserSource::Arc,
            BrowserSource::Firefox,
            BrowserSource::LibreWolf,
            BrowserSource::Zen,
        ];
        #[cfg(target_os = "macos")]
        sources.push(BrowserSource::Safari);
        sources
    }

    /// Detects installed browsers via file presence instead of rookie. Rookie's
    /// code path for chromium-based browsers decrypts the Safe Storage key up
    /// front, which fires a macOS Keychain permission prompt per browser on
    /// every probe — unusable for a background "what's installed" check. A
    /// `Path::exists()` stat is permission-free and runs instantly.
    fn detect_available(self) -> bool {
        let Some(home) = dirs::home_dir() else {
            return false;
        };
        for path in self.candidate_paths(&home) {
            if path_exists(&path) {
                return true;
            }
        }
        false
    }

    fn candidate_paths(self, home: &Path) -> Vec<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let app_support = home.join("Library").join("Application Support");
            match self {
                BrowserSource::Chrome => vec![app_support.join("Google/Chrome/Default/Cookies")],
                BrowserSource::Chromium => vec![app_support.join("Chromium/Default/Cookies")],
                BrowserSource::Brave => {
                    vec![app_support.join("BraveSoftware/Brave-Browser/Default/Cookies")]
                }
                BrowserSource::Edge => vec![app_support.join("Microsoft Edge/Default/Cookies")],
                BrowserSource::Opera => vec![app_support.join("com.operasoftware.Opera/Cookies")],
                BrowserSource::OperaGx => {
                    vec![app_support.join("com.operasoftware.OperaGX/Cookies")]
                }
                BrowserSource::Vivaldi => vec![app_support.join("Vivaldi/Default/Cookies")],
                BrowserSource::Arc => vec![app_support.join("Arc/User Data/Default/Cookies")],
                BrowserSource::Firefox => mozilla_profile_cookies(&app_support, "Firefox"),
                BrowserSource::LibreWolf => mozilla_profile_cookies(&app_support, "LibreWolf"),
                BrowserSource::Zen => mozilla_profile_cookies(&app_support, "zen"),
                BrowserSource::Safari => vec![
                    home.join("Library/HTTPStorages/com.apple.Safari/Cookies.binarycookies"),
                    home.join("Library/Cookies/Cookies.binarycookies"),
                ],
            }
        }
        #[cfg(target_os = "windows")]
        {
            let local = home.join("AppData/Local");
            let roaming = home.join("AppData/Roaming");
            match self {
                BrowserSource::Chrome => vec![local
                    .join("Google/Chrome/User Data/Default/Network/Cookies")],
                BrowserSource::Chromium => vec![local
                    .join("Chromium/User Data/Default/Network/Cookies")],
                BrowserSource::Brave => vec![local
                    .join("BraveSoftware/Brave-Browser/User Data/Default/Network/Cookies")],
                BrowserSource::Edge => vec![local
                    .join("Microsoft/Edge/User Data/Default/Network/Cookies")],
                BrowserSource::Opera => vec![roaming
                    .join("Opera Software/Opera Stable/Network/Cookies")],
                BrowserSource::OperaGx => vec![roaming
                    .join("Opera Software/Opera GX Stable/Network/Cookies")],
                BrowserSource::Vivaldi => vec![local
                    .join("Vivaldi/User Data/Default/Network/Cookies")],
                BrowserSource::Arc => vec![local
                    .join("Packages/TheBrowserCompany.Arc_ttt1ap7aakyb4/LocalCache/Local/Arc/User Data/Default/Network/Cookies")],
                BrowserSource::Firefox => mozilla_profile_cookies(&roaming, "Mozilla/Firefox"),
                BrowserSource::LibreWolf => mozilla_profile_cookies(&roaming, "LibreWolf"),
                BrowserSource::Zen => mozilla_profile_cookies(&roaming, "zen"),
            }
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            let config = home.join(".config");
            match self {
                BrowserSource::Chrome => vec![config.join("google-chrome/Default/Cookies")],
                BrowserSource::Chromium => vec![config.join("chromium/Default/Cookies")],
                BrowserSource::Brave => {
                    vec![config.join("BraveSoftware/Brave-Browser/Default/Cookies")]
                }
                BrowserSource::Edge => vec![config.join("microsoft-edge/Default/Cookies")],
                BrowserSource::Opera => vec![config.join("opera/Cookies")],
                BrowserSource::OperaGx => vec![config.join("opera-gx/Cookies")],
                BrowserSource::Vivaldi => vec![config.join("vivaldi/Default/Cookies")],
                BrowserSource::Arc => vec![],
                BrowserSource::Firefox => {
                    mozilla_profile_cookies(&home.join(".mozilla"), "firefox")
                }
                BrowserSource::LibreWolf => mozilla_profile_cookies(&home.join(".librewolf"), ""),
                BrowserSource::Zen => mozilla_profile_cookies(&home.join(".zen"), ""),
            }
        }
    }
}

fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

fn mozilla_profile_cookies(root: &Path, subdir: &str) -> Vec<PathBuf> {
    let profiles_dir = if subdir.is_empty() {
        root.join("Profiles")
    } else {
        root.join(subdir).join("Profiles")
    };
    let Ok(entries) = std::fs::read_dir(&profiles_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path().join("cookies.sqlite"))
        .filter(|path| path_exists(path))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBrowser {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CookieImportResult {
    pub source: String,
    pub imported: u32,
    pub skipped: u32,
    pub domains: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum HelperOutput {
    Ok { cookies: Vec<HelperCookie> },
    Err { message: String },
}

#[derive(Debug, Clone, Deserialize)]
struct HelperCookie {
    domain: String,
    path: String,
    secure: bool,
    expires: Option<u64>,
    name: String,
    value: String,
    http_only: bool,
    same_site: i64,
}

#[tauri::command]
pub fn browser_list_cookie_sources<R: Runtime>(
    _app: AppHandle<R>,
    _state: State<'_, BrowserState>,
) -> CommandResult<Vec<DetectedBrowser>> {
    // Path-only detection, no rookie invocation — probing via rookie would fire
    // a macOS Keychain prompt per chromium-based browser on every call.
    let mut out = Vec::new();
    for source in BrowserSource::all() {
        out.push(DetectedBrowser {
            id: source.id().to_string(),
            label: source.label().to_string(),
            available: source.detect_available(),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn browser_import_cookies<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, BrowserState>,
    source: String,
    domains: Option<Vec<String>>,
) -> CommandResult<CookieImportResult> {
    let source_enum = parse_source(&source)?;
    let helper = locate_helper()?;

    let cleaned_domains: Vec<String> = domains
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();

    let mut argv: Vec<&str> = vec!["import", source_enum.id()];
    for d in &cleaned_domains {
        argv.push(d);
    }

    let cookies = match run_helper(&helper, &argv)? {
        HelperOutput::Ok { cookies } => cookies,
        HelperOutput::Err { message } => {
            return Err(CommandError::user_fixable(
                "browser_cookie_read_failed",
                format!(
                    "Could not read cookies from {label}: {message}. Close that browser and try again; on macOS Xero may also need Full Disk Access.",
                    label = source_enum.label(),
                ),
            ));
        }
    };

    let tabs = state.tabs();
    let webview = resolve_any_webview(&app, &tabs)?;

    let mut imported: u32 = 0;
    let mut skipped: u32 = 0;
    let mut unique_domains = std::collections::HashSet::new();

    for raw in cookies {
        unique_domains.insert(raw.domain.clone());
        match build_cookie(&raw) {
            Some(cookie) => match webview.set_cookie(cookie) {
                Ok(()) => imported += 1,
                Err(_) => skipped += 1,
            },
            None => {
                skipped += 1;
            }
        }
    }

    Ok(CookieImportResult {
        source: source_enum.id().to_string(),
        imported,
        skipped,
        domains: unique_domains.len() as u32,
    })
}

fn parse_source(raw: &str) -> CommandResult<BrowserSource> {
    for candidate in BrowserSource::all() {
        if candidate.id().eq_ignore_ascii_case(raw) {
            return Ok(candidate);
        }
    }
    Err(CommandError::user_fixable(
        "browser_cookie_source_unknown",
        format!("Unknown cookie source `{raw}`."),
    ))
}

fn resolve_any_webview<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
) -> CommandResult<tauri::webview::Webview<R>> {
    // Cookies set on any of our webviews populate the shared WKWebsiteDataStore
    // / WebView2 cookie manager, so one live webview is enough to stage the
    // import for every tab (current and future).
    let list = tabs.list()?;
    for tab in list {
        if let Some(webview) = app.get_webview(&tab.label) {
            return Ok(webview);
        }
    }
    Err(CommandError::user_fixable(
        "browser_not_open",
        "Open a page in the in-app browser before importing cookies.",
    ))
}

fn locate_helper() -> CommandResult<PathBuf> {
    // The helper is built as a sibling binary in the same cargo workspace, so
    // in both `cargo tauri dev` and bundled builds it sits next to the main
    // exe. Fall back to PATH if that lookup fails so devs can stash it
    // somewhere custom without rebuilds.
    let helper_name = if cfg!(windows) {
        format!("{HELPER_BIN_NAME}.exe")
    } else {
        HELPER_BIN_NAME.to_string()
    };

    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join(&helper_name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Ok(PathBuf::from(helper_name))
}

fn run_helper(helper: &PathBuf, args: &[&str]) -> CommandResult<HelperOutput> {
    let output = Command::new(helper).args(args).output().map_err(|error| {
        CommandError::system_fault(
            "browser_cookie_helper_spawn_failed",
            format!("Could not launch cookie importer helper: {error}"),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last_line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");

    if last_line.is_empty() {
        return Err(CommandError::system_fault(
            "browser_cookie_helper_no_output",
            format!(
                "Cookie importer produced no output. stderr: {}",
                stderr.trim()
            ),
        ));
    }

    serde_json::from_str::<HelperOutput>(last_line).map_err(|error| {
        CommandError::system_fault(
            "browser_cookie_helper_parse_failed",
            format!("Could not parse cookie importer output: {error}. Raw: {last_line}"),
        )
    })
}

fn build_cookie(raw: &HelperCookie) -> Option<Cookie<'static>> {
    if raw.name.is_empty() {
        return None;
    }

    let domain = raw.domain.trim_matches('.').to_string();
    if domain.is_empty() {
        return None;
    }
    let path = if raw.path.is_empty() {
        "/".to_string()
    } else {
        raw.path.clone()
    };

    let same_site = match raw.same_site {
        0 => Some(cookie::SameSite::None),
        1 => Some(cookie::SameSite::Lax),
        2 => Some(cookie::SameSite::Strict),
        _ => None,
    };

    let mut builder = Cookie::build((raw.name.clone(), raw.value.clone()))
        .domain(domain)
        .path(path)
        .secure(raw.secure)
        .http_only(raw.http_only);

    if let Some(ss) = same_site {
        builder = builder.same_site(ss);
    }

    if let Some(expires) = raw.expires {
        if let Ok(dt) = OffsetDateTime::from_unix_timestamp(expires as i64) {
            builder = builder.expires(cookie::Expiration::DateTime(dt));
        }
    } else {
        // Session cookie — give it a far-future expiry so WKHTTPCookieStore
        // persists it past this webview's lifetime. Without an explicit
        // expiration, session cookies get dropped when the webview tears down.
        let expires = OffsetDateTime::now_utc() + Duration::days(30);
        builder = builder.expires(cookie::Expiration::DateTime(expires));
    }

    Some(builder.build())
}
