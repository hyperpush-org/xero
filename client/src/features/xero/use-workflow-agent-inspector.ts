import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import {
  agentRefKey,
  agentRefSchema,
  agentRefsEqual,
  type AgentRefDto,
  type WorkflowAgentDetailDto,
  type WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'
import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'

const SELECTED_REF_STORAGE_KEY = 'xero.workflows.selectedAgent'
const SELECTED_REF_UI_STATE_KEY = 'workflows.selected-agent.v1'

export type AgentListStatus = 'idle' | 'loading' | 'ready' | 'error'

export type AgentDetailStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface UseWorkflowAgentInspectorOptions {
  adapter: Pick<
    XeroDesktopAdapter,
    | 'listWorkflowAgents'
    | 'getWorkflowAgentDetail'
    | 'readProjectUiState'
    | 'writeProjectUiState'
  >
  projectId: string | null
}

export interface UseWorkflowAgentInspectorResult {
  agents: WorkflowAgentSummaryDto[]
  agentsStatus: AgentListStatus
  agentsError: Error | null
  selectedRef: AgentRefDto | null
  selectAgent: (ref: AgentRefDto | null) => void
  detail: WorkflowAgentDetailDto | null
  detailStatus: AgentDetailStatus
  detailError: Error | null
  refreshAgents: () => Promise<void>
  reloadDetail: () => Promise<void>
}

function readPersistedRef(): AgentRefDto | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage.getItem(SELECTED_REF_STORAGE_KEY)
    if (!raw) return null
    const parsed = agentRefSchema.safeParse(JSON.parse(raw))
    return parsed.success ? parsed.data : null
  } catch {
    return null
  }
}

function persistRef(ref: AgentRefDto | null): void {
  if (typeof window === 'undefined') return
  try {
    if (!ref) {
      window.localStorage.removeItem(SELECTED_REF_STORAGE_KEY)
    } else {
      window.localStorage.setItem(SELECTED_REF_STORAGE_KEY, JSON.stringify(ref))
    }
  } catch {
    // best-effort: storage may be disabled
  }
}

function parsePersistedAgentRef(value: unknown): AgentRefDto | null {
  const parsed = agentRefSchema.safeParse(value)
  return parsed.success ? parsed.data : null
}

export function useWorkflowAgentInspector(
  options: UseWorkflowAgentInspectorOptions,
): UseWorkflowAgentInspectorResult {
  const { adapter, projectId } = options
  const hasProjectUiStateStorage = Boolean(
    projectId && adapter.readProjectUiState && adapter.writeProjectUiState,
  )
  const [agents, setAgents] = useState<WorkflowAgentSummaryDto[]>([])
  const [agentsStatus, setAgentsStatus] = useState<AgentListStatus>('idle')
  const [agentsError, setAgentsError] = useState<Error | null>(null)

  const [selectedRef, setSelectedRefState] = useState<AgentRefDto | null>(() =>
    hasProjectUiStateStorage ? null : readPersistedRef(),
  )
  const [detail, setDetail] = useState<WorkflowAgentDetailDto | null>(null)
  const [detailStatus, setDetailStatus] = useState<AgentDetailStatus>('idle')
  const [detailError, setDetailError] = useState<Error | null>(null)

  const detailRequestId = useRef(0)
  const listRequestId = useRef(0)
  const persistedSelectionRequestId = useRef(0)
  const currentProjectIdRef = useRef(projectId)
  const [stateProjectId, setStateProjectId] = useState(projectId)
  currentProjectIdRef.current = projectId
  const isStateForCurrentProject = stateProjectId === projectId

  const loadAgents = useCallback(async () => {
    if (!projectId) {
      setAgents([])
      setAgentsStatus('idle')
      setAgentsError(null)
      return
    }
    const requestProjectId = projectId
    const requestId = ++listRequestId.current
    setAgentsStatus('loading')
    setAgentsError(null)
    try {
      const response = await adapter.listWorkflowAgents({ projectId: requestProjectId, includeArchived: false })
      if (
        requestId !== listRequestId.current ||
        currentProjectIdRef.current !== requestProjectId
      ) {
        return
      }
      setAgents(response.agents)
      setAgentsStatus('ready')
    } catch (error) {
      if (
        requestId !== listRequestId.current ||
        currentProjectIdRef.current !== requestProjectId
      ) {
        return
      }
      setAgentsError(error instanceof Error ? error : new Error(String(error)))
      setAgentsStatus('error')
    }
  }, [adapter, projectId])

  const loadDetail = useCallback(
    async (ref: AgentRefDto) => {
      if (!projectId) {
        setDetail(null)
        setDetailStatus('idle')
        setDetailError(null)
        return
      }
      const requestProjectId = projectId
      const requestId = ++detailRequestId.current
      setDetailStatus('loading')
      setDetailError(null)
      try {
        const response = await adapter.getWorkflowAgentDetail({ projectId: requestProjectId, ref })
        if (
          requestId !== detailRequestId.current ||
          currentProjectIdRef.current !== requestProjectId
        ) {
          return
        }
        setDetail(response)
        setDetailStatus('ready')
      } catch (error) {
        if (
          requestId !== detailRequestId.current ||
          currentProjectIdRef.current !== requestProjectId
        ) {
          return
        }
        setDetail(null)
        setDetailError(error instanceof Error ? error : new Error(String(error)))
        setDetailStatus('error')
      }
    },
    [adapter, projectId],
  )

  useEffect(() => {
    listRequestId.current += 1
    detailRequestId.current += 1
    persistedSelectionRequestId.current += 1
    setStateProjectId(projectId)
    setAgents([])
    setAgentsStatus(projectId ? 'loading' : 'idle')
    setAgentsError(null)
    setSelectedRefState(null)
    setDetail(null)
    setDetailStatus('idle')
    setDetailError(null)
  }, [projectId])

  // Initial load + reload on project change.
  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  useEffect(() => {
    const requestId = ++persistedSelectionRequestId.current
    const requestProjectId = projectId
    if (!projectId) {
      setSelectedRefState(null)
      return
    }

    if (!adapter.readProjectUiState) {
      setSelectedRefState(readPersistedRef())
      return
    }

    adapter
      .readProjectUiState({ projectId, key: SELECTED_REF_UI_STATE_KEY })
      .then((response) => {
        if (
          requestId !== persistedSelectionRequestId.current ||
          currentProjectIdRef.current !== requestProjectId
        ) {
          return
        }
        setSelectedRefState(parsePersistedAgentRef(response.value ?? null))
      })
      .catch(() => {
        if (
          requestId !== persistedSelectionRequestId.current ||
          currentProjectIdRef.current !== requestProjectId
        ) {
          return
        }
        setSelectedRefState(null)
      })
  }, [adapter, projectId])

  // Drive detail fetch from selection.
  useEffect(() => {
    if (!isStateForCurrentProject) {
      return
    }
    if (!selectedRef) {
      setDetail(null)
      setDetailStatus('idle')
      setDetailError(null)
      return
    }
    void loadDetail(selectedRef)
  }, [isStateForCurrentProject, selectedRef, loadDetail])

  const selectAgent = useCallback(
    (ref: AgentRefDto | null) => {
      setSelectedRefState((prev) => {
        if (!ref) {
          if (prev !== null) {
            persistedSelectionRequestId.current += 1
            if (projectId && adapter.writeProjectUiState) {
              void adapter
                .writeProjectUiState({
                  projectId,
                  key: SELECTED_REF_UI_STATE_KEY,
                  value: null,
                })
                .catch(() => {})
            } else {
              persistRef(null)
            }
          }
          return null
        }
        if (prev && agentRefsEqual(prev, ref)) return prev
        persistedSelectionRequestId.current += 1
        if (projectId && adapter.writeProjectUiState) {
          void adapter
            .writeProjectUiState({
              projectId,
              key: SELECTED_REF_UI_STATE_KEY,
              value: ref,
            })
            .catch(() => {})
        } else {
          persistRef(ref)
        }
        return ref
      })
    },
    [adapter, projectId],
  )

  const reloadDetail = useCallback(async () => {
    if (!selectedRef) return
    await loadDetail(selectedRef)
  }, [selectedRef, loadDetail])

  // Drop selection if the persisted ref is no longer in the loaded list (e.g. archived).
  useEffect(() => {
    if (!selectedRef || agentsStatus !== 'ready') return
    const stillExists = agents.some((agent) => agentRefsEqual(agent.ref, selectedRef))
    if (!stillExists) {
      setSelectedRefState(null)
      if (projectId && adapter.writeProjectUiState) {
        void adapter
          .writeProjectUiState({ projectId, key: SELECTED_REF_UI_STATE_KEY, value: null })
          .catch(() => {})
      } else {
        persistRef(null)
      }
    }
  }, [adapter, agents, agentsStatus, projectId, selectedRef])

  const visibleAgents = isStateForCurrentProject ? agents : []
  const visibleAgentsStatus = isStateForCurrentProject
    ? agentsStatus
    : projectId ? 'loading' : 'idle'
  const visibleSelectedRef = isStateForCurrentProject ? selectedRef : null
  const visibleDetail = isStateForCurrentProject ? detail : null
  const visibleDetailStatus = isStateForCurrentProject ? detailStatus : 'idle'
  const visibleDetailError = isStateForCurrentProject ? detailError : null
  const visibleAgentsError = isStateForCurrentProject ? agentsError : null

  return useMemo(
    () => ({
      agents: visibleAgents,
      agentsStatus: visibleAgentsStatus,
      agentsError: visibleAgentsError,
      selectedRef: visibleSelectedRef,
      selectAgent,
      detail: visibleDetail,
      detailStatus: visibleDetailStatus,
      detailError: visibleDetailError,
      refreshAgents: loadAgents,
      reloadDetail,
    }),
    [
      visibleAgents,
      visibleAgentsStatus,
      visibleAgentsError,
      visibleSelectedRef,
      selectAgent,
      visibleDetail,
      visibleDetailStatus,
      visibleDetailError,
      loadAgents,
      reloadDetail,
    ],
  )
}

export { agentRefKey }
