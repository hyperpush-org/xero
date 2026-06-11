import { useEffect, useMemo, useState } from 'react'
import {
  Bot,
  Play,
  Plus,
  Workflow as WorkflowIcon,
  type LucideIcon,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { WORKFLOWS_ENABLED } from '@/src/features/xero/workflows-feature-flag'
import type {
  AgentRefDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'
import type { WorkflowTemplateIdDto } from '@/src/lib/xero-model/workflow-templates'

import { CreateAgentDialog } from './create-agent-dialog'
import type { CreateEntityDialogView } from './create-entity-dialog'
import { CreateWorkflowDialog } from './create-workflow-dialog'

interface WorkflowCanvasEmptyStateProps {
  onCreateAgent?: () => void
  onCreateAgentFromTemplate?: (ref: AgentRefDto) => void
  onCreateWorkflow?: () => void
  onCreateWorkflowWithAgentCreate?: () => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onBrowseWorkflows?: () => void
  templates?: WorkflowAgentSummaryDto[]
  templatesLoading?: boolean
  templatesError?: Error | null
  className?: string
}

interface Action {
  icon: LucideIcon
  label: string
  onSelect?: () => void
  comingSoon?: boolean
  exiting?: boolean
}

type CreateDialogKind = 'agent' | 'workflow'

export function WorkflowCanvasEmptyState({
  onCreateAgent,
  onCreateAgentFromTemplate,
  onCreateWorkflow,
  onCreateWorkflowWithAgentCreate,
  onCreateWorkflowFromTemplate,
  onBrowseWorkflows,
  templates = [],
  templatesLoading = false,
  templatesError = null,
  className,
}: WorkflowCanvasEmptyStateProps) {
  const [dialogKind, setDialogKind] = useState<CreateDialogKind | null>(null)
  const [dialogView, setDialogView] = useState<CreateEntityDialogView>('choice')

  const canStartBlankAgent = Boolean(onCreateAgent)
  const canPickAgentTemplate = Boolean(onCreateAgentFromTemplate)
  const canCreateAgent = canStartBlankAgent || canPickAgentTemplate
  const canStartBlankWorkflow = WORKFLOWS_ENABLED && Boolean(onCreateWorkflow)
  const canUseAgentCreateForWorkflow =
    WORKFLOWS_ENABLED && Boolean(onCreateWorkflowWithAgentCreate)
  const canPickWorkflowTemplate = WORKFLOWS_ENABLED && Boolean(onCreateWorkflowFromTemplate)
  const canCreateWorkflow =
    canStartBlankWorkflow || canUseAgentCreateForWorkflow || canPickWorkflowTemplate
  const canBrowseWorkflows = WORKFLOWS_ENABLED && Boolean(onBrowseWorkflows)
  const shouldShowBrowseWorkflowAction = WORKFLOWS_ENABLED ? canBrowseWorkflows : true
  const [browseWorkflowsMounted, setBrowseWorkflowsMounted] = useState(
    shouldShowBrowseWorkflowAction,
  )
  const [browseWorkflowsVisible, setBrowseWorkflowsVisible] = useState(
    shouldShowBrowseWorkflowAction,
  )

  const visibleTemplates = useMemo(
    () =>
      templates.filter((agent) => {
        if (agent.ref.kind !== 'built_in') return true
        return agent.ref.runtimeAgentId !== 'crawl' && agent.ref.runtimeAgentId !== 'agent_create'
      }),
    [templates],
  )

  function openCreateAgentDialog() {
    setDialogView('choice')
    setDialogKind('agent')
  }

  function openCreateWorkflowDialog() {
    setDialogView('choice')
    setDialogKind('workflow')
  }

  function closeCreateDialog() {
    setDialogKind(null)
    setDialogView('choice')
  }

  function handleDialogOpenChange(open: boolean) {
    if (!open) closeCreateDialog()
  }

  function handleStartBlankAgent() {
    if (!onCreateAgent) return
    closeCreateDialog()
    onCreateAgent()
  }

  function handlePickAgentTemplate(ref: AgentRefDto) {
    if (!onCreateAgentFromTemplate) return
    closeCreateDialog()
    onCreateAgentFromTemplate(ref)
  }

  function handleStartBlankWorkflow() {
    if (!onCreateWorkflow) return
    closeCreateDialog()
    onCreateWorkflow()
  }

  function handleCreateWorkflowWithAgentCreate() {
    if (!onCreateWorkflowWithAgentCreate) return
    closeCreateDialog()
    onCreateWorkflowWithAgentCreate()
  }

  function handlePickWorkflowTemplate(templateId: WorkflowTemplateIdDto) {
    if (!onCreateWorkflowFromTemplate) return
    closeCreateDialog()
    onCreateWorkflowFromTemplate(templateId)
  }

  useEffect(() => {
    if (shouldShowBrowseWorkflowAction) {
      setBrowseWorkflowsMounted(true)

      if (
        typeof window === 'undefined' ||
        typeof window.requestAnimationFrame !== 'function'
      ) {
        setBrowseWorkflowsVisible(true)
        return
      }

      const frame = window.requestAnimationFrame(() => setBrowseWorkflowsVisible(true))
      return () => window.cancelAnimationFrame(frame)
    }

    setBrowseWorkflowsVisible(false)

    if (!browseWorkflowsMounted) return

    const timeout = window.setTimeout(() => {
      setBrowseWorkflowsMounted(false)
    }, 180)
    return () => window.clearTimeout(timeout)
  }, [browseWorkflowsMounted, shouldShowBrowseWorkflowAction])

  const actions: Action[] = [
    { icon: Bot, label: 'Create agent', onSelect: canCreateAgent ? openCreateAgentDialog : undefined },
    { icon: Plus, label: 'Create workflow', onSelect: canCreateWorkflow ? openCreateWorkflowDialog : undefined, comingSoon: !canCreateWorkflow },
    ...(browseWorkflowsMounted
      ? [
          {
            icon: Play,
            label: 'Run an existing workflow',
            onSelect: onBrowseWorkflows,
            comingSoon: !canBrowseWorkflows,
            exiting: !browseWorkflowsVisible,
          },
        ]
      : []),
  ]

  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-0 z-[5] flex items-center justify-center px-6',
        className,
      )}
    >
      <div
        className="workflow-empty-state pointer-events-auto relative flex w-full max-w-xl flex-col items-center px-8 py-12 text-center"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <BrandGlyph />

        <h2 className="mt-5 text-2xl font-semibold tracking-tight text-foreground sm:text-[26px]">
          {WORKFLOWS_ENABLED ? (
            <>
              Start with a <span className="text-primary">workflow</span>
            </>
          ) : (
            <>
              Start with an <span className="text-primary">agent</span>
            </>
          )}
        </h2>
        {!WORKFLOWS_ENABLED ? (
          <Badge
            variant="outline"
            className="mt-3 text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground"
          >
            Workflows coming soon
          </Badge>
        ) : null}
        <p className="mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          {WORKFLOWS_ENABLED
            ? 'Compose agents into a workflow on the canvas, or define a new agent to use as a building block.'
            : 'Create or inspect agents on the canvas. Multi-agent workflows will return here when they are ready.'}
        </p>

        <ul className="relative mt-8 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          {actions.map((action) => {
            const disabled = action.comingSoon || !action.onSelect
            const exiting = action.exiting ?? false
            return (
              <li
                aria-hidden={exiting || undefined}
                className={cn(
                  'grid transition-[grid-template-rows,opacity] duration-150 ease-out motion-reduce:transition-none',
                  exiting ? 'grid-rows-[0fr] opacity-0' : 'grid-rows-[1fr] opacity-100',
                )}
                data-state={exiting ? 'exiting' : 'visible'}
                key={action.label}
              >
                <div
                  className={cn(
                    'min-h-0 overflow-hidden transition-transform duration-150 ease-out motion-reduce:transition-none',
                    exiting && '-translate-y-1',
                  )}
                >
                  <button
                    aria-disabled={disabled || undefined}
                    className={cn(
                      'group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors',
                      disabled
                        ? 'cursor-not-allowed'
                        : 'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                    )}
                    disabled={disabled}
                    onClick={action.onSelect}
                    tabIndex={exiting ? -1 : undefined}
                    type="button"
                  >
                    <action.icon
                      className={cn(
                        'h-3.5 w-3.5 shrink-0 transition-colors',
                        disabled
                          ? 'text-muted-foreground/50'
                          : 'text-muted-foreground group-hover:text-primary',
                      )}
                    />
                    <span
                      className={cn(
                        'flex-1 truncate text-[13px]',
                        disabled
                          ? 'text-foreground/45'
                          : 'text-foreground/85 group-hover:text-foreground',
                      )}
                    >
                      {action.label}
                    </span>
                    {action.comingSoon ? (
                      <Badge
                        variant="outline"
                        className="shrink-0 text-[9.5px] uppercase tracking-[0.12em] font-semibold text-muted-foreground"
                      >
                        Coming soon
                      </Badge>
                    ) : null}
                  </button>
                </div>
              </li>
            )
          })}
        </ul>
      </div>

      {dialogKind === 'agent' && canCreateAgent ? (
        <CreateAgentDialog
          open
          onOpenChange={handleDialogOpenChange}
          view={dialogView}
          onSetView={setDialogView}
          canStartBlank={canStartBlankAgent}
          canPickTemplate={canPickAgentTemplate}
          templates={visibleTemplates}
          templatesLoading={templatesLoading}
          templatesError={templatesError}
          onStartBlank={handleStartBlankAgent}
          onPickTemplate={handlePickAgentTemplate}
        />
      ) : null}

      {dialogKind === 'workflow' && canCreateWorkflow ? (
        <CreateWorkflowDialog
          open
          onOpenChange={handleDialogOpenChange}
          view={dialogView}
          onSetView={setDialogView}
          canStartBlank={canStartBlankWorkflow}
          canUseAgentCreate={canUseAgentCreateForWorkflow}
          canPickTemplate={canPickWorkflowTemplate}
          onStartBlank={handleStartBlankWorkflow}
          onUseAgentCreate={handleCreateWorkflowWithAgentCreate}
          onPickTemplate={handlePickWorkflowTemplate}
        />
      ) : null}
    </div>
  )
}

function BrandGlyph() {
  return (
    <div className="relative">
      <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-card/80 shadow-sm">
        <WorkflowIcon className="h-6 w-6 text-foreground" />
      </div>
    </div>
  )
}
