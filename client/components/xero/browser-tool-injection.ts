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

export interface BrowserToolPageContext {
  url: string
  title: string | null
}

export interface BrowserToolElementContext {
  selector: string | null
  tagName: string
  id: string | null
  classes: string[]
  role: string | null
  label: string | null
  text: string | null
  rect: {
    x: number
    y: number
    width: number
    height: number
  }
}

export type BrowserToolContext =
  | {
      kind: "pen"
      note: string
      page: BrowserToolPageContext
      strokeCount: number
      viewport: { width: number; height: number }
    }
  | {
      kind: "inspect"
      note: string
      page: BrowserToolPageContext
      element: BrowserToolElementContext
      viewport: { width: number; height: number }
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
  image: {
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

  function bridgeEmit(kind, payload) {
    try {
      if (window.__xeroBridge__ && typeof window.__xeroBridge__.emit === "function") {
        window.__xeroBridge__.emit(kind, payload || {});
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
      height: Math.round(window.innerHeight || 0)
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

  function elementContext(element) {
    var rect = element.getBoundingClientRect();
    var classes = [];
    try {
      classes = Array.prototype.slice.call(element.classList || []).slice(0, 8).map(String);
    } catch (_error) {
      classes = [];
    }
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
    var first = stroke.points[0];
    var last = stroke.points[stroke.points.length - 1] || first;
    var x1 = first.x;
    var y1 = first.y;
    var x2 = last.x;
    var y2 = last.y;
    if (Math.hypot(x2 - x1, y2 - y1) < 8) {
      x1 = stroke.minX;
      y1 = stroke.minY;
      x2 = stroke.maxX;
      y2 = stroke.maxY;
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

  function positionComposer(composer, x, y) {
    var width = 320;
    var height = 154;
    var left = x + 12;
    var top = y;
    if (left + width + 8 > window.innerWidth) left = x - width - 12;
    left = clamp(left, 8, Math.max(8, window.innerWidth - width - 8));
    top = clamp(top, 8, Math.max(8, window.innerHeight - height - 8));
    composer.style.left = left + "px";
    composer.style.top = top + "px";
  }

  function makeComposer(state, options) {
    if (state.composer) {
      state.composer.remove();
      state.composer = null;
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
    var send = createNode("button", "send-button", "Add");
    send.type = "button";
    send.setAttribute("aria-label", "Add browser context to composer");
    footer.appendChild(hint);
    footer.appendChild(send);

    composer.appendChild(header);
    composer.appendChild(textarea);
    composer.appendChild(footer);
    state.layer.appendChild(composer);
    state.composer = composer;
    state.composerInput = textarea;
    positionComposer(composer, options.x, options.y);

    close.addEventListener("click", function () {
      composer.remove();
      if (state.composer === composer) {
        state.composer = null;
        state.composerInput = null;
      }
    });
    send.addEventListener("click", function () {
      options.onSubmit(String(textarea.value || "").trim());
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
    var badge = createNode("span", "toolbar-badge", mode === "pen" ? "Pen mode" : "Inspect mode");
    var label = createNode("span", "toolbar-label", pageLabel ? "On " + pageLabel : (mode === "pen" ? "Sketch over the page" : "Select an element"));
    var clear = createNode("button", "toolbar-button", "Clear");
    var exit = createNode("button", "toolbar-button", "Exit");
    clear.type = "button";
    exit.type = "button";
    clear.hidden = mode !== "pen";
    clear.addEventListener("click", function () {
      if (state.clearPen) state.clearPen();
    });
    exit.addEventListener("click", function () {
      var closingMode = state.mode;
      api.deactivate();
      bridgeEmit("tool_closed", { mode: closingMode });
    });
    toolbar.appendChild(badge);
    toolbar.appendChild(createNode("span", "toolbar-dot", "|"));
    toolbar.appendChild(label);
    toolbar.appendChild(createNode("span", "toolbar-dot", "|"));
    toolbar.appendChild(clear);
    toolbar.appendChild(exit);
    state.layer.appendChild(toolbar);
    state.toolbar = toolbar;
  }

  function startCapture(state, context) {
    state.pendingContext = context;
    state.captureMode = true;
    if (state.root) state.root.setAttribute("data-capture", "true");
    bridgeEmit("tool_context", context);
  }

  function setupPen(state) {
    var svg = createSvgNode("svg", "pen-layer");
    var defs = createPenDefs(svg);
    var active = null;
    var rafId = 0;
    var strokeIndex = 0;
    state.strokes = [];
    state.layer.appendChild(svg);
    state.penLayer = svg;

    function resize() {
      var width = Math.max(1, Math.round(window.innerWidth || 1));
      var height = Math.max(1, Math.round(window.innerHeight || 1));
      svg.setAttribute("viewBox", "0 0 " + width + " " + height);
      svg.setAttribute("width", String(width));
      svg.setAttribute("height", String(height));
    }

    function pathData(points) {
      if (!points || points.length === 0) return "";
      var segments = ["M " + round(points[0].x) + " " + round(points[0].y)];
      for (var index = 1; index < points.length; index += 1) {
        segments.push("L " + round(points[index].x) + " " + round(points[index].y));
      }
      return segments.join(" ");
    }

    function updateActivePath() {
      rafId = 0;
      if (!active || !active.path) return;
      active.path.setAttribute("d", pathData(active.points));
      updateRainbowGradient(active);
    }

    function scheduleActivePathUpdate() {
      if (rafId) return;
      rafId = requestAnimationFrame(updateActivePath);
    }

    function createStroke(start) {
      var gradientId = "xero-pen-rainbow-" + Date.now().toString(36) + "-" + strokeIndex;
      strokeIndex += 1;
      var gradient = createRainbowGradient(defs, gradientId);
      var path = createSvgNode("path", "pen-path active");
      path.setAttribute("d", pathData([start]));
      path.style.stroke = "url(#" + gradientId + ")";
      svg.appendChild(path);
      var stroke = {
        points: [start],
        minX: start.x,
        maxX: start.x,
        minY: start.y,
        maxY: start.y,
        gradient: gradient,
        path: path
      };
      updateRainbowGradient(stroke);
      return stroke;
    }

    function point(event) {
      return { x: event.clientX, y: event.clientY };
    }

    function appendPoint(event, force) {
      if (!active) return;
      var next = point(event);
      var last = active.points[active.points.length - 1];
      if (last && Math.hypot(next.x - last.x, next.y - last.y) < (force ? 0.5 : 2)) return;
      active.points.push(next);
      active.minX = Math.min(active.minX, next.x);
      active.maxX = Math.max(active.maxX, next.x);
      active.minY = Math.min(active.minY, next.y);
      active.maxY = Math.max(active.maxY, next.y);
      scheduleActivePathUpdate();
    }

    state.clearPen = function () {
      state.strokes = [];
      active = null;
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
      state.pendingContext = null;
      if (state.composer) state.composer.remove();
      state.composer = null;
      state.composerInput = null;
      clearNode(svg);
      defs = createPenDefs(svg);
    };

    svg.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || eventHitsChrome(event)) return;
      event.preventDefault();
      state.captureMode = false;
      if (state.root) state.root.setAttribute("data-capture", "false");
      active = createStroke(point(event));
      try {
        svg.setPointerCapture(event.pointerId);
      } catch (_error) {
        // ignore
      }
    });

    svg.addEventListener("pointermove", function (event) {
      if (!active) return;
      event.preventDefault();
      appendPoint(event, false);
    });

    function finish(event) {
      if (!active) return;
      event.preventDefault();
      try {
        svg.releasePointerCapture(event.pointerId);
      } catch (_error) {
        // ignore
      }
      appendPoint(event, true);
      var finished = active;
      finished.path.setAttribute("d", pathData(finished.points));
      updateRainbowGradient(finished);
      finished.path.setAttribute("class", "pen-path");
      active = null;
      if (finished.points.length > 1) {
        state.strokes.push({
          points: finished.points,
          minX: finished.minX,
          maxX: finished.maxX,
          minY: finished.minY,
          maxY: finished.maxY
        });
        makeComposer(state, {
          title: "Sketch note",
          subtitle: state.strokes.length + " stroke" + (state.strokes.length === 1 ? "" : "s"),
          placeholder: "Tell the agent what to do with this sketch...",
          footer: "Drawing will be attached as an image",
          x: finished.maxX,
          y: finished.minY,
          onSubmit: function (note) {
            if (state.strokes.length === 0 && !note) return;
            startCapture(state, {
              kind: "pen",
              note: note,
              page: pageContext(),
              strokeCount: state.strokes.length,
              viewport: viewportContext()
            });
          }
        });
      } else if (finished.path && finished.path.parentNode) {
        finished.path.parentNode.removeChild(finished.path);
      }
    }

    svg.addEventListener("pointerup", finish);
    svg.addEventListener("pointercancel", finish);
    window.addEventListener("resize", resize);
    state.cleanups.push(function () {
      window.removeEventListener("resize", resize);
      if (rafId) cancelAnimationFrame(rafId);
    });
    resize();
  }

  function setupInspect(state) {
    var highlight = createNode("div", "inspect-highlight");
    var label = createNode("div", "inspect-label");
    highlight.appendChild(label);
    state.layer.appendChild(highlight);
    state.highlight = highlight;
    state.hoveredElement = null;
    state.selectedElement = null;

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

    function showElement(element, selected) {
      if (!element) {
        highlight.style.display = "none";
        return;
      }
      var context = elementContext(element);
      var rect = context.rect;
      if (rect.width <= 0 && rect.height <= 0) {
        highlight.style.display = "none";
        return;
      }
      highlight.style.display = "block";
      highlight.style.left = rect.x + "px";
      highlight.style.top = rect.y + "px";
      highlight.style.width = Math.max(1, rect.width) + "px";
      highlight.style.height = Math.max(1, rect.height) + "px";
      highlight.setAttribute("data-selected", selected ? "true" : "false");
      label.textContent = context.selector || context.tagName;
    }

    state.layer.addEventListener("pointermove", function (event) {
      if (state.captureMode || eventHitsChrome(event)) return;
      var element = elementAt(event.clientX, event.clientY);
      state.hoveredElement = element;
      if (!state.selectedElement) showElement(element, false);
    });

    state.layer.addEventListener("pointerleave", function () {
      if (!state.selectedElement) showElement(null, false);
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
      showElement(element, true);
      makeComposer(state, {
        title: "Element note",
        subtitle: (context.selector || context.tagName),
        placeholder: "Describe what should change about this element...",
        footer: "Selection and screenshot will be attached",
        x: context.rect.x + context.rect.width,
        y: context.rect.y,
        onSubmit: function (note) {
          startCapture(state, {
            kind: "inspect",
            note: note,
            page: pageContext(),
            element: state.selectedContext,
            viewport: viewportContext()
          });
        }
      });
    }, true);
  }

  function installStyles(shadow) {
    var style = createNode("style");
    style.textContent =
      ":host{all:initial}" +
      ".layer{position:fixed;inset:0;z-index:2147483647;box-sizing:border-box;color:var(--xero-tool-foreground,#fafafa);font-family:ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;letter-spacing:0}" +
      ".layer *{box-sizing:border-box;letter-spacing:0}" +
      ".pen-layer{position:absolute;inset:0;z-index:1;display:block;width:100vw;height:100vh;cursor:crosshair;touch-action:none;overflow:visible}" +
      ".pen-path{fill:none;stroke:var(--xero-tool-pen,#f97316);stroke-width:3;stroke-linecap:round;stroke-linejoin:round;vector-effect:non-scaling-stroke;pointer-events:none}" +
      ".pen-path.active{stroke:var(--xero-tool-ring,#f97316)}" +
      ".toolbar{position:fixed;z-index:4;top:10px;left:50%;transform:translateX(-50%);display:flex;align-items:center;gap:8px;max-width:min(760px,calc(100vw - 24px));height:34px;padding:0 12px;border:1px solid var(--xero-tool-border,#3f3f46);border-radius:999px;background:var(--xero-tool-popover,#18181b);box-shadow:0 16px 42px rgba(0,0,0,.26);font-size:12px;line-height:1;color:var(--xero-tool-muted-foreground,#a1a1aa);white-space:nowrap}" +
      ".toolbar-badge{font-weight:700;color:var(--xero-tool-popover-foreground,#fafafa)}" +
      ".toolbar-label{min-width:0;overflow:hidden;text-overflow:ellipsis}" +
      ".toolbar-dot{color:var(--xero-tool-muted-foreground,#a1a1aa)}" +
      ".toolbar-button{appearance:none;border:0;border-radius:6px;background:transparent;color:var(--xero-tool-muted-foreground,#a1a1aa);height:24px;padding:0 6px;font:inherit;cursor:pointer}" +
      ".toolbar-button:hover{background:var(--xero-tool-secondary,#27272a);color:var(--xero-tool-secondary-foreground,#fafafa)}" +
      ".composer{position:fixed;z-index:5;width:320px;overflow:hidden;border:1px solid var(--xero-tool-border,#3f3f46);border-radius:8px;background:var(--xero-tool-popover,#18181b);color:var(--xero-tool-popover-foreground,#fafafa);box-shadow:0 24px 70px rgba(0,0,0,.32),0 0 0 1px rgba(255,255,255,.03) inset}" +
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
      ".send-button{appearance:none;border:1px solid var(--xero-tool-primary,#fafafa);border-radius:8px;background:var(--xero-tool-primary,#fafafa);color:var(--xero-tool-primary-foreground,#18181b);height:28px;padding:0 10px;font:700 11px/1 ui-sans-serif,system-ui;cursor:pointer}" +
      ".send-button:hover{filter:brightness(1.08)}" +
      ".inspect-highlight{position:fixed;z-index:2;display:none;border:2px solid var(--xero-tool-selection,#f97316);border-radius:6px;background:rgba(249,115,22,.08);box-shadow:0 0 0 9999px rgba(0,0,0,.08),0 0 0 1px rgba(255,255,255,.1) inset;pointer-events:none}" +
      ".inspect-highlight[data-selected='true']{border-color:var(--xero-tool-primary,#fafafa);background:rgba(255,255,255,.1)}" +
      ".inspect-label{position:absolute;left:-2px;top:-24px;max-width:360px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;border-radius:5px;background:var(--xero-tool-selection,#f97316);color:var(--xero-tool-accent-foreground,#111827);padding:4px 6px;font:700 10px/1 ui-monospace,SFMono-Regular,Menlo,monospace}" +
      ".inspect-highlight[data-selected='true'] .inspect-label{background:var(--xero-tool-primary,#fafafa);color:var(--xero-tool-primary-foreground,#18181b)}" +
      "[data-capture='true'] .toolbar,[data-capture='true'] .composer{display:none!important}";
    shadow.appendChild(style);
  }

  function createState(mode, pageLabel, theme) {
    var existing = document.getElementById(ROOT_ID);
    if (existing) existing.remove();

    var host = document.createElement("div");
    host.id = ROOT_ID;
    host.setAttribute("data-xero-browser-tool-host", "true");
    host.style.position = "fixed";
    host.style.inset = "0";
    host.style.zIndex = "2147483647";
    host.style.pointerEvents = "auto";
    host.style.background = "transparent";
    applyTheme(host, theme);
    var shadow = host.attachShadow({ mode: "open" });
    installStyles(shadow);
    var layer = createNode("div", "layer");
    layer.setAttribute("data-capture", "false");
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
      penLayer: null,
      highlight: null,
      cleanups: [],
      pendingContext: null,
      captureMode: false,
      clearPen: null,
      strokes: [],
      hoveredElement: null,
      selectedElement: null,
      selectedContext: null
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
      return { active: true, mode: mode };
    },
    prepareCapture: function () {
      var state = api.state;
      if (!state) return null;
      state.captureMode = true;
      if (state.root) state.root.setAttribute("data-capture", "true");
      return state.pendingContext || null;
    },
    restoreCapture: function () {
      var state = api.state;
      if (!state) return false;
      state.captureMode = false;
      if (state.root) state.root.setAttribute("data-capture", "false");
      return true;
    },
    deactivate: function () {
      var state = api.state;
      if (state) {
        for (var index = 0; index < state.cleanups.length; index += 1) {
          try { state.cleanups[index](); } catch (_error) { /* ignore */ }
        }
        if (state.host && state.host.parentNode) state.host.parentNode.removeChild(state.host);
      } else {
        var existing = document.getElementById(ROOT_ID);
        if (existing) existing.remove();
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

export const BROWSER_TOOL_RESTORE_CAPTURE_SCRIPT = `
if (window.__xeroBrowserTool && typeof window.__xeroBrowserTool.restoreCapture === "function") {
  window.__xeroBrowserTool.restoreCapture();
}
`

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

export function buildBrowserToolAgentPrompt(context: BrowserToolContext): string {
  const pageLine = browserToolPromptPageReference(context.page)

  if (context.kind === "pen") {
    return [
      "Browser sketch context:",
      `Page: ${pageLine}`,
      `Drawing: ${context.strokeCount} stroke${context.strokeCount === 1 ? "" : "s"} on the attached browser screenshot.`,
    ].join("\n")
  }

  const element = context.element
  const details = [
    `Selector: ${element.selector ?? "unavailable"}`,
    `Tag: <${element.tagName}>`,
    element.id ? `ID: ${element.id}` : null,
    element.classes.length ? `Classes: ${element.classes.join(" ")}` : null,
    element.role ? `Role: ${element.role}` : null,
    element.label ? `Label: ${element.label}` : null,
    element.text ? `Text: ${element.text}` : null,
    `Bounds: x=${element.rect.x}, y=${element.rect.y}, width=${element.rect.width}, height=${element.rect.height}`,
  ].filter((line): line is string => Boolean(line))

  return [
    "Browser element inspection context:",
    `Page: ${pageLine}`,
    "Selected element:",
    ...details.map((line) => `- ${line}`),
    "The attached browser screenshot highlights this selection.",
  ].join("\n")
}

export function buildBrowserToolVisiblePrompt(context: BrowserToolContext): string {
  return context.note.trim()
}
