export const DEFAULT_AGENT_ROUTING_AUTO_SWITCH_ENABLED = false

const AGENT_ROUTING_AUTO_SWITCH_APP_STATE_KEY = 'agent.routingAutoSwitch.v1'
const AGENT_ROUTING_AUTO_SWITCH_STORAGE_KEY = 'xero.agent.routingAutoSwitch.v1'

interface AppUiStateAdapter {
  readAppUiState?: (request: { key: string }) => Promise<{ value?: unknown | null }>
  writeAppUiState?: (request: { key: string; value?: unknown | null }) => Promise<unknown>
}

export function normalizeAgentRoutingAutoSwitchPreference(value: unknown): boolean | null {
  if (value === true || value === 'true') return true
  if (value === false || value === 'false') return false
  return null
}

export function readStoredAgentRoutingAutoSwitchPreference(): boolean {
  if (typeof window === 'undefined') return DEFAULT_AGENT_ROUTING_AUTO_SWITCH_ENABLED
  try {
    const stored = window.localStorage.getItem(AGENT_ROUTING_AUTO_SWITCH_STORAGE_KEY)
    return (
      normalizeAgentRoutingAutoSwitchPreference(stored) ??
      DEFAULT_AGENT_ROUTING_AUTO_SWITCH_ENABLED
    )
  } catch {
    return DEFAULT_AGENT_ROUTING_AUTO_SWITCH_ENABLED
  }
}

export function writeStoredAgentRoutingAutoSwitchPreference(enabled: boolean): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(AGENT_ROUTING_AUTO_SWITCH_STORAGE_KEY, String(enabled))
  } catch {
    // Storage may be unavailable; app-data persistence still carries the setting.
  }
}

export async function readAgentRoutingAutoSwitchPreference(
  adapter: AppUiStateAdapter | null | undefined,
): Promise<boolean> {
  const fallback = readStoredAgentRoutingAutoSwitchPreference()
  if (!adapter?.readAppUiState) {
    return fallback
  }

  const response = await adapter.readAppUiState({
    key: AGENT_ROUTING_AUTO_SWITCH_APP_STATE_KEY,
  })
  const appStatePreference = normalizeAgentRoutingAutoSwitchPreference(response.value)
  if (appStatePreference === null) {
    return fallback
  }

  writeStoredAgentRoutingAutoSwitchPreference(appStatePreference)
  return appStatePreference
}

export async function persistAgentRoutingAutoSwitchPreference(
  adapter: AppUiStateAdapter | null | undefined,
  enabled: boolean,
): Promise<void> {
  writeStoredAgentRoutingAutoSwitchPreference(enabled)
  if (!adapter?.writeAppUiState) {
    return
  }

  await adapter.writeAppUiState({
    key: AGENT_ROUTING_AUTO_SWITCH_APP_STATE_KEY,
    value: enabled,
  })
}
