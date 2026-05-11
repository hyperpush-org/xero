import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { HandoffContextDialog } from '@/components/xero/agent-runtime/handoff-context-dialog'
import type { AgentHandoffContextSummaryDto } from '@/src/lib/xero-model/agent-reports'

const createdAt = '2026-05-10T10:00:00Z'

const fixture: AgentHandoffContextSummaryDto = {
  schema: 'xero.agent_handoff_context_summary.v1',
  projectId: 'project-handoff-dialog',
  handoffId: 'handoff-dialog-1',
  status: 'completed',
  source: {
    agentSessionId: 'agent-session-source',
    runId: 'run-source-123',
    runtimeAgentId: 'engineer',
    agentDefinitionId: 'custom-engineer',
    agentDefinitionVersion: 4,
    contextHash: 'context-hash-source',
  },
  target: {
    agentSessionId: 'agent-session-target',
    runId: 'run-target-456',
    runtimeAgentId: 'engineer',
    agentDefinitionId: 'custom-engineer',
    agentDefinitionVersion: 4,
  },
  provider: { providerId: 'openrouter', modelId: 'openai/gpt-5.4' },
  carriedContext: {
    userGoal: 'Ship the handoff context dialog.',
    currentTask: 'Render carried context and lineage in a dialog.',
    currentStatus: 'in_progress',
    completedWork: [
      {
        messageId: 14,
        createdAt,
        summary: 'Backend schema confirmed.',
      },
    ],
    pendingWork: [
      {
        kind: 'user_prompt',
        text: 'Wire dialog to agent runtime.',
      },
    ],
    activeTodoItems: [
      {
        id: 'todo-1',
        status: 'pending',
        text: 'Add tests for the dialog.',
      },
    ],
    importantDecisions: [
      {
        kind: 'decision',
        eventId: 30,
        eventKind: 'PlanUpdated',
        createdAt,
        summary: 'Existing notice becomes clickable.',
      },
    ],
    constraints: [
      'Raw bundle payload must remain hidden.',
      'Use ShadCN dialog primitives.',
    ],
    durableContext: {
      deliveryModel: 'tool_mediated',
      toolName: 'project_context',
      rawContextInjected: false,
      sourceContextHash: 'context-hash-source',
      instruction: 'Use project_context for durable context.',
    },
    workingSetSummary: {
      schema: 'xero.agent_handoff.working_set.v1',
      sourceRunId: 'run-source-123',
      sourceContextHash: 'context-hash-source',
      activeTodoCount: 1,
      recentFileChangeCount: 1,
      latestChangedPaths: [
        'client/components/xero/agent-runtime/handoff-context-dialog.tsx',
      ],
      assistantMessageIds: [14],
    },
    sourceCitedContinuityRecords: [
      {
        sourceKind: 'agent_message',
        sourceId: 14,
        createdAt,
        summary: 'Backend schema confirmed.',
      },
    ],
    recentFileChanges: [
      {
        path: 'client/components/xero/agent-runtime/handoff-context-dialog.tsx',
        operation: 'created',
        oldHash: null,
        newHash: 'new-hash-dialog',
        createdAt,
      },
    ],
    toolAndCommandEvidence: [
      {
        toolCallId: 'tool-1',
        toolName: 'read',
        state: 'Succeeded',
        inputPreview: 'src/lib/xero-model/agent-reports.ts',
        error: null,
      },
    ],
    verificationStatus: {
      status: 'recorded',
      evidence: [
        {
          kind: 'verification',
          eventId: 31,
          eventKind: 'VerificationGate',
          createdAt,
          summary: 'Dialog tests planned.',
        },
      ],
    },
    knownRisks: [],
    openQuestions: [],
    approvedMemories: [],
    relevantProjectRecords: [],
    agentSpecific: {},
  },
  omittedContext: [
    {
      kind: 'raw_bundle_payload',
      status: 'hidden',
      reason: 'This summary exposes only whitelisted carried-context fields.',
    },
    {
      kind: 'raw_transcript',
      status: 'omitted',
      reason: 'Raw transcript is not part of the durable bundle.',
      referenceCount: 5,
    },
  ],
  redaction: {
    state: 'redacted',
    bundleRedactionCount: 2,
    summaryRedactionApplied: true,
    rawPayloadHidden: true,
  },
  safetyRationale: {
    sameRuntimeAgent: true,
    sameDefinitionVersion: true,
    sourceContextHashPresent: true,
    targetRunCreated: true,
    handoffRecordPersisted: true,
    reasons: [
      'Source context hash present.',
      'Target run created in same project.',
    ],
  },
  createdAt,
  updatedAt: createdAt,
  completedAt: '2026-05-10T10:05:00Z',
  uiDeferred: true,
}

describe('HandoffContextDialog', () => {
  it('does not render dialog content while closed', () => {
    render(
      <HandoffContextDialog
        open={false}
        onOpenChange={() => undefined}
        status="idle"
        errorMessage={null}
        summary={null}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.queryByText(/what carried over in this handoff/i),
    ).not.toBeInTheDocument()
  })

  it('renders an idle hint when no summary is loaded yet', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="idle"
        errorMessage={null}
        summary={null}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByText(/no handoff context summary is loaded yet/i),
    ).toBeInTheDocument()
  })

  it('renders the spinner while loading and no prior summary is cached', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="loading"
        errorMessage={null}
        summary={null}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByText(/loading handoff context summary/i),
    ).toBeInTheDocument()
  })

  it('renders an error message when the load fails', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="error"
        errorMessage="Backend unavailable."
        summary={null}
        onRefresh={() => undefined}
      />,
    )
    expect(screen.getByText('Backend unavailable.')).toBeInTheDocument()
  })

  it('renders lineage, definition pin, carried context, and omitted sections', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={fixture}
        onRefresh={() => undefined}
      />,
    )

    expect(screen.getByText('handoff-dialog-1')).toBeInTheDocument()
    expect(screen.getByText('completed')).toBeInTheDocument()
    expect(screen.getByText(/run-source-123\s+→\s+run-target-456/)).toBeInTheDocument()
    expect(screen.getByText('openrouter · openai/gpt-5.4')).toBeInTheDocument()

    expect(screen.getByText('Definition pin')).toBeInTheDocument()
    expect(screen.getByText('pinned')).toBeInTheDocument()
    expect(screen.getAllByText('custom-engineer v4').length).toBeGreaterThanOrEqual(2)

    expect(screen.getByText('Carried context')).toBeInTheDocument()
    expect(screen.getByText('Ship the handoff context dialog.')).toBeInTheDocument()
    expect(screen.getByText('Render carried context and lineage in a dialog.')).toBeInTheDocument()

    expect(screen.getByText('Working set')).toBeInTheDocument()
    expect(
      screen.getByText(
        'client/components/xero/agent-runtime/handoff-context-dialog.tsx',
      ),
    ).toBeInTheDocument()

    expect(screen.getByText('What was omitted')).toBeInTheDocument()
    expect(screen.getByText('raw_bundle_payload')).toBeInTheDocument()
    expect(screen.getByText('raw_transcript')).toBeInTheDocument()
    expect(screen.getByText(/5 references/)).toBeInTheDocument()
  })

  it('surfaces redaction state and counts in the redaction strip', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={fixture}
        onRefresh={() => undefined}
      />,
    )

    expect(screen.getByText(/State: redacted/)).toBeInTheDocument()
    expect(screen.getByText(/2 bundle field\(s\) redacted/)).toBeInTheDocument()
    expect(screen.getByText(/Summary redaction applied/)).toBeInTheDocument()
    expect(screen.getByText(/Raw bundle payload hidden/)).toBeInTheDocument()
  })

  it('renders the safety rationale indicators and reasons', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={fixture}
        onRefresh={() => undefined}
      />,
    )

    const rationale = screen.getByLabelText('Safety rationale')
    const region = within(rationale)
    expect(region.getByText('Same runtime agent')).toBeInTheDocument()
    expect(region.getByText('Source context hash present')).toBeInTheDocument()
    expect(region.getAllByText('yes').length).toBeGreaterThanOrEqual(5)
    expect(region.getByText(/source context hash present\./i)).toBeInTheDocument()
    expect(region.getByText(/target run created in same project\./i)).toBeInTheDocument()
  })

  it('flags definition pin as changed when source and target differ', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={{
          ...fixture,
          target: {
            ...fixture.target,
            agentDefinitionVersion: 5,
          },
        }}
        onRefresh={() => undefined}
      />,
    )
    expect(screen.getByText('changed')).toBeInTheDocument()
  })

  it('shows a placeholder when no work was omitted', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={{ ...fixture, omittedContext: [] }}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByText(/nothing was omitted from the handoff bundle/i),
    ).toBeInTheDocument()
  })

  it('invokes the refresh callback when the user clicks refresh', () => {
    const onRefresh = vi.fn()
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="ready"
        errorMessage={null}
        summary={fixture}
        onRefresh={onRefresh}
      />,
    )
    fireEvent.click(
      screen.getByRole('button', { name: /refresh handoff context summary/i }),
    )
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('disables the refresh button while the summary is loading', () => {
    render(
      <HandoffContextDialog
        open
        onOpenChange={() => undefined}
        status="loading"
        errorMessage={null}
        summary={fixture}
        onRefresh={() => undefined}
      />,
    )
    expect(
      screen.getByRole('button', { name: /refresh handoff context summary/i }),
    ).toBeDisabled()
  })

  it('asks the host to close when the user clicks the close button', () => {
    const onOpenChange = vi.fn()
    render(
      <HandoffContextDialog
        open
        onOpenChange={onOpenChange}
        status="ready"
        errorMessage={null}
        summary={fixture}
        onRefresh={() => undefined}
      />,
    )
    fireEvent.click(
      screen.getByRole('button', { name: /close handoff context summary/i }),
    )
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
