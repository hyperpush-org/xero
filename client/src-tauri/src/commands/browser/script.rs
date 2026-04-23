pub const BROWSER_BRIDGE_INIT_SCRIPT: &str = r#"
;(function () {
  if (window.__cadenceBridge__ && window.__cadenceBridge__.__installed) return;

  const invoke = (name, args) => {
    try {
      const tauri = window.__TAURI_INTERNALS__;
      if (tauri && typeof tauri.invoke === 'function') {
        return tauri.invoke(name, args);
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
    invoke('browser_internal_event', {
      kind: String(kind || ''),
      payload: safeStringify(payload),
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

  Object.defineProperty(window, '__cadenceBridge__', {
    value: bridge,
    writable: false,
    configurable: false,
    enumerable: false,
  });

  const emitPage = () => {
    try {
      emit('page', {
        url: location.href,
        title: document.title,
        readyState: document.readyState,
      });
    } catch (_error) {
      // swallow
    }
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', emitPage, { once: true });
  } else {
    emitPage();
  }
  window.addEventListener('load', emitPage);
  window.addEventListener('hashchange', emitPage);
  window.addEventListener('popstate', emitPage);

  const wrapHistory = (name) => {
    const original = history[name];
    if (typeof original !== 'function' || original.__cadence_wrapped__) return;
    const wrapped = function () {
      const result = original.apply(this, arguments);
      try { emitPage(); } catch (_e) { /* swallow */ }
      return result;
    };
    wrapped.__cadence_wrapped__ = true;
    history[name] = wrapped;
  };
  wrapHistory('pushState');
  wrapHistory('replaceState');

  const forwardConsole = (level) => {
    const original = console[level];
    if (typeof original !== 'function' || original.__cadence_wrapped__) return;
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
    wrapped.__cadence_wrapped__ = true;
    console[level] = wrapped;
  };
  ['log', 'info', 'warn', 'error', 'debug'].forEach(forwardConsole);

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
