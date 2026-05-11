import { useMemo, useState } from 'react'
import {
  ArrowLeft,
  Bot,
  ChevronRight,
  Copy,
  Play,
  Plus,
  Sparkles,
  Workflow as WorkflowIcon,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import type {
  AgentRefDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'

import { AgentTemplatePicker } from './agent-template-picker'

interface WorkflowCanvasEmptyStateProps {
  onCreateAgent?: () => void
  onCreateAgentFromTemplate?: (ref: AgentRefDto) => void
  onBrowseWorkflows?: () => void
  templates?: WorkflowAgentSummaryDto[]
  templatesLoading?: boolean
  templatesError?: Error | null
  className?: string
}

interface Action {
  icon: typeof WorkflowIcon
  label: string
  onSelect?: () => void
  comingSoon?: boolean
}

type DialogView = 'choice' | 'templates'

export function WorkflowCanvasEmptyState({
  onCreateAgent,
  onCreateAgentFromTemplate,
  onBrowseWorkflows,
  templates = [],
  templatesLoading = false,
  templatesError = null,
  className,
}: WorkflowCanvasEmptyStateProps) {
  const [dialogOpen, setDialogOpen] = useState(false)
  const [dialogView, setDialogView] = useState<DialogView>('choice')

  const templatesAvailable = Boolean(onCreateAgentFromTemplate)

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
    setDialogOpen(true)
  }

  function handleStartBlank() {
    setDialogOpen(false)
    onCreateAgent?.()
  }

  function handlePickTemplate(ref: AgentRefDto) {
    setDialogOpen(false)
    onCreateAgentFromTemplate?.(ref)
  }

  const createAgentHandler = templatesAvailable
    ? openCreateAgentDialog
    : onCreateAgent

  const actions: Action[] = [
    { icon: Bot, label: 'Create agent', onSelect: createAgentHandler },
    { icon: Plus, label: 'Create workflow', comingSoon: true },
    ...(onBrowseWorkflows
      ? [{ icon: Play, label: 'Run an existing workflow', comingSoon: true }]
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
          Start with a <span className="text-primary">workflow</span>
        </h2>
        <p className="mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          Compose agents into a workflow on the canvas, or define a new agent to use as a building block.
        </p>

        <ul className="relative mt-8 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          {actions.map((action) => {
            const disabled = action.comingSoon || !action.onSelect
            return (
              <li key={action.label}>
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
              </li>
            )
          })}
        </ul>
      </div>

      {templatesAvailable ? (
        <CreateAgentDialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          view={dialogView}
          onSetView={setDialogView}
          templates={visibleTemplates}
          templatesLoading={templatesLoading}
          templatesError={templatesError}
          onStartBlank={handleStartBlank}
          onPickTemplate={handlePickTemplate}
        />
      ) : null}
    </div>
  )
}

interface CreateAgentDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  view: DialogView
  onSetView: (view: DialogView) => void
  templates: WorkflowAgentSummaryDto[]
  templatesLoading: boolean
  templatesError: Error | null
  onStartBlank: () => void
  onPickTemplate: (ref: AgentRefDto) => void
}

function CreateAgentDialog({
  open,
  onOpenChange,
  view,
  onSetView,
  templates,
  templatesLoading,
  templatesError,
  onStartBlank,
  onPickTemplate,
}: CreateAgentDialogProps) {
  const isChoice = view === 'choice'
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-[460px]">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-32 bg-gradient-to-b from-primary/[0.06] to-transparent"
        />

        <div className="relative px-6 pb-2 pt-6">
          <DialogHeader className="space-y-2">
            <div className="flex items-center gap-2.5">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
                <Sparkles className="h-4 w-4" />
              </span>
              <DialogTitle className="text-[15px]">Create agent</DialogTitle>
            </div>
            <DialogDescription className="text-[12.5px] leading-relaxed">
              {isChoice
                ? 'Start from scratch or copy an existing agent as a template.'
                : 'Templates open on the canvas with “(copy)” appended so you can edit freely.'}
            </DialogDescription>
          </DialogHeader>
        </div>

        <div className="relative px-6 pb-5 mt-4">
          {isChoice ? (
            <div className="flex flex-col gap-2">
              <ChoiceCard
                icon={<Plus className="h-4 w-4" />}
                title="New agent"
                description="Open the canvas with an empty agent header."
                onClick={onStartBlank}
              />
              <ChoiceCard
                icon={<Copy className="h-4 w-4" />}
                title="From template"
                description="Copy a built-in or saved agent and tweak it."
                onClick={() => onSetView('templates')}
              />
            </div>
          ) : (
            <AgentTemplatePicker
              agents={templates}
              loading={templatesLoading}
              error={templatesError}
              onSelectTemplate={onPickTemplate}
              onStartBlank={onStartBlank}
              headless
              hideStartBlank
              className="max-w-none gap-0 rounded-lg border-0 bg-transparent p-0 shadow-none"
            />
          )}
        </div>

        <DialogFooter className="border-t border-border/60 bg-secondary/20 px-6 py-3 sm:justify-between">
          {isChoice ? (
            <>
              <p className="hidden text-[11px] text-muted-foreground/70 sm:block">
                Agents become reusable building blocks across workflows.
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onOpenChange(false)}
                className="text-muted-foreground hover:text-foreground"
              >
                Cancel
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onSetView('choice')}
                className="text-muted-foreground hover:text-foreground"
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                Back
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onOpenChange(false)}
                className="text-muted-foreground hover:text-foreground"
              >
                Cancel
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface ChoiceCardProps {
  icon: React.ReactNode
  title: string
  description: string
  onClick: () => void
}

function ChoiceCard({ icon, title, description, onClick }: ChoiceCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card/40 px-3.5 py-3 text-left transition-all',
        'hover:border-primary/40 hover:bg-primary/[0.04]',
        'focus-visible:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30',
      )}
    >
      <span
        className={cn(
          'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors',
          'border-border/60 bg-secondary/60 text-muted-foreground',
          'group-hover:border-primary/40 group-hover:bg-primary/10 group-hover:text-primary',
        )}
      >
        {icon}
      </span>
      <div className="min-w-0 flex-1 space-y-0.5">
        <div className="text-[13px] font-medium text-foreground">{title}</div>
        <div className="text-[11.5px] leading-snug text-muted-foreground">{description}</div>
      </div>
      <ChevronRight
        className={cn(
          'h-4 w-4 shrink-0 text-muted-foreground/50 transition-all',
          'group-hover:translate-x-0.5 group-hover:text-primary',
        )}
      />
    </button>
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
