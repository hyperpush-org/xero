/** @vitest-environment jsdom */

import { describe, expect, it } from "vitest"
import {
  createBrowserResizeScheduler,
  type ViewportRect,
} from "./browser-resize-scheduler"

function installRect(
  node: HTMLElement,
  rect: Pick<DOMRect, "height" | "left" | "top" | "width">,
) {
  Object.defineProperty(node, "getBoundingClientRect", {
    configurable: true,
    value: () =>
      ({
        bottom: rect.top + rect.height,
        height: rect.height,
        left: rect.left,
        right: rect.left + rect.width,
        top: rect.top,
        width: rect.width,
        x: rect.left,
        y: rect.top,
        toJSON: () => ({}),
      }) as DOMRect,
  })
}

function createFrameController() {
  let nextId = 1
  const frames = new Map<number, FrameRequestCallback>()

  return {
    cancelFrame(id: number) {
      frames.delete(id)
    },
    flushFrame() {
      const [id, callback] = frames.entries().next().value ?? []
      if (!id || !callback) {
        throw new Error("No pending frame to flush")
      }
      frames.delete(id)
      callback(0)
    },
    get pendingCount() {
      return frames.size
    },
    requestFrame(callback: FrameRequestCallback) {
      const id = nextId
      nextId += 1
      frames.set(id, callback)
      return id
    },
  }
}

describe("browser resize scheduler", () => {
  it("coalesces multiple resize requests into one animation frame", () => {
    const node = document.createElement("div")
    installRect(node, { height: 240, left: 10, top: 20, width: 400 })
    const frames = createFrameController()
    const calls: Array<ViewportRect & { tabId: string | null }> = []
    const scheduler = createBrowserResizeScheduler({
      cancelFrame: frames.cancelFrame,
      getEnabled: () => true,
      getNode: () => node,
      getTabId: () => "tab-1",
      inset: 6,
      onResize: (rect, tabId) => calls.push({ ...rect, tabId }),
      requestFrame: frames.requestFrame,
    })

    scheduler.schedule()
    scheduler.schedule()

    expect(frames.pendingCount).toBe(1)
    frames.flushFrame()
    expect(frames.pendingCount).toBe(0)
    expect(calls).toEqual([
      { height: 240, tabId: "tab-1", width: 394, x: 16, y: 20 },
    ])
  })

  it("does not schedule another frame after syncing a steady-state rect", () => {
    const node = document.createElement("div")
    installRect(node, { height: 240, left: 10, top: 20, width: 400 })
    const frames = createFrameController()
    const calls: ViewportRect[] = []
    const scheduler = createBrowserResizeScheduler({
      cancelFrame: frames.cancelFrame,
      getEnabled: () => true,
      getNode: () => node,
      getTabId: () => null,
      onResize: (rect) => calls.push(rect),
      requestFrame: frames.requestFrame,
    })

    scheduler.schedule()
    frames.flushFrame()

    expect(calls).toHaveLength(1)
    expect(frames.pendingCount).toBe(0)

    scheduler.schedule()
    frames.flushFrame()

    expect(calls).toHaveLength(1)
    expect(frames.pendingCount).toBe(0)
  })

  it("allows explicit resize triggers to force an unchanged rect through", () => {
    const node = document.createElement("div")
    installRect(node, { height: 240, left: 10, top: 20, width: 400 })
    const frames = createFrameController()
    const calls: ViewportRect[] = []
    const scheduler = createBrowserResizeScheduler({
      cancelFrame: frames.cancelFrame,
      getEnabled: () => true,
      getNode: () => node,
      getTabId: () => null,
      onResize: (rect) => calls.push(rect),
      requestFrame: frames.requestFrame,
    })

    scheduler.schedule()
    frames.flushFrame()
    scheduler.schedule({ force: true })
    frames.flushFrame()

    expect(calls).toHaveLength(2)
  })

  it("cancels pending resize work", () => {
    const node = document.createElement("div")
    installRect(node, { height: 240, left: 10, top: 20, width: 400 })
    const frames = createFrameController()
    const calls: ViewportRect[] = []
    const scheduler = createBrowserResizeScheduler({
      cancelFrame: frames.cancelFrame,
      getEnabled: () => true,
      getNode: () => node,
      getTabId: () => null,
      onResize: (rect) => calls.push(rect),
      requestFrame: frames.requestFrame,
    })

    scheduler.schedule()
    scheduler.cancel()

    expect(frames.pendingCount).toBe(0)
    expect(calls).toHaveLength(0)
  })
})
