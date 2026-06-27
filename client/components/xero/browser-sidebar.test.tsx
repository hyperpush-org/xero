/** @vitest-environment jsdom */

import { useState } from "react"
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
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
  BROWSER_TOOL_DICTATION_TOGGLE_EVENT,
  BROWSER_TOOL_NOTE_EVENT,
  buildBrowserToolActivationScript,
  buildBrowserToolAgentPrompt,
  buildBrowserToolVisiblePrompt,
  type BrowserAgentContextRequest,
  type BrowserToolMode,
  type BrowserToolTheme,
} from "./browser-tool-injection"
import type { SpeechDictationAdapter } from "./agent-runtime/use-speech-dictation"
import type { DictationEngineDto, DictationEventDto, DictationStatusDto } from "@/src/lib/xero-model/dictation"
import {
  applyBrowserTabOrder,
  BrowserSidebar,
  browserTabTranslateX,
  collectBrowserOverlayOcclusionRects,
  createBrowserEventCoalescer,
  reorderBrowserTabs,
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

function browserToolTestTheme(): BrowserToolTheme {
  return {
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
}

function makeDictationStatus(overrides: Partial<DictationStatusDto> = {}): DictationStatusDto {
  return {
    platform: "macos",
    osVersion: "26.0.0",
    defaultLocale: "en_US",
    supportedLocales: ["en_US"],
    modern: {
      available: false,
      compiled: false,
      runtimeSupported: false,
      reason: "modern_sdk_unavailable",
    },
    legacy: {
      available: true,
      compiled: true,
      runtimeSupported: true,
      reason: null,
    },
    windowsSdk: {
      available: false,
      compiled: false,
      runtimeSupported: false,
      reason: null,
    },
    modernAssets: {
      status: "unavailable",
      locale: null,
      reason: "modern_sdk_unavailable",
    },
    microphonePermission: "authorized",
    speechPermission: "authorized",
    activeSession: null,
    ...overrides,
  }
}

function createDictationAdapter(options: {
  engine?: DictationEngineDto
  status?: DictationStatusDto
  start?: (
    handler: (event: DictationEventDto) => void,
    session: {
      response: {
        sessionId: string
        engine: DictationEngineDto
        locale: string
      }
    },
  ) => Promise<void>
  stop?: () => Promise<void>
  cancel?: () => Promise<void>
} = {}) {
  let eventHandler: ((event: DictationEventDto) => void) | null = null
  const engine = options.engine ?? "legacy"
  const session = {
    response: {
      sessionId: "dictation-session-1",
      engine,
      locale: "en_US",
    },
    unsubscribe: vi.fn(),
    stop: vi.fn(options.stop ?? (async () => undefined)),
    cancel: vi.fn(options.cancel ?? (async () => undefined)),
  }
  const adapter: SpeechDictationAdapter = {
    isDesktopRuntime: () => true,
    speechDictationStatus: vi.fn(async () => options.status ?? makeDictationStatus()),
    speechDictationSettings: vi.fn(async () => ({
      enginePreference: "automatic" as const,
      privacyMode: "on_device_preferred" as const,
      locale: "en_US",
      updatedAt: null,
    })),
    speechDictationStart: vi.fn(async (_request, handler) => {
      eventHandler = handler
      if (options.start) {
        await options.start(handler, session)
        return session
      }
      handler({
        kind: "started",
        sessionId: session.response.sessionId,
        engine,
        locale: "en_US",
      })
      return session
    }),
    speechDictationStop: vi.fn(async () => undefined),
    speechDictationCancel: vi.fn(async () => undefined),
  }

  return {
    adapter,
    session,
    emit(event: DictationEventDto) {
      if (!eventHandler) {
        throw new Error("Dictation session has not started.")
      }

      act(() => {
        eventHandler?.(event)
      })
    },
  }
}

beforeEach(() => {
  cookieStorage = installLocalStorage()
})

afterEach(() => {
  resetBridge()
  vi.restoreAllMocks()
  document.documentElement.removeAttribute("style")
  document.body.removeAttribute("style")
  document.getElementById("__xero-browser-pen-document-root")?.remove()
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

  it("renders and hydrates browser tabs only for the active project", async () => {
    const tabListRequests: Array<Record<string, unknown> | undefined> = []
    registerInvoke("browser_tab_list", async (args) => {
      tabListRequests.push(args)
      return [
        {
          id: "tab-project-a",
          projectId: "project-a",
          label: "xero-browser-tab-a",
          title: "Project A",
          url: "https://project-a.example/",
          loading: false,
          canGoBack: false,
          canGoForward: false,
          active: false,
        },
        {
          id: "tab-project-b",
          projectId: "project-b",
          label: "xero-browser-tab-b",
          title: "Project B",
          url: "https://project-b.example/",
          loading: false,
          canGoBack: false,
          canGoForward: false,
          active: true,
        },
      ]
    })

    render(<BrowserSidebar open projectId="project-a" />)

    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    await waitFor(() => expect(input.value).toBe("https://project-a.example/"))
    expect(tabListRequests[0]).toMatchObject({ projectId: "project-a" })
    expect(screen.getByText("Project A")).toBeInTheDocument()
    expect(screen.queryByText("Project B")).not.toBeInTheDocument()

    act(() => {
      emitEvent("browser:tab_updated", {
        tabs: [
          {
            id: "tab-project-a",
            projectId: "project-a",
            label: "xero-browser-tab-a",
            title: "Project A",
            url: "https://project-a.example/updated",
            loading: false,
            canGoBack: false,
            canGoForward: false,
            active: false,
          },
          {
            id: "tab-project-b",
            projectId: "project-b",
            label: "xero-browser-tab-b",
            title: "Project B",
            url: "https://project-b.example/",
            loading: false,
            canGoBack: false,
            canGoForward: false,
            active: true,
          },
        ],
      })
    })

    await waitFor(() => expect(input.value).toBe("https://project-a.example/updated"))
    expect(screen.getByText("Project A")).toBeInTheDocument()
    expect(screen.queryByText("Project B")).not.toBeInTheDocument()
  })

  it("reorders browser tabs inside the current project", () => {
    const tabs = [
      {
        id: "tab-project-a-1",
        projectId: "project-a",
        label: "xero-browser-tab-a-1",
        title: "Project A 1",
        url: "https://project-a.example/1",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: false,
      },
      {
        id: "tab-project-b",
        projectId: "project-b",
        label: "xero-browser-tab-b",
        title: "Project B",
        url: "https://project-b.example/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
      {
        id: "tab-project-a-2",
        projectId: "project-a",
        label: "xero-browser-tab-a-2",
        title: "Project A 2",
        url: "https://project-a.example/2",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: false,
      },
    ]

    expect(
      reorderBrowserTabs(
        tabs,
        "project-a",
        "tab-project-a-2",
        "tab-project-a-1",
      ).map((tab) => tab.id),
    ).toEqual(["tab-project-a-2", "tab-project-b", "tab-project-a-1"])
    expect(
      reorderBrowserTabs(
        tabs,
        "project-a",
        "tab-project-a-2",
        "tab-project-b",
      ),
    ).toBe(tabs)
  })

  it("applies pending browser tab order to stale native tab lists", () => {
    const tabs = [
      {
        id: "tab-project-a-1",
        projectId: "project-a",
        label: "xero-browser-tab-a-1",
        title: "Project A 1",
        url: "https://project-a.example/1",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: false,
      },
      {
        id: "tab-project-b",
        projectId: "project-b",
        label: "xero-browser-tab-b",
        title: "Project B",
        url: "https://project-b.example/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
      {
        id: "tab-project-a-2",
        projectId: "project-a",
        label: "xero-browser-tab-a-2",
        title: "Project A 2",
        url: "https://project-a.example/2",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: false,
      },
    ]

    expect(
      applyBrowserTabOrder(
        tabs,
        "project-a",
        ["tab-project-a-2", "tab-project-a-1"],
      ).map((tab) => tab.id),
    ).toEqual(["tab-project-a-2", "tab-project-b", "tab-project-a-1"])
  })

  it("keeps sortable tab transforms translate-only so tab widths do not stretch", () => {
    expect(
      browserTabTranslateX({
        x: 42,
        y: 7,
        scaleX: 1.8,
        scaleY: 0.75,
      }),
    ).toBe("translate3d(42px, 0px, 0)")
  })

  it("submits a URL and invokes browser_show with the expected shape", async () => {
    registerInvoke("browser_tab_list", async () => [])
    const shownRequests: Array<Record<string, unknown> | undefined> = []
    registerInvoke("browser_show", async (args) => {
      shownRequests.push(args)
      return {
        id: "tab-1",
        projectId: "project-a",
        label: "xero-browser-tab-1",
        title: null,
        url: String((args as { url?: string })?.url ?? ""),
        loading: true,
        canGoBack: false,
        canGoForward: false,
        active: true,
      }
    })

    render(<BrowserSidebar open projectId="project-a" />)

    const input = await screen.findByLabelText("Address")
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: "example.com" } })
    const form = input.closest("form")!
    fireEvent.submit(form)

    await waitFor(() => {
      expect(shownRequests).toHaveLength(1)
      expect(shownRequests[0]).toMatchObject({
        projectId: "project-a",
        url: "https://example.com",
      })
    })
  })

  it("preserves localhost URLs for the embedded WebView", async () => {
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
      expect(shownUrls).toEqual(["http://localhost:4200/"])
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
      expect(shownUrls).toEqual(["http://localhost:4200"])
    })
  })

  it("opens a detected project app from the browser header", async () => {
    registerInvoke("browser_tab_list", async () => [])
    registerInvoke("browser_dev_server_running", async () => true)
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

    const projectButton = await screen.findByRole("button", { name: "Open project app in browser" })
    await waitFor(() => expect(projectButton).toBeEnabled())
    fireEvent.click(projectButton)

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
    })
  })

  it("switches to another detected project app from the browser header picker", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Current app",
        url: "http://127.0.0.1:4100/feed",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_dev_server_running", async () => true)
    const shownRequests: Array<Record<string, unknown> | undefined> = []
    registerInvoke("browser_show", async (args) => {
      shownRequests.push(args)
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
            id: "browser-app:http://127.0.0.1:4100/",
            label: "All - 127.0.0.1:4100",
            url: "http://127.0.0.1:4100/",
            source: "All",
            detectedAt: 1,
          },
          {
            id: "browser-app:http://127.0.0.1:4200/",
            label: "All - 127.0.0.1:4200",
            url: "http://127.0.0.1:4200/",
            source: "All",
            detectedAt: 2,
          },
        ]}
      />,
    )

    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    await waitFor(() => expect(input.value).toBe("http://127.0.0.1:4100/feed"))

    const projectButton = await screen.findByRole("button", { name: "Open project app in browser" })
    await waitFor(() => expect(projectButton).toBeEnabled())
    invokeCalls.length = 0
    fireEvent.click(projectButton)
    const panel = await screen.findByLabelText("Project app suggestions")
    expect(panel).toHaveClass("absolute")
    expect(panel).toHaveClass("rounded-md")
    expect(panel).toHaveAttribute("data-slot", "dropdown-menu-content")
    expect(panel).toHaveStyle({ left: "104px", top: "78px" })
    const wheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      deltaY: 64,
    })
    const preventDefault = vi.spyOn(wheel, "preventDefault")
    act(() => {
      panel.dispatchEvent(wheel)
    })
    expect(panel.scrollTop).toBe(64)
    expect(preventDefault).toHaveBeenCalled()
    act(() => {
      emitEvent("browser:occlusion_wheel", { deltaX: 0, deltaY: 48 })
    })
    expect(panel.scrollTop).toBe(112)
    const localServerButton = await screen.findByRole("button", { name: /Open All .*127\.0\.0\.1:4200/ })
    const originalElementFromPoint = Object.getOwnPropertyDescriptor(document, "elementFromPoint")
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => localServerButton),
    })
    try {
      act(() => {
        emitEvent("browser:occlusion_click", { x: 220, y: 32 })
      })
    } finally {
      if (originalElementFromPoint) {
        Object.defineProperty(document, "elementFromPoint", originalElementFromPoint)
      } else {
        delete (document as unknown as { elementFromPoint?: Document["elementFromPoint"] })
          .elementFromPoint
      }
    }

    await waitFor(() => {
      expect(shownRequests.at(-1)).toMatchObject({
        tabId: "tab-1",
        url: "http://127.0.0.1:4200/",
      })
    })
  })

  it("opens a running local dev server discovered outside the Xero terminal", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "Current app",
        url: "http://127.0.0.1:4100/feed",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_list_running_dev_servers", async () => [
      {
        cwd: "/repo/apps/api",
        detectedAt: 10,
        label: "beam.smp · 127.0.0.1:4100",
        processName: "beam.smp",
        url: "http://127.0.0.1:4100/",
      },
      {
        cwd: "/repo/apps/web",
        detectedAt: 11,
        label: "node · 127.0.0.1:5173",
        processName: "node",
        url: "http://127.0.0.1:5173/",
      },
      {
        cwd: "/repo/apps/admin",
        detectedAt: 12,
        label: "node · 127.0.0.1:3101",
        processName: "node",
        url: "http://127.0.0.1:3101/",
      },
      {
        cwd: "/repo/apps/landing",
        detectedAt: 13,
        label: "node · 127.0.0.1:3001",
        processName: "node",
        url: "http://127.0.0.1:3001/",
      },
      {
        cwd: "/repo/apps/landing",
        detectedAt: 14,
        label: "node · 127.0.0.1:4200",
        processName: "node",
        url: "http://127.0.0.1:4200/",
      },
    ])
    registerInvoke("browser_dev_server_running", async () => true)
    const shownRequests: Array<Record<string, unknown> | undefined> = []
    registerInvoke("browser_show", async (args) => {
      shownRequests.push(args)
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
        projectRootPath="/repo"
        projectStartTargets={[
          {
            name: "api",
            command: "cd apps/api && mix phx.server",
            browserSupported: false,
          },
          {
            name: "web",
            command: "cd apps/web && pnpm dev",
            browserSupported: true,
          },
          {
            name: "admin",
            command: "cd apps/admin && pnpm dev",
            browserSupported: true,
          },
          {
            name: "landing",
            command: "cd apps/landing && pnpm dev",
            browserSupported: true,
          },
        ]}
      />,
    )

    const projectButton = await screen.findByRole("button", { name: "Open project app in browser" })
    await waitFor(() => expect(projectButton).toBeEnabled())
    fireEvent.click(projectButton)
    expect(screen.queryByRole("button", { name: /Open api .*127\.0\.0\.1:4100/ })).not.toBeInTheDocument()
    const panel = await screen.findByLabelText("Project app suggestions")
    expect(await within(panel).findByRole("button", { name: /Open admin .*127\.0\.0\.1:3101/ })).toBeInTheDocument()
    expect(within(panel).getAllByRole("button", { name: /Open landing / })).toHaveLength(1)
    expect(within(panel).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "web · 127.0.0.1:5173",
      "admin · 127.0.0.1:3101",
      "landing · 127.0.0.1:4200",
    ])
    fireEvent.click(await screen.findByRole("button", { name: /Open web .*127\.0\.0\.1:5173/ }))

    await waitFor(() => {
      expect(shownRequests.at(-1)).toMatchObject({
        tabId: "tab-1",
        url: "http://127.0.0.1:5173/",
      })
    })
  })

  it("opens a detected project app from the address bar suggestions", async () => {
    registerInvoke("browser_tab_list", async () => [])
    registerInvoke("browser_dev_server_running", async () => true)
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

    const input = (await screen.findByLabelText("Address")) as HTMLInputElement
    await waitFor(() => {
      expect(invokeCalls.some((call) => call.command === "browser_dev_server_running")).toBe(true)
    })
    expect(input).toHaveAttribute("autocomplete", "off")

    fireEvent.focus(input)
    fireEvent.click(await screen.findByRole("button", { name: /Open web .*localhost:5173/ }))

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
    })
  })

  it("disables project app navigation when its dev server liveness probe fails", async () => {
    registerInvoke("browser_tab_list", async () => [])
    registerInvoke("browser_dev_server_running", async () => false)

    function BrowserSidebarWithTargets() {
      const [targets, setTargets] = useState([
        {
          id: "browser-app:http://127.0.0.1:5173/",
          label: "web · localhost:5173",
          url: "http://127.0.0.1:5173/",
          source: "web",
          detectedAt: 1,
        },
      ])

      return (
        <BrowserSidebar
          open
          projectBrowserTargets={targets}
          onProjectBrowserTargetUnavailable={(url) => {
            setTargets((current) =>
              current.filter((target) => !target.url.startsWith(new URL(url).origin)),
            )
          }}
        />
      )
    }

    render(<BrowserSidebarWithTargets />)

    const projectButton = await screen.findByRole("button", { name: "Open project app in browser" })
    await waitFor(() => {
      expect(invokeCalls.some((call) => call.command === "browser_dev_server_running")).toBe(true)
      expect(projectButton).toBeDisabled()
    })
  })

  it("opens a previously verified project app without a second blocking liveness probe", async () => {
    registerInvoke("browser_tab_list", async () => [])
    let probeCount = 0
    registerInvoke("browser_dev_server_running", async () => {
      probeCount += 1
      return probeCount === 1
    })
    const shownUrls: string[] = []
    registerInvoke("browser_show", async (args) => {
      shownUrls.push(String((args as { url?: string })?.url ?? ""))
      return undefined
    })

    function BrowserSidebarWithTargets() {
      const [targets, setTargets] = useState([
        {
          id: "browser-app:http://127.0.0.1:5173/",
          label: "web · localhost:5173",
          url: "http://127.0.0.1:5173/",
          source: "web",
          detectedAt: 1,
        },
      ])

      return (
        <BrowserSidebar
          open
          projectBrowserTargets={targets}
          onProjectBrowserTargetUnavailable={(url) => {
            setTargets((current) =>
              current.filter((target) => !target.url.startsWith(new URL(url).origin)),
            )
          }}
        />
      )
    }

    render(<BrowserSidebarWithTargets />)

    const projectButton = await screen.findByRole("button", { name: "Open project app in browser" })
    await waitFor(() => expect(projectButton).toBeEnabled())
    fireEvent.click(projectButton)

    await waitFor(() => {
      expect(shownUrls).toEqual(["http://127.0.0.1:5173/"])
    })
    expect(probeCount).toBe(1)
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
      expect(shownUrls).toEqual(["http://localhost:5173/"])
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
      expect(shownUrls).toEqual(["http://localhost:5173/"])
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

  it("places the browser focus toggle before the pen tool", async () => {
    const onFullWidthChange = vi.fn()
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

    const { rerender } = render(
      <BrowserSidebar
        open
        fullWidth={false}
        fullWidthTarget={900}
        onFullWidthChange={onFullWidthChange}
      />,
    )

    const tools = await screen.findByTestId("browser-dev-tools")
    expect(
      within(tools).getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual(["Hide agent panel", "Sketch on page", "Inspect element"])

    fireEvent.click(within(tools).getByRole("button", { name: "Hide agent panel" }))
    expect(onFullWidthChange).toHaveBeenCalledWith(true)

    rerender(
      <BrowserSidebar
        open
        fullWidth
        fullWidthTarget={900}
        onFullWidthChange={onFullWidthChange}
      />,
    )

    const restoreButton = within(await screen.findByTestId("browser-dev-tools")).getByRole(
      "button",
      { name: "Show agent panel" },
    )
    expect(restoreButton).toHaveAttribute("aria-pressed", "true")
    fireEvent.click(restoreButton)
    expect(onFullWidthChange).toHaveBeenLastCalledWith(false)
  })

  it("uses the focused browser width target and hides the resize handle", async () => {
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

    render(
      <BrowserSidebar
        open
        fullWidth
        fullWidthTarget={900}
        onFullWidthChange={() => undefined}
      />,
    )

    await screen.findByLabelText("Address")
    const sidebar = document.querySelector("aside")!

    await waitFor(() => expect(sidebar).toHaveStyle({ width: "900px" }))
    expect(
      screen.queryByRole("separator", { name: "Resize browser sidebar" }),
    ).not.toBeInTheDocument()
  })

  it("disables pen mode when the selected model cannot accept image input", async () => {
    const reason = "Text model does not support image attachments. Choose a model with image input to use the pen tool."
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

    render(<BrowserSidebar open penToolDisabledReason={reason} />)
    const penButton = await screen.findByLabelText("Sketch on page")

    expect(penButton).toBeDisabled()
    expect(penButton).toHaveAttribute("title", reason)
    expect(penButton).toHaveAttribute("aria-pressed", "false")
    fireEvent.click(penButton)
    expect(
      invokeCalls.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes('"mode":"pen"'),
      ),
    ).toBe(false)
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

    render(
      <BrowserSidebar
        open
        onAddAgentContext={onAddAgentContext}
        projectBrowserTargets={[
          {
            id: "web-app",
            label: "Web app - 127.0.0.1:5173",
            url: "http://127.0.0.1:5173/",
            detectedAt: 1,
          },
        ]}
      />,
    )
    fireEvent.click(await screen.findByLabelText("Inspect element"))
    const callCountBeforeSubmit = invokeCalls.length

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "inspect",
          note: "Tighten the spacing here",
          page: { url: "http://localhost:5173/", title: "Local" },
          viewport: { width: 800, height: 600, devicePixelRatio: 2 },
          scroll: { x: 0, y: 120 },
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
    expect(request.prompt).toContain("capture 1")
    expect(request.prompt).toContain("App: Web app - 127.0.0.1:5173")
    expect(request.prompt).toContain("local dev server /")
    expect(request.prompt).not.toContain("localhost:")
    expect(request.prompt).toContain("User note: Tighten the spacing here")
    expect(request.prompt).toContain("Viewport: 800x600 CSS px, DPR 2")
    expect(request.prompt).toContain("Scroll: x=0 y=120")
    expect(request.prompt).toContain("Element bounds: x=20 y=40 w=120 h=36")
    expect(request.prompt).toContain("Selector: button.cta")
    expect(request.prompt).toContain("for locating code; no screenshot")
    expect(request.prompt).toContain("Source: /app/src/components/HeroCta.tsx:42:7")
    expect(request.prompt).toContain('Stable attrs: data-testid="hero-cta"')
    expect(request.prompt).toContain('Parent chain: <section> section.hero label "Hero"')
    expect(request.prompt).not.toContain("DOM snippet")
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

  it("starts dictation from a browser tool note and writes dictated text back into the note", async () => {
    const dictation = createDictationAdapter()
    const latestComposerNoteScript = () => {
      const calls = invokeCalls.filter(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("setComposerNote"),
      )
      return String(calls[calls.length - 1]?.args?.js ?? "")
    }
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
    registerInvoke("browser_eval_fire_and_forget", async () => null)

    render(<BrowserSidebar open dictationAdapter={dictation.adapter} />)
    fireEvent.click(await screen.findByLabelText("Inspect element"))

    await act(async () => {
      emitEvent(BROWSER_TOOL_NOTE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start",
        active: true,
      })
    })

    await waitFor(() => expect(dictation.adapter.speechDictationStatus).toHaveBeenCalledTimes(1))
    await waitFor(() => {
      expect(
        invokeCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes("setDictationState") &&
            String(call.args?.js ?? "").includes('"visible":true'),
        ),
      ).toBe(true)
    })

    await act(async () => {
      emitEvent(BROWSER_TOOL_DICTATION_TOGGLE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start after edit",
      })
    })

    await waitFor(() => expect(dictation.adapter.speechDictationStart).toHaveBeenCalledTimes(1))
    dictation.emit({
      kind: "partial",
      sessionId: "dictation-session-1",
      text: "dictated",
      sequence: 1,
    })

    await waitFor(() => {
      expect(latestComposerNoteScript()).toContain("Typed start after edit dictated")
    })

    await act(async () => {
      emitEvent(BROWSER_TOOL_NOTE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start after edit dictated",
        active: true,
      })
    })

    dictation.emit({
      kind: "final",
      sessionId: "dictation-session-1",
      text: "dictated finish",
      sequence: 2,
    })

    await waitFor(() => {
      expect(latestComposerNoteScript()).toContain("Typed start after edit dictated finish")
      expect(latestComposerNoteScript()).not.toContain("Typed start after edit dictated dictated finish")
    })

    await act(async () => {
      emitEvent(BROWSER_TOOL_NOTE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start after edit dictated finish",
        active: true,
      })
    })

    dictation.emit({
      kind: "final",
      sessionId: "dictation-session-1",
      text: "dictated finish",
      sequence: 3,
    })

    await waitFor(() => {
      expect(latestComposerNoteScript()).toContain("Typed start after edit dictated finish")
      expect(latestComposerNoteScript()).not.toContain("Typed start after edit dictated dictated finish")
      expect(latestComposerNoteScript()).not.toContain("Typed start after edit dictated finish dictated finish")
    })

    dictation.emit({
      kind: "final",
      sessionId: "dictation-session-1",
      text: "dictated finish and next sentence",
      sequence: 4,
    })

    await waitFor(() => {
      expect(latestComposerNoteScript()).toContain("Typed start after edit dictated finish and next sentence")
      expect(latestComposerNoteScript()).not.toContain("dictated finish dictated finish")
    })
  })

  it("marks browser tool note dictation active while native startup is pending", async () => {
    let resolveStart: (() => void) | null = null
    const dictation = createDictationAdapter({
      start: async () => {
        await new Promise<void>((resolve) => {
          resolveStart = resolve
        })
      },
    })
    const latestDictationStateScript = () => {
      const calls = invokeCalls.filter(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("setDictationState"),
      )
      return String(calls[calls.length - 1]?.args?.js ?? "")
    }
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
    registerInvoke("browser_eval_fire_and_forget", async () => null)

    render(<BrowserSidebar open dictationAdapter={dictation.adapter} />)
    fireEvent.click(await screen.findByLabelText("Inspect element"))

    await act(async () => {
      emitEvent(BROWSER_TOOL_NOTE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start",
        active: true,
      })
    })

    await waitFor(() => expect(dictation.adapter.speechDictationStatus).toHaveBeenCalledTimes(1))

    await act(async () => {
      emitEvent(BROWSER_TOOL_DICTATION_TOGGLE_EVENT, {
        tabId: "tab-1",
        mode: "inspect",
        note: "Typed start",
      })
    })

    await waitFor(() => expect(dictation.adapter.speechDictationStart).toHaveBeenCalledTimes(1))
    await waitFor(() => {
      expect(latestDictationStateScript()).toContain('"ariaLabel":"Starting dictation"')
      expect(latestDictationStateScript()).toContain('"isListening":true')
      expect(latestDictationStateScript()).toContain('"isToggleDisabled":true')
    })

    await act(async () => {
      resolveStart?.()
    })
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

    render(
      <BrowserSidebar
        open
        onAddAgentContext={onAddAgentContext}
        projectBrowserTargets={[
          {
            id: "web-app",
            label: "Web app - 127.0.0.1:5173",
            url: "http://127.0.0.1:5173/",
            detectedAt: 1,
          },
        ]}
      />,
    )
    fireEvent.click(await screen.findByLabelText("Sketch on page"))

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "Tighten the spacing here",
          page: { url: "http://localhost:5173/", title: "Local" },
          viewport: { width: 800, height: 600, devicePixelRatio: 2 },
          scroll: { x: 0, y: 120 },
          annotationBounds: { x: 24, y: 80, width: 320, height: 160 },
          strokeCount: 1,
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))
    const request = addedRequests[0]!
    expect(request.prompt).toContain("Browser sketch context")
    expect(request.prompt).toContain("capture 1")
    expect(request.prompt).toContain("App: Web app - 127.0.0.1:5173")
    expect(request.prompt).toContain("User note: Tighten the spacing here")
    expect(request.prompt).toContain("Viewport: 800x600 CSS px, DPR 2")
    expect(request.prompt).toContain("Scroll: x=0 y=120")
    expect(request.prompt).toContain("Annotation bounds: x=24 y=80 w=320 h=160")
    expect(request.visiblePrompt).toBe("Tighten the spacing here")
    expect(request.contextCard).toEqual({
      kind: "sketch",
      title: "Browser sketch context",
      subtitle: "1 stroke on browser screenshot",
    })
    expect(request.image).toBeTruthy()
    expect(Array.from(request.image!.bytes)).toEqual([104, 101, 108, 108, 111])
    expect(request.image!.originalName).toMatch(/^browser-pen-/)
    expect(request.prompt).toContain(request.image!.originalName)
    const prepareIndex = invokeCalls.findIndex(
      (call) =>
        call.command === "browser_eval_fire_and_forget" &&
        String(call.args?.js ?? "").includes("prepareCapture"),
    )
    const screenshotIndex = invokeCalls.findIndex(
      (call) => call.command === "browser_screenshot",
    )
    const finishIndex = invokeCalls.findIndex(
      (call, index) =>
        index > screenshotIndex &&
        call.command === "browser_eval_fire_and_forget" &&
        String(call.args?.js ?? "").includes("finishCapture"),
    )
    expect(prepareIndex).toBeGreaterThanOrEqual(0)
    expect(screenshotIndex).toBeGreaterThan(prepareIndex)
    expect(finishIndex).toBeGreaterThan(screenshotIndex)
  })

  it("labels multiple sketch captures from separate apps with ordered metadata", async () => {
    const addedRequests: BrowserAgentContextRequest[] = []
    const onAddAgentContext = vi.fn(async (request: BrowserAgentContextRequest) => {
      addedRequests.push(request)
    })
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "xero-browser-tab-1",
        title: "App A",
        url: "http://localhost:5173/",
        loading: false,
        canGoBack: false,
        canGoForward: false,
        active: true,
      },
    ])
    registerInvoke("browser_screenshot", async () => "aGVsbG8=")

    render(
      <BrowserSidebar
        open
        onAddAgentContext={onAddAgentContext}
        projectBrowserTargets={[
          {
            id: "app-a",
            label: "App A - 127.0.0.1:5173",
            url: "http://127.0.0.1:5173/",
            detectedAt: 1,
          },
          {
            id: "app-b",
            label: "App B - 127.0.0.1:3000",
            url: "http://127.0.0.1:3000/",
            detectedAt: 1,
          },
        ]}
      />,
    )
    await screen.findByLabelText("Sketch on page")

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "First app note",
          page: { url: "http://localhost:5173/dashboard", title: "App A" },
          viewport: { width: 800, height: 600, devicePixelRatio: 2 },
          scroll: { x: 0, y: 10 },
          annotationBounds: { x: 10, y: 20, width: 100, height: 60 },
          strokeCount: 2,
        },
      })
    })
    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(1))

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "Second app note",
          page: { url: "http://localhost:3000/settings", title: "App B" },
          viewport: { width: 1024, height: 768, devicePixelRatio: 1 },
          scroll: { x: 0, y: 220 },
          annotationBounds: { x: 48, y: 96, width: 240, height: 120 },
          strokeCount: 1,
        },
      })
    })

    await waitFor(() => expect(onAddAgentContext).toHaveBeenCalledTimes(2))
    expect(addedRequests[0]?.prompt).toContain("Browser sketch context (capture 1)")
    expect(addedRequests[0]?.prompt).toContain("App: App A - 127.0.0.1:5173")
    expect(addedRequests[0]?.prompt).toContain("User note: First app note")
    expect(addedRequests[0]?.prompt).toContain("Attached image:")
    expect(addedRequests[1]?.prompt).toContain("Browser sketch context (capture 2)")
    expect(addedRequests[1]?.prompt).toContain("App: App B - 127.0.0.1:3000")
    expect(addedRequests[1]?.prompt).toContain("User note: Second app note")
    expect(addedRequests[1]?.prompt).toContain("Viewport: 1024x768 CSS px, DPR 1")
  })

  it("keeps the pen drawing visible until the composer insert is handed off", async () => {
    let resolveComposerInsert: (() => void) | null = null
    let notifyComposerInsertStarted: (() => void) | null = null
    const composerInsertStarted = new Promise<void>((resolve) => {
      notifyComposerInsertStarted = resolve
    })
    const onAddAgentContext = vi.fn(
      async (_request: BrowserAgentContextRequest) =>
        new Promise<void>((resolve) => {
          resolveComposerInsert = resolve
          notifyComposerInsertStarted?.()
        }),
    )
    const finishOverlayStates: Array<string | null> = []
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
    registerInvoke("browser_eval_fire_and_forget", (args) => {
      if (String(args?.js ?? "").includes("window.__xeroBrowserTool.finishCapture")) {
        finishOverlayStates.push(
          screen
            .queryByRole("status", { name: "Adding browser context" })
            ?.getAttribute("data-state") ?? null,
        )
      }
      return undefined
    })

    render(<BrowserSidebar open onAddAgentContext={onAddAgentContext} />)

    fireEvent.click(await screen.findByLabelText("Sketch on page"))
    const callCountBeforeSubmit = invokeCalls.length

    await act(async () => {
      emitEvent("browser:tool_context", {
        tabId: "tab-1",
        context: {
          kind: "pen",
          note: "Attach this sketch",
          page: { url: "http://localhost:5173/", title: "Local" },
          viewport: { width: 800, height: 600 },
          strokeCount: 1,
        },
      })
    })

    await composerInsertStarted
    const submitCallsBeforeInsertSettles = invokeCalls.slice(callCountBeforeSubmit)
    expect(
      submitCallsBeforeInsertSettles.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("showLoading"),
      ),
    ).toBe(false)
    expect(
      submitCallsBeforeInsertSettles.some(
        (call) =>
          call.command === "browser_eval_fire_and_forget" &&
          String(call.args?.js ?? "").includes("finishCapture"),
      ),
    ).toBe(false)

    await act(async () => {
      resolveComposerInsert?.()
    })

    await waitFor(() => {
      const submitCalls = invokeCalls.slice(callCountBeforeSubmit)
      expect(
        submitCalls.some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes("finishCapture"),
        ),
      ).toBe(true)
    })
    expect(finishOverlayStates).toEqual(["closed"])
    expect(
      invokeCalls
        .slice(callCountBeforeSubmit)
        .some(
          (call) =>
            call.command === "browser_eval_fire_and_forget" &&
            String(call.args?.js ?? "").includes("deactivate"),
        ),
    ).toBe(false)
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

  it("lets the floating browser tool toolbar be dragged in pen and inspect modes", () => {
    const originalWidth = window.innerWidth
    const originalHeight = window.innerHeight
    const modes: BrowserToolMode[] = ["pen", "inspect"]

    try {
      setWindowInnerSize(900, 600)

      for (const mode of modes) {
        const script = buildBrowserToolActivationScript({
          mode,
          pageLabel: "Local App",
          theme: browserToolTestTheme(),
        })
        new Function(script)()

        const toolHost = document.getElementById("__xero-browser-tool-root")
        const shadow = toolHost?.shadowRoot
        const toolbar = shadow?.querySelector<HTMLElement>(".toolbar")
        const handle = shadow?.querySelector<HTMLButtonElement>(".toolbar-handle")
        expect(toolbar).toBeTruthy()
        expect(handle).toBeTruthy()
        expect(handle?.getAttribute("aria-label")).toBe("Move browser tool controls")

        vi.spyOn(toolbar!, "getBoundingClientRect").mockReturnValue(
          rect({
            bottom: 44,
            height: 34,
            left: 220,
            right: 540,
            top: 10,
            width: 320,
          }),
        )

        dispatchPointer(handle!, "pointerdown", { clientX: 236, clientY: 22 })
        expect(toolbar?.getAttribute("data-dragging")).toBe("true")
        dispatchPointer(window, "pointermove", { clientX: 436, clientY: 112 })
        dispatchPointer(window, "pointerup", { clientX: 436, clientY: 112 })

        expect(toolbar?.style.left).toBe("420px")
        expect(toolbar?.style.top).toBe("100px")
        expect(toolbar?.style.transform).toBe("none")
        expect(toolbar?.getAttribute("data-dragging")).toBeNull()

        if (mode === "pen") {
          expect(
            document
              .getElementById("__xero-browser-pen-document-layer")
              ?.querySelector(".xero-document-pen-path"),
          ).toBeNull()
        }

        ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
          .__xeroBrowserTool?.deactivate()
      }
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      setWindowInnerSize(originalWidth, originalHeight)
    }
  })

  it("clears the selected inspect target from the floating toolbar", () => {
    const originalElementFromPoint = Object.getOwnPropertyDescriptor(
      document,
      "elementFromPoint",
    )
    const target = document.createElement("button")
    target.textContent = "Launch"
    target.setAttribute("aria-label", "Launch")
    target.setAttribute("data-xero-browser-tool-selected", "existing-marker")
    target.setAttribute("data-xero-browser-tool-selected-label", "existing-label")
    target.style.setProperty("outline", "1px dotted red", "important")
    target.style.setProperty("outline-offset", "3px", "important")
    document.body.appendChild(target)
    vi.spyOn(target, "getBoundingClientRect").mockReturnValue(
      rect({
        bottom: 100,
        height: 40,
        left: 40,
        right: 160,
        top: 60,
        width: 120,
      }),
    )
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => target),
    })

    try {
      const script = buildBrowserToolActivationScript({
        mode: "inspect",
        pageLabel: "Local App",
        theme: browserToolTestTheme(),
      })
      new Function(script)()

      const toolHost = document.getElementById("__xero-browser-tool-root")
      const shadow = toolHost?.shadowRoot
      const layer = shadow?.querySelector<HTMLElement>(".layer")
      const clear = Array.from(
        shadow?.querySelectorAll<HTMLButtonElement>(".toolbar-button") ?? [],
      ).find((button) => button.textContent === "Clear")
      expect(layer).toBeTruthy()
      expect(clear).toBeTruthy()
      expect(clear?.hidden).toBe(false)

      dispatchPointer(layer!, "pointermove", { clientX: 80, clientY: 80 })
      dispatchPointer(layer!, "click", { clientX: 80, clientY: 80 })

      expect(shadow?.querySelector(".composer")).toBeTruthy()
      expect(target.getAttribute("data-xero-browser-tool-selected")).toBe("true")
      expect(target.getAttribute("data-xero-browser-tool-selected-label")).toContain("button")
      expect(target.style.getPropertyValue("outline")).toContain("2px solid")
      expect(target.style.getPropertyPriority("outline")).toBe("important")
      expect((shadow?.querySelector(".inspect-highlight") as HTMLElement | null)?.style.display).toBe(
        "none",
      )

      clear!.click()

      expect(shadow?.querySelector(".composer")).toBeNull()
      expect(target.getAttribute("data-xero-browser-tool-selected")).toBe("existing-marker")
      expect(target.getAttribute("data-xero-browser-tool-selected-label")).toBe("existing-label")
      expect(target.style.getPropertyValue("outline")).toBe("1px dotted red")
      expect(target.style.getPropertyPriority("outline")).toBe("important")
      expect(target.style.getPropertyValue("outline-offset")).toBe("3px")
      expect(target.style.getPropertyPriority("outline-offset")).toBe("important")
      expect((shadow?.querySelector(".inspect-highlight") as HTMLElement | null)?.style.display).toBe(
        "none",
      )
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      target.remove()
      if (originalElementFromPoint) {
        Object.defineProperty(document, "elementFromPoint", originalElementFromPoint)
      } else {
        delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint
      }
    }
  })

  it("binds the selected inspect outline to the element and refreshes submitted metadata after scroll", async () => {
    const originalElementFromPoint = Object.getOwnPropertyDescriptor(
      document,
      "elementFromPoint",
    )
    const originalTauriInternals = Object.getOwnPropertyDescriptor(
      window,
      "__TAURI_INTERNALS__",
    )
    const invoke = vi.fn(async (_command: string, _args?: Record<string, unknown>) => null)
    const target = document.createElement("section")
    target.id = "projects"
    target.textContent = "Projects"
    document.body.appendChild(target)
    let currentRect = rect({
      bottom: 460,
      height: 180,
      left: 40,
      right: 340,
      top: 280,
      width: 300,
    })
    vi.spyOn(target, "getBoundingClientRect").mockImplementation(() => currentRect)
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => target),
    })
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke },
    })

    try {
      const script = buildBrowserToolActivationScript({
        mode: "inspect",
        pageLabel: "Local App",
        theme: browserToolTestTheme(),
      })
      new Function(script)()

      const toolHost = document.getElementById("__xero-browser-tool-root")
      const shadow = toolHost?.shadowRoot
      const layer = shadow?.querySelector<HTMLElement>(".layer")
      const highlight = shadow?.querySelector<HTMLElement>(".inspect-highlight")
      expect(layer).toBeTruthy()
      expect(highlight).toBeTruthy()

      dispatchPointer(layer!, "pointermove", { clientX: 80, clientY: 300 })
      dispatchPointer(layer!, "click", { clientX: 80, clientY: 300 })

      expect(target.getAttribute("data-xero-browser-tool-selected")).toBe("true")
      expect(target.getAttribute("data-xero-browser-tool-selected-label")).toBe("#projects")
      expect(target.style.getPropertyValue("outline")).toContain("2px solid")
      expect(target.style.getPropertyPriority("outline")).toBe("important")
      expect(highlight?.style.display).toBe("none")

      currentRect = rect({
        bottom: 220,
        height: 180,
        left: 40,
        right: 340,
        top: 40,
        width: 300,
      })
      window.dispatchEvent(new Event("scroll"))
      expect(target.getAttribute("data-xero-browser-tool-selected")).toBe("true")
      expect(highlight?.style.display).toBe("none")

      const textarea = shadow?.querySelector<HTMLTextAreaElement>(".composer-input")
      const send = shadow?.querySelector<HTMLButtonElement>(".send-button")
      expect(textarea).toBeTruthy()
      expect(send).toBeTruthy()
      textarea!.value = "Tighten this section"
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
      ) as {
        element?: { rect?: { x?: number; y?: number; width?: number; height?: number } }
        kind?: unknown
      }
      expect(payload.kind).toBe("inspect")
      expect(payload.element?.rect).toMatchObject({ x: 40, y: 40, width: 300, height: 180 })
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      target.remove()
      if (originalElementFromPoint) {
        Object.defineProperty(document, "elementFromPoint", originalElementFromPoint)
      } else {
        delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint
      }
      if (originalTauriInternals) {
        Object.defineProperty(window, "__TAURI_INTERNALS__", originalTauriInternals)
      } else {
        delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
      }
    }
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

  it("renders dictation controls inside the injected browser tool note composer", async () => {
    const originalTauriInternals = Object.getOwnPropertyDescriptor(
      window,
      "__TAURI_INTERNALS__",
    )
    const originalBridge = Object.getOwnPropertyDescriptor(window, "__xeroBridge__")
    const invoke = vi.fn(async (_command: string, _args?: Record<string, unknown>) => null)
    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme: browserToolTestTheme(),
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
      const dictationButton = shadow?.querySelector<HTMLButtonElement>(".dictation-button")
      expect(textarea).toBeTruthy()
      expect(dictationButton).toBeTruthy()
      expect(dictationButton?.hidden).toBe(true)

      ;(window as unknown as {
        __xeroBrowserTool?: {
          setComposerNote: (note: string) => boolean
          setDictationState: (state: Record<string, unknown>) => boolean
        }
      }).__xeroBrowserTool?.setDictationState({
        ariaLabel: "Start dictation",
        audioLevel: 0,
        isListening: false,
        isToggleDisabled: false,
        tooltip: "Start dictation",
        visible: true,
      })

      expect(dictationButton?.hidden).toBe(false)
      expect(dictationButton?.getAttribute("aria-label")).toBe("Start dictation")
      expect(dictationButton?.getAttribute("aria-pressed")).toBe("false")

      textarea!.value = "Typed note"
      dictationButton!.click()

      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith(
          "browser_internal_event",
          expect.objectContaining({
            kind: "tool_dictation_toggle",
            payload: expect.any(String),
          }),
        ),
      )
      const toggleCall = invoke.mock.calls.find(
        ([command, args]) =>
          command === "browser_internal_event" &&
          (args as { kind?: unknown } | undefined)?.kind === "tool_dictation_toggle",
      )
      const togglePayload = JSON.parse(
        String((toggleCall?.[1] as { payload?: unknown } | undefined)?.payload ?? "{}"),
      ) as { note?: unknown }
      expect(togglePayload.note).toBe("Typed note")

      ;(window as unknown as {
        __xeroBrowserTool?: {
          setComposerNote: (note: string) => boolean
          setDictationState: (state: Record<string, unknown>) => boolean
        }
      }).__xeroBrowserTool?.setComposerNote("Typed note dictated")
      expect(textarea?.value).toBe("Typed note dictated")

      ;(window as unknown as {
        __xeroBrowserTool?: {
          setDictationState: (state: Record<string, unknown>) => boolean
        }
      }).__xeroBrowserTool?.setDictationState({
        ariaLabel: "Stop dictation",
        audioLevel: 0.75,
        isListening: true,
        isToggleDisabled: false,
        tooltip: "Stop dictation",
        visible: true,
      })
      expect(dictationButton?.getAttribute("aria-label")).toBe("Stop dictation")
      expect(dictationButton?.getAttribute("aria-pressed")).toBe("true")
      expect(dictationButton?.getAttribute("data-listening")).toBe("true")
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

  it("keeps the pen SVG viewport-sized while preserving document-space strokes", async () => {
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
      const documentRoot = document.getElementById("__xero-browser-pen-document-root")
      const documentLayer = document.getElementById("__xero-browser-pen-document-layer")
      const documentFrame = documentLayer?.parentElement as HTMLElement | null
      expect(overlay).toBeTruthy()
      expect(documentRoot).toBeTruthy()
      expect(documentLayer).toBeTruthy()
      expect(documentRoot?.parentElement).toBe(document.documentElement)
      expect(documentRoot?.nextElementSibling).toBe(toolHost)
      expect(documentRoot?.getAttribute("data-xero-browser-tool-document-root")).toBe("true")
      expect(documentRoot?.style.zIndex).toBe("2147483647")
      expect(documentRoot?.style.pointerEvents).toBe("none")
      expect(documentFrame?.parentElement).toBe(documentRoot)
      expect(documentFrame?.getAttribute("data-xero-browser-tool-document-frame")).toBe("true")
      expect(documentFrame?.style.overflow).toBe("visible")
      expect(documentLayer?.getAttribute("data-xero-browser-tool-document-layer")).toBe("true")
      expect(overlay?.getAttribute("viewBox")).toBe("0 0 800 600")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 0 800 600")
      expect(documentLayer?.getAttribute("width")).toBe("800")
      expect((documentLayer as SVGSVGElement | null)?.style.width).toBe("800px")
      expect(documentLayer?.style.transform).toBe("none")
      expect(documentFrame?.style.width).toBe("800px")
      expect(documentFrame?.style.height).toBe("600px")

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
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 0 400 600")
      expect(documentLayer?.getAttribute("width")).toBe("400")
      expect(documentFrame?.style.width).toBe("400px")
      expect(documentFrame?.style.height).toBe("600px")
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

  it("promotes pen drawing layers into the browser top layer when available", () => {
    const originalShowPopover = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "showPopover",
    )
    const originalHidePopover = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "hidePopover",
    )
    const originalMatches = Object.getOwnPropertyDescriptor(Element.prototype, "matches")
    const opened: string[] = []
    const hidden: string[] = []
    Object.defineProperty(HTMLElement.prototype, "showPopover", {
      configurable: true,
      value(this: HTMLElement) {
        ;(this as unknown as { __testPopoverOpen?: boolean }).__testPopoverOpen = true
        opened.push(this.id)
      },
    })
    Object.defineProperty(HTMLElement.prototype, "hidePopover", {
      configurable: true,
      value(this: HTMLElement) {
        ;(this as unknown as { __testPopoverOpen?: boolean }).__testPopoverOpen = false
        hidden.push(this.id)
      },
    })
    Object.defineProperty(Element.prototype, "matches", {
      configurable: true,
      value(this: Element, selector: string) {
        if (selector === ":popover-open") {
          return Boolean((this as unknown as { __testPopoverOpen?: boolean }).__testPopoverOpen)
        }
        return originalMatches?.value.call(this, selector) ?? false
      },
    })

    const script = buildBrowserToolActivationScript({
      mode: "pen",
      pageLabel: "Local App",
      theme: browserToolTestTheme(),
    })

    try {
      new Function(script)()

      const toolHost = document.getElementById("__xero-browser-tool-root")
      const documentRoot = document.getElementById("__xero-browser-pen-document-root")
      const overlay = toolHost?.shadowRoot?.querySelector(".pen-layer")
      expect(toolHost?.getAttribute("popover")).toBe("manual")
      expect(documentRoot?.getAttribute("popover")).toBe("manual")
      expect(toolHost?.style.maxWidth).toBe("none")
      expect(documentRoot?.style.maxWidth).toBe("none")
      expect(opened.slice(-2)).toEqual([
        "__xero-browser-pen-document-root",
        "__xero-browser-tool-root",
      ])

      dispatchPointer(overlay!, "pointerdown", { clientX: 100, clientY: 100 })

      expect(hidden.slice(-2)).toEqual([
        "__xero-browser-pen-document-root",
        "__xero-browser-tool-root",
      ])
      expect(opened.slice(-2)).toEqual([
        "__xero-browser-pen-document-root",
        "__xero-browser-tool-root",
      ])
    } finally {
      ;(window as unknown as { __xeroBrowserTool?: { deactivate: () => void } })
        .__xeroBrowserTool?.deactivate()
      if (originalShowPopover) {
        Object.defineProperty(HTMLElement.prototype, "showPopover", originalShowPopover)
      } else {
        delete (HTMLElement.prototype as unknown as { showPopover?: unknown }).showPopover
      }
      if (originalHidePopover) {
        Object.defineProperty(HTMLElement.prototype, "hidePopover", originalHidePopover)
      } else {
        delete (HTMLElement.prototype as unknown as { hidePopover?: unknown }).hidePopover
      }
      if (originalMatches) {
        Object.defineProperty(Element.prototype, "matches", originalMatches)
      }
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
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 300 800 600")
      expect(documentLayer?.style.transform).toBe("none")
      expect(path?.getAttribute("d")).toContain("M 680 620")
      expect(path?.getAttribute("d")).toContain("L 760 640")

      await act(async () => {
        setWindowScroll(0, 520)
        window.dispatchEvent(new Event("scroll"))
        await new Promise((resolve) => window.requestAnimationFrame(resolve))
      })

      expect(documentLayer?.getAttribute("viewBox")).toBe("0 520 800 600")
      expect(documentLayer?.style.transform).toBe("none")
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

  it("keeps pen strokes attached to an inner scroll container without clipping overlays", async () => {
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

      const documentRoot = document.getElementById("__xero-browser-pen-document-root")
      const documentLayer = document.getElementById("__xero-browser-pen-document-layer")
      const documentFrame = documentLayer?.parentElement as HTMLElement | null
      expect(documentRoot?.parentElement).toBe(document.documentElement)
      expect(documentRoot?.nextElementSibling).toBe(toolHost)
      expect(documentFrame?.parentElement).toBe(documentRoot)
      expect(documentFrame?.style.left).toBe("50px")
      expect(documentFrame?.style.top).toBe("100px")
      expect(documentFrame?.style.width).toBe("320px")
      expect(documentFrame?.style.height).toBe("240px")
      expect(documentFrame?.style.overflow).toBe("visible")
      expect(scroller.style.position).toBe("")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 200 320 240")
      expect(documentLayer?.style.transform).toBe("none")

      const path = documentLayer?.querySelector(".xero-document-pen-path")
      expect(path?.getAttribute("d")).toContain("M 50 250")
      expect(path?.getAttribute("d")).toContain("L 130 275")

      scroller.scrollTop = 320
      await act(async () => {
        scroller.dispatchEvent(new Event("scroll", { bubbles: true }))
        await new Promise((resolve) => window.requestAnimationFrame(resolve))
      })

      expect(documentFrame?.style.left).toBe("50px")
      expect(documentFrame?.style.top).toBe("100px")
      expect(documentLayer?.getAttribute("viewBox")).toBe("0 320 320 240")
      expect(documentLayer?.style.transform).toBe("none")
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
