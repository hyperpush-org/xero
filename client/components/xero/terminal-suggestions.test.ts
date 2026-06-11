import { describe, expect, it } from "vitest"

import {
  StaleTerminalSuggestionGate,
  TerminalInputTracker,
  acceptedSuggestionWrite,
  isProbablySecretCommand,
  shouldShowCandidate,
  type TerminalSuggestionCandidate,
} from "./terminal-suggestions"

function candidate(overrides: Partial<TerminalSuggestionCandidate> = {}): TerminalSuggestionCandidate {
  return {
    replacement: " status",
    display: "git status",
    description: "Show working tree status",
    source: "command",
    confidence: 0.9,
    replacementRange: { start: 3, end: 3 },
    ...overrides,
  }
}

describe("TerminalInputTracker", () => {
  it("tracks normal shell edits without becoming terminal truth", () => {
    const tracker = new TerminalInputTracker()

    tracker.applyInput("g")
    tracker.applyInput("i")
    tracker.applyInput("x")
    tracker.applyInput("\x7f")
    tracker.applyInput("t")

    expect(tracker.snapshot()).toMatchObject({
      buffer: "git",
      cursor: 3,
      suppressed: false,
    })
  })

  it("emits submitted commands and clears suppression after Enter", () => {
    const tracker = new TerminalInputTracker()

    tracker.applyInput("git status")
    const submitted = tracker.applyInput("\r")

    expect(submitted).toMatchObject({ kind: "submit", command: "git status" })
    expect(tracker.snapshot()).toMatchObject({ buffer: "", cursor: 0, suppressed: false })
  })

  it("tracks shell editing shortcuts used by terminal keybindings", () => {
    const tracker = new TerminalInputTracker()

    tracker.applyInput("git checkout main")
    tracker.applyInput("\x1bb")
    tracker.applyInput("\x17")

    expect(tracker.snapshot()).toMatchObject({ buffer: "git main", cursor: 4 })

    tracker.applyInput("\x1bf")
    tracker.applyInput("\x15")

    expect(tracker.snapshot()).toMatchObject({ buffer: "", cursor: 0 })

    tracker.applyInput("git status")
    tracker.applyInput("\x01")
    tracker.applyInput("\x1bf")
    tracker.applyInput("\x1bd")

    expect(tracker.snapshot()).toMatchObject({ buffer: "git", cursor: 3 })
  })

  it("suppresses suggestions for paste bursts, password prompts, and alternate-screen apps", () => {
    const tracker = new TerminalInputTracker()

    tracker.applyInput("pnpm install --frozen-lockfile")
    expect(tracker.snapshot().suppressionReason).toBe("paste")

    tracker.applyInput("\r")
    tracker.observeOutput("Password: ")
    expect(tracker.snapshot().suppressionReason).toBe("password-prompt")

    tracker.applyInput("\r")
    tracker.observeOutput("\x1b[?1049h")
    expect(tracker.snapshot().suppressionReason).toBe("alternate-screen")

    tracker.observeOutput("\x1b[?1049l")
    expect(tracker.snapshot().suppressed).toBe(false)
  })
})

describe("terminal suggestion acceptance", () => {
  it("accepts full or word-level deltas without submitting Enter", () => {
    const full = candidate({ replacement: " checkout -b feature/foo" })

    expect(acceptedSuggestionWrite(full, "full")).toBe(" checkout -b feature/foo")
    expect(acceptedSuggestionWrite(full, "word")).toBe(" checkout ")
  })

  it("only renders cursor-local safe replacements", () => {
    const tracker = new TerminalInputTracker()
    tracker.applyInput("git")
    const snapshot = tracker.snapshot()

    expect(shouldShowCandidate(snapshot, candidate())).toBe(true)
    expect(shouldShowCandidate(snapshot, candidate({ replacement: " status\r" }))).toBe(false)
    expect(
      shouldShowCandidate(snapshot, candidate({ replacementRange: { start: 0, end: 3 } })),
    ).toBe(false)
  })

  it("ignores stale async suggestion responses", () => {
    const gate = new StaleTerminalSuggestionGate()
    const first = gate.next()
    const second = gate.next()

    expect(gate.isCurrent(first)).toBe(false)
    expect(gate.isCurrent(second)).toBe(true)
  })

  it("redacts secret-like commands before history or suggestion reuse", () => {
    expect(isProbablySecretCommand("export OPENAI_API_KEY=sk-test")).toBe(true)
    expect(isProbablySecretCommand("curl -H 'Authorization: Bearer abc' https://example.test")).toBe(true)
    expect(isProbablySecretCommand("git status --short")).toBe(false)
  })
})
