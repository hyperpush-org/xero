"use client"

import { useMemo } from "react"
import { formatDistanceToNow } from "date-fns"
import { ArrowRight, Bell, MessageSquare, X } from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { FloatingRightSidebarFrame } from "@/components/xero/floating-right-sidebar-frame"
import type { CompletedAgentSessionNotificationView } from "@/src/features/xero/use-xero-desktop-state"

interface SessionNotificationsSidebarProps {
  open: boolean
  notifications: readonly CompletedAgentSessionNotificationView[]
  onClose: () => void
  onOpenSession: (projectId: string, agentSessionId: string) => void
}

interface ProjectNotificationGroup {
  projectId: string
  projectName: string
  sessions: CompletedAgentSessionNotificationView[]
}

export function SessionNotificationsSidebar({
  open,
  notifications,
  onClose,
  onOpenSession,
}: SessionNotificationsSidebarProps) {
  const groups = useMemo(() => groupNotificationsByProject(notifications), [notifications])

  return (
    <FloatingRightSidebarFrame
      label="Session notifications"
      onOverlayClick={onClose}
      open={open}
      width={360}
    >
      <div className="flex min-h-0 flex-1 flex-col">
        <header className="flex items-center justify-between gap-2 border-b border-border/60 px-2 py-1">
          <div className="min-w-0">
            <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
              Unread sessions
            </p>
          </div>
          <Button
            aria-label="Close notifications"
            className="text-muted-foreground hover:text-foreground"
            onClick={onClose}
            size="icon-xs"
            type="button"
            variant="ghost"
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
          {groups.length > 0 ? (
            <div className="space-y-3">
              {groups.map((group) => (
                <section key={group.projectId} aria-label={group.projectName} className="space-y-2">
                  <div className="px-1">
                    <h3 className="truncate text-[11px] font-medium uppercase text-muted-foreground">
                      {group.projectName}
                    </h3>
                  </div>
                  <div className="space-y-1">
                    {group.sessions.map((session) => (
                      <button
                        key={`${session.projectId}:${session.agentSessionId}`}
                        className={cn(
                          "group flex w-full items-center gap-2 rounded-md border border-foreground/10 bg-foreground/[0.035] px-2 py-2 text-left",
                          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                        )}
                        onClick={() => onOpenSession(session.projectId, session.agentSessionId)}
                        type="button"
                      >
                        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-foreground/[0.055] text-muted-foreground">
                          <MessageSquare className="h-3.5 w-3.5" />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-xs font-medium text-foreground">
                            {session.sessionTitle}
                          </span>
                          <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                            Finished {formatCompletionTime(session.completedAt)}
                          </span>
                        </span>
                        <ArrowRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground group-hover:text-primary" />
                      </button>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          ) : (
            <div className="flex h-full min-h-40 flex-col items-center justify-center gap-2 text-center">
              <span className="flex h-9 w-9 items-center justify-center rounded-md bg-foreground/5 text-muted-foreground">
                <Bell className="h-4 w-4" />
              </span>
              <p className="text-sm font-medium text-foreground">No unseen responses</p>
            </div>
          )}
        </div>
      </div>
    </FloatingRightSidebarFrame>
  )
}

function groupNotificationsByProject(
  notifications: readonly CompletedAgentSessionNotificationView[],
): ProjectNotificationGroup[] {
  const groups = new Map<string, ProjectNotificationGroup>()

  for (const notification of notifications) {
    const group = groups.get(notification.projectId)
    if (group) {
      group.sessions.push(notification)
      continue
    }

    groups.set(notification.projectId, {
      projectId: notification.projectId,
      projectName: notification.projectName,
      sessions: [notification],
    })
  }

  return Array.from(groups.values())
}

function formatCompletionTime(value: string): string {
  const parsed = new Date(value)
  if (Number.isNaN(parsed.getTime())) {
    return "just now"
  }

  return formatDistanceToNow(parsed, { addSuffix: true })
}
