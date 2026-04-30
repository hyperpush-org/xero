"use client"

import { useCallback, useEffect, useState } from 'react'
import { Archive, ArchiveRestore, Loader2, Trash2 } from 'lucide-react'

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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import type { AgentSessionView } from '@/src/lib/xero-model'

interface ArchivedSessionsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string | null
  projectLabel: string | null
  onLoad: (projectId: string) => Promise<readonly AgentSessionView[]>
  onRestore: (agentSessionId: string) => Promise<void>
  onDelete: (agentSessionId: string) => Promise<void>
}

type LoadStatus = 'idle' | 'loading' | 'loaded' | 'error'

export function ArchivedSessionsDialog({
  open,
  onOpenChange,
  projectId,
  projectLabel,
  onLoad,
  onRestore,
  onDelete,
}: ArchivedSessionsDialogProps) {
  const [sessions, setSessions] = useState<readonly AgentSessionView[]>([])
  const [status, setStatus] = useState<LoadStatus>('idle')
  const [loadError, setLoadError] = useState<string | null>(null)
  const [pendingRestoreId, setPendingRestoreId] = useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [confirmDeleteSession, setConfirmDeleteSession] = useState<AgentSessionView | null>(null)

  const refresh = useCallback(async () => {
    if (!projectId) {
      setSessions([])
      setStatus('idle')
      return
    }
    setStatus('loading')
    setLoadError(null)
    try {
      const loaded = await onLoad(projectId)
      setSessions(loaded)
      setStatus('loaded')
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to load archived sessions.')
      setStatus('error')
    }
  }, [onLoad, projectId])

  useEffect(() => {
    if (!open) return
    void refresh()
  }, [open, refresh])

  useEffect(() => {
    if (!open) {
      setActionError(null)
      setPendingRestoreId(null)
      setPendingDeleteId(null)
      setConfirmDeleteSession(null)
    }
  }, [open])

  const handleRestore = useCallback(
    async (session: AgentSessionView) => {
      setPendingRestoreId(session.agentSessionId)
      setActionError(null)
      try {
        await onRestore(session.agentSessionId)
        setSessions((prev) => prev.filter((entry) => entry.agentSessionId !== session.agentSessionId))
        onOpenChange(false)
      } catch (error) {
        setActionError(error instanceof Error ? error.message : 'Failed to restore session.')
      } finally {
        setPendingRestoreId(null)
      }
    },
    [onOpenChange, onRestore],
  )

  const handleConfirmDelete = useCallback(async () => {
    if (!confirmDeleteSession) return
    const targetId = confirmDeleteSession.agentSessionId
    setPendingDeleteId(targetId)
    setActionError(null)
    try {
      await onDelete(targetId)
      setSessions((prev) => prev.filter((entry) => entry.agentSessionId !== targetId))
      setConfirmDeleteSession(null)
    } catch (error) {
      setActionError(error instanceof Error ? error.message : 'Failed to delete session.')
    } finally {
      setPendingDeleteId(null)
    }
  }, [confirmDeleteSession, onDelete])

  const isAnyActionPending = pendingRestoreId !== null || pendingDeleteId !== null

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent
          className="flex h-[min(560px,82vh)] w-[min(640px,92vw)] max-w-none flex-col gap-0 overflow-hidden border-border/80 p-0 shadow-xl sm:max-w-none"
          showCloseButton
        >
          <DialogTitle className="sr-only">Archived sessions</DialogTitle>
          <DialogDescription className="sr-only">
            Restore archived sessions back into the sidebar or permanently delete them.
          </DialogDescription>

          <div className="flex shrink-0 items-start gap-3 border-b border-border/70 bg-sidebar px-5 py-4">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/70 bg-secondary/70 text-muted-foreground">
              <Archive className="h-4 w-4" />
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="text-[13px] font-semibold leading-tight text-foreground">
                Archived sessions
              </h2>
              <p className="mt-0.5 truncate text-[11.5px] leading-relaxed text-muted-foreground">
                {projectLabel
                  ? `Archived sessions for ${projectLabel}.`
                  : 'Archived sessions for this project.'}
              </p>
            </div>
          </div>

          <div className="flex flex-1 flex-col overflow-y-auto scrollbar-thin">
            {status === 'loading' && sessions.length === 0 ? (
              <div className="flex flex-1 items-center justify-center py-10 text-[12px] text-muted-foreground">
                <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                Loading archived sessions…
              </div>
            ) : status === 'error' ? (
              <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center">
                <p className="text-[12.5px] font-medium text-foreground">
                  Couldn't load archived sessions
                </p>
                <p className="text-[11.5px] leading-relaxed text-muted-foreground">{loadError}</p>
                <button
                  type="button"
                  onClick={() => void refresh()}
                  className="mt-2 rounded-md border border-border/70 bg-background px-2.5 py-1 text-[11.5px] font-medium text-foreground transition-colors hover:bg-secondary/60"
                >
                  Retry
                </button>
              </div>
            ) : sessions.length === 0 ? (
              <div className="flex flex-1 flex-col items-center justify-center gap-1.5 px-6 py-10 text-center">
                <Archive className="h-4 w-4 text-muted-foreground/70" />
                <p className="text-[12.5px] font-medium text-foreground">No archived sessions</p>
                <p className="text-[11.5px] leading-relaxed text-muted-foreground">
                  Archived sessions will appear here. You can restore or permanently delete them.
                </p>
              </div>
            ) : (
              <ul className="flex flex-col px-3 py-3">
                {sessions.map((session) => {
                  const isRestoring = pendingRestoreId === session.agentSessionId
                  const isDeleting = pendingDeleteId === session.agentSessionId
                  const archivedAt = formatArchivedAt(session.archivedAt)
                  return (
                    <li
                      key={session.agentSessionId}
                      className="group flex items-center gap-3 rounded-md border border-transparent px-2 py-2 transition-colors hover:border-border/60 hover:bg-secondary/40"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[12.5px] font-medium text-foreground">
                          {session.title}
                        </div>
                        <div className="mt-0.5 truncate text-[10.5px] text-muted-foreground">
                          Archived {archivedAt}
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <button
                          type="button"
                          aria-label={`Restore ${session.title}`}
                          disabled={isAnyActionPending}
                          onClick={() => void handleRestore(session)}
                          className={cn(
                            'inline-flex h-7 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 text-[11.5px] font-medium text-foreground transition-colors',
                            'hover:bg-secondary/60 disabled:cursor-not-allowed disabled:opacity-50',
                          )}
                        >
                          {isRestoring ? (
                            <Loader2 className="h-3 w-3 animate-spin" />
                          ) : (
                            <ArchiveRestore className="h-3 w-3" />
                          )}
                          Restore
                        </button>
                        <button
                          type="button"
                          aria-label={`Delete ${session.title} permanently`}
                          disabled={isAnyActionPending}
                          onClick={() => setConfirmDeleteSession(session)}
                          className={cn(
                            'inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors',
                            'hover:bg-destructive/10 hover:text-destructive disabled:cursor-not-allowed disabled:opacity-50',
                          )}
                        >
                          {isDeleting ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            <Trash2 className="h-3.5 w-3.5" />
                          )}
                        </button>
                      </div>
                    </li>
                  )
                })}
              </ul>
            )}
            {actionError ? (
              <div className="border-t border-destructive/30 bg-destructive/5 px-5 py-2 text-[11.5px] text-destructive">
                {actionError}
              </div>
            ) : null}
          </div>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={confirmDeleteSession !== null}
        onOpenChange={(value) => {
          if (!value) setConfirmDeleteSession(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Permanently delete "{confirmDeleteSession?.title ?? ''}"?
            </AlertDialogTitle>
            <AlertDialogDescription>
              This permanently removes the session and all of its conversation history, runs, and
              checkpoints. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pendingDeleteId !== null}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: 'destructive' })}
              disabled={pendingDeleteId !== null}
              onClick={(event) => {
                event.preventDefault()
                void handleConfirmDelete()
              }}
            >
              {pendingDeleteId !== null ? 'Deleting…' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function formatArchivedAt(iso: string | null): string {
  if (!iso) return 'recently'
  const parsed = Date.parse(iso)
  if (!Number.isFinite(parsed)) return 'recently'
  return new Date(parsed).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
