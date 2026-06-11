"use client"

export type TerminalSuggestionSource = "history" | "shell_history" | "path" | "command" | "next_command" | "ai"

export interface TerminalSuggestionReplacementRange {
  start: number
  end: number
}

export interface TerminalSuggestionCandidate {
  replacement: string
  display: string
  description?: string | null
  source: TerminalSuggestionSource
  confidence: number
  replacementRange: TerminalSuggestionReplacementRange
}

export interface TerminalSuggestionSnapshot {
  buffer: string
  cursor: number
  suppressed: boolean
  suppressionReason: TerminalInputSuppressionReason | null
}

export type TerminalInputSuppressionReason =
  | "alternate-screen"
  | "password-prompt"
  | "paste"
  | "control-sequence"

export type TerminalInputApplyResult =
  | { kind: "edit"; snapshot: TerminalSuggestionSnapshot }
  | { kind: "submit"; command: string; snapshot: TerminalSuggestionSnapshot }
  | { kind: "reset"; snapshot: TerminalSuggestionSnapshot }

const BRACKETED_PASTE_START = "\x1b[200~"
const BRACKETED_PASTE_END = "\x1b[201~"
const ALT_SCREEN_ENTER = /\x1b\[\?(?:47|1047|1049)h/
const ALT_SCREEN_EXIT = /\x1b\[\?(?:47|1047|1049)l/
const PASSWORD_PROMPT = /(?:^|[\r\n])[^\r\n]{0,120}(?:password|passphrase)(?:\s+for\s+[^:\r\n]+)?\s*[:：]\s*$/i
const PRINTABLE_CONTROL = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/

export function isProbablySecretCommand(command: string): boolean {
  const lower = command.toLowerCase()
  if (/\b(?:password|passwd|passphrase|secret|token|apikey|api_key|access_key|private_key)\b/.test(lower)) {
    return true
  }
  if (/\b(?:authorization|bearer)\b/.test(lower)) return true
  if (/\b(?:export|set)\s+[A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD)[A-Z0-9_]*=/.test(command)) {
    return true
  }
  if (/(?:^|\s)(?:--password|--token|--secret|--api-key|--apikey)(?:=|\s+\S)/i.test(command)) {
    return true
  }
  return false
}

export function acceptedSuggestionWrite(
  candidate: TerminalSuggestionCandidate,
  mode: "full" | "word" = "full",
): string {
  if (mode === "full") return candidate.replacement
  const match = candidate.replacement.match(/^\s*\S+\s*/)
  return match?.[0] ?? candidate.replacement
}

export function shouldShowCandidate(
  snapshot: TerminalSuggestionSnapshot,
  candidate: TerminalSuggestionCandidate | null | undefined,
): candidate is TerminalSuggestionCandidate {
  if (!candidate) return false
  if (snapshot.suppressed) return false
  if (candidate.replacement.length === 0) return false
  if (candidate.replacement.includes("\r") || candidate.replacement.includes("\n")) return false
  if (candidate.replacementRange.start !== snapshot.cursor) return false
  if (candidate.replacementRange.end !== snapshot.cursor) return false
  return true
}

export class StaleTerminalSuggestionGate {
  private requestId = 0

  next(): number {
    this.requestId += 1
    return this.requestId
  }

  isCurrent(requestId: number): boolean {
    return requestId === this.requestId
  }

  invalidate(): number {
    return this.next()
  }
}

export class TerminalInputTracker {
  private buffer = ""
  private cursor = 0
  private alternateScreen = false
  private suppressedUntilSubmit: TerminalInputSuppressionReason | null = null

  snapshot(): TerminalSuggestionSnapshot {
    const suppressionReason = this.alternateScreen
      ? "alternate-screen"
      : this.suppressedUntilSubmit
    return {
      buffer: this.buffer,
      cursor: this.cursor,
      suppressed: suppressionReason !== null,
      suppressionReason,
    }
  }

  observeOutput(data: string): TerminalSuggestionSnapshot {
    if (ALT_SCREEN_ENTER.test(data)) {
      this.alternateScreen = true
      this.resetLine()
    }
    if (ALT_SCREEN_EXIT.test(data)) {
      this.alternateScreen = false
      this.resetLine()
    }
    if (PASSWORD_PROMPT.test(data)) {
      this.suppressedUntilSubmit = "password-prompt"
      this.resetLine()
    }
    return this.snapshot()
  }

  applyInput(data: string): TerminalInputApplyResult {
    if (data.includes(BRACKETED_PASTE_START) || data.includes(BRACKETED_PASTE_END)) {
      this.suppressedUntilSubmit = "paste"
      return { kind: "reset", snapshot: this.snapshot() }
    }

    if (data === "\r" || data === "\n") {
      const command = this.buffer.trim()
      this.resetLine()
      this.suppressedUntilSubmit = null
      return { kind: "submit", command, snapshot: this.snapshot() }
    }

    if (data === "\x03" || data === "\x1b") {
      this.resetLine()
      this.suppressedUntilSubmit = null
      return { kind: "reset", snapshot: this.snapshot() }
    }

    if (data === "\x15") {
      this.deleteBeforeCursor()
      this.suppressedUntilSubmit = null
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x02") {
      this.cursor = Math.max(0, this.cursor - 1)
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x06") {
      this.cursor = Math.min([...this.buffer].length, this.cursor + 1)
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x04" || data === "\x1b[3~") {
      this.deleteAtCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x0b") {
      this.deleteAfterCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x17") {
      this.deleteWordBeforeCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x7f" || data === "\b") {
      if (this.cursor > 0) {
        const chars = [...this.buffer]
        chars.splice(this.cursor - 1, 1)
        this.buffer = chars.join("")
        this.cursor -= 1
      }
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x01") {
      this.cursor = 0
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x05") {
      this.cursor = [...this.buffer].length
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x1b[D") {
      this.cursor = Math.max(0, this.cursor - 1)
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x1b[C") {
      this.cursor = Math.min([...this.buffer].length, this.cursor + 1)
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x1bb") {
      this.cursor = this.previousWordCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x1bf") {
      this.cursor = this.nextWordCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data === "\x1bd") {
      this.deleteWordAfterCursor()
      return { kind: "edit", snapshot: this.snapshot() }
    }

    if (data.startsWith("\x1b") || PRINTABLE_CONTROL.test(data)) {
      this.suppressedUntilSubmit = "control-sequence"
      return { kind: "reset", snapshot: this.snapshot() }
    }

    if (data.length > 12 || data.includes("\n") || data.includes("\r")) {
      this.suppressedUntilSubmit = "paste"
      return { kind: "edit", snapshot: this.insertPrintable(data) }
    }

    return { kind: "edit", snapshot: this.insertPrintable(data) }
  }

  private insertPrintable(data: string): TerminalSuggestionSnapshot {
    const chars = [...this.buffer]
    const inserted = [...data]
    chars.splice(this.cursor, 0, ...inserted)
    this.buffer = chars.join("")
    this.cursor += inserted.length
    return this.snapshot()
  }

  private resetLine(): void {
    this.buffer = ""
    this.cursor = 0
  }

  private deleteBeforeCursor(): void {
    const chars = [...this.buffer]
    chars.splice(0, this.cursor)
    this.buffer = chars.join("")
    this.cursor = 0
  }

  private deleteAfterCursor(): void {
    const chars = [...this.buffer]
    chars.splice(this.cursor)
    this.buffer = chars.join("")
  }

  private deleteAtCursor(): void {
    const chars = [...this.buffer]
    if (this.cursor < chars.length) {
      chars.splice(this.cursor, 1)
      this.buffer = chars.join("")
    }
  }

  private deleteWordBeforeCursor(): void {
    const chars = [...this.buffer]
    const previousCursor = this.previousWordCursor()
    chars.splice(previousCursor, this.cursor - previousCursor)
    this.buffer = chars.join("")
    this.cursor = previousCursor
  }

  private deleteWordAfterCursor(): void {
    const chars = [...this.buffer]
    const nextCursor = this.nextWordCursor()
    chars.splice(this.cursor, nextCursor - this.cursor)
    this.buffer = chars.join("")
  }

  private previousWordCursor(): number {
    const chars = [...this.buffer]
    let index = Math.min(this.cursor, chars.length)
    while (index > 0 && /\s/.test(chars[index - 1] ?? "")) index -= 1
    while (index > 0 && !/\s/.test(chars[index - 1] ?? "")) index -= 1
    return index
  }

  private nextWordCursor(): number {
    const chars = [...this.buffer]
    let index = Math.min(this.cursor, chars.length)
    while (index < chars.length && /\s/.test(chars[index] ?? "")) index += 1
    while (index < chars.length && !/\s/.test(chars[index] ?? "")) index += 1
    return index
  }
}
