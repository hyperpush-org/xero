import { fireEvent, render, screen, waitFor } from "@testing-library/react"
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
      attachCustomKeyEventHandler: () => void
      onData: (handler: (data: string) => void) => void
      dataHandler?: (data: string) => void
      onResize: (handler: (size: { cols: number; rows: number }) => void) => void
      onTitleChange: (handler: (title: string) => void) => void
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
    attachCustomKeyEventHandler() {}
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

import { TerminalSidebar } from "./terminal-sidebar"

const basePersistedTab = {
  clientId: "client-web",
  label: "web",
  labelLocked: true,
  browserSupported: true,
  cwd: "/repo/project-a",
  command: {
    text: "pnpm dev",
    sourceKind: "start-target",
    sourceId: "target-web",
    sourceLabel: "web",
    autoReplay: false,
  },
}

function persistedState(
  tabs: Array<typeof basePersistedTab>,
  activeTabId: string | null = tabs[0]?.clientId ?? null,
) {
  return {
    schema: "xero.terminal.tabs.v1",
    tabs,
    activeTabId,
  }
}

function setupAdapter({
  states = new Map<string, unknown>(),
  transcripts = new Map<string, string>(),
}: {
  states?: Map<string, unknown>
  transcripts?: Map<string, string>
} = {}) {
  let nextTerminal = 1
  mocks.adapter.readProjectUiState.mockImplementation(async ({ projectId }: { projectId: string }) => ({
    schema: "xero.project_ui_state.v1",
    projectId,
    key: "terminal.tabs.v1",
    value: states.get(projectId) ?? null,
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
      content: transcripts.get(clientTerminalId) ?? "",
    }),
  )
  mocks.adapter.terminalWrite.mockResolvedValue(undefined)
  mocks.adapter.terminalResize.mockResolvedValue(undefined)
  mocks.adapter.terminalClose.mockResolvedValue(undefined)
  mocks.adapter.terminalClearTranscript.mockResolvedValue(undefined)
}

function emitTerminalData(terminalId: string, data: string) {
  for (const listener of mocks.listeners.get("terminal:data") ?? []) {
    listener({ payload: { terminalId, data } })
  }
}

describe("TerminalSidebar persistence", () => {
  beforeEach(() => {
    mocks.listeners.clear()
    mocks.terminals.length = 0
    Object.values(mocks.adapter).forEach((mock) => mock.mockReset())
    setupAdapter()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it("hydrates project tabs from app-data and replays saved transcript without replaying commands", async () => {
    setupAdapter({
      states: new Map([
        [
          "project-a",
          persistedState([basePersistedTab]),
        ],
      ]),
      transcripts: new Map([["client-web", "old prompt\r\nold output\r\n"]]),
    })

    render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    expect(mocks.adapter.terminalOpen).toHaveBeenCalledWith({
      projectId: "project-a",
      clientTerminalId: "client-web",
      cols: 120,
      rows: 32,
      suppressTranscriptUntilInput: true,
    })
    expect(mocks.terminals[0].writes.join("")).toContain("old output")
    expect(mocks.adapter.terminalWrite).not.toHaveBeenCalledWith(
      "pty-1",
      expect.stringContaining("pnpm dev"),
    )

    await waitFor(() => expect(mocks.adapter.writeProjectUiState).toHaveBeenCalled())
    const write = mocks.adapter.writeProjectUiState.mock.calls.at(-1)?.[0]
    expect(write.value.tabs[0]).toMatchObject({
      clientId: "client-web",
      label: "web",
      command: expect.objectContaining({ text: "pnpm dev", autoReplay: false }),
    })
    expect(write.value.tabs[0]).not.toHaveProperty("id")
    expect(write.value.tabs[0]).not.toHaveProperty("terminalId")
  })

  it("switches projects by hiding and closing the old project's PTY, then restoring the next project", async () => {
    setupAdapter({
      states: new Map([
        ["project-a", persistedState([basePersistedTab])],
        [
          "project-b",
          persistedState([
            {
              ...basePersistedTab,
              clientId: "client-api",
              label: "api",
              cwd: "/repo/project-b",
            },
          ]),
        ],
      ]),
    })

    const { rerender } = render(<TerminalSidebar open projectId="project-a" />)
    expect(await screen.findByRole("button", { name: "web" })).toBeVisible()

    rerender(<TerminalSidebar open projectId="project-b" />)

    expect(screen.queryByRole("button", { name: "web" })).not.toBeInTheDocument()
    expect(await screen.findByRole("button", { name: "api" })).toBeVisible()
    await waitFor(() => expect(mocks.adapter.terminalClose).toHaveBeenCalledWith("pty-1"))
    expect(mocks.adapter.terminalOpen).toHaveBeenLastCalledWith({
      projectId: "project-b",
      clientTerminalId: "client-api",
      cols: 120,
      rows: 32,
      suppressTranscriptUntilInput: true,
    })
  })

  it("clears transcript storage and removes the descriptor when a tab is closed", async () => {
    setupAdapter({
      states: new Map([["project-a", persistedState([basePersistedTab])]]),
    })

    render(<TerminalSidebar open projectId="project-a" />)
    const closeButton = await screen.findByRole("button", { name: "Close terminal" })

    fireEvent.click(closeButton)

    await waitFor(() =>
      expect(mocks.adapter.terminalClearTranscript).toHaveBeenCalledWith({
        projectId: "project-a",
        clientTerminalId: "client-web",
      }),
    )
    await waitFor(() => {
      const values = mocks.adapter.writeProjectUiState.mock.calls.map(([request]) => request.value)
      expect(
        values.some((value) =>
          Array.isArray(value.tabs) &&
          value.tabs.every((tab: { clientId: string }) => tab.clientId !== "client-web"),
        ),
      ).toBe(true)
    })
  })

  it("wipes malformed persisted state and falls back to a fresh terminal", async () => {
    setupAdapter({
      states: new Map([["project-a", { schema: "legacy-terminal-state" }]]),
    })

    render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() =>
      expect(mocks.adapter.writeProjectUiState).toHaveBeenCalledWith({
        projectId: "project-a",
        key: "terminal.tabs.v1",
        value: null,
      }),
    )
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    expect(mocks.adapter.terminalReadTranscript).not.toHaveBeenCalled()
  })

  it("suppresses restored shell startup output until user input", async () => {
    setupAdapter({
      states: new Map([["project-a", persistedState([basePersistedTab])]]),
      transcripts: new Map([["client-web", "old prompt\r\nold output\r\n"]]),
    })

    render(<TerminalSidebar open projectId="project-a" />)

    await waitFor(() => expect(mocks.listeners.get("terminal:data")?.length).toBeGreaterThan(0))
    await waitFor(() => expect(mocks.adapter.terminalOpen).toHaveBeenCalledTimes(1))
    emitTerminalData("pty-1", "duplicate startup\r\n")
    expect(mocks.terminals[0].writes.join("")).toContain("old output")
    expect(mocks.terminals[0].writes.join("")).not.toContain("duplicate startup")

    mocks.terminals[0].dataHandler?.("git status\r")
    expect(mocks.adapter.terminalWrite).toHaveBeenCalledWith("pty-1", "git status\r")

    emitTerminalData("pty-1", "new command output\r\n")
    expect(mocks.terminals[0].writes.join("")).toContain("new command output")
  })

  it("does not wipe persisted tabs when StrictMode cleanup runs before hydration finishes", () => {
    let resolveRead: (value: unknown) => void = () => undefined
    mocks.adapter.readProjectUiState.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveRead = resolve
        }),
    )

    const { unmount } = render(<TerminalSidebar open projectId="project-a" />)

    unmount()
    resolveRead({
      schema: "xero.project_ui_state.v1",
      projectId: "project-a",
      key: "terminal.tabs.v1",
      value: persistedState([basePersistedTab]),
      storageScope: "os_app_data",
      uiDeferred: true,
    })

    expect(mocks.adapter.writeProjectUiState).not.toHaveBeenCalled()
    expect(mocks.adapter.terminalOpen).not.toHaveBeenCalled()
  })

  it("restores the persisted active tab instead of selecting the last hydrated tab", async () => {
    setupAdapter({
      states: new Map([
        [
          "project-a",
          persistedState(
            [
              basePersistedTab,
              {
                ...basePersistedTab,
                clientId: "client-api",
                label: "api",
              },
            ],
            "client-web",
          ),
        ],
      ]),
    })

    render(<TerminalSidebar open projectId="project-a" />)

    const webTab = await screen.findByRole("button", { name: "web" })
    await screen.findByRole("button", { name: "api" })

    expect(webTab.closest("div")).toHaveClass("text-foreground")
  })

  it("switches tabs when clicking the visual tab outside the label text", async () => {
    setupAdapter({
      states: new Map([
        [
          "project-a",
          persistedState(
            [
              basePersistedTab,
              {
                ...basePersistedTab,
                clientId: "client-api",
                label: "api",
              },
            ],
            "client-web",
          ),
        ],
      ]),
    })

    render(<TerminalSidebar open projectId="project-a" />)

    const apiLabelButton = await screen.findByRole("button", { name: "api" })
    const apiTab = apiLabelButton.closest("div")
    expect(apiTab).not.toBeNull()
    expect(apiTab).not.toHaveClass("text-foreground")

    fireEvent.click(apiTab!)

    await waitFor(() => expect(apiTab).toHaveClass("text-foreground"))
  })
})
