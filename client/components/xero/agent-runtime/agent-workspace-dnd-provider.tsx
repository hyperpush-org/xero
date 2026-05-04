"use client"

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core'
import { sortableKeyboardCoordinates } from '@dnd-kit/sortable'

const SESSION_DRAG_PREFIX = 'session-'
export const AGENT_WORKSPACE_DROP_TARGET_ID = 'agent-workspace-drop-target'

export type SessionDragData = {
  type: 'session'
  sessionId: string
  title: string
  projectLabel?: string | null
}

interface PaneSlotInfo {
  id: string
  agentSessionId: string | null
  title: string | null
  projectLabel: string | null
  index: number
}

interface AgentWorkspaceDndProviderProps {
  children: ReactNode
  paneSlots: ReadonlyArray<PaneSlotInfo>
  onReorderPanes: (activePaneId: string, overPaneId: string) => void
  onOpenSessionInNewPane: (
    sessionId: string,
    options?: { atIndex?: number },
  ) => 'opened' | 'focused' | 'rejected-max' | 'noop'
}

function getDragEndPointerPoint(event: DragEndEvent): { x: number; y: number } | null {
  const activatorEvent = event.activatorEvent
  if (
    !('clientX' in activatorEvent) ||
    !('clientY' in activatorEvent) ||
    typeof activatorEvent.clientX !== 'number' ||
    typeof activatorEvent.clientY !== 'number'
  ) {
    return null
  }

  return {
    x: activatorEvent.clientX + event.delta.x,
    y: activatorEvent.clientY + event.delta.y,
  }
}

function isPointInRect(
  point: { x: number; y: number },
  rect: Pick<DOMRect, 'bottom' | 'left' | 'right' | 'top'>,
): boolean {
  return point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom
}

function isDragEndInsideWorkspaceDropTarget(event: DragEndEvent): boolean {
  if (typeof document === 'undefined') {
    return false
  }

  const point = getDragEndPointerPoint(event)
  if (!point) {
    return false
  }

  const target = document.querySelector('[data-agent-workspace-drop-target]')
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return isPointInRect(point, target.getBoundingClientRect())
}

export const AgentWorkspaceDndProvider = memo(function AgentWorkspaceDndProvider({
  children,
  paneSlots,
  onReorderPanes,
  onOpenSessionInNewPane,
}: AgentWorkspaceDndProviderProps) {
  const [activeDrag, setActiveDrag] = useState<
    | {
        kind: 'pane'
        paneId: string
        width: number
        height: number
        title: string
        subtitle: string | null
        cloneHtml: string | null
      }
    | { kind: 'session'; sessionId: string; title: string; subtitle: string | null }
    | null
  >(null)
  const overlayHostRef = useRef<HTMLDivElement | null>(null)

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const handleDragStart = useCallback(
    (event: DragStartEvent) => {
      const { active } = event
      const data = active.data.current as Partial<SessionDragData> | undefined
      if (data?.type === 'session' && data.sessionId) {
        setActiveDrag({
          kind: 'session',
          sessionId: data.sessionId,
          title: data.title ?? 'Session',
          subtitle: data.projectLabel ?? null,
        })
        return
      }
      const paneId = String(active.id)
      const slot = paneSlots.find((p) => p.id === paneId)
      const node =
        typeof document !== 'undefined'
          ? (document.querySelector(
              `[data-pane-id="${CSS.escape(paneId)}"]`,
            ) as HTMLElement | null)
          : null
      const measured = node?.getBoundingClientRect()
      const initialRect = active.rect.current.initial
      const width = measured?.width ?? initialRect?.width ?? 0
      const height = measured?.height ?? initialRect?.height ?? 0
      let cloneHtml: string | null = null
      if (node) {
        const clone = node.cloneNode(true) as HTMLElement
        clone.removeAttribute('data-pane-dragging')
        clone.style.opacity = ''
        clone.style.transform = ''
        clone.style.transition = ''
        clone.style.width = '100%'
        clone.style.height = '100%'
        cloneHtml = clone.outerHTML
      }
      setActiveDrag({
        kind: 'pane',
        paneId,
        width,
        height,
        title: slot?.title ?? 'Session',
        subtitle: slot?.projectLabel ?? null,
        cloneHtml,
      })
    },
    [paneSlots],
  )

  const handleDragCancel = useCallback(() => setActiveDrag(null), [])

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event
      setActiveDrag(null)
      const activeData = active.data.current as Partial<SessionDragData> | undefined
      const result = resolveAgentWorkspaceDragEnd(
        {
          activeId: String(active.id),
          overId: over ? String(over.id) : null,
          activeData,
          droppedOnWorkspace: isDragEndInsideWorkspaceDropTarget(event),
        },
        paneSlots.map((slot) => ({ id: slot.id })),
      )
      if (!result) return
      if (result.kind === 'open-session') {
        onOpenSessionInNewPane(result.sessionId, { atIndex: result.atIndex })
        return
      }
      onReorderPanes(result.activeId, result.overId)
    },
    [onOpenSessionInNewPane, onReorderPanes, paneSlots],
  )

  const overlayContent = useMemo<ReactNode>(() => {
    if (!activeDrag) return null
    if (activeDrag.kind === 'session') {
      return (
        <div className="pointer-events-none flex max-w-xs items-center gap-1.5 rounded-md border border-primary/40 bg-background px-2.5 py-1.5 text-[12.5px] font-semibold text-foreground shadow-lg shadow-black/20">
          <span className="truncate">{activeDrag.title}</span>
        </div>
      )
    }
    if (activeDrag.cloneHtml) {
      return (
        <div
          ref={overlayHostRef}
          className="pointer-events-none rounded-lg shadow-2xl shadow-black/40 ring-1 ring-primary/40"
          style={{ width: activeDrag.width, height: activeDrag.height }}
          dangerouslySetInnerHTML={{ __html: activeDrag.cloneHtml }}
        />
      )
    }
    return (
      <div
        className="pointer-events-none flex flex-col overflow-hidden rounded-lg border border-primary/40 bg-background shadow-2xl shadow-black/30"
        style={{ width: activeDrag.width, height: activeDrag.height }}
      />
    )
  }, [activeDrag])

  useEffect(() => {
    if (!activeDrag || activeDrag.kind !== 'pane') return
    const host = overlayHostRef.current
    if (!host) return
    const cloned = host.firstElementChild as HTMLElement | null
    if (!cloned) return
    cloned.style.width = '100%'
    cloned.style.height = '100%'
    cloned.style.transform = 'none'
    cloned.style.transition = 'none'
  }, [activeDrag])

  return (
    <DndContext
      sensors={sensors}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      {children}
      <DragOverlay
        dropAnimation={null}
        style={
          activeDrag?.kind === 'pane'
            ? { width: activeDrag.width, height: activeDrag.height }
            : undefined
        }
      >
        {overlayContent}
      </DragOverlay>
    </DndContext>
  )
})

export const AGENT_WORKSPACE_SESSION_DRAG_PREFIX = SESSION_DRAG_PREFIX

export type AgentWorkspaceDragEndResult =
  | { kind: 'reorder'; activeId: string; overId: string }
  | { kind: 'open-session'; sessionId: string; atIndex?: number }
  | null

export function resolveAgentWorkspaceDragEnd(
  event: {
    activeId: string
    overId: string | null
    activeData: Partial<SessionDragData> | undefined
    droppedOnWorkspace?: boolean
  },
  paneSlots: ReadonlyArray<{ id: string }>,
): AgentWorkspaceDragEndResult {
  const draggedSessionId = getDraggedSessionId(event.activeId, event.activeData)
  if (draggedSessionId) {
    if (!event.overId && !event.droppedOnWorkspace) {
      return null
    }

    const targetIndex = paneSlots.findIndex((slot) => slot.id === event.overId)
    return {
      kind: 'open-session',
      sessionId: draggedSessionId,
      atIndex: targetIndex >= 0 ? targetIndex : undefined,
    }
  }
  if (!event.overId) return null
  if (event.activeId === event.overId) return null
  return { kind: 'reorder', activeId: event.activeId, overId: event.overId }
}

function getDraggedSessionId(
  activeId: string,
  activeData: Partial<SessionDragData> | undefined,
): string | null {
  if (activeData?.type === 'session' && activeData.sessionId) {
    return activeData.sessionId
  }

  return activeId.startsWith(SESSION_DRAG_PREFIX)
    ? activeId.slice(SESSION_DRAG_PREFIX.length)
    : null
}
