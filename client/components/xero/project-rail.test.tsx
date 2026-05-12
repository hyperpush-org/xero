import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ProjectRail } from './project-rail'

const projects = [
  {
    id: 'project-1',
    name: 'mesh-lang',
    description: 'Xero desktop shell',
    milestone: 'No milestone assigned',
    projectOrigin: 'unknown' as const,
    totalPhases: 1,
    completedPhases: 0,
    activePhase: 0,
    branch: 'main',
    runtime: 'Runtime unavailable',
    branchLabel: 'main',
    runtimeLabel: 'Runtime unavailable',
    phaseProgressPercent: 0,
    startTargets: [],
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

    fireEvent.contextMenu(screen.getByRole('button', { name: 'Open mesh-lang (active)' }))

    expect(await screen.findByText('Remove mesh-lang from the sidebar?')).toBeInTheDocument()
    expect(screen.getByText(/You can import the same folder again any time/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))

    expect(onRemoveProject).toHaveBeenCalledWith('project-1')
    expect(onRemoveProject).toHaveBeenCalledTimes(1)
  })

  it('renders compact project items without milestone or progress copy', () => {
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

    expect(screen.getByRole('button', { name: 'Open mesh-lang (active)' })).toBeVisible()
    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('0%')).not.toBeInTheDocument()
  })

  it('keeps only compact project monograms', () => {
    const onImportProject = vi.fn()
    const { container } = render(
      <ProjectRail
        activeProjectId="project-1"
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
    const projectButton = screen.getByRole('button', { name: 'Open mesh-lang (active)' })

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
    expect(rail).toHaveClass('w-12')
    expect(onImportProject).not.toHaveBeenCalled()
  })

  it('does not render project load errors as a destructive rail slot', () => {
    const { container } = render(
      <ProjectRail
        activeProjectId="project-1"
        errorMessage="Repository not found"
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

    expect(screen.getByRole('button', { name: 'Open mesh-lang (active)' })).toBeVisible()
    expect(screen.queryByText('!')).not.toBeInTheDocument()
    expect(container.querySelector('[title="Repository not found"]')).toBeNull()
    expect(container.querySelector('.text-destructive')).toBeNull()
  })

  it('imports a project from the compact rail add button', () => {
    const onImportProject = vi.fn()

    render(
      <ProjectRail
        activeProjectId="project-1"
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

  it('does not start native project prefetch from pointerdown selection preview', () => {
    const onPreloadProject = vi.fn()
    const onPreviewProject = vi.fn()
    const onSelectProject = vi.fn()
    const secondProject = {
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
        onPreloadProject={onPreloadProject}
        onPreviewProject={onPreviewProject}
        onRemoveProject={() => undefined}
        onSelectProject={onSelectProject}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={[...projects, secondProject]}
      />,
    )

    const projectButton = screen.getByRole('button', { name: 'Open nova-ui' })
    fireEvent.pointerDown(projectButton, { button: 0 })

    expect(onPreviewProject).toHaveBeenCalledWith('project-2')
    expect(onPreviewProject).toHaveBeenCalledTimes(1)
    expect(onPreloadProject).not.toHaveBeenCalled()

    fireEvent.click(projectButton)

    expect(onSelectProject).toHaveBeenCalledWith('project-2')
    expect(onPreviewProject).toHaveBeenLastCalledWith('project-2')
    expect(onPreviewProject).toHaveBeenCalledTimes(2)
    expect(onPreloadProject).not.toHaveBeenCalled()
  })

  it('preloads a project from rail hover and keyboard focus intent', () => {
    const onPreloadProject = vi.fn()
    const secondProject = {
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
        onPreloadProject={onPreloadProject}
        onRemoveProject={() => undefined}
        onSelectProject={() => undefined}
        pendingProjectRemovalId={null}
        projectRemovalStatus="idle"
        projects={[...projects, secondProject]}
      />,
    )

    const projectButton = screen.getByRole('button', { name: 'Open nova-ui' })
    fireEvent.pointerEnter(projectButton)
    expect(onPreloadProject).toHaveBeenCalledWith('project-2')

    fireEvent.focus(projectButton)
    expect(onPreloadProject).toHaveBeenLastCalledWith('project-2')
    expect(onPreloadProject).toHaveBeenCalledTimes(2)
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

  it('emits sessions peek intent when the project rail is hovered', () => {
      const onSessionsHoverEnter = vi.fn()
      const onSessionsHoverLeave = vi.fn()

      render(
        <ProjectRail
          activeProjectId="project-1"
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
  })
})
