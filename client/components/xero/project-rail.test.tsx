import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ProjectRail } from './project-rail'
import type { AgentSessionView } from '@/src/lib/xero-model'

const projects = [
  {
    id: 'project-1',
    name: 'mesh-lang',
    description: 'Xero desktop shell',
    milestone: 'No milestone assigned',
    totalPhases: 1,
    completedPhases: 0,
    activePhase: 0,
    branch: 'main',
    runtime: 'Runtime unavailable',
    branchLabel: 'main',
    runtimeLabel: 'Runtime unavailable',
    phaseProgressPercent: 0,
  },
]

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

describe('ProjectRail', () => {
  it('confirms before removing a project from the sidebar', async () => {
    const onRemoveProject = vi.fn()

    render(
      <ProjectRail
        activeProjectId="project-1"
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={() => undefined}
        onRemoveProject={onRemoveProject}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Remove mesh-lang' }))

    expect(await screen.findByText('Remove mesh-lang from the sidebar?')).toBeInTheDocument()
    expect(screen.getByText(/You can import the same folder again any time/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))

    expect(onRemoveProject).toHaveBeenCalledWith('project-1')
    expect(onRemoveProject).toHaveBeenCalledTimes(1)
  })

  it('removes milestone text from the expanded project row', () => {
    render(
      <ProjectRail
        activeProjectId="project-1"
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={() => undefined}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    expect(screen.getByText('mesh-lang')).toBeVisible()
    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('0%')).not.toBeInTheDocument()
  })

  it('resizes the expanded rail from the separator and persists the width', () => {
    render(
      <ProjectRail
        activeProjectId="project-1"
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={() => undefined}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    const separator = screen.getByRole('separator', { name: 'Resize projects sidebar' })
    const before = Number(separator.getAttribute('aria-valuenow'))

    fireEvent.keyDown(separator, { key: 'ArrowRight' })

    const after = Number(separator.getAttribute('aria-valuenow'))
    expect(after).toBeGreaterThan(before)
    expect(window.localStorage.getItem('xero.projectRail.width')).toBe(String(after))
  })

  it('keeps a compact monogram rail when collapsed', () => {
    const { container } = render(
      <ProjectRail
        activeProjectId="project-1"
        collapsed
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={() => undefined}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    const rail = screen.getByRole('complementary')
    const projectButton = screen.getByRole('button', { name: 'mesh-lang' })

    expect(screen.getByRole('button', { name: 'Import repository' })).toBeVisible()
    expect(projectButton).toBeVisible()
    expect(projectButton).not.toHaveClass('bg-primary/10')
    expect(screen.getByText('M')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Remove mesh-lang' })).not.toBeInTheDocument()
    expect(container.querySelector('button[aria-label="Remove mesh-lang"]')).toBeNull()
    expect(screen.queryByRole('separator', { name: 'Resize projects sidebar' })).not.toBeInTheDocument()
    expect(rail).toHaveAttribute('data-collapsed', 'true')
    expect(rail).toHaveClass('w-11')
  })

  it('keeps the project monogram visible while selection is pending', () => {
    const pendingProject = {
      ...projects[0],
      id: 'project-2',
      name: 'nova-ui',
    }

    render(
      <ProjectRail
        activeProjectId="project-1"
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={() => undefined}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        pendingProjectSelectionId="project-2"
        projectRemovalStatus="idle"
        projects={[...projects, pendingProject]}
      />,
    )

    const pendingProjectButton = screen.getByText('nova-ui').closest('button') as HTMLElement

    expect(pendingProjectButton).not.toHaveAttribute('aria-busy')
    expect(within(pendingProjectButton).getByText('N')).toBeVisible()
  })

  it('hides collapsed sessions and keeps only the expand control at the bottom', () => {
    const onCreateSession = vi.fn()
    const onSelectSession = vi.fn()
    const onExpandExplorer = vi.fn()

    render(
      <ProjectRail
        activeProjectId="project-1"
        collapsed
        errorMessage={null}
        explorerCollapsed
        isCreatingSession={false}
        isImporting={false}
        isLoading={false}
        onArchiveSession={() => undefined}
        onCreateSession={onCreateSession}
        onExpandExplorer={onExpandExplorer}
        onImportProject={() => undefined}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        onSelectSession={onSelectSession}
        pendingProjectRemovalId={null}
        pendingSessionId={null}
        projectRemovalStatus="idle"
        projects={projects}
        selectedSessionId="agent-session-main"
        sessions={sessions}
      />,
    )

    expect(screen.queryByRole('button', { name: 'New session' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Main session' })).not.toBeInTheDocument()

    const expandButton = screen.getByRole('button', { name: 'Expand sessions sidebar' })
    fireEvent.click(expandButton)

    expect(onExpandExplorer).toHaveBeenCalledTimes(1)
    expect(onCreateSession).not.toHaveBeenCalled()
    expect(onSelectSession).not.toHaveBeenCalled()
  })
})
