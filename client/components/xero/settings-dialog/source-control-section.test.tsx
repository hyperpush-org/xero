import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import type { StartTargetsModelOption } from "@/components/xero/start-targets-editor"
import { SOURCE_CONTROL_SETTINGS_KEY } from "@/components/xero/source-control-settings"

import { SourceControlSection } from "./source-control-section"

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

describe("SourceControlSection", () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  it("persists a dedicated commit message model and thinking level", async () => {
    render(<SourceControlSection modelOptions={modelOptions} />)

    expect(screen.getByRole("heading", { name: "Source Control" })).toBeVisible()
    expect(screen.getByText("Commit message model")).toBeVisible()
    expect(screen.getByText(/last active chat model and thinking level/i)).toBeVisible()

    ensurePointerCaptureApi()
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Commit message model" }), {
      button: 0,
      pointerId: 1,
      pointerType: "mouse",
    })
    expect(screen.getByRole("option", { name: "Default model" })).toBeVisible()
    fireEvent.click(await screen.findByRole("option", { name: "GPT-5.4" }))

    const thinkingItem = screen.getByRole("menuitem", { name: /Thinking/i })
    fireEvent.keyDown(thinkingItem, { key: "ArrowRight" })
    fireEvent.click(screen.getByRole("menuitemradio", { name: "High" }))

    await waitFor(() => {
      const persisted = JSON.parse(
        window.localStorage.getItem(SOURCE_CONTROL_SETTINGS_KEY) ?? "null",
      )
      expect(persisted).toMatchObject({
        commitMessageModelSelection: {
          providerId: "openai_codex",
          providerProfileId: "openai_codex-default",
          modelId: "gpt-5.4",
          thinkingEffort: "high",
        },
      })
    })
  })
})
