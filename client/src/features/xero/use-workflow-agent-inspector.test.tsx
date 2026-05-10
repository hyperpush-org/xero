import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type {
  AgentRefDto,
  WorkflowAgentDetailDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'

import { useWorkflowAgentInspector } from './use-workflow-agent-inspector'

const selectedRef: AgentRefDto = {
  kind: 'built_in',
  runtimeAgentId: 'engineer',
  version: 1,
}

const nextRef: AgentRefDto = {
  kind: 'custom',
  definitionId: 'custom-agent',
  version: 1,
}

const selectedSummary: WorkflowAgentSummaryDto = {
  ref: selectedRef,
  displayName: 'Engineer',
  shortLabel: 'Eng',
  description: 'Implementation agent',
  scope: 'built_in',
  lifecycleState: 'active',
  baseCapabilityProfile: 'engineering',
  lastUsedAt: null,
  useCount: 0,
}

function createDeferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve
    reject = promiseReject
  })
  return { promise, resolve, reject }
}

function makeDetail(ref: AgentRefDto): WorkflowAgentDetailDto {
  return {
    ref,
    header: {
      displayName: 'Engineer',
      shortLabel: 'Eng',
      description: 'Implementation agent',
      taskPurpose: 'Implement changes',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'engineering',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest'],
      allowPlanGate: true,
      allowVerificationGate: true,
      allowAutoCompact: true,
    },
    prompts: [],
    tools: [],
    dbTouchpoints: {
      reads: [],
      writes: [],
      encouraged: [],
    },
    output: {
      contract: 'engineering_summary',
      label: 'Engineering Summary',
      description: 'Summary of implementation work',
      sections: [],
    },
    consumes: [],
    attachedSkills: [],
    graphProjection: {
      schema: 'xero.workflow_agent_graph_projection.v1',
      nodes: [],
      edges: [],
      groups: [],
    },
  } as WorkflowAgentDetailDto
}

describe('useWorkflowAgentInspector', () => {
  afterEach(() => {
    window.localStorage.clear()
  })

  it('persists selected agent refs through project UI state when available', async () => {
    const readProjectUiState = vi.fn(async () => ({
      schema: 'xero.project_ui_state.v1' as const,
      projectId: 'project-1',
      key: 'workflows.selected-agent.v1',
      value: selectedRef,
      storageScope: 'os_app_data' as const,
      uiDeferred: true,
    }))
    const writeProjectUiState = vi.fn(async () => ({
      schema: 'xero.project_ui_state.v1' as const,
      projectId: 'project-1',
      key: 'workflows.selected-agent.v1',
      value: nextRef,
      storageScope: 'os_app_data' as const,
      uiDeferred: true,
    }))
    const adapter = {
      listWorkflowAgents: vi.fn(async () => ({ agents: [selectedSummary] })),
      getWorkflowAgentDetail: vi.fn(async () => ({})),
      readProjectUiState,
      writeProjectUiState,
    } as unknown as Pick<
      XeroDesktopAdapter,
      | 'listWorkflowAgents'
      | 'getWorkflowAgentDetail'
      | 'readProjectUiState'
      | 'writeProjectUiState'
    >

    const { result } = renderHook(() =>
      useWorkflowAgentInspector({ adapter, projectId: 'project-1' }),
    )

    await waitFor(() => expect(result.current.selectedRef).toEqual(selectedRef))

    act(() => {
      result.current.selectAgent(nextRef)
    })

    expect(writeProjectUiState).toHaveBeenCalledWith({
      projectId: 'project-1',
      key: 'workflows.selected-agent.v1',
      value: nextRef,
    })
    expect(window.localStorage.getItem('xero.workflows.selectedAgent')).toBeNull()
  })

  it('hides stale selected detail immediately when the project changes', async () => {
    const projectTwoSelection = createDeferred<{
      schema: 'xero.project_ui_state.v1'
      projectId: string
      key: string
      value: AgentRefDto | null
      storageScope: 'os_app_data'
      uiDeferred: true
    }>()
    const readProjectUiState = vi.fn(async ({ projectId }: { projectId: string }) => {
      if (projectId === 'project-2') {
        return projectTwoSelection.promise
      }

      return {
        schema: 'xero.project_ui_state.v1' as const,
        projectId,
        key: 'workflows.selected-agent.v1',
        value: selectedRef,
        storageScope: 'os_app_data' as const,
        uiDeferred: true,
      }
    })
    const adapter = {
      listWorkflowAgents: vi.fn(async () => ({ agents: [selectedSummary] })),
      getWorkflowAgentDetail: vi.fn(async ({ ref }: { ref: AgentRefDto }) => makeDetail(ref)),
      readProjectUiState,
      writeProjectUiState: vi.fn(async () => ({
        schema: 'xero.project_ui_state.v1' as const,
        projectId: 'project-1',
        key: 'workflows.selected-agent.v1',
        value: null,
        storageScope: 'os_app_data' as const,
        uiDeferred: true,
      })),
    } as unknown as Pick<
      XeroDesktopAdapter,
      | 'listWorkflowAgents'
      | 'getWorkflowAgentDetail'
      | 'readProjectUiState'
      | 'writeProjectUiState'
    >

    const { result, rerender } = renderHook(
      ({ projectId }) => useWorkflowAgentInspector({ adapter, projectId }),
      { initialProps: { projectId: 'project-1' } },
    )

    await waitFor(() => expect(result.current.detail).toEqual(makeDetail(selectedRef)))

    rerender({ projectId: 'project-2' })

    expect(result.current.selectedRef).toBeNull()
    expect(result.current.detail).toBeNull()
    expect(result.current.detailStatus).toBe('idle')
    expect(adapter.getWorkflowAgentDetail).not.toHaveBeenCalledWith({
      projectId: 'project-2',
      ref: selectedRef,
    })
  })
})
