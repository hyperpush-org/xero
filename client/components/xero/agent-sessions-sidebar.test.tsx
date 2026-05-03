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

  it('requests and releases peek from the collapsed strip', () => {
    const onRequestPeek = vi.fn()
    const onReleasePeek = vi.fn()
    const { container } = renderSidebar({
      collapsed: true,
      mode: 'collapsed',
      onPin: vi.fn(),
      onRequestPeek,
      onReleasePeek,
    })

    const sidebar = container.querySelector('aside') as HTMLElement
    const strip = screen.getByRole('button', { name: 'Show sessions sidebar' })
    const chevron = screen.getByRole('button', { name: 'Expand sessions sidebar' })

    expect(chevron).toBeVisible()
    expect(chevron).toHaveClass('opacity-100', 'pointer-events-auto')
    expect(sidebar).toHaveClass('z-40')

    fireEvent.pointerEnter(strip)
    fireEvent.pointerLeave(strip)

    expect(onRequestPeek).toHaveBeenCalledTimes(1)
    expect(onReleasePeek).toHaveBeenCalledTimes(1)
  })

  it('expands the collapsed sessions strip when the chevron is pressed without requesting peek', () => {
    const onPin = vi.fn()
    const onRequestPeek = vi.fn()
    renderSidebar({
      collapsed: true,
      mode: 'collapsed',
      onPin,
      onRequestPeek,
    })

    const chevron = screen.getByRole('button', { name: 'Expand sessions sidebar' })
    fireEvent.pointerEnter(chevron)
    fireEvent.click(chevron)

    expect(onPin).toHaveBeenCalledTimes(1)
    expect(onRequestPeek).not.toHaveBeenCalled()
  })

  it('does not paint-contain the collapsed strip while the peek overlay is open', () => {
    const onRequestPeek = vi.fn()
    const onReleasePeek = vi.fn()
    const { container } = renderSidebar({
      collapsed: true,
      mode: 'collapsed',
      peeking: true,
      onPin: vi.fn(),
      onRequestPeek,
      onReleasePeek,
    })

    const sidebar = container.querySelector('aside') as HTMLElement
    const overlay = Array.from(container.querySelectorAll('aside > div')).find((element) =>
      element.className.includes('shadow-2xl'),
    ) as HTMLElement

    expect(sidebar.style.width).toBe('6px')
    expect(sidebar).toHaveClass('sidebar-peek-island', 'overflow-visible', 'z-40')
    expect(sidebar).not.toHaveClass('sidebar-motion-island')
    expect(screen.getByRole('button', { name: 'Show sessions sidebar' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Main session' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Expand sessions sidebar' })).toHaveClass('z-50')

    fireEvent.pointerEnter(overlay)
    fireEvent.pointerLeave(overlay)

    expect(onRequestPeek).toHaveBeenCalled()
    expect(onReleasePeek).toHaveBeenCalled()
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

  it('renders pane number chips for sessions loaded in non-focused panes', () => {
    const altSession: AgentSessionView = {
      ...sessions[0],
      agentSessionId: 'agent-session-alt',
      title: 'Side session',
      selected: false,
    }

    renderSidebar({
      sessions: [...sessions, altSession],
      // Main session is focused (selectedSessionId), alt session is loaded in pane 2.
      sessionPaneAssignments: {
        'agent-session-main': 1,
        'agent-session-alt': 2,
      },
    })

    // The focused session should NOT show a pane chip.
    expect(screen.queryByLabelText('Loaded in pane 1')).not.toBeInTheDocument()
    // The non-focused loaded session should show a P2 chip.
    expect(screen.getByLabelText('Loaded in pane 2')).toHaveTextContent('P2')
  })
})
