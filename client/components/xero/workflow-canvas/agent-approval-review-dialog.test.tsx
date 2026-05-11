import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { AgentApprovalReviewDialog } from '@/components/xero/workflow-canvas/agent-approval-review-dialog'
import type { AgentDefinitionPreSaveReviewDto } from '@/src/lib/xero-model/agent-definition'

function buildReview(
  overrides: Partial<AgentDefinitionPreSaveReviewDto> = {},
): AgentDefinitionPreSaveReviewDto {
  return {
    schema: 'xero.agent_definition_pre_save_review.v1',
    definitionId: 'release_notes_helper',
    isInitialVersion: false,
    fromVersion: 4,
    toVersion: 5,
    fromCreatedAt: '2026-05-01T12:00:00Z',
    toCreatedAt: '2026-05-02T09:30:00Z',
    changed: true,
    changedSections: ['toolPolicy', 'memoryPolicy'],
    sections: [
      {
        section: 'identity',
        changed: false,
        fields: ['displayName'],
        before: { displayName: 'Release Helper' },
        after: { displayName: 'Release Helper' },
      },
      {
        section: 'toolPolicy',
        changed: true,
        fields: ['toolPolicy'],
        before: { toolPolicy: { allowedTools: ['read'] } },
        after: { toolPolicy: { allowedTools: ['read', 'write'] } },
      },
      {
        section: 'memoryPolicy',
        changed: true,
        fields: ['memoryPolicy'],
        before: { memoryPolicy: { reviewRequired: false } },
        after: { memoryPolicy: { reviewRequired: true } },
      },
    ],
    ...overrides,
  }
}

describe('AgentApprovalReviewDialog', () => {
  it('renders the structured diff for an update with changed sections', () => {
    render(
      <AgentApprovalReviewDialog
        open
        review={buildReview()}
        busy={false}
        errorMessage={null}
        onApprove={() => {}}
        onCancel={() => {}}
      />,
    )

    expect(
      screen.getByRole('heading', { name: /Approve update to this agent/i }),
    ).toBeInTheDocument()
    expect(screen.getByText('release_notes_helper')).toBeInTheDocument()
    expect(screen.getByText(/v4/)).toBeInTheDocument()
    expect(screen.getByText(/v5/)).toBeInTheDocument()
    expect(screen.getByText(/2 sections changed/)).toBeInTheDocument()

    const toolPolicy = screen.getByText('Tool policy').closest('li')
    expect(toolPolicy).not.toBeNull()
    expect(within(toolPolicy as HTMLElement).getByText('changed')).toBeInTheDocument()
    expect(
      within(toolPolicy as HTMLElement).getByText('Before'),
    ).toBeInTheDocument()
    expect(
      within(toolPolicy as HTMLElement).getByText('After'),
    ).toBeInTheDocument()

    const identity = screen.getByText('Identity').closest('li')
    expect(within(identity as HTMLElement).getByText('unchanged')).toBeInTheDocument()
    expect(
      within(identity as HTMLElement).queryByText('Before'),
    ).not.toBeInTheDocument()
  })

  it('shows the zero-delta notice and uses the no-change confirm label', () => {
    const onApprove = vi.fn()
    const review = buildReview({
      changed: false,
      changedSections: [],
      sections: [
        {
          section: 'identity',
          changed: false,
          fields: ['displayName'],
          before: { displayName: 'Release Helper' },
          after: { displayName: 'Release Helper' },
        },
      ],
    })

    render(
      <AgentApprovalReviewDialog
        open
        review={review}
        busy={false}
        errorMessage={null}
        onApprove={onApprove}
        onCancel={() => {}}
      />,
    )

    expect(screen.getByText('No changes detected')).toBeInTheDocument()
    expect(screen.getAllByText(/No changes/).length).toBeGreaterThanOrEqual(1)
    const approve = screen.getByRole('button', { name: /Approve no-change save/i })
    expect(approve).toBeEnabled()
    fireEvent.click(approve)
    expect(onApprove).toHaveBeenCalledTimes(1)
  })

  it('renders an initial-version review with single-pane "added" cells', () => {
    const review = buildReview({
      isInitialVersion: true,
      fromVersion: null,
      fromCreatedAt: null,
      changed: true,
      changedSections: ['identity', 'toolPolicy'],
      sections: [
        {
          section: 'identity',
          changed: true,
          fields: ['displayName'],
          before: { displayName: null },
          after: { displayName: 'Release Helper' },
        },
        {
          section: 'toolPolicy',
          changed: true,
          fields: ['toolPolicy'],
          before: { toolPolicy: null },
          after: { toolPolicy: { allowedTools: ['read'] } },
        },
      ],
    })

    render(
      <AgentApprovalReviewDialog
        open
        review={review}
        busy={false}
        errorMessage={null}
        onApprove={() => {}}
        onCancel={() => {}}
      />,
    )

    expect(
      screen.getByRole('heading', { name: /Approve new agent definition/i }),
    ).toBeInTheDocument()
    const identity = screen.getByText('Identity').closest('li')
    expect(within(identity as HTMLElement).getByText('added')).toBeInTheDocument()
    // Initial-version rows show only the "After" pane — no Before pane.
    expect(
      within(identity as HTMLElement).queryByText('Before'),
    ).not.toBeInTheDocument()
    expect(
      within(identity as HTMLElement).getByText('After'),
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: /Approve and save$/i }),
    ).toBeInTheDocument()
  })

  it('disables the approve button while busy and surfaces error messages', () => {
    render(
      <AgentApprovalReviewDialog
        open
        review={buildReview()}
        busy
        errorMessage="Validation failed on save."
        onApprove={() => {}}
        onCancel={() => {}}
      />,
    )

    expect(
      screen.getByRole('button', { name: /Approve and save changes/i }),
    ).toBeDisabled()
    expect(screen.getByRole('button', { name: /Cancel/i })).toBeDisabled()
    expect(screen.getByText('Validation failed on save.')).toBeInTheDocument()
  })

  it('triggers cancel when the user clicks Cancel', () => {
    const onCancel = vi.fn()
    render(
      <AgentApprovalReviewDialog
        open
        review={buildReview()}
        busy={false}
        errorMessage={null}
        onApprove={() => {}}
        onCancel={onCancel}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Cancel/i }))
    expect(onCancel).toHaveBeenCalledTimes(1)
  })
})
