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

pub fn hover<R: Runtime>(
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
         const rect = el.getBoundingClientRect();\
         const x = rect.left + rect.width / 2;\
         const y = rect.top + rect.height / 2;\
         if (typeof el.focus === 'function') el.focus();\
         ['mouseenter', 'mouseover', 'mousemove'].forEach((type) => el.dispatchEvent(new MouseEvent(type, {{ bubbles: true, cancelable: true, clientX: x, clientY: y }})));\
         return {{ selector: {sel}, x, y }};",
        sel = encoded,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn focus<R: Runtime>(
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
         if (typeof el.focus !== 'function') throw new Error('element cannot be focused');\
         el.focus();\
         if (document.activeElement !== el) throw new Error('page prevented focus for: {sel}');\
         return {{ selector: {sel}, focused: true }};",
        sel = encoded,
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn select_option<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    value: Option<&str>,
    label: Option<&str>,
    index: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    let selector_json = serde_json::to_string(&selector).map_err(encode_err)?;
    let value_json = optional_string_json(value)?;
    let label_json = optional_string_json(label)?;
    let index_json = index
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".into());
    let body = format!(
        "const selector = {selector_json};\
         const wantedValue = {value_json};\
         const wantedLabel = {label_json};\
         const wantedIndex = {index_json};\
         const el = document.querySelector(selector);\
         if (!el) throw new Error('element not found: ' + selector);\
         if ((el.tagName || '').toLowerCase() !== 'select') throw new Error('element is not a select');\
         const options = Array.from(el.options || []);\
         const option = wantedIndex != null ? options[wantedIndex] : options.find((item) => wantedValue != null && item.value === String(wantedValue)) || options.find((item) => wantedLabel != null && item.textContent.trim() === String(wantedLabel));\
         if (!option) throw new Error('option not found for: ' + selector);\
         el.value = option.value;\
         option.selected = true;\
         el.dispatchEvent(new Event('input', {{ bubbles: true }}));\
         el.dispatchEvent(new Event('change', {{ bubbles: true }}));\
         if (el.value !== option.value) throw new Error('page prevented option selection for: ' + selector);\
         return {{ selector, value: el.value, label: option.textContent.trim(), index: options.indexOf(option) }};",
    );
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn set_checked<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: &str,
    checked: bool,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector = validate_selector(selector)?;
    let selector_json = serde_json::to_string(&selector).map_err(encode_err)?;
    let checked_json = if checked { "true" } else { "false" };
    let body = format!(
        "const selector = {selector_json};\
         const checked = {checked_json};\
         const el = document.querySelector(selector);\
         if (!el) throw new Error('element not found: ' + selector);\
         if (typeof el.checked !== 'boolean') throw new Error('element does not expose checked state');\
         if (el.checked !== checked) {{\
           el.checked = checked;\
           el.dispatchEvent(new Event('input', {{ bubbles: true }}));\
           el.dispatchEvent(new Event('change', {{ bubbles: true }}));\
         }}\
         if (el.checked !== checked) throw new Error('page prevented checked-state update for: ' + selector);\
         return {{ selector, checked: el.checked }};",
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

pub fn page_source<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let body = "return { html: document.documentElement ? document.documentElement.outerHTML : '', url: location.href, title: document.title };".to_string();
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn snapshot<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    mode: &str,
    visible_only: bool,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let mode_json = serde_json::to_string(mode).map_err(encode_err)?;
    let limit = limit
        .unwrap_or(150)
        .clamp(1, MAX_BROWSER_DIAGNOSTIC_LIMIT * 2);
    let body = BROWSER_SNAPSHOT_SCRIPT
        .replace("__MODE__", &mode_json)
        .replace(
            "__VISIBLE_ONLY__",
            if visible_only { "true" } else { "false" },
        )
        .replace("__LIMIT__", &limit.to_string());
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn resolve_ref<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    node: &JsonValue,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let node_json = serde_json::to_string(node).map_err(encode_err)?;
    let body = BROWSER_RESOLVE_REF_SCRIPT.replace("__NODE__", &node_json);
    let result = run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))?;
    if result.get("ok").and_then(JsonValue::as_bool) == Some(true) {
        return Ok(result);
    }
    Err(CommandError::user_fixable(
        "browser_ref_stale",
        result
            .get("message")
            .and_then(JsonValue::as_str)
            .unwrap_or("Browser ref no longer resolves to the snapshotted element. Run snapshot again and use a fresh ref.")
            .to_owned(),
    ))
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

pub fn wait_for_condition<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    condition: &str,
    selector: Option<&str>,
    text: Option<&str>,
    url_contains: Option<&str>,
    title_contains: Option<&str>,
    count: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let total = resolve_timeout(timeout_ms);
    let selector_json = optional_validated_selector_json(selector)?;
    let text_json = optional_string_json(text)?;
    let url_json = optional_string_json(url_contains)?;
    let title_json = optional_string_json(title_contains)?;
    let condition_json = serde_json::to_string(condition).map_err(encode_err)?;
    let count_value = count.unwrap_or(0);
    let body = BROWSER_WAIT_FOR_SCRIPT
        .replace("__CONDITION__", &condition_json)
        .replace("__SELECTOR__", &selector_json)
        .replace("__TEXT__", &text_json)
        .replace("__URL_CONTAINS__", &url_json)
        .replace("__TITLE_CONTAINS__", &title_json)
        .replace("__COUNT__", &count_value.to_string())
        .replace("__TIMEOUT__", &total.to_string());
    run_script(app, tabs, waiters, &body, total.saturating_add(2_000))
}

pub fn assert_condition<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    assertion: &str,
    selector: Option<&str>,
    expected: Option<&str>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let assertion_json = serde_json::to_string(assertion).map_err(encode_err)?;
    let selector_json = optional_validated_selector_json(selector)?;
    let expected_json = optional_string_json(expected)?;
    let body = BROWSER_ASSERT_SCRIPT
        .replace("__ASSERTION__", &assertion_json)
        .replace("__SELECTOR__", &selector_json)
        .replace("__EXPECTED__", &expected_json);
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
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

pub fn frame_inventory<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let body = "const frames = Array.from(document.querySelectorAll('iframe, frame')).map((frame, index) => ({ index, name: frame.getAttribute('name') || null, id: frame.id || null, title: frame.getAttribute('title') || null, src: frame.getAttribute('src') || null, visible: !!(frame.offsetWidth || frame.offsetHeight || (frame.getClientRects && frame.getClientRects().length)), bounds: (() => { const r = frame.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }; })() })); return { currentFrame: 'main', frames, partialSupport: 'Tauri WebView can inventory frames from the main document; cross-origin frame DOM actions require native CDP.' };".to_string();
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn find_best<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    intent: &str,
    text: Option<&str>,
    role: Option<&str>,
    cached_selectors: &[String],
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    if intent.trim().is_empty() {
        return Err(CommandError::invalid_request("intent"));
    }
    let intent_json = serde_json::to_string(intent).map_err(encode_err)?;
    let text_json = optional_string_json(text)?;
    let role_json = optional_string_json(role)?;
    let cached_json = serde_json::to_string(cached_selectors).map_err(encode_err)?;
    let body = BROWSER_FIND_BEST_SCRIPT
        .replace("__INTENT__", &intent_json)
        .replace("__TEXT__", &text_json)
        .replace("__ROLE__", &role_json)
        .replace("__CACHED_SELECTORS__", &cached_json);
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn analyze_form<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector_json = optional_validated_selector_json(selector)?;
    let body = BROWSER_ANALYZE_FORM_SCRIPT.replace("__SELECTOR__", &selector_json);
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn fill_form<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    selector: Option<&str>,
    fields: &std::collections::BTreeMap<String, String>,
    submit: bool,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    if fields.is_empty() {
        return Err(CommandError::invalid_request("fields"));
    }
    for value in fields.values() {
        validate_text_input(value)?;
    }
    let selector_json = optional_validated_selector_json(selector)?;
    let fields_json = serde_json::to_string(fields).map_err(encode_err)?;
    let body = BROWSER_FILL_FORM_SCRIPT
        .replace("__SELECTOR__", &selector_json)
        .replace("__FIELDS__", &fields_json)
        .replace("__SUBMIT__", if submit { "true" } else { "false" });
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

pub fn prompt_injection_scan<R: Runtime>(
    app: &AppHandle<R>,
    tabs: &Arc<BrowserTabs>,
    waiters: &Arc<BridgeWaiters>,
    include_hidden: bool,
    selector: Option<&str>,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> CommandResult<JsonValue> {
    let selector_json = optional_validated_selector_json(selector)?;
    let limit = limit.unwrap_or(80).clamp(1, MAX_BROWSER_DIAGNOSTIC_LIMIT);
    let body = BROWSER_PROMPT_INJECTION_SCAN_SCRIPT
        .replace("__SELECTOR__", &selector_json)
        .replace(
            "__INCLUDE_HIDDEN__",
            if include_hidden { "true" } else { "false" },
        )
        .replace("__LIMIT__", &limit.to_string());
    run_script(app, tabs, waiters, &body, resolve_timeout(timeout_ms))
}

fn optional_validated_selector_json(selector: Option<&str>) -> CommandResult<String> {
    match selector {
        Some(selector) => {
            let selector = validate_selector(selector)?;
            serde_json::to_string(&selector).map_err(encode_err)
        }
        None => Ok("null".into()),
    }
}

fn optional_string_json(value: Option<&str>) -> CommandResult<String> {
    match value {
        Some(value) => serde_json::to_string(value).map_err(encode_err),
        None => Ok("null".into()),
    }
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

const BROWSER_RESOLVE_REF_SCRIPT: &str = r#"
const node = __NODE__;
const norm = (value, max = 500) => String(value == null ? '' : value).trim().replace(/\s+/g, ' ').slice(0, max);
const attr = (el, name) => el && el.getAttribute ? el.getAttribute(name) : null;
const escapeCss = (value) => {
  if (window.CSS && typeof window.CSS.escape === 'function') return window.CSS.escape(String(value));
  return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => '\\' + ch);
};
const implicitRole = (el) => {
  const tag = (el && el.tagName || '').toLowerCase();
  if (tag === 'a' && el.hasAttribute('href')) return 'link';
  if (tag === 'button' || tag === 'summary') return 'button';
  if (tag === 'input') {
    const type = (el.type || 'text').toLowerCase();
    if (type === 'checkbox') return 'checkbox';
    if (type === 'radio') return 'radio';
    if (['button', 'submit', 'reset'].includes(type)) return 'button';
    return 'textbox';
  }
  if (tag === 'textarea') return 'textbox';
  if (tag === 'select') return 'combobox';
  if (/^h[1-6]$/.test(tag)) return 'heading';
  if (tag === 'nav') return 'navigation';
  if (tag === 'main') return 'main';
  if (tag === 'form') return 'form';
  if (tag === 'dialog') return 'dialog';
  return null;
};
const textOf = (el) => norm((el && (el.innerText || el.textContent)) || '', 500);
const nameOf = (el) => {
  const labelledBy = attr(el, 'aria-labelledby');
  if (labelledBy) {
    const label = labelledBy.split(/\s+/).map((id) => document.getElementById(id)).filter(Boolean).map(textOf).join(' ').trim();
    if (label) return norm(label, 300);
  }
  const id = attr(el, 'id');
  if (id) {
    const label = document.querySelector(`label[for="${escapeCss(id)}"]`);
    if (label && textOf(label)) return norm(textOf(label), 300);
  }
  return norm(attr(el, 'aria-label') || attr(el, 'alt') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el), 300);
};
const stableDataAttributes = (el) => {
  const out = {};
  if (!el || !el.getAttributeNames) return out;
  for (const name of el.getAttributeNames()) {
    if (/^(data-testid|data-test|data-cy|data-xero-ref|id|name|aria-label)$/.test(name)) {
      const value = attr(el, name);
      if (value) out[name] = value;
    }
  }
  return out;
};
const fingerprint = (el) => {
  const tag = (el && el.tagName || '').toLowerCase();
  const rect = el.getBoundingClientRect ? el.getBoundingClientRect() : { x: 0, y: 0, width: 0, height: 0 };
  return {
    tag,
    role: attr(el, 'role') || implicitRole(el),
    name: nameOf(el),
    text: textOf(el),
    value: typeof el.value === 'string' ? norm(el.value, 300) : null,
    checked: typeof el.checked === 'boolean' ? Boolean(el.checked) : null,
    href: attr(el, 'href'),
    stableDataAttributes: stableDataAttributes(el),
    visible: !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length))),
    bounds: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
  };
};
const candidateMeta = () => {
  if (Array.isArray(node.selectorMeta)) return node.selectorMeta;
  if (Array.isArray(node.selectorCandidates)) {
    return node.selectorCandidates.map((selector) => ({ selector, unique: false, roleOnly: /^\[role=/.test(String(selector || '')) }));
  }
  return [];
};
const mismatchesFor = (el) => {
  const current = fingerprint(el);
  const mismatches = [];
  const expectedStable = node.stableDataAttributes && typeof node.stableDataAttributes === 'object' ? node.stableDataAttributes : {};
  if (node.frame && node.frame.url && node.frame.url !== location.href) mismatches.push('page_url');
  if (node.tag && current.tag !== node.tag) mismatches.push('tag');
  if (node.role && current.role !== node.role) mismatches.push('role');
  if (node.name && norm(current.name, 300) !== norm(node.name, 300)) mismatches.push('name');
  if (node.text && norm(current.text, 180) !== norm(node.text, 180)) mismatches.push('text');
  if (node.href && current.href !== node.href) mismatches.push('href');
  if (node.value != null && norm(current.value, 300) !== norm(node.value, 300)) mismatches.push('value');
  if (node.checked != null && current.checked !== node.checked) mismatches.push('checked');
  for (const [key, value] of Object.entries(expectedStable)) {
    if (attr(el, key) !== value) mismatches.push(`stable_attr:${key}`);
  }
  if (!current.visible && node.visible) mismatches.push('visibility');
  return { current, mismatches };
};
const registry = window.__xeroRefRegistry__;
const registryId = node.engineNodeId || node.nodeId || null;
if (registry && registryId && registry.nodes && !registry.invalidated?.has?.(registryId)) {
  const el = registry.nodes.get(registryId);
  if (el && document.contains(el)) {
    const verified = mismatchesFor(el);
    if (verified.mismatches.length === 0) {
      return { ok: true, selector: node.primarySelector || null, strategy: 'registry', engineNodeId: registryId, fingerprint: verified.current };
    }
  }
}
const tried = [];
for (const meta of candidateMeta()) {
  const selector = String(meta.selector || '').trim();
  if (!selector) continue;
  let matches = [];
  try { matches = Array.from(document.querySelectorAll(selector)); } catch (_error) { continue; }
  const uniqueNow = matches.length === 1;
  tried.push({ selector, count: matches.length, snapshotUnique: Boolean(meta.unique), roleOnly: Boolean(meta.roleOnly) });
  if (!uniqueNow) continue;
  const verified = mismatchesFor(matches[0]);
  if (verified.mismatches.length === 0) {
    return { ok: true, selector, strategy: 'selector', selectorUnique: true, fingerprint: verified.current };
  }
}
return {
  ok: false,
  code: 'browser_ref_stale',
  message: 'Browser ref no longer resolves to the snapshotted element. Run snapshot again and use a fresh ref.',
  ref: node.ref || null,
  tried,
  currentUrl: location.href,
  snapshotUrl: node.frame && node.frame.url || null,
  mutationGeneration: registry && registry.mutationGeneration || null,
};
"#;

const BROWSER_SNAPSHOT_SCRIPT: &str = r#"
const mode = __MODE__;
const visibleOnly = __VISIBLE_ONLY__;
const limit = __LIMIT__;
const registry = (() => {
  const existing = window.__xeroRefRegistry__;
  if (existing && existing.__version === 1) return existing;
  const next = {
    __version: 1,
    nextId: 1,
    elementIds: new WeakMap(),
    nodes: new Map(),
    invalidated: new Set(),
    mutationGeneration: 0,
    navigationGeneration: (existing && existing.navigationGeneration) || 1,
  };
  next.idFor = (el) => {
    let id = next.elementIds.get(el);
    if (!id) {
      id = `inapp-node-${next.nextId++}`;
      next.elementIds.set(el, id);
    }
    next.nodes.set(id, el);
    return id;
  };
  try {
    const observer = new MutationObserver((mutations) => {
      next.mutationGeneration += 1;
      for (const mutation of mutations) {
        const invalidate = (node) => {
          if (!node || node.nodeType !== 1) return;
          const id = next.elementIds.get(node);
          if (id) next.invalidated.add(id);
          if (node.querySelectorAll) {
            for (const child of node.querySelectorAll('*')) {
              const childId = next.elementIds.get(child);
              if (childId) next.invalidated.add(childId);
            }
          }
        };
        invalidate(mutation.target);
        for (const removed of mutation.removedNodes || []) invalidate(removed);
      }
    });
    observer.observe(document.documentElement || document, { subtree: true, childList: true, attributes: true, characterData: true });
    next.observer = observer;
  } catch (_error) {}
  Object.defineProperty(window, '__xeroRefRegistry__', { value: next, writable: false, configurable: false, enumerable: false });
  return next;
})();
const escapeCss = (value) => {
  if (window.CSS && typeof window.CSS.escape === 'function') return window.CSS.escape(String(value));
  return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => '\\' + ch);
};
const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 500);
const attr = (el, name) => el.getAttribute && el.getAttribute(name);
const implicitRole = (el) => {
  const tag = (el.tagName || '').toLowerCase();
  if (tag === 'a' && el.hasAttribute('href')) return 'link';
  if (tag === 'button') return 'button';
  if (tag === 'summary') return 'button';
  if (tag === 'input') {
    if (el.type === 'checkbox') return 'checkbox';
    if (el.type === 'radio') return 'radio';
    if (['button', 'submit', 'reset'].includes(el.type)) return 'button';
    return 'textbox';
  }
  if (tag === 'textarea') return 'textbox';
  if (tag === 'select') return 'combobox';
  if (/^h[1-6]$/.test(tag)) return 'heading';
  if (tag === 'nav') return 'navigation';
  if (tag === 'main') return 'main';
  if (tag === 'form') return 'form';
  if (tag === 'dialog') return 'dialog';
  return null;
};
const nameOf = (el) => {
  const labelledBy = attr(el, 'aria-labelledby');
  if (labelledBy) {
    const label = labelledBy.split(/\s+/).map((id) => document.getElementById(id)).filter(Boolean).map(textOf).join(' ').trim();
    if (label) return label.slice(0, 300);
  }
  const id = attr(el, 'id');
  if (id) {
    const label = document.querySelector(`label[for="${escapeCss(id)}"]`);
    if (label && textOf(label)) return textOf(label).slice(0, 300);
  }
  return (attr(el, 'aria-label') || attr(el, 'alt') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el)).slice(0, 300);
};
const isVisible = (el) => {
  if (!el || el.nodeType !== 1) return false;
  const style = window.getComputedStyle ? window.getComputedStyle(el) : null;
  if (style && (style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0)) return false;
  return !!(el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length));
};
const isEnabled = (el) => !(el.disabled || attr(el, 'aria-disabled') === 'true');
const isEditable = (el) => {
  const tag = (el.tagName || '').toLowerCase();
  return el.isContentEditable || tag === 'textarea' || tag === 'select' || (tag === 'input' && !['button', 'submit', 'reset', 'hidden', 'image'].includes(el.type || 'text'));
};
const isInteractive = (el, role) => {
  const tag = (el.tagName || '').toLowerCase();
  return isEditable(el) || ['button', 'summary', 'select', 'textarea'].includes(tag) || (tag === 'a' && el.hasAttribute('href')) || ['button', 'link', 'checkbox', 'radio', 'textbox', 'combobox', 'menuitem', 'tab', 'switch', 'slider', 'searchbox'].includes(role || '') || typeof el.onclick === 'function' || el.tabIndex >= 0;
};
const nthSelector = (el) => {
  const parts = [];
  let node = el;
  while (node && node.nodeType === 1 && node !== document.body && parts.length < 5) {
    const tag = (node.tagName || '').toLowerCase();
    let index = 1;
    let sibling = node;
    while ((sibling = sibling.previousElementSibling)) {
      if ((sibling.tagName || '').toLowerCase() === tag) index += 1;
    }
    parts.unshift(`${tag}:nth-of-type(${index})`);
    node = node.parentElement;
  }
  return parts.length ? parts.join(' > ') : null;
};
const structuralPath = (el) => {
  const parts = [];
  let node = el;
  while (node && node.nodeType === 1 && node !== document && parts.length < 8) {
    const tag = (node.tagName || '').toLowerCase();
    let index = 1;
    let sibling = node;
    while ((sibling = sibling.previousElementSibling)) {
      if ((sibling.tagName || '').toLowerCase() === tag) index += 1;
    }
    parts.unshift(`${tag}:${index}`);
    node = node.parentElement;
  }
  return parts.join('/');
};
const stableDataAttributes = (el) => {
  const out = {};
  if (!el.getAttributeNames) return out;
  for (const name of el.getAttributeNames()) {
    if (/^(data-testid|data-test|data-cy|data-xero-ref|id|name|aria-label)$/.test(name)) {
      const value = attr(el, name);
      if (value) out[name] = value;
    }
  }
  return out;
};
const selectorCount = (selector) => {
  try { return document.querySelectorAll(selector).length; } catch (_error) { return 0; }
};
const selectorCandidates = (el, role, name) => {
  const tag = (el.tagName || '').toLowerCase();
  const out = [];
  const add = (selector, stability, roleOnly = false) => {
    if (!selector) return;
    const count = selectorCount(selector);
    out.push({ selector, unique: count === 1, count, stability, roleOnly });
  };
  if (el.id) add(`#${escapeCss(el.id)}`, 'id');
  ['data-testid', 'data-test', 'data-cy', 'name', 'aria-label'].forEach((key) => {
    const value = attr(el, key);
    if (value) add(`${tag}[${key}="${String(value).replace(/"/g, '\\"')}"]`, key);
  });
  if (role && attr(el, 'role')) add(`[role="${String(role).replace(/"/g, '\\"')}"]`, 'role', true);
  if (role && name) add(`[role="${String(role).replace(/"/g, '\\"')}"][aria-label="${String(name).replace(/"/g, '\\"')}"]`, 'role_name');
  const path = nthSelector(el);
  if (path) add(path, 'structural');
  const seen = new Set();
  return out
    .filter((item) => {
      if (seen.has(item.selector)) return false;
      seen.add(item.selector);
      return true;
    })
    .sort((a, b) => Number(b.unique) - Number(a.unique) || Number(a.roleOnly) - Number(b.roleOnly))
    .slice(0, 8);
};
const includeForMode = (el, role, name, text, visible) => {
  const tag = (el.tagName || '').toLowerCase();
  if (visibleOnly && !visible) return false;
  if (mode === 'interactive') return isInteractive(el, role);
  if (mode === 'form') return ['input', 'textarea', 'select', 'button', 'form', 'label'].includes(tag) || ['textbox', 'checkbox', 'radio', 'combobox', 'button', 'form'].includes(role || '');
  if (mode === 'dialog') return role === 'dialog' || tag === 'dialog' || attr(el, 'aria-modal') === 'true' || isInteractive(el, role);
  if (mode === 'navigation') return role === 'navigation' || tag === 'nav' || tag === 'a' || role === 'link' || ['button', 'tab'].includes(role || '');
  if (mode === 'errors') return attr(el, 'aria-invalid') === 'true' || attr(el, 'role') === 'alert' || /error|required|invalid|failed/i.test(`${name} ${text}`);
  if (mode === 'headings') return role === 'heading' || /^h[1-6]$/.test(tag);
  return isInteractive(el, role) || role || /^h[1-6]$/.test(tag);
};
const refs = [];
const all = Array.from(document.querySelectorAll('body, body *'));
for (const el of all) {
  if (refs.length >= limit) break;
  if (!el || el.nodeType !== 1) continue;
  const role = attr(el, 'role') || implicitRole(el);
  const visible = isVisible(el);
  const text = textOf(el);
  const name = nameOf(el);
  if (!includeForMode(el, role, name, text, visible)) continue;
  const rect = el.getBoundingClientRect();
  const selectorMeta = selectorCandidates(el, role, name);
  const engineNodeId = registry.idFor(el);
  refs.push({
    tag: (el.tagName || '').toLowerCase(),
    role,
    name: name || null,
    text: text || null,
    visible,
    enabled: isEnabled(el),
    editable: isEditable(el),
    checked: typeof el.checked === 'boolean' ? Boolean(el.checked) : null,
    value: isEditable(el) && typeof el.value === 'string' ? el.value.slice(0, 300) : null,
    href: attr(el, 'href'),
    form: { action: attr(el, 'action'), method: attr(el, 'method'), name: attr(el, 'name') },
    structuralPath: structuralPath(el),
    stableDataAttributes: stableDataAttributes(el),
    selectorCandidates: selectorMeta.map((item) => item.selector),
    selectorMeta,
    primarySelector: selectorMeta.find((item) => item.unique && !item.roleOnly)?.selector || selectorMeta.find((item) => item.unique)?.selector || null,
    engineNodeId,
    mutationGeneration: registry.mutationGeneration,
    navigationGeneration: registry.navigationGeneration,
    bounds: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
    frame: { id: 'main', url: location.href },
  });
}
return {
  url: location.href,
  title: document.title,
  readyState: document.readyState,
  mode,
  visibleOnly,
  refs,
  totalCandidates: all.length,
  truncated: refs.length >= limit,
  bridge: {
    protocol: 'xero.in_app_browser_bridge.v1',
    refRegistryVersion: registry.__version,
    mutationGeneration: registry.mutationGeneration,
    navigationGeneration: registry.navigationGeneration,
  },
};
"#;

const BROWSER_WAIT_FOR_SCRIPT: &str = r#"
const condition = __CONDITION__;
const selector = __SELECTOR__;
const text = __TEXT__;
const urlContains = __URL_CONTAINS__;
const titleContains = __TITLE_CONTAINS__;
const expectedCount = __COUNT__;
const timeout = __TIMEOUT__;
const deadline = Date.now() + timeout;
const isVisible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
const pageText = () => (document.body && (document.body.innerText || document.body.textContent) || '').trim();
const bridgeState = () => window.__xeroBridgeState__ || {};
const rectFor = (el) => {
  const rect = (el || document.documentElement).getBoundingClientRect();
  return { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) };
};
let stableRegion = null;
let stableMutationGeneration = bridgeState().mutationGeneration || 0;
let stableSince = Date.now();
const check = () => {
  if (condition === 'load') return document.readyState === 'complete' ? { ok: true, detail: { readyState: document.readyState } } : { ok: false, detail: { readyState: document.readyState } };
  if (condition === 'selector_visible') { const el = document.querySelector(selector || ''); return { ok: !!(el && isVisible(el)), detail: { selector, found: !!el, visible: !!(el && isVisible(el)) } }; }
  if (condition === 'selector_hidden') { const el = document.querySelector(selector || ''); return { ok: !el || !isVisible(el), detail: { selector, found: !!el, visible: !!(el && isVisible(el)) } }; }
  if (condition === 'text_visible') { const haystack = pageText(); return { ok: !!(text && haystack.includes(text)), detail: { text, matched: !!(text && haystack.includes(text)) } }; }
  if (condition === 'text_hidden') { const haystack = pageText(); return { ok: !(text && haystack.includes(text)), detail: { text, matched: !!(text && haystack.includes(text)) } }; }
  if (condition === 'url_contains') return { ok: !!(urlContains && location.href.includes(urlContains)), detail: { url: location.href, urlContains } };
  if (condition === 'title_contains') return { ok: !!(titleContains && document.title.includes(titleContains)), detail: { title: document.title, titleContains } };
  if (condition === 'element_count') { const actual = document.querySelectorAll(selector || '').length; return { ok: actual === expectedCount, detail: { selector, expectedCount, actual } }; }
  if (condition === 'element_count_at_least') { const actual = document.querySelectorAll(selector || '').length; return { ok: actual >= expectedCount, detail: { selector, expectedCount, actual } }; }
  if (condition === 'region_stable') {
    const state = bridgeState();
    const target = selector ? document.querySelector(selector) : document.documentElement;
    if (!target) return { ok: false, detail: { selector, found: false } };
    const currentRect = rectFor(target);
    const mutationGeneration = state.mutationGeneration || 0;
    const changed = !stableRegion ||
      stableRegion.x !== currentRect.x ||
      stableRegion.y !== currentRect.y ||
      stableRegion.width !== currentRect.width ||
      stableRegion.height !== currentRect.height ||
      stableMutationGeneration !== mutationGeneration;
    if (changed) {
      stableRegion = currentRect;
      stableMutationGeneration = mutationGeneration;
      stableSince = Date.now();
    }
    const quietMs = Date.now() - stableSince;
    return {
      ok: quietMs >= 500,
      detail: {
        selector,
        bounds: currentRect,
        quietMs,
        mutationGeneration,
        limitation: 'In-app region stability observes DOM mutations and bounding boxes in the WebView main document.',
      },
    };
  }
  if (condition === 'network_idle') {
    const state = bridgeState();
    const inflight = Number(state.inFlightFetch || 0) + Number(state.inFlightXhr || 0);
    const lastFinished = Number(state.lastNetworkFinishedAt || 0);
    const quietMs = lastFinished ? Date.now() - lastFinished : timeout;
    return {
      ok: inflight === 0 && quietMs >= 500,
      detail: {
        inflight,
        quietMs,
        limitation: 'In-app network idle observes fetch/XHR instrumented by the WebView bridge. Parser, image, stylesheet, and browser-internal requests may be invisible without native CDP.',
      },
    };
  }
  return { ok: false, detail: { unsupportedCondition: condition } };
};
let last = null;
while (Date.now() < deadline) {
  last = check();
  if (last.ok) return { condition, waitedMs: timeout - (deadline - Date.now()), detail: last.detail };
  await new Promise((resolve) => setTimeout(resolve, 80));
}
throw new Error('browser wait_for failed for ' + condition + ': ' + JSON.stringify(last && last.detail));
"#;

const BROWSER_ASSERT_SCRIPT: &str = r#"
const assertion = __ASSERTION__;
const selector = __SELECTOR__;
const expected = __EXPECTED__;
const isVisible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
const pageText = () => (document.body && (document.body.innerText || document.body.textContent) || '').trim();
const selected = () => selector ? document.querySelector(selector) : null;
const result = (() => {
  if (assertion === 'url') return { pass: expected != null && location.href === expected, actual: location.href, expected };
  if (assertion === 'url_contains') return { pass: expected != null && location.href.includes(expected), actual: location.href, expected };
  if (assertion === 'title') return { pass: expected != null && document.title === expected, actual: document.title, expected };
  if (assertion === 'title_contains') return { pass: expected != null && document.title.includes(expected), actual: document.title, expected };
  if (assertion === 'text') return { pass: expected != null && pageText().includes(expected), actual: pageText().slice(0, 1000), expected };
  if (assertion === 'selector') { const el = selected(); return { pass: !!el, actual: !!el, expected: true, selector }; }
  if (assertion === 'selector_visible') { const el = selected(); return { pass: !!(el && isVisible(el)), actual: { found: !!el, visible: !!(el && isVisible(el)) }, expected: true, selector }; }
  if (assertion === 'value') { const el = selected(); return { pass: !!el && String(el.value || '') === String(expected || ''), actual: el ? String(el.value || '') : null, expected, selector }; }
  if (assertion === 'checked') { const el = selected(); const expectedBool = expected === true || expected === 'true'; return { pass: !!el && Boolean(el.checked) === expectedBool, actual: el ? Boolean(el.checked) : null, expected: expectedBool, selector }; }
  if (assertion === 'element_count') { const actual = document.querySelectorAll(selector || '').length; const expectedNumber = Number(expected); return { pass: actual === expectedNumber, actual, expected: expectedNumber, selector }; }
  return { pass: false, actual: null, expected, unsupportedAssertion: assertion };
})();
if (!result.pass) throw new Error('browser assertion failed for ' + assertion + ': ' + JSON.stringify(result));
return Object.assign({ assertion }, result);
"#;

const BROWSER_FIND_BEST_SCRIPT: &str = r#"
const intent = __INTENT__;
const requestedText = __TEXT__;
const requestedRole = __ROLE__;
const cachedSelectors = __CACHED_SELECTORS__;
const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 500);
const attr = (el, name) => el.getAttribute && el.getAttribute(name);
const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
const roleOf = (el) => attr(el, 'role') || (((el.tagName || '').toLowerCase() === 'button' || ['submit','button'].includes(el.type || '')) ? 'button' : ((el.tagName || '').toLowerCase() === 'a' && el.hasAttribute('href') ? 'link' : ((el.tagName || '').toLowerCase() === 'input' ? 'textbox' : null)));
const nameOf = (el) => (attr(el, 'aria-label') || attr(el, 'title') || attr(el, 'placeholder') || attr(el, 'name') || textOf(el)).slice(0, 300);
const selectorFor = (el) => {
  if (el.id) return '#' + (window.CSS && CSS.escape ? CSS.escape(el.id) : el.id);
  for (const key of ['data-testid', 'data-test', 'data-cy', 'name', 'aria-label']) {
    const value = attr(el, key);
    if (value) return `${(el.tagName || '').toLowerCase()}[${key}="${String(value).replace(/"/g, '\\"')}"]`;
  }
  return null;
};
for (const selector of cachedSelectors || []) {
  try {
    const el = document.querySelector(selector);
    if (el && visible(el)) return { cacheHit: true, confidence: 92, intent, node: { tag: (el.tagName || '').toLowerCase(), role: roleOf(el), name: nameOf(el), text: textOf(el), selectorCandidates: [selector] } };
  } catch (_) {}
}
const terms = [intent, requestedText].filter(Boolean).join(' ').toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
const candidates = Array.from(document.querySelectorAll('button, a[href], input, textarea, select, [role], [tabindex], summary')).filter(visible);
let best = null;
for (const el of candidates) {
  const role = roleOf(el);
  const name = nameOf(el);
  const haystack = `${role || ''} ${name} ${textOf(el)} ${(el.tagName || '').toLowerCase()} ${attr(el, 'type') || ''}`.toLowerCase();
  let score = 0;
  if (requestedRole && role === requestedRole) score += 35;
  for (const term of terms) {
    if (haystack.includes(term)) score += 12;
  }
  if (/submit|continue|next|primary|login|sign in|search|accept|close|dismiss/.test(haystack)) score += 8;
  if (!el.disabled && attr(el, 'aria-disabled') !== 'true') score += 5;
  const selector = selectorFor(el);
  if (selector) score += 3;
  if (!best || score > best.score) best = { el, score, selector };
}
if (!best || best.score <= 0) throw new Error('browser find_best could not identify a target for intent: ' + intent);
const selector = best.selector;
return {
  cacheHit: false,
  confidence: Math.max(1, Math.min(99, best.score)),
  intent,
  node: {
    tag: (best.el.tagName || '').toLowerCase(),
    role: roleOf(best.el),
    name: nameOf(best.el),
    text: textOf(best.el),
    visible: visible(best.el),
    enabled: !(best.el.disabled || attr(best.el, 'aria-disabled') === 'true'),
    selectorCandidates: selector ? [selector] : [],
  },
  fallbackExplanation: selector ? null : 'The best element did not have a stable selector candidate; re-run snapshot for ref-based action.',
};
"#;

const BROWSER_ANALYZE_FORM_SCRIPT: &str = r#"
const selector = __SELECTOR__;
const root = selector ? document.querySelector(selector) : document;
if (!root) throw new Error('form root not found: ' + selector);
const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ').slice(0, 300);
const labelFor = (field) => {
  if (field.id) {
    const label = root.querySelector(`label[for="${field.id}"]`) || document.querySelector(`label[for="${field.id}"]`);
    if (label && textOf(label)) return textOf(label);
  }
  const parentLabel = field.closest && field.closest('label');
  if (parentLabel && textOf(parentLabel)) return textOf(parentLabel);
  return field.getAttribute('aria-label') || field.getAttribute('placeholder') || field.getAttribute('name') || field.id || '';
};
const forms = Array.from(root.querySelectorAll ? root.querySelectorAll('form') : []).concat(root.tagName === 'FORM' ? [root] : []);
const scanRoot = forms.length ? forms : [root];
return {
  forms: scanRoot.map((form, index) => ({
    index,
    name: form.getAttribute && (form.getAttribute('name') || form.getAttribute('aria-label')) || null,
    action: form.getAttribute && form.getAttribute('action') || null,
    method: form.getAttribute && form.getAttribute('method') || null,
    fields: Array.from(form.querySelectorAll('input, textarea, select')).map((field) => ({
      tag: (field.tagName || '').toLowerCase(),
      type: field.getAttribute('type') || null,
      name: field.getAttribute('name') || null,
      id: field.id || null,
      label: labelFor(field),
      required: !!field.required || field.getAttribute('aria-required') === 'true',
      valuePresent: !!field.value,
      disabled: !!field.disabled,
    })),
    submitCandidates: Array.from(form.querySelectorAll('button, input[type="submit"], [role="button"]')).map((button) => ({
      tag: (button.tagName || '').toLowerCase(),
      type: button.getAttribute('type') || null,
      label: textOf(button) || button.value || button.getAttribute('aria-label') || null,
      disabled: !!button.disabled,
    })),
  })),
};
"#;

const BROWSER_FILL_FORM_SCRIPT: &str = r#"
const selector = __SELECTOR__;
const fields = __FIELDS__;
const submit = __SUBMIT__;
const root = selector ? document.querySelector(selector) : document;
if (!root) throw new Error('form root not found: ' + selector);
const normalize = (value) => String(value || '').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim();
const textOf = (el) => ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, ' ');
const labelFor = (field) => {
  if (field.id) {
    const label = root.querySelector(`label[for="${field.id}"]`) || document.querySelector(`label[for="${field.id}"]`);
    if (label && textOf(label)) return textOf(label);
  }
  const parentLabel = field.closest && field.closest('label');
  if (parentLabel && textOf(parentLabel)) return textOf(parentLabel);
  return field.getAttribute('aria-label') || field.getAttribute('placeholder') || field.getAttribute('name') || field.id || '';
};
const setField = (field, value) => {
  const tag = (field.tagName || '').toLowerCase();
  const type = (field.getAttribute('type') || '').toLowerCase();
  if (type === 'checkbox' || type === 'radio') field.checked = ['true', '1', 'yes', 'on', 'checked'].includes(String(value).toLowerCase());
  else if (tag === 'select') field.value = String(value);
  else field.value = String(value);
  field.dispatchEvent(new Event('input', { bubbles: true }));
  field.dispatchEvent(new Event('change', { bubbles: true }));
};
const candidates = Array.from(root.querySelectorAll('input, textarea, select')).filter((field) => !field.disabled);
const matched = [];
const unmatched = [];
for (const [label, value] of Object.entries(fields)) {
  const wanted = normalize(label);
  const field = candidates.find((candidate) => {
    const haystack = normalize([labelFor(candidate), candidate.name, candidate.id, candidate.getAttribute('placeholder'), candidate.getAttribute('aria-label'), candidate.type].filter(Boolean).join(' '));
    return haystack === wanted || haystack.includes(wanted) || wanted.includes(haystack);
  });
  if (!field) {
    unmatched.push(label);
    continue;
  }
  setField(field, value);
  matched.push({ label, field: labelFor(field), name: field.name || null, id: field.id || null });
}
let submitted = false;
if (submit) {
  const form = root.tagName === 'FORM' ? root : (root.querySelector('form') || candidates[0]?.form);
  const button = form && Array.from(form.querySelectorAll('button, input[type="submit"], [role="button"]')).find((el) => !el.disabled);
  if (button) { button.click(); submitted = true; }
  else if (form && typeof form.requestSubmit === 'function') { form.requestSubmit(); submitted = true; }
}
return { matched, unmatched, submitted };
"#;

const BROWSER_PROMPT_INJECTION_SCAN_SCRIPT: &str = r#"
const selector = __SELECTOR__;
const includeHidden = __INCLUDE_HIDDEN__;
const limit = __LIMIT__;
const root = selector ? document.querySelector(selector) : document.body;
if (!root) throw new Error('scan root not found: ' + selector);
const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || (el.getClientRects && el.getClientRects().length)));
const patterns = [
  { id: 'ignore_previous_instructions', pattern: /ignore (all )?(previous|prior|above) instructions/i },
  { id: 'system_prompt_request', pattern: /(system|developer) (prompt|message|instructions)/i },
  { id: 'tool_exfiltration', pattern: /(send|exfiltrate|post|upload).{0,80}(token|secret|cookie|password|api key)/i },
  { id: 'hidden_agent_instruction', pattern: /(assistant|agent|model).{0,80}(must|should|will).{0,80}(click|type|download|submit|send)/i },
  { id: 'credential_request', pattern: /(enter|share|paste).{0,80}(password|token|secret|api key|cookie)/i }
];
const findings = [];
const scanText = (source, text, hidden, node) => {
  if (!text || findings.length >= limit) return;
  for (const item of patterns) {
    const match = String(text).match(item.pattern);
    if (match) {
      findings.push({
        id: item.id,
        source,
        hidden,
        snippet: String(text).replace(/\s+/g, ' ').slice(Math.max(0, match.index - 40), Math.min(String(text).length, match.index + 160)),
        tag: node && node.tagName ? node.tagName.toLowerCase() : null,
      });
      break;
    }
  }
};
const nodes = Array.from(root.querySelectorAll ? root.querySelectorAll('*') : []);
for (const node of [root].concat(nodes)) {
  if (findings.length >= limit) break;
  const hidden = !visible(node);
  if (hidden && !includeHidden) continue;
  scanText('text', node.innerText || node.textContent || '', hidden, node);
  if (node.getAttributeNames) {
    for (const name of node.getAttributeNames()) scanText(`attribute:${name}`, node.getAttribute(name), hidden, node);
  }
}
return {
  scannedNodes: nodes.length + 1,
  includeHidden,
  findings,
  risk: findings.length ? 'suspicious' : 'none_detected',
};
"#;

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
