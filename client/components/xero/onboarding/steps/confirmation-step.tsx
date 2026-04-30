import type { NotificationRouteHealthView } from "@/src/features/xero/use-xero-desktop-state"
import { Bell, Check, FolderGit2, Sparkles } from "lucide-react"
import { cn } from "@/lib/utils"
import { StepHeader } from "./providers-step"

interface ConfirmationStepProps {
  providerValue: string
  providerReady: boolean
  projectName: string | null
  notifications: NotificationRouteHealthView[]
}

export function ConfirmationStep({ providerValue, providerReady, projectName, notifications }: ConfirmationStepProps) {
  const enabledRoutes = notifications.filter((route) => route.enabled)

  const rows: Array<{ label: string; value: string; ready: boolean; Icon: React.ElementType }> = [
    {
      label: "Provider",
      ready: providerReady,
      value: providerValue,
      Icon: Sparkles,
    },
    {
      label: "Project",
      ready: Boolean(projectName),
      value: projectName ?? "Not imported",
      Icon: FolderGit2,
    },
    {
      label: "Notifications",
      ready: enabledRoutes.length > 0,
      value:
        enabledRoutes.length === 0
          ? "Not configured"
          : enabledRoutes.map((route) => route.routeKindLabel).join(", "),
      Icon: Bell,
    },
  ]

  const readyCount = rows.filter((row) => row.ready).length

  return (
    <div>
      <StepHeader title="Review and finish" description="You can change any of this later from the main app and Settings." />

      <div className="mt-7 flex flex-col gap-2 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
        {rows.map((row) => (
          <div
            key={row.label}
            className={cn(
              "flex items-center gap-3 rounded-lg border px-3.5 py-3 transition-colors",
              row.ready
                ? "border-primary/40 bg-primary/[0.04]"
                : "border-border/70 bg-card/30",
            )}
          >
            <span
              className={cn(
                "flex h-9 w-9 shrink-0 items-center justify-center rounded-md border",
                row.ready
                  ? "border-primary/40 bg-primary/10 text-primary"
                  : "border-border bg-secondary/50 text-muted-foreground",
              )}
            >
              <row.Icon className="h-4 w-4" />
            </span>

            <div className="min-w-0 flex-1">
              <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{row.label}</p>
              <p
                className={cn(
                  "mt-0.5 truncate text-[13px]",
                  row.ready ? "font-medium text-foreground" : "text-muted-foreground",
                )}
              >
                {row.value}
              </p>
            </div>

            <span
              className={cn(
                "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border",
                row.ready
                  ? "border-primary/40 bg-primary/10 text-primary"
                  : "border-border bg-secondary/50 text-muted-foreground/60",
              )}
              aria-label={row.ready ? "Ready" : "Not set"}
            >
              {row.ready ? (
                <Check className="h-3 w-3" strokeWidth={3} />
              ) : (
                <span className="h-1 w-1 rounded-full bg-muted-foreground/50" />
              )}
            </span>
          </div>
        ))}
      </div>

      <p className="mt-5 text-center text-[11px] text-muted-foreground/80">
        {readyCount === rows.length
          ? "Everything’s set. You’re ready to enter Xero."
          : `${readyCount} of ${rows.length} steps ready — skip anything you’d rather configure later.`}
      </p>
    </div>
  )
}
