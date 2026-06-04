/** @vitest-environment jsdom */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

type ListenerHandle = () => void
type InvokeHandler = (args: Record<string, unknown> | undefined) => unknown

const invokeResponses = new Map<string, InvokeHandler>()
const eventListeners = new Map<string, ((event: { payload: unknown }) => void)[]>()
const invokeCalls: Array<{ command: string; args: Record<string, unknown> | undefined }> = []

function resetBridge() {
  invokeResponses.clear()
  eventListeners.clear()
  invokeCalls.length = 0
}

function registerInvoke(command: string, handler: InvokeHandler) {
  invokeResponses.set(command, handler)
}

function emitEvent(name: string, payload: unknown) {
  const listeners = eventListeners.get(name) ?? []
  listeners.forEach((listener) => listener({ payload }))
}

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: async (command: string, args?: Record<string, unknown>) => {
    invokeCalls.push({ command, args })
    const handler = invokeResponses.get(command)
    if (!handler) return undefined
    return handler(args)
  },
}))

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<ListenerHandle> => {
    const list = eventListeners.get(eventName) ?? []
    list.push(handler)
    eventListeners.set(eventName, list)
    return () => {
      const existing = eventListeners.get(eventName) ?? []
      eventListeners.set(
        eventName,
        existing.filter((entry) => entry !== handler),
      )
    }
  },
}))

import {
  buildBrowserToolActivationScript,
  buildBrowserToolAgentPrompt,
  buildBrowserToolVisiblePrompt,
  type BrowserAgentContextRequest,
  type BrowserToolTheme,
} from "./browser-tool-injection"
import {
  BrowserSidebar,
  collectBrowserOverlayOcclusionRects,
  createBrowserEventCoalescer,
} from "./browser-sidebar"

// jsdom in this project ships a localStorage object whose methods aren't
// functions; install a minimal in-memory shim so the component's first-run
// check (which reads cookie-import state from storage) has something to call.
function installLocalStorage() {
  const store = new Map<string, string>()
  const shim: Storage = {
    get length() {
      return store.size
    },
    clear() {
      store.clear()
    },
    getItem(key) {
      return store.has(key) ? store.get(key)! : null
    },
    key(index) {
      return Array.from(store.keys())[index] ?? null
    },
    removeItem(key) {
      store.delete(key)
    },
    setItem(key, value) {
      store.set(key, String(value))
    },
  }
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: shim,
  })
  return shim
}

let cookieStorage: Storage | null = null

function rect({
  bottom,
  height,
  left,
  right,
  top,
  width,
  x = left,
  y = top,
}: {
  bottom: number
  height: number
  left: number
  right: number
  top: number
  width: number
  x?: number
  y?: number
}): DOMRect {
  return {
    bottom,
    height,
    left,
    right,
    top,
    width,
    x,
    y,
    toJSON: () => ({}),
  } as DOMRect
}

function setWindowInnerSize(width: number, height = window.innerHeight) {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: width,
  })
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: height,
  })
}

function setWindowScroll(x: number, y: number) {
  Object.defineProperty(window, "scrollX", {
    configurable: true,
    value: x,
  })
  Object.defineProperty(window, "scrollY", {
    configurable: true,
    value: y,
  })
  Object.defineProperty(window, "pageXOffset", {
    configurable: true,
    value: x,
  })
  Object.defineProperty(window, "pageYOffset", {
    configurable: true,
    value: y,
  })
}

function setNumberProperty(target: object, key: string, value: number): () => void {
  const original = Object.getOwnPropertyDescriptor(target, key)
  Object.defineProperty(target, key, {
    configurable: true,
    value,
  })

  return () => {
    if (original) {
      Object.defineProperty(target, key, original)
    } else {
      delete (target as Record<string, unknown>)[key]
    }
  }
}

function setDocumentSize(width: number, height: number): () => void {
  const restores = [
    setNumberProperty(document.documentElement, "scrollWidth", width),
    setNumberProperty(document.documentElement, "scrollHeight", height),
    setNumberProperty(document.documentElement, "clientWidth", Math.min(width, window.innerWidth)),
    setNumberProperty(document.documentElement, "clientHeight", Math.min(height, window.innerHeight)),
    setNumberProperty(document.body, "scrollWidth", width),
    setNumberProperty(document.body, "scrollHeight", height),
    setNumberProperty(document.body, "clientWidth", Math.min(width, window.innerWidth)),
    setNumberProperty(document.body, "clientHeight", Math.min(height, window.innerHeight)),
  ]

  return () => {
    for (let index = restores.length - 1; index >= 0; index -= 1) {
      restores[index]?.()
    }
  }
}

function dispatchPointer(
  target: EventTarget,
  type: string,
  init: MouseEventInit,
) {
  target.dispatchEvent(
    new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      ...init,
    }),
  )
}

beforeEach(() => {
  cookieStorage = installLocalStorage()
})

afterEach(() => {
  resetBridge()
  vi.restoreAllMocks()
  document.documentElement.removeAttribute("style")
  document.body.removeAttribute("style")
  document.getElementById("__xero-browser-pen-document-layer")?.remove()
  setWindowScroll(0, 0)
  cookieStorage?.clear()
})

describe("BrowserSidebar", () => {
  it("collects dropdown intersections as browser webview occlusion rects", () => {
    const viewport = document.createElement("div")
    const dropdown = document.createElement("div")
    const hiddenDropdown = document.createElement("div")

    dropdown.dataset.slot = "dropdown-menu-content"
    hiddenDropdown.dataset.slot = "dropdown-menu-content"
    hiddenDropdown.style.display = "none"

    viewport.getBoundingClientRect = vi.fn(() =>
      rect({
        bottom: 400,
        height: 300,
        left: 100,
        right: 500,
        top: 100,
        width: 400,
      }),
    )
    dropdown.getBoundingClientRect = vi.fn(() =>
      rect({
        bottom: 160,
        height: 70,
        left: 80,
        right: 180,
        top: 90,
        width: 100,
      }),
    )
    hiddenDropdown.getBoundingClientRect = vi.fn(() =>
      rect({
        bottom: 260,
        height: 70,
        left: 120,
        right: 220,
        top: 190,
        width: 100,
      }),
    )

    document.body.append(viewport, dropdown, hiddenDropdown)

    expect(collectBrowserOverlayOcclusionRects(viewport, 6)).toEqual([
      { x: 0, y: 0, width: 82, height: 68 },
    ])
  })

  it("sends overlay occlusion rects to the native browser webview", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Example",
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_set_occlusion_regions", async () => null)

    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      if (this.dataset.slot === "dropdown-menu-content") {
        return rect({
          bottom: 260,
          height: 100,
          left: 900,
          right: 1160,
          top: 160,
          width: 260,
        })
      }

      return rect({
        bottom: 800,
        height: 600,
        left: 1000,
        right: 1600,
        top: 200,
        width: 600,
      })
    })

    render(<BrowserSidebar open />)

    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    await waitFor(() => expect(input.value).toBe("https://example.com/"))

    const dropdown = document.createElement("div")
    dropdown.dataset.slot = "dropdown-menu-content"

    await act(async () => {
      document.body.appendChild(dropdown)
      await new Promise((resolve) => window.requestAnimationFrame(resolve))
    })

    await waitFor(() => {
      const occlusionCall = invokeCalls
        .filter((call) => call.command === "browser_set_occlusion_regions")
        .at(-1)
      expect(occlusionCall?.args).toEqual({
        tabId: "tab-1",
        rects: [{ x: 0, y: 0, width: 162, height: 68 }],
      })
    })
  })

  it("coalesces repeated browser events by tab before applying them", () => {
    let flush: () => void = () => undefined
    const urlUpdates: string[] = []
    const loadUpdates: boolean[] = []
    const tabUpdates: number[] = []
    const coalescer = createBrowserEventCoalescer({
      onUrlChanged: (payload) => urlUpdates.push(payload.url),
      onLoadState: (payload) => loadUpdates.push(payload.loading),
      onTabUpdated: (payload) => tabUpdates.push(payload.tabs.length),
      schedule: (callback) => {
        flush = callback
        return () => {
          flush = () => undefined
        }
      },
    })

    coalescer.enqueueUrlChanged({
      tabId: "tab-1",
      url: "https://example.com/old",
      title: null,
      canGoBack: false,
      canGoForward: false,
    })
    coalescer.enqueueUrlChanged({
      tabId: "tab-1",
      url: "https://example.com/new",
      title: "New",
      canGoBack: true,
      canGoForward: false,
    })
    coalescer.enqueueLoadState({
      tabId: "tab-1",
      loading: true,
      url: "https://example.com/new",
      error: null,
    })
    coalescer.enqueueLoadState({
      tabId: "tab-1",
      loading: false,
      url: "https://example.com/new",
      error: null,
    })
    coalescer.enqueueTabUpdated({
      tabs: [
        {
          id: "tab-1",
          label: "xero-browser-tab-1",
          title: "New",
          url: "https://example.com/new",
          loading: false,
          canGoBack: true,
          canGoForward: false,
          active: true,
        },
      ],
    })

    flush()

    expect(urlUpdates).toEqual(["https://example.com/new"])
    expect(loadUpdates).toEqual([false])
    expect(tabUpdates).toEqual([1])
  })

  it("hydrates existing tabs when opened", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Example",
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)

    await waitFor(() => {
      const input = screen.getByLabelText("Address") as HTMLInputElement
      expect(input.value).toBe("https://example.com/")
    })
  })

  it("submits a URL and invokes browser_show with the expected shape", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)

    const input = await screen.findByLabelText("Address")
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: "example.com" } })
    const form = input.closest("form")!
    fireEvent.submit(form)

    await waitFor(() => {
      expect(shownUrls).toEqual(["https://example.com"])
    })
  })

  it("submits localhost URLs as IPv4 loopback for the embedded WebView", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)

    const input = await screen.findByLabelText("Address")
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: "http://localhost:4200/" } })
    const form = input.closest("form")!
    fireEvent.submit(form)

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:4200/"])
    })
  })

  it("treats bare localhost ports as URLs instead of search queries", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)

    const input = await screen.findByLabelText("Address")
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: "localhost:4200" } })
    const form = input.closest("form")!
    fireEvent.submit(form)

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:4200/"])
    })
  })

  it("opens a detected project app from the browser header", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(
      <BrowserSidebar
        open
        projectBrowserTargets={[
          {
            id: "browser-app:http://127.0.0.1:5173/",
            label: "web · localhost:5173",
            url: "http://127.0.0.1:5173/",
            source: "web",
            detectedAt: 1,
          },
        ]}
      />,
    )

    fireEvent.click(await screen.findByRole("button", { name: "Open project app in browser" }))

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
    })
  })

  it("opens a pending in-app browser URL request", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })
    const onConsumed = vi.fn()

    render(
      <BrowserSidebar
        open
        pendingOpenUrl={{ id: "open-1", url: "http://localhost:5173/" }}
        onPendingOpenUrlConsumed={onConsumed}
      />,
    )

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
      expect(onConsumed).toHaveBeenCalledWith("open-1")
    })
  })

  it("waits for the opening sidebar geometry before opening a pending URL", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })
    const pendingOpenUrl = { id: "open-while-closed", url: "http://localhost:5173/" }
    const onConsumed = vi.fn()
    const { rerender } = render(
      <BrowserSidebar
        open={false}
        pendingOpenUrl={pendingOpenUrl}
        onPendingOpenUrlConsumed={onConsumed}
      />,
    )

    rerender(
      <BrowserSidebar
        open
        pendingOpenUrl={pendingOpenUrl}
        onPendingOpenUrlConsumed={onConsumed}
      />,
    )

    expect(shownUrls).toEqual([])

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
      expect(onConsumed).toHaveBeenCalledWith("open-while-closed")
    })
  })

  it("enables back and forward buttons whenever a tab is active and dispatches the right command", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    const invoked: string[] = []
    registerInvoke("browser_back", async () => {
      invoked.push("back")
      return null
    })
    registerInvoke("browser_forward", async () => {
      invoked.push("forward")
      return null
    })

    render(<BrowserSidebar open />)

    // Wait for hydration so activeTab is set; once it is, both buttons should be
    // clickable (the webview safely no-ops at history endpoints).
    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    await waitFor(() => expect(input.value).toBe("https://example.com/"))

    const back = await screen.findByLabelText("Back")
    const forward = await screen.findByLabelText("Forward")
    await waitFor(() => expect(back).not.toBeDisabled())
    await waitFor(() => expect(forward).not.toBeDisabled())

    fireEvent.click(back)
    await waitFor(() => expect(invoked).toContain("back"))
    fireEvent.click(forward)
    await waitFor(() => expect(invoked).toContain("forward"))
  })

  it("disables back and forward when no tab is active", async () => {
    registerInvoke("browser_tab_list", async () => [])
    render(<BrowserSidebar open />)
    const back = await screen.findByLabelText("Back")
    const forward = await screen.findByLabelText("Forward")
    expect(back).toBeDisabled()
    expect(forward).toBeDisabled()
  })

  it("clears toolbar loading state when the active browser tab is closed natively", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://127.0.0.1:5173/",
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)

    await screen.findByLabelText("Stop")
    await act(async () => {
      emitEvent("browser:tab_updated", { tabs: [] })
    })

    await waitFor(() => expect(screen.getByLabelText("Reload")).toBeDisabled())
    expect((screen.getByLabelText("Address") as HTMLInputElement).value).toBe("")
  })

  it("dispatches stop while loading and reload once the page is idle", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://127.0.0.1:5173/",
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    const commands: string[] = []
    registerInvoke("browser_stop", async () => {
      commands.push("stop")
      return null
    })
    registerInvoke("browser_reload", async () => {
      commands.push("reload")
      return null
    })

    render(<BrowserSidebar open />)
    fireEvent.click(await screen.findByLabelText("Stop"))
    await waitFor(() => expect(commands).toEqual(["stop"]))

    fireEvent.click(await screen.findByLabelText("Reload"))
    await waitFor(() => expect(commands).toEqual(["stop", "reload"]))
  })

  it("exposes the new-tab button as soon as a single tab exists", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return {
        id: "tab-2",
        label: "xero-browser-tab-2",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)
    const newTabButton = await screen.findByLabelText("New tab")
    expect(newTabButton).toBeVisible()
    fireEvent.click(newTabButton)
    await waitFor(() => expect(shownUrls.length).toBe(1))
  })

  it("sends newTab=true and no tabId when the + button is clicked so the existing tab is not reused", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    let recordedArgs: Record<string, unknown> | null = null
    registerInvoke("browser_show", async (args) => {
      recordedArgs = (args as Record<string, unknown>) ?? null
      return {
        id: "tab-2",
        label: "xero-browser-tab-2",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)
    const newTabButton = await screen.findByLabelText("New tab")
    fireEvent.click(newTabButton)
    await waitFor(() => expect(recordedArgs).not.toBeNull())
    expect(recordedArgs!.newTab).toBe(true)
    expect(recordedArgs!.tabId).toBeNull()
  })

  it("applies the resize handle inset to browser_show so the handle stays clickable", async () => {
    registerInvoke("browser_tab_list", async () => [])
    let recordedArgs: Record<string, unknown> | null = null
    registerInvoke("browser_show", async (args) => {
      recordedArgs = (args as Record<string, unknown>) ?? null
      return {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open />)
    const input = await screen.findByLabelText("Address")
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: "https://example.com" } })
    const form = input.closest("form")!
    fireEvent.submit(form)

    await waitFor(() => expect(recordedArgs).not.toBeNull())
    // The inset is 6px; the webview must start at least that far from the sidebar's left edge.
    expect(Number(recordedArgs!.x)).toBeGreaterThanOrEqual(6)
  })

  it("starts native drag tracking without sending stale pointermove resize IPC", async () => {
    registerInvoke("browser_tab_list", async () => [])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)
    registerInvoke("browser_show", async (args) => ({
      id: "tab-1",
      label: "xero-browser-tab-1",
      title: null,
      url: String((args as { url?: string })?.url ?? ""),
      loading: true,
      canGoBack: false,
      canGoForward: false,
      active: true,
    }))

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      const input = await screen.findByLabelText("Address")
      fireEvent.focus(input)
      fireEvent.change(input, { target: { value: "https://example.com" } })
      fireEvent.submit(input.closest("form")!)

      await act(async () => {
        emitEvent("browser:tab_updated", {
          tabs: [
            {
              id: "tab-1",
              label: "xero-browser-tab-1",
              title: null,
              url: "https://example.com",
              loading: true,
              canGoBack: false,
              canGoForward: false,
              active: true,
            },
          ],
        })
      })

      await waitFor(() =>
        expect(invokeCalls.some((call) => call.command === "browser_show")).toBe(true),
      )
      invokeCalls.length = 0

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })
      await waitFor(() => {
        const dragStart = invokeCalls.find(
          (call) => call.command === "browser_resize_drag_start",
        )
        expect(dragStart?.args).toMatchObject({
          startClientX: 760,
          startWidth: 640,
          right: 1400,
          top: 100,
          height: 800,
          minWidth: 320,
          maxWidth: 1400,
          inset: 6,
          tabId: "tab-1",
        })
      })
      await screen.findByTitle("https://example.com")
      invokeCalls.length = 0

      fireEvent.pointerMove(window, { clientX: 680 })

      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)

      fireEvent.pointerUp(window)

      await waitFor(() => {
        const dragEnd = invokeCalls.find(
          (call) => call.command === "browser_resize_drag_end",
        )
        expect(dragEnd?.args).toMatchObject({
          x: 686,
          y: 100,
          width: 714,
          height: 800,
          tabId: "tab-1",
        })
      })
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("resizes hydrated browser tabs that were already open", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Example",
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      const input = (await screen.findByLabelText("Address")) as HTMLInputElement
      await waitFor(() => expect(input.value).toBe("https://example.com/"))
      invokeCalls.length = 0

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })
      await waitFor(() =>
        expect(
          invokeCalls.some((call) => call.command === "browser_resize_drag_start"),
        ).toBe(true),
      )
      invokeCalls.length = 0

      fireEvent.pointerMove(window, { clientX: 680 })

      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)

      fireEvent.pointerUp(window)
      await waitFor(() => {
        const dragEnd = invokeCalls.find(
          (call) => call.command === "browser_resize_drag_end",
        )
        expect(dragEnd?.args).toMatchObject({
          x: 686,
          width: 714,
          tabId: "tab-1",
        })
      })
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("starts native drag tracking for tabs announced by backend events", async () => {
    registerInvoke("browser_tab_list", async () => [])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      await screen.findByLabelText("Address")

      await act(async () => {
        emitEvent("browser:tab_updated", {
          tabs: [
            {
              id: "tab-1",
              label: "xero-browser-tab-1",
              title: null,
              url: "https://example.com",
              loading: false,
              canGoBack: false,
              canGoForward: false,
              active: true,
            },
          ],
        })
      })
      await screen.findByTitle("https://example.com")
      invokeCalls.length = 0

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })

      await waitFor(() => {
        const dragStart = invokeCalls.find(
          (call) => call.command === "browser_resize_drag_start",
        )
        expect(dragStart?.args).toMatchObject({
          tabId: "tab-1",
          startClientX: 760,
          startWidth: 640,
        })
      })
      invokeCalls.length = 0

      fireEvent.pointerMove(window, { clientX: 680 })
      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("applies native resize drag events when the webview swallows pointerup", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Example",
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      const input = (await screen.findByLabelText("Address")) as HTMLInputElement
      await waitFor(() => expect(input.value).toBe("https://example.com/"))
      await waitFor(() =>
        expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(
          true,
        ),
      )
      invokeCalls.length = 0

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })
      await waitFor(() =>
        expect(
          invokeCalls.some((call) => call.command === "browser_resize_drag_start"),
        ).toBe(true),
      )
      invokeCalls.length = 0

      await act(async () => {
        emitEvent("browser:resize_drag", {
          tabId: "tab-1",
          sidebarWidth: 720,
          x: 686,
          y: 100,
          width: 714,
          height: 800,
          complete: true,
        })
      })

      await waitFor(() => {
        const dragEnd = invokeCalls.find(
          (call) => call.command === "browser_resize_drag_end",
        )
        expect(dragEnd?.args).toMatchObject({
          tabId: "tab-1",
          x: 686,
          y: 100,
          width: 714,
          height: 800,
        })
      })
      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)
      expect(document.body.style.cursor).toBe("")
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("blocks sidebar resizing as soon as pen mode is active", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      const penButton = await screen.findByLabelText("Sketch on page")

      fireEvent.click(penButton)

      await waitFor(() => expect(penButton).toHaveAttribute("aria-pressed", "true"))

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      expect(separator.getAttribute("aria-disabled")).toBe("true")
      expect(separator.className).toContain("cursor-not-allowed")

      invokeCalls.length = 0
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })
      fireEvent.keyDown(separator, { key: "ArrowLeft" })

      expect(invokeCalls.some((call) => call.command === "browser_resize_drag_start")).toBe(false)
      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)

      fireEvent.click(penButton)

      await waitFor(() => expect(penButton).toHaveAttribute("aria-pressed", "false"))

      expect(separator.getAttribute("aria-disabled")).toBeNull()
      expect(separator.className).toContain("cursor-col-resize")
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("blocks sidebar resizing while a pen drawing exists", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_resize", async () => null)
    registerInvoke("browser_resize_drag_start", async () => null)
    registerInvoke("browser_resize_drag_end", async () => null)

    const originalInnerWidth = window.innerWidth
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1600,
    })
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 760,
      y: 100,
      left: 760,
      top: 100,
      right: 1400,
      bottom: 900,
      width: 640,
      height: 800,
      toJSON: () => ({}),
    } as DOMRect)

    try {
      render(<BrowserSidebar open />)
      await screen.findByDisplayValue("http://localhost:5173/")

      await act(async () => {
        emitEvent("browser:tool_state", {
          tabId: "tab-1",
          mode: "pen",
          strokeCount: 1,
          hasDrawing: true,
        })
      })

      const separator = screen.getByRole("separator", {
        name: "Resize browser sidebar",
      })
      expect(separator.getAttribute("aria-disabled")).toBe("true")
      expect(separator.className).toContain("cursor-not-allowed")

      invokeCalls.length = 0
      fireEvent.pointerDown(separator, { button: 0, clientX: 760 })
      fireEvent.keyDown(separator, { key: "ArrowLeft" })

      expect(invokeCalls.some((call) => call.command === "browser_resize_drag_start")).toBe(false)
      expect(invokeCalls.some((call) => call.command === "browser_resize")).toBe(false)
      expect(document.body.style.cursor).toBe("")

      await act(async () => {
        emitEvent("browser:tool_state", {
          tabId: "tab-1",
          mode: "pen",
          strokeCount: 0,
          hasDrawing: false,
        })
      })

      expect(separator.getAttribute("aria-disabled")).toBeNull()
      expect(separator.className).toContain("cursor-col-resize")
    } finally {
      Object.defineProperty(window, "innerWidth", {
        configurable: true,
        value: originalInnerWidth,
      })
    }
  })

  it("shows the cookie-import banner once a tab exists and a source is available, then dispatches browser_import_cookies", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_list_cookie_sources", async () => [
      { id: "chrome", label: "Google Chrome", available: true },
      { id: "firefox", label: "Firefox", available: false },
    ])
    const importCalls: Array<Record<string, unknown> | undefined> = []
    registerInvoke("browser_import_cookies", async (args) => {
      importCalls.push(args)
      return { source: "chrome", imported: 42, skipped: 1, domains: 7 }
    })

    render(<BrowserSidebar open />)

    const btn = await screen.findByRole("button", { name: "Google Chrome" })
    expect(btn).toBeVisible()
    // Unavailable source shouldn't render as a button.
    expect(screen.queryByRole("button", { name: "Firefox" })).toBeNull()

    fireEvent.click(btn)
    await waitFor(() => expect(importCalls.length).toBe(1))
    expect(importCalls[0]).toMatchObject({ source: "chrome" })

    // Success summary appears
    await waitFor(() =>
      expect(screen.getByText(/Imported 42 cookies/i)).toBeInTheDocument(),
    )

    // Banner is dismissible and sets the "prompted" flag so it stays closed.
    expect(window.localStorage.getItem("xero.browser.cookieImportPrompted")).toBe(
      "true",
    )
  })

  it("does not show the cookie-import banner when the prompted flag is already set", async () => {
    window.localStorage.setItem("xero.browser.cookieImportPrompted", "true")
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_list_cookie_sources", async () => [
      { id: "chrome", label: "Google Chrome", available: true },
    ])

    render(<BrowserSidebar open />)

    // Give the effect a chance to run.
    await screen.findByLabelText("Address")
    // Banner would render a "Google Chrome" action button; the toolbar doesn't.
    expect(screen.queryByRole("button", { name: "Google Chrome" })).toBeNull()
  })

  it("updates the address bar when a load_state event delivers a new URL while unfocused", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: null,
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)
    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    // Wait for the initial URL so activeTabId is set before emitting load state.
    await waitFor(() => expect(input.value).toBe("https://example.com/"))

    await act(async () => {
      emitEvent("browser:load_state", {
        tabId: "tab-1",
        loading: false,
        url: "https://example.com/changed",
        error: null,
      })
    })

    await waitFor(() => expect(input.value).toBe("https://example.com/changed"))
  })

  it("hides the dev-server tools toolbar on a non-localhost tab", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Example",
        url: "https://example.com/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)
    await screen.findByLabelText("Address")

    expect(screen.queryByLabelText("Sketch on page")).toBeNull()
    expect(screen.queryByLabelText("Inspect element")).toBeNull()
  })

  it("reveals the pen and inspect tools when the active tab is on a dev server", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)

    expect(await screen.findByLabelText("Sketch on page")).toBeInTheDocument()
    expect(await screen.findByLabelText("Inspect element")).toBeInTheDocument()
  })

  it("toggles pen mode by injecting the tool into the live webview; clicking again exits", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://127.0.0.1:3000/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)

    document.documentElement.style.setProperty("--popover", "#123456")
    document.documentElement.style.setProperty("--popover-foreground", "#abcdef")

    const penButton = await screen.findByLabelText("Sketch on page")
    fireEvent.click(penButton)

    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes('"mode":"pen"'),
        ),
      ).toBe(true)
    })
    const activationCall = invokeCalls.find(
      (call) =>
        call.command === "browser_eval_fire_and_forget" &&
        String(call.args?.js ?? "").includes('"mode":"pen"'),
    )
    expect(activationCall?.args).not.toHaveProperty("timeout_ms")
    expect(String(activationCall?.args?.js ?? "")).toContain('"popover":"#123456"')
    expect(String(activationCall?.args?.js ?? "")).toContain('"popoverForeground":"#abcdef"')
    expect(String(activationCall?.args?.js ?? "")).toContain("bestComposerPlacement")
    expect(String(activationCall?.args?.js ?? "")).toContain(".composer[data-closing='true']")
    expect(penButton).toHaveAttribute("aria-pressed", "true")
    expect(invokeCalls.some((call) => call.command === "browser_hide")).toBe(false)

    fireEvent.click(penButton)
    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes("deactivate"),
        ),
      ).toBe(true)
    })
    expect(penButton).toHaveAttribute("aria-pressed", "false")
  })

  it("injects inspect mode into the active dev-server webview", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:8080/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)
    const inspectButton = await screen.findByLabelText("Inspect element")
    fireEvent.click(inspectButton)

    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes('"mode":"inspect"'),
        ),
      ).toBe(true)
    })
    expect(inspectButton).toHaveAttribute("aria-pressed", "true")
  })

  it("switching from pen to inspect replaces the injected browser tool", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)
    fireEvent.click(await screen.findByLabelText("Sketch on page"))
    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes('"mode":"pen"'),
        ),
      ).toBe(true)
    })

    fireEvent.click(screen.getByLabelText("Inspect element"))
    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes('"mode":"inspect"'),
        ),
      ).toBe(true)
    })
    expect(screen.getByLabelText("Sketch on page")).toHaveAttribute("aria-pressed", "false")
    expect(screen.getByLabelText("Inspect element")).toHaveAttribute("aria-pressed", "true")
  })

  it("clears tool mode when the URL changes off the dev server", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open />)
    fireEvent.click(await screen.findByLabelText("Sketch on page"))
    await waitFor(() => expect(screen.getByLabelText("Sketch on page")).toHaveAttribute("aria-pressed", "true"))

    await act(async () => {
      emitEvent("browser:url_changed", {
        tabId: "tab-1",
        url: "https://example.com/",
        title: "Example",
        canGoBack: true,
        canGoForward: false,
      })
    })

    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes("deactivate"),
        ),
      ).toBe(true)
    })
    await waitFor(() => expect(screen.queryByLabelText("Sketch on page")).toBeNull())
  })

  it("adds selected element context without taking a screenshot", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    fireEvent.click(await screen.findByLabelText("Inspect element"))
    const callCountBeforeSubmit = invokeCalls.length

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "inspect",
          note: "Tighten the spacing here",
          page: { url: "http://localhost:5173/", title: "Local" },
          viewport: { width: 800, height: 600 },
          element: {
            selector: "button.cta",
            tagName: "button",
            id: null,
            classes: ["cta"],
            role: "button",
            label: "Start",
            text: "Start",
            attributes: [
              { name: "data-testid", value: "hero-cta" },
            ],
            ancestors: [
              {
                selector: "section.hero",
                tagName: "section",
                id: null,
                role: null,
                label: "Hero",
              },
            ],
            source: {
              framework: "react",
              componentName: "HeroCta",
              filePath: "/app/src/components/HeroCta.tsx",
              lineNumber: 42,
              columnNumber: 7,
              raw: "/app/src/components/HeroCta.tsx:42:7",
            },
            rect: { x: 20, y: 40, width: 120, height: 36 },
          },
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    const request = addedRequests[0]!
    expect(request.prompt).toContain("Browser element inspection context")
    expect(request.prompt).toContain("local dev server /")
    expect(request.prompt).not.toContain("localhost:")
    expect(request.prompt).toContain("Selector: button.cta")
    expect(request.prompt).toContain("for locating code; no screenshot")
    expect(request.prompt).toContain("Source: /app/src/components/HeroCta.tsx:42:7")
    expect(request.prompt).toContain('Stable attrs: data-testid="hero-cta"')
    expect(request.prompt).toContain('Parent chain: <section> section.hero label "Hero"')
    expect(request.prompt).not.toContain("DOM snippet")
    expect(request.prompt).not.toContain("Tighten the spacing here")
    expect(request.visiblePrompt).toBe("Tighten the spacing here")
    expect(request.contextCard).toEqual({
      kind: "element",
      title: "Element context",
      subtitle: "HeroCta.tsx:42",
    })
    expect(request.image).toBeUndefined()
    const submitCalls = invokeCalls.slice(callCountBeforeSubmit)
    expect(submitCalls.some((call) => call.command === "browser_screenshot")).toBe(false)
    expect(
      submitCalls.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("prepareCapture"),
      ),
    ).toBe(false)
    expect(
      submitCalls.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("deactivate"),
      ),
    ).toBe(true)
  })

  it("captures submitted pen context and adds it to the agent composer", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    fireEvent.click(await screen.findByLabelText("Sketch on page"))

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "Tighten the spacing here",
          page: { url: "http://localhost:5173/", title: "Local" },
          viewport: { width: 800, height: 600 },
          strokeCount: 1,
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    const request = addedRequests[0]!
    expect(request.prompt).toContain("Browser sketch context")
    expect(request.visiblePrompt).toBe("Tighten the spacing here")
    expect(request.contextCard).toEqual({
      kind: "sketch",
      title: "Browser sketch context",
      subtitle: "1 stroke on browser screenshot",
    })
    expect(request.image).toBeTruthy()
    expect(Array.from(request.image!.bytes)).toEqual([104, 101, 108, 108, 111])
    expect(request.image!.originalName).toMatch(/^browser-pen-/)
    const prepareIndex = invokeCalls.findIndex(
      (call) =>
        call.command === "browser_eval_fire_and_forget" &&
        String(call.args?.js ?? "").includes("prepareCapture"),
    )
    const screenshotIndex = invokeCalls.findIndex(
      (call) => call.command === "browser_screenshot",
    )
    const deactivateIndex = invokeCalls.findIndex(
      (call, index) =>
        index > screenshotIndex &&
        call.command === "browser_eval_fire_and_forget" &&
        String(call.args?.js ?? "").includes("deactivate"),
    )
    expect(prepareIndex).toBeGreaterThanOrEqual(0)
    expect(screenshotIndex).toBeGreaterThan(prepareIndex)
    expect(deactivateIndex).toBeGreaterThan(screenshotIndex)
  })

  it("shows the browser capture overlay while submitted context is being prepared", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    let resolveScreenshot: ((value: string) => void) | null = null
    const screenshotStarted = new Promise<void>((resolveStarted) => {
      registerInvoke("browser_screenshot", async () => {
        resolveStarted()
        return new Promise<string>((resolve) => {
          resolveScreenshot = resolve
        })
      })
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "Attach this sketch",
          page: { url: "http://localhost:5173/", title: "Local" },
          strokeCount: 1,
          viewport: { width: 800, height: 600 },
        },
      })
    })

    const captureStatus = await screen.findByRole("status", { name: "Adding browser context" })
    expect(captureStatus).toBeVisible()
    expect(captureStatus.querySelector(".xero-browser-capture-aura-field")).not.toBeNull()
    expect(captureStatus.querySelectorAll(".xero-browser-capture-occlusion")).toHaveLength(8)
    expect(
      Array.from(captureStatus.querySelectorAll(".xero-browser-capture-occlusion")).every(
        (element) => element.getAttribute("data-xero-browser-capture-overlay") === "true",
      ),
    ).toBe(true)
    expect(captureStatus.querySelector(".xero-loading-symbol")).toBeNull()
    await screenshotStarted
    await act(async () => {
      resolveScreenshot?.("aGVsbG8=")
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "Adding browser context" })).toBeNull(),
    )
    expect(addedRequests[0]?.visiblePrompt).toBe("Attach this sketch")
  })

  it("accepts raw browser tool context events and adds them to the agent composer", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        kind: "pen",
        note: "Keep this arrow attached",
        page: { url: "http://localhost:5173/", title: "Local" },
        strokeCount: 1,
        viewport: { width: 800, height: 600 },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    const request = addedRequests[0]!
    expect(request.prompt).toContain("Browser sketch context")
    expect(request.visiblePrompt).toBe("Keep this arrow attached")
    expect(request.image).toBeTruthy()
    expect(Array.from(request.image!.bytes)).toEqual([104, 101, 108, 108, 111])
  })

  it("adds browser context without requiring bridge-backed eval during capture prep", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_eval", async () => {
      throw new Error("bridge not installed")
    })
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        tab_id: "tab-1",
        context: {
          kind: "pen",
          note: "Still attach this",
          page: { url: "http://localhost:5173/", title: "Local" },
          strokeCount: 1,
          viewport: { width: 800, height: 600 },
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    expect(addedRequests[0]?.visiblePrompt).toBe("Still attach this")
    expect(
      invokeCalls.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("prepareCapture"),
      ),
    ).toBe(true)
  })

  it("still adds the browser note when screenshot capture fails", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => {
      throw new Error("screenshot failed")
    })

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        tab_id: "tab-1",
        context: {
          kind: "pen",
          note: "Keep this arrow attached",
          page: { url: "http://localhost:5173/", title: "Local" },
          strokeCount: 1,
          viewport: { width: 800, height: 600 },
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    expect(addedRequests[0]?.visiblePrompt).toBe("Keep this arrow attached")
    expect(addedRequests[0]?.prompt).toContain("Browser sketch context")
    expect(addedRequests[0]?.prompt).toContain("screenshot could not be captured")
    expect(addedRequests[0]?.image).toBeUndefined()
  })

  it("accepts native browser tool context envelopes with bridge metadata", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Local",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        tab_id: "tab-1",
        context: {
          protocolVersion: "xero.in_app_browser_bridge.v1",
          sequence: 12,
          navigationGeneration: 1,
          mutationGeneration: 0,
          kind: "pen",
          note: "Use this exact spot",
          page: { url: "http://localhost:5173/", title: "Local" },
          strokeCount: 2,
          viewport: { width: 800, height: 600 },
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    expect(addedRequests[0]?.visiblePrompt).toBe("Use this exact spot")
  })

  it("redacts local dev-server URLs from browser tool prompts", () => {
    const prompt = buildBrowserToolAgentPrompt({
      kind: "pen",
      note: "Make the heading tighter",
      page: { url: "http://localhost:5173/oauth/callback?code=abc", title: "Local App" },
      strokeCount: 1,
      viewport: { width: 800, height: 600 },
    })

    expect(prompt).toContain("Local App (local dev server /oauth/callback)")
    expect(prompt).not.toContain("localhost:")
    expect(prompt).not.toContain("code=abc")
  })

  it("keeps browser tool notes as the only visible composer text", () => {
    const visiblePrompt = buildBrowserToolVisiblePrompt({
      kind: "pen",
      note: "Make the heading tighter",
      page: { url: "http://localhost:5173/oauth/callback?code=abc", title: "Local App" },
      strokeCount: 1,
      viewport: { width: 800, height: 600 },
    })

    expect(visiblePrompt).toBe("Make the heading tighter")
  })

  it("renders pen strokes with blended rainbow gradients", () => {
    const theme: BrowserToolTheme = {
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

    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    expect(script).toContain("linearGradient")
    expect(script).toContain('gradient.setAttribute("gradientUnits", "userSpaceOnUse")')
    expect(script).toContain("#ff2d55")
    expect(script).toContain("#34c759")
    expect(script).toContain("#ff2dff")
    expect(script).toContain('stylePenPath(path, "url(#" + gradientId + ")")')
  })

  it("emits browser tool context through Tauri internals when the page bridge is unavailable", async () => {
    const originalTauriInternals = Object.getOwnPropertyDescriptor(
      window,
      "__TAURI_INTERNALS__",
    )
    const originalBridge = Object.getOwnPropertyDescriptor(window, "__xeroBridge__")
    const invoke = vi.fn(async (_command: string, _args?: Record<string, unknown>) => null)
    const theme: BrowserToolTheme = {
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
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke },
    })
    Object.defineProperty(window, "__xeroBridge__", {
      configurable: true,
      value: undefined,
    })

    try {
      new Function(script)()
      const toolHost = document.getElementById("__xero-browser-tool-root")
      const shadow = toolHost?.shadowRoot
      const overlay = shadow?.querySelector(".pen-layer")
      expect(overlay).toBeTruthy()

      dispatchPointer(overlay!, "pointerdown", { clientX: 100, clientY: 100 })
      dispatchPointer(overlay!, "pointermove", { clientX: 140, clientY: 110 })
      dispatchPointer(overlay!, "pointerup", { clientX: 180, clientY: 120 })

      const textarea = shadow?.querySelector<HTMLTextAreaElement>(".composer-input")
      const send = shadow?.querySelector<HTMLButtonElement>(".send-button")
      expect(textarea).toBeTruthy()
      expect(send).toBeTruthy()

      textarea!.value = "Keep this attached"
      send!.click()

      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith(
          "browser_internal_event",
          expect.objectContaining({
            kind: "tool_context",
            payload: expect.any(String),
          }),
        ),
      )
      const contextCall = invoke.mock.calls.find(
        ([command, args]) =>
          command === "browser_internal_event" &&
          (args as { kind?: unknown } | undefined)?.kind === "tool_context",
      )
      const payload = JSON.parse(
        String((contextCall?.[1] as { payload?: unknown } | undefined)?.payload ?? "{}"),
      ) as { kind?: unknown; note?: unknown; strokeCount?: unknown }
      expect(payload.kind).toBe("pen")
      expect(payload.note).toBe("Keep this attached")
      expect(payload.strokeCount).toBe(1)
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      if (originalTauriInternals) {
        Object.defineProperty(window, "__TAURI_INTERNALS__", originalTauriInternals)
      } else {
        delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
      }
      if (originalBridge) {
        Object.defineProperty(window, "__xeroBridge__", originalBridge)
      } else {
        delete (window as unknown as { __xeroBridge__?: unknown }).__xeroBridge__
      }
    }
  })

  it("commits pen strokes into one document-space SVG layer without resize scaling", async () => {
    const originalWidth = window.innerWidth
    const originalHeight = window.innerHeight
    let restoreDocumentSize = () => {}
    const theme: BrowserToolTheme = {
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
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    try {
      setWindowScroll(0, 0)
      setWindowInnerSize(800, 600)
      restoreDocumentSize = setDocumentSize(1200, 1600)
      new Function(script)()

      const toolHost = document.getElementById("__xero-browser-tool-root")
      const shadow = toolHost?.shadowRoot
      const overlay = shadow?.querySelector(".pen-layer")
      const documentLayer = document.getElementById("__xero-browser-pen-document-layer")
      expect(overlay).toBeTruthy()
      expect(documentLayer).toBeTruthy()
      expect(documentLayer?.parentElement).toBe(document.body)
      expect(documentLayer?.getAttribute("data-xero-browser-tool-document-layer")).toBe("true")
      expect(overlay?.getAttribute("viewBox")).toBe("0 0 800 600")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 0 1200 1600")
      expect(documentLayer?.getAttribute("width")).toBe("1200")
      expect((documentLayer as SVGSVGElement | null)?.style.width).toBe("1200px")

      dispatchPointer(overlay!, "pointerdown", { clientX: 100, clientY: 100 })
      dispatchPointer(overlay!, "pointermove", { clientX: 140, clientY: 110 })
      dispatchPointer(overlay!, "pointerup", { clientX: 180, clientY: 120 })

      expect(shadow?.querySelector(".pen-path")).toBeNull()
      const path = documentLayer?.querySelector(".xero-document-pen-path")
      expect(path?.getAttribute("d")).toContain("M 100 100")
      expect(path?.getAttribute("d")).toContain("L 180 120")

      setWindowInnerSize(400, 600)
      restoreDocumentSize()
      restoreDocumentSize = setDocumentSize(900, 1600)
      await act(async () => {
        window.dispatchEvent(new Event("resize"))
        await new Promise((resolve) => window.requestAnimationFrame(resolve))
      })

      expect(overlay?.getAttribute("viewBox")).toBe("0 0 400 600")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 0 900 1600")
      expect(documentLayer?.getAttribute("width")).toBe("900")
      expect(path?.getAttribute("d")).toContain("M 100 100")
      expect(path?.getAttribute("d")).toContain("L 180 120")
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      restoreDocumentSize()
      setWindowInnerSize(originalWidth, originalHeight)
      setWindowScroll(0, 0)
    }
  })

  it("records scrolled drawings in document coordinates instead of viewport coordinates", async () => {
    const originalWidth = window.innerWidth
    const originalHeight = window.innerHeight
    let restoreDocumentSize = () => {}
    const theme: BrowserToolTheme = {
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
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    try {
      setWindowScroll(0, 0)
      setWindowInnerSize(800, 600)
      restoreDocumentSize = setDocumentSize(1000, 1800)
      new Function(script)()

      const toolHost = document.getElementById("__xero-browser-tool-root")
      const shadow = toolHost?.shadowRoot
      const overlay = shadow?.querySelector(".pen-layer")
      const documentLayer = document.getElementById("__xero-browser-pen-document-layer")
      expect(overlay).toBeTruthy()
      expect(documentLayer).toBeTruthy()

      setWindowScroll(0, 300)
      dispatchPointer(overlay!, "pointerdown", { clientX: 680, clientY: 320 })
      dispatchPointer(overlay!, "pointermove", { clientX: 715, clientY: 325 })
      dispatchPointer(overlay!, "pointerup", { clientX: 760, clientY: 340 })

      const path = documentLayer?.querySelector(".xero-document-pen-path")
      expect(path?.getAttribute("d")).toContain("M 680 620")
      expect(path?.getAttribute("d")).toContain("L 760 640")

      await act(async () => {
        setWindowScroll(0, 520)
        window.dispatchEvent(new Event("scroll"))
        await new Promise((resolve) => window.requestAnimationFrame(resolve))
      })

      expect(path?.getAttribute("d")).toContain("M 680 620")
      expect(path?.getAttribute("d")).toContain("L 760 640")
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      restoreDocumentSize()
      setWindowInnerSize(originalWidth, originalHeight)
      setWindowScroll(0, 0)
    }
  })

  it("keeps pen strokes attached to an inner scroll container", async () => {
    const scroller = document.createElement("div")
    const child = document.createElement("div")
    const originalElementFromPoint = Object.getOwnPropertyDescriptor(
      document,
      "elementFromPoint",
    )
    const restores = [
      setNumberProperty(scroller, "clientWidth", 320),
      setNumberProperty(scroller, "clientHeight", 240),
      setNumberProperty(scroller, "scrollWidth", 320),
      setNumberProperty(scroller, "scrollHeight", 1000),
    ]
    scroller.style.overflowY = "auto"
    child.textContent = "Scrollable content"
    scroller.appendChild(child)
    document.body.appendChild(scroller)
    vi.spyOn(scroller, "getBoundingClientRect").mockReturnValue(
      rect({
        bottom: 340,
        height: 240,
        left: 50,
        right: 370,
        top: 100,
        width: 320,
      }),
    )
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => child),
    })
    scroller.scrollTop = 200
    const theme: BrowserToolTheme = {
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
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    try {
      new Function(script)()
      const toolHost = document.getElementById("__xero-browser-tool-root")
      const overlay = toolHost?.shadowRoot?.querySelector(".pen-layer")
      expect(overlay).toBeTruthy()

      dispatchPointer(overlay!, "pointerdown", { clientX: 100, clientY: 150 })
      dispatchPointer(overlay!, "pointermove", { clientX: 140, clientY: 165 })
      dispatchPointer(overlay!, "pointerup", { clientX: 180, clientY: 175 })

      const documentLayer = document.getElementById("__xero-browser-pen-document-layer")
      expect(documentLayer?.parentElement).toBe(scroller)
      expect(scroller.style.position).toBe("relative")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 0 320 1000")

      const path = documentLayer?.querySelector(".xero-document-pen-path")
      expect(path?.getAttribute("d")).toContain("M 50 250")
      expect(path?.getAttribute("d")).toContain("L 130 275")

      scroller.scrollTop = 320
      await act(async () => {
        scroller.dispatchEvent(new Event("scroll", { bubbles: true }))
        await new Promise((resolve) => window.requestAnimationFrame(resolve))
      })

      expect(documentLayer?.parentElement).toBe(scroller)
      expect(path?.getAttribute("d")).toContain("M 50 250")
      expect(path?.getAttribute("d")).toContain("L 130 275")
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      for (let index = restores.length - 1; index >= 0; index -= 1) {
        restores[index]?.()
      }
      scroller.remove()
      if (originalElementFromPoint) {
        Object.defineProperty(document, "elementFromPoint", originalElementFromPoint)
      } else {
        delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint
      }
    }
  })

  it("forwards wheel scrolling through the pen overlay to the page", async () => {
    const scroller = document.createElement("div")
    const originalElementFromPoint = Object.getOwnPropertyDescriptor(
      document,
      "elementFromPoint",
    )
    scroller.style.overflowY = "auto"
    document.body.appendChild(scroller)
    Object.defineProperty(scroller, "clientHeight", {
      configurable: true,
      value: 200,
    })
    Object.defineProperty(scroller, "scrollHeight", {
      configurable: true,
      value: 1000,
    })
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => scroller),
    })
    const theme: BrowserToolTheme = {
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
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme,
    })

    try {
      new Function(script)()
      const toolHost = document.getElementById("__xero-browser-tool-root")
      const overlay = toolHost?.shadowRoot?.querySelector(".pen-layer")
      expect(overlay).toBeTruthy()

      overlay!.dispatchEvent(
        new WheelEvent("wheel", {
          bubbles: true,
          cancelable: true,
          clientX: 20,
          clientY: 20,
          deltaY: 120,
        }),
      )

      expect(scroller.scrollTop).toBe(120)
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      scroller.remove()
      if (originalElementFromPoint) {
        Object.defineProperty(document, "elementFromPoint", originalElementFromPoint)
      } else {
        delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint
      }
    }
  })
})
