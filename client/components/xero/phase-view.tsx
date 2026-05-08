'use client'

import { memo, type ReactNode } from 'react'
import { Bot, Plus, Workflow as WorkflowIcon, X } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { WorkflowCanvasEmptyState } from '@/components/xero/workflow-canvas-empty-state'
import { AgentVisualization } from '@/components/xero/workflow-canvas/agent-visualization'
import { cn } from '@/lib/utils'
import type { WorkflowPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  AgentDetailStatus,
  AgentListStatus,
} from '@/src/features/xero/use-workflow-agent-inspector'
import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'

interface PhaseViewProps {
  workflow?: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  onOpenSettings?: () => void
  canStartRun?: boolean
  isStartingRun?: boolean
  onToggleWorkflows?: () => void
  workflowsOpen?: boolean
  onCreateWorkflow?: () => void
  onCreateAgent?: () => void
  agentDetail?: WorkflowAgentDetailDto | null
  agentDetailStatus?: AgentDetailStatus | AgentListStatus
  agentDetailError?: Error | null
  onClearAgentSelection?: () => void
  onReloadAgentDetail?: () => Promise<void>
}

export const PhaseView = memo(function PhaseView(props: PhaseViewProps) {
  const {
    onToggleWorkflows,
    workflowsOpen = false,
    onCreateWorkflow,
    onCreateAgent,
    agentDetail = null,
    agentDetailStatus = 'idle',
    agentDetailError = null,
    onClearAgentSelection,
    onReloadAgentDetail,
  } = props

  const showAgentVisualization =
    agentDetailStatus === 'ready' && agentDetail !== null
  const selectedAgent = showAgentVisualization ? agentDetail : null
  const selectedAgentHeader = selectedAgent?.header ?? null
  const selectedAgentIsSystem = selectedAgent?.ref.kind === 'built_in'
  const emptyCanvasState = (
    <WorkflowCanvasEmptyState
      onCreateWorkflow={onCreateWorkflow}
      onCreateAgent={onCreateAgent}
      onBrowseWorkflows={
        onToggleWorkflows && !workflowsOpen ? onToggleWorkflows : undefined
      }
    />
  )
  const agentErrorState =
    agentDetailStatus === 'error' ? (
      <AgentDetailErrorState
        error={agentDetailError}
        onClearAgentSelection={onClearAgentSelection}
        onReloadAgentDetail={onReloadAgentDetail}
      />
    ) : null

  return (
    <div
      aria-label="Workflow canvas"
      className={cn(
        'relative flex h-full w-full select-none flex-col overflow-hidden bg-background',
      )}
      role="presentation"
    >
      {showAgentVisualization ? (
        <div
          aria-label="Selected agent"
          className="pointer-events-none absolute left-2.5 top-2.5 z-10 flex h-[30px] max-w-[max(0px,min(34rem,calc(100%_-_18rem)))] items-center gap-2 rounded-md px-2"
        >
          <Bot
            aria-label="Agent"
            role="img"
            className="size-3.5 shrink-0 text-foreground/65"
          />
          <span className="min-w-0 truncate text-[12.5px] font-semibold text-foreground/80">
            {selectedAgentHeader?.displayName}
          </span>
          {selectedAgentIsSystem ? (
            <Badge
              variant="secondary"
              className="h-[18px] rounded px-1.5 py-0 text-[10px] font-semibold text-muted-foreground"
            >
              system
            </Badge>
          ) : null}
          {selectedAgentHeader?.shortLabel &&
          selectedAgentHeader.shortLabel !== selectedAgentHeader.displayName ? (
            <span className="min-w-0 truncate text-[11px] font-medium text-muted-foreground/70">
              {selectedAgentHeader.shortLabel}
            </span>
          ) : null}
        </div>
      ) : null}

      <AgentVisualization
        detail={selectedAgent}
        emptyState={agentErrorState ?? emptyCanvasState}
        emptyStateVisible={!showAgentVisualization && agentDetailStatus !== 'loading'}
      />

      {onToggleWorkflows || onCreateWorkflow || showAgentVisualization ? (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 top-0 z-[5] h-20 bg-gradient-to-b from-background to-transparent"
        />
      ) : null}

      {onToggleWorkflows || onCreateWorkflow || showAgentVisualization ? (
        <div
          className="absolute right-2.5 top-2.5 z-10 flex items-center gap-1.5"
          onPointerDown={(event) => event.stopPropagation()}
        >
          {showAgentVisualization && onClearAgentSelection ? (
            <Button
              type="button"
              aria-label="Close agent inspector"
              onClick={onClearAgentSelection}
              size="sm"
              variant="ghost"
              className={cn(
                'h-[30px] cursor-pointer gap-1 rounded-md bg-transparent px-2 text-[12.5px] font-semibold has-[>svg]:px-2',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <X className="size-3.5" />
              <span>Close</span>
            </Button>
          ) : null}
          {onCreateWorkflow ? (
            <Button
              type="button"
              aria-label="Create workflow"
              onClick={onCreateWorkflow}
              size="sm"
              variant="ghost"
              className={cn(
                'h-[30px] cursor-pointer gap-1 rounded-md bg-transparent px-2 text-[12.5px] font-semibold has-[>svg]:px-2',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <Plus className="size-3.5" />
              <span>Create</span>
            </Button>
          ) : null}
          {onCreateWorkflow && onToggleWorkflows ? (
            <span aria-hidden="true" className="h-3.5 w-px bg-foreground/30" />
          ) : null}
          {onToggleWorkflows ? (
            <Button
              type="button"
              aria-label={workflowsOpen ? 'Close workflows' : 'Open workflows'}
              aria-pressed={workflowsOpen}
              onClick={onToggleWorkflows}
              title="Workflows"
              size="icon-sm"
              variant="ghost"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                workflowsOpen
                  ? 'text-primary hover:bg-transparent hover:text-primary'
                  : 'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <WorkflowIcon className="size-3.5" />
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  )
})

function PhaseCanvasFallback({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full w-full flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      {children}
    </div>
  )
}

function AgentDetailErrorState({
  error,
  onClearAgentSelection,
  onReloadAgentDetail,
}: {
  error?: Error | null
  onClearAgentSelection?: () => void
  onReloadAgentDetail?: () => Promise<void>
}) {
  return (
    <PhaseCanvasFallback>
      <div
        className="pointer-events-auto flex flex-col items-center gap-3"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <p className="text-sm font-medium text-destructive">
          Failed to load agent details.
        </p>
        {error ? (
          <p className="text-xs text-muted-foreground">{error.message}</p>
        ) : null}
        <div className="flex gap-2 pt-2">
          {onReloadAgentDetail ? (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                void onReloadAgentDetail()
              }}
            >
              Retry
            </Button>
          ) : null}
          {onClearAgentSelection ? (
            <Button size="sm" variant="ghost" onClick={onClearAgentSelection}>
              Clear selection
            </Button>
          ) : null}
        </div>
      </div>
    </PhaseCanvasFallback>
  )
}
