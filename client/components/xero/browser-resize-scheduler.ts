export interface ViewportRect {
  x: number
  y: number
  width: number
  height: number
}

export interface BrowserResizeScheduler {
  cancel: () => void
  markSynced: (rect: ViewportRect) => void
  reset: () => void
  schedule: (options?: { force?: boolean }) => void
}

interface BrowserResizeSchedulerOptions {
  cancelFrame?: (id: number) => void
  getEnabled: () => boolean
  getNode: () => HTMLElement | null
  getTabId: () => string | null
  inset?: number
  onResize: (rect: ViewportRect, tabId: string | null) => void
  requestFrame?: (callback: FrameRequestCallback) => number
}

export function readBrowserViewportRect(
  node: HTMLElement,
  inset = 0,
): ViewportRect {
  const rect = node.getBoundingClientRect()

  return {
    x: Math.round(rect.left) + inset,
    y: Math.round(rect.top),
    width: Math.max(1, Math.round(rect.width) - inset),
    height: Math.max(1, Math.round(rect.height)),
  }
}

export function rectsEqual(a: ViewportRect | null, b: ViewportRect): boolean {
  if (!a) return false
  return (
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height
  )
}

export function createBrowserResizeScheduler({
  cancelFrame,
  getEnabled,
  getNode,
  getTabId,
  inset = 0,
  onResize,
  requestFrame,
}: BrowserResizeSchedulerOptions): BrowserResizeScheduler {
  let frameId: number | null = null
  let forceNext = false
  let lastSyncedRect: ViewportRect | null = null

  const request =
    requestFrame ??
    ((callback: FrameRequestCallback) => window.requestAnimationFrame(callback))
  const cancel =
    cancelFrame ?? ((id: number) => window.cancelAnimationFrame(id))

  const cancelPendingFrame = () => {
    if (frameId === null) return
    cancel(frameId)
    frameId = null
  }

  return {
    cancel() {
      cancelPendingFrame()
      forceNext = false
    },
    markSynced(rect) {
      lastSyncedRect = rect
    },
    reset() {
      lastSyncedRect = null
    },
    schedule(options) {
      if (!getEnabled()) return
      forceNext = forceNext || options?.force === true
      if (frameId !== null) return

      frameId = request(() => {
        frameId = null
        const shouldForce = forceNext
        forceNext = false

        if (!getEnabled()) return

        const node = getNode()
        if (!node) return

        const next = readBrowserViewportRect(node, inset)
        if (!shouldForce && rectsEqual(lastSyncedRect, next)) return

        lastSyncedRect = next
        onResize(next, getTabId())
      })
    },
  }
}
