"use client"

export const SOURCE_CONTROL_SETTINGS_KEY = "xero.sourceControl.settings.v1"
export const SOURCE_CONTROL_SETTINGS_UPDATED_EVENT =
  "xero:source-control-settings-updated"

export type SourceControlThinkingEffort =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "x_high"

export interface SourceControlModelSelection {
  providerId?: string | null
  providerProfileId?: string | null
  modelId?: string | null
  thinkingEffort?: SourceControlThinkingEffort | null
}

export interface SourceControlSettings {
  commitMessageModelSelection: SourceControlModelSelection | null
}

export const DEFAULT_SOURCE_CONTROL_SETTINGS: SourceControlSettings = {
  commitMessageModelSelection: null,
}

const THINKING_EFFORT_VALUES = new Set<SourceControlThinkingEffort>([
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

function normalizeThinkingEffort(value: unknown): SourceControlThinkingEffort | null {
  return typeof value === "string" &&
    THINKING_EFFORT_VALUES.has(value as SourceControlThinkingEffort)
    ? (value as SourceControlThinkingEffort)
    : null
}

export function normalizeSourceControlModelSelection(
  value: unknown,
): SourceControlModelSelection | null {
  if (!value || typeof value !== "object") return null
  const candidate = value as Record<string, unknown>
  const modelId = compactString(candidate.modelId)
  if (!modelId) return null
  return {
    providerId: compactString(candidate.providerId),
    providerProfileId: compactString(candidate.providerProfileId),
    modelId,
    thinkingEffort: normalizeThinkingEffort(candidate.thinkingEffort),
  }
}

export function normalizeSourceControlSettings(value: unknown): SourceControlSettings {
  if (!value || typeof value !== "object") {
    return { ...DEFAULT_SOURCE_CONTROL_SETTINGS }
  }
  const candidate = value as Record<string, unknown>
  return {
    commitMessageModelSelection: normalizeSourceControlModelSelection(
      candidate.commitMessageModelSelection,
    ),
  }
}

export function loadSourceControlSettings(): SourceControlSettings {
  if (typeof window === "undefined") {
    return { ...DEFAULT_SOURCE_CONTROL_SETTINGS }
  }
  try {
    const stored = window.localStorage.getItem(SOURCE_CONTROL_SETTINGS_KEY)
    return normalizeSourceControlSettings(JSON.parse(stored ?? "null"))
  } catch {
    return { ...DEFAULT_SOURCE_CONTROL_SETTINGS }
  }
}

export function persistSourceControlSettings(settings: SourceControlSettings): void {
  if (typeof window === "undefined") return
  const normalized = normalizeSourceControlSettings(settings)
  try {
    window.localStorage.setItem(SOURCE_CONTROL_SETTINGS_KEY, JSON.stringify(normalized))
  } catch {
    // Best effort; the current view can still use the in-memory settings.
  }
  window.dispatchEvent(
    new CustomEvent<SourceControlSettings>(SOURCE_CONTROL_SETTINGS_UPDATED_EVENT, {
      detail: normalized,
    }),
  )
}

export function subscribeSourceControlSettings(
  listener: (settings: SourceControlSettings) => void,
): () => void {
  if (typeof window === "undefined") return () => undefined

  const handleCustomEvent = (event: Event) => {
    listener(
      normalizeSourceControlSettings(
        (event as CustomEvent<SourceControlSettings>).detail,
      ),
    )
  }
  const handleStorageEvent = (event: StorageEvent) => {
    if (event.key !== SOURCE_CONTROL_SETTINGS_KEY) return
    listener(loadSourceControlSettings())
  }

  window.addEventListener(SOURCE_CONTROL_SETTINGS_UPDATED_EVENT, handleCustomEvent)
  window.addEventListener("storage", handleStorageEvent)
  return () => {
    window.removeEventListener(SOURCE_CONTROL_SETTINGS_UPDATED_EVENT, handleCustomEvent)
    window.removeEventListener("storage", handleStorageEvent)
  }
}
