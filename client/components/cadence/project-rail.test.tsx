import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ProjectRail } from '@/components/cadence/project-rail'
import type { ProjectListItem } from '@/src/lib/cadence-model'

const projects: ProjectListItem[] = [
  {
    id: 'project-1',
    name: 'cadence',
    description: 'Cadence desktop shell',
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

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Project actions for cadence' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Remove from sidebar' }))

    expect(screen.getByText('Remove cadence from the sidebar?')).toBeInTheDocument()
    expect(screen.getByText(/You can import the same folder again any time/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))

    expect(onRemoveProject).toHaveBeenCalledWith('project-1')
    expect(onRemoveProject).toHaveBeenCalledTimes(1)
  })
})
