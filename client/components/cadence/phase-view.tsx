"use client"

import { useEffect, useMemo, useState } from 'react'
import {
  safePercent,
  type PlanningLifecycleStageKindDto,
  type PlanningLifecycleStageView,
} from '@/src/lib/cadence-model'
import type { WorkflowPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { GitBranch, Hash, LoaderCircle, Milestone, PanelRight, PanelRightClose, Play, Terminal } from 'lucide-react'
import { CenteredEmptyState } from '@/components/cadence/centered-empty-state'

interface PhaseViewProps {
  workflow: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  canStartRun?: boolean
  isStartingRun?: boolean
}

const LIFECYCLE_STAGE_ORDER: PlanningLifecycleStageKindDto[] = [
  'discussion',
  'research',
  'requirements',
  'roadmap',
]

const LIFECYCLE_STAGE_LABELS: Record<PlanningLifecycleStageKindDto, string> = {
  discussion: 'Discussion',
  research: 'Research',
  requirements: 'Requirements',
  roadmap: 'Roadmap',
}

type LifecycleStageCardModel = {
  stageKind: PlanningLifecycleStageKindDto
  stageLabel: string
  stage: PlanningLifecycleStageView | null
}

function createEmptyLifecycleByStage(): Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView | null> {
  return {
    discussion: null,
    research: null,
    requirements: null,
    roadmap: null,
  }
}

function getStatusColor(status: NonNullable<PlanningLifecycleStageView['status']>): string {
  switch (status) {
    case 'complete':
      return 'border-success/40 bg-success/10 text-success'
    case 'active':
      return 'border-primary/40 bg-primary/10 text-primary'
    case 'blocked':
      return 'border-destructive/40 bg-destructive/10 text-destructive'
    case 'pending':
      return 'border-border bg-secondary/40 text-muted-foreground'
  }
}

function LifecycleStageCard({ card }: { card: LifecycleStageCardModel }) {
  const isEmpty = !card.stage

  return (
    <div
      className={`rounded-lg border p-3.5 ${
        isEmpty
          ? 'border-dashed border-border bg-card/30'
          : 'border-border bg-card/60'
      }`}
    >
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-[13px] font-medium text-foreground">{card.stageLabel}</h3>
        {isEmpty ? (
          <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            —
          </span>
        ) : (
          <span
            className={`rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide ${getStatusColor(card.stage!.status)}`}
          >
            {card.stage!.statusLabel}
          </span>
        )}
      </div>
      {card.stage?.actionRequired && (
        <div className="mt-2">
          <span className="rounded-full border border-destructive/35 bg-destructive/10 px-2 py-0.5 text-[10px] font-medium text-destructive">
            Action required
          </span>
        </div>
      )}
    </div>
  )
}

export function PhaseView({ workflow, onStartRun, canStartRun, isStartingRun }: PhaseViewProps) {
  const lifecycle = workflow.lifecycle ?? {
    stages: [],
    byStage: createEmptyLifecycleByStage(),
    hasStages: false,
    activeStage: null,
    actionRequiredCount: 0,
    blockedCount: 0,
    completedCount: 0,
    percentComplete: 0,
  }
  const hasLifecycle = workflow.hasLifecycle ?? lifecycle.hasStages

  // Sidebar collapses by default when there's no active project/milestone
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => !hasLifecycle)

  // Auto-collapse when lifecycle disappears; auto-expand when it appears for the first time
  useEffect(() => {
    if (!hasLifecycle) {
      setSidebarCollapsed(true)
    }
  }, [hasLifecycle])

  const lifecycleCards = useMemo<LifecycleStageCardModel[]>(() => {
    const byStageFromList = LIFECYCLE_STAGE_ORDER.reduce<Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView[]>>(
      (acc, stageKind) => {
        acc[stageKind] = []
        return acc
      },
      {
        discussion: [],
        research: [],
        requirements: [],
        roadmap: [],
      },
    )

    lifecycle.stages.forEach((stage) => {
      byStageFromList[stage.stage].push(stage)
    })

    return LIFECYCLE_STAGE_ORDER.map((stageKind) => {
      const entries = byStageFromList[stageKind]
      const stage = entries[0] ?? lifecycle.byStage[stageKind] ?? null

      return {
        stageKind,
        stageLabel: LIFECYCLE_STAGE_LABELS[stageKind],
        stage,
      }
    })
  }, [lifecycle.byStage, lifecycle.stages])

  const lifecyclePercent = hasLifecycle
    ? workflow.lifecyclePercent ?? safePercent(lifecycle.completedCount, lifecycleCards.length)
    : 0
  const activeLifecycleLabel = workflow.activeLifecycleStage?.stageLabel ?? lifecycle.activeStage?.stageLabel ?? null

  const milestoneLabel = workflow.project.milestone
  const hasStarted = hasLifecycle && lifecyclePercent > 0

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Milestone header */}
        <div className="shrink-0 border-b border-border px-5 py-3">
          <div className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-2 min-w-0 text-[13px]">
              <span className="shrink-0 text-muted-foreground">Milestone —</span>
              <h2 className="truncate font-medium text-foreground">{milestoneLabel}</h2>
            </div>
            {hasStarted ? (
              <div className="flex items-center gap-3 shrink-0">
                <div className="w-24 h-1 overflow-hidden rounded-full bg-border">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-500"
                    style={{ width: `${lifecyclePercent}%` }}
                  />
                </div>
                <span className="tabular-nums text-[12px] font-medium text-foreground">{lifecyclePercent}%</span>
                <span className="text-[11px] text-muted-foreground">
                  {lifecycle.completedCount}/{LIFECYCLE_STAGE_ORDER.length} stages
                </span>
              </div>
            ) : (
              <div className="flex items-center gap-3 shrink-0 text-[12px]">
                <span className="text-muted-foreground">Not started</span>
                {canStartRun && onStartRun && (
                  <button
                    className="flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                    disabled={isStartingRun}
                    onClick={() => void onStartRun()}
                    type="button"
                  >
                    {isStartingRun ? (
                      <LoaderCircle className="h-3 w-3 animate-spin" />
                    ) : (
                      <Play className="h-3 w-3" />
                    )}
                    Start run
                  </button>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Main content */}
        <div className="flex flex-1 min-h-0 overflow-y-auto scrollbar-thin px-6 py-5">
          {hasLifecycle ? (
            <div className="w-full">
              {/* Lifecycle section */}
              <section>
                <div className="mb-3 flex items-center justify-between">
                  <h2 className="font-mono text-[10px] uppercase tracking-[0.15em] text-muted-foreground">
                    Planning lifecycle
                  </h2>
                  {activeLifecycleLabel && (
                    <span className="text-[11px] text-primary">{activeLifecycleLabel} active</span>
                  )}
                </div>

                <div className="flex flex-col gap-2.5">
                  {lifecycleCards.map((card) => (
                    <LifecycleStageCard card={card} key={card.stageKind} />
                  ))}
                </div>
              </section>
            </div>
          ) : (
            <CenteredEmptyState
              description="Assign a milestone to this project to start tracking planning lifecycle stages."
              icon={Milestone}
              title="No milestone assigned"
            />
          )}
        </div>
      </div>

      {/* Right sidebar — project context */}
      {sidebarCollapsed ? (
        <aside className="flex w-9 shrink-0 flex-col border-l border-border bg-sidebar">
          <div className="flex justify-center border-b border-border py-2.5">
            <button
              onClick={() => setSidebarCollapsed(false)}
              className="rounded p-1 text-muted-foreground hover:bg-secondary/60 hover:text-foreground transition-colors"
              title="Expand context panel"
            >
              <PanelRight className="h-3.5 w-3.5" />
            </button>
          </div>
        </aside>
      ) : (
        <aside className="flex w-52 shrink-0 flex-col border-l border-border bg-sidebar">
          <div className="flex items-center justify-between border-b border-border px-3 py-2.5">
            <span className="font-mono text-[10px] uppercase tracking-[0.15em] text-muted-foreground">Context</span>
            <button
              onClick={() => setSidebarCollapsed(true)}
              className="rounded p-1 text-muted-foreground hover:bg-secondary/60 hover:text-foreground transition-colors"
              title="Collapse context panel"
            >
              <PanelRightClose className="h-3.5 w-3.5" />
            </button>
          </div>
          <div className="flex-1 space-y-3 overflow-y-auto scrollbar-thin px-3 py-3 text-[11px]">
            <div>
              <p className="mb-1 font-mono text-[10px] uppercase tracking-wide text-muted-foreground">Project</p>
              <p className="font-medium text-foreground/90">{workflow.project.name ?? workflow.project.repository?.displayName ?? '—'}</p>
            </div>
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-foreground/70">
                <GitBranch className="h-3 w-3 shrink-0" />
                <span className="truncate font-mono text-[11px]">{workflow.project.branchLabel}</span>
              </div>
              <div className="flex items-center gap-2 text-foreground/70">
                <Hash className="h-3 w-3 shrink-0" />
                <span className="truncate font-mono text-[11px]">{workflow.project.repository?.headShaLabel ?? '—'}</span>
              </div>
              <div className="flex items-center gap-2 text-foreground/70">
                <Terminal className="h-3 w-3 shrink-0" />
                <span className="truncate font-mono text-[11px]">{workflow.project.runtimeLabel}</span>
              </div>
            </div>
            {workflow.project.repository?.rootPath && (
              <div>
                <p className="mb-1 font-mono text-[10px] uppercase tracking-wide text-muted-foreground">Path</p>
                <p className="break-all font-mono text-[10px] text-muted-foreground">{workflow.project.repository.rootPath}</p>
              </div>
            )}
          </div>
        </aside>
      )}
    </div>
  )
}
