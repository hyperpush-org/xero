"use client"

import { useMemo } from "react"
import { formatDistanceToNow } from "date-fns"
import { ArrowRight, Bell, MessageSquare, X } from "lucide-react"

import { cn } from "@/lib/utils"
import { FloatingRightSidebarFrame } from "@/components/xero/floating-right-sidebar-frame"
import {
  FloatingRightSidebarHeader,
  FloatingRightSidebarHeaderButton,
} from "@/components/xero/floating-right-sidebar-header"
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
        <FloatingRightSidebarHeader
          title="Unread sessions"
          actions={
            <FloatingRightSidebarHeaderButton
              aria-label="Close notifications"
              onClick={onClose}
            >
              <X className="h-3.5 w-3.5" />
            </FloatingRightSidebarHeaderButton>
          }
        />

        <div className="min-h-0 flex-1 overflow-y-auto scrollbar-thin">
          {groups.length > 0 ? (
            <div className="flex flex-col py-1">
              {groups.map((group) => (
                <section key={group.projectId} aria-label={group.projectName}>
                  <div className="px-3 pb-1 pt-2">
                    <h3 className="truncate text-[10.5px] font-medium uppercase tracking-wide text-muted-foreground">
                      {group.projectName}
                    </h3>
                  </div>
                  <ul className="flex flex-col">
                    {group.sessions.map((session) => (
                      <li key={`${session.projectId}:${session.agentSessionId}`}>
                        <button
                          className={cn(
                            "group relative flex w-full items-start gap-3 px-3 py-3 text-left transition-colors",
                            "hover:bg-secondary/30",
                            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring",
                          )}
                          onClick={() => onOpenSession(session.projectId, session.agentSessionId)}
                          type="button"
                        >
                          <MessageSquare
                            aria-hidden="true"
                            className="mt-[3px] h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                          />
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-[13px] font-medium leading-tight text-foreground">
                              {session.sessionTitle}
                            </span>
                            <span className="mt-0.5 block truncate text-[11.5px] leading-snug text-muted-foreground">
                              Finished {formatCompletionTime(session.completedAt)}
                            </span>
                          </span>
                          <span className="flex shrink-0 items-center self-center pl-1">
                            <ArrowRight className="h-3.5 w-3.5 text-muted-foreground group-hover:text-primary" />
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
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
