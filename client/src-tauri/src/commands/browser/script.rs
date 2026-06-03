pub const BROWSER_BRIDGE_INIT_SCRIPT: &str = r#"
;(function () {
  if (window.__xeroBridge__ && window.__xeroBridge__.__installed) return;

  const bridgeState = (() => {
    const existing = window.__xeroBridgeState__;
    if (existing && existing.protocolVersion === 'xero.in_app_browser_bridge.v1') return existing;
    const state = {
      protocolVersion: 'xero.in_app_browser_bridge.v1',
      sequence: 0,
      navigationGeneration: 1,
      mutationGeneration: 0,
      errors: [],
      inFlightFetch: 0,
      inFlightXhr: 0,
      lastNetworkStartedAt: 0,
      lastNetworkFinishedAt: Date.now(),
      lastMutationAt: Date.now(),
      lastPaintAt: Date.now(),
    };
    Object.defineProperty(window, '__xeroBridgeState__', {
      value: state,
      writable: false,
      configurable: false,
      enumerable: false,
    });
    return state;
  })();

  const rememberError = (kind, value) => {
    try {
      const text = value && (value.stack || value.message)
        ? String(value.stack || value.message)
        : String(value == null ? '' : value);
      bridgeState.errors.push({
        kind: String(kind || 'error'),
        message: text.slice(0, 2000),
        at: Date.now(),
      });
      if (bridgeState.errors.length > 20) bridgeState.errors.shift();
    } catch (_error) {
      // swallow
    }
  };

  window.addEventListener('error', (event) => {
    rememberError('error', event.error || event.message);
  });
  window.addEventListener('unhandledrejection', (event) => {
    rememberError('unhandledrejection', event.reason);
  });

  const invoke = (name, args) => {
    try {
      const tauri = window.__TAURI_INTERNALS__;
      if (tauri && typeof tauri.invoke === 'function') {
        const result = tauri.invoke(name, args);
        if (result && typeof result.catch === 'function') {
          result.catch(() => {
            // bridge IPC is best-effort for arbitrary remote pages
          });
        }
        return result;
      }
    } catch (_error) {
      // swallow — bridge is best-effort
    }
    return undefined;
  };

  const safeStringify = (value) => {
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
  };

  const reply = (requestId, ok, value, errorMessage) => {
    if (!requestId) return;
    invoke('browser_internal_reply', {
      requestId: String(requestId),
      ok: Boolean(ok),
      value: ok ? safeStringify(value) : null,
      error: ok ? null : String(errorMessage == null ? '' : errorMessage),
    });
  };

  const emit = (kind, payload) => {
    bridgeState.sequence += 1;
    invoke('browser_internal_event', {
      kind: String(kind || ''),
      payload: safeStringify(Object.assign({
        protocolVersion: bridgeState.protocolVersion,
        sequence: bridgeState.sequence,
        navigationGeneration: bridgeState.navigationGeneration,
        mutationGeneration: bridgeState.mutationGeneration,
        inFlightFetch: bridgeState.inFlightFetch,
        inFlightXhr: bridgeState.inFlightXhr,
      }, payload || {})),
    });
  };

  const bridge = {
    __installed: true,
    reply,
    emit,
    run: async (requestId, body) => {
      try {
        const fn = new Function('return (async () => { ' + body + ' })();');
        const value = await fn();
        reply(requestId, true, value, null);
      } catch (error) {
        reply(
          requestId,
          false,
          null,
          (error && (error.stack || error.message)) || String(error),
        );
      }
    },
  };

  Object.defineProperty(window, '__xeroBridge__', {
    value: bridge,
    writable: false,
    configurable: false,
    enumerable: false,
  });

  const emitPage = (navigationChanged) => {
    try {
      if (navigationChanged) bridgeState.navigationGeneration += 1;
      emit('page', {
        url: location.href,
        title: document.title,
        readyState: document.readyState,
        lastPaintAt: bridgeState.lastPaintAt,
      });
    } catch (_error) {
      // swallow
    }
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => emitPage(false), { once: true });
  } else {
    emitPage(false);
  }
  window.addEventListener('load', () => emitPage(false));
  window.addEventListener('hashchange', () => emitPage(true));
  window.addEventListener('popstate', () => emitPage(true));

  const wrapHistory = (name) => {
    const original = history[name];
    if (typeof original !== 'function' || original.__xero_wrapped__) return;
    const wrapped = function () {
      const result = original.apply(this, arguments);
      try { emitPage(true); } catch (_e) { /* swallow */ }
      return result;
    };
    wrapped.__xero_wrapped__ = true;
    history[name] = wrapped;
  };
  wrapHistory('pushState');
  wrapHistory('replaceState');

  const forwardConsole = (level) => {
    const original = console[level];
    if (typeof original !== 'function' || original.__xero_wrapped__) return;
    const wrapped = function () {
      try {
        const args = Array.prototype.slice.call(arguments).map((item) => {
          if (item instanceof Error) return item.stack || item.message || String(item);
          if (typeof item === 'object') {
            try { return JSON.stringify(item); } catch (_err) { return String(item); }
          }
          return String(item);
        });
        emit('console', { level, message: args.join(' ') });
      } catch (_err) {
        // swallow
      }
      return original.apply(this, arguments);
    };
    wrapped.__xero_wrapped__ = true;
    console[level] = wrapped;
  };
  ['log', 'info', 'warn', 'error', 'debug'].forEach(forwardConsole);

  try {
    const observer = new MutationObserver(() => {
      bridgeState.mutationGeneration += 1;
      bridgeState.lastMutationAt = Date.now();
    });
    observer.observe(document.documentElement || document, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
    bridgeState.mutationObserver = observer;
  } catch (_error) {
    // swallow
  }

  const paintTick = () => {
    bridgeState.lastPaintAt = Date.now();
    requestAnimationFrame(paintTick);
  };
  try { requestAnimationFrame(paintTick); } catch (_error) { /* swallow */ }

  const sanitizeNetworkUrl = (value) => {
    try {
      const url = new URL(String(value || ''), location.href);
      url.search = '';
      url.hash = '';
      return url.href;
    } catch (_error) {
      return String(value || '').slice(0, 2048);
    }
  };

  const requestUrlForNetworkInstrumentation = (input) => {
    try {
      if (typeof input === 'string') return input;
      if (input && typeof input.url === 'string') return input.url;
    } catch (_error) {
      // swallow
    }
    return '';
  };

  const isTauriIpcFetch = (input) => {
    try {
      const raw = requestUrlForNetworkInstrumentation(input);
      if (!raw) return false;
      const url = new URL(raw, location.href);
      return url.protocol === 'ipc:' || url.hostname === 'ipc.localhost';
    } catch (_error) {
      return false;
    }
  };

  const emitNetwork = (payload) => {
    try {
      emit('network', Object.assign({ capturedAt: Date.now() }, payload || {}));
    } catch (_error) {
      // swallow
    }
  };

  if (typeof window.fetch === 'function' && !window.fetch.__xero_wrapped__) {
    const originalFetch = window.fetch;
    const wrappedFetch = async function () {
      if (isTauriIpcFetch(arguments[0])) {
        return originalFetch.apply(this, arguments);
      }
      const started = Date.now();
      const input = arguments[0];
      const init = arguments[1] || {};
      const url = requestUrlForNetworkInstrumentation(input);
      const method =
        (init && init.method) ||
        (input && input.method) ||
        'GET';
      bridgeState.inFlightFetch += 1;
      bridgeState.lastNetworkStartedAt = started;
      emitNetwork({
        phase: 'start',
        type: 'fetch',
        url: sanitizeNetworkUrl(url),
        method,
        inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
      });
      try {
        const response = await originalFetch.apply(this, arguments);
        bridgeState.inFlightFetch = Math.max(0, bridgeState.inFlightFetch - 1);
        bridgeState.lastNetworkFinishedAt = Date.now();
        emitNetwork({
          phase: 'finish',
          type: 'fetch',
          url: sanitizeNetworkUrl(url),
          method,
          status: response && response.status,
          ok: response && response.ok,
          durationMs: Date.now() - started,
          inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
        });
        return response;
      } catch (error) {
        bridgeState.inFlightFetch = Math.max(0, bridgeState.inFlightFetch - 1);
        bridgeState.lastNetworkFinishedAt = Date.now();
        emitNetwork({
          phase: 'finish',
          type: 'fetch',
          url: sanitizeNetworkUrl(url),
          method,
          error: (error && (error.message || error.stack)) || String(error),
          durationMs: Date.now() - started,
          inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
        });
        throw error;
      }
    };
    wrappedFetch.__xero_wrapped__ = true;
    window.fetch = wrappedFetch;
  }

  if (window.XMLHttpRequest && window.XMLHttpRequest.prototype) {
    const proto = window.XMLHttpRequest.prototype;
    if (!proto.__xero_network_wrapped__) {
      const originalOpen = proto.open;
      const originalSend = proto.send;
      proto.open = function (method, url) {
        this.__xeroRequestInfo = {
          method: method || 'GET',
          url: sanitizeNetworkUrl(url || ''),
        };
        return originalOpen.apply(this, arguments);
      };
      proto.send = function () {
        const xhr = this;
        const started = Date.now();
        const info = xhr.__xeroRequestInfo || {};
        let completed = false;
        const emitDone = () => {
          if (completed) return;
          completed = true;
          bridgeState.inFlightXhr = Math.max(0, bridgeState.inFlightXhr - 1);
          bridgeState.lastNetworkFinishedAt = Date.now();
          emitNetwork({
            phase: 'finish',
            type: 'xhr',
            url: info.url || '',
            method: info.method || 'GET',
            status: xhr.status || null,
            ok: xhr.status >= 200 && xhr.status < 400,
            durationMs: Date.now() - started,
            inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
          });
        };
        const emitFailed = () => {
          if (completed) return;
          completed = true;
          bridgeState.inFlightXhr = Math.max(0, bridgeState.inFlightXhr - 1);
          bridgeState.lastNetworkFinishedAt = Date.now();
          emitNetwork({
            phase: 'finish',
            type: 'xhr',
            url: info.url || '',
            method: info.method || 'GET',
            error: 'request failed',
            durationMs: Date.now() - started,
            inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
          });
        };
        try {
          bridgeState.inFlightXhr += 1;
          bridgeState.lastNetworkStartedAt = started;
          emitNetwork({
            phase: 'start',
            type: 'xhr',
            url: info.url || '',
            method: info.method || 'GET',
            inFlight: bridgeState.inFlightFetch + bridgeState.inFlightXhr,
          });
          xhr.addEventListener('loadend', emitDone, { once: true });
          xhr.addEventListener('error', emitFailed, { once: true });
        } catch (_error) {
          // swallow
        }
        return originalSend.apply(this, arguments);
      };
      proto.__xero_network_wrapped__ = true;
    }
  }

  window.addEventListener('error', (event) => {
    emit('error', {
      message: (event && event.message) || 'unknown error',
      source: (event && event.filename) || null,
      line: (event && event.lineno) || null,
      column: (event && event.colno) || null,
    });
  });

  window.addEventListener('unhandledrejection', (event) => {
    const reason = event && event.reason;
    emit('error', {
      message: (reason && (reason.stack || reason.message)) || String(reason),
      kind: 'unhandled_rejection',
    });
  });
})();
"#;
