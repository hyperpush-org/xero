"use client"

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from 'react'
import { cn } from '@/lib/utils'
import {
  solveLayout,
  type AgentWorkspaceArrangement,
  type SolvedAgentWorkspaceLayout,
} from '@/lib/agent-workspace-layout'

const REFLOW_DEBOUNCE_MS = 120
const REFLOW_TRANSITION_MS = 200
const STACK_MIN_PANE_HEIGHT = 320
const MIN_RESIZED_RATIO = 0.05
const KEYBOARD_RESIZE_STEP = 0.02

export interface PaneGridSlot {
  paneId: string
  isFocused: boolean
  ariaLabel: string
}

export interface PaneGridProps {
  slots: PaneGridSlot[]
  splitterRatios?: Record<string, number[]>
  onSplitterRatiosChange?: (arrangementKey: string, ratios: number[]) => void
  onFocusPane?: (paneId: string) => void
  renderPane: (slot: PaneGridSlot, index: number) => ReactNode
  className?: string
}

interface ContainerSize {
  width: number
  height: number
}

function useDebouncedContainerSize(elementRef: React.RefObject<HTMLDivElement | null>): ContainerSize {
  const [size, setSize] = useState<ContainerSize>({ width: 0, height: 0 })
  const debounceTimerRef = useRef<number | null>(null)

  useEffect(() => {
    const element = elementRef.current
    if (!element || typeof ResizeObserver === 'undefined') {
      return
    }

    const measure = (entry: ResizeObserverEntry | null) => {
      const rect = entry?.contentRect ?? element.getBoundingClientRect()
      const next = { width: rect.width, height: rect.height }
      setSize((current) => {
        if (Math.abs(current.width - next.width) < 0.5 && Math.abs(current.height - next.height) < 0.5) {
          return current
        }
        return next
      })
    }

    measure(null)

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0] ?? null
      if (debounceTimerRef.current !== null) {
        window.clearTimeout(debounceTimerRef.current)
      }
      debounceTimerRef.current = window.setTimeout(() => {
        debounceTimerRef.current = null
        measure(entry)
      }, REFLOW_DEBOUNCE_MS)
    })

    observer.observe(element)

    return () => {
      observer.disconnect()
      if (debounceTimerRef.current !== null) {
        window.clearTimeout(debounceTimerRef.current)
        debounceTimerRef.current = null
      }
    }
  }, [elementRef])

  return size
}

function ratiosToPercentages(ratios: number[], expectedCount: number): number[] {
  if (ratios.length !== expectedCount) {
    return Array.from({ length: expectedCount }, () => 100 / expectedCount)
  }
  const total = ratios.reduce((sum, value) => sum + value, 0)
  if (total <= 0) {
    return Array.from({ length: expectedCount }, () => 100 / expectedCount)
  }
  return ratios.map((value) => (value / total) * 100)
}

interface ResolvedGrid {
  arrangement: AgentWorkspaceArrangement
  columnPercentages: number[]
  rowPercentages: number[]
  fallback: 'stack' | undefined
}

function resolveGrid(solved: SolvedAgentWorkspaceLayout): ResolvedGrid {
  const { arrangement, ratios, fallback } = solved
  if (fallback === 'stack') {
    return {
      arrangement,
      columnPercentages: [100],
      rowPercentages: Array.from({ length: arrangement.rows }, () => 100 / arrangement.rows),
      fallback,
    }
  }

  const columns = ratios.slice(0, arrangement.columns)
  const rows = ratios.slice(arrangement.columns)
  return {
    arrangement,
    columnPercentages: ratiosToPercentages(columns, arrangement.columns),
    rowPercentages: ratiosToPercentages(rows, arrangement.rows),
    fallback: undefined,
  }
}

function buildSplitterRatiosForArrangement(
  arrangement: AgentWorkspaceArrangement,
  columnPercentages: number[],
  rowPercentages: number[],
): number[] {
  return [
    ...columnPercentages.map((value) => value / 100),
    ...rowPercentages.map((value) => value / 100),
  ]
}

interface PaneShellProps {
  slot: PaneGridSlot
  index: number
  /** When true the pane is the only one in the workspace; suppress the chrome frame entirely. */
  isSolo: boolean
  onFocusPane?: (paneId: string) => void
  children: ReactNode
}

const PaneShell = memo(function PaneShell({ slot, isSolo, onFocusPane, children }: PaneShellProps) {
  const handleFocusCapture = useCallback(() => {
    if (!slot.isFocused) {
      onFocusPane?.(slot.paneId)
    }
  }, [onFocusPane, slot.isFocused, slot.paneId])
  const handleMouseDown = useCallback(() => {
    if (!slot.isFocused) {
      onFocusPane?.(slot.paneId)
    }
  }, [onFocusPane, slot.isFocused, slot.paneId])

  if (isSolo) {
    return (
      <div
        role="region"
        aria-label={slot.ariaLabel}
        data-pane-id={slot.paneId}
        data-pane-focused="true"
        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background"
      >
        {children}
      </div>
    )
  }

  return (
    <div
      role="region"
      aria-label={slot.ariaLabel}
      data-pane-id={slot.paneId}
      data-pane-focused={slot.isFocused ? 'true' : 'false'}
      onFocusCapture={handleFocusCapture}
      onMouseDown={handleMouseDown}
      className={cn(
        'flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-background transition-[border-color,box-shadow] duration-200 ease-out',
        slot.isFocused
          ? 'border-primary/40 shadow-[0_0_0_1px_hsl(var(--primary)/0.18)]'
          : 'border-border/60',
      )}
    >
      {children}
    </div>
  )
})

function getPaneGridStyle(
  index: number,
  arrangement: AgentWorkspaceArrangement,
): CSSProperties {
  return {
    gridColumn: (index % arrangement.columns) + 1,
    gridRow: Math.floor(index / arrangement.columns) + 1,
  }
}

function percentagesToRatios(percentages: number[]): number[] {
  return percentages.map((value) => value / 100)
}

function getCumulativePercentages(percentages: number[]): number[] {
  const boundaries: number[] = []
  let total = 0
  for (let index = 0; index < percentages.length - 1; index += 1) {
    total += percentages[index] ?? 0
    boundaries.push(total)
  }
  return boundaries
}

function resizeAdjacentRatios(
  ratios: number[],
  boundaryIndex: number,
  boundaryPosition: number,
): number[] {
  const leftIndex = boundaryIndex
  const rightIndex = boundaryIndex + 1
  const leftStart = ratios[leftIndex] ?? 0
  const rightStart = ratios[rightIndex] ?? 0
  const pairTotal = leftStart + rightStart
  if (pairTotal <= MIN_RESIZED_RATIO * 2) {
    return ratios
  }

  const beforePair = ratios.slice(0, leftIndex).reduce((sum, value) => sum + value, 0)
  const minLeft = beforePair + MIN_RESIZED_RATIO
  const maxLeft = beforePair + pairTotal - MIN_RESIZED_RATIO
  const nextLeft = Math.min(maxLeft, Math.max(minLeft, boundaryPosition)) - beforePair
  const next = [...ratios]
  next[leftIndex] = nextLeft
  next[rightIndex] = pairTotal - nextLeft
  return next
}

function nudgeAdjacentRatios(
  ratios: number[],
  boundaryIndex: number,
  delta: number,
): number[] {
  const boundaryPosition =
    ratios.slice(0, boundaryIndex + 1).reduce((sum, value) => sum + value, 0) + delta
  return resizeAdjacentRatios(ratios, boundaryIndex, boundaryPosition)
}

interface ResizeHandleProps {
  axis: 'column' | 'row'
  boundaryIndex: number
  label: string
  offsetPercent: number
  onResizeStart: (
    axis: 'column' | 'row',
    boundaryIndex: number,
    event: PointerEvent<HTMLDivElement>,
  ) => void
  onResizeKey: (axis: 'column' | 'row', boundaryIndex: number, delta: number) => void
}

function ResizeHandle({
  axis,
  boundaryIndex,
  label,
  offsetPercent,
  onResizeStart,
  onResizeKey,
}: ResizeHandleProps) {
  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      const isColumn = axis === 'column'
      const negativeKey = isColumn ? 'ArrowLeft' : 'ArrowUp'
      const positiveKey = isColumn ? 'ArrowRight' : 'ArrowDown'
      if (event.key !== negativeKey && event.key !== positiveKey) {
        return
      }

      event.preventDefault()
      onResizeKey(
        axis,
        boundaryIndex,
        event.key === positiveKey ? KEYBOARD_RESIZE_STEP : -KEYBOARD_RESIZE_STEP,
      )
    },
    [axis, boundaryIndex, onResizeKey],
  )

  return (
    <div
      aria-label={label}
      aria-orientation={axis === 'column' ? 'vertical' : 'horizontal'}
      className={cn(
        'absolute z-30 rounded-sm bg-transparent transition-colors hover:bg-primary/20 focus-visible:bg-primary/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        axis === 'column'
          ? 'top-0 h-full w-3 -translate-x-1/2 cursor-col-resize'
          : 'left-0 h-3 w-full -translate-y-1/2 cursor-row-resize',
      )}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => onResizeStart(axis, boundaryIndex, event)}
      role="separator"
      style={axis === 'column' ? { left: `${offsetPercent}%` } : { top: `${offsetPercent}%` }}
      tabIndex={0}
    />
  )
}

export const PaneGrid = memo(function PaneGrid({
  slots,
  splitterRatios = {},
  onSplitterRatiosChange,
  onFocusPane,
  renderPane,
  className,
}: PaneGridProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const dragCleanupRef = useRef<(() => void) | null>(null)
  const containerSize = useDebouncedContainerSize(containerRef)

  const solved = useMemo(
    () =>
      solveLayout({
        paneCount: slots.length,
        availableWidth: containerSize.width,
        availableHeight: containerSize.height,
        userLayout: splitterRatios,
      }),
    [slots.length, containerSize.width, containerSize.height, splitterRatios],
  )
  const grid = useMemo(() => resolveGrid(solved), [solved])

  const handleResizeKey = useCallback(
    (axis: 'column' | 'row', boundaryIndex: number, delta: number) => {
      if (grid.fallback === 'stack' || !onSplitterRatiosChange) return

      const columnRatios = percentagesToRatios(grid.columnPercentages)
      const rowRatios = percentagesToRatios(grid.rowPercentages)
      const nextColumns =
        axis === 'column' ? nudgeAdjacentRatios(columnRatios, boundaryIndex, delta) : columnRatios
      const nextRows =
        axis === 'row' ? nudgeAdjacentRatios(rowRatios, boundaryIndex, delta) : rowRatios
      onSplitterRatiosChange(
        grid.arrangement.key,
        buildSplitterRatiosForArrangement(
          grid.arrangement,
          nextColumns.map((value) => value * 100),
          nextRows.map((value) => value * 100),
        ),
      )
    },
    [grid, onSplitterRatiosChange],
  )

  const handleResizeStart = useCallback(
    (axis: 'column' | 'row', boundaryIndex: number, event: PointerEvent<HTMLDivElement>) => {
      if (grid.fallback === 'stack' || !onSplitterRatiosChange) return

      event.preventDefault()
      event.currentTarget.setPointerCapture?.(event.pointerId)
      dragCleanupRef.current?.()

      const element = containerRef.current
      const rect = element?.getBoundingClientRect()
      if (!element || !rect || rect.width <= 0 || rect.height <= 0) {
        return
      }

      const startColumnRatios = percentagesToRatios(grid.columnPercentages)
      const startRowRatios = percentagesToRatios(grid.rowPercentages)
      const applyDrag = (clientX: number, clientY: number) => {
        const columnRatios =
          axis === 'column'
            ? resizeAdjacentRatios(
                startColumnRatios,
                boundaryIndex,
                (clientX - rect.left) / rect.width,
              )
            : startColumnRatios
        const rowRatios =
          axis === 'row'
            ? resizeAdjacentRatios(
                startRowRatios,
                boundaryIndex,
                (clientY - rect.top) / rect.height,
              )
            : startRowRatios

        onSplitterRatiosChange(
          grid.arrangement.key,
          buildSplitterRatiosForArrangement(
            grid.arrangement,
            columnRatios.map((value) => value * 100),
            rowRatios.map((value) => value * 100),
          ),
        )
      }

      const handlePointerMove = (moveEvent: globalThis.PointerEvent) => {
        applyDrag(moveEvent.clientX, moveEvent.clientY)
      }
      const handlePointerUp = () => {
        dragCleanupRef.current?.()
      }
      const cleanup = () => {
        window.removeEventListener('pointermove', handlePointerMove)
        window.removeEventListener('pointerup', handlePointerUp)
        window.removeEventListener('pointercancel', handlePointerUp)
        dragCleanupRef.current = null
      }
      dragCleanupRef.current = cleanup
      window.addEventListener('pointermove', handlePointerMove)
      window.addEventListener('pointerup', handlePointerUp)
      window.addEventListener('pointercancel', handlePointerUp)
    },
    [grid, onSplitterRatiosChange],
  )

  useEffect(() => {
    return () => {
      dragCleanupRef.current?.()
    }
  }, [])

  if (slots.length === 0) {
    return <div ref={containerRef} className={cn('flex min-h-0 min-w-0 flex-1', className)} />
  }

  if (grid.fallback === 'stack') {
    return (
      <div
        ref={containerRef}
        className={cn(
          'flex min-h-0 min-w-0 flex-1',
          slots.length > 1 ? 'overflow-y-auto scrollbar-thin' : '',
          className,
        )}
      >
        <div className={cn('flex w-full flex-col gap-2', slots.length > 1 ? 'p-2' : '')}>
          {slots.map((slot, index) => (
            <div
              key={slot.paneId}
              className="flex flex-col"
              style={{ minHeight: STACK_MIN_PANE_HEIGHT }}
            >
              <PaneShell
                slot={slot}
                index={index}
                isSolo={slots.length === 1}
                onFocusPane={onFocusPane}
              >
                {renderPane(slot, index)}
              </PaneShell>
            </div>
          ))}
        </div>
      </div>
    )
  }

  const { arrangement, columnPercentages, rowPercentages } = grid
  const columnBoundaries = getCumulativePercentages(columnPercentages)
  const rowBoundaries = getCumulativePercentages(rowPercentages)
  const gridStyle: CSSProperties = {
    contain: 'layout',
    display: 'grid',
    gridTemplateColumns: columnPercentages.map((value) => `${value}fr`).join(' '),
    gridTemplateRows: rowPercentages.map((value) => `${value}fr`).join(' '),
    transition: `grid-template-columns ${REFLOW_TRANSITION_MS}ms ease, grid-template-rows ${REFLOW_TRANSITION_MS}ms ease`,
  }

  return (
    <div
      ref={containerRef}
      className={cn(
        'relative min-h-0 min-w-0 flex-1',
        slots.length > 1 ? 'p-2' : '',
        className,
      )}
    >
      <div
        className="h-full w-full gap-1"
        style={gridStyle}
      >
        {slots.map((slot, index) => (
          <div
            key={slot.paneId}
            className="flex min-h-0 min-w-0"
            style={getPaneGridStyle(index, arrangement)}
          >
            <PaneShell
              slot={slot}
              index={index}
              isSolo={slots.length === 1}
              onFocusPane={onFocusPane}
            >
              {renderPane(slot, index)}
            </PaneShell>
          </div>
        ))}
      </div>
      {slots.length > 1
        ? columnBoundaries.map((offsetPercent, boundaryIndex) => (
            <ResizeHandle
              key={`column-${boundaryIndex}`}
              axis="column"
              boundaryIndex={boundaryIndex}
              label={`Resize columns around agent pane ${boundaryIndex + 1}`}
              offsetPercent={offsetPercent}
              onResizeStart={handleResizeStart}
              onResizeKey={handleResizeKey}
            />
          ))
        : null}
      {slots.length > 1
        ? rowBoundaries.map((offsetPercent, boundaryIndex) => (
            <ResizeHandle
              key={`row-${boundaryIndex}`}
              axis="row"
              boundaryIndex={boundaryIndex}
              label={`Resize rows around agent pane row ${boundaryIndex + 1}`}
              offsetPercent={offsetPercent}
              onResizeStart={handleResizeStart}
              onResizeKey={handleResizeKey}
            />
          ))
        : null}
    </div>
  )
})
