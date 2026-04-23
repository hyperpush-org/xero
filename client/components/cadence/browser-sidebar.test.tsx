/** @vitest-environment jsdom */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

type ListenerHandle = () => void
type InvokeHandler = (args: Record<string, unknown> | undefined) => unknown

const invokeResponses = new Map<string, InvokeHandler>()
const eventListeners = new Map<string, ((event: { payload: unknown }) => void)[]>()

function resetBridge() {
  invokeResponses.clear()
  eventListeners.clear()
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

import { BrowserSidebar } from "./browser-sidebar"

afterEach(() => {
  resetBridge()
  vi.restoreAllMocks()
})

describe("BrowserSidebar", () => {
  it("hydrates existing tabs when opened", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "cadence-browser",
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
        label: "cadence-browser",
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
        label: "cadence-browser",
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

  it("applies the resize handle inset to browser_show so the handle stays clickable", async () => {
    registerInvoke("browser_tab_list", async () => [])
    let recordedArgs: Record<string, unknown> | null = null
    registerInvoke("browser_show", async (args) => {
      recordedArgs = (args as Record<string, unknown>) ?? null
      return {
        id: "tab-1",
        label: "cadence-browser",
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

  it("updates the address bar when a load_state event delivers a new URL while unfocused", async () => {
    registerInvoke("browser_tab_list", async () => [
      {
        id: "tab-1",
        label: "cadence-browser",
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
})
