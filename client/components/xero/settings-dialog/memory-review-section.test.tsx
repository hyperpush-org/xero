import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  MemoryReviewSection,
  type MemoryReviewAdapter,
} from '@/components/xero/settings-dialog/memory-review-section'
import type {
  AgentMemoryReviewQueueItemDto,
  CorrectSessionMemoryResponseDto,
  GetSessionMemoryReviewQueueResponseDto,
  SessionMemoryRecordDto,
} from '@/src/lib/xero-model/session-context'

const PROJECT_ID = 'project-1'
const SESSION_ID = 'session-7'
const CREATED_AT = '2026-05-09T18:00:00Z'

function makeItem(
  overrides: Partial<AgentMemoryReviewQueueItemDto> & Pick<AgentMemoryReviewQueueItemDto, 'memoryId'>,
): AgentMemoryReviewQueueItemDto {
  return {
    memoryId: overrides.memoryId,
    scope: overrides.scope ?? 'session',
    kind: overrides.kind ?? 'project_fact',
    reviewState: overrides.reviewState ?? 'candidate',
    enabled: overrides.enabled ?? false,
    confidence: overrides.confidence ?? 72,
    textPreview:
      overrides.textPreview ??
      'User prefers TypeScript strict mode for new modules in the xero crate.',
    textHash: overrides.textHash ?? 'sha256:abc',
    provenance: overrides.provenance ?? {
      sourceRunId: 'run-12',
      sourceItemIds: ['message-3'],
      diagnostic: null,
    },
    freshness: overrides.freshness ?? {
      state: 'current',
      checkedAt: CREATED_AT,
      staleReason: null,
      supersedesId: null,
      supersededById: null,
      invalidatedAt: null,
      factKey: null,
    },
    retrieval: overrides.retrieval ?? {
      eligible: false,
      reason: 'pending_or_rejected_review',
    },
    redaction: overrides.redaction ?? {
      textPreviewRedacted: false,
      factKeyRedacted: false,
      rawTextHidden: true,
    },
    availableActions: overrides.availableActions ?? {
      canApprove: true,
      canReject: true,
      canDisable: true,
      canDelete: true,
      canEditByCorrection: true,
    },
    createdAt: overrides.createdAt ?? CREATED_AT,
    updatedAt: overrides.updatedAt ?? CREATED_AT,
  }
}

function makeQueueResponse(items: AgentMemoryReviewQueueItemDto[]): GetSessionMemoryReviewQueueResponseDto {
  const counts = {
    candidate: items.filter((item) => item.reviewState === 'candidate').length,
    approved: items.filter((item) => item.reviewState === 'approved').length,
    rejected: items.filter((item) => item.reviewState === 'rejected').length,
    disabled: items.filter((item) => !item.enabled).length,
    retrievableApproved: items.filter(
      (item) => item.reviewState === 'approved' && item.retrieval.eligible,
    ).length,
  }
  return {
    schema: 'xero.agent_memory_review_queue.v1',
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    limit: 50,
    counts,
    items,
    actions: {
      approve: 'Approve memory',
      reject: 'Reject memory',
      disable: 'Disable memory',
      delete: 'Delete memory',
      edit: 'Create a corrected memory',
    },
    uiDeferred: true,
  }
}

function makeAdapter(initial: GetSessionMemoryReviewQueueResponseDto): {
  adapter: MemoryReviewAdapter
  getQueue: ReturnType<typeof vi.fn>
  updateMemory: ReturnType<typeof vi.fn>
  correctMemory: ReturnType<typeof vi.fn>
  deleteMemory: ReturnType<typeof vi.fn>
} {
  const getQueue = vi.fn<MemoryReviewAdapter['getQueue']>().mockResolvedValue(initial)
  const updateMemory = vi.fn<MemoryReviewAdapter['updateMemory']>()
  const correctMemory = vi.fn<MemoryReviewAdapter['correctMemory']>()
  const deleteMemory = vi.fn<MemoryReviewAdapter['deleteMemory']>().mockResolvedValue(undefined)
  return {
    adapter: { getQueue, updateMemory, correctMemory, deleteMemory },
    getQueue,
    updateMemory,
    correctMemory,
    deleteMemory,
  }
}

function dummyMemoryRecord(memoryId: string): SessionMemoryRecordDto {
  return {
    memoryId,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    scope: 'session',
    kind: 'fact',
    reviewState: 'approved',
    enabled: true,
    text: '',
    textHash: 'sha256:abc',
    confidence: 80,
    sourceRunId: 'run-12',
    sourceItemIds: ['message-3'],
    diagnostic: null,
    createdAt: CREATED_AT,
    updatedAt: CREATED_AT,
  } as unknown as SessionMemoryRecordDto
}

describe('MemoryReviewSection', () => {
  it('shows the project-bound empty state when no project is selected', () => {
    render(<MemoryReviewSection projectId={null} projectLabel={null} adapter={null} />)
    expect(screen.getByText('Select a project')).toBeInTheDocument()
  })

  it('renders queue counts and items returned by the adapter', async () => {
    const candidate = makeItem({ memoryId: 'mem-1' })
    const approved = makeItem({
      memoryId: 'mem-2',
      reviewState: 'approved',
      enabled: true,
      retrieval: { eligible: true, reason: 'retrievable' },
    })
    const queue = makeQueueResponse([candidate, approved])
    const { adapter, getQueue } = makeAdapter(queue)

    render(
      <MemoryReviewSection
        projectId={PROJECT_ID}
        projectLabel="Xero"
        agentSessionId={SESSION_ID}
        adapter={adapter}
      />,
    )

    await waitFor(() => {
      expect(getQueue).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        limit: 50,
      })
    })

    expect(await screen.findAllByTestId('memory-review-item')).toHaveLength(2)

    const counts = screen.getByTestId('memory-review-counts')
    expect(within(counts).getByLabelText('Candidates: 1')).toBeVisible()
    expect(within(counts).getByLabelText('Approved: 1')).toBeVisible()
    expect(within(counts).getByLabelText('Retrievable: 1')).toBeVisible()
  })

  it('hides the preview and disables Approve for redacted (secret-shaped) memory', async () => {
    const redacted = makeItem({
      memoryId: 'mem-secret',
      textPreview: null,
      redaction: { textPreviewRedacted: true, factKeyRedacted: true, rawTextHidden: true },
      availableActions: {
        canApprove: false,
        canReject: true,
        canDisable: true,
        canDelete: true,
        canEditByCorrection: true,
      },
    })
    const queue = makeQueueResponse([redacted])
    const { adapter } = makeAdapter(queue)

    render(
      <MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />,
    )

    const item = await screen.findByTestId('memory-review-item')
    expect(within(item).queryByTestId('memory-preview')).toBeNull()
    expect(within(item).getByTestId('memory-redacted-notice')).toBeInTheDocument()
    expect(within(item).getByTestId('redaction-badge')).toBeInTheDocument()
    expect(within(item).getByRole('button', { name: 'Approve memory' })).toBeDisabled()
    expect(within(item).getByRole('button', { name: 'Edit memory' })).toBeEnabled()
  })

  it('approves a candidate and refetches the queue', async () => {
    const candidate = makeItem({ memoryId: 'mem-1' })
    const before = makeQueueResponse([candidate])
    const after = makeQueueResponse([
      { ...candidate, reviewState: 'approved', enabled: true },
    ])

    const { adapter, getQueue, updateMemory } = makeAdapter(before)
    getQueue.mockResolvedValueOnce(before).mockResolvedValueOnce(after)
    updateMemory.mockResolvedValue(dummyMemoryRecord('mem-1'))

    render(<MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const item = await screen.findByTestId('memory-review-item')
    fireEvent.click(within(item).getByRole('button', { name: 'Approve memory' }))

    await waitFor(() => {
      expect(updateMemory).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        memoryId: 'mem-1',
        reviewState: 'approved',
        enabled: true,
      })
    })
    await waitFor(() => {
      expect(getQueue).toHaveBeenCalledTimes(2)
    })
  })

  it('rejects a candidate', async () => {
    const candidate = makeItem({ memoryId: 'mem-r' })
    const { adapter, updateMemory } = makeAdapter(makeQueueResponse([candidate]))
    updateMemory.mockResolvedValue(dummyMemoryRecord('mem-r'))

    render(<MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const item = await screen.findByTestId('memory-review-item')
    fireEvent.click(within(item).getByRole('button', { name: 'Reject memory' }))

    await waitFor(() => {
      expect(updateMemory).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        memoryId: 'mem-r',
        reviewState: 'rejected',
      })
    })
  })

  it('deletes a memory', async () => {
    const item = makeItem({ memoryId: 'mem-del' })
    const { adapter, deleteMemory } = makeAdapter(makeQueueResponse([item]))

    render(<MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const row = await screen.findByTestId('memory-review-item')
    fireEvent.click(within(row).getByRole('button', { name: 'Delete memory' }))

    await waitFor(() => {
      expect(deleteMemory).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        memoryId: 'mem-del',
      })
    })
  })

  it('submits a correction with the edited text', async () => {
    const item = makeItem({ memoryId: 'mem-edit' })
    const { adapter, correctMemory } = makeAdapter(makeQueueResponse([item]))
    const correctionResponse: CorrectSessionMemoryResponseDto = {
      schema: 'xero.agent_memory_correction_command.v1',
      projectId: PROJECT_ID,
      originalMemory: dummyMemoryRecord('mem-edit'),
      correctedMemory: dummyMemoryRecord('mem-edit-2'),
      uiDeferred: true,
    } as unknown as CorrectSessionMemoryResponseDto
    correctMemory.mockResolvedValue(correctionResponse)

    render(<MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const row = await screen.findByTestId('memory-review-item')
    fireEvent.click(within(row).getByRole('button', { name: 'Edit memory' }))

    const textarea = await screen.findByLabelText('Corrected memory text')
    fireEvent.change(textarea, { target: { value: 'Sanitized memory text' } })
    fireEvent.click(screen.getByRole('button', { name: /save correction/i }))

    await waitFor(() => {
      expect(correctMemory).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        memoryId: 'mem-edit',
        correctedText: 'Sanitized memory text',
      })
    })
  })

  it('surfaces adapter errors without losing the queue', async () => {
    const item = makeItem({ memoryId: 'mem-x' })
    const { adapter, updateMemory } = makeAdapter(makeQueueResponse([item]))
    updateMemory.mockRejectedValue(new Error('Network down'))

    render(<MemoryReviewSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const row = await screen.findByTestId('memory-review-item')
    fireEvent.click(within(row).getByRole('button', { name: 'Approve memory' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Network down')
    // queue items are still rendered
    expect(screen.getByTestId('memory-review-item')).toBeInTheDocument()
  })
})
