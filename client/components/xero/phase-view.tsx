'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { Plus, Workflow as WorkflowIcon } from 'lucide-react'
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

export function PhaseView(props: PhaseViewProps) {
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
          className="absolute right-3 top-3 z-10 flex items-center gap-2"
          onPointerDown={(event) => event.stopPropagation()}
        >
          {onCreateWorkflow ? (
            <button
              type="button"
              aria-label="New workflow"
              onClick={onCreateWorkflow}
              className={cn(
                'inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md bg-transparent px-2.5 text-[13px] font-semibold transition-colors',
                'text-foreground/70 hover:text-foreground',
              )}
            >
              <Plus className="h-4 w-4" />
              <span>New Workflow</span>
            </button>
          ) : null}
          {onCreateWorkflow && onToggleWorkflows ? (
            <span aria-hidden="true" className="h-4 w-px bg-foreground/30" />
          ) : null}
          {onToggleWorkflows ? (
            <button
              type="button"
              aria-label={workflowsOpen ? 'Close workflows' : 'Open workflows'}
              aria-pressed={workflowsOpen}
              onClick={onToggleWorkflows}
              title="Workflows"
              className={cn(
                'inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-md bg-transparent transition-colors',
                workflowsOpen
                  ? 'text-primary'
                  : 'text-foreground/70 hover:text-foreground',
              )}
            >
              <WorkflowIcon className="h-4 w-4" />
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
