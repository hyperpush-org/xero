import { fireEvent, render, screen } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { describe, expect, it, vi } from 'vitest'

import {
  ActionPromptCard,
  ActionPromptDispatchProvider,
  type ActionPromptDispatchValue,
} from './action-prompt-card'

function renderPrompt(
  overrides: Partial<ComponentProps<typeof ActionPromptCard>> = {},
  dispatchOverrides: Partial<ActionPromptDispatchValue> = {},
) {
  const resolveActionPrompt = vi.fn()
  const dispatch: ActionPromptDispatchValue = {
    pendingActionId: null,
    pendingDecision: null,
    isResolving: false,
    resolveActionPrompt,
    ...dispatchOverrides,
  }

  render(
    <ActionPromptDispatchProvider value={dispatch}>
      <ActionPromptCard
        actionId="question-1"
        actionType="short_text_required"
        title="Name the plan"
        detail="Choose a concise title."
        shape="short_text"
        options={null}
        allowMultiple={false}
        {...overrides}
      />
    </ActionPromptDispatchProvider>,
  )

  return { resolveActionPrompt }
}

describe('ActionPromptCard', () => {
  it('requires non-empty structured text answers before approval', () => {
    const { resolveActionPrompt } = renderPrompt()
    const submit = screen.getByRole('button', { name: 'Approve' })

    expect(submit).toBeDisabled()

    fireEvent.change(screen.getByLabelText('Operator response'), {
      target: { value: 'Plan the runtime handoff' },
    })
    fireEvent.click(submit)

    expect(resolveActionPrompt).toHaveBeenCalledWith('question-1', 'approve', {
      actionType: 'short_text_required',
      runId: null,
      userAnswer: 'Plan the runtime handoff',
    })
  })

  it('renders the matching action error inline', () => {
    renderPrompt(
      {},
      {
        actionError: {
          actionId: 'question-1',
          message: 'Xero could not resolve this action.',
        },
      },
    )

    expect(screen.getByText('Xero could not resolve this action.')).toBeInTheDocument()
  })

  it('does not render errors for other action cards', () => {
    renderPrompt(
      {},
      {
        actionError: {
          actionId: 'question-2',
          message: 'This belongs to another prompt.',
        },
      },
    )

    expect(screen.queryByText('This belongs to another prompt.')).not.toBeInTheDocument()
  })

  it('renders compact single-choice prompts as command buttons without stale radio state', () => {
    const { resolveActionPrompt } = renderPrompt({
      actionType: 'single_choice_required',
      shape: 'single_choice',
      options: [
        { id: 'small', label: 'Small', description: null },
        { id: 'large', label: 'Large', description: 'Broader scope.' },
      ],
    })

    expect(screen.queryByRole('radio')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Large/ }))

    expect(resolveActionPrompt).toHaveBeenCalledWith('question-1', 'approve', {
      actionType: 'single_choice_required',
      runId: null,
      userAnswer: 'large',
    })
  })

  it('submits multi-choice prompts as a JSON array of selected option ids', () => {
    const { resolveActionPrompt } = renderPrompt({
      actionType: 'user_input_required',
      shape: 'multi_choice',
      options: [
        { id: 'tailwind', label: 'Tailwind', description: null },
        { id: 'shadcn', label: 'ShadCN', description: 'Use component primitives.' },
        { id: 'framer', label: 'Framer Motion', description: null },
      ],
      allowMultiple: true,
    })

    const submit = screen.getByRole('button', { name: 'Submit' })
    expect(submit).toBeDisabled()

    fireEvent.click(screen.getByText('Tailwind'))
    fireEvent.click(screen.getByText('ShadCN'))
    fireEvent.click(submit)

    expect(resolveActionPrompt).toHaveBeenCalledWith('question-1', 'approve', {
      actionType: 'user_input_required',
      runId: null,
      userAnswer: JSON.stringify(['tailwind', 'shadcn']),
    })
  })

  it('keeps sensitive values hidden by default and submits only entered fields', () => {
    const { resolveActionPrompt } = renderPrompt({
      actionType: 'sensitive_input_request',
      shape: 'sensitive_fields',
      detail: 'Need local API credentials.',
      intendedUse: 'Write the provided key into .env.local.',
      sensitiveFields: [
        {
          key: 'api_key',
          label: 'API key',
          description: 'Used only for local setup.',
          required: true,
          validationHint: 'Starts with sk-',
        },
        {
          key: 'webhook_secret',
          label: 'Webhook secret',
          description: null,
          required: false,
          validationHint: null,
        },
      ],
    })

    const approve = screen.getByRole('button', { name: 'Approve' })
    const apiKey = screen.getByLabelText('API key')

    expect(apiKey).toHaveAttribute('type', 'password')
    expect(approve).toBeDisabled()

    fireEvent.change(apiKey, { target: { value: 'sk-live-secret-value' } })
    fireEvent.click(screen.getByRole('button', { name: 'Reveal API key' }))
    expect(apiKey).toHaveAttribute('type', 'text')

    fireEvent.click(approve)

    expect(resolveActionPrompt).toHaveBeenCalledWith('question-1', 'approve', {
      actionType: 'sensitive_input_request',
      runId: null,
      userAnswer: JSON.stringify({ api_key: 'sk-live-secret-value' }),
    })
  })

  it('passes run context through prompt decisions', () => {
    const { resolveActionPrompt } = renderPrompt({
      runId: 'run-owned',
      actionType: 'safety_boundary',
      shape: 'plain_text',
    })

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }))

    expect(resolveActionPrompt).toHaveBeenCalledWith('question-1', 'approve', {
      actionType: 'safety_boundary',
      runId: 'run-owned',
      userAnswer: '',
    })
  })
})
