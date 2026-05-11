"use client"

import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
} from 'react'
import {
  Archive,
  ArchiveRestore,
  ChevronRight,
  FileText,
  Loader2,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Pin,
  PinOff,
  Plus,
  Search,
  Trash2,
} from 'lucide-react'
import { useDraggable } from '@dnd-kit/core'
import type { SessionDragData } from '@/components/xero/agent-runtime/agent-workspace-dnd-provider'
import { cn } from '@/lib/utils'
import { createFrameCoalescer } from '@/lib/frame-governance'
import { useSidebarWidthMotion } from '@/lib/sidebar-motion'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Toggle } from '@/components/ui/toggle'
import type { AgentSessionView } from '@/src/lib/xero-model'
import type { SessionTranscriptSearchResultSnippetDto } from '@/src/lib/xero-model/session-context'

interface AgentSessionsSidebarProps {
  projectId: string | null
  sessions: readonly AgentSessionView[]
  selectedSessionId: string | null
  onSelectSession: (agentSessionId: string) => void
  onCreateSession: () => void
  onArchiveSession: (agentSessionId: string) => void
  onLoadArchivedSessions?: (projectId: string) => Promise<readonly AgentSessionView[]>
  onRestoreSession?: (agentSessionId: string) => Promise<void>
  onDeleteSession?: (agentSessionId: string) => Promise<void>
  onSearchSessions?: (query: string) => Promise<SessionTranscriptSearchResultSnippetDto[]>
  onOpenSearchResult?: (result: SessionTranscriptSearchResultSnippetDto) => void
  onReadProjectUiState?: (key: string) => Promise<unknown | null>
  onWriteProjectUiState?: (key: string, value: unknown | null) => Promise<void>
  pendingSessionId?: string | null
  isCreating?: boolean
  collapsed?: boolean
  mode?: 'pinned' | 'collapsed'
  peeking?: boolean
  onCollapse?: () => void
  onPin?: () => void
  onRequestPeek?: () => void
  onReleasePeek?: () => void
  /**
   * Map of agentSessionId → pane number (1-based) currently displaying that session.
   * Used to render P2/P3/... chips when multiple panes are open.
   */
  sessionPaneAssignments?: Record<string, number>
}

type ArchivedLoadStatus = 'idle' | 'loading' | 'loaded' | 'error'

const PINNED_SESSIONS_STORAGE_PREFIX = 'xero:pinned-sessions:'
const PINNED_SESSIONS_UI_STATE_KEY = 'agent-sessions.pinned.v1'
const MIN_WIDTH = 220
const DEFAULT_WIDTH = 260
const MAX_WIDTH = 560
const RIGHT_PADDING = 360
const WIDTH_STORAGE_KEY = 'xero.agentSessions.width'
const STRIP_WIDTH = 6
const STRIP_COLLAPSE_GHOST_DURATION_MS = 110
const STRIP_EXPAND_DURATION_MS = 140

export function readPinnedSessionIds(projectId: string | null): Set<string> {
  if (!projectId || typeof window === 'undefined') return new Set()
  try {
    const raw = window.localStorage.getItem(`${PINNED_SESSIONS_STORAGE_PREFIX}${projectId}`)
    if (!raw) return new Set()
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return new Set()
    return new Set(parsed.filter((id): id is string => typeof id === 'string'))
  } catch {
    return new Set()
  }
}

function parsePinnedSessionIds(value: unknown): Set<string> {
  if (!Array.isArray(value)) return new Set()
  return new Set(value.filter((id): id is string => typeof id === 'string'))
}

function writePinnedSessionIds(projectId: string | null, ids: Set<string>) {
  if (!projectId || typeof window === 'undefined') return
  try {
    window.localStorage.setItem(
      `${PINNED_SESSIONS_STORAGE_PREFIX}${projectId}`,
      JSON.stringify([...ids]),
    )
  } catch {
    // ignore storage failures (private mode, quota, etc.)
  }
}

function viewportMaxWidth() {
  if (typeof window === 'undefined') return MAX_WIDTH
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, window.innerWidth - RIGHT_PADDING))
}

function clampWidth(width: number, maxWidth = viewportMaxWidth()) {
  return Math.max(MIN_WIDTH, Math.min(maxWidth, width))
}

function readPersistedWidth(): number | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage?.getItem?.(WIDTH_STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return clampWidth(parsed)
  } catch {
    return null
  }
}

function writePersistedWidth(width: number): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage?.setItem?.(WIDTH_STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable — default next session */
  }
}

type SessionEntryState = 'entering' | 'visible' | 'exiting'

interface SessionEntry {
  session: AgentSessionView
  state: SessionEntryState
}

export const AgentSessionsSidebar = memo(function AgentSessionsSidebar({
  projectId,
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  onArchiveSession,
  onLoadArchivedSessions,
  onRestoreSession,
  onDeleteSession,
  onSearchSessions,
  onOpenSearchResult,
  onReadProjectUiState,
  onWriteProjectUiState,
  pendingSessionId,
  isCreating,
  collapsed = false,
  mode = 'pinned',
  peeking = false,
  onCollapse,
  onPin,
  onRequestPeek,
  onReleasePeek,
  sessionPaneAssignments,
}: AgentSessionsSidebarProps) {
  const archiveSupported = Boolean(
    onLoadArchivedSessions && onRestoreSession && onDeleteSession,
  )
  const isStripMode = mode === 'collapsed' && collapsed
  const showOverlay = isStripMode && peeking
  const activeSessions = useMemo(
    () => sessions.filter((session) => session.isActive),
    [sessions],
  )

  const [pinnedIds, setPinnedIds] = useState<Set<string>>(() =>
    onReadProjectUiState ? new Set() : readPinnedSessionIds(projectId),
  )
  const [width, setWidth] = useState(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<SessionTranscriptSearchResultSnippetDto[]>([])
  const [searchStatus, setSearchStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle')
  const [searchError, setSearchError] = useState<string | null>(null)
  const [optimisticSessionId, setOptimisticSessionId] = useState<string | null>(null)
  const [collapseGhostActive, setCollapseGhostActive] = useState(false)
  const [archivedVisible, setArchivedVisible] = useState(false)
  const [archivedSessions, setArchivedSessions] = useState<readonly AgentSessionView[]>([])
  const [archivedStatus, setArchivedStatus] = useState<ArchivedLoadStatus>('idle')
  const [archivedError, setArchivedError] = useState<string | null>(null)
  const [pendingRestoreId, setPendingRestoreId] = useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null)
  const [archivedActionError, setArchivedActionError] = useState<string | null>(null)
  const targetWidth = isStripMode ? STRIP_WIDTH : collapsed ? 0 : width
  const displayedSelectedSessionId = optimisticSessionId ?? selectedSessionId
  const widthMotion = useSidebarWidthMotion(targetWidth, {
    durationMs: STRIP_EXPAND_DURATION_MS,
    isResizing,
  })
  const islandClassName = isStripMode ? 'sidebar-peek-island' : widthMotion.islandClassName
  const sidebarStyle = isStripMode
    ? { ...widthMotion.style, transition: 'none' }
    : widthMotion.style
  const widthRef = useRef(width)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const wasStripModeRef = useRef(isStripMode)
  const wasCollapsedRef = useRef(collapsed)
  const collapseGhostTimerRef = useRef<number | null>(null)
  widthRef.current = width

  const clearCollapseGhostTimer = useCallback(() => {
    if (collapseGhostTimerRef.current === null) return
    window.clearTimeout(collapseGhostTimerRef.current)
    collapseGhostTimerRef.current = null
  }, [])

  useEffect(() => {
    if (!projectId) {
      setPinnedIds(new Set())
      return
    }

    if (!onReadProjectUiState) {
      setPinnedIds(readPinnedSessionIds(projectId))
      return
    }

    let cancelled = false
    onReadProjectUiState(PINNED_SESSIONS_UI_STATE_KEY)
      .then((value) => {
        if (cancelled) return
        setPinnedIds(parsePinnedSessionIds(value))
      })
      .catch(() => {
        if (cancelled) return
        setPinnedIds(new Set())
      })

    return () => {
      cancelled = true
    }
  }, [onReadProjectUiState, projectId])

  useLayoutEffect(() => {
    const wasStripMode = wasStripModeRef.current
    const wasCollapsed = wasCollapsedRef.current
    wasStripModeRef.current = isStripMode
    wasCollapsedRef.current = collapsed

    clearCollapseGhostTimer()

    if (!wasStripMode && isStripMode && !wasCollapsed) {
      setCollapseGhostActive(true)
      collapseGhostTimerRef.current = window.setTimeout(() => {
        collapseGhostTimerRef.current = null
        setCollapseGhostActive(false)
      }, STRIP_COLLAPSE_GHOST_DURATION_MS)
      return
    }

    setCollapseGhostActive(false)
  }, [clearCollapseGhostTimer, collapsed, isStripMode])

  useEffect(() => clearCollapseGhostTimer, [clearCollapseGhostTimer])

  useEffect(() => {
    if (typeof window === 'undefined') return
    const handleResize = () => {
      const nextMax = viewportMaxWidth()
      setMaxWidth(nextMax)
      setWidth((current) => clampWidth(current, nextMax))
    }
    window.addEventListener('resize', handleResize)
    return () => window.removeEventListener('resize', handleResize)
  }, [])

  useEffect(() => {
    if (!optimisticSessionId) {
      return
    }

    if (
      selectedSessionId === optimisticSessionId ||
      !activeSessions.some((session) => session.agentSessionId === optimisticSessionId)
    ) {
      setOptimisticSessionId(null)
    }
  }, [activeSessions, optimisticSessionId, selectedSessionId])

  const handleSelectSession = useCallback(
    (agentSessionId: string) => {
      setOptimisticSessionId(agentSessionId)
      onSelectSession(agentSessionId)
      if (showOverlay) {
        onReleasePeek?.()
      }
    },
    [onReleasePeek, onSelectSession, showOverlay],
  )

  const handlePreviewSession = useCallback((agentSessionId: string | null) => {
    setOptimisticSessionId(agentSessionId)
  }, [])

  useEffect(() => {
    if (!searchOpen) {
      setSearchResults([])
      setSearchStatus('idle')
      setSearchError(null)
      return
    }
    if (!onSearchSessions) return
    const query = searchQuery.trim()
    if (query.length < 2) {
      setSearchResults([])
      setSearchStatus('idle')
      setSearchError(null)
      return
    }

    let cancelled = false
    setSearchStatus('loading')
    setSearchError(null)
    const timeout = window.setTimeout(() => {
      onSearchSessions(query)
        .then((results) => {
          if (cancelled) return
          setSearchResults(results)
          setSearchStatus('ready')
        })
        .catch((error) => {
          if (cancelled) return
          setSearchResults([])
          setSearchStatus('error')
          setSearchError(error instanceof Error ? error.message : 'Session search failed.')
        })
    }, 220)

    return () => {
      cancelled = true
      window.clearTimeout(timeout)
    }
  }, [onSearchSessions, searchOpen, searchQuery])

  useEffect(() => {
    if (!searchOpen) {
      setSearchQuery('')
      return
    }
    searchInputRef.current?.focus()
  }, [searchOpen])

  const refreshArchivedSessions = useCallback(async () => {
    if (!projectId || !onLoadArchivedSessions) {
      setArchivedSessions([])
      setArchivedStatus('idle')
      return
    }
    setArchivedStatus('loading')
    setArchivedError(null)
    try {
      const loaded = await onLoadArchivedSessions(projectId)
      setArchivedSessions(loaded)
      setArchivedStatus('loaded')
    } catch (error) {
      setArchivedSessions([])
      setArchivedError(
        error instanceof Error ? error.message : 'Failed to load archived sessions.',
      )
      setArchivedStatus('error')
    }
  }, [onLoadArchivedSessions, projectId])

  useEffect(() => {
    setArchivedVisible(false)
  }, [projectId])

  useEffect(() => {
    if (!archivedVisible) {
      setArchivedActionError(null)
      return
    }
    void refreshArchivedSessions()
  }, [archivedVisible, refreshArchivedSessions])

  const handleRestoreArchivedSession = useCallback(
    async (session: AgentSessionView) => {
      if (!onRestoreSession) return
      setPendingRestoreId(session.agentSessionId)
      setArchivedActionError(null)
      try {
        await onRestoreSession(session.agentSessionId)
        setArchivedSessions((prev) =>
          prev.filter((entry) => entry.agentSessionId !== session.agentSessionId),
        )
      } catch (error) {
        setArchivedActionError(
          error instanceof Error ? error.message : 'Failed to restore session.',
        )
      } finally {
        setPendingRestoreId(null)
      }
    },
    [onRestoreSession],
  )

  const handleDeleteArchivedSession = useCallback(async (session: AgentSessionView) => {
    if (!onDeleteSession) return
    const targetId = session.agentSessionId
    setPendingDeleteId(targetId)
    setArchivedActionError(null)
    try {
      await onDeleteSession(targetId)
      setArchivedSessions((prev) =>
        prev.filter((entry) => entry.agentSessionId !== targetId),
      )
    } catch (error) {
      setArchivedActionError(
        error instanceof Error ? error.message : 'Failed to delete session.',
      )
    } finally {
      setPendingDeleteId(null)
    }
  }, [onDeleteSession])

  const handleResizeStart = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (collapsed || event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    const ceiling = viewportMaxWidth()
    let latestWidth = startWidth
    const widthUpdates = createFrameCoalescer<number>({
      onFlush: setWidth,
    })
    setMaxWidth(ceiling)
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const handleMove = (ev: globalThis.PointerEvent) => {
      const delta = ev.clientX - startX
      latestWidth = clampWidth(startWidth + delta, ceiling)
      widthUpdates.schedule(latestWidth)
    }
    const handleUp = () => {
      widthUpdates.flush()
      window.removeEventListener('pointermove', handleMove)
      window.removeEventListener('pointerup', handleUp)
      window.removeEventListener('pointercancel', handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
      writePersistedWidth(latestWidth)
    }

    window.addEventListener('pointermove', handleMove)
    window.addEventListener('pointerup', handleUp)
    window.addEventListener('pointercancel', handleUp)
  }, [collapsed])

  const handleResizeKey = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (collapsed || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setWidth((current) => {
      const delta = event.key === 'ArrowRight' ? step : -step
      const next = clampWidth(current + delta, ceiling)
      writePersistedWidth(next)
      return next
    })
  }, [collapsed])

  const togglePinSession = useCallback(
    (agentSessionId: string) => {
      setPinnedIds((prev) => {
        const next = new Set(prev)
        if (next.has(agentSessionId)) {
          next.delete(agentSessionId)
        } else {
          next.add(agentSessionId)
        }
        if (projectId && onWriteProjectUiState) {
          void onWriteProjectUiState(PINNED_SESSIONS_UI_STATE_KEY, [...next]).catch(() => {})
        } else {
          writePinnedSessionIds(projectId, next)
        }
        return next
      })
    },
    [onWriteProjectUiState, projectId],
  )

  const isFirstSyncRef = useRef(true)
  const lastProjectIdRef = useRef(projectId)
  const [entries, setEntries] = useState<SessionEntry[]>(() =>
    activeSessions.map((session) => ({ session, state: 'visible' as const })),
  )

  useEffect(() => {
    const isFirst = isFirstSyncRef.current
    isFirstSyncRef.current = false
    const isProjectChange = lastProjectIdRef.current !== projectId
    lastProjectIdRef.current = projectId

    if (isProjectChange) {
      setEntries(
        activeSessions.map((session) => ({ session, state: 'visible' as const })),
      )
      return
    }

    setEntries((prevEntries) => {
      const activeBySessionId = new Map(
        activeSessions.map((session) => [session.agentSessionId, session]),
      )
      const seenIds = new Set<string>()

      const next: SessionEntry[] = prevEntries.map((entry) => {
        const id = entry.session.agentSessionId
        seenIds.add(id)
        const fresh = activeBySessionId.get(id)
        if (fresh) {
          if (entry.state === 'exiting') {
            return { session: fresh, state: 'entering' }
          }
          return { session: fresh, state: entry.state }
        }
        return entry.state === 'exiting' ? entry : { ...entry, state: 'exiting' }
      })

      for (const session of activeSessions) {
        if (!seenIds.has(session.agentSessionId)) {
          next.push({ session, state: isFirst ? 'visible' : 'entering' })
        }
      }

      return next
    })
  }, [activeSessions, projectId])

  const handleEnterAnimationEnd = useCallback((agentSessionId: string) => {
    setEntries((prev) =>
      prev.map((entry) =>
        entry.session.agentSessionId === agentSessionId && entry.state === 'entering'
          ? { ...entry, state: 'visible' }
          : entry,
      ),
    )
  }, [])

  const handleExitAnimationEnd = useCallback((agentSessionId: string) => {
    setEntries((prev) =>
      prev.filter(
        (entry) =>
          !(entry.session.agentSessionId === agentSessionId && entry.state === 'exiting'),
      ),
    )
  }, [])

  const pinnedEntries = useMemo(
    () => entries.filter((entry) => pinnedIds.has(entry.session.agentSessionId)),
    [entries, pinnedIds],
  )
  const regularEntries = useMemo(
    () => entries.filter((entry) => !pinnedIds.has(entry.session.agentSessionId)),
    [entries, pinnedIds],
  )

  const renderEntry = (entry: SessionEntry, isPinned: boolean) => (
    <li
      key={entry.session.agentSessionId}
      className={cn(
        entry.state === 'entering' &&
          'animate-in fade-in-0 slide-in-from-right-4 duration-300 ease-out',
        entry.state === 'exiting' &&
          'animate-out fade-out-0 slide-out-to-left-4 fill-mode-forwards duration-300 ease-out pointer-events-none',
      )}
      onAnimationEnd={(event) => {
        if (event.target !== event.currentTarget) return
        if (entry.state === 'entering') {
          handleEnterAnimationEnd(entry.session.agentSessionId)
        } else if (entry.state === 'exiting') {
          handleExitAnimationEnd(entry.session.agentSessionId)
        }
      }}
    >
      <AgentSessionsSidebarItem
        session={entry.session}
        isActive={entry.session.agentSessionId === displayedSelectedSessionId}
        isPending={entry.session.agentSessionId === pendingSessionId}
        isPinned={isPinned}
        onSelectSession={handleSelectSession}
        onPreviewSession={handlePreviewSession}
        onArchiveSession={onArchiveSession}
        onTogglePin={togglePinSession}
        canArchive={entry.state !== 'exiting'}
        paneNumber={sessionPaneAssignments?.[entry.session.agentSessionId] ?? null}
      />
    </li>
  )

  const shouldRenderPanelChildren = !collapsed || showOverlay || collapseGhostActive
  const panelChildren = shouldRenderPanelChildren ? (
    <>
      <div className="flex shrink-0 items-start justify-between gap-2 px-3 pt-2 pb-2">
        <div className="min-w-0">
          <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Sessions
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          {onSearchSessions ? (
            <Toggle
              aria-controls="agent-session-search-panel"
              aria-label="Search sessions"
              className={cn(
                'h-6 w-6 min-w-6 p-0 text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
                'data-[state=on]:bg-primary/10 data-[state=on]:text-primary',
              )}
              onPressedChange={setSearchOpen}
              pressed={searchOpen}
              size="sm"
            >
              <Search className="size-3" />
            </Toggle>
          ) : null}
          {archiveSupported ? (
            <Toggle
              aria-label={archivedVisible ? 'Hide archived sessions' : 'Show archived sessions'}
              className={cn(
                'h-6 w-6 min-w-6 p-0 text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
                'data-[state=on]:bg-primary/10 data-[state=on]:text-primary',
              )}
              onPressedChange={setArchivedVisible}
              pressed={archivedVisible}
              size="sm"
            >
              <Archive className="size-3" />
            </Toggle>
          ) : null}
          <button
            aria-label="New session"
            className={cn(
              'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
              'hover:bg-primary/10 hover:text-primary disabled:cursor-not-allowed disabled:opacity-50',
            )}
            disabled={isCreating}
            onClick={onCreateSession}
            type="button"
          >
            {isCreating ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Plus className="size-3" />
            )}
          </button>
          {showOverlay && onPin ? (
            <button
              aria-label="Pin sessions sidebar"
              className={cn(
                'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
              )}
              onClick={onPin}
              type="button"
            >
              <PanelLeftOpen className="size-3" />
            </button>
          ) : !showOverlay && onCollapse ? (
            <button
              aria-label="Collapse sessions sidebar"
              className={cn(
                'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
              )}
              onClick={onCollapse}
              type="button"
            >
              <PanelLeftClose className="size-3" />
            </button>
          ) : null}
        </div>
      </div>

      {onSearchSessions && searchOpen ? (
        <div id="agent-session-search-panel" className="shrink-0 px-3 pb-2">
            <label className="sr-only" htmlFor="agent-session-search">
              Search sessions
            </label>
            <div className="relative">
              <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                id="agent-session-search"
                ref={searchInputRef}
                type="search"
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder="Search sessions"
                className="h-8 pl-7 text-xs"
              />
            </div>
            {searchQuery.trim().length >= 2 ? (
              <div className="mt-2 max-h-48 overflow-y-auto rounded-md border border-border/70 bg-background/80 p-1 scrollbar-thin">
                {searchStatus === 'loading' ? (
                  <div className="flex items-center gap-2 px-2 py-2 text-[11px] text-muted-foreground">
                    <Loader2 className="h-3 w-3 animate-spin" />
                    Searching…
                  </div>
                ) : searchStatus === 'error' ? (
                  <div className="px-2 py-2 text-[11px] leading-5 text-destructive">{searchError}</div>
                ) : searchResults.length > 0 ? (
                  searchResults.map((result) => (
                    <button
                      key={result.resultId}
                      type="button"
                      className="flex w-full items-start gap-2 rounded px-2 py-2 text-left transition-colors hover:bg-secondary/60"
                      onClick={() => {
                        onOpenSearchResult?.(result)
                        setSearchQuery('')
                      }}
                    >
                      <FileText className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1 text-[11px] font-medium text-foreground">
                          <span className="truncate">{result.runId}</span>
                          {result.archived ? (
                            <span className="rounded bg-secondary px-1 text-[9px] uppercase text-muted-foreground">
                              archived
                            </span>
                          ) : null}
                        </span>
                        <span className="mt-1 line-clamp-2 block text-[10.5px] leading-4 text-muted-foreground">
                          {result.snippet}
                        </span>
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="px-2 py-2 text-[11px] text-muted-foreground">No matches</div>
                )}
              </div>
            ) : null}
          </div>
        ) : null}

        <div className="flex-1 overflow-y-auto border-t border-border/60 scrollbar-thin">
          {entries.length === 0 && !archivedVisible ? (
            <div className="px-3 py-5 text-center text-[11px] leading-relaxed text-muted-foreground/80">
              No sessions yet. Start a new chat to begin.
            </div>
          ) : (
            <>
              {pinnedEntries.length > 0 ? (
                <div className="flex flex-col">
                  <SidebarSectionHeader label="Pinned" />
                  <ul className="flex flex-col px-1.5 pb-1.5">
                    {pinnedEntries.map((entry) => renderEntry(entry, true))}
                  </ul>
                </div>
              ) : null}
              {regularEntries.length > 0 ? (
                <div className="flex flex-col">
                  {pinnedEntries.length > 0 ? (
                    <SidebarSectionHeader label="Sessions" />
                  ) : null}
                  <ul
                    className={cn(
                      'flex flex-col px-1.5 pb-1.5',
                      pinnedEntries.length === 0 && 'pt-1.5',
                    )}
                  >
                    {regularEntries.map((entry) => renderEntry(entry, false))}
                  </ul>
                </div>
              ) : null}
              {archivedVisible ? (
                <div className="flex flex-col">
                  <SidebarSectionHeader label="Archived" />
                  {archivedStatus === 'loading' && archivedSessions.length === 0 ? (
                    <div className="flex items-center gap-2 px-3 py-3 text-[11px] text-muted-foreground">
                      <Loader2 className="h-3 w-3 animate-spin" />
                      Loading archived sessions…
                    </div>
                  ) : archivedStatus === 'error' ? (
                    <div className="flex flex-col gap-1.5 px-3 py-3 text-[11px] text-destructive">
                      <span>{archivedError ?? 'Failed to load archived sessions.'}</span>
                      <button
                        className="self-start rounded-md border border-border/70 bg-background px-2 py-1 text-[11px] font-medium text-foreground transition-colors hover:bg-secondary/60"
                        onClick={() => void refreshArchivedSessions()}
                        type="button"
                      >
                        Retry
                      </button>
                    </div>
                  ) : archivedSessions.length === 0 ? (
                    <div className="px-3 py-3 text-[11px] leading-relaxed text-muted-foreground/80">
                      No archived sessions.
                    </div>
                  ) : (
                    <ul className="flex flex-col px-1.5 pb-1.5">
                      {archivedSessions.map((session) => (
                        <li key={session.agentSessionId}>
                          <AgentArchivedSessionsSidebarItem
                            session={session}
                            isRestoring={pendingRestoreId === session.agentSessionId}
                            isDeleting={pendingDeleteId === session.agentSessionId}
                            isAnyActionPending={
                              pendingRestoreId !== null || pendingDeleteId !== null
                            }
                            onRestore={handleRestoreArchivedSession}
                            onDelete={handleDeleteArchivedSession}
                          />
                        </li>
                      ))}
                    </ul>
                  )}
                  {archivedActionError ? (
                    <div className="px-3 pb-2 text-[11px] text-destructive">
                      {archivedActionError}
                    </div>
                  ) : null}
                </div>
              ) : null}
            </>
          )}
      </div>
    </>
  ) : null

  return (
    <aside
      aria-hidden={!isStripMode && !showOverlay && collapsed}
      className={cn(
        islandClassName,
        'group/sidebar relative flex shrink-0 flex-col bg-sidebar',
        isStripMode ? 'overflow-visible' : 'overflow-hidden',
        isStripMode || (collapsed && !showOverlay) ? 'border-r-0' : 'border-r border-border',
        isStripMode && 'z-40',
      )}
      inert={!isStripMode && !showOverlay && collapsed ? true : undefined}
      style={sidebarStyle}
    >
      {!collapsed ? (
        <div
          aria-label="Resize sessions sidebar"
          aria-orientation="vertical"
          aria-valuemax={maxWidth}
          aria-valuemin={MIN_WIDTH}
          aria-valuenow={width}
          className={cn(
            'absolute inset-y-0 -right-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors',
            'hover:bg-primary/30',
            isResizing && 'bg-primary/40',
          )}
          onKeyDown={handleResizeKey}
          onPointerDown={handleResizeStart}
          role="separator"
          tabIndex={0}
        />
      ) : null}

      {isStripMode ? (
        <>
          <div
            aria-label="Show sessions sidebar"
            className={cn(
              'absolute inset-y-0 left-0 z-10 cursor-pointer border-r border-border/60 transition-colors',
              'hover:bg-primary/25',
              showOverlay && 'bg-primary/15',
            )}
            onClick={() => onPin?.()}
            onPointerEnter={onRequestPeek}
            onPointerLeave={onReleasePeek}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault()
                onPin?.()
              }
            }}
            role="button"
            style={{ width: STRIP_WIDTH }}
            tabIndex={0}
          />
          {onPin ? (
            <button
              aria-label="Expand sessions sidebar"
              className={cn(
                'absolute top-1/2 z-50 flex h-7 w-5 -translate-y-1/2 items-center justify-center',
                'rounded-r-md border border-l-0 border-border bg-sidebar text-muted-foreground shadow-sm',
                'opacity-100 transition-colors duration-150 pointer-events-auto',
                'hover:bg-primary/15 hover:text-primary',
              )}
              onClick={(event) => {
                event.stopPropagation()
                onPin?.()
              }}
              style={{ left: STRIP_WIDTH }}
              type="button"
            >
              <ChevronRight className="h-3 w-3" />
            </button>
          ) : null}
        </>
      ) : null}

      {showOverlay ? (
        <div
          className={cn(
            'absolute top-0 bottom-0 z-30 flex flex-col border-r border-border bg-sidebar shadow-2xl',
            'animate-in fade-in-0 slide-in-from-left-2 duration-150',
          )}
          onPointerEnter={(event) => {
            event.stopPropagation()
            onRequestPeek?.()
          }}
          onPointerLeave={(event) => {
            event.stopPropagation()
            onReleasePeek?.()
          }}
          style={{ left: STRIP_WIDTH, width }}
        >
          {panelChildren}
        </div>
      ) : null}

      {collapseGhostActive && isStripMode && !showOverlay ? (
        <div
          aria-hidden="true"
          className="sessions-collapse-ghost pointer-events-none absolute inset-y-0 left-0 z-30 flex flex-col border-r border-border bg-sidebar shadow-2xl"
          data-session-collapse-ghost="true"
          inert
          style={{
            animationDuration: `${STRIP_COLLAPSE_GHOST_DURATION_MS}ms`,
            width,
          }}
        >
          {panelChildren}
        </div>
      ) : null}

      {!isStripMode ? (
        <div className="flex h-full shrink-0 flex-col" style={{ width }}>
          {panelChildren}
        </div>
      ) : null}

    </aside>
  )
})

function SidebarSectionHeader({ label }: { label: string }) {
  return (
    <div className="px-3 pt-2 pb-1 text-[9px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70">
      {label}
    </div>
  )
}

export interface AgentSessionsSidebarItemProps {
  session: AgentSessionView
  isActive: boolean
  isPending: boolean
  isPinned: boolean
  canArchive: boolean
  onSelectSession: (agentSessionId: string) => void
  onPreviewSession?: (agentSessionId: string | null) => void
  onArchiveSession: (agentSessionId: string) => void
  onTogglePin: (agentSessionId: string) => void
  compact?: 'icon' | 'list' | 'full'
  /** Pane number (1-based) when this session is loaded in a non-focused pane. */
  paneNumber?: number | null
}

export const AgentSessionsSidebarItem = memo(function AgentSessionsSidebarItem({
  session,
  isActive,
  isPending,
  isPinned,
  canArchive,
  onSelectSession,
  onPreviewSession,
  onArchiveSession,
  onTogglePin,
  compact = 'full',
  paneNumber = null,
}: AgentSessionsSidebarItemProps) {
  const [archiveConfirmationActive, setArchiveConfirmationActive] = useState(false)
  const dragData = useMemo<SessionDragData>(
    () => ({
      type: 'session',
      sessionId: session.agentSessionId,
      title: session.title,
    }),
    [session.agentSessionId, session.title],
  )
  const {
    attributes: dragAttributes,
    listeners: dragListeners,
    setNodeRef: setDragNodeRef,
    isDragging,
  } = useDraggable({
    id: `session-${session.agentSessionId}`,
    data: dragData,
  })
  const suppressNextClickRef = useRef(false)
  const dragWrapperProps = useMemo(() => {
    const { role: _role, ...restAttributes } =
      (dragAttributes as unknown as Record<string, unknown>) ?? {}
    void _role
    return {
      ref: setDragNodeRef,
      ...restAttributes,
      ...((dragListeners ?? {}) as unknown as Record<string, unknown>),
      style: { opacity: isDragging ? 0 : undefined } as React.CSSProperties,
    }
  }, [dragAttributes, dragListeners, isDragging, setDragNodeRef])
  const handlePointerDown = useCallback((event: PointerEvent<HTMLButtonElement>) => {
    if (event.button === 0) {
      suppressNextClickRef.current = false
      onPreviewSession?.(session.agentSessionId)
    }
  }, [onPreviewSession, session.agentSessionId])

  const handleSelectClick = useCallback((event: MouseEvent<HTMLButtonElement>) => {
    if (suppressNextClickRef.current) {
      suppressNextClickRef.current = false
      event.preventDefault()
      event.stopPropagation()
      onPreviewSession?.(null)
      return
    }

    onSelectSession(session.agentSessionId)
  }, [onPreviewSession, onSelectSession, session.agentSessionId])

  useEffect(() => {
    if (!isDragging) {
      return
    }

    suppressNextClickRef.current = true
    onPreviewSession?.(null)
  }, [isDragging, onPreviewSession])

  useEffect(() => {
    setArchiveConfirmationActive(false)
  }, [session.agentSessionId])

  useEffect(() => {
    if (isPending || !canArchive) {
      setArchiveConfirmationActive(false)
    }
  }, [canArchive, isPending])

  if (compact === 'icon') {
    return (
      <div {...dragWrapperProps} className="w-full">
      <button
        aria-label={session.title}
        aria-current={isActive ? 'true' : undefined}
        className={cn(
          'flex w-full items-center justify-center rounded-md p-1 transition-colors',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/60',
        )}
        onClick={handleSelectClick}
        onPointerDown={handlePointerDown}
        title={session.title}
        type="button"
      >
        <span
          className={cn(
            'relative flex h-7 w-7 items-center justify-center rounded-md border transition-colors',
            isActive
              ? 'border-primary/45 bg-primary/15 text-primary'
              : 'border-border/70 bg-secondary/70 text-muted-foreground hover:border-border hover:bg-secondary hover:text-foreground',
          )}
        >
          {isPending ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <MessageSquare className="h-3.5 w-3.5" />
          )}
          {isPinned ? (
            <Pin
              aria-hidden
              className="absolute -right-0.5 -top-0.5 h-2.5 w-2.5 -rotate-45 text-muted-foreground/80"
            />
          ) : null}
        </span>
      </button>
      </div>
    )
  }

  if (compact === 'list') {
    return (
      <div {...dragWrapperProps} className="w-full">
      <button
        aria-current={isActive ? 'true' : undefined}
        className={cn(
          'group flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
        )}
        onClick={handleSelectClick}
        onPointerDown={handlePointerDown}
        title={session.title}
        type="button"
      >
        <span
          className={cn(
            'flex h-5 w-5 shrink-0 items-center justify-center rounded-md border transition-colors',
            isActive
              ? 'border-primary/45 bg-primary/15 text-primary'
              : 'border-border/70 bg-secondary/70 text-muted-foreground group-hover:border-border group-hover:bg-secondary group-hover:text-foreground',
          )}
        >
          {isPending ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <MessageSquare className="h-2.5 w-2.5" />
          )}
        </span>
        <span
          className={cn(
            'min-w-0 flex-1 truncate text-[12px] leading-tight',
            isActive ? 'text-foreground' : 'text-foreground/85 group-hover:text-foreground',
          )}
        >
          {session.title}
        </span>
        {isPinned ? (
          <Pin aria-hidden className="h-2.5 w-2.5 shrink-0 -rotate-45 text-muted-foreground/70" />
        ) : null}
      </button>
      </div>
    )
  }

  const pinActionLabel = isPinned ? `Unpin ${session.title}` : `Pin ${session.title}`
  const isArchiveConfirming = archiveConfirmationActive && canArchive && !isPending
  const archiveActionLabel = isArchiveConfirming
    ? `Confirm archive ${session.title}`
    : `Archive ${session.title}`
  const archiveActionTitle = isArchiveConfirming
    ? `Press again to archive ${session.title}`
    : `Archive ${session.title}`
  const clearArchiveConfirmation = () => setArchiveConfirmationActive(false)
  const stopActionPreview = (event: PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation()
  }

  return (
    <div {...dragWrapperProps} className="group relative">
      <button
        aria-label={session.title}
        aria-current={isActive ? 'true' : undefined}
        className={cn(
          'flex w-full items-center rounded-md px-3 py-2 pr-[72px] text-left transition-colors',
          isArchiveConfirming && 'pr-[112px]',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
        )}
        onClick={handleSelectClick}
        onPointerDown={handlePointerDown}
        title={session.title}
        type="button"
      >
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <span
            className={cn(
              'truncate text-[12.5px] font-medium leading-tight',
              isActive ? 'text-foreground' : 'text-foreground/85 group-hover:text-foreground',
            )}
          >
            {session.title}
          </span>
          {isPinned ? (
            <Pin
              aria-hidden
              className="h-2.5 w-2.5 shrink-0 -rotate-45 text-muted-foreground/70"
            />
          ) : null}
          {paneNumber != null ? (
            <span
              aria-label={`Loaded in pane ${paneNumber}`}
              className="ml-auto inline-flex h-[16px] shrink-0 items-center justify-center rounded-sm border border-border/60 bg-muted/40 px-1 text-[9.5px] font-semibold uppercase tracking-wider text-muted-foreground"
            >
              P{paneNumber}
            </span>
          ) : null}
        </div>
      </button>

      <div
        className={cn(
          'absolute right-1 top-1/2 z-10 flex -translate-y-1/2 items-center gap-0.5',
          'transition-opacity duration-150',
          isActive || isPending
            ? 'opacity-100'
            : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100',
        )}
      >
        <Button
          aria-label={pinActionLabel}
          className="h-6 w-6 p-0 text-muted-foreground hover:bg-secondary hover:text-foreground"
          disabled={isPending}
          onClick={(event) => {
            event.stopPropagation()
            onTogglePin(session.agentSessionId)
          }}
          onPointerDown={stopActionPreview}
          size="icon-sm"
          title={pinActionLabel}
          type="button"
          variant="ghost"
        >
          {isPinned ? (
            <PinOff className="h-3.5 w-3.5" />
          ) : (
            <Pin className="h-3.5 w-3.5" />
          )}
        </Button>
        {canArchive ? (
          <Button
            aria-label={archiveActionLabel}
            className={cn(
              'h-6 p-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive',
              isArchiveConfirming
                ? 'w-auto min-w-[58px] bg-destructive/10 px-2 text-[11px] font-semibold text-destructive hover:bg-destructive/15'
                : 'w-6',
            )}
            disabled={isPending}
            onClick={(event) => {
              event.stopPropagation()
              if (isArchiveConfirming) {
                setArchiveConfirmationActive(false)
                onArchiveSession(session.agentSessionId)
                return
              }
              setArchiveConfirmationActive(true)
            }}
            onBlur={(event: FocusEvent<HTMLButtonElement>) => {
              const nextFocused = event.relatedTarget
              if (nextFocused instanceof Node && event.currentTarget.contains(nextFocused)) {
                return
              }
              clearArchiveConfirmation()
            }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.stopPropagation()
                clearArchiveConfirmation()
              }
            }}
            onPointerDown={stopActionPreview}
            onPointerLeave={clearArchiveConfirmation}
            size="icon-sm"
            title={archiveActionTitle}
            type="button"
            variant="ghost"
          >
            {isPending ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : isArchiveConfirming ? (
              <span>Archive</span>
            ) : (
              <Archive className="h-3.5 w-3.5" />
            )}
          </Button>
        ) : null}
      </div>
    </div>
  )
})

interface AgentArchivedSessionsSidebarItemProps {
  session: AgentSessionView
  isRestoring: boolean
  isDeleting: boolean
  isAnyActionPending: boolean
  onRestore: (session: AgentSessionView) => void | Promise<void>
  onDelete: (session: AgentSessionView) => void | Promise<void>
}

const AgentArchivedSessionsSidebarItem = memo(function AgentArchivedSessionsSidebarItem({
  session,
  isRestoring,
  isDeleting,
  isAnyActionPending,
  onRestore,
  onDelete,
}: AgentArchivedSessionsSidebarItemProps) {
  const [deleteConfirmationActive, setDeleteConfirmationActive] = useState(false)
  const isDeleteConfirming = deleteConfirmationActive && !isAnyActionPending
  const deleteActionLabel = isDeleteConfirming
    ? `Confirm delete ${session.title}`
    : `Delete ${session.title} permanently`
  const deleteActionTitle = isDeleteConfirming
    ? `Press again to delete ${session.title} permanently`
    : `Delete ${session.title} permanently`
  const clearDeleteConfirmation = () => setDeleteConfirmationActive(false)
  const stopActionPreview = (event: PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation()
  }

  useEffect(() => {
    setDeleteConfirmationActive(false)
  }, [session.agentSessionId])

  useEffect(() => {
    if (isAnyActionPending) {
      setDeleteConfirmationActive(false)
    }
  }, [isAnyActionPending])

  return (
    <div className="group relative">
      <div
        className={cn(
          'flex w-full items-center rounded-md px-3 py-2 pr-[72px] text-left transition-colors',
          isDeleteConfirming && 'pr-[112px]',
          'hover:bg-secondary/50',
        )}
        title={session.title}
      >
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <span className="truncate text-[12.5px] font-medium leading-tight text-foreground/85 group-hover:text-foreground">
            {session.title}
          </span>
        </div>
      </div>

      <div
        className={cn(
          'absolute right-1 top-1/2 z-10 flex -translate-y-1/2 items-center gap-0.5',
          'opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-within:opacity-100',
          (isRestoring || isDeleting) && 'opacity-100',
        )}
      >
        <Button
          aria-label={`Restore ${session.title}`}
          className="h-6 w-6 p-0 text-muted-foreground hover:bg-secondary hover:text-foreground"
          disabled={isAnyActionPending}
          onClick={(event) => {
            event.stopPropagation()
            void onRestore(session)
          }}
          onPointerDown={stopActionPreview}
          size="icon-sm"
          title={`Restore ${session.title}`}
          type="button"
          variant="ghost"
        >
          {isRestoring ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ArchiveRestore className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          aria-label={deleteActionLabel}
          className={cn(
            'h-6 p-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive',
            isDeleteConfirming
              ? 'w-auto min-w-[52px] bg-destructive/10 px-2 text-[11px] font-semibold text-destructive hover:bg-destructive/15'
              : 'w-6',
          )}
          disabled={isAnyActionPending}
          onClick={(event) => {
            event.stopPropagation()
            if (isDeleteConfirming) {
              setDeleteConfirmationActive(false)
              void onDelete(session)
              return
            }
            setDeleteConfirmationActive(true)
          }}
          onBlur={(event: FocusEvent<HTMLButtonElement>) => {
            const nextFocused = event.relatedTarget
            if (nextFocused instanceof Node && event.currentTarget.contains(nextFocused)) {
              return
            }
            clearDeleteConfirmation()
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.stopPropagation()
              clearDeleteConfirmation()
            }
          }}
          onPointerDown={stopActionPreview}
          onPointerLeave={clearDeleteConfirmation}
          size="icon-sm"
          title={deleteActionTitle}
          type="button"
          variant="ghost"
        >
          {isDeleting ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : isDeleteConfirming ? (
            <span>Delete</span>
          ) : (
            <Trash2 className="h-3.5 w-3.5" />
          )}
        </Button>
      </div>
    </div>
  )
})
