"use client"

import { useCallback, useMemo, useRef, useState } from "react"
import { Bot, Plus } from "lucide-react"

import { cn } from "@/lib/utils"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { useSidebarWidthMotion } from "@/lib/sidebar-motion"
import type {
  AgentRuntimeDesktopAdapter,
  AgentRuntimeProps,
} from "@/components/xero/agent-runtime"
import { LiveAgentRuntimeView } from "@/components/xero/agent-runtime/live-agent-runtime"
import type {
  AgentPaneView,
  XeroHighChurnStore,
} from "@/src/features/xero/use-xero-desktop-state"
import type {
  AgentDefinitionSummaryDto,
  AgentSessionView,
} from "@/src/lib/xero-model"

const MIN_WIDTH = 320
const MAX_WIDTH = 720
const DEFAULT_WIDTH = 560
const COMPACT_WIDTH_THRESHOLD = 400
const WIDTH_STORAGE_KEY = "xero.agentDock.width"

function readPersistedWidth(): number | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage?.getItem?.(WIDTH_STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return Math.min(MAX_WIDTH, parsed)
  } catch {
    return null
  }
}

function writePersistedWidth(width: number): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage?.setItem?.(WIDTH_STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable — default next session */
  }
}

export interface AgentDockSidebarProps {
  open: boolean
  agent: AgentPaneView | null
  highChurnStore: XeroHighChurnStore
  sessions: readonly AgentSessionView[]
  selectedSessionId: string | null
  isCreatingSession?: boolean
  onClose: () => void
  onSelectSession: (agentSessionId: string) => void
  onCreateSession: () => void
  desktopAdapter?: AgentRuntimeDesktopAdapter
  accountAvatarUrl?: string | null
  accountLogin?: string | null
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  onOpenAgentManagement?: () => void
  onCreateAgentByHand?: AgentRuntimeProps["onCreateAgentByHand"]
  onStartWorkflowAgentCreate?: AgentRuntimeProps["onStartWorkflowAgentCreate"]
  onOpenSettings?: () => void
  onOpenDiagnostics?: () => void
  onStartLogin?: AgentRuntimeProps["onStartLogin"]
  onStartAutonomousRun?: AgentRuntimeProps["onStartAutonomousRun"]
  onInspectAutonomousRun?: AgentRuntimeProps["onInspectAutonomousRun"]
  onCancelAutonomousRun?: AgentRuntimeProps["onCancelAutonomousRun"]
  onStartRuntimeRun?: AgentRuntimeProps["onStartRuntimeRun"]
  onUpdateRuntimeRunControls?: AgentRuntimeProps["onUpdateRuntimeRunControls"]
  onComposerControlsChange?: AgentRuntimeProps["onComposerControlsChange"]
  onStartRuntimeSession?: AgentRuntimeProps["onStartRuntimeSession"]
  onStopRuntimeRun?: AgentRuntimeProps["onStopRuntimeRun"]
  onSubmitManualCallback?: AgentRuntimeProps["onSubmitManualCallback"]
  onLogout?: AgentRuntimeProps["onLogout"]
  onResolveOperatorAction?: AgentRuntimeProps["onResolveOperatorAction"]
  onResumeOperatorRun?: AgentRuntimeProps["onResumeOperatorRun"]
  onRefreshNotificationRoutes?: AgentRuntimeProps["onRefreshNotificationRoutes"]
  onUpsertNotificationRoute?: AgentRuntimeProps["onUpsertNotificationRoute"]
  onRetryStream?: AgentRuntimeProps["onRetryStream"]
  onCodeUndoApplied?: AgentRuntimeProps["onCodeUndoApplied"]
  agentCreateCanvasIncluded?: AgentRuntimeProps["agentCreateCanvasIncluded"]
  pendingInitialRuntimeAgentId?: AgentRuntimeProps["pendingInitialRuntimeAgentId"]
  onPendingInitialRuntimeAgentIdConsumed?: AgentRuntimeProps["onPendingInitialRuntimeAgentIdConsumed"]
}

export function AgentDockSidebar({
  open,
  agent,
  highChurnStore,
  sessions,
  selectedSessionId,
  isCreatingSession = false,
  onClose,
  onSelectSession,
  onCreateSession,
  ...runtimeProps
}: AgentDockSidebarProps) {
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const targetWidth = open ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { animate: false, isResizing })
  const widthRef = useRef(width)
  widthRef.current = width

  const activeSessions = useMemo(
    () => sessions.filter((session) => session.isActive),
    [sessions],
  )

  const handleResizeStart = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    let latestWidth = startWidth
    const widthUpdates = createFrameCoalescer<number>({ onFlush: setWidth })
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"

    const handleMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX
      latestWidth = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
      widthUpdates.schedule(latestWidth)
    }
    const handleUp = () => {
      widthUpdates.flush()
      window.removeEventListener("pointermove", handleMove)
      window.removeEventListener("pointerup", handleUp)
      window.removeEventListener("pointercancel", handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
      writePersistedWidth(latestWidth)
    }

    window.addEventListener("pointermove", handleMove)
    window.addEventListener("pointerup", handleUp)
    window.addEventListener("pointercancel", handleUp)
  }, [])

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      const next = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, current + delta))
      writePersistedWidth(next)
      return next
    })
  }, [])

  return (
    <aside
      aria-hidden={!open}
      aria-label="Agent dock"
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize agent dock"
        aria-orientation="vertical"
        aria-valuemax={MAX_WIDTH}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div
        className="flex h-full min-w-0 shrink-0 flex-col"
        style={{ width }}
      >
        <div className="flex min-h-0 flex-1 flex-col">
          {open && agent ? (
            <LiveAgentRuntimeView
              {...runtimeProps}
              active={open}
              agent={agent}
              highChurnStore={highChurnStore}
              density={width < COMPACT_WIDTH_THRESHOLD ? "compact" : "comfortable"}
              onCreateSession={onCreateSession}
              isCreatingSession={isCreatingSession}
              inSidebar
              sidebarSessions={activeSessions}
              onSelectSidebarSession={onSelectSession}
              onCloseSidebar={onClose}
            />
          ) : open ? (
            <DockEmptyState
              isCreatingSession={isCreatingSession}
              onCreateSession={onCreateSession}
            />
          ) : null}
        </div>
      </div>
    </aside>
  )
}

function DockEmptyState({
  isCreatingSession,
  onCreateSession,
}: {
  isCreatingSession: boolean
  onCreateSession: () => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 py-8 text-center">
      <Bot className="h-7 w-7 text-muted-foreground" aria-hidden="true" />
      <div className="space-y-1">
        <p className="text-[13px] font-semibold text-foreground">No active session</p>
        <p className="text-[12px] text-muted-foreground">
          Start a new chat to begin working with the agent.
        </p>
      </div>
      <button
        className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-[12px] font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-60"
        disabled={isCreatingSession}
        onClick={onCreateSession}
        type="button"
      >
        <Plus className="h-3.5 w-3.5" />
        New session
      </button>
    </div>
  )
}
