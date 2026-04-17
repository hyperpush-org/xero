"use client"

import { useEffect, useState } from "react"
import { z } from "zod"
import { openUrl } from "@tauri-apps/plugin-opener"
import type { AgentPaneView } from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  NotificationRouteKindDto,
  RuntimeSessionView,
  UpsertNotificationRouteRequestDto,
} from "@/src/lib/cadence-model"
import {
  composeNotificationRouteTarget,
  decomposeNotificationRouteTarget,
  notificationRouteKindSchema,
} from "@/src/lib/cadence-model"
import {
  AlertCircle,
  Bell,
  Bot,
  Check,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
  Plus,
  Send,
  Trash2,
} from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function errMsg(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  if (typeof error === "string" && error.trim().length > 0) return error
  return fallback
}

function routeTargetDisplay(kind: NotificationRouteKindDto, target: string): string {
  try { return decomposeNotificationRouteTarget(kind, target).channelTarget }
  catch { return target || "—" }
}

// ---------------------------------------------------------------------------
// Route form
// ---------------------------------------------------------------------------

const routeFormSchema = z
  .object({
    routeId: z.string().trim().min(1, "Give this route an ID."),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1, "A target is required."),
    enabled: z.boolean(),
  })
  .strict()
  .superRefine((v, ctx) => {
    try { composeNotificationRouteTarget(v.routeKind, v.routeTarget) }
    catch (e) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["routeTarget"], message: errMsg(e, "Invalid target format.") })
    }
  })

type RouteFormValues = z.input<typeof routeFormSchema>
type RouteFormErrors = Partial<Record<"routeId" | "routeKind" | "routeTarget" | "form", string>>

function defaultRouteForm(kind: NotificationRouteKindDto = "telegram"): RouteFormValues {
  return { routeId: "", routeKind: kind, routeTarget: "", enabled: true }
}

function parseFormErrors(error: unknown): RouteFormErrors {
  if (!(error instanceof z.ZodError)) return { form: errMsg(error, "Validation failed.") }
  const out: RouteFormErrors = {}
  for (const issue of error.issues) {
    const p = issue.path[0]
    if ((p === "routeId" || p === "routeKind" || p === "routeTarget") && !out[p]) {
      out[p] = issue.message; continue
    }
    if (!out.form) out.form = issue.message
  }
  return out
}

function toRouteRequest(form: RouteFormValues): Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt"> {
  const v = routeFormSchema.parse(form)
  return {
    routeId: v.routeId,
    routeKind: v.routeKind,
    routeTarget: composeNotificationRouteTarget(v.routeKind, v.routeTarget),
    enabled: v.enabled,
    metadataJson: null,
  }
}

const ROUTE_KINDS: Array<{ value: NotificationRouteKindDto; label: string; placeholder: string }> = [
  { value: "telegram", label: "Telegram", placeholder: "Chat ID or @channel" },
  { value: "discord", label: "Discord", placeholder: "Channel ID" },
]

function FieldError({ msg }: { msg?: string }) {
  if (!msg) return null
  return <p className="text-[12px] text-destructive">{msg}</p>
}

// ---------------------------------------------------------------------------
// Nav
// ---------------------------------------------------------------------------

type SettingsSection = "providers" | "notifications"

const NAV: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = [
  { id: "providers", label: "Providers", icon: KeyRound },
  { id: "notifications", label: "Notifications", icon: Bell },
]

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  agent: AgentPaneView | null
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onRefreshNotificationRoutes?: (options?: { force?: boolean }) => Promise<unknown>
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
}

// ---------------------------------------------------------------------------
// Main dialog
// ---------------------------------------------------------------------------

export function SettingsDialog({
  open, onOpenChange, agent,
  onStartLogin, onLogout,
  onRefreshNotificationRoutes, onUpsertNotificationRoute,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSection>("providers")

  useEffect(() => { if (open) setSection("providers") }, [open])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(560px,85vh)] w-[min(780px,92vw)] max-w-none sm:max-w-none flex-col gap-0 overflow-hidden p-0"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
          <DialogTitle className="text-sm">Settings</DialogTitle>
        </DialogHeader>

        <div className="flex min-h-0 flex-1">
          {/* Sidebar */}
          <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border bg-sidebar/50 px-2 py-3">
            {NAV.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                onClick={() => setSection(id)}
                className={cn(
                  "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-[12px] font-medium transition-colors text-left",
                  section === id
                    ? "bg-secondary text-foreground"
                    : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
                )}
              >
                <Icon className="h-4 w-4 shrink-0" />
                {label}
              </button>
            ))}
          </nav>

          {/* Content */}
          <ScrollArea className="flex-1">
            <div className="flex min-h-full flex-col px-6 py-5">
              {!agent ? (
                <div className="flex flex-col items-center justify-center py-16 text-center">
                  <p className="text-sm text-muted-foreground">Select a project to configure settings.</p>
                </div>
              ) : section === "providers" ? (
                <ProvidersSection
                  agent={agent}
                  onStartLogin={onStartLogin}
                  onLogout={onLogout}
                />
              ) : (
                <NotificationsSection
                  agent={agent}
                  onRefreshNotificationRoutes={onRefreshNotificationRoutes}
                  onUpsertNotificationRoute={onUpsertNotificationRoute}
                />
              )}
            </div>
          </ScrollArea>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ===========================================================================
// Providers Section — card-per-provider layout
// ===========================================================================

type AuthPending = "login" | "logout" | null

function ProvidersSection({ agent, onStartLogin, onLogout }: {
  agent: AgentPaneView
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
}) {
  const rs = agent.runtimeSession ?? null
  const hasBind = Boolean(agent.repositoryPath?.trim())
  const isConnected = Boolean(rs?.isAuthenticated)
  const isInProgress = Boolean(rs?.isLoginInProgress)

  const [pending, setPending] = useState<AuthPending>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => { setError(null) }, [rs?.isAuthenticated, rs?.updatedAt])

  async function handleConnect() {
    if (!hasBind || !onStartLogin) return
    setPending("login"); setError(null)
    try {
      const next = await onStartLogin()
      if (next?.authorizationUrl) {
        try { await openUrl(next.authorizationUrl) } catch { /* browser open failed — login still started */ }
      }
    } catch (e) { setError(errMsg(e, "Could not start login.")) }
    finally { setPending(null) }
  }

  async function handleDisconnect() {
    if (!onLogout) return
    setPending("logout"); setError(null)
    try { await onLogout() }
    catch (e) { setError(errMsg(e, "Could not sign out.")) }
    finally { setPending(null) }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Providers</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Connect AI providers to power agent runtime sessions.
        </p>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Connection error</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="grid gap-2">
        {/* OpenAI */}
        <div className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Bot className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-[13px] font-medium text-foreground">OpenAI</p>
            <p className="text-[11px] text-muted-foreground">Codex agent runtime</p>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {isConnected ? (
              <>
                <Badge variant="default" className="gap-1 text-[10px]"><Check className="h-3 w-3" />Connected</Badge>
                <Button variant="ghost" size="sm" className="h-7 text-[11px]" disabled={pending !== null} onClick={() => void handleDisconnect()}>
                  {pending === "logout" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogOut className="h-3 w-3" />}
                  Disconnect
                </Button>
              </>
            ) : isInProgress ? (
              <Badge variant="secondary" className="gap-1 text-[10px]"><LoaderCircle className="h-3 w-3 animate-spin" />Connecting…</Badge>
            ) : (
              <>
                <Badge variant="outline" className="text-[10px]">Not connected</Badge>
                <Button size="sm" className="h-7 text-[11px]" disabled={!hasBind || pending !== null} onClick={() => void handleConnect()}>
                  {pending === "login" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogIn className="h-3 w-3" />}
                  Connect
                </Button>
              </>
            )}
          </div>
        </div>

        {/* Anthropic — coming soon */}
        <div className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 opacity-45">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Bot className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-[13px] font-medium text-foreground">Anthropic</p>
            <p className="text-[11px] text-muted-foreground">Claude agent runtime</p>
          </div>
          <Badge variant="outline" className="text-[10px]">Coming soon</Badge>
        </div>

        {/* Google — coming soon */}
        <div className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 opacity-45">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Bot className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-[13px] font-medium text-foreground">Google</p>
            <p className="text-[11px] text-muted-foreground">Gemini agent runtime</p>
          </div>
          <Badge variant="outline" className="text-[10px]">Coming soon</Badge>
        </div>
      </div>
    </div>
  )
}

// ===========================================================================
// Notifications Section — simplified route management
// ===========================================================================

type RoutePending = "save" | "toggle" | null

function NotificationsSection({ agent, onRefreshNotificationRoutes, onUpsertNotificationRoute }: {
  agent: AgentPaneView
  onRefreshNotificationRoutes?: (opts?: { force?: boolean }) => Promise<unknown>
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
}) {
  const hasBind = Boolean(agent.repositoryPath?.trim())
  const canMutate = hasBind && typeof onUpsertNotificationRoute === "function"
  const routes = agent.notificationRoutes ?? []
  const isMutating = (agent.notificationRouteMutationStatus ?? "idle") === "running"
  const pendingRouteId = agent.pendingNotificationRouteId ?? null

  const [pending, setPending] = useState<RoutePending>(null)
  const [showForm, setShowForm] = useState(false)
  const [form, setForm] = useState<RouteFormValues>(() => defaultRouteForm())
  const [formErrors, setFormErrors] = useState<RouteFormErrors>({})
  const [msg, setMsg] = useState<string | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)

  useEffect(() => {
    setForm(defaultRouteForm())
    setFormErrors({})
    setMsg(null)
    setEditingId(null)
    setShowForm(false)
  }, [agent.project.id])

  const kindOpt = ROUTE_KINDS.find((o) => o.value === form.routeKind) ?? ROUTE_KINDS[0]

  function setField<F extends keyof RouteFormValues>(field: F, value: RouteFormValues[F]) {
    setForm((p) => ({ ...p, [field]: value }))
    setFormErrors((p) => {
      const key = field as string as keyof RouteFormErrors
      if (!p[key] && !p.form) return p
      const n = { ...p }; delete n[key]; delete n.form; return n
    })
  }

  function startNew() {
    setEditingId(null)
    setForm(defaultRouteForm())
    setFormErrors({})
    setMsg(null)
    setShowForm(true)
  }

  function editRoute(r: AgentPaneView["notificationRoutes"][number]) {
    let target = r.routeTarget
    try { target = decomposeNotificationRouteTarget(r.routeKind, r.routeTarget).channelTarget } catch {}
    setEditingId(r.routeId)
    setForm({ routeId: r.routeId, routeKind: r.routeKind, routeTarget: target, enabled: r.enabled })
    setFormErrors({})
    setMsg(null)
    setShowForm(true)
  }

  async function save() {
    if (!canMutate || !onUpsertNotificationRoute) return
    let req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">
    try { req = toRouteRequest(form); setFormErrors({}) }
    catch (e) { setFormErrors(parseFormErrors(e)); return }
    setPending("save"); setMsg(null)
    try {
      await onUpsertNotificationRoute(req)
      setMsg(`Saved ${req.routeId}.`)
      setEditingId(req.routeId)
    } catch (e) { setMsg(errMsg(e, "Could not save route.")) }
    finally { setPending(null) }
  }

  async function toggleRoute(r: AgentPaneView["notificationRoutes"][number]) {
    if (!canMutate || !onUpsertNotificationRoute) return
    setPending("toggle"); setMsg(null)
    try {
      await onUpsertNotificationRoute({
        routeId: r.routeId, routeKind: r.routeKind, routeTarget: r.routeTarget,
        enabled: !r.enabled, metadataJson: r.metadataJson ?? null,
      })
    } catch (e) { setMsg(errMsg(e, `Could not update ${r.routeId}.`)) }
    finally { setPending(null) }
  }

  return (
    <div className="flex min-h-full flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold text-foreground">Notifications</h3>
          <p className="mt-1 text-[12px] text-muted-foreground">
            Route operator prompts to Telegram or Discord so you can respond from anywhere.
          </p>
        </div>
        <Button size="sm" variant="outline" onClick={startNew} className="shrink-0 h-7 text-[11px]">
          <Plus className="h-3.5 w-3.5" />
          Add route
        </Button>
      </div>

      {msg && (
        <Alert>
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{msg}</AlertDescription>
        </Alert>
      )}

      {/* Route list */}
      {routes.length > 0 ? (
        <div className="grid gap-2">
          {routes.map((r) => {
            const busy = pendingRouteId === r.routeId && (isMutating || pending === "toggle")
            return (
              <div key={r.routeId} className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-2.5">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <p className="text-[13px] font-medium text-foreground">{r.routeId}</p>
                    <Badge variant="outline" className="text-[10px]">{r.routeKindLabel}</Badge>
                  </div>
                  <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                    {routeTargetDisplay(r.routeKind, r.routeTarget)}
                  </p>
                </div>
                <div className="flex items-center gap-3 shrink-0">
                  <Button variant="ghost" size="sm" onClick={() => editRoute(r)} className="text-[11px] h-7">Edit</Button>
                  <div className="flex items-center gap-2">
                    <Label htmlFor={`route-toggle-${r.routeId}`} className="text-[11px] text-muted-foreground">
                      {r.enabled ? "On" : "Off"}
                    </Label>
                    <Switch
                      id={`route-toggle-${r.routeId}`}
                      checked={r.enabled}
                      onCheckedChange={() => void toggleRoute(r)}
                      disabled={!canMutate || busy}
                    />
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      ) : !showForm ? (
        <div className="flex flex-1 flex-col items-center justify-center text-center">
          <Bell className="h-6 w-6 text-muted-foreground/30" />
          <p className="mt-2 text-[13px] font-medium text-foreground/80">No routes configured</p>
          <p className="mt-1 max-w-[260px] text-[11px] leading-relaxed text-muted-foreground">
            Add a Telegram or Discord route to receive operator prompts remotely.
          </p>
          <Button size="sm" variant="outline" onClick={startNew} className="mt-3 h-7 text-[11px]">
            <Plus className="h-3.5 w-3.5" />
            Add your first route
          </Button>
        </div>
      ) : null}

      {/* Add / Edit form */}
      {showForm && (
        <>
          <Separator />
          <div className="rounded-lg border border-border bg-card p-4">
            <div className="mb-3">
              <p className="text-[13px] font-medium text-foreground">{editingId ? `Edit — ${editingId}` : "New route"}</p>
              <p className="text-[11px] text-muted-foreground">
                {editingId ? "Update this notification route." : "Where should operator prompts be sent?"}
              </p>
            </div>
            <div className="grid gap-3">
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <Label htmlFor="s-route-kind" className="text-[11px]">Channel</Label>
                  <Select
                    value={form.routeKind}
                    onValueChange={(v) => setField("routeKind", v as NotificationRouteKindDto)}
                    disabled={isMutating || pending === "save"}
                  >
                    <SelectTrigger id="s-route-kind" className="h-8 text-[12px]"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      {ROUTE_KINDS.map((o) => <SelectItem key={o.value} value={o.value}>{o.label}</SelectItem>)}
                    </SelectContent>
                  </Select>
                  <FieldError msg={formErrors.routeKind} />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="s-route-id" className="text-[11px]">Route name</Label>
                  <Input id="s-route-id" className="h-8 text-[12px]" disabled={isMutating || pending === "save"} onChange={(e) => setField("routeId", e.target.value)} placeholder="e.g. ops-alerts" value={form.routeId} />
                  <FieldError msg={formErrors.routeId} />
                </div>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="s-route-target" className="text-[11px]">Target</Label>
                <Input id="s-route-target" className="h-8 text-[12px]" disabled={isMutating || pending === "save"} onChange={(e) => setField("routeTarget", e.target.value)} placeholder={kindOpt.placeholder} value={form.routeTarget} />
                <FieldError msg={formErrors.routeTarget} />
              </div>
              <div className="flex items-center gap-2">
                <Switch id="s-route-enabled" checked={form.enabled} onCheckedChange={(v) => setField("enabled", v)} disabled={isMutating || pending === "save"} />
                <Label htmlFor="s-route-enabled" className="text-[11px] text-muted-foreground">Enable immediately</Label>
              </div>
              <FieldError msg={formErrors.form} />
              <div className="flex items-center gap-2">
                <Button size="sm" className="h-7 text-[11px]" disabled={!canMutate || isMutating || pending === "save"} onClick={() => void save()}>
                  {pending === "save" || isMutating ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Send className="h-3 w-3" />}
                  {editingId ? "Save changes" : "Create route"}
                </Button>
                <Button size="sm" variant="ghost" className="h-7 text-[11px]" onClick={() => { setShowForm(false); setEditingId(null); setFormErrors({}) }}>
                  Cancel
                </Button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}
