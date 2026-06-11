import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  StartTargetsEditor,
  type StartTargetsModelOption,
} from '@/components/xero/start-targets-editor'
import type { StartTargetDto } from '@/src/lib/xero-desktop'

function buildSuggestRequest() {
  return {
    modelId: 'gpt-test',
    providerId: 'openai_codex',
    providerProfileId: 'profile-1',
    runtimeAgentId: null,
    thinkingEffort: null,
  }
}

const modelOptions: StartTargetsModelOption[] = [
  {
    selectionKey: 'xai:grok-4.3-latest',
    providerId: 'xai',
    providerProfileId: 'xai-default',
    providerLabel: 'xAI / Grok',
    modelId: 'grok-4.3-latest',
    label: 'Grok 4.3',
    thinkingEffortOptions: ['medium', 'high'],
    defaultThinkingEffort: 'medium',
  },
  {
    selectionKey: 'openai_codex:gpt-5.4',
    providerId: 'openai_codex',
    providerProfileId: 'openai_codex-default',
    providerLabel: 'OpenAI Codex',
    modelId: 'gpt-5.4',
    label: 'GPT-5.4',
    thinkingEffortOptions: ['medium', 'high'],
    defaultThinkingEffort: 'high',
  },
]

function ensurePointerCaptureApi() {
  for (const [name, value] of [
    ['hasPointerCapture', () => false],
    ['setPointerCapture', () => undefined],
    ['releasePointerCapture', () => undefined],
  ] as const) {
    if (!(name in HTMLElement.prototype)) {
      Object.defineProperty(HTMLElement.prototype, name, {
        configurable: true,
        value,
      })
    }
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
    fireEvent.click(screen.getByRole('switch', { name: 'Target 1 browser supported' }))

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    expect(onSave).toHaveBeenCalledWith([
      {
        id: 'tgt-existing',
        name: 'web',
        command: 'pnpm dev --port 3000',
        browserSupported: true,
      },
    ])
  })

  it('replaces empty rows wholesale with AI suggestion', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onSuggest = vi.fn().mockResolvedValue({
      targets: [
        { name: 'web', command: 'pnpm dev', browserSupported: true },
        { name: 'api', command: 'cargo run', browserSupported: false },
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
    expect(screen.getByRole('switch', { name: 'Target 1 browser supported' })).toBeChecked()
    expect(screen.getByRole('switch', { name: 'Target 2 browser supported' })).not.toBeChecked()
  })

  it('shows the AI model and sends the selected model/provider route and thinking level', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onSuggest = vi.fn().mockResolvedValue({
      targets: [{ name: 'web', command: 'pnpm dev' }],
    })

    render(
      <StartTargetsEditor
        initialTargets={[]}
        onSave={onSave}
        resolveSuggestRequest={() => ({
          modelId: 'grok-4.3-latest',
          providerId: 'xai',
          providerProfileId: 'xai-default',
          runtimeAgentId: 'ask',
          thinkingEffort: 'medium',
        })}
        onSuggest={onSuggest}
        modelOptions={[...modelOptions]}
      />,
    )

    expect(screen.getByText('AI model')).toBeInTheDocument()
    expect(
      screen.getByRole('combobox', { name: 'AI suggestion model' }),
    ).toHaveTextContent('Grok 4.3')

    ensurePointerCaptureApi()
    fireEvent.pointerDown(screen.getByRole('combobox', { name: 'AI suggestion model' }), {
      button: 0,
      pointerId: 1,
      pointerType: 'mouse',
    })
    fireEvent.click(await screen.findByRole('option', { name: 'GPT-5.4' }))
    const thinkingItem = screen.getByRole('menuitem', { name: /Thinking/i })
    fireEvent.keyDown(thinkingItem, { key: 'ArrowRight' })
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'High' }))

    fireEvent.click(screen.getByRole('button', { name: /Suggest with AI/i }))

    await waitFor(() => expect(onSuggest).toHaveBeenCalledTimes(1))
    expect(onSuggest).toHaveBeenCalledWith({
      modelId: 'gpt-5.4',
      providerId: 'openai_codex',
      providerProfileId: 'openai_codex-default',
      runtimeAgentId: 'ask',
      thinkingEffort: 'high',
    })
  })

  it('can hide the AI model selector while still using the resolved fallback request', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onSuggest = vi.fn().mockResolvedValue({
      targets: [{ name: 'web', command: 'pnpm dev' }],
    })
    const fallbackRequest = {
      modelId: 'grok-4.3-latest',
      providerId: 'xai',
      providerProfileId: 'xai-default',
      runtimeAgentId: 'ask',
      thinkingEffort: 'low',
    } as const

    render(
      <StartTargetsEditor
        initialTargets={[]}
        onSave={onSave}
        resolveSuggestRequest={() => fallbackRequest}
        onSuggest={onSuggest}
        modelOptions={[...modelOptions]}
        showModelSelector={false}
      />,
    )

    expect(screen.queryByText('AI model')).not.toBeInTheDocument()
    expect(
      screen.queryByRole('combobox', { name: 'AI suggestion model' }),
    ).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Suggest with AI/i }))

    await waitFor(() => expect(onSuggest).toHaveBeenCalledWith(fallbackRequest))
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
