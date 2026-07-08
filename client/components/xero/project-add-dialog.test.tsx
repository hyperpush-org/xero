import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ProjectAddDialog } from './project-add-dialog'

describe('ProjectAddDialog', () => {
  it('keeps the dialog open and surfaces import errors when opening an existing folder fails', async () => {
    const onOpenChange = vi.fn()
    const onSelectExisting = vi.fn(async () => false)
    const baseProps = {
      open: true,
      onOpenChange,
      isImporting: false,
      onSelectExisting,
      onPickParentFolder: vi.fn(async () => null),
      onCreate: vi.fn(async () => false),
    }

    const { rerender } = render(<ProjectAddDialog {...baseProps} />)

    expect(screen.getByText('Pick any folder to use as a project.')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /open existing/i }))

    await waitFor(() => {
      expect(onSelectExisting).toHaveBeenCalled()
    })
    expect(onOpenChange).not.toHaveBeenCalledWith(false)

    rerender(
      <ProjectAddDialog
        {...baseProps}
        errorMessage="Selected folder could not be opened."
      />,
    )

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Selected folder could not be opened.',
    )
  })
})
