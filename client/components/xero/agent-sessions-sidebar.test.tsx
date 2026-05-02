import type { ComponentProps } from 'react'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { AgentSessionsSidebar } from './agent-sessions-sidebar'
import type { AgentSessionView } from '@/src/lib/xero-model'
import type { SessionTranscriptSearchResultSnippetDto } from '@/src/lib/xero-model/session-context'

const sessions: AgentSessionView[] = [
  {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    title: 'Main session',
    summary: 'Primary project session',
    status: 'active',
    statusLabel: 'Active',
    selected: true,
    createdAt: '2026-04-15T20:00:00Z',
    updatedAt: '2026-04-15T20:00:00Z',
    archivedAt: null,
    lastRunId: null,
    lastRuntimeKind: null,
    lastProviderId: null,
    lineage: null,
    isActive: true,
    isArchived: false,
  },
]

function renderSidebar(overrides: Partial<ComponentProps<typeof AgentSessionsSidebar>> = {}) {
  return render(
    <AgentSessionsSidebar
      projectId="project-1"
      sessions={sessions}
      selectedSessionId="agent-session-main"
      onSelectSession={vi.fn()}
      onCreateSession={vi.fn()}
      onArchiveSession={vi.fn()}
      onOpenArchivedSessions={vi.fn()}
      {...overrides}
    />,
  )
}

const searchResult: SessionTranscriptSearchResultSnippetDto = {
  contractVersion: 1,
  resultId: 'item:run-history-1:message:1',
  projectId: 'project-1',
  agentSessionId: 'agent-session-main',
  runId: 'run-history-1',
  itemId: 'message:1',
  archived: false,
  rank: 0,
  matchedFields: ['text'],
  snippet: 'Matched validation transcript item.',
  redaction: {
    redactionClass: 'public',
    redacted: false,
    reason: null,
  },
}

describe('AgentSessionsSidebar', () => {
  afterEach(() => {
    vi.useRealTimers()
    window.localStorage.clear()
  })

  it('resizes from the separator and persists the width', () => {
    renderSidebar()

    expect(screen.getByText('Main session')).toBeVisible()
    expect(screen.queryByText('Xero')).not.toBeInTheDocument()

    const separator = screen.getByRole('separator', { name: 'Resize sessions sidebar' })
    const before = Number(separator.getAttribute('aria-valuenow'))

    fireEvent.keyDown(separator, { key: 'ArrowRight' })

    const after = Number(separator.getAttribute('aria-valuenow'))
    expect(after).toBeGreaterThan(before)
    expect(window.localStorage.getItem('xero.agentSessions.width')).toBe(String(after))
  })

  it('previews session selection on pointer down before the click handler settles', () => {
    const onSelectSession = vi.fn()
    const altSession = {
      ...sessions[0],
      agentSessionId: 'agent-session-alt',
      title: 'Alt session',
      selected: false,
    }

    renderSidebar({
      sessions: [...sessions, altSession],
      onSelectSession,
    })

    const altButton = screen.getByRole('button', { name: 'Alt session' })

    fireEvent.pointerDown(altButton, { button: 0 })

    expect(altButton).toHaveClass('bg-primary/[0.08]')
    expect(onSelectSession).not.toHaveBeenCalled()

    fireEvent.click(altButton)

    expect(onSelectSession).toHaveBeenCalledWith('agent-session-alt')
  })

  it('allows archiving the only active session', async () => {
    const onArchiveSession = vi.fn()
    renderSidebar({ onArchiveSession })

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Session actions for Main session' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Archive' }))

    expect(onArchiveSession).toHaveBeenCalledWith('agent-session-main')
  })

  it('keeps the resize control hidden while collapsed', () => {
    const { container } = renderSidebar({ collapsed: true })

    const sidebar = container.querySelector('aside') as HTMLElement
    expect(sidebar.style.width).toBe('0px')
    expect(sidebar).toHaveAttribute('aria-hidden', 'true')
    expect(screen.queryByRole('separator', { name: 'Resize sessions sidebar' })).not.toBeInTheDocument()
  })

  it('validates rename input and recovers after a failed rename', async () => {
    const onRenameSession = vi
      .fn<NonNullable<ComponentProps<typeof AgentSessionsSidebar>['onRenameSession']>>()
      .mockRejectedValueOnce(new Error('Session name already exists.'))
      .mockResolvedValueOnce(undefined)

    renderSidebar({ onRenameSession })

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Session actions for Main session' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Rename' }))

    const input = screen.getByRole('textbox', { name: 'Name' })
    fireEvent.change(input, { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }))
    expect(await screen.findByText('Enter a session name.')).toBeVisible()
    expect(onRenameSession).not.toHaveBeenCalled()

    fireEvent.change(input, { target: { value: 'Investigation notes' } })
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }))
    expect(await screen.findByText('Session name already exists.')).toBeVisible()
    expect(onRenameSession).toHaveBeenCalledWith('agent-session-main', 'Investigation notes')

    fireEvent.click(screen.getByRole('button', { name: 'Rename' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(onRenameSession).toHaveBeenCalledTimes(2)
  })

  it('debounces transcript search and opens the selected result', async () => {
    const onSearchSessions = vi.fn(async () => [searchResult])
    const onOpenSearchResult = vi.fn()

    renderSidebar({ onSearchSessions, onOpenSearchResult })

    const searchToggle = screen.getByRole('button', { name: 'Search sessions' })
    expect(searchToggle).toHaveAttribute('aria-pressed', 'false')
    expect(screen.queryByRole('searchbox', { name: 'Search sessions' })).not.toBeInTheDocument()

    fireEvent.click(searchToggle)
    expect(searchToggle).toHaveAttribute('aria-pressed', 'true')

    const searchbox = screen.getByRole('searchbox', { name: 'Search sessions' })
    fireEvent.change(searchbox, {
      target: { value: 'validation' },
    })

    await waitFor(() => expect(onSearchSessions).toHaveBeenCalledWith('validation'))
    fireEvent.click(await screen.findByText('Matched validation transcript item.'))

    expect(onOpenSearchResult).toHaveBeenCalledWith(searchResult)
    expect(screen.getByRole('searchbox', { name: 'Search sessions' })).toHaveValue('')
  })

  it('clears and hides transcript search when toggled closed', async () => {
    const onSearchSessions = vi.fn(async () => [searchResult])

    renderSidebar({ onSearchSessions })

    const searchToggle = screen.getByRole('button', { name: 'Search sessions' })
    fireEvent.click(searchToggle)
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search sessions' }), {
      target: { value: 'validation' },
    })

    await waitFor(() => expect(onSearchSessions).toHaveBeenCalledWith('validation'))
    fireEvent.click(searchToggle)

    expect(screen.queryByRole('searchbox', { name: 'Search sessions' })).not.toBeInTheDocument()

    fireEvent.click(searchToggle)
    expect(screen.getByRole('searchbox', { name: 'Search sessions' })).toHaveValue('')
  })
})
