use serde_json::Value;
use xero_desktop_lib::commands::project_assets::URI_SCHEME;

#[test]
fn tauri_csp_allows_project_asset_previews_only_in_preview_directives() {
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: Value =
        serde_json::from_slice(&std::fs::read(&config_path).expect("read tauri.conf.json"))
            .expect("parse tauri.conf.json");
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
        csp_sources(security, "devCsp", "connect-src").contains(&"ws://localhost:3000".to_owned())
    );
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
