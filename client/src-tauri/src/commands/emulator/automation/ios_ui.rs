//! iOS UI tree dump.
//!
//! Depends on the idb `AccessibilityInfo` RPC, which returns a JSON tree
//! we can map 1:1 onto our shared [`UiTree`]. Until `idb.proto` is
//! vendored, this is a stub that returns a helpful error — the session
//! module surfaces the error into `emulator_ui_dump`'s response.

#![cfg(target_os = "macos")]

use super::{Bounds, UiNode, UiTree};
use crate::commands::emulator::ios::idb_client::IdbClient;
use crate::commands::CommandError;

/// Pull a fresh accessibility tree from idb. Consumed by `emulator_ui_dump`.
pub fn dump(client: &IdbClient) -> Result<UiTree, CommandError> {
    let raw = client.accessibility_tree()?;
    normalize_tree(raw).map_err(|msg| {
        CommandError::system_fault(
            "ios_ui_dump_parse_failed",
            format!("failed to map idb accessibility tree: {msg}"),
        )
    })
}

/// Map the idb accessibility JSON into our shared tree shape. The idb
/// response shape is (paraphrased):
/// ```json
/// {
///   "type": "XCUIElementTypeButton",
///   "identifier": "continue",
///   "label": "Continue",
///   "value": "",
///   "enabled": true,
///   "focused": false,
///   "frame": { "x": 10, "y": 10, "w": 100, "h": 44 },
///   "children": [ ... ]
/// }
/// ```
///
/// Exposed separately from `dump` so we can unit-test parsing without
/// needing a live idb_companion.
pub fn normalize_tree(raw: serde_json::Value) -> Result<UiTree, String> {
    let root = normalize_node(&raw).ok_or_else(|| "root missing required fields".to_string())?;
    Ok(UiTree { root })
}

fn normalize_node(value: &serde_json::Value) -> Option<UiNode> {
    let obj = value.as_object()?;
    let platform_role = obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let role = canonical_role(platform_role.as_deref());

    let id = obj
        .get("identifier")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let label = obj
        .get("label")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let value_str = obj
        .get("value")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let enabled = obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let focused = obj
        .get("focused")
        .and_then(|v| v.as_bool())
        .or_else(|| obj.get("hasFocus").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    let bounds = obj.get("frame").and_then(parse_frame).unwrap_or(Bounds {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    });

    let children = obj
        .get("children")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(normalize_node).collect())
        .unwrap_or_default();

    Some(UiNode {
        id,
        role,
        label,
        value: value_str,
        enabled,
        focused,
        bounds,
        platform_role,
        children,
    })
}

fn parse_frame(value: &serde_json::Value) -> Option<Bounds> {
    let obj = value.as_object()?;
    let x = obj.get("x")?.as_f64()? as i32;
    let y = obj.get("y")?.as_f64()? as i32;
    let w = obj.get("w").or_else(|| obj.get("width"))?.as_f64()? as i32;
    let h = obj.get("h").or_else(|| obj.get("height"))?.as_f64()? as i32;
    Some(Bounds { x, y, w, h })
}

fn canonical_role(platform: Option<&str>) -> String {
    let raw = platform.unwrap_or("");
    let tail = raw.strip_prefix("XCUIElementType").unwrap_or(raw);
    match tail {
        "Button" => "button",
        "StaticText" => "text",
        "TextField" | "SearchField" => "textfield",
        "SecureTextField" => "password",
        "Image" => "image",
        "Switch" => "switch",
        "Slider" => "slider",
        "ActivityIndicator" => "progress",
        "ProgressIndicator" => "progress",
        "Table" | "CollectionView" => "list",
        "ScrollView" => "scroll",
        "WebView" => "webview",
        "NavigationBar" => "toolbar",
        "TabBar" => "tablist",
        "Alert" | "Sheet" => "dialog",
        "Other" | "Any" | "" => "view",
        _ => "view",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_button_node() {
        let raw = json!({
            "type": "XCUIElementTypeButton",
            "identifier": "continue",
            "label": "Continue",
            "value": "",
            "enabled": true,
            "focused": false,
            "frame": { "x": 10, "y": 10, "w": 100, "h": 44 },
            "children": []
        });
        let tree = normalize_tree(raw).expect("ok");
        assert_eq!(tree.root.role, "button");
        assert_eq!(tree.root.label.as_deref(), Some("Continue"));
        assert_eq!(tree.root.id.as_deref(), Some("continue"));
        assert_eq!(tree.root.bounds.w, 100);
    }

    #[test]
    fn handles_missing_fields_gracefully() {
        let raw =
            json!({ "type": "XCUIElementTypeOther", "frame": { "x": 0, "y": 0, "w": 0, "h": 0 } });
        let tree = normalize_tree(raw).expect("ok");
        assert_eq!(tree.root.role, "view");
        assert!(tree.root.label.is_none());
    }
}
