'use client'

import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { Plus, Workflow as WorkflowIcon, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { WorkflowCanvasEmptyState } from '@/components/xero/workflow-canvas-empty-state'
import { AgentVisualization } from '@/components/xero/workflow-canvas/agent-visualization'
import { Skeleton } from '@/components/ui/skeleton'
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

const BASE_GRID_SIZE = 28
const MIN_ZOOM = 0.25
const MAX_ZOOM = 4

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

  return (
    <div
      aria-label="Workflow canvas"
      className={cn(
        'relative flex h-full w-full select-none flex-col overflow-hidden bg-background',
      )}
      role="presentation"
    >
      {showAgentVisualization ? (
        <AgentVisualization detail={agentDetail!} />
      ) : agentDetailStatus === 'loading' ? (
        <PhaseCanvasFallback>
          <Skeleton className="h-32 w-72" />
          <Skeleton className="h-10 w-48" />
          <Skeleton className="h-10 w-48" />
        </PhaseCanvasFallback>
      ) : agentDetailStatus === 'error' ? (
        <PhaseCanvasFallback>
          <p className="text-sm font-medium text-destructive">
            Failed to load agent details.
          </p>
          {agentDetailError ? (
            <p className="text-xs text-muted-foreground">{agentDetailError.message}</p>
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
        </PhaseCanvasFallback>
      ) : (
        <CanvasEmptyBackground>
          <WorkflowCanvasEmptyState
            onCreateWorkflow={onCreateWorkflow}
            onCreateAgent={onCreateAgent}
            onBrowseWorkflows={
              onToggleWorkflows && !workflowsOpen ? onToggleWorkflows : undefined
            }
          />
        </CanvasEmptyBackground>
      )}

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

function PhaseCanvasFallback({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      {children}
    </div>
  )
}

function CanvasEmptyBackground({ children }: { children: React.ReactNode }) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const [zoom, setZoom] = useState(1)
  const [isDragging, setIsDragging] = useState(false)
  const dragRef = useRef<{
    pointerId: number
    startX: number
    startY: number
    offsetX: number
    offsetY: number
  } | null>(null)

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.currentTarget.setPointerCapture(event.pointerId)
      dragRef.current = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        offsetX: offset.x,
        offsetY: offset.y,
      }
      setIsDragging(true)
    },
    [offset.x, offset.y],
  )

  const handlePointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId) return
    setOffset({
      x: drag.offsetX + (event.clientX - drag.startX),
      y: drag.offsetY + (event.clientY - drag.startY),
    })
  }, [])

  const endDrag = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId) return
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    dragRef.current = null
    setIsDragging(false)
  }, [])

  useEffect(() => {
    const node = containerRef.current
    if (!node) return
    const handleWheel = (event: WheelEvent) => {
      event.preventDefault()
      const rect = node.getBoundingClientRect()
      const cx = event.clientX - rect.left
      const cy = event.clientY - rect.top
      const factor = Math.exp(-event.deltaY * 0.0015)
      setZoom((prevZoom) => {
        const nextZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, prevZoom * factor))
        if (nextZoom === prevZoom) return prevZoom
        const ratio = nextZoom / prevZoom
        setOffset((prevOffset) => ({
          x: cx - (cx - prevOffset.x) * ratio,
          y: cy - (cy - prevOffset.y) * ratio,
        }))
        return nextZoom
      })
    }
    node.addEventListener('wheel', handleWheel, { passive: false })
    return () => {
      node.removeEventListener('wheel', handleWheel)
    }
  }, [])

  const gridSize = BASE_GRID_SIZE * zoom
  const bgX = ((offset.x % gridSize) + gridSize) % gridSize
  const bgY = ((offset.y % gridSize) + gridSize) % gridSize
  const dotRadius = Math.max(0.6, Math.min(1.6, 0.9 * Math.sqrt(zoom)))

  return (
    <div
      ref={containerRef}
      className={cn(
        'workflow-canvas relative flex flex-1 select-none overflow-hidden touch-none',
        isDragging ? 'cursor-grabbing' : 'cursor-grab',
      )}
      onPointerCancel={endDrag}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      style={{
        ['--workflow-grid-size' as string]: `${gridSize}px`,
        ['--workflow-grid-x' as string]: `${bgX}px`,
        ['--workflow-grid-y' as string]: `${bgY}px`,
        ['--workflow-dot-size' as string]: `${dotRadius}px`,
      }}
    >
      <div aria-hidden="true" className="workflow-canvas-grid" />
      {children}
    </div>
  )
}
