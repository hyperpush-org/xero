export type ToolCallGroupingPreference = 'grouped' | 'separate'

export const DEFAULT_TOOL_CALL_GROUPING_PREFERENCE: ToolCallGroupingPreference = 'grouped'

const TOOL_CALL_GROUPING_APP_STATE_KEY = 'agent.toolCallGrouping.v1'
const TOOL_CALL_GROUPING_STORAGE_KEY = 'xero.agent.toolCallGrouping.v1'

interface AppUiStateAdapter {
  readAppUiState?: (request: { key: string }) => Promise<{ value?: unknown | null }>
  writeAppUiState?: (request: { key: string; value?: unknown | null }) => Promise<unknown>
}

export function normalizeToolCallGroupingPreference(
  value: unknown,
): ToolCallGroupingPreference | null {
  return value === 'grouped' || value === 'separate' ? value : null
}

export function readStoredToolCallGroupingPreference(): ToolCallGroupingPreference {
  if (typeof window === 'undefined') return DEFAULT_TOOL_CALL_GROUPING_PREFERENCE
  try {
    const stored = window.localStorage.getItem(TOOL_CALL_GROUPING_STORAGE_KEY)
    return normalizeToolCallGroupingPreference(stored) ?? DEFAULT_TOOL_CALL_GROUPING_PREFERENCE
  } catch {
    return DEFAULT_TOOL_CALL_GROUPING_PREFERENCE
  }
}

export function writeStoredToolCallGroupingPreference(
  preference: ToolCallGroupingPreference,
): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(TOOL_CALL_GROUPING_STORAGE_KEY, preference)
  } catch {
    // Storage may be unavailable; app-data persistence still carries the setting.
  }
}

export async function readToolCallGroupingPreference(
  adapter: AppUiStateAdapter | null | undefined,
): Promise<ToolCallGroupingPreference> {
  const fallback = readStoredToolCallGroupingPreference()
  if (!adapter?.readAppUiState) {
    return fallback
  }

  const response = await adapter.readAppUiState({ key: TOOL_CALL_GROUPING_APP_STATE_KEY })
  const appStatePreference = normalizeToolCallGroupingPreference(response.value)
  if (!appStatePreference) {
    return fallback
  }

  writeStoredToolCallGroupingPreference(appStatePreference)
  return appStatePreference
}

export async function persistToolCallGroupingPreference(
  adapter: AppUiStateAdapter | null | undefined,
  preference: ToolCallGroupingPreference,
): Promise<void> {
  writeStoredToolCallGroupingPreference(preference)
  if (!adapter?.writeAppUiState) {
    return
  }

  await adapter.writeAppUiState({
    key: TOOL_CALL_GROUPING_APP_STATE_KEY,
    value: preference,
  })
}
