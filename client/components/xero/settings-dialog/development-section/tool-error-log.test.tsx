import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { ToolErrorLog } from "./tool-error-log"

const adapterMock = vi.hoisted(() => ({
  isDesktopRuntime: vi.fn(() => true),
  listProjects: vi.fn(),
  developerToolErrorLogList: vi.fn(),
  developerToolErrorLogClear: vi.fn(),
}))

vi.mock("@/src/lib/xero-desktop", () => ({
  XeroDesktopAdapter: adapterMock,
}))

const sampleEntry = {
  id: "tool-error-1",
  occurredAt: "2026-06-02T14:05:00Z",
  source: "tool_registry_v2_dispatch",
  projectId: "project-1",
  agentSessionId: "session-1",
  runId: "run-1",
  turnIndex: 3,
  toolCallId: "call-1",
  toolName: "write",
  inputSha256: "a".repeat(64),
  inputJson: { path: "src/main.rs", api_key: "[REDACTED]" },
  inputRedacted: true,
  errorCode: "agent_tool_write_failed",
  errorClass: "retryable",
  errorCategory: "retryable_provider_tool_failure",
  errorMessage: "The write tool failed.",
  modelMessage: "Retry with different input.",
  retryable: true,
  dispatchJson: { groupMode: "sequential_mutating", elapsedMs: 17 },
  contextJson: { providerId: "openai_codex", modelId: "gpt-5", launchMode: "local-source" },
  messagePreview: "The write tool failed.",
}

function project(id: string, name: string) {
  return {
    id,
    name,
    description: "",
    milestone: "",
    projectOrigin: "brownfield",
    totalPhases: 0,
    completedPhases: 0,
    activePhase: 0,
    branch: null,
    runtime: null,
    startTargets: [],
  }
}

function response(entries = [sampleEntry], projectIds = ["project-1"]) {
  return {
    databasePath: "/tmp/xero/development/tool-call-errors.sqlite",
    entries,
    projectIds,
    totalCount: entries.length,
    limit: 100,
    offset: 0,
  }
}

beforeEach(() => {
  adapterMock.isDesktopRuntime.mockReset()
  adapterMock.isDesktopRuntime.mockReturnValue(true)
  adapterMock.listProjects.mockReset()
  adapterMock.listProjects.mockResolvedValue({
    projects: [project("project-1", "Project One")],
  })
  adapterMock.developerToolErrorLogList.mockReset()
  adapterMock.developerToolErrorLogClear.mockReset()
  adapterMock.developerToolErrorLogList.mockResolvedValue(response())
  adapterMock.developerToolErrorLogClear.mockResolvedValue({
    databasePath: "/tmp/xero/development/tool-call-errors.sqlite",
    clearedCount: 1,
  })
})

describe("ToolErrorLog", () => {
  it("renders the loading state before the first response resolves", async () => {
    adapterMock.developerToolErrorLogList.mockImplementation(
      () => new Promise(() => undefined),
    )

    render(<ToolErrorLog />)

    expect(await screen.findByText("Loading tool-call failures...")).toBeVisible()
  })

  it("renders an empty state", async () => {
    adapterMock.developerToolErrorLogList.mockResolvedValue(response([]))

    render(<ToolErrorLog />)

    expect(await screen.findByText("No tool-call failures logged.")).toBeVisible()
    expect(screen.getByText("0")).toBeVisible()
  })

  it("renders populated rows and the selected details panel", async () => {
    render(<ToolErrorLog />)

    expect(await screen.findByRole("button", { name: "Inspect write failure" })).toBeVisible()
    expect(screen.getByLabelText("Fuzzy search")).toBeVisible()
    expect(screen.getByRole("combobox", { name: "Project" })).toHaveTextContent("All projects")
    expect(screen.getByText("agent_tool_write_failed")).toBeVisible()
    expect(screen.getByText("Retryable")).toBeVisible()
    expect(screen.getByText("Redacted input")).toBeVisible()
    expect(screen.getByText(/tool-call-errors\.sqlite/)).toBeVisible()
    expect(screen.getByText(/REDACTED/)).toBeVisible()
  })

  it("does not render manual actions or removed filter controls", async () => {
    render(<ToolErrorLog />)
    await screen.findByRole("button", { name: "Inspect write failure" })

    expect(screen.queryByRole("button", { name: "Refresh" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Clear" })).not.toBeInTheDocument()
    expect(screen.queryByLabelText("Tool name")).not.toBeInTheDocument()
    expect(screen.queryByLabelText("Error code")).not.toBeInTheDocument()
  })

  it("sends debounced fuzzy search requests through the typed adapter", async () => {
    render(<ToolErrorLog />)
    await screen.findByRole("button", { name: "Inspect write failure" })

    fireEvent.change(screen.getByLabelText("Fuzzy search"), {
      target: { value: "denied" },
    })

    await waitFor(() =>
      expect(adapterMock.developerToolErrorLogList).toHaveBeenLastCalledWith({
        limit: 100,
        query: "denied",
      }),
    )
  })

  it("sends project dropdown requests through the typed adapter", async () => {
    adapterMock.listProjects.mockResolvedValue({
      projects: [
        project("project-1", "Project One"),
        project("project-2", "Project Two"),
      ],
    })
    adapterMock.developerToolErrorLogList.mockResolvedValue(response([sampleEntry], []))

    render(<ToolErrorLog />)
    await screen.findByRole("button", { name: "Inspect write failure" })

    ensurePointerCaptureApi()
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Project" }), {
      button: 0,
      pointerId: 1,
      pointerType: "mouse",
    })
    fireEvent.click(await screen.findByRole("option", { name: "Project Two" }))

    await waitFor(() =>
      expect(adapterMock.developerToolErrorLogList).toHaveBeenLastCalledWith({
        limit: 100,
        projectId: "project-2",
      }),
    )
  })

  it("lists projects in the dropdown even when no failures are logged", async () => {
    adapterMock.listProjects.mockResolvedValue({
      projects: [
        project("project-1", "Project One"),
        project("project-2", "Project Two"),
        project("project-3", "Project Three"),
      ],
    })
    adapterMock.developerToolErrorLogList.mockResolvedValue(response([], []))

    render(<ToolErrorLog />)
    await screen.findByText("No tool-call failures logged.")

    ensurePointerCaptureApi()
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Project" }), {
      button: 0,
      pointerId: 1,
      pointerType: "mouse",
    })

    expect(await screen.findByRole("option", { name: "Project One" })).toBeVisible()
    expect(screen.getByRole("option", { name: "Project Two" })).toBeVisible()
    expect(screen.getByRole("option", { name: "Project Three" })).toBeVisible()
  })

  it("renders command errors", async () => {
    adapterMock.developerToolErrorLogList.mockRejectedValue(new Error("dev log disabled"))

    render(<ToolErrorLog />)

    expect(await screen.findByText("Tool-call failures unavailable")).toBeVisible()
    expect(screen.getByText("dev log disabled")).toBeVisible()
  })

})

function ensurePointerCaptureApi() {
  if (!HTMLElement.prototype.hasPointerCapture) {
    Object.defineProperty(HTMLElement.prototype, "hasPointerCapture", {
      configurable: true,
      value: () => false,
    })
  }
  if (!HTMLElement.prototype.setPointerCapture) {
    Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
      configurable: true,
      value: () => undefined,
    })
  }
  if (!HTMLElement.prototype.releasePointerCapture) {
    Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", {
      configurable: true,
      value: () => undefined,
    })
  }
}
