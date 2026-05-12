/** @vitest-environment jsdom */

import { memo, Profiler } from 'react'
import type { ReactNode } from 'react'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { isTauriMock, tauriWindowMock, invokeMock, listenMock, openUrlMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(() => false),
  tauriWindowMock: {
    close: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    startDragging: vi.fn(),
  },
  invokeMock: vi.fn(async () => ({
    android: { present: false },
    ios: { present: false, supported: false },
  })),
  listenMock: vi.fn(async () => () => undefined),
  openUrlMock: vi.fn(async () => undefined),
}))

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: isTauriMock,
  invoke: invokeMock,
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => tauriWindowMock,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipContent: () => null,
  TooltipTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
}))

import { ProjectRail } from '@/components/xero/project-rail'
import { XeroShell } from '@/components/xero/shell'
import {
  clearProjectSelectionPreview,
  previewProjectSelection,
} from '@/src/features/xero/project-selection-preview'
import type { ProjectListItem } from '@/src/lib/xero-model'

const projects = [
  {
    id: 'project-1',
    name: 'Alpha',
    description: 'Baseline project',
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
  {
    id: 'project-2',
    name: 'Beta',
    description: 'Target project',
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
] satisfies ProjectListItem[]

interface HeavySurfaceCounters {
  renders: number
  checksum: number
}

const HeavySurface = memo(function HeavySurface({
  counters,
}: {
  counters: HeavySurfaceCounters
}) {
  counters.renders += 1
  let checksum = 0
  for (let index = 0; index < 80_000; index += 1) {
    checksum = (checksum + (index * 17) % 97) % 1_000_000
  }
  counters.checksum = checksum

  return <div data-testid="heavy-surface">{checksum}</div>
})

describe('project switch benchmark', () => {
  afterEach(() => {
    act(() => {
      clearProjectSelectionPreview()
    })
  })

  it('updates the shell project name from rail pointerdown without rerendering the heavy surface', () => {
    const onSelectProject = vi.fn()
    const heavyCounters: HeavySurfaceCounters = {
      renders: 0,
      checksum: 0,
    }
    const profilerUpdateDurations: number[] = []

    const { container } = render(
      <Profiler
        id="project-switch-shell"
        onRender={(_id, phase, actualDuration) => {
          if (phase === 'update') {
            profilerUpdateDurations.push(actualDuration)
          }
        }}
      >
        <XeroShell
          activeView="agent"
          onViewChange={() => undefined}
          platformOverride="windows"
          projectId="project-1"
          projectName="Alpha"
        >
          <ProjectRail
            activeProjectId="project-1"
            errorMessage={null}
            isImporting={false}
            isLoading={false}
            onImportProject={() => undefined}
            onPreviewProject={(projectId) => {
              const project = projects.find((candidate) => candidate.id === projectId)
              if (project) {
                previewProjectSelection(project.id, project.name)
              }
            }}
            onRemoveProject={() => undefined}
            onSelectProject={onSelectProject}
            pendingProjectRemovalId={null}
            projectRemovalStatus="idle"
            projects={projects}
          />
          <HeavySurface counters={heavyCounters} />
        </XeroShell>
      </Profiler>,
    )

    const titlebar = container.querySelector('header')
    expect(titlebar).not.toBeNull()
    expect(titlebar).toHaveTextContent('Alpha')
    expect(titlebar).not.toHaveTextContent('Beta')

    const betaButton = screen.getByRole('button', { name: 'Open Beta' })
    const heavyRendersBeforeSwitch = heavyCounters.renders
    const updatesBeforeSwitch = profilerUpdateDurations.length
    const startedAt = performance.now()

    act(() => {
      fireEvent.pointerDown(betaButton, { button: 0 })
    })

    const eventToShellUpdateMs = performance.now() - startedAt
    const switchUpdateDurations = profilerUpdateDurations.slice(updatesBeforeSwitch)
    const maxReactUpdateMs = Math.max(0, ...switchUpdateDurations)

    expect(titlebar).toHaveTextContent('Beta')
    expect(betaButton).toHaveAttribute('aria-current', 'true')
    expect(onSelectProject).not.toHaveBeenCalled()
    expect(heavyCounters.renders).toBe(heavyRendersBeforeSwitch)
    expect(eventToShellUpdateMs).toBeLessThan(50)
    expect(maxReactUpdateMs).toBeLessThan(16)
    if (process.env.XERO_PROJECT_SWITCH_BENCHMARK_LOG === '1') {
      console.info(
        '[project-switch-benchmark]',
        JSON.stringify({
          eventToShellUpdateMs: Math.round(eventToShellUpdateMs * 100) / 100,
          heavySurfaceRerenders: heavyCounters.renders - heavyRendersBeforeSwitch,
          maxReactUpdateMs: Math.round(maxReactUpdateMs * 100) / 100,
        }),
      )
    }

    act(() => {
      fireEvent.click(betaButton)
    })
    expect(onSelectProject).toHaveBeenCalledWith('project-2')
  })
})
