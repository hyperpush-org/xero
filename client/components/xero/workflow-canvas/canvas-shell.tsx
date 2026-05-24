'use client'

import {
  useCallback,
  useEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
} from 'react'
import {
  ControlButton,
  Controls,
  useOnViewportChange,
  useReactFlow,
  type Viewport,
} from '@xyflow/react'
import {
  Lock,
  Magnet,
  Maximize,
  RotateCcw,
  Unlock,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

export const AGENT_CANVAS_EMPTY_VIEWPORT = { x: 0, y: 0, zoom: 0.72 } as const
export const AGENT_CANVAS_CONTROL_ZOOM_TRANSITION_MS = 180
export const AGENT_CANVAS_CONTROL_ICON_CLASS = 'h-5 w-5'
export const AGENT_CANVAS_DOT_GRID_GAP = 32
export const AGENT_CANVAS_SNAP_GRID_SIZE = AGENT_CANVAS_DOT_GRID_GAP / 2
export const AGENT_CANVAS_SNAP_GRID: [number, number] = [
  AGENT_CANVAS_SNAP_GRID_SIZE,
  AGENT_CANVAS_SNAP_GRID_SIZE,
]

const DOT_COORD_PRECISION = 10
const DOT_ZOOM_PRECISION = 1000

interface DotViewportElement extends HTMLElement {
  __agentDotTransform?: string
  __agentDotZoomKey?: string
}

export interface AgentCanvasControlItem {
  key?: string
  label: string
  title?: string
  disabled?: boolean
  pressed?: boolean
  style?: CSSProperties
  onClick: () => void
  children: ReactNode
}

export interface AgentCanvasControlsProps {
  showLayoutControls?: boolean
  layoutControlsDisabled?: boolean
  locked?: boolean
  snapToGrid?: boolean
  extraControls?: readonly AgentCanvasControlItem[]
  onZoomIn?: () => void
  onZoomOut?: () => void
  onFitView?: () => void
  onToggleLock?: () => void
  onToggleSnapToGrid?: () => void
  onResetLayout?: () => void
}

function applyDotViewport(element: HTMLElement | null, viewport: Viewport): void {
  if (!element) return
  const dotElement = element as DotViewportElement
  const zoom = Number.isFinite(viewport.zoom) && viewport.zoom > 0 ? viewport.zoom : 1
  const roundedZoom = Math.max(
    1 / DOT_ZOOM_PRECISION,
    Math.round(zoom * DOT_ZOOM_PRECISION) / DOT_ZOOM_PRECISION,
  )
  const scaledGap = Math.max(1, AGENT_CANVAS_DOT_GRID_GAP * zoom)
  const dotX = `${Math.round((viewport.x % scaledGap) * DOT_COORD_PRECISION) / DOT_COORD_PRECISION}px`
  const dotY = `${Math.round((viewport.y % scaledGap) * DOT_COORD_PRECISION) / DOT_COORD_PRECISION}px`
  const transform = `translate3d(${dotX}, ${dotY}, 0) scale(${roundedZoom})`
  if (dotElement.__agentDotTransform !== transform) {
    dotElement.__agentDotTransform = transform
    dotElement.style.transform = transform
  }

  const zoomKey = String(Math.round(zoom * DOT_ZOOM_PRECISION))
  if (dotElement.__agentDotZoomKey !== zoomKey) {
    dotElement.__agentDotZoomKey = zoomKey
    const size = `calc(${100 / roundedZoom}% + ${AGENT_CANVAS_DOT_GRID_GAP * 2}px)`
    dotElement.style.width = size
    dotElement.style.height = size
  }
}

export function AgentCanvasDots() {
  const ref = useRef<HTMLDivElement | null>(null)
  const reactFlow = useReactFlow()
  const pendingViewportRef = useRef<Viewport | null>(null)
  const pendingFrameRef = useRef<number | null>(null)

  const flushDots = useCallback(() => {
    pendingFrameRef.current = null
    const viewport = pendingViewportRef.current
    pendingViewportRef.current = null
    if (viewport) applyDotViewport(ref.current, viewport)
  }, [])

  const updateDots = useCallback(
    (viewport: Viewport) => {
      pendingViewportRef.current = viewport
      if (pendingFrameRef.current !== null) return

      pendingFrameRef.current = -1
      const frame = window.requestAnimationFrame(flushDots)
      if (pendingFrameRef.current === -1) {
        pendingFrameRef.current = frame
      }
    },
    [flushDots],
  )

  useEffect(
    () => () => {
      if (pendingFrameRef.current !== null && pendingFrameRef.current !== -1) {
        window.cancelAnimationFrame(pendingFrameRef.current)
      }
      pendingFrameRef.current = null
      pendingViewportRef.current = null
    },
    [],
  )

  useEffect(() => {
    updateDots(reactFlow.getViewport())
  }, [reactFlow, updateDots])

  useOnViewportChange({
    onChange: updateDots,
  })

  return <div ref={ref} className="agent-visualization__dots" aria-hidden="true" />
}

export function AgentCanvasControls({
  showLayoutControls = false,
  layoutControlsDisabled = false,
  locked = false,
  snapToGrid = true,
  extraControls,
  onZoomIn,
  onZoomOut,
  onFitView,
  onToggleLock,
  onToggleSnapToGrid,
  onResetLayout,
}: AgentCanvasControlsProps) {
  const reactFlow = useReactFlow()
  const handleZoomIn = useCallback(() => {
    if (onZoomIn) {
      onZoomIn()
      return
    }
    void reactFlow.zoomIn({ duration: AGENT_CANVAS_CONTROL_ZOOM_TRANSITION_MS })
  }, [onZoomIn, reactFlow])
  const handleZoomOut = useCallback(() => {
    if (onZoomOut) {
      onZoomOut()
      return
    }
    void reactFlow.zoomOut({ duration: AGENT_CANVAS_CONTROL_ZOOM_TRANSITION_MS })
  }, [onZoomOut, reactFlow])
  const handleFitView = useCallback(() => {
    if (onFitView) {
      onFitView()
      return
    }
    void reactFlow.fitView()
  }, [onFitView, reactFlow])

  const canShowLayoutControls =
    showLayoutControls && onToggleLock && onToggleSnapToGrid && onResetLayout

  return (
    <Controls
      position="bottom-right"
      showZoom={false}
      showFitView={false}
      showInteractive={false}
      className="agent-canvas-controls"
    >
      <CanvasControlButton
        className="react-flow__controls-zoomin"
        onClick={handleZoomIn}
        label="Zoom in"
      >
        <ZoomIn className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
      </CanvasControlButton>
      <CanvasControlButton
        className="react-flow__controls-zoomout"
        onClick={handleZoomOut}
        label="Zoom out"
      >
        <ZoomOut className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
      </CanvasControlButton>
      <CanvasControlButton
        className="react-flow__controls-fitview"
        onClick={handleFitView}
        label="Fit view"
      >
        <Maximize className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
      </CanvasControlButton>
      {extraControls?.map((item) => (
        <CanvasControlButton
          key={item.key ?? item.label}
          onClick={item.onClick}
          label={item.label}
          tooltip={item.title ?? item.label}
          pressed={item.pressed}
          disabled={item.disabled}
          style={item.style}
        >
          {item.children}
        </CanvasControlButton>
      ))}
      {canShowLayoutControls ? (
        <>
          <CanvasControlButton
            onClick={onToggleLock}
            label={locked ? 'Unlock canvas' : 'Lock canvas'}
            pressed={locked}
            style={locked ? { color: 'var(--primary)' } : undefined}
          >
            {locked ? (
              <Lock className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
            ) : (
              <Unlock className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
            )}
          </CanvasControlButton>
          <CanvasControlButton
            onClick={onToggleSnapToGrid}
            label={snapToGrid ? 'Disable snap to grid' : 'Enable snap to grid'}
            pressed={snapToGrid}
            disabled={layoutControlsDisabled}
            style={
              snapToGrid && !layoutControlsDisabled ? { color: 'var(--primary)' } : undefined
            }
          >
            <Magnet className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
          </CanvasControlButton>
          <CanvasControlButton
            onClick={onResetLayout}
            label="Reset layout"
            disabled={layoutControlsDisabled}
          >
            <RotateCcw className={AGENT_CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
          </CanvasControlButton>
        </>
      ) : null}
    </Controls>
  )
}

function CanvasControlButton({
  children,
  className,
  disabled,
  label,
  onClick,
  pressed,
  style,
  tooltip = label,
}: {
  children: ReactNode
  className?: string
  disabled?: boolean
  label: string
  onClick: () => void
  pressed?: boolean
  style?: CSSProperties
  tooltip?: string
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <ControlButton
          className={cn('agent-canvas-controls__button', className)}
          onClick={onClick}
          aria-label={label}
          aria-pressed={pressed}
          disabled={disabled}
          style={style}
        >
          {children}
        </ControlButton>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={6} className="px-2 py-1 text-[11px]">
        {tooltip}
      </TooltipContent>
    </Tooltip>
  )
}
