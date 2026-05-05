import { Bot, Plus, Play, Workflow as WorkflowIcon } from 'lucide-react'

import { cn } from '@/lib/utils'

interface WorkflowCanvasEmptyStateProps {
  onCreateWorkflow?: () => void
  onCreateAgent?: () => void
  onBrowseWorkflows?: () => void
  className?: string
}

interface Action {
  icon: typeof WorkflowIcon
  label: string
  onSelect?: () => void
}

export function WorkflowCanvasEmptyState({
  onCreateWorkflow,
  onCreateAgent,
  onBrowseWorkflows,
  className,
}: WorkflowCanvasEmptyStateProps) {
  const actions: Action[] = [
    { icon: Plus, label: 'Create workflow', onSelect: onCreateWorkflow },
    { icon: Bot, label: 'Create agent', onSelect: onCreateAgent },
    ...(onBrowseWorkflows
      ? [{ icon: Play, label: 'Run an existing workflow', onSelect: onBrowseWorkflows }]
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
        onWheel={(event) => event.stopPropagation()}
      >
        <BrandGlyph />

        <h2 className="mt-5 text-2xl font-semibold tracking-tight text-foreground sm:text-[26px]">
          Start with a <span className="text-primary">workflow</span>
        </h2>
        <p className="mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          Compose agents into a workflow on the canvas, or define a new agent to use as a building block.
        </p>

        <ul className="relative mt-8 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          {actions.map((action) => (
            <li key={action.label}>
              <button
                className={cn(
                  'group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors',
                  'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                )}
                onClick={action.onSelect}
                type="button"
              >
                <action.icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-primary" />
                <span className="flex-1 truncate text-[13px] text-foreground/85 group-hover:text-foreground">
                  {action.label}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
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
