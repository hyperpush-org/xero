import type { ComponentProps } from 'react'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { DndContext } from '@dnd-kit/core'

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
    remoteVisible: false,
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

  it('exposes session rows as draggable when wrapped in a DndContext, while preserving click-to-select', () => {
    const onSelectSession = vi.fn()
    render(
      <DndContext>
        <AgentSessionsSidebar
          projectId="project-1"
          sessions={sessions}
          selectedSessionId="agent-session-main"
          onSelectSession={onSelectSession}
          onCreateSession={vi.fn()}
          onArchiveSession={vi.fn()}
        />
      </DndContext>,
    )

    const sessionButton = screen.getByRole('button', { name: 'Main session' })
    const draggableWrapper = sessionButton.closest('[aria-roledescription="draggable"]')
    expect(draggableWrapper).not.toBeNull()

    fireEvent.click(sessionButton)
    expect(onSelectSession).toHaveBeenCalledWith('agent-session-main')
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

  it('requires inline confirmation before archiving the only active session from the card', () => {
    const onArchiveSession = vi.fn()
    const onSelectSession = vi.fn()
    renderSidebar({ onArchiveSession, onSelectSession })

    fireEvent.click(screen.getByRole('button', { name: 'Archive Main session' }))

    expect(onArchiveSession).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Confirm archive Main session' })).toHaveTextContent('Archive')

    fireEvent.click(screen.getByRole('button', { name: 'Confirm archive Main session' }))

    expect(onArchiveSession).toHaveBeenCalledWith('agent-session-main')
    expect(onSelectSession).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: 'Session actions for Main session' })).not.toBeInTheDocument()
    expect(screen.queryByText('Rename')).not.toBeInTheDocument()
  })

  it('uses a faster exit animation when an active session leaves the list', async () => {
    const props: ComponentProps<typeof AgentSessionsSidebar> = {
      projectId: 'project-1',
      sessions,
      selectedSessionId: 'agent-session-main',
      onSelectSession: vi.fn(),
      onCreateSession: vi.fn(),
      onArchiveSession: vi.fn(),
    }
    const { rerender } = render(<AgentSessionsSidebar {...props} />)
    const row = screen.getByText('Main session').closest('li') as HTMLElement

    rerender(<AgentSessionsSidebar {...props} sessions={[]} />)

    await waitFor(() => expect(row).toHaveClass('animate-out', 'duration-150'))
    expect(row).not.toHaveClass('duration-300')

    const exitingRow = screen.getByText('Main session').closest('li') as HTMLElement
    fireEvent.animationEnd(exitingRow)

    await waitFor(() => expect(screen.queryByText('Main session')).not.toBeInTheDocument())
  })

  it('clears archive confirmation when the cursor leaves the archive button', () => {
    const onArchiveSession = vi.fn()
    renderSidebar({ onArchiveSession })

    fireEvent.click(screen.getByRole('button', { name: 'Archive Main session' }))
    const confirmButton = screen.getByRole('button', { name: 'Confirm archive Main session' })

    fireEvent.pointerLeave(confirmButton)

    expect(screen.queryByRole('button', { name: 'Confirm archive Main session' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Archive Main session' }))

    expect(onArchiveSession).not.toHaveBeenCalled()
  })

  it('pins and unpins a session from the card without selecting it', () => {
    const onSelectSession = vi.fn()
    renderSidebar({ onSelectSession })

    fireEvent.click(screen.getByRole('button', { name: 'Pin Main session' }))

    expect(onSelectSession).not.toHaveBeenCalled()
    expect(window.localStorage.getItem('xero:pinned-sessions:project-1')).toBe(
      JSON.stringify(['agent-session-main']),
    )
    expect(screen.getByText('Pinned')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Unpin Main session' }))

    expect(onSelectSession).not.toHaveBeenCalled()
    expect(window.localStorage.getItem('xero:pinned-sessions:project-1')).toBe(
      JSON.stringify([]),
    )
  })

  it('uses project UI state for pinned sessions when app-data storage is available', async () => {
    const onReadProjectUiState = vi.fn(async () => ['agent-session-main'])
    const onWriteProjectUiState = vi.fn(async () => undefined)

    renderSidebar({ onReadProjectUiState, onWriteProjectUiState })

    await waitFor(() =>
      expect(onReadProjectUiState).toHaveBeenCalledWith('agent-sessions.pinned.v1'),
    )
    expect(await screen.findByText('Pinned')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Unpin Main session' }))

    await waitFor(() =>
      expect(onWriteProjectUiState).toHaveBeenCalledWith(
        'agent-sessions.pinned.v1',
        [],
      ),
    )
    expect(window.localStorage.getItem('xero:pinned-sessions:project-1')).toBeNull()
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

  it('snaps the sessions strip closed while animating a compositor collapse ghost', () => {
    const props: ComponentProps<typeof AgentSessionsSidebar> = {
      projectId: 'project-1',
      sessions,
      selectedSessionId: 'agent-session-main',
      onSelectSession: vi.fn(),
      onCreateSession: vi.fn(),
      onArchiveSession: vi.fn(),
    }
    const { container, rerender } = render(<AgentSessionsSidebar {...props} />)

    rerender(
      <AgentSessionsSidebar
        {...props}
        collapsed
        mode="collapsed"
        onPin={vi.fn()}
      />,
    )

    const sidebar = container.querySelector('aside') as HTMLElement
    expect(sidebar.style.width).toBe('6px')
    expect(sidebar.style.transition).toBe('none')
    expect(container.querySelector('[data-session-collapse-ghost="true"]')).toBeInTheDocument()
  })

  it('does not animate a collapse ghost when an already-collapsed hidden sidebar enters strip mode', () => {
    const props: ComponentProps<typeof AgentSessionsSidebar> = {
      projectId: 'project-1',
      sessions,
      selectedSessionId: 'agent-session-main',
      onSelectSession: vi.fn(),
      onCreateSession: vi.fn(),
      onArchiveSession: vi.fn(),
      collapsed: true,
      mode: 'pinned',
    }
    const { container, rerender } = render(<AgentSessionsSidebar {...props} />)

    rerender(
      <AgentSessionsSidebar
        {...props}
        mode="collapsed"
        onPin={vi.fn()}
      />,
    )

    const sidebar = container.querySelector('aside') as HTMLElement
    expect(sidebar.style.width).toBe('6px')
    expect(sidebar.style.transition).toBe('none')
    expect(container.querySelector('[data-session-collapse-ghost="true"]')).not.toBeInTheDocument()
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

  it('toggles the archived section, restores, and deletes archived sessions inline', async () => {
    const archivedSession: AgentSessionView = {
      ...sessions[0],
      agentSessionId: 'agent-session-archived',
      title: 'Old session',
      status: 'archived',
      statusLabel: 'Archived',
      selected: false,
      archivedAt: '2026-04-10T20:00:00Z',
      isActive: false,
      isArchived: true,
    }
    const onLoadArchivedSessions = vi.fn(async () => [archivedSession])
    const onRestoreSession = vi.fn(async () => undefined)
    const onDeleteSession = vi.fn(async () => undefined)

    renderSidebar({
      onLoadArchivedSessions,
      onRestoreSession,
      onDeleteSession,
    })

    expect(screen.queryByText('Archived')).not.toBeInTheDocument()
    expect(screen.queryByText('Old session')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Show archived sessions' }))

    expect(onLoadArchivedSessions).toHaveBeenCalledWith('project-1')
    expect(await screen.findByText('Archived')).toBeVisible()
    expect(await screen.findByText('Old session')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Restore Old session' }))

    await waitFor(() =>
      expect(onRestoreSession).toHaveBeenCalledWith('agent-session-archived'),
    )
    await waitFor(() => expect(screen.queryByText('Old session')).not.toBeInTheDocument())
  })

  it('requires inline confirmation before deleting an archived session', async () => {
    const archivedSession: AgentSessionView = {
      ...sessions[0],
      agentSessionId: 'agent-session-archived',
      title: 'Old session',
      status: 'archived',
      statusLabel: 'Archived',
      selected: false,
      archivedAt: '2026-04-10T20:00:00Z',
      isActive: false,
      isArchived: true,
    }
    const onLoadArchivedSessions = vi.fn(async () => [archivedSession])
    const onRestoreSession = vi.fn(async () => undefined)
    const onDeleteSession = vi.fn(async () => undefined)

    renderSidebar({
      onLoadArchivedSessions,
      onRestoreSession,
      onDeleteSession,
    })

    fireEvent.click(screen.getByRole('button', { name: 'Show archived sessions' }))
    await screen.findByText('Old session')

    fireEvent.click(screen.getByRole('button', { name: 'Delete Old session permanently' }))

    expect(onDeleteSession).not.toHaveBeenCalled()
    expect(screen.queryByText('Permanently delete "Old session"?')).not.toBeInTheDocument()

    const confirmButton = screen.getByRole('button', { name: 'Confirm delete Old session' })
    expect(confirmButton).toHaveTextContent('Delete')

    fireEvent.click(confirmButton)

    await waitFor(() =>
      expect(onDeleteSession).toHaveBeenCalledWith('agent-session-archived'),
    )

    const row = screen.getByText('Old session').closest('li') as HTMLElement
    await waitFor(() => expect(row).toHaveClass('animate-out', 'duration-150'))
    expect(row).not.toHaveClass('duration-300')

    const exitingRow = screen.getByText('Old session').closest('li') as HTMLElement
    fireEvent.animationEnd(exitingRow)

    await waitFor(() => expect(screen.queryByText('Old session')).not.toBeInTheDocument())
  })

  it('renders pane number chips for sessions loaded in panes', () => {
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

    // The focused session still needs its pane chip so the sidebar matches the open pane list.
    expect(screen.getByLabelText('Loaded in pane 1')).toHaveTextContent('P1')
    expect(screen.getByLabelText('Loaded in pane 2')).toHaveTextContent('P2')
  })
})
