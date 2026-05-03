import { useCallback, useSyncExternalStore, type SetStateAction } from 'react'
import type {
  RepositoryStatusView,
  RuntimeStreamView,
} from '@/src/lib/xero-model'

export type EqualityFn<T> = (left: T, right: T) => boolean

type StoreListener = () => void

interface SelectorListener<TState, TSelected> {
  selector: (state: TState) => TSelected
  isEqual: EqualityFn<TSelected>
  listener: StoreListener
  selected: TSelected
}

export interface SelectorStore<TState> {
  getSnapshot: () => TState
  setSnapshot: (action: SetStateAction<TState>) => TState
  subscribe: (listener: StoreListener) => () => void
  subscribeSelector: <TSelected>(
    selector: (state: TState) => TSelected,
    isEqual: EqualityFn<TSelected>,
    listener: StoreListener,
  ) => () => void
}

export interface RepositoryShellStatus {
  branchLabel: string | null
  upstream: RepositoryStatusView['upstream'] | null
  hasChanges: boolean
  statusCount: number
  additions: number
  deletions: number
  lastCommit: RepositoryStatusView['lastCommit'] | null
}

export interface XeroHighChurnState {
  repositoryStatus: RepositoryStatusView | null
  runtimeStreams: Record<string, RuntimeStreamView>
}

export type XeroHighChurnStore = SelectorStore<XeroHighChurnState> & {
  setRepositoryStatus: (action: SetStateAction<RepositoryStatusView | null>) => RepositoryStatusView | null
  setRuntimeStreams: (
    action: SetStateAction<Record<string, RuntimeStreamView>>,
  ) => Record<string, RuntimeStreamView>
}

export function resolveSetStateAction<T>(current: T, action: SetStateAction<T>): T {
  return typeof action === 'function' ? (action as (current: T) => T)(current) : action
}

export function shallowEqualObject<T extends object>(left: T, right: T): boolean {
  if (Object.is(left, right)) {
    return true
  }

  const leftKeys = Object.keys(left)
  const rightKeys = Object.keys(right)
  if (leftKeys.length !== rightKeys.length) {
    return false
  }

  for (const key of leftKeys) {
    if (
      !Object.prototype.hasOwnProperty.call(right, key) ||
      !Object.is(
        left[key as keyof T],
        right[key as keyof T],
      )
    ) {
      return false
    }
  }

  return true
}

export function createSelectorStore<TState>(initialState: TState): SelectorStore<TState> {
  let snapshot = initialState
  const listeners = new Set<StoreListener>()
  const selectorListeners = new Set<SelectorListener<TState, unknown>>()

  const notify = () => {
    listeners.forEach((listener) => listener())

    selectorListeners.forEach((entry) => {
      const nextSelected = entry.selector(snapshot)
      if (entry.isEqual(entry.selected, nextSelected)) {
        return
      }

      entry.selected = nextSelected
      entry.listener()
    })
  }

  return {
    getSnapshot: () => snapshot,
    setSnapshot: (action) => {
      const nextSnapshot = resolveSetStateAction(snapshot, action)
      if (Object.is(snapshot, nextSnapshot)) {
        return snapshot
      }

      snapshot = nextSnapshot
      notify()
      return snapshot
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    subscribeSelector: (selector, isEqual, listener) => {
      const entry: SelectorListener<TState, unknown> = {
        selector,
        isEqual: isEqual as EqualityFn<unknown>,
        listener,
        selected: selector(snapshot),
      }
      selectorListeners.add(entry)
      return () => {
        selectorListeners.delete(entry)
      }
    },
  }
}

export function createXeroHighChurnStore(): XeroHighChurnStore {
  const store = createSelectorStore<XeroHighChurnState>({
    repositoryStatus: null,
    runtimeStreams: {},
  })

  return {
    ...store,
    setRepositoryStatus: (action) => {
      let nextStatus: RepositoryStatusView | null = null
      store.setSnapshot((current) => {
        nextStatus = resolveSetStateAction(current.repositoryStatus, action)
        return current.repositoryStatus === nextStatus
          ? current
          : {
              ...current,
              repositoryStatus: nextStatus,
            }
      })
      return nextStatus
    },
    setRuntimeStreams: (action) => {
      let nextStreams: Record<string, RuntimeStreamView> = {}
      store.setSnapshot((current) => {
        nextStreams = resolveSetStateAction(current.runtimeStreams, action)
        return current.runtimeStreams === nextStreams
          ? current
          : {
              ...current,
              runtimeStreams: nextStreams,
            }
      })
      return nextStreams
    },
  }
}

export const selectRepositoryStatus = (state: XeroHighChurnState) => state.repositoryStatus

const RUNTIME_STREAM_STORE_KEY_SEPARATOR = '\u0000'

export function createRuntimeStreamStoreKey(projectId: string, agentSessionId: string): string {
  return `${projectId}${RUNTIME_STREAM_STORE_KEY_SEPARATOR}${agentSessionId}`
}

export function removeRuntimeStreamsForProject(
  streams: Record<string, RuntimeStreamView>,
  projectId: string,
): Record<string, RuntimeStreamView> {
  let changed = false
  const nextStreams: Record<string, RuntimeStreamView> = {}

  for (const [key, stream] of Object.entries(streams)) {
    if (key === projectId || stream.projectId === projectId) {
      changed = true
      continue
    }

    nextStreams[key] = stream
  }

  return changed ? nextStreams : streams
}

export function removeRuntimeStreamForSession(
  streams: Record<string, RuntimeStreamView>,
  projectId: string,
  agentSessionId: string,
): Record<string, RuntimeStreamView> {
  const sessionKey = createRuntimeStreamStoreKey(projectId, agentSessionId)
  let changed = false
  const nextStreams: Record<string, RuntimeStreamView> = {}

  for (const [key, stream] of Object.entries(streams)) {
    if (
      key === sessionKey ||
      (key === projectId && stream.agentSessionId === agentSessionId) ||
      (stream.projectId === projectId && stream.agentSessionId === agentSessionId)
    ) {
      changed = true
      continue
    }

    nextStreams[key] = stream
  }

  return changed ? nextStreams : streams
}

export const selectRepositoryShellStatus = (state: XeroHighChurnState): RepositoryShellStatus => ({
  branchLabel: state.repositoryStatus?.branchLabel ?? null,
  upstream: state.repositoryStatus?.upstream ?? null,
  hasChanges: state.repositoryStatus?.hasChanges ?? false,
  statusCount: state.repositoryStatus?.statusCount ?? 0,
  additions: state.repositoryStatus?.additions ?? 0,
  deletions: state.repositoryStatus?.deletions ?? 0,
  lastCommit: state.repositoryStatus?.lastCommit ?? null,
})

export const selectRuntimeStreams = (state: XeroHighChurnState) => state.runtimeStreams

export function selectRuntimeStreamForProject(projectId: string | null, agentSessionId?: string | null) {
  return (state: XeroHighChurnState): RuntimeStreamView | null => {
    if (!projectId) {
      return null
    }

    const stream = agentSessionId
      ? state.runtimeStreams[createRuntimeStreamStoreKey(projectId, agentSessionId)] ?? state.runtimeStreams[projectId] ?? null
      : state.runtimeStreams[projectId] ?? null
    if (!stream || (agentSessionId && stream.agentSessionId !== agentSessionId)) {
      return null
    }

    return stream
  }
}

export function useSelectorStoreValue<TState, TSelected>(
  store: SelectorStore<TState>,
  selector: (state: TState) => TSelected,
  isEqual: EqualityFn<TSelected> = Object.is,
  options: { disabled?: boolean } = {},
): TSelected {
  const getSnapshot = useCallback(() => selector(store.getSnapshot()), [selector, store])
  const subscribe = useCallback(
    (listener: StoreListener) =>
      options.disabled
        ? () => undefined
        : store.subscribeSelector(selector, isEqual, listener),
    [isEqual, options.disabled, selector, store],
  )

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}
