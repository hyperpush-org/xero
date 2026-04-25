import { useEffect, useState } from "react"
import { Bell, Check, LoaderCircle, Plus } from "lucide-react"
import type { AgentPaneView } from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  NotificationRouteKindDto,
  UpsertNotificationRouteRequestDto,
} from "@/src/lib/cadence-model"
import { DiscordIcon, TelegramIcon } from "@/components/cadence/brand-icons"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import {
  ROUTE_KINDS,
  defaultRouteForm,
  parseRouteFormErrors,
  routeFormErrorMessage,
  routeTargetDisplay,
  toEditableRouteForm,
  toRouteRequest,
  type RouteFormErrors,
  type RouteFormValues,
} from "./route-form"
import { SectionHeader } from "./section-header"

type RoutePending = "save" | "toggle" | null

const CHANNELS: Array<{
  kind: NotificationRouteKindDto
  label: string
  description: string
  Icon: React.ElementType
}> = [
  { kind: "telegram", label: "Telegram", description: "Route operator prompts via Telegram", Icon: TelegramIcon },
  { kind: "discord", label: "Discord", description: "Route operator prompts via Discord", Icon: DiscordIcon },
]

interface NotificationsSectionProps {
  agent: AgentPaneView
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
}

function FieldError({ msg }: { msg?: string }) {
  if (!msg) return null
  return <p className="text-[12px] text-destructive">{msg}</p>
}

export function NotificationsSection({ agent, onUpsertNotificationRoute }: NotificationsSectionProps) {
  const hasBind = Boolean(agent.repositoryPath?.trim())
  const canMutate = hasBind && typeof onUpsertNotificationRoute === "function"
  const routes = agent.notificationRoutes ?? []
  const isMutating = (agent.notificationRouteMutationStatus ?? "idle") === "running"
  const pendingRouteId = agent.pendingNotificationRouteId ?? null

  const [pending, setPending] = useState<RoutePending>(null)
  const [formKind, setFormKind] = useState<NotificationRouteKindDto | null>(null)
  const [form, setForm] = useState<RouteFormValues>(() => defaultRouteForm())
  const [formErrors, setFormErrors] = useState<RouteFormErrors>({})
  const [formError, setFormError] = useState<string | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)

  useEffect(() => {
    setForm(defaultRouteForm())
    setFormErrors({})
    setFormError(null)
    setEditingId(null)
    setFormKind(null)
  }, [agent.project.id])

  const kindOption = ROUTE_KINDS.find((option) => option.value === form.routeKind) ?? ROUTE_KINDS[0]

  function setField<F extends keyof RouteFormValues>(field: F, value: RouteFormValues[F]) {
    setForm((previous) => ({ ...previous, [field]: value }))
    setFormErrors((previous) => {
      const key = field as keyof RouteFormErrors
      if (!previous[key] && !previous.form) return previous

      const next = { ...previous }
      delete next[key]
      delete next.form
      return next
    })
  }

  function startNew(kind: NotificationRouteKindDto) {
    setEditingId(null)
    setForm(defaultRouteForm(kind))
    setFormErrors({})
    setFormError(null)
    setFormKind(kind)
  }

  function editRoute(route: AgentPaneView["notificationRoutes"][number]) {
    setEditingId(route.routeId)
    setForm(toEditableRouteForm(route))
    setFormErrors({})
    setFormError(null)
    setFormKind(route.routeKind)
  }

  function cancelForm() {
    setFormKind(null)
    setEditingId(null)
    setFormErrors({})
    setFormError(null)
  }

  async function save() {
    if (!canMutate || !onUpsertNotificationRoute) return

    let request: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">
    try {
      request = toRouteRequest(form)
      setFormErrors({})
    } catch (error) {
      setFormErrors(parseRouteFormErrors(error))
      return
    }

    setPending("save")
    setFormError(null)
    try {
      await onUpsertNotificationRoute(request)
      setFormKind(null)
      setEditingId(null)
    } catch (error) {
      setFormError(routeFormErrorMessage(error, "Could not save route."))
    } finally {
      setPending(null)
    }
  }

  async function toggleRoute(route: AgentPaneView["notificationRoutes"][number]) {
    if (!canMutate || !onUpsertNotificationRoute) return

    setPending("toggle")
    try {
      await onUpsertNotificationRoute({
        routeId: route.routeId,
        routeKind: route.routeKind,
        routeTarget: route.routeTarget,
        enabled: !route.enabled,
        metadataJson: route.metadataJson ?? null,
      })
    } catch (error) {
      setFormError(routeFormErrorMessage(error, `Could not update ${route.routeId}.`))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        icon={Bell}
        title="Notifications"
        description="Route operator prompts to Telegram or Discord. Each route belongs to this project."
        scope="project-bound"
      />

      <div className="grid gap-3">
        {CHANNELS.map(({ kind, label, description, Icon }) => {
          const channelRoutes = routes.filter((route) => route.routeKind === kind)
          const formOpen = formKind === kind
          const hasRoutes = channelRoutes.length > 0

          return (
            <div
              key={kind}
              className={cn(
                "rounded-lg border bg-card px-5 py-4 transition-colors",
                hasRoutes ? "border-border" : "border-border/70",
              )}
            >
              <div className="flex items-center gap-3.5">
                <div
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors",
                    hasRoutes
                      ? "border-primary/30 bg-primary/[0.08]"
                      : "border-border bg-secondary/60",
                  )}
                >
                  <Icon className={cn("h-4 w-4", hasRoutes ? "text-primary" : "text-foreground/70")} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="text-[14px] font-medium text-foreground">{label}</p>
                    {hasRoutes ? (
                      <Badge variant="secondary" className="h-[18px] px-1.5 text-[10.5px] font-medium">
                        {channelRoutes.length} {channelRoutes.length === 1 ? "route" : "routes"}
                      </Badge>
                    ) : null}
                  </div>
                  <p className="mt-0.5 text-[12px] text-muted-foreground">{description}</p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {!hasRoutes && !formOpen ? (
                    <>
                      <Badge variant="outline" className="text-[11px] text-muted-foreground">
                        Not configured
                      </Badge>
                      <Button size="sm" className="h-8 text-[12px]" disabled={!canMutate} onClick={() => startNew(kind)}>
                        <Plus className="h-3.5 w-3.5" />
                        Add route
                      </Button>
                    </>
                  ) : !formOpen ? (
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8 text-[12px]"
                      disabled={!canMutate}
                      onClick={() => startNew(kind)}
                    >
                      <Plus className="h-3.5 w-3.5" />
                      Add
                    </Button>
                  ) : null}
                </div>
              </div>

              {hasRoutes ? (
                <div className="mt-3.5 grid gap-0.5 border-t border-border pt-2.5">
                  {channelRoutes.map((route) => {
                    const busy = pendingRouteId === route.routeId && (isMutating || pending === "toggle")
                    const isActiveEdit = editingId === route.routeId && formOpen

                    return (
                      <div
                        key={route.routeId}
                        className={cn(
                          "-mx-1.5 flex items-center gap-2 rounded-md px-2.5 py-2 transition-colors",
                          isActiveEdit ? "bg-secondary/50" : "hover:bg-secondary/30",
                        )}
                      >
                        <div className="min-w-0 flex-1">
                          <p className="text-[13px] leading-none font-medium text-foreground">{route.routeId}</p>
                          <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                            {routeTargetDisplay(route.routeKind, route.routeTarget)}
                          </p>
                        </div>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 px-2.5 text-[12px] text-muted-foreground hover:text-foreground"
                          onClick={() => editRoute(route)}
                        >
                          Edit
                        </Button>
                        <div className="flex items-center gap-2">
                          <Label htmlFor={`rt-${route.routeId}`} className="w-6 text-[11px] text-muted-foreground">
                            {route.enabled ? "On" : "Off"}
                          </Label>
                          <Switch
                            id={`rt-${route.routeId}`}
                            checked={route.enabled}
                            onCheckedChange={() => void toggleRoute(route)}
                            disabled={!canMutate || busy}
                          />
                        </div>
                      </div>
                    )
                  })}
                </div>
              ) : null}

              {formOpen ? (
                <div
                  className={cn(
                    "animate-in fade-in-0 slide-in-from-top-1 motion-enter",
                    hasRoutes ? "mt-2.5" : "mt-3.5",
                  )}
                >
                  <p className="mb-3 text-[13px] font-medium text-foreground">
                    {editingId ? `Edit — ${editingId}` : `New ${label} route`}
                  </p>
                  <div className="grid gap-3.5">
                    <div className="grid grid-cols-2 gap-3.5">
                      <div className="space-y-2">
                        <Label htmlFor={`s-route-id-${kind}`} className="text-[12px]">
                          Route name
                        </Label>
                        <Input
                          id={`s-route-id-${kind}`}
                          className="h-9 text-[13px]"
                          disabled={isMutating || pending === "save"}
                          onChange={(event) => setField("routeId", event.target.value)}
                          placeholder="e.g. ops-alerts"
                          value={form.routeId}
                        />
                        <FieldError msg={formErrors.routeId} />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor={`s-route-target-${kind}`} className="text-[12px]">
                          Target
                        </Label>
                        <Input
                          id={`s-route-target-${kind}`}
                          className="h-9 font-mono text-[13px]"
                          disabled={isMutating || pending === "save"}
                          onChange={(event) => setField("routeTarget", event.target.value)}
                          placeholder={kindOption.placeholder}
                          value={form.routeTarget}
                        />
                        <FieldError msg={formErrors.routeTarget} />
                      </div>
                    </div>
                    {formErrors.form || formError ? (
                      <p className="text-[12.5px] text-destructive">{formErrors.form ?? formError}</p>
                    ) : null}
                    <div className="flex items-center gap-2.5">
                      <Button
                        size="sm"
                        className="h-8 gap-1.5 text-[12px]"
                        disabled={!canMutate || isMutating || pending === "save"}
                        onClick={() => void save()}
                      >
                        {pending === "save" || isMutating ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <Check className="h-3.5 w-3.5" />
                        )}
                        {editingId ? "Save changes" : "Create route"}
                      </Button>
                      <Button size="sm" variant="ghost" className="h-8 text-[12px]" onClick={cancelForm}>
                        Cancel
                      </Button>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          )
        })}
      </div>
    </div>
  )
}
