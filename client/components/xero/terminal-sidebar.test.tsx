import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => {
  const listeners = new Map<string, ((event: { payload: unknown }) => void)[]>()
  return {
    listeners,
    terminals: [] as Array<{
      writes: string[]
      options: Record<string, unknown>
      write: (data: string) => void
      open: () => void
      focus: () => void
      dispose: () => void
      loadAddon: () => void
      attachCustomKeyEventHandler: (handler: (event: KeyboardEvent) => boolean) => void
      customKeyHandler?: (event: KeyboardEvent) => boolean
      onData: (handler: (data: string) => void) => void
      dataHandler?: (data: string) => void
      onResize: (handler: (size: { cols: number; rows: number }) => void) => void
      onTitleChange: (handler: (title: string) => void) => void
      buffer: { active: { cursorX: number; cursorY: number } }
      _core: { _renderService: { dimensions: { css: { cell: { width: number; height: number } } } } }
      cols: number
      rows: number
    }>,
    adapter: {
      readProjectUiState: vi.fn(),
      writeProjectUiState: vi.fn(),
      terminalOpen: vi.fn(),
      terminalWrite: vi.fn(),
      terminalResize: vi.fn(),
      terminalClose: vi.fn(),
      terminalReadTranscript: vi.fn(),
      terminalClearTranscript: vi.fn(),
      terminalSuggest: vi.fn(),
      terminalRecordCommand: vi.fn(),
      terminalIgnoreSuggestion: vi.fn(),
    },
  }
})

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
}))

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ) => {
    const listeners = mocks.listeners.get(eventName) ?? []
    listeners.push(handler)
    mocks.listeners.set(eventName, listeners)
    return () => {
      const current = mocks.listeners.get(eventName) ?? []
      mocks.listeners.set(
        eventName,
        current.filter((entry) => entry !== handler),
      )
    }
  },
}))

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    writes: string[] = []
    options: Record<string, unknown> = {}
    dataHandler?: (data: string) => void
    buffer = { active: { cursorX: 12, cursorY: 2 } }
    _core = {
      _renderService: {
        dimensions: { css: { cell: { width: 9, height: 18 } } },
      },
    }
    cols = 120
    rows = 32

    constructor(options: Record<string, unknown>) {
      this.options = options
      mocks.terminals.push(this)
    }

    write(data: string) {
      this.writes.push(data)
    }

    open() {}
    focus() {}
    dispose() {}
    loadAddon() {}
    customKeyHandler?: (event: KeyboardEvent) => boolean
    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean) {
      this.customKeyHandler = handler
    }
    onData(handler: (data: string) => void) {
      this.dataHandler = handler
    }
    onResize() {}
    onTitleChange() {}
  },
}))

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockFitAddon {
    fit() {}
  },
}))

vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class MockWebLinksAddon {
    constructor() {}
  },
}))

vi.mock("@/src/lib/xero-desktop", () => ({
  XeroDesktopAdapter: mocks.adapter,
}))

import { TerminalSidebar, type TerminalSidebarHandle } from "./terminal-sidebar"
import { TERMINAL_SUGGESTION_SETTINGS_KEY } from "./terminal-suggestion-settings"

function setupAdapter() {
  let nextTerminal = 1
  mocks.adapter.readProjectUiState.mockImplementation(async ({ projectId }: { projectId: string }) => ({
    schema: "xero.project_ui_state.v1",
    projectId,
    key: "terminal.tabs.v1",
    value: null,
    storageScope: "os_app_data",
    uiDeferred: true,
  }))
  mocks.adapter.writeProjectUiState.mockImplementation(async (request) => ({
    schema: "xero.project_ui_state.v1",
    projectId: request.projectId,
    key: request.key,
    value: request.value,
    storageScope: "os_app_data",
    uiDeferred: true,
  }))
  mocks.adapter.terminalOpen.mockImplementation(async (request) => ({
    terminalId: `pty-${nextTerminal++}`,
    shell: "/bin/zsh",
    cwd: `/repo/${request.projectId}`,
    startedAt: "2026-06-01T12:00:00Z",
  }))
  mocks.adapter.terminalReadTranscript.mockImplementation(
    async ({ projectId, clientTerminalId }) => ({
      projectId,
      clientTerminalId,
      content: "",
    }),
  )
  mocks.adapter.terminalWrite.mockResolvedValue(undefined)
  mocks.adapter.terminalResize.mockResolvedValue(undefined)
  mocks.adapter.terminalClose.mockResolvedValue(undefined)
  mocks.adapter.terminalClearTranscript.mockResolvedValue(undefined)
  mocks.adapter.terminalSuggest.mockResolvedValue({
    requestId: 1,
    candidates: [],
    deterministicExhausted: true,
    aiAttempted: false,
  })
  mocks.adapter.terminalRecordCommand.mockResolvedValue(undefined)
  mocks.adapter.terminalIgnoreSuggestion.mockResolvedValue(undefined)
}

function emitTerminalData(terminalId: string, data: string) {
  for (const listener of mocks.listeners.get("terminal:data") ?? []) {
    listener({ payload: { terminalId, data } })
  }
}

function renderWithHandle(projectId: string) {
  const handleRef: { current: TerminalSidebarHandle | null } = { current: null }
  const view = render(
    <TerminalSidebar
      open
      projectId={projectId}
      registerHandle={(handle) => {
        handleRef.current = handle
      }}
    />,
  )
  return { ...view, handleRef }
}

async function spawnLabeledTab(
  handleRef: { current: TerminalSidebarHandle | null },
  label: string,
): Promise<string | null> {
  await waitFor(() => expect(handleRef.current).not.toBeNull())
  let terminalId: string | null = null
  await act(async () => {
    terminalId = await handleRef.current?.spawnTabWithCommand("", {
      label,
      source: { kind: "xero-command", label },
    }) ?? null
  })
  await screen.findByRole("button", { name: label })
  return terminalId
}

describe("TerminalSidebar session lifetime", () => {
  beforeEach(() => {
    mocks.listeners.clear()
    mocks.terminals.length = 0
    Object.values(mocks.adapter).forEach((mock) => mock.mockReset())
    window.localStorage.clear()
    setupAdapter()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it("starts fresh on cold mount and ignores stale app-data terminal tabs", async () => {
    render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledWith({
      projectId: "project-a",
      clientTerminalId: null,
      cols: 120,
      rows: 32,
      suppressTranscriptUntilInput: false,
    })
    expect(screen.queryByRole("button", { name: "web" })).not.toBeInTheDocument()
    expect(mocks.adapter.readProjectUiState).not.toHaveBeenCalled()
    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
    expect(mocks.adapter.terminalReadTranscript).not.toHaveBeenCalled()
    expect(mocks.adapter.terminalClearTranscript).not.toHaveBeenCalled()
  })

  it("lifts xterm custom scrollbars above elevated chrome", async () => {
    render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    const terminalViewportStyle = document.querySelector(
      ".xero-terminal-viewport style",
    )
    const css = terminalViewportStyle?.textContent ?? ""

    expect(css).toContain(
      ".xero-terminal-viewport .xterm .xterm-scrollable-element > .scrollbar",
    )
    expect(css).toContain("z-index: var(--scrollbar-z-index) !important;")
  })

  it("keeps project PTYs alive across project switches without durable writes", async () => {
    const { rerender } = render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    emitTerminalData("pty-1", "project a output\r\n")
    expect(mocks.terminals[0].writes.join("")).toContain("project a output")

    rerender(<TerminalSidebar open projectId="project-b" />)

    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(2))
    expect(mocks.adapter.terminalClose).not.toHaveBeenCalledWith("pty-1")
    expect(mocks.adapter.terminalOpen).toHaveBeenLastCalledWith({
      projectId: "project-b",
      clientTerminalId: null,
      cols: 120,
      rows: 32,
      suppressTranscriptUntilInput: false,
    })

    emitTerminalData("pty-1", "still alive while hidden\r\n")
    expect(mocks.terminals[0].writes.join("")).toContain("still alive while hidden")

    rerender(<TerminalSidebar open projectId="project-a" />)

    expect(await screen.findByRole("button", { name: "zsh" })).toBeVisible()
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(2)
    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
  })

  it("replaces an unused auto-created blank tab when launching a project command", async () => {
    const { handleRef } = renderWithHandle("project-a")
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    await act(async () => {
      await handleRef.current?.spawnTabWithCommand("pnpm dev", {
        label: "landing",
        browserSupported: true,
        source: {
          kind: "start-target",
          targetId: "target-landing",
          targetName: "landing",
        },
      })
    })

    expect(await screen.findByRole("button", { name: "landing" })).toBeVisible()
    expect(screen.queryByRole("button", { name: "zsh" })).not.toBeInTheDocument()
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "pnpm dev\r"))
  })

  it("stamps detected browser launch targets with the terminal tab project", async () => {
    const detected: unknown[] = []
    const handleRef: { current: TerminalSidebarHandle | null } = { current: null }
    render(
      <TerminalSidebar
        open
        projectId="project-a"
        onBrowserLaunchTargetDetected={(target) => detected.push(target)}
        registerHandle={(handle) => {
          handleRef.current = handle
        }}
      />,
    )
    await waitFor(() => expect(handleRef.current).not.toBeNull())

    await act(async () => {
      await handleRef.current?.spawnTabWithCommand("pnpm dev", {
        label: "web",
        browserSupported: true,
      })
    })

    emitTerminalData("pty-1", "Local: http://localhost:4100/\r\n")

    await waitFor(() => {
      expect(detected.at(-1)).toMatchObject({
        label: "web · 127.0.0.1:4100",
        projectId: "project-a",
        url: "http://127.0.0.1:4100/",
      })
    })
  })

  it("claims the auto-opening blank tab when a project command arrives during terminal startup", async () => {
    let resolveFirstOpen: (response: {
      terminalId: string
      shell: string
      cwd: string
      startedAt: string
    }) => void = () => undefined
    mocks.adapter.terminalOpen.mockImplementationOnce(
      async (request) =>
        new Promise((resolve) => {
          resolveFirstOpen = resolve
        }).then(() => ({
          terminalId: "pty-1",
          shell: "/bin/zsh",
          cwd: `/repo/${request.projectId}`,
          startedAt: "2026-06-01T12:00:00Z",
        })),
    )

    const { handleRef } = renderWithHandle("project-a")
    await waitFor(() => expect(handleRef.current).not.toBeNull())
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    let commandPromise: Promise<string | null> = Promise.resolve(null)
    act(() => {
      commandPromise = handleRef.current?.spawnTabWithCommand("pnpm dev", {
        label: "landing",
        source: {
          kind: "start-target",
          targetId: "target-landing",
          targetName: "landing",
        },
      }) ?? Promise.resolve(null)
    })
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1)

    await act(async () => {
      resolveFirstOpen({
        terminalId: "pty-1",
        shell: "/bin/zsh",
        cwd: "/repo/project-a",
        startedAt: "2026-06-01T12:00:00Z",
      })
      await commandPromise
    })

    expect(await screen.findByRole("button", { name: "landing" })).toBeVisible()
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "pnpm dev\r"))
  })

  it("opens a new command tab when the existing blank terminal has user input", async () => {
    const { handleRef } = renderWithHandle("project-a")
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    mocks.terminals[0].dataHandler?.("git")
    await waitFor(() => expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "git"))

    await act(async () => {
      await handleRef.current?.spawnTabWithCommand("pnpm dev", {
        label: "landing",
        source: {
          kind: "start-target",
          targetId: "target-landing",
          targetName: "landing",
        },
      })
    })

    expect(await screen.findByRole("button", { name: "landing" })).toBeVisible()
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(2)
    await waitFor(() => expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-2", "pnpm dev\r"))
  })

  it("closes an explicit tab without clearing or writing durable terminal state", async () => {
    render(<TerminalSidebar open projectId="project-a" />)
    const closeButton = await screen.findByRole("button", { name: "Close terminal" })

    fireEvent.click(closeButton)

    await waitFor(() => expect(mocks.adapter.terminalClose).toHaveBeenCalledWith("pty-1"))
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(2))
    expect(mocks.adapter.terminalOpen).toHaveBeenLastCalledWith({
      projectId: "project-a",
      clientTerminalId: null,
      cols: 120,
      rows: 32,
      suppressTranscriptUntilInput: false,
    })
    expect(mocks.adapter.terminalClearTranscript).not.toHaveBeenCalled()
    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
  })

  it("does not persist unsubmitted input buffers on unmount", async () => {
    const { unmount } = render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    mocks.terminals[0].dataHandler?.("clear")
    unmount()

    await waitFor(() => expect(mocks.adapter.terminalClose).toHaveBeenCalledWith("pty-1"))
    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
    expect(mocks.adapter.terminalClearTranscript).not.toHaveBeenCalled()
  })

  it("cleans up live PTYs when the sidebar unmounts", async () => {
    const { unmount } = render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    unmount()

    await waitFor(() => expect(mocks.adapter.terminalClose).toHaveBeenCalledWith("pty-1"))
  })

  it("explains local and AI terminal suggestion modes in settings", async () => {
    render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    fireEvent.click(screen.getByRole("button", { name: "Terminal suggestion settings" }))

    expect(await screen.findByText("Inline terminal suggestions")).toBeVisible()
    expect(screen.getByText("Command suggestions")).toBeVisible()
    expect(screen.getByText("Local")).toBeVisible()
    expect(screen.getByText(/recent terminal commands, shell history, project files, and package scripts/i)).toBeVisible()
    expect(screen.getByText("AI suggestions")).toBeVisible()
    expect(screen.getByText("Fallback")).toBeVisible()
    expect(screen.getByText(/configured model when local sources have no useful match/i)).toBeVisible()
  })

  it("does not touch project UI state if unmounted immediately", () => {
    const { unmount } = render(<TerminalSidebar open projectId="project-a" />)

    unmount()

    expect(mocks.adapter.readProjectUiState).not.toHaveBeenCalled()
    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
  })

  it("remembers the active tab for each project in memory", async () => {
    const { handleRef, rerender } = renderWithHandle("project-a")
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    await spawnLabeledTab(handleRef, "web")
    await spawnLabeledTab(handleRef, "api")

    const webTab = await screen.findByRole("button", { name: "web" })
    await screen.findByRole("button", { name: "api" })
    fireEvent.click(webTab.closest("div")!)
    await waitFor(() => expect(webTab.closest("div")).toHaveClass("text-foreground"))

    rerender(
      <TerminalSidebar
        open
        projectId="project-b"
        registerHandle={(handle) => {
          handleRef.current = handle
        }}
      />,
    )
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(4))

    rerender(
      <TerminalSidebar
        open
        projectId="project-a"
        registerHandle={(handle) => {
          handleRef.current = handle
        }}
      />,
    )

    const restoredWebTab = await screen.findByRole("button", { name: "web" })
    expect(restoredWebTab).toBeVisible()
    expect(restoredWebTab.closest("div")).toHaveClass("text-foreground")
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(4)
  })

  it("switches tabs when clicking the visual tab outside the label text", async () => {
    const { handleRef } = renderWithHandle("project-a")
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    await spawnLabeledTab(handleRef, "web")
    await spawnLabeledTab(handleRef, "api")

    const webLabelButton = await screen.findByRole("button", { name: "web" })
    const webTab = webLabelButton.closest("div")
    expect(webTab).not.toBeNull()
    expect(webTab).not.toHaveClass("text-foreground")

    fireEvent.click(webTab!)

    await waitFor(() => expect(webTab).toHaveClass("text-foreground"))
  })

  it("renders ghost suggestions without writing them until accepted", async () => {
    mocks.adapter.terminalSuggest.mockImplementation(async (request) => ({
      requestId: request.requestId,
      candidates: [
        {
          replacement: " status",
          display: "git status",
          description: "Show working tree status",
          source: "command",
          confidence: 0.9,
          replacementRange: { start: 3, end: 3 },
        },
      ],
      deterministicExhausted: false,
      aiAttempted: false,
    }))

    render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    mocks.terminals[0].dataHandler?.("git")
    await new Promise((resolve) => window.setTimeout(resolve, 150))

    expect(await screen.findByText(/status/)).toBeVisible()
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "git")
    expect(mocks.adapter.terminalWrite).not.toHaveBeenCalledWith("pty-1", " status")

    const inlineSuggestion = await screen.findByTestId("terminal-inline-suggestion")
    expect(inlineSuggestion).toHaveStyle({ left: "120px", top: "48px" })
    expect(screen.queryByRole("option")).not.toBeInTheDocument()

    mocks.terminals[0].customKeyHandler?.(new KeyboardEvent("keydown", { key: "Tab" }))

    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", " status")
  })

  it("forwards common terminal text-navigation shortcuts as shell control sequences", async () => {
    render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    const handler = mocks.terminals[0].customKeyHandler
    expect(handler).toBeDefined()

    expect(handler?.(new KeyboardEvent("keydown", { key: "Backspace", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "Backspace", ctrlKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "Backspace", altKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "Delete", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowLeft", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowRight", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowUp", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowDown", metaKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "ArrowRight", altKey: true }))).toBe(false)
    expect(handler?.(new KeyboardEvent("keydown", { key: "Delete", ctrlKey: true }))).toBe(false)

    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x15")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x17")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x0b")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x01")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x05")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x1bb")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x1bf")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "\x1bd")
  })

  it("uses the configured AI model when requesting terminal suggestions", async () => {
    window.localStorage.setItem(
      TERMINAL_SUGGESTION_SETTINGS_KEY,
      JSON.stringify({
        enabled: true,
        aiEnabled: true,
        modelSelection: {
          providerId: "openai_codex",
          providerProfileId: "openai_codex-default",
          modelId: "gpt-5.4",
          runtimeAgentId: "ask",
          thinkingEffort: "low",
        },
      }),
    )
    mocks.adapter.terminalSuggest.mockImplementation(async (request) => ({
      requestId: request.requestId,
      candidates: [],
      deterministicExhausted: true,
      aiAttempted: true,
    }))

    render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    mocks.terminals[0].dataHandler?.("git")
    await waitFor(() =>
      expect(mocks.adapter.terminalSuggest).toHaveBeenCalledWith(
        expect.objectContaining({
          enableAi: true,
          providerId: "openai_codex",
          providerProfileId: "openai_codex-default",
          modelId: "gpt-5.4",
          runtimeAgentId: "ask",
          thinkingEffort: "low",
        }),
      ),
    )
  })

  it("records submitted commands and dismisses bad suggestions through app-data", async () => {
    mocks.adapter.terminalSuggest.mockImplementation(async (request) => ({
      requestId: request.requestId,
      candidates: [
        {
          replacement: " diff",
          display: "git diff",
          description: "Review changes",
          source: "history",
          confidence: 0.9,
          replacementRange: { start: 3, end: 3 },
        },
      ],
      deterministicExhausted: false,
      aiAttempted: false,
    }))

    render(<TerminalSidebar open projectId="project-a" />)
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))

    mocks.terminals[0].dataHandler?.("git")
    await new Promise((resolve) => window.setTimeout(resolve, 150))
    expect(await screen.findByText(/diff/)).toBeVisible()

    mocks.terminals[0].customKeyHandler?.(
      new KeyboardEvent("keydown", { key: "Escape" }),
    )
    await waitFor(() =>
      expect(mocks.adapter.terminalIgnoreSuggestion).toHaveBeenCalledWith({
        projectId: "project-a",
        display: "git diff",
      }),
    )

    mocks.terminals[0].dataHandler?.(" status")
    mocks.terminals[0].dataHandler?.("\r")

    await waitFor(() =>
      expect(mocks.adapter.terminalRecordCommand).toHaveBeenCalledWith({
        projectId: "project-a",
        command: "git status",
        cwd: "/repo/project-a",
        shell: "/bin/zsh",
      }),
    )
  })
})
