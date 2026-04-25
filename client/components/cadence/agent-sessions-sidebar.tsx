"use client"

import { useState } from 'react'
import { Loader2, MessageSquare, MoreHorizontal, Plus, Trash2 } from 'lucide-react'

import { cn } from '@/lib/utils'
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import type { AgentSessionView } from '@/src/lib/cadence-model'

interface AgentSessionsSidebarProps {
  projectLabel: string | null
  sessions: readonly AgentSessionView[]
  selectedSessionId: string | null
  onSelectSession: (agentSessionId: string) => void
  onCreateSession: () => void
  onArchiveSession: (agentSessionId: string) => void
  pendingSessionId?: string | null
  isCreating?: boolean
  collapsed?: boolean
}

export function AgentSessionsSidebar({
  projectLabel,
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  onArchiveSession,
  pendingSessionId,
  isCreating,
  collapsed = false,
}: AgentSessionsSidebarProps) {
  const activeSessions = sessions.filter((session) => session.isActive)

  return (
    <aside
      aria-hidden={collapsed}
      className={cn(
        'motion-layout-island flex shrink-0 flex-col overflow-hidden border-r border-border bg-sidebar transition-[width,border-color] motion-panel',
        collapsed ? 'w-0 border-r-transparent' : 'w-[260px]',
      )}
    >
      <div className="flex w-[260px] shrink-0 flex-col h-full">
      <div className="flex shrink-0 items-start justify-between gap-2 px-3 pt-2.5 pb-2">
        <div className="min-w-0">
          <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Sessions
          </span>
          {projectLabel ? (
            <p className="truncate text-[11px] text-foreground/85">{projectLabel}</p>
          ) : null}
        </div>
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
      </div>

      <div className="flex-1 overflow-y-auto scrollbar-thin">
        {activeSessions.length === 0 ? (
          <div className="px-3 py-5 text-center text-[11px] leading-relaxed text-muted-foreground/80">
            No sessions yet. Start a new chat to begin.
          </div>
        ) : (
          <ul className="flex flex-col px-1.5 py-1.5">
            {activeSessions.map((session) => (
              <li key={session.agentSessionId}>
                <AgentSessionsSidebarItem
                  session={session}
                  isActive={session.agentSessionId === selectedSessionId}
                  isPending={session.agentSessionId === pendingSessionId}
                  onSelectSession={onSelectSession}
                  onArchiveSession={onArchiveSession}
                  canArchive={activeSessions.length > 1}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
      </div>
    </aside>
  )
}

interface AgentSessionsSidebarItemProps {
  session: AgentSessionView
  isActive: boolean
  isPending: boolean
  canArchive: boolean
  onSelectSession: (agentSessionId: string) => void
  onArchiveSession: (agentSessionId: string) => void
}

function AgentSessionsSidebarItem({
  session,
  isActive,
  isPending,
  canArchive,
  onSelectSession,
  onArchiveSession,
}: AgentSessionsSidebarItemProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const formattedUpdatedAt = formatRelativeDate(session.updatedAt)

  return (
    <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
      <div className="group relative">
        <button
          className={cn(
            'flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition-colors',
            isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
          )}
          onClick={() => onSelectSession(session.agentSessionId)}
          type="button"
        >
          <div
            className={cn(
              'flex h-6 w-6 shrink-0 items-center justify-center rounded-md border transition-colors',
              isActive
                ? 'border-primary/45 bg-primary/15 text-primary'
                : 'border-border/70 bg-secondary/70 text-muted-foreground group-hover:border-border group-hover:bg-secondary group-hover:text-foreground',
            )}
          >
            <MessageSquare className="h-3 w-3" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1 pr-5">
              <span
                className={cn(
                  'truncate text-[12px] font-medium leading-tight',
                  isActive ? 'text-foreground' : 'text-foreground/85 group-hover:text-foreground',
                )}
              >
                {session.title}
              </span>
            </div>
            {formattedUpdatedAt ? (
              <div className="mt-0.5 truncate text-[10px] text-muted-foreground">
                {formattedUpdatedAt}
              </div>
            ) : null}
          </div>
        </button>

        {canArchive ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                aria-label={`Session actions for ${session.title}`}
                className={cn(
                  'absolute right-1 top-1 z-10 flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground transition-colors',
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
                  setConfirmOpen(true)
                }}
                variant="destructive"
              >
                <Trash2 className="h-4 w-4" />
                Archive
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
      </div>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Archive "{session.title}"?</AlertDialogTitle>
          <AlertDialogDescription>
            The session will be hidden from the sidebar but its conversation history is preserved.
            You can't archive a session while one of its runs is still active.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className={buttonVariants({ variant: 'destructive' })}
            disabled={isPending}
            onClick={() => onArchiveSession(session.agentSessionId)}
          >
            {isPending ? 'Archiving…' : 'Archive'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function formatRelativeDate(isoTimestamp: string): string | null {
  const parsed = Date.parse(isoTimestamp)
  if (!Number.isFinite(parsed)) {
    return null
  }

  const now = Date.now()
  const diffSeconds = Math.floor((now - parsed) / 1000)

  if (diffSeconds < 60) return 'Just now'
  if (diffSeconds < 3600) {
    const minutes = Math.floor(diffSeconds / 60)
    return `${minutes}m ago`
  }
  if (diffSeconds < 86400) {
    const hours = Math.floor(diffSeconds / 3600)
    return `${hours}h ago`
  }
  if (diffSeconds < 86400 * 7) {
    const days = Math.floor(diffSeconds / 86400)
    return `${days}d ago`
  }

  return new Date(parsed).toLocaleDateString()
}
