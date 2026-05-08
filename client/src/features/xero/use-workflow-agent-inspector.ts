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

export type AgentListStatus = 'idle' | 'loading' | 'ready' | 'error'

export type AgentDetailStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface UseWorkflowAgentInspectorOptions {
  adapter: Pick<XeroDesktopAdapter, 'listWorkflowAgents' | 'getWorkflowAgentDetail'>
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

export function useWorkflowAgentInspector(
  options: UseWorkflowAgentInspectorOptions,
): UseWorkflowAgentInspectorResult {
  const { adapter, projectId } = options
  const [agents, setAgents] = useState<WorkflowAgentSummaryDto[]>([])
  const [agentsStatus, setAgentsStatus] = useState<AgentListStatus>('idle')
  const [agentsError, setAgentsError] = useState<Error | null>(null)

  const [selectedRef, setSelectedRefState] = useState<AgentRefDto | null>(() => readPersistedRef())
  const [detail, setDetail] = useState<WorkflowAgentDetailDto | null>(null)
  const [detailStatus, setDetailStatus] = useState<AgentDetailStatus>('idle')
  const [detailError, setDetailError] = useState<Error | null>(null)

  const detailRequestId = useRef(0)
  const listRequestId = useRef(0)

  const loadAgents = useCallback(async () => {
    if (!projectId) {
      setAgents([])
      setAgentsStatus('idle')
      setAgentsError(null)
      return
    }
    const requestId = ++listRequestId.current
    setAgentsStatus('loading')
    setAgentsError(null)
    try {
      const response = await adapter.listWorkflowAgents({ projectId, includeArchived: false })
      if (requestId !== listRequestId.current) return
      setAgents(response.agents)
      setAgentsStatus('ready')
    } catch (error) {
      if (requestId !== listRequestId.current) return
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
      const requestId = ++detailRequestId.current
      setDetailStatus('loading')
      setDetailError(null)
      try {
        const response = await adapter.getWorkflowAgentDetail({ projectId, ref })
        if (requestId !== detailRequestId.current) return
        setDetail(response)
        setDetailStatus('ready')
      } catch (error) {
        if (requestId !== detailRequestId.current) return
        setDetail(null)
        setDetailError(error instanceof Error ? error : new Error(String(error)))
        setDetailStatus('error')
      }
    },
    [adapter, projectId],
  )

  // Initial load + reload on project change.
  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  // Drive detail fetch from selection.
  useEffect(() => {
    if (!selectedRef) {
      setDetail(null)
      setDetailStatus('idle')
      setDetailError(null)
      return
    }
    void loadDetail(selectedRef)
  }, [selectedRef, loadDetail])

  const selectAgent = useCallback((ref: AgentRefDto | null) => {
    setSelectedRefState((prev) => {
      if (!ref) {
        if (prev !== null) persistRef(null)
        return null
      }
      if (prev && agentRefsEqual(prev, ref)) return prev
      persistRef(ref)
      return ref
    })
  }, [])

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
      persistRef(null)
    }
  }, [agents, agentsStatus, selectedRef])

  return useMemo(
    () => ({
      agents,
      agentsStatus,
      agentsError,
      selectedRef,
      selectAgent,
      detail,
      detailStatus,
      detailError,
      refreshAgents: loadAgents,
      reloadDetail,
    }),
    [
      agents,
      agentsStatus,
      agentsError,
      selectedRef,
      selectAgent,
      detail,
      detailStatus,
      detailError,
      loadAgents,
      reloadDetail,
    ],
  )
}

export { agentRefKey }
