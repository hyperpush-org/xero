/**
 * Regression coverage for the import-screen text jitter bug.
 *
 * Root cause: `loadProjectState` sets `activeProject` twice — once after
 * the snapshot promise resolves (partial data) and once after the runtime
 * promise resolves (full data). Both updates happen while `isBusy` is
 * still true. Previously `ProjectStep` switched from the loading button to
 * the project card as soon as `project` became non-null, causing a
 * mid-import layout switch that re-animated the text and looked like jitter.
 *
 * Fix: `showProjectCard = !isBusy && project !== null` — the card only
 * appears after the full load is complete.
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ProjectStep } from '@/components/cadence/onboarding/steps/project-step'

const PROJECT = { name: 'my-app', path: '/Users/dev/my-app' }

describe('ProjectStep import stability', () => {
  it('shows loading button while importing and project is null', () => {
    render(
      <ProjectStep
        project={null}
        isImporting={true}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('Importing repository…')).toBeInTheDocument()
    expect(screen.queryByText('my-app')).not.toBeInTheDocument()
  })

  // ── THE REGRESSION CASE: importing=true but project data has partially
  //    arrived (first async phase of loadProjectState completed while the
  //    second is still in-flight). Before the fix this would switch the
  //    entire UI to the project card mid-import.

  it('does not render project card while isImporting is true, even when project data is present', () => {
    render(
      <ProjectStep
        project={PROJECT}
        isImporting={true}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    // Should still be showing the loading button
    expect(screen.getByText('Importing repository…')).toBeInTheDocument()
    // Project card must not be visible
    expect(screen.queryByText('my-app')).not.toBeInTheDocument()
    expect(screen.queryByText('Imported')).not.toBeInTheDocument()
  })

  it('does not render project card while isProjectLoading is true, even when project data is present', () => {
    render(
      <ProjectStep
        project={PROJECT}
        isImporting={false}
        isProjectLoading={true}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('Importing repository…')).toBeInTheDocument()
    expect(screen.queryByText('my-app')).not.toBeInTheDocument()
    expect(screen.queryByText('Imported')).not.toBeInTheDocument()
  })

  it('shows project card only after both isImporting and isProjectLoading are false', () => {
    render(
      <ProjectStep
        project={PROJECT}
        isImporting={false}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('my-app')).toBeInTheDocument()
    expect(screen.getByText('Imported')).toBeInTheDocument()
    expect(screen.getByText('/Users/dev/my-app')).toBeInTheDocument()
    // Loading text must be gone
    expect(screen.queryByText('Importing repository…')).not.toBeInTheDocument()
    expect(screen.queryByText('Choose a folder')).not.toBeInTheDocument()
  })

  it('shows project card with pick-different-folder button when done', () => {
    render(
      <ProjectStep
        project={PROJECT}
        isImporting={false}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByRole('button', { name: /pick a different folder/i })).toBeInTheDocument()
    // The pick-different-folder button should not be disabled
    expect(screen.getByRole('button', { name: /pick a different folder/i })).not.toBeDisabled()
  })

  it('shows choose-a-folder prompt when no project and not busy', () => {
    render(
      <ProjectStep
        project={null}
        isImporting={false}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('Choose a folder')).toBeInTheDocument()
    expect(screen.getByText('Select a local Git repository.')).toBeInTheDocument()
  })

  it('shows error message when present', () => {
    render(
      <ProjectStep
        project={null}
        isImporting={false}
        isProjectLoading={false}
        errorMessage="Repository not found"
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('Repository not found')).toBeInTheDocument()
  })

  it('shows error alongside loading state during busy import', () => {
    render(
      <ProjectStep
        project={null}
        isImporting={true}
        isProjectLoading={false}
        errorMessage="Network timeout"
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('Network timeout')).toBeInTheDocument()
    expect(screen.getByText('Importing repository…')).toBeInTheDocument()
  })

  // text stays stable during both async phases
  // Simulate the two-phase load: first render with isProjectLoading=true +
  // project data (phase-1 partial), then final render with isProjectLoading=false.
  // Text should only appear once, in the final render.

  it('text appears exactly once — in the final stable render, not during phase-1', () => {
    const { rerender } = render(
      <ProjectStep
        project={null}
        isImporting={true}
        isProjectLoading={true}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )

    // Phase 1: project data arrives but isBusy still true
    rerender(
      <ProjectStep
        project={PROJECT}
        isImporting={true}
        isProjectLoading={true}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.queryByText('my-app')).not.toBeInTheDocument()

    // Phase 2: runtime data arrives, still busy
    rerender(
      <ProjectStep
        project={PROJECT}
        isImporting={false}
        isProjectLoading={true}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.queryByText('my-app')).not.toBeInTheDocument()

    // Final: load complete
    rerender(
      <ProjectStep
        project={PROJECT}
        isImporting={false}
        isProjectLoading={false}
        errorMessage={null}
        onImportProject={() => {}}
      />,
    )
    expect(screen.getByText('my-app')).toBeInTheDocument()
  })
})
