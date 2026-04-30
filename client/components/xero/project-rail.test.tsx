import { fireEvent, render, screen } from '@testing-library/react'
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

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Project actions for mesh-lang' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }))

    expect(screen.getByText('Remove mesh-lang from the sidebar?')).toBeInTheDocument()
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
    expect(screen.queryByRole('button', { name: 'Project actions for mesh-lang' })).not.toBeInTheDocument()
    expect(container.querySelector('button[aria-label="Project actions for mesh-lang"]')).toBeNull()
    expect(screen.queryByRole('separator', { name: 'Resize projects sidebar' })).not.toBeInTheDocument()
    expect(rail).toHaveAttribute('data-collapsed', 'true')
    expect(rail).toHaveClass('w-11')
  })
})
