import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  ProjectRecordsSection,
  type ProjectRecordsAdapter,
} from '@/components/xero/settings-dialog/project-records-section'
import type {
  ListProjectContextRecordsResponseDto,
  ProjectContextRecordSummaryDto,
} from '@/src/lib/xero-model/project-records'

const PROJECT_ID = 'project-records-test'

function makeRecord(
  overrides: Partial<ProjectContextRecordSummaryDto> &
    Pick<ProjectContextRecordSummaryDto, 'recordId' | 'title'>,
): ProjectContextRecordSummaryDto {
  const base: ProjectContextRecordSummaryDto = {
    recordId: overrides.recordId,
    title: overrides.title,
    recordKind: 'finding',
    summary: 'Stale finding from a prior run.',
    textPreview: null,
    importance: 'normal',
    redactionState: 'clean',
    visibility: 'retrieval',
    freshnessState: 'current',
    tags: [],
    relatedPaths: [],
    supersedesId: null,
    supersededById: null,
    invalidatedAt: null,
    runtimeAgentId: 'engineer',
    agentDefinitionId: 'def-1',
    agentDefinitionVersion: 1,
    runId: 'run-1',
    createdAt: '2026-05-09T18:00:00Z',
    updatedAt: '2026-05-09T18:00:00Z',
  }
  return { ...base, ...overrides }
}

function makeListResponse(
  records: ProjectContextRecordSummaryDto[],
): ListProjectContextRecordsResponseDto {
  return {
    schema: 'xero.project_context_record_list_command.v1',
    projectId: PROJECT_ID,
    records,
    uiDeferred: true,
  }
}

function makeAdapter(initial: ListProjectContextRecordsResponseDto): {
  adapter: ProjectRecordsAdapter
  listRecords: ReturnType<typeof vi.fn>
  deleteRecord: ReturnType<typeof vi.fn>
  supersedeRecord: ReturnType<typeof vi.fn>
} {
  const listRecords = vi
    .fn<ProjectRecordsAdapter['listRecords']>()
    .mockResolvedValue(initial)
  const deleteRecord = vi.fn<ProjectRecordsAdapter['deleteRecord']>()
  const supersedeRecord = vi.fn<ProjectRecordsAdapter['supersedeRecord']>()
  return {
    adapter: { listRecords, deleteRecord, supersedeRecord },
    listRecords,
    deleteRecord,
    supersedeRecord,
  }
}

describe('ProjectRecordsSection', () => {
  it('renders supersede chains with predecessor and successor lineage', async () => {
    const predecessor = makeRecord({
      recordId: 'project-record-old',
      title: 'Stale fact: limit is 5',
      supersededById: 'project-record-new',
    })
    const successor = makeRecord({
      recordId: 'project-record-new',
      title: 'Corrected fact: limit is 10',
      supersedesId: 'project-record-old',
    })
    const { adapter, listRecords } = makeAdapter(makeListResponse([predecessor, successor]))

    render(
      <ProjectRecordsSection
        projectId={PROJECT_ID}
        projectLabel="xero"
        adapter={adapter}
      />,
    )

    await waitFor(() => expect(listRecords).toHaveBeenCalledTimes(1))
    await screen.findByText('Stale fact: limit is 5')

    const predecessorChain = await screen.findByTestId(
      `supersede-chain-${predecessor.recordId}`,
    )
    expect(within(predecessorChain).getByText('Superseded by')).toBeInTheDocument()
    expect(within(predecessorChain).getByText('project-record-new')).toBeInTheDocument()
    expect(
      within(predecessorChain).getByText(/Corrected fact: limit is 10/),
    ).toBeInTheDocument()
    const predecessorRow = within(predecessorChain).getByText('project-record-new')
      .parentElement as HTMLElement
    expect(predecessorRow.getAttribute('data-direction')).toBe('to')

    const successorChain = await screen.findByTestId(
      `supersede-chain-${successor.recordId}`,
    )
    expect(within(successorChain).getByText('Supersedes')).toBeInTheDocument()
    expect(within(successorChain).getByText('project-record-old')).toBeInTheDocument()
    expect(within(successorChain).getByText(/Stale fact: limit is 5/)).toBeInTheDocument()
    const successorRow = within(successorChain).getByText('project-record-old')
      .parentElement as HTMLElement
    expect(successorRow.getAttribute('data-direction')).toBe('from')

    const predecessorItem = document.querySelector(
      `[data-record-id="${predecessor.recordId}"]`,
    ) as HTMLElement
    expect(predecessorItem.getAttribute('data-superseded')).toBe('true')
    const supersedeBtn = within(predecessorItem).getByRole('button', { name: /Supersede/ })
    expect(supersedeBtn).toBeDisabled()
  })

  it('renders the unresolved chain row when the predecessor is no longer in the list', async () => {
    const orphan = makeRecord({
      recordId: 'project-record-orphan',
      title: 'Plan replaces archived fact',
      supersedesId: 'project-record-gone',
    })
    const { adapter } = makeAdapter(makeListResponse([orphan]))

    render(
      <ProjectRecordsSection projectId={PROJECT_ID} projectLabel="xero" adapter={adapter} />,
    )

    const chain = await screen.findByTestId(`supersede-chain-${orphan.recordId}`)
    expect(within(chain).getByText('Supersedes')).toBeInTheDocument()
    expect(within(chain).getByText('project-record-gone')).toBeInTheDocument()
    expect(within(chain).queryByText(/·/)).not.toBeInTheDocument()
  })

  it('does not render a chain block for records with no lineage', async () => {
    const standalone = makeRecord({ recordId: 'project-record-solo', title: 'Solo plan' })
    const { adapter } = makeAdapter(makeListResponse([standalone]))

    render(
      <ProjectRecordsSection projectId={PROJECT_ID} projectLabel="xero" adapter={adapter} />,
    )

    await screen.findByText('Solo plan')
    expect(
      screen.queryByTestId(`supersede-chain-${standalone.recordId}`),
    ).not.toBeInTheDocument()
  })

  it('drives the supersede adapter when the dialog is confirmed', async () => {
    const stale = makeRecord({ recordId: 'project-record-1', title: 'Stale plan' })
    const { adapter, supersedeRecord, listRecords } = makeAdapter(makeListResponse([stale]))
    supersedeRecord.mockResolvedValue({
      schema: 'xero.project_context_record_supersede_command.v1',
      projectId: PROJECT_ID,
      supersededRecordId: stale.recordId,
      supersedingRecordId: 'project-record-2',
      retrievalChanged: true,
      uiDeferred: true,
    })

    render(
      <ProjectRecordsSection projectId={PROJECT_ID} projectLabel="xero" adapter={adapter} />,
    )

    await waitFor(() => expect(listRecords).toHaveBeenCalledTimes(1))
    await screen.findByText('Stale plan')

    fireEvent.click(screen.getByRole('button', { name: /Supersede/ }))
    const input = await screen.findByLabelText('Superseding record id')
    fireEvent.change(input, { target: { value: 'project-record-2' } })

    const confirmButtons = screen.getAllByRole('button', { name: /Supersede/ })
    fireEvent.click(confirmButtons[confirmButtons.length - 1]!)

    await waitFor(() =>
      expect(supersedeRecord).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        supersededRecordId: stale.recordId,
        supersedingRecordId: 'project-record-2',
      }),
    )
    await waitFor(() => expect(listRecords).toHaveBeenCalledTimes(2))
  })

  it('drives the delete adapter and removes the record from the list', async () => {
    const stale = makeRecord({ recordId: 'project-record-1', title: 'Stale plan' })
    const { adapter, deleteRecord } = makeAdapter(makeListResponse([stale]))
    deleteRecord.mockResolvedValue({
      schema: 'xero.project_context_record_delete_command.v1',
      projectId: PROJECT_ID,
      recordId: stale.recordId,
      retrievalRemoved: true,
      uiDeferred: true,
    })

    render(
      <ProjectRecordsSection projectId={PROJECT_ID} projectLabel="xero" adapter={adapter} />,
    )

    await screen.findByText('Stale plan')
    fireEvent.click(screen.getByRole('button', { name: /Delete/ }))
    const confirmButtons = screen.getAllByRole('button', { name: /Delete/ })
    fireEvent.click(confirmButtons[confirmButtons.length - 1]!)

    await waitFor(() =>
      expect(deleteRecord).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        recordId: stale.recordId,
      }),
    )
    await waitFor(() => expect(screen.queryByText('Stale plan')).not.toBeInTheDocument())
  })

  it('hides redacted text but keeps actions usable', async () => {
    const redacted = makeRecord({
      recordId: 'project-record-blocked',
      title: 'Withheld decision',
      summary: null,
      textPreview: null,
      redactionState: 'blocked',
    })
    const { adapter } = makeAdapter(makeListResponse([redacted]))

    render(
      <ProjectRecordsSection projectId={PROJECT_ID} projectLabel="xero" adapter={adapter} />,
    )

    await screen.findByText('Withheld decision')
    expect(screen.getByText(/Content withheld/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Delete/ })).toBeEnabled()
  })
})
