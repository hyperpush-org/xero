/** @vitest-environment jsdom */

import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { BaseAlertDialog, BaseDialog } from './base-dialog'

describe('BaseDialog', () => {
  it('renders shared dialog chrome with action loading and disabled states', () => {
    render(
      <BaseDialog
        open
        onOpenChange={vi.fn()}
        variant="form"
        title="Edit project"
        description="Update the project metadata."
        actions={[
          { label: 'Cancel', variant: 'outline', disabled: true },
          { label: 'Save', loading: true, loadingLabel: 'Saving...' },
        ]}
      >
        <p>Project form body</p>
      </BaseDialog>,
    )

    expect(screen.getByRole('dialog')).toHaveAttribute(
      'data-dialog-variant',
      'form',
    )
    expect(screen.getByRole('heading', { name: 'Edit project' })).toBeVisible()
    expect(screen.getByText('Update the project metadata.')).toBeVisible()
    expect(screen.getByText('Project form body')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Saving...' })).toBeDisabled()
  })

  it('supports controlled close actions', () => {
    const onOpenChange = vi.fn()

    render(
      <BaseDialog
        open
        onOpenChange={onOpenChange}
        title="Info"
        actions={[{ label: 'Done', close: true }]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Done' }))

    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})

describe('BaseAlertDialog', () => {
  it('keeps destructive confirmation actions behind the safe cancel action', () => {
    render(
      <BaseAlertDialog
        open
        onOpenChange={vi.fn()}
        variant="destructive-confirmation"
        title="Delete project?"
        description="This cannot be undone."
        cancelAction={{ label: 'Cancel' }}
        action={{ label: 'Delete', loading: true, loadingLabel: 'Deleting...' }}
      />,
    )

    const buttons = screen.getAllByRole('button')
    expect(buttons.map((button) => button.textContent)).toEqual([
      'Cancel',
      'Deleting...',
    ])
    expect(screen.getByRole('button', { name: 'Deleting...' })).toBeDisabled()
  })
})
