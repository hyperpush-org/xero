import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import type { StartTargetsModelOption } from "@/components/xero/start-targets-editor"
import { TERMINAL_SUGGESTION_SETTINGS_KEY } from "@/components/xero/terminal-suggestion-settings"

import { TerminalSection } from "./terminal-section"

const modelOptions: StartTargetsModelOption[] = [
  {
    selectionKey: "xai:grok-4.3-latest",
    providerId: "xai",
    providerProfileId: "xai-default",
    providerLabel: "xAI / Grok",
    modelId: "grok-4.3-latest",
    label: "Grok 4.3",
    thinkingEffortOptions: ["medium", "high"],
    defaultThinkingEffort: "medium",
  },
  {
    selectionKey: "openai_codex:gpt-5.4",
    providerId: "openai_codex",
    providerProfileId: "openai_codex-default",
    providerLabel: "OpenAI Codex",
    modelId: "gpt-5.4",
    label: "GPT-5.4",
    thinkingEffortOptions: ["low", "medium", "high"],
    defaultThinkingEffort: "low",
  },
]

function ensurePointerCaptureApi() {
  for (const [name, value] of [
    ["hasPointerCapture", () => false],
    ["setPointerCapture", () => undefined],
    ["releasePointerCapture", () => undefined],
  ] as const) {
    if (!(name in HTMLElement.prototype)) {
      Object.defineProperty(HTMLElement.prototype, name, {
        configurable: true,
        value,
      })
    }
  }
}

describe("TerminalSection", () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  it("explains terminal suggestion modes and persists a dedicated AI model", async () => {
    render(<TerminalSection modelOptions={modelOptions} />)

    expect(screen.getByRole("heading", { name: "Terminal" })).toBeVisible()
    expect(screen.getByText("Command suggestions")).toBeVisible()
    expect(screen.getByText("Local")).toBeVisible()
    expect(screen.getByText(/recent terminal commands, shell history/i)).toBeVisible()
    expect(screen.getByText("AI suggestions")).toBeVisible()
    expect(screen.getByText("Fallback")).toBeVisible()
    expect(screen.getByText("Model")).toBeVisible()
    expect(screen.getByText(/active chat model/i)).toBeVisible()

    fireEvent.click(screen.getByRole("switch", { name: "AI suggestions" }))

    ensurePointerCaptureApi()
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Model" }), {
      button: 0,
      pointerId: 1,
      pointerType: "mouse",
    })
    expect(screen.getByRole("option", { name: "Default model" })).toBeVisible()
    expect(screen.queryByRole("option", { name: "Provider default" })).not.toBeInTheDocument()
    fireEvent.click(await screen.findByRole("option", { name: "GPT-5.4" }))

    const thinkingItem = screen.getByRole("menuitem", { name: /Thinking/i })
    fireEvent.keyDown(thinkingItem, { key: "ArrowRight" })
    fireEvent.click(screen.getByRole("menuitemradio", { name: "High" }))

    await waitFor(() => {
      const persisted = JSON.parse(
        window.localStorage.getItem(TERMINAL_SUGGESTION_SETTINGS_KEY) ?? "null",
      )
      expect(persisted).toMatchObject({
        enabled: true,
        aiEnabled: true,
        modelSelection: {
          providerId: "openai_codex",
          providerProfileId: "openai_codex-default",
          modelId: "gpt-5.4",
          runtimeAgentId: null,
          thinkingEffort: "high",
        },
      })
    })
  })
})
