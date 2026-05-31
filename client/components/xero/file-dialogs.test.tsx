/** @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { DeleteFileDialog } from './delete-file-dialog'
import { NewFileDialog } from './new-file-dialog'

describe('file dialogs', () => {
  it('creates a new file from the shared form dialog', async () => {
    const onCreate = vi.fn(async () => undefined)
    const onOpenChange = vi.fn()

    render(
      <NewFileDialog
        open
        onOpenChange={onOpenChange}
        parentPath="/workspace"
        type="file"
        onCreate={onCreate}
      />,
    )

    expect(screen.getByRole('dialog')).toHaveAttribute(
      'data-dialog-variant',
      'form',
    )

    fireEvent.change(screen.getByPlaceholderText('filename.ext'), {
      target: { value: 'notes.md' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Create' }))

    await waitFor(() => expect(onCreate).toHaveBeenCalledWith('notes.md'))
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
  })

  it('keeps delete confirmation destructive while preserving the path preview', () => {
    const onDelete = vi.fn()

    render(
      <DeleteFileDialog
        open
        onOpenChange={vi.fn()}
        path="/workspace/notes.md"
        type="file"
        onDelete={onDelete}
      />,
    )

    expect(screen.getByRole('dialog')).toHaveAttribute(
      'data-dialog-variant',
      'destructive-confirmation',
    )
    expect(screen.getByText('/workspace/notes.md')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))

    expect(onDelete).toHaveBeenCalledTimes(1)
  })
})
