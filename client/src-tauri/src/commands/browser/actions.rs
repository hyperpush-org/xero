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
pub const MAX_BROWSER_DIAGNOSTIC_LIMIT: usize = 200;
pub const MAX_STATE_SNAPSHOT_JSON_LEN: usize = 256 * 1024;

pub fn parse_url(input: &str) -> CommandResult<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("url"));
    }
    Url::parse(trimmed).map_err(|error| {
        CommandError::user_fixable(
            "browser_invalid_url",
            format!("Xero could not parse the requested URL: {error}"),
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
            format!("Xero could not run the browser history action: {error}"),
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
                format!("Xero could not run window.stop(): {error}"),
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

pub fn network_performance_summary<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let limit = limit.unwrap_or(100).clamp(1, MAX_BROWSER_DIAGNOSTIC_LIMIT);
    let body = format!(
        "const limit = {limit};\
         const sanitize = (value) => {{\
           try {{ const url = new URL(String(value || ''), location.href); url.search = ''; url.hash = ''; return url.href; }}\
           catch (_error) {{ return String(value || '').slice(0, 2048); }}\
         }};\
         const number = (value) => Number.isFinite(value) ? Math.round(value) : null;\
         const resourceEntries = (performance.getEntriesByType && performance.getEntriesByType('resource')) || [];\
         const navigationEntries = (performance.getEntriesByType && performance.getEntriesByType('navigation')) || [];\
         const entries = Array.from(navigationEntries).concat(Array.from(resourceEntries));\
         entries.sort((left, right) => (left.startTime || 0) - (right.startTime || 0));\
         const selected = entries.slice(-limit).map((entry) => ({{\
           url: sanitize(entry.name),\
           initiatorType: entry.initiatorType || entry.entryType || null,\
           startTimeMs: number(entry.startTime),\
           durationMs: number(entry.duration),\
           transferSize: Number.isFinite(entry.transferSize) ? Math.round(entry.transferSize) : null,\
           encodedBodySize: Number.isFinite(entry.encodedBodySize) ? Math.round(entry.encodedBodySize) : null,\
           decodedBodySize: Number.isFinite(entry.decodedBodySize) ? Math.round(entry.decodedBodySize) : null,\
           nextHopProtocol: entry.nextHopProtocol || null,\
           responseStatus: Number.isFinite(entry.responseStatus) ? Math.round(entry.responseStatus) : null,\
         }}));\
         return {{ entries: selected, total: entries.length, truncated: entries.length > selected.length }};",
        limit = limit,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn accessibility_tree<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector_json = match selector {
        Some(selector) => {
            let selector = validate_selector(selector)?;
            serde_json::to_string(&selector).map_err(encode_err)?
        }
        None => "null".into(),
    };
    let limit = limit.unwrap_or(100).clamp(1, MAX_BROWSER_DIAGNOSTIC_LIMIT);
    let body = format!(
        "const selector = {selector_json};\
         const root = selector ? document.querySelector(selector) : document.body;\
         if (!root) throw new Error('element not found: ' + selector);\
         const limit = {limit};\
         const implicitRole = (el) => {{\
           const tag = (el.tagName || '').toLowerCase();\
           if (tag === 'a' && el.hasAttribute('href')) return 'link';\
           if (tag === 'button') return 'button';\
           if (tag === 'img') return 'img';\
           if (tag === 'input') return el.type === 'checkbox' ? 'checkbox' : 'textbox';\
           if (tag === 'textarea') return 'textbox';\
           if (tag === 'select') return 'combobox';\
           if (/^h[1-6]$/.test(tag)) return 'heading';\
           if (tag === 'nav') return 'navigation';\
           if (tag === 'main') return 'main';\
           if (tag === 'form') return 'form';\
           return null;\
         }};\
         const isVisible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));\
         const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\\s+/g, ' ').slice(0, 200);\
         const nameOf = (el) => (el.getAttribute('aria-label') || el.getAttribute('alt') || el.getAttribute('title') || textOf(el)).slice(0, 200);\
         const nodes = [];\
         const queue = [root];\
         while (queue.length && nodes.length < limit) {{\
           const el = queue.shift();\
           if (!el || el.nodeType !== 1) continue;\
           const role = el.getAttribute('role') || implicitRole(el);\
           const name = nameOf(el);\
           const tag = (el.tagName || '').toLowerCase();\
           const visible = isVisible(el);\
           if (role || name || visible) {{\
             nodes.push({{\
               tag,\
               id: el.id || null,\
               role,\
               name: name || null,\
               text: textOf(el) || null,\
               visible,\
               disabled: !!(el.disabled || el.getAttribute('aria-disabled') === 'true'),\
               href: el.getAttribute('href') || null,\
             }});\
           }}\
           Array.prototype.forEach.call(el.children || [], (child) => queue.push(child));\
         }}\
         return {{ root: selector || 'body', nodes, truncated: queue.length > 0 }};",
        selector_json = selector_json,
        limit = limit,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn state_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    include_storage: bool,
    include_cookies: bool,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let body = format!(
        "const copyStorage = (store) => {{ const out = {{}}; for (let i = 0; i < store.length; i++) {{ const key = store.key(i); if (key !== null) out[key] = store.getItem(key); }} return out; }};\
         const cookies = {include_cookies} && document.cookie ? document.cookie.split(';').map((cookie) => {{ const idx = cookie.indexOf('='); return idx === -1 ? {{ name: cookie.trim(), value: '' }} : {{ name: cookie.slice(0, idx).trim(), value: cookie.slice(idx + 1) }}; }}) : [];\
         return {{\
           url: location.href,\
           title: document.title,\
           readyState: document.readyState,\
           cookies,\
           localStorage: {include_storage} ? copyStorage(localStorage) : null,\
           sessionStorage: {include_storage} ? copyStorage(sessionStorage) : null,\
         }};",
        include_storage = include_storage,
        include_cookies = include_cookies,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn state_restore<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    snapshot_json: &str,
    navigate: bool,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    if snapshot_json.len() > MAX_STATE_SNAPSHOT_JSON_LEN {
        return Err(CommandError::user_fixable(
            "browser_state_snapshot_too_large",
            format!("Browser state snapshots are limited to {MAX_STATE_SNAPSHOT_JSON_LEN} bytes."),
        ));
    }
    let snapshot = serde_json::from_str::<JsonValue>(snapshot_json).map_err(|error| {
        CommandError::user_fixable(
            "browser_state_snapshot_invalid",
            format!("Xero could not parse browser state snapshot JSON: {error}"),
        )
    })?;
    let snapshot = validate_state_restore_snapshot(snapshot)?;
    let snapshot_json = serde_json::to_string(&snapshot).map_err(encode_err)?;
    let body = format!(
        "const snapshot = {snapshot_json};\
         const setStorage = (store, values) => {{\
           if (!values || typeof values !== 'object') return 0;\
           let count = 0;\
           Object.entries(values).forEach(([key, value]) => {{ store.setItem(key, value == null ? '' : String(value)); count += 1; }});\
           return count;\
         }};\
         const localCount = setStorage(localStorage, snapshot.localStorage);\
         const sessionCount = setStorage(sessionStorage, snapshot.sessionStorage);\
         let cookieCount = 0;\
         if (Array.isArray(snapshot.cookies)) {{\
           snapshot.cookies.forEach((cookie) => {{\
             if (cookie && cookie.name) {{ document.cookie = String(cookie.name) + '=' + encodeURIComponent(String(cookie.value || '')) + '; path=/'; cookieCount += 1; }}\
           }});\
         }}\
         const targetUrl = snapshot.url ? String(snapshot.url) : null;\
         const shouldNavigate = {navigate} && targetUrl && targetUrl !== location.href;\
         if (shouldNavigate) location.href = targetUrl;\
         return {{ restoredLocalStorage: localCount, restoredSessionStorage: sessionCount, restoredCookies: cookieCount, navigated: !!shouldNavigate }};",
        snapshot_json = snapshot_json,
        navigate = navigate,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

fn validate_state_restore_snapshot(mut snapshot: JsonValue) -> CommandResult<JsonValue> {
    if !snapshot.is_object() {
        return Err(CommandError::user_fixable(
            "browser_state_snapshot_invalid",
            "Xero requires browser state restore snapshots to be JSON objects.",
        ));
    }

    if let Some(cookies) = snapshot.get_mut("cookies") {
        if cookies.is_null() {
            return Ok(snapshot);
        }
        let cookies_array = cookies.as_array_mut().ok_or_else(|| {
            CommandError::user_fixable(
                "browser_state_snapshot_invalid",
                "Xero requires browser state restore cookies to be an array.",
            )
        })?;

        for cookie in cookies_array {
            validate_state_restore_cookie(cookie)?;
        }
    }

    Ok(snapshot)
}

fn validate_state_restore_cookie(cookie: &mut JsonValue) -> CommandResult<()> {
    let object = cookie.as_object_mut().ok_or_else(|| {
        CommandError::user_fixable(
            "browser_state_cookie_invalid",
            "Xero requires each browser state restore cookie to be an object.",
        )
    })?;
    let name = object
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "browser_state_cookie_invalid",
                "Xero requires each browser state restore cookie to include a string name.",
            )
        })?;
    let value = object
        .get("value")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();

    if !is_valid_cookie_name(name) {
        return Err(CommandError::user_fixable(
            "browser_state_cookie_invalid",
            "Xero refused a browser state restore cookie with a malformed name.",
        ));
    }
    if !is_valid_cookie_value(value) {
        return Err(CommandError::user_fixable(
            "browser_state_cookie_invalid",
            "Xero refused a browser state restore cookie with a value that could inject cookie attributes.",
        ));
    }

    object.retain(|key, _| matches!(key.as_str(), "name" | "value"));
    Ok(())
}

fn is_valid_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(byte, 0x21 | 0x23..=0x27 | 0x2a..=0x2b | 0x2d..=0x2e | 0x30..=0x39 | 0x41..=0x5a | 0x5e..=0x7a | 0x7c | 0x7e)
        })
}

fn is_valid_cookie_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e))
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
        format!("Xero could not encode browser bridge payload: {error}"),
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

    #[test]
    fn state_restore_snapshot_rejects_cookie_attribute_injection() {
        let snapshot = serde_json::json!({
            "url": "https://example.com",
            "cookies": [
                { "name": "session; SameSite=None", "value": "abc" }
            ]
        });

        let error = validate_state_restore_snapshot(snapshot)
            .expect_err("malformed cookie names should fail closed");
        assert_eq!(error.code, "browser_state_cookie_invalid");

        let snapshot = serde_json::json!({
            "url": "https://example.com",
            "cookies": [
                { "name": "session", "value": "abc; Secure" }
            ]
        });
        let error = validate_state_restore_snapshot(snapshot)
            .expect_err("malformed cookie values should fail closed");
        assert_eq!(error.code, "browser_state_cookie_invalid");
    }

    #[test]
    fn state_restore_snapshot_preserves_valid_cookies_without_extra_attributes() {
        let snapshot = serde_json::json!({
            "url": "https://example.com",
            "cookies": [
                { "name": "session_id", "value": "abc-123", "sameSite": "None" }
            ]
        });

        let sanitized = validate_state_restore_snapshot(snapshot).expect("valid cookie snapshot");
        assert_eq!(sanitized["cookies"][0]["name"], "session_id");
        assert_eq!(sanitized["cookies"][0]["value"], "abc-123");
        assert!(sanitized["cookies"][0].get("sameSite").is_none());
    }
}
