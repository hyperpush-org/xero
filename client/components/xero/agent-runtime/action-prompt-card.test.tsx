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
      userAnswer: 'Plan the runtime handoff',
    })
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
      userAnswer: 'large',
    })
  })
})
