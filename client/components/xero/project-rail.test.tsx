import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ProjectRail } from './project-rail'

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

  it('keeps only compact project monograms when collapsed', () => {
    const onImportProject = vi.fn()
    const { container } = render(
      <ProjectRail
        activeProjectId="project-1"
        collapsed
        errorMessage={null}
        isImporting
        isLoading={false}
        onImportProject={onImportProject}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    const rail = screen.getByRole('complementary')
    const importButton = screen.getByRole('button', { name: 'Import repository' })
    const projectButton = screen.getByRole('button', { name: 'mesh-lang' })

    expect(importButton).toBeVisible()
    expect(importButton).toBeDisabled()
    expect(screen.queryByText('Projects')).not.toBeInTheDocument()
    expect(screen.queryByText(/Importing/)).not.toBeInTheDocument()
    expect(projectButton).toBeVisible()
    expect(projectButton).not.toHaveClass('bg-primary/10')
    expect(screen.getByText('M')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Remove mesh-lang' })).not.toBeInTheDocument()
    expect(container.querySelector('button[aria-label="Remove mesh-lang"]')).toBeNull()
    expect(screen.queryByRole('separator', { name: 'Resize projects sidebar' })).not.toBeInTheDocument()
    expect(rail).toHaveAttribute('data-collapsed', 'true')
    expect(rail).toHaveClass('w-11')
    expect(onImportProject).not.toHaveBeenCalled()
  })

  it('imports a project from the collapsed rail add button', () => {
    const onImportProject = vi.fn()

    render(
      <ProjectRail
        activeProjectId="project-1"
        collapsed
        errorMessage={null}
        isImporting={false}
        isLoading={false}
        onImportProject={onImportProject}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={projects}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Import repository' }))

    expect(onImportProject).toHaveBeenCalledTimes(1)
  })

  it('snaps width transitions when requested', () => {
    render(
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
        snapWidth
      />,
    )

    expect(screen.getByRole('complementary')).toHaveStyle({ transition: 'none' })
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

  it('never renders session controls inside the project rail', () => {
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

    expect(screen.queryByText('Sessions')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'New session' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Expand sessions sidebar' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'View archived sessions' })).not.toBeInTheDocument()
  })

  it.each([false, true])(
    'emits sessions peek intent when the project rail is hovered while collapsed=%s',
    (collapsed) => {
      const onSessionsHoverEnter = vi.fn()
      const onSessionsHoverLeave = vi.fn()

      render(
        <ProjectRail
          activeProjectId="project-1"
          collapsed={collapsed}
          errorMessage={null}
          isImporting={false}
          isLoading={false}
          onImportProject={() => undefined}
          onRemoveProject={() => undefined}
          onSelectProject={() => undefined}
          onSessionsHoverEnter={onSessionsHoverEnter}
          onSessionsHoverLeave={onSessionsHoverLeave}
          pendingProjectRemovalId={null}
          projectRemovalStatus="idle"
          projects={projects}
        />,
      )

      const rail = screen.getByRole('complementary')
      fireEvent.pointerEnter(rail)
      fireEvent.pointerLeave(rail)

      expect(onSessionsHoverEnter).toHaveBeenCalledTimes(1)
      expect(onSessionsHoverLeave).toHaveBeenCalledTimes(1)
      expect(screen.queryByText('Sessions')).not.toBeInTheDocument()
    },
  )
})
