use serde_json::Value;
use xero_desktop_lib::commands::project_assets::URI_SCHEME;

#[test]
fn tauri_csp_allows_project_asset_previews_only_in_preview_directives() {
    let config = read_json("tauri.conf.json");
    let security = &config["app"]["security"];
    let scheme_source = format!("{URI_SCHEME}:");

    for directive in ["img-src", "media-src", "object-src", "frame-src"] {
        assert!(
            csp_sources(security, "csp", directive).contains(&scheme_source),
            "production CSP {directive} should allow {scheme_source}"
        );
        assert!(
            csp_sources(security, "devCsp", directive).contains(&scheme_source),
            "development CSP {directive} should allow {scheme_source}"
        );
    }

    assert!(
        !csp_sources(security, "csp", "script-src").contains(&scheme_source),
        "project assets must not be script-loadable"
    );
    assert!(
        !csp_sources(security, "devCsp", "script-src").contains(&scheme_source),
        "project assets must not be script-loadable in dev"
    );
    assert!(csp_sources(security, "csp", "connect-src").contains(&"ipc:".to_owned()));
    assert!(
        csp_sources(security, "csp", "connect-src").contains(&"http://ipc.localhost".to_owned())
    );
    assert!(
        csp_sources(security, "devCsp", "connect-src").contains(&"ws://localhost:26100".to_owned())
    );
}

#[test]
fn tauri_config_enables_in_app_browser_webview_capability() {
    let config = read_json("tauri.conf.json");
    let browser_capability = read_json("capabilities/browser.json");
    let security = &config["app"]["security"];

    let configured_capabilities = string_array(security, "capabilities");
    assert!(
        configured_capabilities.contains(&"browser-webview".to_owned()),
        "tauri.conf.json must include browser-webview because setting app.security.capabilities narrows the enabled capability files"
    );

    assert_eq!(browser_capability["identifier"], "browser-webview");
    assert!(
        string_array(&browser_capability, "webviews").contains(&"xero-browser-tab-*".to_owned()),
        "browser-webview must apply to dynamically-created in-app browser tabs"
    );

    let remote_urls = browser_capability["remote"]["urls"]
        .as_array()
        .expect("browser-webview remote.urls must be an array")
        .iter()
        .map(|source| {
            source
                .as_str()
                .expect("browser-webview remote.urls entries must be strings")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let allows_http = remote_urls.contains(&"http://*:*".to_owned());
    let allows_https = remote_urls.contains(&"https://*:*".to_owned());
    assert!(
        allows_http && allows_https,
        "browser-webview must allow http(s) pages with non-default ports so localhost dev servers can call the browser bridge"
    );
}

fn read_json(relative_path: &str) -> Value {
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    serde_json::from_slice(&std::fs::read(&config_path).unwrap_or_else(|error| {
        panic!("read {relative_path}: {error}");
    }))
    .unwrap_or_else(|error| panic!("parse {relative_path}: {error}"))
}

fn csp_sources(security: &Value, profile: &str, directive: &str) -> Vec<String> {
    security[profile][directive]
        .as_array()
        .unwrap_or_else(|| panic!("{profile}.{directive} must be a source array"))
        .iter()
        .map(|source| {
            source
                .as_str()
                .unwrap_or_else(|| panic!("{profile}.{directive} entries must be strings"))
                .to_owned()
        })
        .collect()
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be a string array"))
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .unwrap_or_else(|| panic!("{key} entries must be strings"))
                .to_owned()
        })
        .collect()
}
