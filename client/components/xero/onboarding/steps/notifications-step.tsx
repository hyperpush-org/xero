import { useEffect, useRef, useState } from "react"
import { AlertCircle, LoaderCircle } from "lucide-react"
import { DiscordIcon, TelegramIcon } from "@/components/xero/brand-icons"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type {
  NotificationRouteHealthView,
  NotificationRouteMutationStatus,
  OperatorActionErrorView,
} from "@/src/features/xero/use-xero-desktop-state"
import {
  composeNotificationRouteTarget,
  type NotificationRouteKindDto,
  type UpsertNotificationRouteRequestDto,
} from "@/src/lib/xero-model"
import { routeTargetDisplay } from "@/components/xero/settings-dialog/route-form"
import { cn } from "@/lib/utils"
import { StepHeader } from "./providers-step"

interface NotificationsStepProps {
  projectName: string | null
  routes: NotificationRouteHealthView[]
  mutationStatus: NotificationRouteMutationStatus
  pendingRouteId: string | null
  mutationError: OperatorActionErrorView | null
  onUpsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, "projectId">,
  ) => Promise<unknown>
}

const CHANNELS: Array<{
  kind: NotificationRouteKindDto
  label: string
  description: string
  placeholder: string
  Icon: React.ElementType
}> = [
  {
    kind: "telegram",
    label: "Telegram",
    description: "Send run alerts to a chat or channel.",
    placeholder: "Chat ID or @channel",
    Icon: TelegramIcon,
  },
  {
    kind: "discord",
    label: "Discord",
    description: "Post updates into a Discord channel.",
    placeholder: "Channel ID",
    Icon: DiscordIcon,
  },
]

export function NotificationsStep({
  projectName,
  routes,
  mutationStatus,
  pendingRouteId,
  mutationError,
  onUpsertNotificationRoute,
}: NotificationsStepProps) {
  const projectReady = Boolean(projectName)

  return (
    <div>
      <StepHeader
        title="Add notification routes"
        description="Notification routes stay project-bound. Providers are app-wide, but delivery targets belong to the repository you imported."
      />

      {!projectReady ? (
        <div className="mt-7 flex items-start gap-3 rounded-lg border border-dashed border-border bg-card/30 px-4 py-3.5 text-[12px] leading-relaxed text-muted-foreground">
          <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-border bg-secondary/70 text-[10px] font-semibold text-muted-foreground">
            !
          </span>
          <span>Import a project first, then come back here to add Telegram or Discord routes.</span>
        </div>
      ) : null}

      <div className="mt-7 flex flex-col gap-2 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
        {CHANNELS.map((channel) => {
          const channelRoutes = routes.filter((route) => route.routeKind === channel.kind)
          const route = channelRoutes.find((item) => item.enabled) ?? channelRoutes[0] ?? null

          return (
            <ChannelRow
              key={channel.kind}
              kind={channel.kind}
              label={channel.label}
              description={channel.description}
              placeholder={channel.placeholder}
              Icon={channel.Icon}
              projectReady={projectReady}
              route={route}
              mutationStatus={mutationStatus}
              pendingRouteId={pendingRouteId}
              mutationError={mutationError}
              onUpsertNotificationRoute={onUpsertNotificationRoute}
            />
          )
        })}
      </div>
    </div>
  )
}

interface ChannelRowProps {
  kind: NotificationRouteKindDto
  label: string
  description: string
  placeholder: string
  Icon: React.ElementType
  projectReady: boolean
  route: NotificationRouteHealthView | null
  mutationStatus: NotificationRouteMutationStatus
  pendingRouteId: string | null
  mutationError: OperatorActionErrorView | null
  onUpsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, "projectId">,
  ) => Promise<unknown>
}

function ChannelRow({
  kind,
  label,
  description,
  placeholder,
  Icon,
  projectReady,
  route,
  mutationStatus,
  pendingRouteId,
  mutationError,
  onUpsertNotificationRoute,
}: ChannelRowProps) {
  const [open, setOpen] = useState(false)
  const [target, setTarget] = useState("")
  const [formError, setFormError] = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const routeId = route?.routeId ?? `${kind}-primary`
  const busy = mutationStatus === "running" && pendingRouteId === routeId
  const enabled = Boolean(route?.enabled)

  useEffect(() => {
    if (open) {
      const id = requestAnimationFrame(() => inputRef.current?.focus())
      return () => cancelAnimationFrame(id)
    }
    return
  }, [open])

  useEffect(() => {
    if (route) {
      setTarget(routeTargetDisplay(kind, route.routeTarget))
      return
    }

    setTarget("")
  }, [kind, route])

  async function handleSave() {
    const trimmedTarget = target.trim()
    if (!trimmedTarget) {
      setFormError("Target is required.")
      return
    }

    setFormError(null)

    try {
      await onUpsertNotificationRoute({
        routeId,
        routeKind: kind,
        routeTarget: composeNotificationRouteTarget(kind, trimmedTarget),
        enabled: true,
        metadataJson: null,
        updatedAt: new Date().toISOString(),
      })

      setOpen(false)
    } catch {
      // Mutation state surfaces the typed error; preserve the current input for correction.
    }
  }

  async function handleDisable() {
    if (!route) return

    setFormError(null)

    try {
      await onUpsertNotificationRoute({
        routeId: route.routeId,
        routeKind: route.routeKind,
        routeTarget: route.routeTarget,
        enabled: false,
        metadataJson: route.metadataJson,
        updatedAt: new Date().toISOString(),
      })
    } catch {
      // Mutation state surfaces the typed error; preserve the current route view.
    }
  }

  const readinessStatus = route?.credentialReadiness?.status ?? null
  const readinessLabel =
    readinessStatus === "ready"
      ? "Ready"
      : readinessStatus === "missing"
        ? "Needs credentials"
        : readinessStatus === "malformed"
          ? "Fix credentials"
          : readinessStatus === "unavailable"
            ? "Credentials unavailable"
            : null

  return (
    <div
      className={cn(
        "group/card relative rounded-lg border bg-card/40 px-3.5 py-3 transition-[background-color,border-color,box-shadow,opacity] motion-fast",
        !projectReady
          ? "border-border/60 bg-card/20"
          : enabled
            ? "border-primary/40 bg-primary/[0.03]"
            : "border-border hover:border-border/80 hover:bg-card/60",
      )}
    >
      {enabled ? (
        <span
          aria-hidden
          className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-primary/70"
        />
      ) : null}

      <div className="flex items-center gap-3">
        <span
          className={cn(
            "flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors",
            enabled
              ? "border-primary/40 bg-primary/10 text-primary"
              : "border-border bg-secondary/50 text-foreground/75",
            !projectReady && "opacity-50",
          )}
        >
          <Icon className="h-4 w-4" />
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="text-[13px] font-medium text-foreground">{label}</p>
            {enabled ? (
              <Badge
                variant="secondary"
                className="gap-1 border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0 text-[10px] font-medium text-emerald-500 dark:text-emerald-400"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500 dark:bg-emerald-400" />
                Enabled
              </Badge>
            ) : route ? (
              <Badge variant="outline" className="px-1.5 py-0 text-[10px] text-muted-foreground">
                Disabled
              </Badge>
            ) : null}
            {readinessLabel ? (
              <Badge
                variant={readinessStatus === "ready" ? "default" : "outline"}
                className={cn(
                  "px-1.5 py-0 text-[10px]",
                  readinessStatus !== "ready" && "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400",
                )}
              >
                {readinessLabel}
              </Badge>
            ) : null}
          </div>
          {route ? (
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
              {routeTargetDisplay(kind, route.routeTarget)}
            </p>
          ) : (
            <p className="mt-0.5 truncate text-[11px] text-muted-foreground">{description}</p>
          )}
        </div>

        {enabled && !open ? (
          <div className="flex items-center gap-0.5">
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-[11px] text-muted-foreground hover:text-foreground"
              disabled={!projectReady || busy}
              onClick={() => {
                setOpen(true)
                setFormError(null)
              }}
            >
              Edit
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-[11px] text-muted-foreground hover:text-destructive"
              disabled={!projectReady || busy}
              onClick={() => void handleDisable()}
            >
              {busy ? <LoaderCircle className="h-3 w-3 animate-spin" /> : "Disable"}
            </Button>
          </div>
        ) : !open ? (
          <Button
            size="sm"
            variant="outline"
            className="h-7 min-w-[96px] text-[11px]"
            disabled={!projectReady}
            onClick={() => {
              setOpen(true)
              setFormError(null)
            }}
          >
            {route ? "Update" : "Add route"}
          </Button>
        ) : null}
      </div>

      <div
        aria-hidden={!open}
        className={cn(
          "grid transition-[grid-template-rows] motion-standard",
          open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
        )}
      >
        <div
          className={cn(
            "overflow-hidden transition-opacity motion-fast",
            open ? "opacity-100 delay-75" : "pointer-events-none opacity-0",
          )}
        >
          <div className="mt-3 flex flex-col gap-2.5">
            <div className="flex items-center gap-2">
              <Input
                ref={inputRef}
                value={target}
                onChange={(event) => setTarget(event.target.value)}
                placeholder={placeholder}
                className="h-8 font-mono text-[12px]"
                disabled={busy}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault()
                    void handleSave()
                  }

                  if (event.key === "Escape") {
                    setOpen(false)
                    setFormError(null)
                  }
                }}
              />
              <Button
                size="sm"
                onClick={() => void handleSave()}
                disabled={busy || !target.trim()}
                className="h-8 min-w-[84px] bg-primary text-[11px] hover:bg-primary/90"
              >
                {busy ? <LoaderCircle className="h-3 w-3 animate-spin" /> : "Save route"}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="h-8 text-[11px] text-muted-foreground hover:text-foreground"
                onClick={() => {
                  setOpen(false)
                  setFormError(null)
                }}
                disabled={busy}
              >
                Cancel
              </Button>
            </div>

            {mutationError || formError ? (
              <Alert variant="destructive" className="py-2.5">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription className="text-[12px]">
                  {formError ?? mutationError?.message ?? "Could not save the notification route."}
                </AlertDescription>
              </Alert>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  )
}
