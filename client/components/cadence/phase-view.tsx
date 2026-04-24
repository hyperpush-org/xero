"use client"

import { useMemo } from 'react'
import {
  safePercent,
  type PlanningLifecycleStageKindDto,
  type PlanningLifecycleStageView,
} from '@/src/lib/cadence-model'
import type { WorkflowPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { Bot, LoaderCircle, Milestone, Play, ChevronRight } from 'lucide-react'
import { CenteredEmptyState } from '@/components/cadence/centered-empty-state'
import { Button } from '@/components/ui/button'

interface PhaseViewProps {
  workflow: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  onOpenSettings?: () => void
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
        isEmpty ? 'border-dashed border-border bg-card/30' : 'border-border bg-card/60'
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
      {card.stage?.actionRequired ? (
        <div className="mt-2 space-y-2">
          <span className="rounded-full border border-destructive/35 bg-destructive/10 px-2 py-0.5 text-[10px] font-medium text-destructive">
            Action required
          </span>
          {card.stage.unblock ? (
            <div className="rounded-md border border-destructive/20 bg-destructive/5 px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
              <p className="text-foreground/85">{card.stage.unblock.reason}</p>
              <p className="mt-1 font-mono text-[10px] text-muted-foreground/90">
                gate: {card.stage.unblock.gateKey}
                {card.stage.unblock.actionId ? ` · action: ${card.stage.unblock.actionId}` : ''}
              </p>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

export function PhaseView({ workflow, onStartRun, onOpenSettings, canStartRun, isStartingRun }: PhaseViewProps) {
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
  const runtimeSession = workflow.runtimeSession ?? null
  const selectedProviderId = workflow.selectedProviderId ?? 'openai_codex'
  const providerMismatch = workflow.providerMismatch ?? false
  const showRuntimeSetupEmptyState =
    !hasLifecycle && !providerMismatch && (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle')

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="shrink-0 border-b border-border bg-card/30 px-4 py-[10px]">
          <div className="flex items-center gap-3 text-[12px]">
            <span className="shrink-0 text-muted-foreground">Milestone</span>
            <ChevronRight className="h-3 w-3 text-muted-foreground/40" />
            <h2 className="truncate font-medium text-foreground/80">{milestoneLabel}</h2>
            {hasStarted ? (
              <div className="ml-auto shrink-0 flex items-center gap-3">
                <div className="h-1 w-24 overflow-hidden rounded-full bg-border">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-500"
                    style={{ width: `${lifecyclePercent}%` }}
                  />
                </div>
                <span className="tabular-nums font-medium text-foreground/80">{lifecyclePercent}%</span>
                <span className="text-muted-foreground">
                  {lifecycle.completedCount}/{LIFECYCLE_STAGE_ORDER.length} stages
                </span>
              </div>
            ) : (
              <div className="ml-auto shrink-0 flex items-center gap-3">
                <span className="text-muted-foreground">Not started</span>
                {canStartRun && onStartRun ? (
                  <button
                    className="flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
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
                ) : null}
              </div>
            )}
          </div>
        </div>

        <div className="flex min-h-0 flex-1 overflow-y-auto scrollbar-thin px-6 py-5">
          {hasLifecycle ? (
            <div className="w-full">
              <section>
                <div className="mb-3 flex items-center justify-between">
                  <h2 className="font-mono text-[10px] uppercase tracking-[0.15em] text-muted-foreground">
                    Planning lifecycle
                  </h2>
                  {activeLifecycleLabel ? (
                    <span className="text-[11px] text-primary">{activeLifecycleLabel} active</span>
                  ) : null}
                </div>

                <div className="flex flex-col gap-2.5">
                  {lifecycleCards.map((card) => (
                    <LifecycleStageCard card={card} key={card.stageKind} />
                  ))}
                </div>
              </section>
            </div>
          ) : showRuntimeSetupEmptyState ? (
            <CenteredEmptyState
              description="Open Settings to choose a provider and model before using the workflow tab for this imported project."
              icon={Bot}
              title="Configure agent runtime"
              action={
                onOpenSettings ? (
                  <Button onClick={onOpenSettings} type="button">
                    Configure
                  </Button>
                ) : undefined
              }
            />
          ) : (
            <CenteredEmptyState
              description="Assign a milestone to this project to start tracking planning lifecycle stages."
              icon={Milestone}
              title="No milestone assigned"
            />
          )}
        </div>
      </div>
    </div>
  )
}
