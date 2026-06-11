"use client"

import type { RuntimeAgentIdDto } from "@/src/lib/xero-model/runtime"

export const TERMINAL_SUGGESTION_SETTINGS_KEY =
  "xero.terminal.suggestions.settings.v1"

export const TERMINAL_SUGGESTION_SETTINGS_UPDATED_EVENT =
  "xero:terminal-suggestion-settings-updated"

export type TerminalSuggestionThinkingEffort =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "x_high"

export interface TerminalSuggestionModelSelection {
  providerId?: string | null
  providerProfileId?: string | null
  modelId?: string | null
  runtimeAgentId?: RuntimeAgentIdDto | null
  thinkingEffort?: TerminalSuggestionThinkingEffort | null
}

export interface TerminalSuggestionSettings {
  enabled: boolean
  aiEnabled: boolean
  modelSelection: TerminalSuggestionModelSelection | null
}

export const DEFAULT_TERMINAL_SUGGESTION_SETTINGS: TerminalSuggestionSettings = {
  enabled: true,
  aiEnabled: false,
  modelSelection: null,
}

const THINKING_EFFORT_VALUES = new Set<TerminalSuggestionThinkingEffort>([
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "x_high",
])

function compactString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null
}

function normalizeThinkingEffort(value: unknown): TerminalSuggestionThinkingEffort | null {
  return typeof value === "string" &&
    THINKING_EFFORT_VALUES.has(value as TerminalSuggestionThinkingEffort)
    ? (value as TerminalSuggestionThinkingEffort)
    : null
}

function normalizeModelSelection(value: unknown): TerminalSuggestionModelSelection | null {
  if (!value || typeof value !== "object") return null
  const candidate = value as Record<string, unknown>
  const modelId = compactString(candidate.modelId)
  if (!modelId) return null
  return {
    providerId: compactString(candidate.providerId),
    providerProfileId: compactString(candidate.providerProfileId),
    modelId,
    runtimeAgentId: compactString(candidate.runtimeAgentId) as RuntimeAgentIdDto | null,
    thinkingEffort: normalizeThinkingEffort(candidate.thinkingEffort),
  }
}

export function normalizeTerminalSuggestionSettings(
  value: unknown,
): TerminalSuggestionSettings {
  if (!value || typeof value !== "object") {
    return { ...DEFAULT_TERMINAL_SUGGESTION_SETTINGS }
  }
  const candidate = value as Record<string, unknown>
  return {
    enabled: candidate.enabled !== false,
    aiEnabled: candidate.aiEnabled === true,
    modelSelection: normalizeModelSelection(candidate.modelSelection),
  }
}

export function loadTerminalSuggestionSettings(): TerminalSuggestionSettings {
  if (typeof window === "undefined") {
    return { ...DEFAULT_TERMINAL_SUGGESTION_SETTINGS }
  }
  try {
    const stored = window.localStorage.getItem(TERMINAL_SUGGESTION_SETTINGS_KEY)
    return normalizeTerminalSuggestionSettings(
      JSON.parse(stored ?? "null"),
    )
  } catch {
    return { ...DEFAULT_TERMINAL_SUGGESTION_SETTINGS }
  }
}

export function persistTerminalSuggestionSettings(
  settings: TerminalSuggestionSettings,
): void {
  if (typeof window === "undefined") return
  const normalized = normalizeTerminalSuggestionSettings(settings)
  try {
    window.localStorage.setItem(
      TERMINAL_SUGGESTION_SETTINGS_KEY,
      JSON.stringify(normalized),
    )
  } catch {
    // Best effort; the current view can still use the in-memory settings.
  }
  window.dispatchEvent(
    new CustomEvent<TerminalSuggestionSettings>(
      TERMINAL_SUGGESTION_SETTINGS_UPDATED_EVENT,
      { detail: normalized },
    ),
  )
}

export function subscribeTerminalSuggestionSettings(
  listener: (settings: TerminalSuggestionSettings) => void,
): () => void {
  if (typeof window === "undefined") return () => undefined

  const handleCustomEvent = (event: Event) => {
    listener(
      normalizeTerminalSuggestionSettings(
        (event as CustomEvent<TerminalSuggestionSettings>).detail,
      ),
    )
  }
  const handleStorageEvent = (event: StorageEvent) => {
    if (event.key !== TERMINAL_SUGGESTION_SETTINGS_KEY) return
    listener(loadTerminalSuggestionSettings())
  }

  window.addEventListener(TERMINAL_SUGGESTION_SETTINGS_UPDATED_EVENT, handleCustomEvent)
  window.addEventListener("storage", handleStorageEvent)
  return () => {
    window.removeEventListener(
      TERMINAL_SUGGESTION_SETTINGS_UPDATED_EVENT,
      handleCustomEvent,
    )
    window.removeEventListener("storage", handleStorageEvent)
  }
}
