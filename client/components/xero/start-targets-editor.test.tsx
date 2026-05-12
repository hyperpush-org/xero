import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { StartTargetsEditor } from '@/components/xero/start-targets-editor'
import type { StartTargetDto } from '@/src/lib/xero-desktop'

function buildSuggestRequest() {
  return {
    modelId: 'gpt-test',
    providerProfileId: 'profile-1',
    runtimeAgentId: null,
    thinkingEffort: null,
  }
}

describe('StartTargetsEditor', () => {
  it('rejects an empty target name on save', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<StartTargetsEditor initialTargets={[]} onSave={onSave} />)

    const commandInput = screen.getByLabelText('Target 1 command') as HTMLInputElement
    fireEvent.change(commandInput, { target: { value: 'pnpm dev' } })

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText(/Every target needs a name\./i)).toBeInTheDocument()
    expect(onSave).not.toHaveBeenCalled()
  })

  it('rejects duplicate target names on save', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<StartTargetsEditor initialTargets={[]} onSave={onSave} />)

    const nameInput = screen.getByLabelText('Target 1 name') as HTMLInputElement
    const commandInput = screen.getByLabelText('Target 1 command') as HTMLInputElement
    fireEvent.change(nameInput, { target: { value: 'web' } })
    fireEvent.change(commandInput, { target: { value: 'pnpm dev' } })

    fireEvent.click(screen.getByRole('button', { name: /Add target/i }))

    const name2 = screen.getByLabelText('Target 2 name') as HTMLInputElement
    const command2 = screen.getByLabelText('Target 2 command') as HTMLInputElement
    fireEvent.change(name2, { target: { value: 'WEB' } })
    fireEvent.change(command2, { target: { value: 'echo' } })

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(/Target names must be unique\./i),
    ).toBeInTheDocument()
    expect(onSave).not.toHaveBeenCalled()
  })

  it('submits trimmed payload and keeps ids for existing rows', async () => {
    const initial: StartTargetDto[] = [
      { id: 'tgt-existing', name: 'web', command: 'pnpm dev' },
    ]
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<StartTargetsEditor initialTargets={initial} onSave={onSave} />)

    const command = screen.getByLabelText('Target 1 command') as HTMLInputElement
    fireEvent.change(command, { target: { value: '  pnpm dev --port 3000  ' } })

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    expect(onSave).toHaveBeenCalledWith([
      { id: 'tgt-existing', name: 'web', command: 'pnpm dev --port 3000' },
    ])
  })

  it('replaces empty rows wholesale with AI suggestion', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onSuggest = vi.fn().mockResolvedValue({
      targets: [
        { name: 'web', command: 'pnpm dev' },
        { name: 'api', command: 'cargo run' },
      ],
    })

    render(
      <StartTargetsEditor
        initialTargets={[]}
        onSave={onSave}
        resolveSuggestRequest={buildSuggestRequest}
        onSuggest={onSuggest}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Suggest with AI/i }))

    await waitFor(() => {
      const name1 = screen.getByLabelText('Target 1 name') as HTMLInputElement
      expect(name1.value).toBe('web')
    })
    const name2 = screen.getByLabelText('Target 2 name') as HTMLInputElement
    expect(name2.value).toBe('api')
  })

  it('asks before replacing user-entered rows with AI suggestion', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onSuggest = vi.fn().mockResolvedValue({
      targets: [{ name: 'web', command: 'pnpm dev' }],
    })

    render(
      <StartTargetsEditor
        initialTargets={[]}
        onSave={onSave}
        resolveSuggestRequest={buildSuggestRequest}
        onSuggest={onSuggest}
      />,
    )

    const nameInput = screen.getByLabelText('Target 1 name') as HTMLInputElement
    const commandInput = screen.getByLabelText('Target 1 command') as HTMLInputElement
    fireEvent.change(nameInput, { target: { value: 'mine' } })
    fireEvent.change(commandInput, { target: { value: 'echo' } })

    fireEvent.click(screen.getByRole('button', { name: /Suggest with AI/i }))

    expect(
      await screen.findByText(/Replace current targets\?/i),
    ).toBeInTheDocument()

    // Cancel keeps existing rows
    fireEvent.click(screen.getByRole('button', { name: /Keep current/i }))

    const keptName = screen.getByLabelText('Target 1 name') as HTMLInputElement
    expect(keptName.value).toBe('mine')
  })
})
