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
import { BrowserSidebar, createBrowserEventCoalescer } from "./browser-sidebar"

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

beforeEach(() => {
  cookieStorage = installLocalStorage()
})

afterEach(() => {
  resetBridge()
  vi.restoreAllMocks()
  document.documentElement.removeAttribute("style")
  cookieStorage?.clear()
})

describe("BrowserSidebar", () => {
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

  it("captures submitted browser tool context and adds it to the agent composer", async () => {
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
    expect(request.prompt).not.toContain("Tighten the spacing here")
    expect(request.visiblePrompt).toBe("Tighten the spacing here")
    expect(Array.from(request.image.bytes)).toEqual([104, 101, 108, 108, 111])
    expect(request.image.originalName).toMatch(/^browser-inspect-/)
    expect(invokeCalls.some((call) => call.command === "browser_screenshot")).toBe(true)
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
    expect(script).toContain('path.style.stroke = "url(#" + gradientId + ")"')
  })
})
