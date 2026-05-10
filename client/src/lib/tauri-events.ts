import type { UnlistenFn } from '@tauri-apps/api/event'

export function createSafeTauriUnlisten(unlisten: UnlistenFn): UnlistenFn {
  let disposed = false

  return () => {
    if (disposed) {
      return
    }

    disposed = true

    try {
      const maybePromise = unlisten() as unknown
      if (maybePromise && typeof (maybePromise as PromiseLike<unknown>).then === 'function') {
        void Promise.resolve(maybePromise).catch(() => undefined)
      }
    } catch {
      // Tauri can drop WebView listener internals during teardown; cleanup is best-effort.
    }
  }
}
