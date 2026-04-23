use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime};
use url::Url;

use crate::commands::{CommandError, CommandResult};

use super::{
    bridge::{run_script, BridgeWaiters, BRIDGE_DEFAULT_TIMEOUT_MS, BRIDGE_MAX_TIMEOUT_MS},
    tabs::BrowserTabs,
};

pub const MAX_SELECTOR_LEN: usize = 1_024;
pub const MAX_TEXT_INPUT_LEN: usize = 16_384;
pub const MAX_QUERY_LIMIT: usize = 100;

pub fn parse_url(input: &str) -> CommandResult<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("url"));
    }
    Url::parse(trimmed).map_err(|error| {
        CommandError::user_fixable(
            "browser_invalid_url",
            format!("Cadence could not parse the requested URL: {error}"),
        )
    })
}

pub fn validate_selector(selector: &str) -> CommandResult<String> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("selector"));
    }
    if trimmed.len() > MAX_SELECTOR_LEN {
        return Err(CommandError::user_fixable(
            "browser_selector_too_long",
            format!("Selector exceeds the {MAX_SELECTOR_LEN} character limit."),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_text_input(text: &str) -> CommandResult<&str> {
    if text.len() > MAX_TEXT_INPUT_LEN {
        return Err(CommandError::user_fixable(
            "browser_text_too_long",
            format!("Input text exceeds the {MAX_TEXT_INPUT_LEN} character limit."),
        ));
    }
    Ok(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingMode {
    Replace,
    Append,
}

pub fn resolve_timeout(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(BRIDGE_DEFAULT_TIMEOUT_MS)
        .clamp(100, BRIDGE_MAX_TIMEOUT_MS)
}

// ---------- Action helpers --------------------------------------------------

pub fn click<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
    let body = format!(
        "const el = document.querySelector({sel});\
         if (!el) throw new Error('element not found: {sel}');\
         if (typeof el.scrollIntoView === 'function') el.scrollIntoView({{ block: 'center', inline: 'center' }});\
         if (typeof el.focus === 'function') el.focus();\
         if (typeof el.click === 'function') el.click();\
         else el.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }}));\
         return {{ selector: {sel} }};",
        sel = encoded,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn type_text<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    text: &str,
    mode: TypingMode,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    validate_text_input(text)?;
    let encoded_sel = serde_json::to_string(&selector).map_err(encode_err)?;
    let encoded_text = serde_json::to_string(text).map_err(encode_err)?;
    let append = matches!(mode, TypingMode::Append);
    let body = format!(
        "const el = document.querySelector({sel});\
         if (!el) throw new Error('element not found: {sel}');\
         if (typeof el.focus === 'function') el.focus();\
         const isInput = el.tagName === 'INPUT' || el.tagName === 'TEXTAREA';\
         const append = {append};\
         if (isInput) {{\
           el.value = append ? (el.value + {txt}) : {txt};\
           el.dispatchEvent(new Event('input', {{ bubbles: true }}));\
           el.dispatchEvent(new Event('change', {{ bubbles: true }}));\
         }} else if (el.isContentEditable) {{\
           if (!append) el.textContent = '';\
           document.execCommand('insertText', false, {txt});\
         }} else {{\
           throw new Error('element is not editable');\
         }}\
         return {{ selector: {sel}, appended: append }};",
        sel = encoded_sel,
        txt = encoded_text,
        append = append,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn scroll_to<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let body = if let Some(selector) = selector {
        let selector = validate_selector(selector)?;
        let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
        format!(
            "const el = document.querySelector({sel});\
             if (!el) throw new Error('element not found: {sel}');\
             el.scrollIntoView({{ block: 'center', inline: 'center', behavior: 'instant' }});\
             return {{ selector: {sel} }};",
            sel = encoded,
        )
    } else {
        let x = x.unwrap_or(0.0);
        let y = y.unwrap_or(0.0);
        format!(
            "window.scrollTo({{ left: {x}, top: {y}, behavior: 'instant' }});\
             return {{ x: window.scrollX, y: window.scrollY }};",
            x = x,
            y = y,
        )
    };
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn press_key<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    key: &str,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    if key.trim().is_empty() {
        return Err(CommandError::invalid_request("key"));
    }
    let encoded_key = serde_json::to_string(key).map_err(encode_err)?;
    let selector_body = if let Some(selector) = selector {
        let selector = validate_selector(selector)?;
        let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
        format!(
            "const el = document.querySelector({sel});\
             if (!el) throw new Error('element not found: {sel}');\
             if (typeof el.focus === 'function') el.focus();",
            sel = encoded,
        )
    } else {
        "const el = document.activeElement || document.body;".to_string()
    };

    let body = format!(
        "{selector_body}\
         const key = {key};\
         const dispatch = (type) => el.dispatchEvent(new KeyboardEvent(type, {{ key, bubbles: true, cancelable: true }}));\
         dispatch('keydown');\
         dispatch('keypress');\
         dispatch('keyup');\
         return {{ key }};",
        selector_body = selector_body,
        key = encoded_key,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn read_text<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let body = if let Some(selector) = selector {
        let selector = validate_selector(selector)?;
        let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
        format!(
            "const el = document.querySelector({sel});\
             if (!el) throw new Error('element not found: {sel}');\
             const text = (el.innerText || el.textContent || '').trim();\
             return {{ selector: {sel}, text }};",
            sel = encoded,
        )
    } else {
        "return {{ text: (document.body && (document.body.innerText || document.body.textContent) || '').trim() }};".to_string()
    };
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn query<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    let limit = limit.unwrap_or(20).min(MAX_QUERY_LIMIT);
    let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
    let body = format!(
        "const list = Array.from(document.querySelectorAll({sel})).slice(0, {limit});\
         return list.map((el) => ({{\
           tag: el.tagName ? el.tagName.toLowerCase() : null,\
           id: el.id || null,\
           classes: el.className ? String(el.className).split(/\\s+/).filter(Boolean) : [],\
           role: el.getAttribute ? el.getAttribute('role') : null,\
           name: el.getAttribute ? (el.getAttribute('name') || el.getAttribute('aria-label')) : null,\
           href: el.getAttribute ? el.getAttribute('href') : null,\
           text: ((el.innerText || el.textContent || '').trim()).slice(0, 400),\
           visible: !!(el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)),\
         }}));",
        sel = encoded,
        limit = limit,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn wait_for_selector<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    timeout_ms: Option<u64>,
    visible: bool,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    let total = resolve_timeout(timeout_ms);
    let encoded = serde_json::to_string(&selector).map_err(encode_err)?;
    let body = format!(
        "const sel = {sel}; const deadline = Date.now() + {total}; const mustBeVisible = {visible};\
         const isVisible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));\
         while (Date.now() < deadline) {{\
           const el = document.querySelector(sel);\
           if (el && (!mustBeVisible || isVisible(el))) return {{ selector: sel, found: true, waitedMs: {total} - (deadline - Date.now()) }};\
           await new Promise((resolve) => setTimeout(resolve, 80));\
         }}\
         throw new Error('timed out waiting for ' + sel);",
        sel = encoded,
        total = total,
        visible = visible,
    );
    // bridge timeout = waited timeout + a small slack so the bridge doesn't abort mid-poll
    run_script(app, tabs, waiters, &body, total.saturating_add(2_000))
}

pub fn wait_for_load<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let total = resolve_timeout(timeout_ms);
    let body = format!(
        "const deadline = Date.now() + {total};\
         while (Date.now() < deadline) {{\
           if (document.readyState === 'complete') return {{ readyState: 'complete', url: location.href, title: document.title }};\
           await new Promise((resolve) => setTimeout(resolve, 80));\
         }}\
         throw new Error('timed out waiting for load');",
        total = total,
    );
    run_script(app, tabs, waiters, &body, total.saturating_add(2_000))
}

pub fn history_state<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
) -> CommandResult<JsonValue> {
    let body =
        "return { length: history.length, url: location.href, title: document.title };".to_string();
    run_script(app, tabs, waiters, &body, 2_000)
}

pub fn history_navigate<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    _waiters: &Arc<BridgeWaiters>,
    delta: i32,
) -> CommandResult<JsonValue> {
    // Must NOT round-trip through the bridge: history.go() starts tearing the
    // current page down, so any pending browser_internal_reply IPC is discarded
    // when the bridge unloads. We'd block until the bridge timeout (2s+) waiting
    // for a reply that never arrives, which makes the button feel frozen.
    let webview = tabs.active_webview(app)?;
    let script = format!("try {{ window.history.go({delta}); }} catch (_e) {{}}");
    webview.eval(&script).map_err(|error| {
        CommandError::system_fault(
            "browser_history_navigate_failed",
            format!("Cadence could not run the browser history action: {error}"),
        )
    })?;
    Ok(serde_json::json!({ "delta": delta }))
}

pub fn stop<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    _waiters: &Arc<BridgeWaiters>,
) -> CommandResult<JsonValue> {
    // Same rationale as history_navigate: window.stop() may abort an in-flight
    // load whose bridge context is about to be discarded.
    let webview = tabs.active_webview(app)?;
    webview
        .eval("try { if (typeof window.stop === 'function') window.stop(); } catch (_e) {}")
        .map_err(|error| {
            CommandError::system_fault(
                "browser_stop_failed",
                format!("Cadence could not run window.stop(): {error}"),
            )
        })?;
    Ok(serde_json::json!({ "stopped": true }))
}

pub fn cookies_get<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
) -> CommandResult<JsonValue> {
    let body = "return document.cookie ? document.cookie.split(';').map((c) => { const idx = c.indexOf('='); return idx === -1 ? { name: c.trim(), value: '' } : { name: c.slice(0, idx).trim(), value: c.slice(idx + 1) }; }) : [];".to_string();
    run_script(app, tabs, waiters, &body, 2_000)
}

pub fn cookies_set<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    cookie: &str,
) -> CommandResult<JsonValue> {
    if cookie.trim().is_empty() {
        return Err(CommandError::invalid_request("cookie"));
    }
    let encoded = serde_json::to_string(cookie).map_err(encode_err)?;
    let body = format!(
        "document.cookie = {c}; return {{ set: true }};",
        c = encoded
    );
    run_script(app, tabs, waiters, &body, 2_000)
}

pub fn storage_read<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    area: StorageArea,
    key: Option<&str>,
) -> CommandResult<JsonValue> {
    let store = match area {
        StorageArea::Local => "localStorage",
        StorageArea::Session => "sessionStorage",
    };
    let body = if let Some(key) = key {
        let encoded = serde_json::to_string(key).map_err(encode_err)?;
        format!(
            "return {{ value: {store}.getItem({k}) }};",
            store = store,
            k = encoded,
        )
    } else {
        format!(
            "const out = {{}}; for (let i = 0; i < {store}.length; i++) {{ const k = {store}.key(i); if (k !== null) out[k] = {store}.getItem(k); }} return out;",
            store = store,
        )
    };
    run_script(app, tabs, waiters, &body, 2_000)
}

pub fn storage_write<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    area: StorageArea,
    key: &str,
    value: Option<&str>,
) -> CommandResult<JsonValue> {
    if key.trim().is_empty() {
        return Err(CommandError::invalid_request("key"));
    }
    let store = match area {
        StorageArea::Local => "localStorage",
        StorageArea::Session => "sessionStorage",
    };
    let encoded_key = serde_json::to_string(key).map_err(encode_err)?;
    let body = match value {
        Some(value) => {
            let encoded_value = serde_json::to_string(value).map_err(encode_err)?;
            format!(
                "{store}.setItem({k}, {v}); return {{ set: true }};",
                store = store,
                k = encoded_key,
                v = encoded_value,
            )
        }
        None => format!(
            "{store}.removeItem({k}); return {{ removed: true }};",
            store = store,
            k = encoded_key,
        ),
    };
    run_script(app, tabs, waiters, &body, 2_000)
}

pub fn storage_clear<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    area: StorageArea,
) -> CommandResult<JsonValue> {
    let store = match area {
        StorageArea::Local => "localStorage",
        StorageArea::Session => "sessionStorage",
    };
    let body = format!(
        "{store}.clear(); return {{ cleared: true }};",
        store = store
    );
    run_script(app, tabs, waiters, &body, 2_000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageArea {
    Local,
    Session,
}

fn encode_err(error: serde_json::Error) -> CommandError {
    CommandError::system_fault(
        "browser_payload_encode_failed",
        format!("Cadence could not encode browser bridge payload: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_rejects_blank() {
        assert!(parse_url("").is_err());
        assert!(parse_url("   ").is_err());
    }

    #[test]
    fn parse_url_rejects_invalid() {
        assert!(parse_url("not a url").is_err());
    }

    #[test]
    fn parse_url_accepts_https() {
        let parsed = parse_url("https://example.com/path?q=1").unwrap();
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("example.com"));
    }

    #[test]
    fn validate_selector_enforces_len_and_nonempty() {
        assert!(validate_selector("").is_err());
        assert!(validate_selector("   ").is_err());
        assert!(validate_selector("button.primary").is_ok());
        let huge = "a".repeat(MAX_SELECTOR_LEN + 1);
        assert!(validate_selector(&huge).is_err());
    }

    #[test]
    fn validate_text_input_enforces_len() {
        assert!(validate_text_input("short").is_ok());
        let huge = "a".repeat(MAX_TEXT_INPUT_LEN + 1);
        assert!(validate_text_input(&huge).is_err());
    }

    #[test]
    fn resolve_timeout_clamps() {
        assert_eq!(resolve_timeout(None), BRIDGE_DEFAULT_TIMEOUT_MS);
        assert_eq!(resolve_timeout(Some(50)), 100);
        assert_eq!(
            resolve_timeout(Some(BRIDGE_MAX_TIMEOUT_MS + 5_000)),
            BRIDGE_MAX_TIMEOUT_MS
        );
        assert_eq!(resolve_timeout(Some(5_000)), 5_000);
    }
}
