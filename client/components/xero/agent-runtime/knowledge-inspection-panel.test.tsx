import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { KnowledgeInspectionPanel } from '@/components/xero/agent-runtime/knowledge-inspection-panel'
import type { AgentKnowledgeInspectionDto } from '@/src/lib/xero-model/agent-reports'

const fixture: AgentKnowledgeInspectionDto = {
  schema: 'xero.agent_knowledge_inspection.v1',
  projectId: 'project-1',
  agentSessionId: 'agent-session-main',
  runId: 'run-abc-123',
  limit: 25,
  retrievalPolicy: {
    source: 'runtime_audit_export',
    policy: {
      deliveryModel: 'tool_mediated',
    },
    recordKindFilter: ['project_fact', 'context_note'],
    memoryKindFilter: ['decision'],
    filtersApplied: true,
  },
  projectRecords: [
    {
      recordId: 'record-project-fact',
      recordKind: 'project_fact',
      title: 'Storage uses app-data',
      summary: 'Project state goes under OS app-data.',
      textPreview: 'Persist new project state under the OS app-data directory.',
      schemaName: null,
      importance: 'high',
      confidence: 0.94,
      tags: ['storage'],
      relatedPaths: ['client/src-tauri/src/db/project_store'],
      freshnessState: 'current',
      redactionState: 'clean',
      sourceItemIds: ['manifest-1'],
      updatedAt: '2026-05-10T10:00:00Z',
    },
  ],
  continuityRecords: [
    {
      recordId: 'record-continuity',
      recordKind: 'context_note',
      title: 'Current problem continuity',
      summary: null,
      textPreview: null,
      schemaName: 'xero.project_record.current_problem_continuity.v1',
      importance: 'normal',
      confidence: null,
      tags: ['handoff'],
      relatedPaths: [],
      freshnessState: 'current',
      redactionState: 'redacted',
      sourceItemIds: ['continuity-1'],
      updatedAt: '2026-05-10T10:00:00Z',
    },
  ],
  approvedMemory: [
    {
      memoryId: 'memory-decision',
      scope: 'session',
      kind: 'decision',
      textPreview: 'Keep UI work deferred until the end.',
      confidence: 93,
      sourceRunId: 'run-abc-123',
      sourceItemIds: ['memory-source-1'],
      freshnessState: 'current',
      updatedAt: '2026-05-10T10:00:00Z',
    },
  ],
  handoffRecords: [
    {
      handoffId: 'handoff-1',
      status: 'completed',
      sourceRunId: 'run-abc-123',
      targetRunId: 'run-xyz-456',
      runtimeAgentId: 'engineer',
      agentDefinitionId: 'custom-engineer',
      agentDefinitionVersion: 3,
      providerId: 'openrouter',
      modelId: 'openai/gpt-5.4',
      handoffRecordId: 'record-handoff',
      bundleKeys: ['userGoal', 'pendingWork'],
      createdAt: '2026-05-10T10:00:00Z',
      updatedAt: '2026-05-10T10:00:00Z',
    },
  ],
  redaction: {
    rawBlockedRecordsExcluded: true,
    redactedProjectRecordTextHidden: true,
    handoffBundleRawPayloadHidden: true,
  },
}

describe('KnowledgeInspectionPanel', () => {
  it('does not render dialog content while closed', () => {
    render(
      <KnowledgeInspectionPanel
        open={false}
        onOpenChange={() => undefined}
        status="idle"
        errorMessage={null}
        inspection={null}
        onRefresh={() => undefined}
      />,
    )
    expect(screen.queryByText(/what this agent can see right now/i)).not.toBeInTheDocument()
  })

  it('renders an idle hint when there is no inspection yet', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="idle"
        errorMessage={null}
        inspection={null}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByText(/start an agent run to see what records/i),
    ).toBeInTheDocument()
  })

  it('renders the spinner while loading and no prior inspection is cached', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="loading"
        errorMessage={null}
        inspection={null}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByText(/inspecting the agent's current knowledge/i),
    ).toBeInTheDocument()
  })

  it('renders an error message when the load fails', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="error"
        errorMessage="Backend unavailable."
        inspection={null}
        onRefresh={() => undefined}
      />,
    )
    expect(screen.getByText('Backend unavailable.')).toBeInTheDocument()
  })

  it('renders each section with counts and a fixture inspection', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )

    expect(screen.getByText('Project records')).toBeInTheDocument()
    expect(screen.getByText('Continuity records')).toBeInTheDocument()
    expect(screen.getByText('Approved memory')).toBeInTheDocument()
    expect(screen.getByText('Handoff records')).toBeInTheDocument()

    expect(screen.getAllByText('1 / 25').length).toBe(4)

    expect(screen.getByText('Storage uses app-data')).toBeInTheDocument()
    expect(
      screen.getByText(/persist new project state under the os app-data directory/i),
    ).toBeInTheDocument()

    expect(screen.getByText('Keep UI work deferred until the end.')).toBeInTheDocument()
    expect(screen.getByText('handoff-1')).toBeInTheDocument()
  })

  it('hides redacted continuity record text and shows a placeholder instead', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )

    expect(
      screen.getByText(/text hidden — record is redacted/i),
    ).toBeInTheDocument()
  })

  it('lists the retrieval policy filters when filters are applied', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )

    const filterHeader = screen.getByText('Retrieval policy filters')
    const filterBlock = filterHeader.closest('div')?.parentElement ?? null
    expect(filterBlock).not.toBeNull()
    if (filterBlock) {
      const region = within(filterBlock as HTMLElement)
      expect(region.getByText('project_fact')).toBeInTheDocument()
      expect(region.getByText('context_note')).toBeInTheDocument()
      expect(region.getByText('decision')).toBeInTheDocument()
    }
  })

  it('surfaces redaction policy notes when applied by the retrieval pipeline', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )

    expect(screen.getByText(/blocked records excluded/i)).toBeInTheDocument()
    expect(screen.getByText(/redacted record text hidden/i)).toBeInTheDocument()
    expect(screen.getByText(/handoff bundle payloads hidden/i)).toBeInTheDocument()
  })

  it('invokes the refresh callback when the user clicks refresh', () => {
    const onRefresh = vi.fn()
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={onRefresh}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /refresh knowledge inspection/i }))
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('disables the refresh button while the inspection is loading', () => {
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={() => undefined}
        status="loading"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByRole('button', { name: /refresh knowledge inspection/i }),
    ).toBeDisabled()
  })

  it('asks the host to close when the user clicks the close button', () => {
    const onOpenChange = vi.fn()
    render(
      <KnowledgeInspectionPanel
        open
        onOpenChange={onOpenChange}
        status="ready"
        errorMessage={null}
        inspection={fixture}
        onRefresh={() => undefined}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /close knowledge inspection/i }))
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
