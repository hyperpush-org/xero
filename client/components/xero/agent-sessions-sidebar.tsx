"use client"

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from 'react'
import {
  Archive,
  ChevronRight,
  FileText,
  Loader2,
  MessageSquare,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Search,
  Trash2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { createFrameCoalescer } from '@/lib/frame-governance'
import { useSidebarWidthMotion } from '@/lib/sidebar-motion'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
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
  onOpenArchivedSessions: () => void
  onRenameSession?: (agentSessionId: string, title: string) => Promise<void> | void
  onSearchSessions?: (query: string) => Promise<SessionTranscriptSearchResultSnippetDto[]>
  onOpenSearchResult?: (result: SessionTranscriptSearchResultSnippetDto) => void
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

const PINNED_SESSIONS_STORAGE_PREFIX = 'xero:pinned-sessions:'
const MIN_WIDTH = 220
const DEFAULT_WIDTH = 260
const MAX_WIDTH = 560
const RIGHT_PADDING = 360
const WIDTH_STORAGE_KEY = 'xero.agentSessions.width'
const STRIP_WIDTH = 6

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

export function AgentSessionsSidebar({
  projectId,
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  onArchiveSession,
  onOpenArchivedSessions,
  onRenameSession,
  onSearchSessions,
  onOpenSearchResult,
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
  const isStripMode = mode === 'collapsed' && collapsed
  const showOverlay = isStripMode && peeking
  const activeSessions = useMemo(
    () => sessions.filter((session) => session.isActive),
    [sessions],
  )

  const [pinnedIds, setPinnedIds] = useState<Set<string>>(() => readPinnedSessionIds(projectId))
  const [width, setWidth] = useState(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<SessionTranscriptSearchResultSnippetDto[]>([])
  const [searchStatus, setSearchStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle')
  const [searchError, setSearchError] = useState<string | null>(null)
  const [renameSession, setRenameSession] = useState<AgentSessionView | null>(null)
  const [renameTitle, setRenameTitle] = useState('')
  const [renameError, setRenameError] = useState<string | null>(null)
  const [pendingRename, setPendingRename] = useState(false)
  const [optimisticSessionId, setOptimisticSessionId] = useState<string | null>(null)
  const targetWidth = isStripMode ? STRIP_WIDTH : collapsed ? 0 : width
  const displayedSelectedSessionId = optimisticSessionId ?? selectedSessionId
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const islandClassName = isStripMode ? 'sidebar-peek-island' : widthMotion.islandClassName
  const widthRef = useRef(width)
  const searchInputRef = useRef<HTMLInputElement>(null)
  widthRef.current = width

  useEffect(() => {
    setPinnedIds(readPinnedSessionIds(projectId))
  }, [projectId])

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

  const handlePreviewSession = useCallback((agentSessionId: string) => {
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
        writePinnedSessionIds(projectId, next)
        return next
      })
    },
    [projectId],
  )

  const handleOpenRename = useCallback((session: AgentSessionView) => {
    setRenameSession(session)
    setRenameTitle(session.title)
    setRenameError(null)
  }, [])

  const handleRenameSubmit = useCallback(async () => {
    if (!renameSession || !onRenameSession) return
    const title = renameTitle.trim()
    if (title.length === 0) {
      setRenameError('Enter a session name.')
      return
    }

    setPendingRename(true)
    setRenameError(null)
    try {
      await onRenameSession(renameSession.agentSessionId, title)
      setRenameSession(null)
    } catch (error) {
      setRenameError(error instanceof Error ? error.message : 'Xero could not rename this session.')
    } finally {
      setPendingRename(false)
    }
  }, [onRenameSession, renameSession, renameTitle])

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
        onRenameSession={onRenameSession ? handleOpenRename : undefined}
        canArchive={entry.state !== 'exiting'}
        paneNumber={sessionPaneAssignments?.[entry.session.agentSessionId] ?? null}
      />
    </li>
  )

  const panelChildren = (
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
              <Search className="h-3.5 w-3.5" />
            </Toggle>
          ) : null}
          <button
            aria-label="View archived sessions"
            className={cn(
              'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
              'hover:bg-primary/10 hover:text-primary',
            )}
            onClick={onOpenArchivedSessions}
            type="button"
          >
            <Archive className="h-3.5 w-3.5" />
          </button>
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
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Plus className="h-3.5 w-3.5" />
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
              <PanelLeftOpen className="h-3.5 w-3.5" />
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
              <PanelLeftClose className="h-3.5 w-3.5" />
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
          {entries.length === 0 ? (
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
            </>
          )}
      </div>
    </>
  )

  const renameDialog = (
    <Dialog open={renameSession !== null} onOpenChange={(open) => !open && setRenameSession(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Rename session</DialogTitle>
            <DialogDescription>Change the session name shown in the project sidebar.</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <label className="text-sm font-medium text-foreground" htmlFor="agent-session-rename">
              Name
            </label>
            <Input
              id="agent-session-rename"
              value={renameTitle}
              onChange={(event) => setRenameTitle(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  void handleRenameSubmit()
                }
              }}
              disabled={pendingRename}
              autoFocus
            />
            {renameError ? <p className="text-xs text-destructive">{renameError}</p> : null}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={pendingRename}
              onClick={() => setRenameSession(null)}
            >
              Cancel
            </Button>
            <Button type="button" disabled={pendingRename} onClick={() => void handleRenameSubmit()}>
              {pendingRename ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              Rename
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
  )

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
      style={widthMotion.style}
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

      {!isStripMode ? (
        <div className="flex h-full shrink-0 flex-col" style={{ width }}>
          {panelChildren}
        </div>
      ) : null}

      {renameDialog}
    </aside>
  )
}

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
  onPreviewSession?: (agentSessionId: string) => void
  onArchiveSession: (agentSessionId: string) => void
  onTogglePin: (agentSessionId: string) => void
  onRenameSession?: (session: AgentSessionView) => void
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
  onRenameSession,
  compact = 'full',
  paneNumber = null,
}: AgentSessionsSidebarItemProps) {
  const handlePointerDown = useCallback((event: PointerEvent<HTMLButtonElement>) => {
    if (event.button === 0) {
      onPreviewSession?.(session.agentSessionId)
    }
  }, [onPreviewSession, session.agentSessionId])

  if (compact === 'icon') {
    return (
      <button
        aria-label={session.title}
        className={cn(
          'flex w-full items-center justify-center rounded-md p-1 transition-colors',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/60',
        )}
        onClick={() => onSelectSession(session.agentSessionId)}
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
    )
  }

  if (compact === 'list') {
    return (
      <button
        className={cn(
          'group flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
        )}
        onClick={() => onSelectSession(session.agentSessionId)}
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
    )
  }

  return (
    <div className="group relative">
      <button
        className={cn(
          'flex w-full items-center rounded-md px-3 py-2 text-left transition-colors',
          isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
        )}
        onClick={() => onSelectSession(session.agentSessionId)}
        onPointerDown={handlePointerDown}
        type="button"
      >
        <div className="flex min-w-0 flex-1 items-center gap-1 pr-6">
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
          {!isActive && paneNumber != null ? (
            <span
              aria-label={`Loaded in pane ${paneNumber}`}
              className="ml-auto inline-flex h-[16px] shrink-0 items-center justify-center rounded-sm border border-border/60 bg-muted/40 px-1 text-[9.5px] font-semibold uppercase tracking-wider text-muted-foreground"
            >
              P{paneNumber}
            </span>
          ) : null}
        </div>
      </button>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            aria-label={`Session actions for ${session.title}`}
            className={cn(
              'absolute right-1 top-1/2 z-10 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors',
              'hover:bg-secondary hover:text-foreground disabled:opacity-50',
              isActive || isPending
                ? 'opacity-100'
                : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100',
            )}
            disabled={isPending}
            type="button"
          >
            {isPending ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <MoreHorizontal className="h-3.5 w-3.5" />
            )}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem
            onSelect={(event) => {
              event.preventDefault()
              onTogglePin(session.agentSessionId)
            }}
          >
            {isPinned ? (
              <>
                <PinOff className="h-4 w-4" />
                Unpin
              </>
            ) : (
              <>
                <Pin className="h-4 w-4" />
                Pin
              </>
            )}
          </DropdownMenuItem>
          {onRenameSession ? (
            <DropdownMenuItem
              onSelect={(event) => {
                event.preventDefault()
                onRenameSession(session)
              }}
            >
              <Pencil className="h-4 w-4" />
              Rename
            </DropdownMenuItem>
          ) : null}
          {canArchive ? (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onSelect={(event) => {
                  event.preventDefault()
                  onArchiveSession(session.agentSessionId)
                }}
                variant="destructive"
              >
                <Trash2 className="h-4 w-4" />
                Archive
              </DropdownMenuItem>
            </>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
})
