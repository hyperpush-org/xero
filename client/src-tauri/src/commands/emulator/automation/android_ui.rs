//! Android UI tree dump.
//!
//! `adb shell uiautomator dump /dev/tty` prints a snapshot of the window
//! hierarchy as XML directly to stdout — we parse it with `quick-xml` and
//! normalize the AOSP attribute soup into the shared `UiTree` shape.

use std::io::{Error, ErrorKind, Result};

use quick_xml::events::{attributes::Attributes, Event};
use quick_xml::Reader;

use super::{Bounds, UiNode, UiTree};
use crate::commands::emulator::android::adb::Adb;

/// Pull a fresh UI tree from the device. Blocks until `uiautomator dump`
/// completes — typically <500 ms on a Pixel-class AVD.
pub fn dump(adb: &Adb) -> Result<UiTree> {
    let output = adb.shell(["uiautomator", "dump", "/dev/tty"])?;
    // `uiautomator dump` sometimes prefixes output with a status line like
    // "UI hierchary dumped to: /dev/tty"; strip anything before the first `<`.
    let xml_start = output
        .find("<?xml")
        .or_else(|| output.find("<hierarchy"))
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("uiautomator dump produced no XML — raw output: {output}"),
            )
        })?;
    let xml = &output[xml_start..];
    parse_xml(xml)
}

/// Parse the raw `uiautomator` XML document into a [`UiTree`].
pub fn parse_xml(xml: &str) -> Result<UiTree> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<UiNode> = Vec::new();
    let mut root: Option<UiNode> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(err) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("xml parse error: {err}"),
                ));
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                if name == b"hierarchy" {
                    stack.push(root_placeholder());
                } else if name == b"node" {
                    let node = node_from_attrs(e.attributes())?;
                    stack.push(node);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                if name == b"hierarchy" || name == b"node" {
                    if let Some(done) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(done);
                        } else {
                            root = Some(done);
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_vec();
                if name == b"node" {
                    let node = node_from_attrs(e.attributes())?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        stack.push(node);
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }

    let root = root
        .or_else(|| stack.pop())
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "uiautomator XML contained no nodes"))?;

    Ok(UiTree { root })
}

fn root_placeholder() -> UiNode {
    UiNode {
        id: None,
        role: "hierarchy".to_string(),
        label: None,
        value: None,
        enabled: true,
        focused: false,
        bounds: Bounds {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        platform_role: None,
        children: Vec::new(),
    }
}

fn node_from_attrs(attrs: Attributes<'_>) -> Result<UiNode> {
    let mut id: Option<String> = None;
    let mut platform_role: Option<String> = None;
    let mut label: Option<String> = None;
    let mut value: Option<String> = None;
    let mut enabled = true;
    let mut focused = false;
    let mut bounds = Bounds {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    for attr in attrs.flatten() {
        let key = attr.key.as_ref();
        let text = attr
            .unescape_value()
            .map_err(|e| Error::new(ErrorKind::InvalidData, format!("attr decode: {e}")))?
            .into_owned();

        match key {
            b"class" => platform_role = Some(text),
            b"resource-id" => {
                if !text.is_empty() {
                    id = Some(text);
                }
            }
            b"content-desc" => {
                if !text.is_empty() {
                    label = Some(text);
                }
            }
            b"text" => {
                if !text.is_empty() {
                    if value.is_none() {
                        value = Some(text.clone());
                    }
                    if label.is_none() {
                        label = Some(text);
                    }
                }
            }
            b"enabled" => enabled = text == "true",
            b"focused" => focused = text == "true",
            b"bounds" => bounds = parse_bounds(&text).unwrap_or(bounds),
            _ => {}
        }
    }

    let role = normalize_role(platform_role.as_deref());

    Ok(UiNode {
        id,
        role,
        label,
        value,
        enabled,
        focused,
        bounds,
        platform_role,
        children: Vec::new(),
    })
}

/// `bounds="[x1,y1][x2,y2]"` → `Bounds { x, y, w, h }`.
fn parse_bounds(input: &str) -> Option<Bounds> {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            '[' | ']' => ' ',
            ',' => ' ',
            c => c,
        })
        .collect();
    let mut parts = cleaned.split_whitespace();
    let x1: i32 = parts.next()?.parse().ok()?;
    let y1: i32 = parts.next()?.parse().ok()?;
    let x2: i32 = parts.next()?.parse().ok()?;
    let y2: i32 = parts.next()?.parse().ok()?;
    Some(Bounds {
        x: x1,
        y: y1,
        w: (x2 - x1).max(0),
        h: (y2 - y1).max(0),
    })
}

/// Map the AOSP class-name soup into a short, canonical role. We keep the
/// full platform role on `UiNode.platform_role` so selectors can still
/// fingerprint specific widgets when they need to.
fn normalize_role(platform: Option<&str>) -> String {
    let raw = platform.unwrap_or("");
    let tail = raw.rsplit('.').next().unwrap_or(raw);
    match tail {
        "Button" | "ImageButton" | "CompoundButton" => "button",
        "TextView" => "text",
        "EditText" | "AutoCompleteTextView" => "textfield",
        "ImageView" => "image",
        "CheckBox" => "checkbox",
        "RadioButton" => "radio",
        "Switch" | "ToggleButton" => "switch",
        "Spinner" => "combobox",
        "ProgressBar" => "progress",
        "SeekBar" => "slider",
        "ListView" | "RecyclerView" => "list",
        "GridView" => "grid",
        "ScrollView" | "HorizontalScrollView" | "NestedScrollView" => "scroll",
        "WebView" => "webview",
        "LinearLayout" | "FrameLayout" | "RelativeLayout" | "ConstraintLayout"
        | "CoordinatorLayout" => "group",
        "ViewPager" | "ViewPager2" => "pager",
        "TabLayout" => "tablist",
        "Toolbar" | "ActionBar" => "toolbar",
        "Dialog" | "AlertDialog" => "dialog",
        _ => "view",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
    <node index="0" text="" resource-id="com.android.systemui:id/clock" class="android.widget.TextView" package="com.android.systemui" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][150,60]">
        <node index="0" text="OK" resource-id="btn_ok" class="android.widget.Button" package="com.android.systemui" content-desc="" enabled="true" focused="false" bounds="[0,0][60,40]" />
    </node>
</hierarchy>"#;

    #[test]
    fn parses_sample_tree_shape() {
        let tree = parse_xml(SAMPLE).expect("parse");
        assert_eq!(tree.root.role, "hierarchy");
        assert_eq!(tree.root.children.len(), 1);
        let clock = &tree.root.children[0];
        assert_eq!(clock.role, "text");
        assert_eq!(clock.id.as_deref(), Some("com.android.systemui:id/clock"));
        assert_eq!(clock.children.len(), 1);
        let ok = &clock.children[0];
        assert_eq!(ok.role, "button");
        assert_eq!(ok.label.as_deref(), Some("OK"));
        assert_eq!(ok.bounds.w, 60);
        assert_eq!(ok.bounds.h, 40);
    }

    #[test]
    fn parse_bounds_handles_negatives() {
        let b = parse_bounds("[-10,-20][10,20]").unwrap();
        assert_eq!(b.x, -10);
        assert_eq!(b.y, -20);
        assert_eq!(b.w, 20);
        assert_eq!(b.h, 40);
    }

    #[test]
    fn parse_bounds_clamps_inverted_rectangles() {
        let b = parse_bounds("[100,100][50,50]").unwrap();
        assert_eq!(b.w, 0);
        assert_eq!(b.h, 0);
    }
}
