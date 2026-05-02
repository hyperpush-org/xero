'use client'

import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { Plus, Workflow as WorkflowIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { WorkflowPaneView } from '@/src/features/xero/use-xero-desktop-state'

interface PhaseViewProps {
  workflow?: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  onOpenSettings?: () => void
  canStartRun?: boolean
  isStartingRun?: boolean
  onToggleWorkflows?: () => void
  workflowsOpen?: boolean
  onCreateWorkflow?: () => void
}

const BASE_GRID_SIZE = 28
const MIN_ZOOM = 0.25
const MAX_ZOOM = 4

export const PhaseView = memo(function PhaseView(props: PhaseViewProps) {
  const { onToggleWorkflows, workflowsOpen = false, onCreateWorkflow } = props
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

  // Wheel needs to be a native non-passive listener so we can preventDefault
  // and keep the page from scrolling while the user zooms over the canvas.
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
      aria-label="Workflow canvas"
      className={cn(
        'workflow-canvas relative h-full w-full select-none overflow-hidden bg-background touch-none',
        isDragging ? 'cursor-grabbing' : 'cursor-grab',
      )}
      onPointerCancel={endDrag}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      role="presentation"
      style={{
        backgroundImage:
          'radial-gradient(circle, color-mix(in oklab, var(--foreground) 14%, transparent) var(--workflow-dot-size), transparent calc(var(--workflow-dot-size) + 0.5px))',
        backgroundSize: `${gridSize}px ${gridSize}px`,
        backgroundPosition: `${bgX}px ${bgY}px`,
        // CSS custom property for the dot radius so the gradient stops stay in sync.
        ['--workflow-dot-size' as string]: `${dotRadius}px`,
      }}
    >
      {onToggleWorkflows || onCreateWorkflow ? (
        <div
          className="absolute right-2.5 top-2.5 z-10 flex items-center gap-1.5"
          onPointerDown={(event) => event.stopPropagation()}
        >
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
