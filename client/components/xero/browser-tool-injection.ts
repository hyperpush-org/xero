"use client"

export type BrowserToolMode = "pen" | "inspect"

export interface BrowserToolTheme {
  background: string
  foreground: string
  card: string
  cardForeground: string
  popover: string
  popoverForeground: string
  primary: string
  primaryForeground: string
  secondary: string
  secondaryForeground: string
  muted: string
  mutedForeground: string
  accent: string
  accentForeground: string
  destructive: string
  destructiveForeground: string
  border: string
  input: string
  ring: string
}

export const BROWSER_TOOL_CONTEXT_EVENT = "browser:tool_context"
export const BROWSER_TOOL_CLOSED_EVENT = "browser:tool_closed"
export const BROWSER_TOOL_STATE_EVENT = "browser:tool_state"
export const BROWSER_TOOL_NOTE_EVENT = "browser:tool_note"
export const BROWSER_TOOL_DICTATION_TOGGLE_EVENT = "browser:tool_dictation_toggle"

export interface BrowserToolPageContext {
  url: string
  title: string | null
}

export interface BrowserToolRectContext {
  x: number
  y: number
  width: number
  height: number
}

export interface BrowserToolScrollContext {
  x: number
  y: number
}

export interface BrowserToolViewportContext {
  width: number
  height: number
  devicePixelRatio?: number | null
}

export interface BrowserToolElementContext {
  selector: string | null
  tagName: string
  id: string | null
  classes: string[]
  role: string | null
  label: string | null
  text: string | null
  attributes?: Array<{ name: string; value: string }>
  ancestors?: Array<{
    selector: string | null
    tagName: string
    id: string | null
    role: string | null
    label: string | null
  }>
  source?: {
    framework: string | null
    componentName: string | null
    filePath: string | null
    lineNumber: number | null
    columnNumber: number | null
    raw: string | null
  } | null
  rect: BrowserToolRectContext
}

export type BrowserToolContext =
  | {
      kind: "pen"
      note: string
      page: BrowserToolPageContext
      strokeCount: number
      viewport: BrowserToolViewportContext
      scroll?: BrowserToolScrollContext | null
      annotationBounds?: BrowserToolRectContext | null
    }
  | {
      kind: "inspect"
      note: string
      page: BrowserToolPageContext
      element: BrowserToolElementContext
      viewport: BrowserToolViewportContext
      scroll?: BrowserToolScrollContext | null
    }

export interface BrowserToolPromptMetadata {
  appLabel?: string | null
  attachmentName?: string | null
  captureIndex?: number | null
}

export interface BrowserToolContextEventPayload {
  tabId: string
  context: BrowserToolContext
}

export interface BrowserToolClosedEventPayload {
  tabId: string
  mode: BrowserToolMode | null
}

export interface BrowserAgentContextRequest {
  prompt: string
  visiblePrompt: string
  contextCard?: {
    kind: "element" | "sketch"
    title: string
    subtitle?: string
  }
  image?: {
    bytes: Uint8Array
    mediaType: "image/png"
    originalName: string
  }
}

export function isDevServerUrl(url: string | null | undefined): boolean {
  if (!url) return false
  try {
    const parsed = new URL(url)
    const host = parsed.hostname.toLowerCase()
    if (host === "localhost" || host === "127.0.0.1" || host === "0.0.0.0" || host === "::1") {
      return true
    }
    if (/^10\./.test(host)) return true
    if (/^192\.168\./.test(host)) return true
    if (/^172\.(1[6-9]|2\d|3[0-1])\./.test(host)) return true
    return false
  } catch {
    return false
  }
}

const DEFAULT_BROWSER_TOOL_THEME: BrowserToolTheme = {
  background: "#09090b",
  foreground: "#fafafa",
  card: "#18181b",
  cardForeground: "#fafafa",
  popover: "#18181b",
  popoverForeground: "#fafafa",
  primary: "#fafafa",
  primaryForeground: "#18181b",
  secondary: "#27272a",
  secondaryForeground: "#fafafa",
  muted: "#27272a",
  mutedForeground: "#a1a1aa",
  accent: "#f97316",
  accentForeground: "#111827",
  destructive: "#ef4444",
  destructiveForeground: "#fafafa",
  border: "#3f3f46",
  input: "#3f3f46",
  ring: "#f97316",
}

function normalizeCssColor(value: string | undefined, fallback: string): string {
  const trimmed = value?.trim()
  if (!trimmed) return fallback
  if (/^(#|rgb|hsl|oklch|oklab|color|var\()/i.test(trimmed)) return trimmed
  return `hsl(${trimmed})`
}

function readThemeColor(styles: CSSStyleDeclaration, name: string, fallback: string): string {
  return normalizeCssColor(styles.getPropertyValue(name), fallback)
}

export function readBrowserToolTheme(): BrowserToolTheme {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return DEFAULT_BROWSER_TOOL_THEME
  }

  const styles = window.getComputedStyle(document.documentElement)
  return {
    background: readThemeColor(styles, "--background", DEFAULT_BROWSER_TOOL_THEME.background),
    foreground: readThemeColor(styles, "--foreground", DEFAULT_BROWSER_TOOL_THEME.foreground),
    card: readThemeColor(styles, "--card", DEFAULT_BROWSER_TOOL_THEME.card),
    cardForeground: readThemeColor(styles, "--card-foreground", DEFAULT_BROWSER_TOOL_THEME.cardForeground),
    popover: readThemeColor(styles, "--popover", DEFAULT_BROWSER_TOOL_THEME.popover),
    popoverForeground: readThemeColor(styles, "--popover-foreground", DEFAULT_BROWSER_TOOL_THEME.popoverForeground),
    primary: readThemeColor(styles, "--primary", DEFAULT_BROWSER_TOOL_THEME.primary),
    primaryForeground: readThemeColor(styles, "--primary-foreground", DEFAULT_BROWSER_TOOL_THEME.primaryForeground),
    secondary: readThemeColor(styles, "--secondary", DEFAULT_BROWSER_TOOL_THEME.secondary),
    secondaryForeground: readThemeColor(styles, "--secondary-foreground", DEFAULT_BROWSER_TOOL_THEME.secondaryForeground),
    muted: readThemeColor(styles, "--muted", DEFAULT_BROWSER_TOOL_THEME.muted),
    mutedForeground: readThemeColor(styles, "--muted-foreground", DEFAULT_BROWSER_TOOL_THEME.mutedForeground),
    accent: readThemeColor(styles, "--accent", DEFAULT_BROWSER_TOOL_THEME.accent),
    accentForeground: readThemeColor(styles, "--accent-foreground", DEFAULT_BROWSER_TOOL_THEME.accentForeground),
    destructive: readThemeColor(styles, "--destructive", DEFAULT_BROWSER_TOOL_THEME.destructive),
    destructiveForeground: readThemeColor(styles, "--destructive-foreground", DEFAULT_BROWSER_TOOL_THEME.destructiveForeground),
    border: readThemeColor(styles, "--border", DEFAULT_BROWSER_TOOL_THEME.border),
    input: readThemeColor(styles, "--input", DEFAULT_BROWSER_TOOL_THEME.input),
    ring: readThemeColor(styles, "--ring", DEFAULT_BROWSER_TOOL_THEME.ring),
  }
}

const BROWSER_TOOL_RUNTIME = String.raw`
;(function () {
  var VERSION = 1;
  var ROOT_ID = "__xero-browser-tool-root";
  var PEN_DOCUMENT_LAYER_ID = "__xero-browser-pen-document-layer";
  var PEN_DOCUMENT_ROOT_ID = "__xero-browser-pen-document-root";
  var TOOL_Z_INDEX = "2147483647";
  var TOOLBAR_POSITION_KEY = "__xeroBrowserToolToolbarPosition";
  var DEFAULT_THEME = ${JSON.stringify(DEFAULT_BROWSER_TOOL_THEME)};
  var THEME_KEYS = Object.keys(DEFAULT_THEME);
  var RAINBOW_STOPS = [
    ["0%", "#ff2d55"],
    ["16%", "#ff9500"],
    ["32%", "#ffcc00"],
    ["50%", "#34c759"],
    ["66%", "#00c7ff"],
    ["82%", "#5856d6"],
    ["100%", "#ff2dff"]
  ];

  function safeStringify(value) {
    try {
      if (value === undefined) return null;
      return JSON.stringify(value);
    } catch (_error) {
      try {
        return JSON.stringify(String(value));
      } catch (_inner) {
        return null;
      }
    }
  }

  function emitTauriInternalBrowserEvent(kind, payload) {
    try {
      var tauri = window.__TAURI_INTERNALS__;
      if (!tauri || typeof tauri.invoke !== "function") return false;
      var result = tauri.invoke("browser_internal_event", {
        kind: String(kind || ""),
        payload: safeStringify(payload || {})
      });
      if (result && typeof result.catch === "function") {
        result.catch(function () {});
      }
      return true;
    } catch (_error) {
      return false;
    }
  }

  function bridgeEmit(kind, payload) {
    if (emitTauriInternalBrowserEvent(kind, payload)) {
      return;
    }

    try {
      if (window.__xeroBridge__ && typeof window.__xeroBridge__.emit === "function") {
        window.__xeroBridge__.emit(kind, payload || {});
        return;
      }
    } catch (_error) {
      // best-effort bridge
    }
  }

  function pageContext() {
    return {
      url: String(location.href || ""),
      title: document.title ? String(document.title) : null
    };
  }

  function viewportContext() {
    return {
      width: Math.round(window.innerWidth || 0),
      height: Math.round(window.innerHeight || 0),
      devicePixelRatio: round(window.devicePixelRatio || 1)
    };
  }

  function pageScrollContext() {
    return {
      x: Math.round(window.scrollX || window.pageXOffset || 0),
      y: Math.round(window.scrollY || window.pageYOffset || 0)
    };
  }

  function round(value) {
    return Math.round(Number(value || 0) * 10) / 10;
  }

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  function compactText(value, max) {
    var text = String(value || "").replace(/\s+/g, " ").trim();
    if (!text) return null;
    return text.length > max ? text.slice(0, max - 1) + "..." : text;
  }

  function cssEscape(value) {
    if (window.CSS && typeof window.CSS.escape === "function") {
      return window.CSS.escape(String(value));
    }
    return String(value).replace(/[^a-zA-Z0-9_-]/g, function (char) {
      return "\\" + char;
    });
  }

  function isUniqueSelector(selector, element) {
    if (!selector) return false;
    try {
      return document.querySelector(selector) === element;
    } catch (_error) {
      return false;
    }
  }

  function selectorFor(element) {
    if (!element || element.nodeType !== 1) return null;
    if (element.id) {
      var byId = "#" + cssEscape(element.id);
      if (isUniqueSelector(byId, element)) return byId;
    }

    var stableAttrs = ["data-testid", "data-test", "data-cy", "aria-label", "name"];
    for (var attrIndex = 0; attrIndex < stableAttrs.length; attrIndex += 1) {
      var attr = stableAttrs[attrIndex];
      var attrValue = element.getAttribute(attr);
      if (!attrValue) continue;
      var byAttr = element.tagName.toLowerCase() + "[" + attr + "=\"" + String(attrValue).replace(/"/g, "\\\"") + "\"]";
      if (isUniqueSelector(byAttr, element)) return byAttr;
    }

    var parts = [];
    var current = element;
    while (current && current.nodeType === 1 && current !== document.body && parts.length < 5) {
      var tag = current.tagName.toLowerCase();
      var part = tag;
      var parent = current.parentElement;
      if (parent) {
        var siblings = Array.prototype.filter.call(parent.children, function (candidate) {
          return candidate.tagName === current.tagName;
        });
        if (siblings.length > 1) {
          part += ":nth-of-type(" + (siblings.indexOf(current) + 1) + ")";
        }
      }
      parts.unshift(part);
      var candidateSelector = parts.join(" > ");
      if (isUniqueSelector(candidateSelector, element)) return candidateSelector;
      current = parent;
    }

    return parts.length > 0 ? parts.join(" > ") : element.tagName.toLowerCase();
  }

  function compactAttributeValue(value, max) {
    return compactText(String(value || ""), max) || "";
  }

  function importantAttributes(element) {
    if (!element || typeof element.getAttributeNames !== "function") return [];
    var names = [];
    try {
      names = element.getAttributeNames();
    } catch (_error) {
      names = [];
    }
    var priority = {
      role: true,
      "aria-label": true,
      "aria-labelledby": true,
      title: true,
      alt: true,
      name: true,
      type: true,
      href: true,
      placeholder: true,
      "data-testid": true,
      "data-test": true,
      "data-cy": true,
      "data-component": true,
    };
    var attrs = [];
    names.sort(function (a, b) {
      var ap = priority[a] ? 0 : 1;
      var bp = priority[b] ? 0 : 1;
      if (ap !== bp) return ap - bp;
      return a < b ? -1 : a > b ? 1 : 0;
    });
    for (var index = 0; index < names.length && attrs.length < 6; index += 1) {
      var name = String(names[index] || "");
      if (!name || !priority[name]) continue;
      if (/password|secret|token|key/i.test(name)) continue;
      var value = element.getAttribute(name);
      if (value == null) continue;
      attrs.push({ name: name, value: compactAttributeValue(value, 140) });
    }
    return attrs;
  }

  function readNumericSourceValue(value) {
    var number = Number(value);
    return Number.isFinite(number) && number > 0 ? Math.round(number) : null;
  }

  function parseSourceRaw(raw) {
    var text = compactText(raw, 500);
    if (!text) return null;
    var match = text.match(/((?:[A-Za-z]:[\\/]|\/|\.{1,2}\/|[^\s:()]+\/)[^\s:()]+\.(?:tsx?|jsx?|vue|svelte|astro|html|css|scss))(?:[:(](\d+))?(?::(\d+))?/i);
    if (!match) return { raw: text };
    return {
      filePath: match[1] || null,
      lineNumber: readNumericSourceValue(match[2]),
      columnNumber: readNumericSourceValue(match[3]),
      raw: text
    };
  }

  function normalizeSourceHint(hint) {
    if (!hint) return null;
    var raw = hint.raw ? compactText(hint.raw, 500) : null;
    var parsed = hint.filePath ? null : parseSourceRaw(raw || "");
    return {
      framework: hint.framework || null,
      componentName: hint.componentName || null,
      filePath: hint.filePath || (parsed && parsed.filePath) || hint.fileName || null,
      lineNumber: readNumericSourceValue(hint.lineNumber) || (parsed && parsed.lineNumber) || null,
      columnNumber: readNumericSourceValue(hint.columnNumber) || (parsed && parsed.columnNumber) || null,
      raw: raw || (parsed && parsed.raw) || null
    };
  }

  function sourceFromAttributes(element) {
    var filePath =
      element.getAttribute("data-file-path") ||
      element.getAttribute("data-file") ||
      element.getAttribute("data-source-file") ||
      element.getAttribute("data-astro-source-file") ||
      null;
    var raw =
      element.getAttribute("data-source") ||
      element.getAttribute("data-src") ||
      element.getAttribute("data-loc") ||
      element.getAttribute("data-vite-dev-id") ||
      element.getAttribute("data-astro-source-loc") ||
      filePath ||
      null;
    var componentName =
      element.getAttribute("data-component") ||
      element.getAttribute("data-component-name") ||
      null;
    var lineNumber =
      readNumericSourceValue(element.getAttribute("data-line")) ||
      readNumericSourceValue(element.getAttribute("data-source-line"));
    var columnNumber =
      readNumericSourceValue(element.getAttribute("data-column")) ||
      readNumericSourceValue(element.getAttribute("data-source-column"));
    if (!filePath && !raw && !componentName && !lineNumber && !columnNumber) return null;
    return normalizeSourceHint({
      framework: "dom-attributes",
      componentName: componentName,
      filePath: filePath,
      lineNumber: lineNumber,
      columnNumber: columnNumber,
      raw: raw
    });
  }

  function componentNameForType(type) {
    if (!type) return null;
    if (typeof type === "string") return type;
    return type.displayName || type.name || type.__name || null;
  }

  function reactFiberForElement(element) {
    var keys = [];
    try {
      keys = Object.keys(element);
    } catch (_error) {
      keys = [];
    }
    for (var index = 0; index < keys.length; index += 1) {
      var key = keys[index];
      if (/^__react(?:Fiber|InternalInstance)\$/i.test(key)) {
        return element[key] || null;
      }
    }
    return null;
  }

  function sourceFromReact(element) {
    var fiber = reactFiberForElement(element);
    var current = fiber;
    var fallbackName = null;
    for (var depth = 0; current && depth < 30; depth += 1) {
      fallbackName =
        fallbackName ||
        componentNameForType(current.elementType) ||
        componentNameForType(current.type);
      var source = current._debugSource || (current._debugOwner && current._debugOwner._debugSource);
      if (source) {
        return normalizeSourceHint({
          framework: "react",
          componentName:
            fallbackName ||
            componentNameForType(current._debugOwner && current._debugOwner.type),
          filePath: source.fileName || source.filePath || null,
          lineNumber: source.lineNumber || null,
          columnNumber: source.columnNumber || null,
          raw: source.fileName || null
        });
      }
      current = current.return || null;
    }
    return fallbackName ? normalizeSourceHint({ framework: "react", componentName: fallbackName }) : null;
  }

  function sourceFromVue(element) {
    var current = element && element.__vueParentComponent;
    for (var depth = 0; current && depth < 20; depth += 1) {
      var type = current.type || {};
      var file = type.__file || null;
      var name = type.name || type.__name || null;
      if (file || name) {
        return normalizeSourceHint({
          framework: "vue",
          componentName: name,
          filePath: file,
          raw: file
        });
      }
      current = current.parent || null;
    }
    return null;
  }

  function sourceHintForElement(element) {
    return (
      sourceFromAttributes(element) ||
      sourceFromReact(element) ||
      sourceFromVue(element) ||
      null
    );
  }

  function ancestorSummary(element) {
    var ancestors = [];
    var current = element ? element.parentElement : null;
    while (
      current &&
      current.nodeType === 1 &&
      current !== document.body &&
      current !== document.documentElement &&
      ancestors.length < 3
    ) {
      ancestors.push({
        selector: selectorFor(current),
        tagName: current.tagName.toLowerCase(),
        id: current.id ? String(current.id) : null,
        role: current.getAttribute("role") || null,
        label: (
          current.getAttribute("aria-label") ||
          current.getAttribute("title") ||
          current.getAttribute("alt") ||
          current.getAttribute("name") ||
          null
        )
      });
      current = current.parentElement;
    }
    return ancestors;
  }

  function elementContext(element) {
    var rect = element.getBoundingClientRect();
    var classes = [];
    try {
      classes = Array.prototype.slice.call(element.classList || []).slice(0, 4).map(String);
    } catch (_error) {
      classes = [];
    }
    var attrs = importantAttributes(element);
    return {
      selector: selectorFor(element),
      tagName: element.tagName.toLowerCase(),
      id: element.id ? String(element.id) : null,
      classes: classes,
      role: element.getAttribute("role") || null,
      label: (
        element.getAttribute("aria-label") ||
        element.getAttribute("title") ||
        element.getAttribute("alt") ||
        element.getAttribute("name") ||
        null
      ),
      text: compactText(element.innerText || element.textContent || "", 500),
      attributes: attrs,
      ancestors: ancestorSummary(element),
      source: sourceHintForElement(element),
      rect: {
        x: round(rect.left),
        y: round(rect.top),
        width: round(rect.width),
        height: round(rect.height)
      }
    };
  }

  function createNode(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = String(text);
    return node;
  }

  function createSvgNode(tag, className) {
    var node = document.createElementNS("http://www.w3.org/2000/svg", tag);
    if (className) node.setAttribute("class", className);
    return node;
  }

  function createMicIcon() {
    var svg = createSvgNode("svg", "dictation-icon");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    var mic = createSvgNode("path");
    mic.setAttribute("d", "M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z");
    var stem = createSvgNode("path");
    stem.setAttribute("d", "M19 10v1a7 7 0 0 1-14 0v-1M12 18v4M8 22h8");
    svg.appendChild(mic);
    svg.appendChild(stem);
    return svg;
  }

  function clearNode(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  function createPenDefs(svg) {
    var defs = createSvgNode("defs");
    svg.appendChild(defs);
    return defs;
  }

  function createRainbowGradient(defs, id) {
    var gradient = createSvgNode("linearGradient");
    gradient.setAttribute("id", id);
    gradient.setAttribute("gradientUnits", "userSpaceOnUse");
    for (var index = 0; index < RAINBOW_STOPS.length; index += 1) {
      var stop = createSvgNode("stop");
      stop.setAttribute("offset", RAINBOW_STOPS[index][0]);
      stop.setAttribute("stop-color", RAINBOW_STOPS[index][1]);
      gradient.appendChild(stop);
    }
    defs.appendChild(gradient);
    return gradient;
  }

  function updateRainbowGradient(stroke) {
    if (!stroke || !stroke.gradient || !stroke.points || stroke.points.length === 0) return;
    var points = stroke.renderedPoints || stroke.points;
    var first = points[0];
    var last = points[points.length - 1] || first;
    var x1 = first.x;
    var y1 = first.y;
    var x2 = last.x;
    var y2 = last.y;
    if (Math.hypot(x2 - x1, y2 - y1) < 8) {
      var bounds = stroke.renderedBounds || {
        minX: stroke.minX,
        minY: stroke.minY,
        maxX: stroke.maxX,
        maxY: stroke.maxY
      };
      x1 = bounds.minX;
      y1 = bounds.minY;
      x2 = bounds.maxX;
      y2 = bounds.maxY;
    }
    if (Math.hypot(x2 - x1, y2 - y1) < 1) {
      x2 = x1 + 1;
    }
    stroke.gradient.setAttribute("x1", String(round(x1)));
    stroke.gradient.setAttribute("y1", String(round(y1)));
    stroke.gradient.setAttribute("x2", String(round(x2)));
    stroke.gradient.setAttribute("y2", String(round(y2)));
  }

  function themeValue(theme, key) {
    var value = theme && typeof theme[key] === "string" ? theme[key].trim() : "";
    return value || DEFAULT_THEME[key];
  }

  function themeVarName(key) {
    return "--xero-tool-" + key.replace(/[A-Z]/g, function (char) {
      return "-" + char.toLowerCase();
    });
  }

  function applyTheme(host, theme) {
    var resolved = {};
    for (var index = 0; index < THEME_KEYS.length; index += 1) {
      var key = THEME_KEYS[index];
      resolved[key] = themeValue(theme, key);
      host.style.setProperty(themeVarName(key), resolved[key]);
    }
    host.style.setProperty("--xero-tool-pen", resolved.ring || resolved.primary || resolved.accent);
    host.style.setProperty("--xero-tool-selection", resolved.ring || resolved.primary);
  }

  function isToolTopLayerOpen(element) {
    if (!element) return false;
    try {
      if (typeof element.matches === "function" && element.matches(":popover-open")) {
        return true;
      }
    } catch (_error) {
      // Some test DOMs do not understand the :popover-open pseudo-class.
    }
    return element.__xeroBrowserToolTopLayerOpen === true;
  }

  function applyTopLayerStyles(element) {
    if (!element) return;
    element.style.margin = "0";
    element.style.padding = "0";
    element.style.border = "0";
    element.style.width = "100vw";
    element.style.height = "100vh";
    element.style.maxWidth = "none";
    element.style.maxHeight = "none";
    element.style.background = "transparent";
  }

  function showInTopLayer(element, bringToFront) {
    if (!element || typeof element.showPopover !== "function") return false;
    element.setAttribute("popover", "manual");
    applyTopLayerStyles(element);
    try {
      if (bringToFront && isToolTopLayerOpen(element) && typeof element.hidePopover === "function") {
        try {
          element.hidePopover();
        } catch (_hideError) {
          // If the browser already closed it, the next show call will repair it.
        }
        element.__xeroBrowserToolTopLayerOpen = false;
      }
      if (!isToolTopLayerOpen(element)) {
        element.showPopover();
        element.__xeroBrowserToolTopLayerOpen = true;
      }
      return true;
    } catch (_error) {
      if (!isToolTopLayerOpen(element)) {
        element.removeAttribute("popover");
      }
      return false;
    }
  }

  function promoteToolLayers(state, bringToFront) {
    if (!state) return;
    if (state.pageRoot) showInTopLayer(state.pageRoot, bringToFront);
    if (state.host) showInTopLayer(state.host, bringToFront);
  }

  function eventHitsChrome(event) {
    var path = typeof event.composedPath === "function" ? event.composedPath() : [];
    for (var index = 0; index < path.length; index += 1) {
      var item = path[index];
      if (item && item.classList && item.classList.contains("xero-tool-chrome")) {
        return true;
      }
    }
    return false;
  }

  function rectFromEdges(left, top, width, height) {
    return {
      left: left,
      top: top,
      right: left + width,
      bottom: top + height,
      width: width,
      height: height
    };
  }

  function normalizeClientRect(rect) {
    if (!rect) return null;
    var left = Number(rect.left != null ? rect.left : rect.x);
    var top = Number(rect.top != null ? rect.top : rect.y);
    var width = Number(rect.width != null ? rect.width : (rect.right - rect.left));
    var height = Number(rect.height != null ? rect.height : (rect.bottom - rect.top));
    if (!Number.isFinite(left) || !Number.isFinite(top) || !Number.isFinite(width) || !Number.isFinite(height)) {
      return null;
    }
    width = Math.max(1, width);
    height = Math.max(1, height);
    return rectFromEdges(left, top, width, height);
  }

  function inflateRect(rect, padding) {
    return rectFromEdges(
      rect.left - padding,
      rect.top - padding,
      rect.width + padding * 2,
      rect.height + padding * 2
    );
  }

  function overlapArea(a, b) {
    var width = Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left));
    var height = Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
    return width * height;
  }

  function clampComposerRect(left, top, width, height) {
    var margin = 8;
    return rectFromEdges(
      clamp(left, margin, Math.max(margin, window.innerWidth - width - margin)),
      clamp(top, margin, Math.max(margin, window.innerHeight - height - margin)),
      width,
      height
    );
  }

  function bestComposerPlacement(width, height, avoidRect) {
    var avoid = normalizeClientRect(avoidRect);
    if (!avoid) return null;

    var gap = 20;
    var centerX = avoid.left + avoid.width / 2;
    var centerY = avoid.top + avoid.height / 2;
    var candidates = [
      {
        side: "left",
        space: avoid.left,
        origin: "right center",
        rect: clampComposerRect(avoid.left - width - gap, centerY - height / 2, width, height)
      },
      {
        side: "right",
        space: window.innerWidth - avoid.right,
        origin: "left center",
        rect: clampComposerRect(avoid.right + gap, centerY - height / 2, width, height)
      },
      {
        side: "above",
        space: avoid.top,
        origin: "center bottom",
        rect: clampComposerRect(centerX - width / 2, avoid.top - height - gap, width, height)
      },
      {
        side: "below",
        space: window.innerHeight - avoid.bottom,
        origin: "center top",
        rect: clampComposerRect(centerX - width / 2, avoid.bottom + gap, width, height)
      }
    ];
    var inflatedAvoid = inflateRect(avoid, 14);
    candidates.sort(function (a, b) {
      var aOverlap = overlapArea(a.rect, inflatedAvoid);
      var bOverlap = overlapArea(b.rect, inflatedAvoid);
      if (aOverlap !== bOverlap) return aOverlap - bOverlap;
      return b.space - a.space;
    });
    return candidates[0];
  }

  function positionComposer(composer, x, y, avoidRect) {
    var width = composer.offsetWidth || 320;
    var height = composer.offsetHeight || 154;
    var placement = bestComposerPlacement(width, height, avoidRect);
    var left;
    var top;
    if (placement) {
      left = placement.rect.left;
      top = placement.rect.top;
      composer.style.setProperty("--xero-composer-origin", placement.origin);
    } else {
      left = x + 12;
      top = y;
      if (left + width + 8 > window.innerWidth) left = x - width - 12;
      left = clamp(left, 8, Math.max(8, window.innerWidth - width - 8));
      top = clamp(top, 8, Math.max(8, window.innerHeight - height - 8));
      composer.style.setProperty("--xero-composer-origin", left > x ? "left top" : "right top");
    }
    composer.style.left = left + "px";
    composer.style.top = top + "px";
  }

  function readToolbarPosition() {
    try {
      var stored = window[TOOLBAR_POSITION_KEY];
      if (!stored || typeof stored !== "object") return null;
      var left = Number(stored.left);
      var top = Number(stored.top);
      if (!Number.isFinite(left) || !Number.isFinite(top)) return null;
      return { left: left, top: top };
    } catch (_error) {
      return null;
    }
  }

  function rememberToolbarPosition(position) {
    try {
      window[TOOLBAR_POSITION_KEY] = {
        left: Number(position.left) || 0,
        top: Number(position.top) || 0
      };
    } catch (_error) {
      // best-effort per-page placement memory
    }
  }

  function toolbarSize(toolbar) {
    var rect = toolbar && typeof toolbar.getBoundingClientRect === "function"
      ? toolbar.getBoundingClientRect()
      : null;
    return {
      width: Math.max(1, Number(rect && rect.width) || Number(toolbar.offsetWidth) || 360),
      height: Math.max(1, Number(rect && rect.height) || Number(toolbar.offsetHeight) || 34)
    };
  }

  function clampToolbarPosition(toolbar, left, top) {
    var size = toolbarSize(toolbar);
    var margin = 8;
    return {
      left: clamp(Number(left) || 0, margin, Math.max(margin, window.innerWidth - size.width - margin)),
      top: clamp(Number(top) || 0, margin, Math.max(margin, window.innerHeight - size.height - margin))
    };
  }

  function applyToolbarPosition(toolbar, left, top, options) {
    var position = clampToolbarPosition(toolbar, left, top);
    toolbar.style.left = round(position.left) + "px";
    toolbar.style.top = round(position.top) + "px";
    toolbar.style.transform = "none";
    if (!options || options.persist !== false) {
      rememberToolbarPosition(position);
    }
    return position;
  }

  function defaultToolbarPosition(toolbar) {
    var size = toolbarSize(toolbar);
    return {
      left: (window.innerWidth - size.width) / 2,
      top: 10
    };
  }

  function syncToolbarPosition(toolbar, options) {
    var stored = readToolbarPosition();
    if (stored) {
      return applyToolbarPosition(toolbar, stored.left, stored.top, options);
    }

    var rect = toolbar.getBoundingClientRect();
    var fallback = defaultToolbarPosition(toolbar);
    return applyToolbarPosition(
      toolbar,
      Number(rect.left) || fallback.left,
      Number(rect.top) || fallback.top,
      { persist: false }
    );
  }

  function setupToolbarDrag(state, toolbar, handle) {
    var dragging = false;
    var activePointerId = null;
    var offsetX = 0;
    var offsetY = 0;

    function move(event) {
      if (!dragging) return;
      if (activePointerId !== null && event.pointerId !== activePointerId) return;
      event.preventDefault();
      event.stopPropagation();
      applyToolbarPosition(toolbar, event.clientX - offsetX, event.clientY - offsetY);
    }

    function stop(event) {
      if (!dragging) return;
      if (activePointerId !== null && event && event.pointerId !== activePointerId) return;
      dragging = false;
      activePointerId = null;
      toolbar.removeAttribute("data-dragging");
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", stop, true);
      window.removeEventListener("pointercancel", stop, true);
      if (event) {
        event.preventDefault();
        event.stopPropagation();
        try {
          handle.releasePointerCapture(event.pointerId);
        } catch (_error) {
          // ignore
        }
      }
    }

    function nudge(event) {
      var horizontal = event.key === "ArrowLeft" || event.key === "ArrowRight";
      var vertical = event.key === "ArrowUp" || event.key === "ArrowDown";
      if (!horizontal && !vertical) return;
      event.preventDefault();
      event.stopPropagation();
      var step = event.shiftKey ? 32 : 8;
      var rect = toolbar.getBoundingClientRect();
      var left = rect.left + (event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0);
      var top = rect.top + (event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0);
      applyToolbarPosition(toolbar, left, top);
    }

    handle.addEventListener("pointerdown", function (event) {
      if (event.button != null && event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      var rect = toolbar.getBoundingClientRect();
      offsetX = event.clientX - rect.left;
      offsetY = event.clientY - rect.top;
      dragging = true;
      activePointerId = event.pointerId != null ? event.pointerId : null;
      toolbar.setAttribute("data-dragging", "true");
      applyToolbarPosition(toolbar, rect.left, rect.top);
      window.addEventListener("pointermove", move, true);
      window.addEventListener("pointerup", stop, true);
      window.addEventListener("pointercancel", stop, true);
      try {
        handle.setPointerCapture(event.pointerId);
      } catch (_error) {
        // ignore
      }
    });
    handle.addEventListener("click", function (event) {
      event.preventDefault();
      event.stopPropagation();
    });
    handle.addEventListener("keydown", nudge);

    var resize = function () {
      syncToolbarPosition(toolbar, { persist: Boolean(readToolbarPosition()) });
    };
    window.addEventListener("resize", resize);
    state.cleanups.push(function () {
      stop();
      window.removeEventListener("resize", resize);
    });
  }

  function removeComposer(state, composer, afterRemove) {
    if (!composer || composer.getAttribute("data-closing") === "true") return;
    composer.setAttribute("data-closing", "true");
    composer.removeAttribute("data-open");
    var note = state && state.composerInput ? String(state.composerInput.value || "") : "";
    emitComposerNote(state, note, false);
    var finished = false;
    function complete() {
      if (finished) return;
      finished = true;
      composer.removeEventListener("transitionend", complete);
      if (composer.parentNode) composer.parentNode.removeChild(composer);
      if (state.composer === composer) {
        state.composer = null;
        state.composerInput = null;
        state.composerDictationButton = null;
        state.composerAvoidRect = null;
      }
      if (typeof afterRemove === "function") afterRemove();
    }
    composer.addEventListener("transitionend", complete);
    var reducedMotion = false;
    try {
      reducedMotion = Boolean(window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
    } catch (_error) {
      reducedMotion = false;
    }
    window.setTimeout(complete, reducedMotion ? 0 : 290);
  }

  function showCaptureLoading(state) {
    if (!state) return false;
    state.loadingMode = true;
    if (state.root) state.root.setAttribute("data-loading", "true");
    if (state.captureLoading && state.captureLoading.parentNode) {
      state.captureLoading.setAttribute("data-open", "true");
      return true;
    }
    var loading = createNode("div", "capture-loading");
    loading.setAttribute("aria-hidden", "true");
    state.layer.appendChild(loading);
    state.captureLoading = loading;
    requestAnimationFrame(function () {
      loading.setAttribute("data-open", "true");
    });
    return true;
  }

  function hideCaptureLoading(state) {
    if (!state) return false;
    state.loadingMode = false;
    if (state.root) state.root.setAttribute("data-loading", "false");
    var loading = state.captureLoading;
    state.captureLoading = null;
    if (loading && loading.parentNode) {
      loading.parentNode.removeChild(loading);
    }
    return true;
  }

  function emitComposerNote(state, note, active) {
    if (!state) return;
    bridgeEmit("tool_note", {
      mode: state.mode,
      note: String(note || ""),
      active: Boolean(active)
    });
  }

  function applyComposerDictationState(state) {
    if (!state || !state.composerDictationButton) return;
    var control = state.composerDictationButton;
    var dictation = state.dictationState || {};
    var visible = dictation.visible === true;
    var listening = dictation.isListening === true;
    var disabled = dictation.isToggleDisabled === true;
    var ariaLabel = String(dictation.ariaLabel || (listening ? "Stop dictation" : "Start dictation"));
    var tooltip = String(dictation.tooltip || ariaLabel);
    var level = Number(dictation.audioLevel || 0);

    control.hidden = !visible;
    control.disabled = disabled;
    control.setAttribute("aria-label", ariaLabel);
    control.setAttribute("aria-pressed", listening ? "true" : "false");
    control.setAttribute("title", tooltip);
    control.setAttribute("data-listening", listening ? "true" : "false");
    control.style.setProperty("--xero-dictation-level", String(clamp(level, 0, 1)));
  }

  function setComposerNoteValue(state, note) {
    var input = state && state.composerInput;
    if (!input) return false;
    var next = String(note || "");
    if (input.value !== next) {
      input.value = next;
      try {
        input.selectionStart = next.length;
        input.selectionEnd = next.length;
      } catch (_error) {
        // ignore
      }
    }
    try {
      input.focus();
    } catch (_error) {
      // ignore
    }
    return true;
  }

  function makeComposer(state, options) {
    if (state.composer) {
      state.composer.remove();
      state.composer = null;
      state.composerDictationButton = null;
    }
    var composer = createNode("div", "composer xero-tool-chrome");
    var header = createNode("div", "composer-header");
    var titleWrap = createNode("div", "composer-title-wrap");
    var title = createNode("div", "composer-title", options.title);
    var subtitle = createNode("div", "composer-subtitle", options.subtitle || "");
    var close = createNode("button", "icon-button", "x");
    close.type = "button";
    close.setAttribute("aria-label", "Close note");
    titleWrap.appendChild(title);
    titleWrap.appendChild(subtitle);
    header.appendChild(titleWrap);
    header.appendChild(close);

    var textarea = createNode("textarea", "composer-input");
    textarea.setAttribute("aria-label", "Browser context note");
    textarea.placeholder = options.placeholder || "";
    textarea.value = options.initialValue || "";

    var footer = createNode("div", "composer-footer");
    var hint = createNode("span", "composer-hint", options.footer || "");
    var actions = createNode("div", "composer-actions");
    var dictation = createNode("button", "dictation-button");
    dictation.type = "button";
    dictation.hidden = true;
    dictation.setAttribute("aria-label", "Start dictation");
    dictation.setAttribute("aria-pressed", "false");
    dictation.appendChild(createMicIcon());
    var send = createNode("button", "send-button", "Add");
    send.type = "button";
    send.setAttribute("aria-label", "Add browser context to composer");
    footer.appendChild(hint);
    actions.appendChild(dictation);
    actions.appendChild(send);
    footer.appendChild(actions);

    composer.appendChild(header);
    composer.appendChild(textarea);
    composer.appendChild(footer);
    state.layer.appendChild(composer);
    state.composer = composer;
    state.composerInput = textarea;
    state.composerDictationButton = dictation;
    state.composerAvoidRect = options.avoidRect || null;
    applyComposerDictationState(state);
    positionComposer(composer, options.x, options.y, options.avoidRect || null);
    emitComposerNote(state, textarea.value, true);
    requestAnimationFrame(function () {
      composer.setAttribute("data-open", "true");
    });

    close.addEventListener("click", function () {
      removeComposer(state, composer);
    });
    send.addEventListener("click", function () {
      if (composer.getAttribute("data-closing") === "true") return;
      var note = String(textarea.value || "").trim();
      removeComposer(state, composer, function () {
        options.onSubmit(note);
      });
    });
    dictation.addEventListener("click", function (event) {
      event.preventDefault();
      event.stopPropagation();
      if (dictation.disabled || dictation.hidden) return;
      bridgeEmit("tool_dictation_toggle", {
        mode: state.mode,
        note: String(textarea.value || "")
      });
    });
    textarea.addEventListener("input", function () {
      emitComposerNote(state, textarea.value, true);
    });
    textarea.addEventListener("keydown", function (event) {
      if (event.key === "Escape") {
        event.preventDefault();
        close.click();
      }
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        send.click();
      }
    });

    requestAnimationFrame(function () {
      try {
        textarea.focus();
      } catch (_error) {
        // ignore
      }
    });
  }

  function makeToolbar(state, mode, pageLabel) {
    var toolbar = createNode("div", "toolbar xero-tool-chrome");
    var handle = createNode("button", "toolbar-handle");
    var badge = createNode("span", "toolbar-badge", mode === "pen" ? "Pen mode" : "Inspect mode");
    var label = createNode("span", "toolbar-label", pageLabel ? "On " + pageLabel : (mode === "pen" ? "Sketch over the page" : "Select an element"));
    var clear = createNode("button", "toolbar-button", "Clear");
    var exit = createNode("button", "toolbar-button", "Exit");
    handle.type = "button";
    handle.setAttribute("aria-label", "Move browser tool controls");
    handle.setAttribute("title", "Move controls");
    clear.type = "button";
    exit.type = "button";
    clear.addEventListener("click", function () {
      if (state.mode === "inspect") {
        if (state.clearInspect) state.clearInspect();
        return;
      }
      if (state.clearPen) state.clearPen();
    });
    exit.addEventListener("click", function () {
      var closingMode = state.mode;
      api.deactivate();
      bridgeEmit("tool_closed", { mode: closingMode });
    });
    toolbar.appendChild(handle);
    toolbar.appendChild(badge);
    toolbar.appendChild(createNode("span", "toolbar-dot", "|"));
    toolbar.appendChild(label);
    toolbar.appendChild(createNode("span", "toolbar-dot", "|"));
    toolbar.appendChild(clear);
    toolbar.appendChild(exit);
    state.layer.appendChild(toolbar);
    state.toolbar = toolbar;
    setupToolbarDrag(state, toolbar, handle);
    requestAnimationFrame(function () {
      if (state.toolbar === toolbar) {
        syncToolbarPosition(toolbar, { persist: false });
      }
    });
  }

  function startCapture(state, context) {
    state.pendingContext = context;
    state.captureMode = true;
    if (state.root) state.root.setAttribute("data-capture", "true");
    bridgeEmit("tool_context", context);
  }

  function setupPen(state) {
    var overlay = createSvgNode("svg", "pen-layer");
    var existingPageLayer = document.getElementById(PEN_DOCUMENT_LAYER_ID);
    if (existingPageLayer && existingPageLayer.parentNode) {
      existingPageLayer.parentNode.removeChild(existingPageLayer);
    }
    var existingPageRoot = document.getElementById(PEN_DOCUMENT_ROOT_ID);
    if (existingPageRoot && existingPageRoot.parentNode) {
      existingPageRoot.parentNode.removeChild(existingPageRoot);
    }
    var pageRoot = createNode("div");
    pageRoot.id = PEN_DOCUMENT_ROOT_ID;
    pageRoot.setAttribute("data-xero-browser-tool-document-root", "true");
    pageRoot.setAttribute("aria-hidden", "true");
    pageRoot.style.position = "fixed";
    pageRoot.style.inset = "0";
    pageRoot.style.overflow = "visible";
    pageRoot.style.pointerEvents = "none";
    pageRoot.style.background = "transparent";
    pageRoot.style.zIndex = TOOL_Z_INDEX;
    var pageFrame = createNode("div");
    pageFrame.setAttribute("data-xero-browser-tool-document-frame", "true");
    pageFrame.style.position = "absolute";
    pageFrame.style.left = "0";
    pageFrame.style.top = "0";
    pageFrame.style.overflow = "visible";
    pageFrame.style.pointerEvents = "none";
    pageFrame.style.zIndex = "1";
    var pageLayer = createSvgNode("svg", "xero-pen-document-layer");
    pageLayer.id = PEN_DOCUMENT_LAYER_ID;
    pageLayer.setAttribute("data-xero-browser-tool-document-layer", "true");
    pageLayer.setAttribute("aria-hidden", "true");
    pageLayer.setAttribute("preserveAspectRatio", "none");
    pageLayer.style.position = "absolute";
    pageLayer.style.left = "0";
    pageLayer.style.top = "0";
    pageLayer.style.overflow = "visible";
    pageLayer.style.pointerEvents = "none";
    pageLayer.style.zIndex = "1";
    pageLayer.style.opacity = "1";
    pageLayer.style.transformOrigin = "top left";
    pageLayer.style.transitionProperty = "opacity";
    pageLayer.style.transitionDuration = "180ms";
    pageLayer.style.transitionTimingFunction = "cubic-bezier(.2,0,0,1)";
    pageLayer.style.willChange = "transform, opacity";
    pageFrame.appendChild(pageLayer);
    pageRoot.appendChild(pageFrame);
    if (state.host && state.host.parentNode) {
      state.host.parentNode.insertBefore(pageRoot, state.host);
    } else {
      (document.documentElement || document.body).appendChild(pageRoot);
    }
    var pageDefs = createPenDefs(pageLayer);
    var active = null;
    var rafId = 0;
    var syncFrameId = 0;
    var strokeIndex = 0;
    var visualViewport = window.visualViewport || null;
    var mutationObserver = null;
    var penSurface = {
      kind: "document",
      element: null,
      restorePosition: null
    };
    state.strokes = [];
    state.pageRoot = pageRoot;
    state.pageFrame = pageFrame;
    state.pageLayer = pageLayer;
    state.layer.appendChild(overlay);
    state.penLayer = overlay;
    overlay.setAttribute("width", "100%");
    overlay.setAttribute("height", "100%");
    overlay.setAttribute("preserveAspectRatio", "none");

    function readViewportSize() {
      return {
        width: Math.max(1, Math.round(window.innerWidth || 1)),
        height: Math.max(1, Math.round(window.innerHeight || 1))
      };
    }

    function readScrollPosition() {
      var doc = document.documentElement || {};
      var body = document.body || {};
      return {
        x: Number(window.scrollX || window.pageXOffset || doc.scrollLeft || body.scrollLeft || 0),
        y: Number(window.scrollY || window.pageYOffset || doc.scrollTop || body.scrollTop || 0)
      };
    }

    function isDocumentScrollRoot(element) {
      return (
        !element ||
        element === document.documentElement ||
        element === document.body ||
        element === document.scrollingElement
      );
    }

    function isScrollableContainer(element) {
      if (!element || element.nodeType !== 1 || isDocumentScrollRoot(element)) return false;
      var style = window.getComputedStyle(element);
      var overflowX = style.overflowX;
      var overflowY = style.overflowY;
      var scrollsX = /(auto|scroll|overlay)/.test(overflowX) && element.scrollWidth > element.clientWidth;
      var scrollsY = /(auto|scroll|overlay)/.test(overflowY) && element.scrollHeight > element.clientHeight;
      return scrollsX || scrollsY;
    }

    function scrollContainerForElement(element) {
      var current = element;
      while (current && current.nodeType === 1) {
        if (isScrollableContainer(current)) return current;
        current = current.parentElement;
      }
      return null;
    }

    function samePenSurface(surface, element) {
      if (!surface) return false;
      if (!element) return surface.kind === "document";
      return surface.kind === "element" && surface.element === element;
    }

    function restorePenSurfacePosition() {
      if (penSurface && typeof penSurface.restorePosition === "function") {
        penSurface.restorePosition();
      }
      if (penSurface) penSurface.restorePosition = null;
    }

    function activatePenSurface(element) {
      var nextElement = isDocumentScrollRoot(element) ? null : element;
      if (samePenSurface(penSurface, nextElement)) return;

      restorePenSurfacePosition();

      if (!nextElement) {
        penSurface = {
          kind: "document",
          element: null,
          restorePosition: null
        };
      } else {
        penSurface = {
          kind: "element",
          element: nextElement,
          restorePosition: null
        };
      }

      clearNode(pageLayer);
      pageDefs = createPenDefs(pageLayer);
      syncLayerSize();
    }

    function activatePenSurfaceForPoint(clientX, clientY) {
      if (state.strokes.length > 0 || active) return;
      var element = underlyingElementAt(clientX, clientY);
      activatePenSurface(scrollContainerForElement(element));
    }

    function readSurfaceScrollPosition() {
      if (penSurface.kind === "element" && penSurface.element) {
        return {
          x: Number(penSurface.element.scrollLeft || 0),
          y: Number(penSurface.element.scrollTop || 0)
        };
      }
      return readScrollPosition();
    }

    function readSurfaceClientOrigin() {
      if (penSurface.kind === "element" && penSurface.element) {
        var rect = penSurface.element.getBoundingClientRect();
        return {
          x: rect.left,
          y: rect.top
        };
      }
      return { x: 0, y: 0 };
    }

    function readVisibleSurfaceRect() {
      var viewport = readViewportSize();
      var scroll = readSurfaceScrollPosition();
      if (penSurface.kind === "element" && penSurface.element) {
        var rect = penSurface.element.getBoundingClientRect();
        return {
          x: round(scroll.x),
          y: round(scroll.y),
          left: round(rect.left),
          top: round(rect.top),
          width: Math.max(1, round(rect.width)),
          height: Math.max(1, round(rect.height))
        };
      }
      return {
        x: round(scroll.x),
        y: round(scroll.y),
        left: 0,
        top: 0,
        width: viewport.width,
        height: viewport.height
      };
    }

    function syncLayerSize() {
      var visible = readVisibleSurfaceRect();
      pageLayer.setAttribute(
        "viewBox",
        visible.x + " " + visible.y + " " + visible.width + " " + visible.height
      );
      pageLayer.setAttribute("width", String(visible.width));
      pageLayer.setAttribute("height", String(visible.height));
      pageLayer.style.width = visible.width + "px";
      pageLayer.style.height = visible.height + "px";
      pageLayer.style.transform = "none";

      if (penSurface.kind === "element" && penSurface.element) {
        pageFrame.style.left = visible.left + "px";
        pageFrame.style.top = visible.top + "px";
        pageFrame.style.width = visible.width + "px";
        pageFrame.style.height = visible.height + "px";
        pageFrame.style.overflow = "visible";
      } else {
        pageFrame.style.left = "0px";
        pageFrame.style.top = "0px";
        pageFrame.style.width = visible.width + "px";
        pageFrame.style.height = visible.height + "px";
        pageFrame.style.overflow = "visible";
      }
    }

    function syncOverlayViewport() {
      var viewport = readViewportSize();
      overlay.setAttribute(
        "viewBox",
        "0 0 " + viewport.width + " " + viewport.height
      );
    }

    function pagePoint(event) {
      var scroll = readSurfaceScrollPosition();
      var origin = readSurfaceClientOrigin();
      return {
        x: event.clientX - origin.x + scroll.x,
        y: event.clientY - origin.y + scroll.y,
        clientX: event.clientX,
        clientY: event.clientY
      };
    }

    function pathData(points) {
      if (!points || points.length === 0) return "";
      var segments = ["M " + round(points[0].x) + " " + round(points[0].y)];
      for (var index = 1; index < points.length; index += 1) {
        segments.push("L " + round(points[index].x) + " " + round(points[index].y));
      }
      return segments.join(" ");
    }

    function boundsForPoints(points) {
      var first = points && points[0] ? points[0] : { x: 0, y: 0 };
      var bounds = {
        minX: first.x,
        maxX: first.x,
        minY: first.y,
        maxY: first.y
      };
      for (var index = 1; points && index < points.length; index += 1) {
        bounds.minX = Math.min(bounds.minX, points[index].x);
        bounds.maxX = Math.max(bounds.maxX, points[index].x);
        bounds.minY = Math.min(bounds.minY, points[index].y);
        bounds.maxY = Math.max(bounds.maxY, points[index].y);
      }
      return bounds;
    }

    function updateActivePath() {
      rafId = 0;
      if (!active || !active.path) return;
      syncLayerSize();
      active.path.setAttribute("d", pathData(active.points));
      active.renderedPoints = active.points;
      active.renderedBounds = boundsForPoints(active.points);
      updateRainbowGradient(active);
    }

    function scheduleActivePathUpdate() {
      if (rafId) return;
      rafId = requestAnimationFrame(updateActivePath);
    }

    function stylePenPath(path, stroke) {
      path.setAttribute("fill", "none");
      path.setAttribute("stroke-width", "3");
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
      path.style.vectorEffect = "non-scaling-stroke";
      path.style.pointerEvents = "none";
      path.style.stroke = stroke;
    }

    function createStroke(start) {
      syncLayerSize();
      var gradientId = "xero-pen-rainbow-doc-" + Date.now().toString(36) + "-" + strokeIndex;
      strokeIndex += 1;
      var gradient = createRainbowGradient(pageDefs, gradientId);
      var path = createSvgNode("path", "xero-document-pen-path active");
      path.setAttribute("d", pathData([start]));
      stylePenPath(path, "url(#" + gradientId + ")");
      pageLayer.appendChild(path);
      var stroke = {
        points: [start],
        minX: start.x,
        maxX: start.x,
        minY: start.y,
        maxY: start.y,
        renderedPoints: [start],
        renderedBounds: { minX: start.x, maxX: start.x, minY: start.y, maxY: start.y },
        gradient: gradient,
        path: path
      };
      updateRainbowGradient(stroke);
      return stroke;
    }

    function appendPoint(event, force) {
      if (!active) return;
      var next = pagePoint(event);
      var last = active.points[active.points.length - 1];
      if (last && Math.hypot(next.x - last.x, next.y - last.y) < (force ? 0.5 : 2)) return;
      active.points.push(next);
      active.minX = Math.min(active.minX, next.x);
      active.maxX = Math.max(active.maxX, next.x);
      active.minY = Math.min(active.minY, next.y);
      active.maxY = Math.max(active.maxY, next.y);
      scheduleActivePathUpdate();
    }

    function pagePointToClient(point) {
      var scroll = readSurfaceScrollPosition();
      var origin = readSurfaceClientOrigin();
      return {
        x: point.x - scroll.x + origin.x,
        y: point.y - scroll.y + origin.y
      };
    }

    function strokeClientRect(stroke) {
      var bounds = stroke.renderedBounds || boundsForPoints(stroke.points || []);
      var topLeft = pagePointToClient({ x: bounds.minX, y: bounds.minY });
      var bottomRight = pagePointToClient({ x: bounds.maxX, y: bounds.maxY });
      return {
        x: Math.min(topLeft.x, bottomRight.x),
        y: Math.min(topLeft.y, bottomRight.y),
        width: Math.max(1, Math.abs(bottomRight.x - topLeft.x)),
        height: Math.max(1, Math.abs(bottomRight.y - topLeft.y))
      };
    }

    function allStrokeClientRect() {
      if (!state.strokes || state.strokes.length === 0) return null;
      var rect = null;
      for (var index = 0; index < state.strokes.length; index += 1) {
        var next = strokeClientRect(state.strokes[index]);
        if (!rect) {
          rect = {
            x: next.x,
            y: next.y,
            width: next.width,
            height: next.height
          };
          continue;
        }
        var left = Math.min(rect.x, next.x);
        var top = Math.min(rect.y, next.y);
        var right = Math.max(rect.x + rect.width, next.x + next.width);
        var bottom = Math.max(rect.y + rect.height, next.y + next.height);
        rect = {
          x: round(left),
          y: round(top),
          width: round(right - left),
          height: round(bottom - top)
        };
      }
      return rect;
    }

    function emitPenState() {
      bridgeEmit("tool_state", {
        mode: "pen",
        strokeCount: state.strokes.length,
        hasDrawing: state.strokes.length > 0
      });
    }

    function repositionComposer() {
      if (!state.composer || !state.composerStroke) return;
      var points = state.composerStroke.points || [];
      if (points.length === 0) return;
      var anchor = pagePointToClient(points[points.length - 1]);
      positionComposer(state.composer, anchor.x, anchor.y, strokeClientRect(state.composerStroke));
    }

    function syncPenLayer() {
      syncFrameId = 0;
      promoteToolLayers(state, false);
      syncLayerSize();
      syncOverlayViewport();
      repositionComposer();
    }

    function schedulePenSync() {
      if (syncFrameId) return;
      syncFrameId = requestAnimationFrame(syncPenLayer);
    }

    function mutationBelongsToTool(target) {
      return Boolean(
        target &&
        (
          target === pageLayer ||
          target === pageRoot ||
          target === pageFrame ||
          target === state.host ||
          (pageLayer.contains && pageLayer.contains(target)) ||
          (pageRoot.contains && pageRoot.contains(target)) ||
          (state.host.contains && state.host.contains(target))
        )
      );
    }

    function pageMutationCallback(records) {
      for (var index = 0; index < records.length; index += 1) {
        var record = records[index];
        if (!mutationBelongsToTool(record.target)) {
          schedulePenSync();
          return;
        }
      }
    }

    state.clearPen = function () {
      state.strokes = [];
      active = null;
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
      if (syncFrameId) {
        cancelAnimationFrame(syncFrameId);
        syncFrameId = 0;
      }
      emitComposerNote(state, state.composerInput ? state.composerInput.value : "", false);
      state.pendingContext = null;
      state.composerAnchor = null;
      state.composerStroke = null;
      if (state.composer) state.composer.remove();
      state.composer = null;
      state.composerInput = null;
      state.composerDictationButton = null;
      clearNode(pageLayer);
      pageDefs = createPenDefs(pageLayer);
      emitPenState();
    };

    function underlyingElementAt(clientX, clientY) {
      var previous = state.host.style.pointerEvents;
      state.host.style.pointerEvents = "none";
      try {
        return document.elementFromPoint ? document.elementFromPoint(clientX, clientY) : null;
      } finally {
        state.host.style.pointerEvents = previous;
      }
    }

    function canScrollElement(element, axis, delta) {
      if (!element) return false;
      if (element === document.documentElement || element === document.body || element === document.scrollingElement) {
        var scrolling = document.scrollingElement || document.documentElement || document.body;
        if (!scrolling) return false;
        if (axis === "x") return scrolling.scrollWidth > scrolling.clientWidth;
        return scrolling.scrollHeight > scrolling.clientHeight;
      }
      var style = window.getComputedStyle(element);
      var overflow = axis === "x" ? style.overflowX : style.overflowY;
      if (!/(auto|scroll|overlay)/.test(overflow)) return false;
      if (axis === "x") {
        if (element.scrollWidth <= element.clientWidth) return false;
        if (delta < 0) return element.scrollLeft > 0;
        if (delta > 0) return element.scrollLeft + element.clientWidth < element.scrollWidth;
        return true;
      }
      if (element.scrollHeight <= element.clientHeight) return false;
      if (delta < 0) return element.scrollTop > 0;
      if (delta > 0) return element.scrollTop + element.clientHeight < element.scrollHeight;
      return true;
    }

    function scrollTargetForWheel(start, event) {
      var axis = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? "x" : "y";
      var delta = axis === "x" ? event.deltaX : event.deltaY;
      var current = start;
      while (current && current.nodeType === 1) {
        if (canScrollElement(current, axis, delta)) return current;
        current = current.parentElement;
      }
      return document.scrollingElement || document.documentElement || document.body;
    }

    overlay.addEventListener("wheel", function (event) {
      if (eventHitsChrome(event)) return;
      var start = underlyingElementAt(event.clientX, event.clientY);
      var target = scrollTargetForWheel(start, event);
      if (!target) return;
      event.preventDefault();
      if (target === document.documentElement || target === document.body || target === document.scrollingElement) {
        window.scrollBy(event.deltaX, event.deltaY);
        schedulePenSync();
        return;
      }
      target.scrollLeft += event.deltaX;
      target.scrollTop += event.deltaY;
      schedulePenSync();
    }, { passive: false });

    overlay.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || eventHitsChrome(event)) return;
      event.preventDefault();
      promoteToolLayers(state, true);
      activatePenSurfaceForPoint(event.clientX, event.clientY);
      syncPenLayer();
      state.captureMode = false;
      if (state.root) state.root.setAttribute("data-capture", "false");
      active = createStroke(pagePoint(event));
      try {
        overlay.setPointerCapture(event.pointerId);
      } catch (_error) {
        // ignore
      }
    });

    overlay.addEventListener("pointermove", function (event) {
      if (!active) return;
      event.preventDefault();
      appendPoint(event, false);
    });

    function finish(event) {
      if (!active) return;
      event.preventDefault();
      try {
        overlay.releasePointerCapture(event.pointerId);
      } catch (_error) {
        // ignore
      }
      appendPoint(event, true);
      var finished = active;
      active = null;
      if (finished.points.length > 1) {
        finished.path.setAttribute("class", "xero-document-pen-path");
        finished.path.setAttribute("d", pathData(finished.points));
        finished.renderedPoints = finished.points;
        finished.renderedBounds = boundsForPoints(finished.points);
        updateRainbowGradient(finished);
        state.strokes.push(finished);
        state.composerStroke = finished;
        emitPenState();
        var composerAnchor = pagePointToClient(finished.points[finished.points.length - 1]);
        makeComposer(state, {
          title: "Sketch note",
          subtitle: state.strokes.length + " stroke" + (state.strokes.length === 1 ? "" : "s"),
          placeholder: "Tell the agent what to do with this sketch...",
          footer: "Drawing will be attached as an image",
          x: composerAnchor.x,
          y: composerAnchor.y,
          avoidRect: strokeClientRect(finished),
          onSubmit: function (note) {
            if (state.strokes.length === 0 && !note) return;
            startCapture(state, {
              kind: "pen",
              note: note,
              page: pageContext(),
              strokeCount: state.strokes.length,
              viewport: viewportContext(),
              scroll: pageScrollContext(),
              annotationBounds: allStrokeClientRect()
            });
          }
        });
      } else if (finished.path && finished.path.parentNode) {
        finished.path.parentNode.removeChild(finished.path);
      }
    }

    overlay.addEventListener("pointerup", finish);
    overlay.addEventListener("pointercancel", finish);
    window.addEventListener("resize", schedulePenSync);
    window.addEventListener("scroll", schedulePenSync, true);
    if (visualViewport) {
      visualViewport.addEventListener("resize", schedulePenSync);
      visualViewport.addEventListener("scroll", schedulePenSync);
    }
    if (typeof MutationObserver === "function" && document.documentElement) {
      mutationObserver = new MutationObserver(pageMutationCallback);
      mutationObserver.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["class", "style", "hidden"],
        childList: true,
        subtree: true
      });
    }
    state.syncPenLayer = syncPenLayer;
    state.cleanups.push(function () {
      window.removeEventListener("resize", schedulePenSync);
      window.removeEventListener("scroll", schedulePenSync, true);
      if (visualViewport) {
        visualViewport.removeEventListener("resize", schedulePenSync);
        visualViewport.removeEventListener("scroll", schedulePenSync);
      }
      if (mutationObserver) mutationObserver.disconnect();
      if (rafId) cancelAnimationFrame(rafId);
      if (syncFrameId) cancelAnimationFrame(syncFrameId);
      bridgeEmit("tool_state", { mode: "pen", strokeCount: 0, hasDrawing: false });
      if (pageRoot && pageRoot.parentNode) pageRoot.parentNode.removeChild(pageRoot);
      restorePenSurfacePosition();
    });
    syncPenLayer();
    emitPenState();
  }

  function setupInspect(state) {
    var highlight = createNode("div", "inspect-highlight");
    var label = createNode("div", "inspect-label");
    highlight.appendChild(label);
    state.layer.appendChild(highlight);
    state.highlight = highlight;
    state.hoveredElement = null;
    state.selectedElement = null;

    state.clearInspect = function () {
      restoreSelectedElement();
      state.hoveredElement = null;
      state.selectedElement = null;
      state.selectedContext = null;
      emitComposerNote(state, state.composerInput ? state.composerInput.value : "", false);
      if (state.composer && state.composer.parentNode) {
        state.composer.parentNode.removeChild(state.composer);
      }
      state.composer = null;
      state.composerInput = null;
      state.composerDictationButton = null;
      state.composerAvoidRect = null;
      showElement(null, false);
    };

    function elementAt(x, y) {
      var previous = state.host.style.pointerEvents;
      state.host.style.pointerEvents = "none";
      var element = null;
      try {
        element = document.elementFromPoint(x, y);
      } finally {
        state.host.style.pointerEvents = previous;
      }
      if (!element || element === document.documentElement || element === document.body) return null;
      return element;
    }

    function showElement(element, existingContext) {
      if (!element) {
        highlight.style.display = "none";
        return null;
      }
      var context = existingContext || elementContext(element);
      var rect = context.rect;
      if (rect.width <= 0 && rect.height <= 0) {
        highlight.style.display = "none";
        return context;
      }
      highlight.style.display = "block";
      highlight.style.left = rect.x + "px";
      highlight.style.top = rect.y + "px";
      highlight.style.width = Math.max(1, rect.width) + "px";
      highlight.style.height = Math.max(1, rect.height) + "px";
      highlight.setAttribute("data-selected", "false");
      label.textContent = context.selector || context.tagName;
      return context;
    }

    function readInlineStyle(element, property) {
      return {
        value: element.style.getPropertyValue(property),
        priority: element.style.getPropertyPriority(property)
      };
    }

    function restoreInlineStyle(element, property, snapshot) {
      if (!snapshot || !snapshot.value) {
        element.style.removeProperty(property);
        return;
      }
      element.style.setProperty(property, snapshot.value, snapshot.priority || "");
    }

    function readAttribute(element, name) {
      return {
        present: element.hasAttribute(name),
        value: element.getAttribute(name)
      };
    }

    function restoreAttribute(element, name, snapshot) {
      if (snapshot && snapshot.present) {
        element.setAttribute(name, snapshot.value || "");
        return;
      }
      element.removeAttribute(name);
    }

    function restoreSelectedElement() {
      var restore = state.selectedElementRestore;
      state.selectedElementRestore = null;
      if (!restore || !restore.element || !restore.element.style) return;
      restoreInlineStyle(restore.element, "outline", restore.outline);
      restoreInlineStyle(restore.element, "outline-offset", restore.outlineOffset);
      restoreAttribute(restore.element, "data-xero-browser-tool-selected", restore.selectedAttr);
      restoreAttribute(restore.element, "data-xero-browser-tool-selected-label", restore.labelAttr);
    }

    function applySelectedElement(element, context) {
      restoreSelectedElement();
      if (!element || !element.style) return;
      state.selectedElementRestore = {
        element: element,
        outline: readInlineStyle(element, "outline"),
        outlineOffset: readInlineStyle(element, "outline-offset"),
        selectedAttr: readAttribute(element, "data-xero-browser-tool-selected"),
        labelAttr: readAttribute(element, "data-xero-browser-tool-selected-label")
      };
      element.setAttribute("data-xero-browser-tool-selected", "true");
      element.setAttribute("data-xero-browser-tool-selected-label", context.selector || context.tagName);
      element.style.setProperty("outline", "2px solid " + state.selectedOutlineColor, "important");
      element.style.setProperty("outline-offset", "-2px", "important");
    }

    function refreshSelectedContext() {
      if (!state.selectedElement) return state.selectedContext || null;
      var context = elementContext(state.selectedElement);
      state.selectedContext = context;
      if (state.composer) {
        state.composerAvoidRect = context.rect;
      }
      return context;
    }

    state.layer.addEventListener("pointermove", function (event) {
      if (state.captureMode || eventHitsChrome(event)) return;
      var element = elementAt(event.clientX, event.clientY);
      state.hoveredElement = element;
      if (!state.selectedElement) showElement(element);
    });

    state.layer.addEventListener("pointerleave", function () {
      if (!state.selectedElement) showElement(null);
    });

    state.layer.addEventListener("click", function (event) {
      if (eventHitsChrome(event)) return;
      event.preventDefault();
      event.stopPropagation();
      var element = elementAt(event.clientX, event.clientY) || state.hoveredElement;
      if (!element) return;
      var context = elementContext(element);
      state.selectedElement = element;
      state.selectedContext = context;
      showElement(null);
      applySelectedElement(element, context);
      makeComposer(state, {
        title: "Element note",
        subtitle: (context.selector || context.tagName),
        placeholder: "Describe what should change about this element...",
        footer: "Element metadata will be attached",
        x: context.rect.x + context.rect.width,
        y: context.rect.y,
        avoidRect: context.rect,
        onSubmit: function (note) {
          refreshSelectedContext();
          startCapture(state, {
            kind: "inspect",
            note: note,
            page: pageContext(),
            element: state.selectedContext,
            viewport: viewportContext(),
            scroll: pageScrollContext()
          });
        }
      });
    }, true);

    state.syncInspectLayer = refreshSelectedContext;
    state.cleanups.push(function () {
      restoreSelectedElement();
    });
  }

  function installStyles(shadow) {
    var style = createNode("style");
    style.textContent =
      ":host{all:initial}" +
      ".layer{position:fixed;inset:0;z-index:" + TOOL_Z_INDEX + ";box-sizing:border-box;color:var(--xero-tool-foreground,#fafafa);font-family:ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;letter-spacing:0}" +
      ".layer *{box-sizing:border-box;letter-spacing:0}" +
      ".pen-layer{position:absolute;inset:0;z-index:1;display:block;width:100vw;height:100vh;cursor:crosshair;touch-action:none;overflow:visible}" +
      ".pen-path{fill:none;stroke:var(--xero-tool-pen,#f97316);stroke-width:3;stroke-linecap:round;stroke-linejoin:round;vector-effect:non-scaling-stroke;pointer-events:none}" +
      ".pen-path.active{stroke:var(--xero-tool-ring,#f97316)}" +
      ".toolbar{position:fixed;z-index:4;top:10px;left:50%;transform:translateX(-50%);display:flex;align-items:center;gap:8px;max-width:min(760px,calc(100vw - 24px));height:34px;padding:0 8px 0 6px;border:1px solid var(--xero-tool-border,#3f3f46);border-radius:999px;background:var(--xero-tool-popover,#18181b);box-shadow:0 16px 42px rgba(0,0,0,.26);font-size:12px;line-height:1;color:var(--xero-tool-muted-foreground,#a1a1aa);white-space:nowrap;user-select:none}" +
      ".toolbar[data-dragging='true']{cursor:grabbing}" +
      ".toolbar-handle{appearance:none;display:flex;align-items:center;justify-content:center;width:22px;height:24px;flex:0 0 22px;border:0;border-radius:999px;background:transparent;color:var(--xero-tool-muted-foreground,#a1a1aa);cursor:grab;touch-action:none}" +
      ".toolbar-handle::before{content:'';width:12px;height:14px;background:radial-gradient(circle,currentColor 1.15px,transparent 1.3px) 0 0/6px 6px;opacity:.75}" +
      ".toolbar-handle:hover,.toolbar-handle:focus-visible{background:var(--xero-tool-secondary,#27272a);color:var(--xero-tool-secondary-foreground,#fafafa)}" +
      ".toolbar-handle:active,.toolbar[data-dragging='true'] .toolbar-handle{cursor:grabbing}" +
      ".toolbar-handle:focus-visible{outline:2px solid var(--xero-tool-ring,#f97316);outline-offset:1px}" +
      ".toolbar-badge{font-weight:700;color:var(--xero-tool-popover-foreground,#fafafa)}" +
      ".toolbar-label{min-width:0;overflow:hidden;text-overflow:ellipsis}" +
      ".toolbar-dot{color:var(--xero-tool-muted-foreground,#a1a1aa)}" +
      ".toolbar-button{appearance:none;border:0;border-radius:6px;background:transparent;color:var(--xero-tool-muted-foreground,#a1a1aa);height:24px;padding:0 6px;font:inherit;cursor:pointer}" +
      ".toolbar-button:hover{background:var(--xero-tool-secondary,#27272a);color:var(--xero-tool-secondary-foreground,#fafafa)}" +
      ".composer{position:fixed;z-index:5;width:320px;overflow:hidden;border:1px solid var(--xero-tool-border,#3f3f46);border-radius:8px;background:var(--xero-tool-popover,#18181b);color:var(--xero-tool-popover-foreground,#fafafa);box-shadow:0 24px 70px rgba(0,0,0,.32),0 0 0 1px rgba(255,255,255,.03) inset;opacity:0;filter:blur(6px);transform:translateY(10px) scale(.96);transform-origin:var(--xero-composer-origin,top left);transition-property:opacity,transform,filter;transition-duration:240ms;transition-timing-function:cubic-bezier(.2,0,0,1);will-change:opacity,transform,filter}" +
      ".composer[data-open='true']{opacity:1;filter:blur(0);transform:translateY(0) scale(1)}" +
      ".composer[data-closing='true']{opacity:0;filter:blur(6px);transform:translateY(10px) scale(.96);pointer-events:none}" +
      ".capture-loading{position:fixed;inset:0;z-index:6;background:rgba(0,0,0,.38);opacity:0;pointer-events:all;transition-property:opacity;transition-duration:240ms;transition-timing-function:cubic-bezier(.2,0,0,1);will-change:opacity}" +
      ".capture-loading[data-open='true']{opacity:1}" +
      "[data-loading='true'] .pen-layer,[data-loading='true'] .inspect-highlight{display:none!important}" +
      "@media (prefers-reduced-motion:reduce){.composer,.capture-loading{filter:none;transform:none;transition-duration:0ms}.composer[data-closing='true']{filter:none;transform:none}}" +
      ".composer-header{display:flex;align-items:center;justify-content:space-between;gap:10px;border-bottom:1px solid var(--xero-tool-border,#3f3f46);padding:9px 10px 8px}" +
      ".composer-title-wrap{min-width:0}" +
      ".composer-title{font-size:12px;font-weight:750;color:var(--xero-tool-popover-foreground,#fafafa);line-height:1.2}" +
      ".composer-subtitle{margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px;color:var(--xero-tool-muted-foreground,#a1a1aa)}" +
      ".icon-button{appearance:none;display:flex;align-items:center;justify-content:center;width:22px;height:22px;border:0;border-radius:6px;background:transparent;color:var(--xero-tool-muted-foreground,#a1a1aa);font:700 13px/1 ui-sans-serif,system-ui;cursor:pointer}" +
      ".icon-button:hover{background:var(--xero-tool-secondary,#27272a);color:var(--xero-tool-secondary-foreground,#fafafa)}" +
      ".composer-input{display:block;width:100%;min-height:72px;max-height:120px;resize:none;border:0;outline:0;background:var(--xero-tool-background,#09090b);color:var(--xero-tool-foreground,#fafafa);padding:9px 10px;font:12px/1.45 ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}" +
      ".composer-input::placeholder{color:var(--xero-tool-muted-foreground,#a1a1aa)}" +
      ".composer-footer{display:flex;align-items:center;justify-content:space-between;gap:10px;border-top:1px solid var(--xero-tool-border,#3f3f46);background:var(--xero-tool-card,#18181b);padding:7px 8px}" +
      ".composer-hint{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px;color:var(--xero-tool-muted-foreground,#a1a1aa)}" +
      ".composer-actions{display:flex;align-items:center;gap:6px;flex:0 0 auto}" +
      ".dictation-button{appearance:none;position:relative;display:flex;align-items:center;justify-content:center;width:28px;height:28px;border:1px solid var(--xero-tool-border,#3f3f46);border-radius:8px;background:transparent;color:var(--xero-tool-muted-foreground,#a1a1aa);cursor:pointer}" +
      ".dictation-button[hidden]{display:none!important}" +
      ".dictation-button:hover{background:var(--xero-tool-secondary,#27272a);color:var(--xero-tool-secondary-foreground,#fafafa)}" +
      ".dictation-button:focus-visible{outline:2px solid var(--xero-tool-ring,#f97316);outline-offset:1px}" +
      ".dictation-button:disabled{cursor:not-allowed;opacity:.48}" +
      ".dictation-button[data-listening='true']{border-color:var(--xero-tool-primary,#fafafa);background:var(--xero-tool-primary,#fafafa);color:var(--xero-tool-primary-foreground,#18181b);box-shadow:0 0 0 calc(2px + var(--xero-dictation-level,0) * 5px) color-mix(in srgb,var(--xero-tool-primary,#fafafa) 18%,transparent)}" +
      ".dictation-icon{width:14px;height:14px;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}" +
      ".send-button{appearance:none;border:1px solid var(--xero-tool-primary,#fafafa);border-radius:8px;background:var(--xero-tool-primary,#fafafa);color:var(--xero-tool-primary-foreground,#18181b);height:28px;padding:0 10px;font:700 11px/1 ui-sans-serif,system-ui;cursor:pointer}" +
      ".send-button:hover{filter:brightness(1.08)}" +
      ".inspect-highlight{position:fixed;z-index:2;display:none;border:2px solid var(--xero-tool-selection,#f97316);border-radius:6px;background:rgba(249,115,22,.08);box-shadow:0 0 0 9999px rgba(0,0,0,.08),0 0 0 1px rgba(255,255,255,.1) inset;pointer-events:none}" +
      ".inspect-highlight[data-selected='true']{border-color:var(--xero-tool-primary,#fafafa);background:rgba(255,255,255,.1)}" +
      ".inspect-label{position:absolute;left:-2px;top:-24px;max-width:360px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;border-radius:5px;background:var(--xero-tool-selection,#f97316);color:var(--xero-tool-accent-foreground,#111827);padding:4px 6px;font:700 10px/1 ui-monospace,SFMono-Regular,Menlo,monospace}" +
      ".inspect-highlight[data-selected='true'] .inspect-label{background:var(--xero-tool-primary,#fafafa);color:var(--xero-tool-primary-foreground,#18181b)}" +
      "[data-exiting='true'] .pen-layer,[data-exiting='true'] .inspect-highlight{opacity:0;pointer-events:none}" +
      "[data-capture='true'] .toolbar,[data-capture='true'] .composer{display:none!important}";
    shadow.appendChild(style);
  }

  function createState(mode, pageLabel, theme) {
    var existing = document.getElementById(ROOT_ID);
    if (existing) existing.remove();
    var existingPenRoot = document.getElementById(PEN_DOCUMENT_ROOT_ID);
    if (existingPenRoot) existingPenRoot.remove();
    var existingPenLayer = document.getElementById(PEN_DOCUMENT_LAYER_ID);
    if (existingPenLayer) existingPenLayer.remove();

    var host = document.createElement("div");
    host.id = ROOT_ID;
    host.setAttribute("data-xero-browser-tool-host", "true");
    host.style.position = "fixed";
    host.style.inset = "0";
    host.style.zIndex = TOOL_Z_INDEX;
    host.style.pointerEvents = "auto";
    host.style.background = "transparent";
    applyTheme(host, theme);
    var shadow = host.attachShadow({ mode: "open" });
    installStyles(shadow);
    var layer = createNode("div", "layer");
    layer.setAttribute("data-capture", "false");
    layer.setAttribute("data-loading", "false");
    shadow.appendChild(layer);
    document.documentElement.appendChild(host);

    var state = {
      mode: mode,
      pageLabel: pageLabel || null,
      host: host,
      root: layer,
      layer: layer,
      toolbar: null,
      composer: null,
      composerInput: null,
      composerDictationButton: null,
      composerAnchor: null,
      composerStroke: null,
      penLayer: null,
      highlight: null,
      dictationState: null,
      cleanups: [],
      pendingContext: null,
      captureMode: false,
      syncPenLayer: null,
      syncInspectLayer: null,
      clearPen: null,
      strokes: [],
      hoveredElement: null,
      selectedElement: null,
      selectedContext: null,
      selectedElementRestore: null,
      selectedOutlineColor: themeValue(theme, "primary")
    };
    makeToolbar(state, mode, pageLabel || null);
    return state;
  }

  var api = {
    __version: VERSION,
    state: null,
    activate: function (options) {
      var mode = options && options.mode === "inspect" ? "inspect" : "pen";
      var pageLabel = options && options.pageLabel ? String(options.pageLabel) : null;
      var theme = options && options.theme ? options.theme : null;
      api.deactivate();
      var state = createState(mode, pageLabel, theme);
      api.state = state;
      if (mode === "inspect") {
        setupInspect(state);
      } else {
        setupPen(state);
      }
      promoteToolLayers(state, true);
      return { active: true, mode: mode };
    },
    prepareCapture: function () {
      var state = api.state;
      if (!state) {
        return null;
      }
      promoteToolLayers(state, true);
      if (typeof state.syncPenLayer === "function") state.syncPenLayer();
      if (typeof state.syncInspectLayer === "function") state.syncInspectLayer();
      state.captureMode = true;
      if (state.root) state.root.setAttribute("data-capture", "true");
      return state.pendingContext || null;
    },
    finishCapture: function (durationMs) {
      var state = api.state;
      if (!state) {
        return false;
      }
      var duration = Number(durationMs);
      if (!Number.isFinite(duration) || duration < 0) duration = 0;
      try {
        if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
          duration = 0;
        }
      } catch (_error) {
        // default to the requested duration
      }
      hideCaptureLoading(state);
      state.captureMode = false;
      if (state.root) {
        state.root.setAttribute("data-capture", "false");
        state.root.setAttribute("data-exiting", "true");
      }
      if (state.pageLayer) {
        state.pageLayer.style.transitionDuration = duration + "ms";
        try {
          state.pageLayer.getBoundingClientRect();
        } catch (_error) {
          // continue with the fade even if layout cannot be read
        }
        state.pageLayer.style.opacity = "0";
      }
      window.setTimeout(function () {
        if (api.state === state) {
          api.deactivate();
        }
      }, duration);
      return true;
    },
    restoreCapture: function () {
      var state = api.state;
      if (!state) {
        return false;
      }
      hideCaptureLoading(state);
      state.captureMode = false;
      if (state.root) {
        state.root.setAttribute("data-capture", "false");
        state.root.setAttribute("data-exiting", "false");
      }
      if (state.pageLayer) {
        state.pageLayer.style.opacity = "1";
      }
      return true;
    },
    setComposerNote: function (note) {
      return setComposerNoteValue(api.state, note);
    },
    focusComposerNote: function () {
      var input = api.state && api.state.composerInput;
      if (!input) return false;
      try {
        input.focus();
        return true;
      } catch (_error) {
        return false;
      }
    },
    setDictationState: function (dictationState) {
      var state = api.state;
      if (!state) return false;
      state.dictationState = dictationState || null;
      applyComposerDictationState(state);
      return true;
    },
    showLoading: function () {
      return showCaptureLoading(api.state);
    },
    hideLoading: function () {
      return hideCaptureLoading(api.state);
    },
    deactivate: function () {
      var state = api.state;
      if (state) {
        hideCaptureLoading(state);
        emitComposerNote(state, state.composerInput ? state.composerInput.value : "", false);
        for (var index = 0; index < state.cleanups.length; index += 1) {
          try { state.cleanups[index](); } catch (_error) { /* ignore */ }
        }
        if (state.host && state.host.parentNode) state.host.parentNode.removeChild(state.host);
      } else {
        var existing = document.getElementById(ROOT_ID);
        if (existing) existing.remove();
        var existingPenRoot = document.getElementById(PEN_DOCUMENT_ROOT_ID);
        if (existingPenRoot) existingPenRoot.remove();
        var existingPenLayer = document.getElementById(PEN_DOCUMENT_LAYER_ID);
        if (existingPenLayer) existingPenLayer.remove();
      }
      api.state = null;
      return true;
    }
  };

  window.__xeroBrowserTool = api;
})();
`

export function buildBrowserToolActivationScript({
  mode,
  pageLabel,
  theme,
}: {
  mode: BrowserToolMode
  pageLabel: string | null
  theme: BrowserToolTheme
}): string {
  return `${BROWSER_TOOL_RUNTIME}
window.__xeroBrowserTool.activate(${JSON.stringify({ mode, pageLabel, theme })});`
}

export const BROWSER_TOOL_DEACTIVATE_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.deactivate === "function") {
  window.__xeroBrowserTool.deactivate();
}
`

export const BROWSER_TOOL_PREPARE_CAPTURE_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.prepareCapture === "function") {
  window.__xeroBrowserTool.prepareCapture();
}
`

export const BROWSER_TOOL_FINISH_CAPTURE_SCRIPT = (durationMs: number) => `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.finishCapture === "function") {
  window.__xeroBrowserTool.finishCapture(${JSON.stringify(durationMs)});
}
`

export const BROWSER_TOOL_SHOW_LOADING_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.showLoading === "function") {
  window.__xeroBrowserTool.showLoading();
}
`

export const BROWSER_TOOL_RESTORE_CAPTURE_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.restoreCapture === "function") {
  window.__xeroBrowserTool.restoreCapture();
}
`

export function buildBrowserToolSetComposerNoteScript(note: string): string {
  return `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.setComposerNote === "function") {
  window.__xeroBrowserTool.setComposerNote(${JSON.stringify(note)});
}
`
}

export const BROWSER_TOOL_FOCUS_COMPOSER_NOTE_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.focusComposerNote === "function") {
  window.__xeroBrowserTool.focusComposerNote();
}
`

export function buildBrowserToolDictationStateScript(state: {
  ariaLabel: string
  audioLevel?: number
  isListening: boolean
  isToggleDisabled: boolean
  tooltip: string
  visible: boolean
}): string {
  return `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.setDictationState === "function") {
  window.__xeroBrowserTool.setDictationState(${JSON.stringify(state)});
}
`
}

export function browserScreenshotBytesFromBase64(base64: string): Uint8Array {
  const raw = base64.includes(",") ? base64.slice(base64.lastIndexOf(",") + 1) : base64
  const binary = atob(raw)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return bytes
}

function browserToolPromptPageReference(page: BrowserToolPageContext): string {
  const title = page.title?.trim()
  const url = sanitizedBrowserToolPromptUrl(page.url)
  return title ? `${title} (${url})` : url
}

function compactBrowserToolMetadataText(value: string | null | undefined, maxLength = 280): string | null {
  const normalized = value?.replace(/\s+/g, " ").trim()
  if (!normalized) return null
  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, Math.max(0, maxLength - 3)).trimEnd()}...`
}

function browserToolPromptHeader(kind: BrowserToolContext["kind"], metadata?: BrowserToolPromptMetadata): string {
  const index = metadata?.captureIndex
  if (typeof index === "number" && Number.isFinite(index) && index > 0) {
    return kind === "pen"
      ? `Browser sketch context (capture ${Math.round(index)}):`
      : `Browser element inspection context (capture ${Math.round(index)}):`
  }
  return kind === "pen" ? "Browser sketch context:" : "Browser element inspection context:"
}

function formatBrowserToolViewport(viewport: BrowserToolViewportContext): string {
  const parts = [`${viewport.width}x${viewport.height} CSS px`]
  if (typeof viewport.devicePixelRatio === "number" && Number.isFinite(viewport.devicePixelRatio)) {
    parts.push(`DPR ${viewport.devicePixelRatio}`)
  }
  return `Viewport: ${parts.join(", ")}`
}

function formatBrowserToolScroll(scroll: BrowserToolScrollContext | null | undefined): string | null {
  if (!scroll) return null
  return `Scroll: x=${Math.round(scroll.x)} y=${Math.round(scroll.y)}`
}

function formatBrowserToolRect(label: string, rect: BrowserToolRectContext | null | undefined): string | null {
  if (!rect) return null
  return `${label}: x=${Math.round(rect.x)} y=${Math.round(rect.y)} w=${Math.round(rect.width)} h=${Math.round(rect.height)} (viewport CSS px)`
}

function formatBrowserToolAttachment(metadata: BrowserToolPromptMetadata | undefined, screenshotAttached: boolean): string | null {
  if (!screenshotAttached) return null
  const name = compactBrowserToolMetadataText(metadata?.attachmentName, 120)
  const index = metadata?.captureIndex
  const imageLabel =
    typeof index === "number" && Number.isFinite(index) && index > 0
      ? `attached image ${Math.round(index)}`
      : "attached image"
  return name
    ? `Attached image: ${imageLabel}, ${name} (paired with this capture; images are ordered by capture number).`
    : `Attached image: ${imageLabel} (paired with this capture; images are ordered by capture number).`
}

function browserToolCaptureMetadataLines(
  context: BrowserToolContext,
  metadata: BrowserToolPromptMetadata | undefined,
  options: { screenshotAttached?: boolean } = {},
): string[] {
  const screenshotAttached = options.screenshotAttached ?? context.kind === "pen"
  return [
    metadata?.appLabel ? `App: ${compactBrowserToolMetadataText(metadata.appLabel, 120)}` : null,
    `Page: ${browserToolPromptPageReference(context.page)}`,
    context.note ? `User note: ${compactBrowserToolMetadataText(context.note)}` : null,
    formatBrowserToolAttachment(metadata, screenshotAttached),
    formatBrowserToolViewport(context.viewport),
    formatBrowserToolScroll(context.scroll),
  ].filter((line): line is string => Boolean(line))
}

function sanitizedBrowserToolPromptUrl(rawUrl: string): string {
  try {
    const parsed = new URL(rawUrl)
    const path = parsed.pathname && parsed.pathname !== "/" ? parsed.pathname : "/"
    if (isDevServerUrl(rawUrl)) {
      return `local dev server ${path}`
    }
    parsed.search = ""
    parsed.hash = ""
    return parsed.toString()
  } catch {
    return "browser page"
  }
}

function formatElementSourceHint(source: BrowserToolElementContext["source"]): string {
  if (!source) return "Source: unavailable"
  const location = source.filePath
    ? `${source.filePath}${source.lineNumber ? `:${source.lineNumber}` : ""}${source.columnNumber ? `:${source.columnNumber}` : ""}`
    : null
  const details = [
    location,
    source.componentName ? `component ${source.componentName}` : null,
    source.framework ? `via ${source.framework}` : null,
    !location && source.raw ? source.raw : null,
  ].filter((value): value is string => Boolean(value))
  return details.length > 0
    ? `Source: ${details.join(" | ")}`
    : "Source: unavailable"
}

function lastBrowserToolPathSegment(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.at(-1) ?? path
}

function compactBrowserToolCardText(value: string, maxLength = 54): string {
  const normalized = value.replace(/\s+/g, " ").trim()
  if (normalized.length <= maxLength) return normalized
  return `${normalized.slice(0, Math.max(0, maxLength - 3)).trimEnd()}...`
}

function elementContextCardSubtitle(element: BrowserToolElementContext): string {
  const source = element.source ?? null
  if (source?.filePath) {
    const fileName = lastBrowserToolPathSegment(source.filePath)
    return source.lineNumber ? `${fileName}:${source.lineNumber}` : fileName
  }
  if (source?.componentName) {
    return source.componentName
  }
  if (element.label) {
    return element.label
  }
  if (element.selector) {
    return element.selector
  }
  return `<${element.tagName}>`
}

export function buildBrowserToolContextCard(
  context: BrowserToolContext,
): BrowserAgentContextRequest["contextCard"] | undefined {
  if (context.kind === "pen") {
    const strokeLabel = `${context.strokeCount} stroke${context.strokeCount === 1 ? "" : "s"}`
    return {
      kind: "sketch",
      title: "Browser sketch context",
      subtitle: compactBrowserToolCardText(`${strokeLabel} on browser screenshot`),
    }
  }
  return {
    kind: "element",
    title: "Element context",
    subtitle: compactBrowserToolCardText(elementContextCardSubtitle(context.element)),
  }
}

function formatElementAttributes(
  attributes: BrowserToolElementContext["attributes"],
): string | null {
  if (!attributes?.length) return null
  return `Stable attrs: ${attributes
    .slice(0, 6)
    .map((attribute) => `${attribute.name}="${attribute.value}"`)
    .join(", ")}`
}

function formatElementAncestors(
  ancestors: BrowserToolElementContext["ancestors"],
): string | null {
  if (!ancestors?.length) return null
  const chain = ancestors.slice(0, 3).map((ancestor) => {
    const parts = [
      `<${ancestor.tagName}>`,
      ancestor.selector,
      ancestor.id ? `id ${ancestor.id}` : null,
      ancestor.role ? `role ${ancestor.role}` : null,
      ancestor.label ? `label "${ancestor.label}"` : null,
    ].filter((value): value is string => Boolean(value))
    return parts.join(" ")
  })
  return `Parent chain: ${chain.join(" > ")}`
}

export function buildBrowserToolAgentPrompt(
  context: BrowserToolContext,
  options: { metadata?: BrowserToolPromptMetadata; screenshotAttached?: boolean } = {},
): string {
  const screenshotAttached = options.screenshotAttached ?? true
  const captureLines = browserToolCaptureMetadataLines(context, options.metadata, {
    screenshotAttached,
  })

  if (context.kind === "pen") {
    return [
      browserToolPromptHeader(context.kind, options.metadata),
      ...captureLines,
      formatBrowserToolRect("Annotation bounds", context.annotationBounds),
      screenshotAttached
        ? `Drawing: ${context.strokeCount} stroke${context.strokeCount === 1 ? "" : "s"} on the attached browser screenshot.`
        : `Drawing: ${context.strokeCount} stroke${context.strokeCount === 1 ? "" : "s"} captured by the browser sketch tool. No browser screenshot was attached.`,
    ]
      .filter((line): line is string => Boolean(line))
      .join("\n")
  }

  const element = context.element
  const details = [
    formatElementSourceHint(element.source ?? null),
    `Element: <${element.tagName}>`,
    formatBrowserToolRect("Element bounds", element.rect),
    element.selector ? `Selector: ${element.selector}` : null,
    element.id ? `ID: ${element.id}` : null,
    element.classes.length ? `Classes: ${element.classes.join(" ")}` : null,
    element.role ? `Role: ${element.role}` : null,
    element.label ? `Label: ${element.label}` : null,
    element.text ? `Text: ${element.text}` : null,
    formatElementAttributes(element.attributes),
    formatElementAncestors(element.ancestors),
  ].filter((line): line is string => Boolean(line))

  return [
    browserToolPromptHeader(context.kind, options.metadata),
    ...captureLines,
    "Selected element (for locating code; no screenshot):",
    ...details.map((line) => `- ${line}`),
    "Use these identifiers to find the implementation before editing.",
  ].join("\n")
}

export function buildBrowserToolVisiblePrompt(context: BrowserToolContext): string {
  return context.note.trim()
}
