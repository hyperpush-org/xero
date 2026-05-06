//! Metro Inspector WebSocket client for React Native / Expo apps.
//!
//! Connects to Metro's built-in inspector proxy (default port 8081, Expo
//! uses 19000-19006) to provide element-at-point inspection, component
//! source mapping, and highlight overlays for RN/Expo apps running in
//! the iOS Simulator.
//!
//! Protocol: HTTP `GET /json/list` → discover debuggable pages, then
//! WebSocket connection using a subset of Chrome DevTools Protocol (CDP).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::Bounds;
use crate::commands::CommandError;

/// A debuggable page/target exposed by Metro.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorPage {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub vm: String,
    #[serde(default)]
    pub description: String,
}

/// Information about an element at a specific point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementInfo {
    pub component_name: Option<String>,
    pub native_type: Option<String>,
    pub bounds: Bounds,
    #[serde(default)]
    pub props: Value,
    pub source: Option<SourceLocation>,
}

/// Source file + line for a React component.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Metro inspector status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetroStatus {
    pub connected: bool,
    pub port: u16,
    pub pages: Vec<InspectorPage>,
}

/// Live connection to a Metro Inspector WebSocket.
pub struct MetroInspector {
    ws: tungstenite::WebSocket<TcpStream>,
    msg_id: AtomicU32,
    port: u16,
    #[allow(dead_code)]
    page_id: String,
}

impl MetroInspector {
    /// Discover Metro on the given port range and connect to the first
    /// available debuggable page.
    pub fn connect_auto(port_range: &[u16]) -> Result<(Self, MetroStatus), CommandError> {
        let port = discover_metro(port_range).ok_or_else(|| {
            CommandError::user_fixable(
                "metro_not_found",
                format!(
                    "Metro bundler not found on ports {:?}. \
                     Make sure your React Native / Expo app is running.",
                    port_range
                ),
            )
        })?;
        Self::connect(port)
    }

    /// Connect to Metro on a specific port.
    pub fn connect(port: u16) -> Result<(Self, MetroStatus), CommandError> {
        let pages = fetch_pages(port)?;
        if pages.is_empty() {
            return Err(CommandError::user_fixable(
                "metro_no_pages",
                "Metro is running but no debuggable pages found.".to_string(),
            ));
        }

        // Pick the first page (usually "React Native Experimental (Improved Chrome Reloading)").
        let page = &pages[0];
        let ws_url = format!("ws://127.0.0.1:{}/inspector/device?page={}", port, page.id);

        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_secs(5),
        )
        .map_err(|e| {
            CommandError::system_fault(
                "metro_ws_connect_failed",
                format!("WebSocket connect to Metro: {e}"),
            )
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

        let (ws, _response) =
            tungstenite::client(&ws_url, stream).map_err(|e| {
                CommandError::system_fault(
                    "metro_ws_handshake_failed",
                    format!("WebSocket handshake: {e}"),
                )
            })?;

        let status = MetroStatus {
            connected: true,
            port,
            pages: pages.clone(),
        };

        Ok((
            Self {
                ws,
                msg_id: AtomicU32::new(1),
                port,
                page_id: page.id.clone(),
            },
            status,
        ))
    }

    /// Get the element at a device-pixel coordinate.
    pub fn element_at_point(&mut self, x: f32, y: f32) -> Result<ElementInfo, CommandError> {
        // Inject JS that walks the React fiber tree to find the component
        // at the given coordinates. Uses __REACT_DEVTOOLS_GLOBAL_HOOK__
        // which Metro injects when DevTools support is enabled.
        let js = format!(
            r#"(function() {{
                try {{
                    var hook = window.__REACT_DEVTOOLS_GLOBAL_HOOK__;
                    if (!hook || !hook.renderers || hook.renderers.size === 0) {{
                        return JSON.stringify({{ error: 'no_devtools_hook' }});
                    }}
                    var renderer = hook.renderers.values().next().value;
                    if (!renderer || !renderer.findFiberByHostInstance) {{
                        return JSON.stringify({{ error: 'no_renderer' }});
                    }}

                    // Walk all host instances to find one containing the point.
                    var x = {x};
                    var y = {y};
                    var best = null;
                    var bestArea = Infinity;

                    function walkFiber(fiber) {{
                        if (!fiber) return;
                        if (fiber.stateNode && fiber.stateNode.measure) {{
                            try {{
                                fiber.stateNode.measure(function(fx, fy, fw, fh, px, py) {{
                                    if (px <= x && py <= y && px + fw >= x && py + fh >= y) {{
                                        var area = fw * fh;
                                        if (area < bestArea) {{
                                            bestArea = area;
                                            var source = fiber._debugSource || null;
                                            var name = fiber.type;
                                            if (typeof name === 'function') name = name.displayName || name.name || 'Anonymous';
                                            if (typeof name === 'object' && name !== null) name = name.displayName || 'ForwardRef';
                                            best = {{
                                                componentName: typeof name === 'string' ? name : String(name || 'View'),
                                                nativeType: fiber.stateNode.viewConfig ? fiber.stateNode.viewConfig.uiViewClassName : null,
                                                bounds: {{ x: Math.round(px), y: Math.round(py), w: Math.round(fw), h: Math.round(fh) }},
                                                source: source ? {{ file: source.fileName, line: source.lineNumber, column: source.columnNumber || 0 }} : null
                                            }};
                                        }}
                                    }}
                                }});
                            }} catch(e) {{}}
                        }}
                        walkFiber(fiber.child);
                        walkFiber(fiber.sibling);
                    }}

                    var roots = hook.getFiberRoots ? hook.getFiberRoots(1) : null;
                    if (roots && roots.size > 0) {{
                        roots.forEach(function(root) {{
                            walkFiber(root.current);
                        }});
                    }}

                    if (best) {{
                        return JSON.stringify(best);
                    }}
                    return JSON.stringify({{ error: 'no_element_at_point' }});
                }} catch(e) {{
                    return JSON.stringify({{ error: e.message }});
                }}
            }})()"#,
        );

        let result = self.evaluate_js(&js)?;
        let parsed: Value = serde_json::from_str(&result).map_err(|e| {
            CommandError::system_fault(
                "metro_parse_error",
                format!("failed to parse inspector result: {e}"),
            )
        })?;

        if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
            return Err(CommandError::system_fault(
                "metro_element_error",
                format!("inspector: {err}"),
            ));
        }

        Ok(ElementInfo {
            component_name: parsed["componentName"].as_str().map(|s| s.to_string()),
            native_type: parsed["nativeType"].as_str().map(|s| s.to_string()),
            bounds: Bounds {
                x: parsed["bounds"]["x"].as_i64().unwrap_or(0) as i32,
                y: parsed["bounds"]["y"].as_i64().unwrap_or(0) as i32,
                w: parsed["bounds"]["w"].as_i64().unwrap_or(0) as i32,
                h: parsed["bounds"]["h"].as_i64().unwrap_or(0) as i32,
            },
            props: Value::Object(Default::default()),
            source: parsed.get("source").and_then(|s| {
                Some(SourceLocation {
                    file: s["file"].as_str()?.to_string(),
                    line: s["line"].as_u64()? as u32,
                    column: s["column"].as_u64().unwrap_or(0) as u32,
                })
            }),
        })
    }

    /// Get the full React component tree.
    pub fn component_tree(&mut self) -> Result<Value, CommandError> {
        let js = r#"(function() {
            try {
                var hook = window.__REACT_DEVTOOLS_GLOBAL_HOOK__;
                if (!hook) return JSON.stringify({ error: 'no_devtools_hook' });

                function fiberToTree(fiber, depth) {
                    if (!fiber || depth > 50) return null;
                    var name = fiber.type;
                    if (typeof name === 'function') name = name.displayName || name.name || 'Anonymous';
                    if (typeof name === 'object' && name !== null) name = name.displayName || 'ForwardRef';
                    if (typeof name !== 'string') name = String(name || '');
                    if (!name || name === 'View' || name === 'RCTView') {
                        // Skip anonymous native wrappers, recurse children.
                        var kids = [];
                        var child = fiber.child;
                        while (child) { var t = fiberToTree(child, depth); if (t) kids.push(t); child = child.sibling; }
                        if (kids.length === 1) return kids[0];
                        if (kids.length === 0) return null;
                        return { name: 'Fragment', children: kids };
                    }
                    var node = { name: name };
                    var source = fiber._debugSource;
                    if (source) node.source = { file: source.fileName, line: source.lineNumber };
                    var kids = [];
                    var child = fiber.child;
                    while (child) { var t = fiberToTree(child, depth + 1); if (t) kids.push(t); child = child.sibling; }
                    if (kids.length > 0) node.children = kids;
                    return node;
                }

                var roots = hook.getFiberRoots ? hook.getFiberRoots(1) : null;
                if (roots && roots.size > 0) {
                    var root = roots.values().next().value;
                    return JSON.stringify(fiberToTree(root.current, 0) || { error: 'empty_tree' });
                }
                return JSON.stringify({ error: 'no_roots' });
            } catch(e) {
                return JSON.stringify({ error: e.message });
            }
        })()"#;

        let result = self.evaluate_js(js)?;
        serde_json::from_str(&result).map_err(|e| {
            CommandError::system_fault(
                "metro_tree_parse_error",
                format!("failed to parse component tree: {e}"),
            )
        })
    }

    /// Port the inspector is connected to.
    pub fn port(&self) -> u16 {
        self.port
    }

    // MARK: - CDP message helpers

    fn evaluate_js(&mut self, expression: &str) -> Result<String, CommandError> {
        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        let msg = json!({
            "id": id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expression,
                "returnByValue": true,
            }
        });

        self.ws
            .send(tungstenite::Message::Text(msg.to_string()))
            .map_err(|e| {
                CommandError::system_fault("metro_ws_send", format!("WebSocket send: {e}"))
            })?;

        // Read responses until we get ours.
        for _ in 0..50 {
            let response = self.ws.read().map_err(|e| {
                CommandError::system_fault("metro_ws_read", format!("WebSocket read: {e}"))
            })?;

            if let tungstenite::Message::Text(text) = response {
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if val["id"].as_u64() == Some(id as u64) {
                        if let Some(err) = val.get("error") {
                            return Err(CommandError::system_fault(
                                "metro_cdp_error",
                                format!("CDP error: {}", err),
                            ));
                        }
                        if let Some(result) = val["result"]["result"]["value"].as_str() {
                            return Ok(result.to_string());
                        }
                        return Err(CommandError::system_fault(
                            "metro_no_result",
                            "CDP returned no value".to_string(),
                        ));
                    }
                }
            }
        }

        Err(CommandError::system_fault(
            "metro_response_timeout",
            format!("no CDP response for message id {id} within 50 reads"),
        ))
    }
}

// MARK: - Discovery

/// Default ports to scan for Metro.
pub const METRO_PORT_RANGE: &[u16] = &[8081, 19000, 19001, 19002, 19003, 19004, 19005, 19006];

/// Probe ports for a running Metro bundler. Returns the first port that
/// responds to `GET /status`.
pub fn discover_metro(port_range: &[u16]) -> Option<u16> {
    for &port in port_range {
        if probe_metro_port(port) {
            return Some(port);
        }
    }
    None
}

fn probe_metro_port(port: u16) -> bool {
    let addr = format!("127.0.0.1:{port}");
    let Ok(mut stream) = TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        Duration::from_millis(200),
    ) else {
        return false;
    };
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

    let request = format!(
        "GET /status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).unwrap_or(0);
    let response = String::from_utf8_lossy(&buf[..n]);
    // Metro responds with "packager-status:running" or similar.
    response.contains("200") || response.contains("packager-status")
}

/// Fetch the list of debuggable pages from Metro's /json/list endpoint.
fn fetch_pages(port: u16) -> Result<Vec<InspectorPage>, CommandError> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        Duration::from_secs(5),
    )
    .map_err(|e| {
        CommandError::system_fault(
            "metro_connect_failed",
            format!("connect to Metro on port {port}: {e}"),
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let request = format!(
        "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(|e| {
        CommandError::system_fault("metro_request_failed", format!("send request: {e}"))
    })?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    let response = String::from_utf8_lossy(&buf);

    // Extract JSON body after \r\n\r\n.
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .trim();

    // Handle chunked transfer encoding: strip chunk headers.
    let json_body = if body.contains('[') {
        let start = body.find('[').unwrap_or(0);
        let end = body.rfind(']').map(|i| i + 1).unwrap_or(body.len());
        &body[start..end]
    } else {
        body
    };

    serde_json::from_str::<Vec<InspectorPage>>(json_body).map_err(|e| {
        CommandError::system_fault(
            "metro_parse_pages_failed",
            format!("parse /json/list: {e} (body: {json_body})"),
        )
    })
}
