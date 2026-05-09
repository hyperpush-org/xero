export type IpcPayloadBudgetKey =
  | 'browserConsoleEvent'
  | 'browserTabEvent'
  | 'emulatorFrameEvent'
  | 'notificationDiagnosticsPage'
  | 'projectSearchResults'
  | 'projectTree'
  | 'providerRegistry'
  | 'repositoryDiff'
  | 'repositoryStatus'
  | 'runtimeStreamItem'
  | 'settingsRegistry'

export interface IpcPayloadBudget {
  key: IpcPayloadBudgetKey
  label: string
  warnBytes: number
  maxBytes: number
}

export interface IpcPayloadSample {
  boundary: 'channel' | 'command' | 'event'
  name: string
  payload: unknown
  budgetKey?: IpcPayloadBudgetKey | null
}

export interface RecordedIpcPayloadSample {
  boundary: IpcPayloadSample['boundary']
  budget: IpcPayloadBudget
  name: string
  observedBytes: number
  overMaxBudget: boolean
  overWarnBudget: boolean
}

export interface IpcPayloadBudgetMetric {
  budgetBytes: number
  budgetKey: IpcPayloadBudgetKey
  droppedCount: number
  label: string
  largestBoundary: IpcPayloadSample['boundary']
  largestBytes: number
  largestName: string
  overBudgetCount: number
  sampleCount: number
}

export const IPC_PAYLOAD_BUDGETS = {
  runtimeStreamItem: {
    key: 'runtimeStreamItem',
    label: 'runtime stream item',
    warnBytes: 32 * 1024,
    maxBytes: 96 * 1024,
  },
  repositoryStatus: {
    key: 'repositoryStatus',
    label: 'repository status',
    warnBytes: 384 * 1024,
    maxBytes: 768 * 1024,
  },
  repositoryDiff: {
    key: 'repositoryDiff',
    label: 'repository diff',
    warnBytes: 96 * 1024,
    maxBytes: 128 * 1024,
  },
  projectTree: {
    key: 'projectTree',
    label: 'project tree',
    warnBytes: 512 * 1024,
    maxBytes: 1024 * 1024,
  },
  projectSearchResults: {
    key: 'projectSearchResults',
    label: 'project search results',
    warnBytes: 1024 * 1024,
    maxBytes: 2 * 1024 * 1024,
  },
  browserTabEvent: {
    key: 'browserTabEvent',
    label: 'browser tab event',
    warnBytes: 8 * 1024,
    maxBytes: 32 * 1024,
  },
  browserConsoleEvent: {
    key: 'browserConsoleEvent',
    label: 'browser console event',
    warnBytes: 16 * 1024,
    maxBytes: 64 * 1024,
  },
  emulatorFrameEvent: {
    key: 'emulatorFrameEvent',
    label: 'emulator frame event',
    warnBytes: 1024,
    maxBytes: 4 * 1024,
  },
  providerRegistry: {
    key: 'providerRegistry',
    label: 'provider/model registry',
    warnBytes: 512 * 1024,
    maxBytes: 1024 * 1024,
  },
  settingsRegistry: {
    key: 'settingsRegistry',
    label: 'settings registry',
    warnBytes: 256 * 1024,
    maxBytes: 512 * 1024,
  },
  notificationDiagnosticsPage: {
    key: 'notificationDiagnosticsPage',
    label: 'notification/diagnostics page',
    warnBytes: 384 * 1024,
    maxBytes: 768 * 1024,
  },
} satisfies Record<IpcPayloadBudgetKey, IpcPayloadBudget>

const COMMAND_BUDGET_KEYS: Record<string, IpcPayloadBudgetKey | undefined> = {
  browser_control_settings: 'settingsRegistry',
  browser_tab_list: 'browserTabEvent',
  check_provider_profile: 'notificationDiagnosticsPage',
  get_environment_discovery_status: 'notificationDiagnosticsPage',
  get_environment_profile_summary: 'notificationDiagnosticsPage',
  get_provider_model_catalog: 'providerRegistry',
  preflight_provider_profile: 'providerRegistry',
  get_repository_diff: 'repositoryDiff',
  get_repository_status: 'repositoryStatus',
  list_mcp_servers: 'settingsRegistry',
  list_notification_dispatches: 'notificationDiagnosticsPage',
  list_notification_routes: 'settingsRegistry',
  list_project_files: 'projectTree',
  list_skill_registry: 'settingsRegistry',
  reload_skill_registry: 'settingsRegistry',
  run_doctor_report: 'notificationDiagnosticsPage',
  search_project: 'projectSearchResults',
  soul_settings: 'settingsRegistry',
  speech_dictation_settings: 'settingsRegistry',
}

const EVENT_BUDGET_KEYS: Record<string, IpcPayloadBudgetKey | undefined> = {
  'browser:console': 'browserConsoleEvent',
  'browser:load_state': 'browserTabEvent',
  'browser:tab_updated': 'browserTabEvent',
  'browser:url_changed': 'browserTabEvent',
  'emulator:frame': 'emulatorFrameEvent',
  'emulator:status': 'emulatorFrameEvent',
  'repository:status_changed': 'repositoryStatus',
  'runtime:updated': 'settingsRegistry',
}

const CHANNEL_BUDGET_KEYS: Record<string, IpcPayloadBudgetKey | undefined> = {
  'subscribe_runtime_stream:item': 'runtimeStreamItem',
}

const metrics = new Map<IpcPayloadBudgetKey, IpcPayloadBudgetMetric>()

function shouldCollectIpcPayloadMetrics(): boolean {
  if (import.meta.env.DEV || import.meta.env.MODE === 'test') {
    return true
  }

  return Boolean((globalThis as { __XERO_IPC_PAYLOAD_METRICS__?: boolean }).__XERO_IPC_PAYLOAD_METRICS__)
}

function resolveBudgetKey(sample: Pick<IpcPayloadSample, 'boundary' | 'budgetKey' | 'name'>): IpcPayloadBudgetKey | null {
  if (sample.budgetKey) {
    return sample.budgetKey
  }

  switch (sample.boundary) {
    case 'channel':
      return CHANNEL_BUDGET_KEYS[sample.name] ?? null
    case 'command':
      return COMMAND_BUDGET_KEYS[sample.name] ?? null
    case 'event':
      return EVENT_BUDGET_KEYS[sample.name] ?? null
  }
}

export function estimateIpcPayloadBytes(payload: unknown): number {
  if (payload === null || payload === undefined) {
    return 0
  }

  if (typeof payload === 'string') {
    return payload.length * 2
  }

  if (payload instanceof ArrayBuffer) {
    return payload.byteLength
  }

  try {
    return JSON.stringify(payload).length * 2
  } catch {
    return Number.POSITIVE_INFINITY
  }
}

function estimateRuntimeStreamItemPayloadBytes(payload: unknown): number {
  if (payload === null || payload === undefined) {
    return 0
  }

  if (typeof payload === 'string') {
    return payload.length * 2
  }

  if (typeof payload !== 'object') {
    return 16
  }

  const seen = new Set<object>()
  const stack: unknown[] = [payload]
  let estimatedBytes = 64
  let visitedNodes = 0

  while (stack.length > 0 && visitedNodes < 96) {
    const value = stack.pop()
    if (!value || typeof value !== 'object') {
      continue
    }

    if (seen.has(value)) {
      continue
    }
    seen.add(value)
    visitedNodes += 1

    if (Array.isArray(value)) {
      estimatedBytes += 16
      const inspectedLength = Math.min(value.length, 64)
      for (let index = 0; index < inspectedLength; index += 1) {
        stack.push(value[index])
      }
      if (value.length > inspectedLength) {
        estimatedBytes += (value.length - inspectedLength) * 16
      }
      continue
    }

    for (const [key, nestedValue] of Object.entries(value as Record<string, unknown>)) {
      estimatedBytes += key.length * 2 + 8
      if (typeof nestedValue === 'string') {
        estimatedBytes += nestedValue.length * 2
      } else if (typeof nestedValue === 'number' || typeof nestedValue === 'boolean') {
        estimatedBytes += 16
      } else if (nestedValue === null || nestedValue === undefined) {
        estimatedBytes += 4
      } else {
        stack.push(nestedValue)
      }
    }
  }

  if (stack.length > 0) {
    estimatedBytes += stack.length * 16
  }

  return estimatedBytes
}

function estimatePayloadBytesForBudget(payload: unknown, budgetKey: IpcPayloadBudgetKey): number {
  if (budgetKey === 'runtimeStreamItem') {
    return estimateRuntimeStreamItemPayloadBytes(payload)
  }

  return estimateIpcPayloadBytes(payload)
}

export function recordIpcPayloadSample(sample: IpcPayloadSample): RecordedIpcPayloadSample | null {
  const budgetKey = resolveBudgetKey(sample)
  if (!budgetKey) {
    return null
  }

  const budget = IPC_PAYLOAD_BUDGETS[budgetKey]
  if (!shouldCollectIpcPayloadMetrics()) {
    return null
  }

  const observedBytes = estimatePayloadBytesForBudget(sample.payload, budgetKey)
  const overWarnBudget = observedBytes > budget.warnBytes
  const overMaxBudget = observedBytes > budget.maxBytes
  const current = metrics.get(budgetKey)
  const next: IpcPayloadBudgetMetric = current
    ? { ...current }
    : {
        budgetBytes: budget.warnBytes,
        budgetKey,
        droppedCount: 0,
        label: budget.label,
        largestBoundary: sample.boundary,
        largestBytes: 0,
        largestName: sample.name,
        overBudgetCount: 0,
        sampleCount: 0,
      }

  next.sampleCount += 1
  if (overWarnBudget) {
    next.overBudgetCount += 1
  }
  if (overMaxBudget) {
    next.droppedCount += 1
  }
  if (observedBytes >= next.largestBytes) {
    next.largestBoundary = sample.boundary
    next.largestBytes = observedBytes
    next.largestName = sample.name
  }

  metrics.set(budgetKey, next)

  return {
    boundary: sample.boundary,
    budget,
    name: sample.name,
    observedBytes,
    overMaxBudget,
    overWarnBudget,
  }
}

export function getIpcPayloadBudgetMetrics(): IpcPayloadBudgetMetric[] {
  return Array.from(metrics.values()).sort((left, right) => right.largestBytes - left.largestBytes)
}

export function resetIpcPayloadBudgetMetricsForTests(): void {
  metrics.clear()
}
