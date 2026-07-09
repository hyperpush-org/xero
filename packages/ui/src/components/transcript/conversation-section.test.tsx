import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { ConversationSection, type ConversationTurn } from './conversation-section'

function renderConversation(visibleTurns: ConversationTurn[]) {
  return render(
    <ConversationSection
      runtimeRun={null}
      visibleTurns={visibleTurns}
      streamIssue={null}
      streamFailure={null}
    />,
  )
}

describe('ConversationSection thinking rendering', () => {
  it('hides markdown HTML comments in standalone thinking turns', () => {
    renderConversation([
      {
        id: 'thinking-1',
        kind: 'thinking',
        sequence: 1,
        text: ['Planning project inspection', '<!---->'].join('\n'),
      },
    ])

    expect(screen.getByText('Planning project inspection')).toBeInTheDocument()
    expect(screen.queryByText(/<!---->/)).not.toBeInTheDocument()
  })

  it('hides markdown HTML comments split out of assistant think tags', () => {
    renderConversation([
      {
        id: 'assistant-1',
        kind: 'message',
        role: 'assistant',
        sequence: 1,
        text: ['<think>', 'Planning README inspection', '<!---->', '</think>', 'Ready.'].join(
          '\n',
        ),
      },
    ])

    expect(screen.getByText('Planning README inspection')).toBeInTheDocument()
    expect(screen.getByText('Ready.')).toBeInTheDocument()
    expect(screen.queryByText(/<!---->/)).not.toBeInTheDocument()
  })

  it('renders headline-only thoughts as a labelled headline list', () => {
    renderConversation([
      {
        id: 'thinking-headline',
        kind: 'thinking',
        sequence: 1,
        text: '**Listing project files for inspection**\n\n<!-- -->\n\n',
      },
    ])

    expect(
      screen.getByText('Listing project files for inspection'),
    ).toBeInTheDocument()
    expect(screen.getByText('headlines')).toBeInTheDocument()
    expect(
      screen.getByRole('list', { name: 'Thought headlines' }),
    ).toBeInTheDocument()
    expect(screen.queryByText(/\*\*/)).not.toBeInTheDocument()
    expect(screen.queryByText(/<!--/)).not.toBeInTheDocument()
  })

  it('lists every headline when merged reasoning contains several', () => {
    renderConversation([
      {
        id: 'thinking-headlines',
        kind: 'thinking',
        sequence: 1,
        text: '**Planning project inspection**\n\n<!-- -->\n\n**Reading README for overview**\n\n<!-- -->\n\n',
      },
    ])

    expect(screen.getByText('Planning project inspection')).toBeInTheDocument()
    expect(screen.getByText('Reading README for overview')).toBeInTheDocument()
  })

  it('keeps markdown rendering for thoughts with prose bodies', () => {
    renderConversation([
      {
        id: 'thinking-prose',
        kind: 'thinking',
        sequence: 1,
        text: '**Inspecting repository requirements**\n\nI need to focus on the manifest first.',
      },
    ])

    expect(
      screen.getByText('I need to focus on the manifest first.'),
    ).toBeInTheDocument()
    expect(screen.queryByText('headlines')).not.toBeInTheDocument()
    expect(
      screen.queryByRole('list', { name: 'Thought headlines' }),
    ).not.toBeInTheDocument()
  })
})
