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
import { Archive, Loader2, PanelLeftOpen, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { motion, type Transition } from 'motion/react'

import { cn } from '@/lib/utils'
import { useSidebarMotion, useSidebarWidthMotion } from '@/lib/sidebar-motion'
import {
  AgentSessionsSidebarItem,
  readPinnedSessionIds,
} from '@/components/xero/agent-sessions-sidebar'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { buttonVariants } from '@/components/ui/button'
import type { AgentSessionView, ProjectListItem } from '@/src/lib/xero-model'

const COLLAPSED_WIDTH = 44
const MIN_WIDTH = 180
const DEFAULT_WIDTH = 224
const MAX_WIDTH = 480
const RIGHT_PADDING = 360
const STORAGE_KEY = 'xero.projectRail.width'

interface ProjectRailProps {
  projects: ProjectListItem[]
  activeProjectId: string | null
  collapsed?: boolean
  isLoading: boolean
  isImporting: boolean
  projectRemovalStatus: 'idle' | 'running'
  pendingProjectRemovalId: string | null
  pendingProjectSelectionId?: string | null
  errorMessage: string | null
  onSelectProject: (projectId: string) => void
  onImportProject: () => void
  onRemoveProject: (projectId: string) => void
  explorerCollapsed?: boolean
  onExpandExplorer?: () => void
  sessions?: readonly AgentSessionView[]
  selectedSessionId?: string | null
  pendingSessionId?: string | null
  isCreatingSession?: boolean
  onSelectSession?: (agentSessionId: string) => void
  onCreateSession?: () => void
  onArchiveSession?: (agentSessionId: string) => void
  onOpenArchivedSessions?: () => void
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
    const raw = window.localStorage?.getItem?.(STORAGE_KEY)
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
    window.localStorage?.setItem?.(STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable — default next session */
  }
}

export function ProjectRail({
  projects,
  activeProjectId,
  collapsed = false,
  isLoading,
  isImporting,
  projectRemovalStatus,
  pendingProjectRemovalId,
  pendingProjectSelectionId = null,
  errorMessage,
  onSelectProject,
  onImportProject,
  onRemoveProject,
  explorerCollapsed = false,
  onExpandExplorer,
  sessions,
  selectedSessionId = null,
  pendingSessionId = null,
  isCreatingSession = false,
  onSelectSession,
  onCreateSession,
  onArchiveSession,
  onOpenArchivedSessions,
}: ProjectRailProps) {
  const isRemovingProject = projectRemovalStatus === 'running'
  const isBusy = isLoading || isImporting || isRemovingProject
  const [width, setWidth] = useState(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [optimisticProjectId, setOptimisticProjectId] = useState<string | null>(null)
  const targetWidth = collapsed ? COLLAPSED_WIDTH : width
  const displayedActiveProjectId = optimisticProjectId ?? pendingProjectSelectionId ?? activeProjectId
  const {
    contentTransition: railContentTransition,
    layoutTransition: railLayoutTransition,
  } = useSidebarMotion(isResizing)
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const widthRef = useRef(width)
  widthRef.current = width

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
    writePersistedWidth(width)
  }, [width])

  useEffect(() => {
    if (!optimisticProjectId) {
      return
    }

    if (
      activeProjectId === optimisticProjectId ||
      !projects.some((project) => project.id === optimisticProjectId)
    ) {
      setOptimisticProjectId(null)
    }
  }, [activeProjectId, optimisticProjectId, projects])

  const handleSelectProject = useCallback(
    (projectId: string) => {
      setOptimisticProjectId((currentProjectId) => {
        const currentDisplayedProjectId =
          currentProjectId ?? pendingProjectSelectionId ?? activeProjectId
        return currentDisplayedProjectId === projectId ? currentProjectId : projectId
      })
      onSelectProject(projectId)
    },
    [activeProjectId, onSelectProject, pendingProjectSelectionId],
  )

  const handlePreviewProject = useCallback(
    (projectId: string) => {
      setOptimisticProjectId((currentProjectId) => {
        const currentDisplayedProjectId =
          currentProjectId ?? pendingProjectSelectionId ?? activeProjectId
        return currentDisplayedProjectId === projectId ? currentProjectId : projectId
      })
    },
    [activeProjectId, pendingProjectSelectionId],
  )

  const handleResizeStart = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (collapsed || event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const handleMove = (ev: globalThis.PointerEvent) => {
      const delta = ev.clientX - startX
      setWidth(clampWidth(startWidth + delta, ceiling))
    }
    const handleUp = () => {
      window.removeEventListener('pointermove', handleMove)
      window.removeEventListener('pointerup', handleUp)
      window.removeEventListener('pointercancel', handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
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
      return clampWidth(current + delta, ceiling)
    })
  }, [collapsed])

  return (
    <aside
      className={cn(
        widthMotion.islandClassName,
        'relative flex shrink-0 flex-col overflow-hidden border-r border-border/80 bg-sidebar',
        collapsed && 'w-11',
      )}
      data-collapsed={collapsed ? 'true' : 'false'}
      style={widthMotion.style}
    >
      {!collapsed ? (
        <div
          aria-label="Resize projects sidebar"
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

      <motion.div
        className={cn(
          'flex h-10 items-center border-b border-border/70 transition-[padding] motion-panel',
          collapsed ? 'justify-center px-1' : 'justify-between px-3',
        )}
        layout="position"
        transition={railLayoutTransition}
      >
        <motion.div
          animate={{
            maxWidth: collapsed ? 0 : 160,
            opacity: collapsed ? 0 : 1,
            x: collapsed ? -4 : 0,
          }}
          aria-hidden={collapsed ? true : undefined}
          className="flex items-center gap-1.5 overflow-hidden will-change-[max-width,opacity,transform]"
          initial={false}
          transition={railContentTransition}
        >
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Projects
          </span>
          {projects.length > 0 ? (
            <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
              {projects.length}
            </span>
          ) : null}
        </motion.div>
        <button
          aria-label="Import repository"
          className={cn(
            'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
            'hover:bg-primary/10 hover:text-primary disabled:cursor-not-allowed disabled:opacity-50',
          )}
          disabled={isImporting || isRemovingProject}
          onClick={onImportProject}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </motion.div>

      {errorMessage ? (
        <div
          className={cn(
            'border-b border-border/70 bg-destructive/5 text-[11px] leading-snug text-destructive transition-[padding,opacity,max-height] motion-standard',
            collapsed ? 'max-h-0 px-0 py-0 opacity-0' : 'max-h-20 px-3 py-2 opacity-100',
          )}
        >
          {errorMessage}
        </div>
      ) : null}

      <div
        className={cn(
          'min-h-0 flex-1 overflow-y-auto scrollbar-thin',
          collapsed ? 'py-2' : '',
        )}
      >
        {projects.length === 0 ? (
          <div
            aria-hidden={collapsed ? true : undefined}
            className={cn(
              'px-3 py-5 text-center text-[11px] leading-relaxed text-muted-foreground/80 transition-[max-height,opacity] motion-standard',
              collapsed ? 'max-h-0 opacity-0' : 'max-h-24 opacity-100',
            )}
          >
            No projects imported yet.
          </div>
        ) : (
          <ul
            className={cn('flex flex-col', collapsed ? 'gap-1.5 px-1.5' : 'gap-1.5 px-1.5 py-1.5')}
          >
            {projects.map((project) => (
              <li key={project.id}>
                <ProjectRailItem
                  collapsed={collapsed}
                  contentTransition={railContentTransition}
                  project={project}
                  isActive={project.id === displayedActiveProjectId}
                  isRemovalPending={project.id === pendingProjectRemovalId}
                  isRemovalLocked={isRemovingProject}
                  onPreviewProject={handlePreviewProject}
                  onRemoveProject={onRemoveProject}
                  onSelectProject={handleSelectProject}
                />
              </li>
            ))}
          </ul>
        )}
      </div>

      {explorerCollapsed && onSelectSession ? (
        <ProjectRailSessionsSection
          railCollapsed={collapsed}
          activeProjectId={activeProjectId}
          sessions={sessions ?? []}
          selectedSessionId={selectedSessionId}
          pendingSessionId={pendingSessionId}
          isCreatingSession={isCreatingSession}
          onSelectSession={onSelectSession}
          onCreateSession={onCreateSession}
          onArchiveSession={onArchiveSession}
          onOpenArchivedSessions={onOpenArchivedSessions}
          onExpandExplorer={onExpandExplorer}
          contentTransition={railContentTransition}
          layoutTransition={railLayoutTransition}
        />
      ) : null}

      {isBusy && (
        <motion.div
          className={cn(
            'flex items-center border-t border-border/70 bg-sidebar text-[11px] text-muted-foreground transition-[padding,gap] motion-panel',
            collapsed ? 'justify-center gap-0 px-1.5 py-2.5' : 'gap-2 px-3 py-2.5',
          )}
          layout="position"
          transition={railLayoutTransition}
        >
          <RefreshCw className="h-3 w-3 animate-spin text-primary/80" />
          <motion.span
            animate={{
              maxWidth: collapsed ? 0 : 96,
              opacity: collapsed ? 0 : 1,
              x: collapsed ? -4 : 0,
            }}
            className="overflow-hidden whitespace-nowrap will-change-[max-width,opacity,transform]"
            initial={false}
            transition={railContentTransition}
          >
            {isImporting ? 'Importing…' : isRemovingProject ? 'Removing…' : 'Refreshing…'}
          </motion.span>
        </motion.div>
      )}
    </aside>
  )
}

interface ProjectRailItemProps {
  project: ProjectListItem
  collapsed: boolean
  isActive: boolean
  isRemovalPending: boolean
  isRemovalLocked: boolean
  contentTransition: Transition
  onSelectProject: (projectId: string) => void
  onPreviewProject: (projectId: string) => void
  onRemoveProject: (projectId: string) => void
}

const ProjectRailItem = memo(function ProjectRailItem({
  project,
  collapsed,
  contentTransition,
  isActive,
  isRemovalPending,
  isRemovalLocked,
  onSelectProject,
  onPreviewProject,
  onRemoveProject,
}: ProjectRailItemProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const projectInitial = Array.from(project.name.trim())[0]?.toUpperCase() ?? '?'

  return (
    <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
      <div className="group relative">
        <button
          className={cn(
            'relative w-full transition-colors duration-150',
            collapsed
              ? cn(
                  'flex items-center justify-center rounded-md p-1',
                  !isActive && 'hover:bg-secondary/60',
                )
              : cn(
                  'flex items-center gap-2 rounded-md px-2 py-1 text-left',
                  isActive
                    ? 'border border-border/40 bg-primary/[0.08]'
                    : 'border border-transparent hover:bg-secondary/50',
            ),
          )}
          onClick={() => onSelectProject(project.id)}
          onPointerDown={(event) => {
            if (event.button === 0) {
              onPreviewProject(project.id)
            }
          }}
          title={collapsed ? project.name : undefined}
          type="button"
        >
          <div
            className={cn(
              'flex h-5 w-5 shrink-0 items-center justify-center rounded-md border text-[10px] font-semibold leading-none transition-colors duration-150',
              isActive
                ? 'border-primary/45 bg-primary/15 text-primary'
              : 'border-border/70 bg-secondary/70 text-muted-foreground group-hover:border-border group-hover:bg-secondary group-hover:text-foreground',
            )}
          >
            <span aria-hidden="true">{projectInitial}</span>
            {collapsed ? <span className="sr-only">{project.name}</span> : null}
          </div>

          <motion.div
            animate={{
              maxWidth: collapsed ? 0 : 168,
              opacity: collapsed ? 0 : 1,
              x: collapsed ? -6 : 0,
            }}
            aria-hidden={collapsed ? true : undefined}
            className="min-w-0 flex-1 overflow-hidden will-change-[max-width,opacity,transform]"
            initial={false}
            transition={contentTransition}
          >
            <div className="flex items-center pr-6">
              <span
                className={cn(
                  'truncate text-[12.5px] font-medium leading-tight',
                  isActive ? 'text-foreground' : 'text-foreground/85 group-hover:text-foreground',
                )}
              >
                {project.name}
              </span>
            </div>
          </motion.div>
        </button>

        {!collapsed ? (
          <button
            aria-label={`Remove ${project.name}`}
            className={cn(
              'absolute right-1 top-1/2 z-10 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-[opacity,color,background-color] motion-fast',
              'hover:bg-destructive/10 hover:text-destructive disabled:opacity-50',
              isActive || isRemovalPending
                ? 'opacity-100'
                : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100',
            )}
            disabled={isRemovalLocked}
            onClick={(event) => {
              event.stopPropagation()
              setConfirmOpen(true)
            }}
            type="button"
          >
            {isRemovalPending ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Trash2 className="h-3 w-3" />
            )}
          </button>
        ) : null}
      </div>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove {project.name} from the sidebar?</AlertDialogTitle>
          <AlertDialogDescription>
            Xero will only forget this project in the desktop registry. The repo and its app-data
            project state stay untouched. You can import the same folder again any time.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isRemovalPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className={buttonVariants({ variant: 'destructive' })}
            disabled={isRemovalPending}
            onClick={() => onRemoveProject(project.id)}
          >
            {isRemovalPending ? 'Removing…' : 'Remove'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
})

interface ProjectRailSessionsSectionProps {
  railCollapsed: boolean
  activeProjectId: string | null
  sessions: readonly AgentSessionView[]
  selectedSessionId: string | null
  pendingSessionId: string | null
  isCreatingSession: boolean
  onSelectSession: (agentSessionId: string) => void
  onCreateSession?: () => void
  onArchiveSession?: (agentSessionId: string) => void
  onOpenArchivedSessions?: () => void
  onExpandExplorer?: () => void
  contentTransition: Transition
  layoutTransition: Transition
}

function ProjectRailSessionsSection({
  railCollapsed,
  activeProjectId,
  sessions,
  selectedSessionId,
  pendingSessionId,
  isCreatingSession,
  onSelectSession,
  onCreateSession,
  onArchiveSession,
  onOpenArchivedSessions,
  onExpandExplorer,
  contentTransition,
  layoutTransition,
}: ProjectRailSessionsSectionProps) {
  const activeSessions = useMemo(
    () => sessions.filter((session) => session.isActive),
    [sessions],
  )
  const pinnedIds = useMemo(
    () => readPinnedSessionIds(activeProjectId),
    [activeProjectId, sessions],
  )
  const noopArchive = useCallback(() => {}, [])
  const noopTogglePin = useCallback(() => {}, [])

  if (railCollapsed) {
    if (!onExpandExplorer) return null

    return (
      <motion.section
        className="flex shrink-0 items-center justify-center border-t border-border/70 px-1 py-1.5"
        layout="position"
        transition={layoutTransition}
      >
        <button
          aria-label="Expand sessions sidebar"
          className={cn(
            'flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors',
            'hover:bg-primary/10 hover:text-primary',
          )}
          onClick={onExpandExplorer}
          type="button"
        >
          <PanelLeftOpen className="h-3.5 w-3.5" />
        </button>
      </motion.section>
    )
  }

  return (
    <motion.section
      className="flex min-h-0 flex-1 flex-col border-t border-border/70"
      layout="position"
      transition={layoutTransition}
    >
      <motion.div
        className={cn(
          'flex h-9 shrink-0 items-center border-b border-border/60 transition-[padding] motion-panel',
          railCollapsed ? 'justify-center px-1' : 'justify-between px-3',
        )}
        layout="position"
        transition={layoutTransition}
      >
        <motion.div
          animate={{
            maxWidth: railCollapsed ? 0 : 160,
            opacity: railCollapsed ? 0 : 1,
            x: railCollapsed ? -4 : 0,
          }}
          aria-hidden={railCollapsed ? true : undefined}
          className="flex items-center gap-1.5 overflow-hidden will-change-[max-width,opacity,transform]"
          initial={false}
          transition={contentTransition}
        >
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Sessions
          </span>
          {activeSessions.length > 0 ? (
            <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
              {activeSessions.length}
            </span>
          ) : null}
        </motion.div>
        <div className="flex shrink-0 items-center gap-0.5">
          {!railCollapsed && onOpenArchivedSessions ? (
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
          ) : null}
          {onCreateSession ? (
            <button
              aria-label="New session"
              className={cn(
                'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary disabled:cursor-not-allowed disabled:opacity-50',
              )}
              disabled={isCreatingSession}
              onClick={onCreateSession}
              type="button"
            >
              {isCreatingSession ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Plus className="h-3.5 w-3.5" />
              )}
            </button>
          ) : null}
          {!railCollapsed && onExpandExplorer ? (
            <button
              aria-label="Expand sessions sidebar"
              className={cn(
                'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
              )}
              onClick={onExpandExplorer}
              type="button"
            >
              <PanelLeftOpen className="h-3.5 w-3.5" />
            </button>
          ) : null}
        </div>
      </motion.div>

      <div
        className={cn(
          'min-h-0 flex-1 overflow-y-auto scrollbar-thin',
          railCollapsed ? 'py-2' : '',
        )}
      >
        {activeSessions.length === 0 ? (
          <div
            aria-hidden={railCollapsed ? true : undefined}
            className={cn(
              'px-3 py-4 text-center text-[11px] leading-relaxed text-muted-foreground/80 transition-[max-height,opacity] motion-standard',
              railCollapsed ? 'max-h-0 opacity-0' : 'max-h-24 opacity-100',
            )}
          >
            No sessions yet.
          </div>
        ) : (
          <ul
            className={cn(
              'flex flex-col',
              railCollapsed ? 'gap-1.5 px-1.5' : 'gap-0.5 px-1.5 py-1.5',
            )}
          >
            {activeSessions.map((session) => (
              <li key={session.agentSessionId}>
                <AgentSessionsSidebarItem
                  session={session}
                  isActive={session.agentSessionId === selectedSessionId}
                  isPending={session.agentSessionId === pendingSessionId}
                  isPinned={pinnedIds.has(session.agentSessionId)}
                  canArchive={false}
                  onSelectSession={onSelectSession}
                  onArchiveSession={onArchiveSession ?? noopArchive}
                  onTogglePin={noopTogglePin}
                  compact={railCollapsed ? 'icon' : 'list'}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </motion.section>
  )
}
