"use client"

import { useEffect, useState } from "react"
import { z } from "zod"
import { openUrl } from "@tauri-apps/plugin-opener"
import type {
  AgentPaneView,
  OperatorActionErrorView,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  NotificationRouteKindDto,
  RuntimeSessionView,
  RuntimeSettingsDto,
  UpsertNotificationRouteRequestDto,
  UpsertRuntimeSettingsRequestDto,
} from "@/src/lib/cadence-model"
import type { PlatformVariant } from "@/components/cadence/shell"
import { detectPlatform } from "@/components/cadence/shell"
import {
  composeNotificationRouteTarget,
  decomposeNotificationRouteTarget,
  notificationRouteKindSchema,
} from "@/src/lib/cadence-model"
import {
  AlertCircle,
  Bell,
  Check,
  Code2,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
  Plus,
} from "lucide-react"
import {
  AnthropicIcon,
  DiscordIcon,
  GoogleIcon,
  OpenAIIcon,
  TelegramIcon,
} from "@/components/cadence/brand-icons"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
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
  try {
    return decomposeNotificationRouteTarget(kind, target).channelTarget
  } catch {
    return target || "—"
  }
}

function errorViewMessage(error: OperatorActionErrorView | null, fallback: string): string {
  if (error?.message?.trim()) return error.message
  return fallback
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
    try {
      composeNotificationRouteTarget(v.routeKind, v.routeTarget)
    } catch (e) {
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
      out[p] = issue.message
      continue
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

type SettingsSection = "providers" | "notifications" | "development"

const NAV_BASE: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = [
  { id: "providers", label: "Providers", icon: KeyRound },
  { id: "notifications", label: "Notifications", icon: Bell },
]

const NAV: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = import.meta.env.DEV
  ? [...NAV_BASE, { id: "development" as SettingsSection, label: "Development", icon: Code2 }]
  : NAV_BASE

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  agent: AgentPaneView | null
  runtimeSettings: RuntimeSettingsDto | null
  runtimeSettingsLoadStatus: RuntimeSettingsLoadStatus
  runtimeSettingsLoadError: OperatorActionErrorView | null
  runtimeSettingsSaveStatus: RuntimeSettingsSaveStatus
  runtimeSettingsSaveError: OperatorActionErrorView | null
  onRefreshRuntimeSettings?: (options?: { force?: boolean }) => Promise<RuntimeSettingsDto>
  onUpsertRuntimeSettings?: (request: UpsertRuntimeSettingsRequestDto) => Promise<RuntimeSettingsDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onRefreshNotificationRoutes?: (options?: { force?: boolean }) => Promise<unknown>
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
  /** Dev-only toolbar platform override */
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (v: PlatformVariant | null) => void
}

// ---------------------------------------------------------------------------
// Main dialog
// ---------------------------------------------------------------------------

export function SettingsDialog({
  open,
  onOpenChange,
  agent,
  runtimeSettings,
  runtimeSettingsLoadStatus,
  runtimeSettingsLoadError,
  runtimeSettingsSaveStatus,
  runtimeSettingsSaveError,
  onRefreshRuntimeSettings,
  onUpsertRuntimeSettings,
  onStartLogin,
  onLogout,
  onRefreshNotificationRoutes,
  onUpsertNotificationRoute,
  platformOverride,
  onPlatformOverrideChange,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSection>("providers")

  useEffect(() => {
    if (open) setSection("providers")
  }, [open])

  useEffect(() => {
    if (!open || !onRefreshRuntimeSettings) {
      return
    }

    void onRefreshRuntimeSettings({ force: true }).catch(() => undefined)
  }, [open, onRefreshRuntimeSettings])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(560px,85vh)] w-[min(780px,92vw)] max-w-none sm:max-w-none flex-col gap-0 overflow-hidden p-0"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
          <DialogTitle className="text-sm">Settings</DialogTitle>
          <DialogDescription className="sr-only">
            Configure app-global providers, selected-project notification routes, and development options.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1">
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

          <div className="flex flex-1 flex-col overflow-y-auto px-6 py-5">
            {section === "providers" ? (
              <ProvidersSection
                agent={agent}
                runtimeSettings={runtimeSettings}
                runtimeSettingsLoadStatus={runtimeSettingsLoadStatus}
                runtimeSettingsLoadError={runtimeSettingsLoadError}
                runtimeSettingsSaveStatus={runtimeSettingsSaveStatus}
                runtimeSettingsSaveError={runtimeSettingsSaveError}
                onRefreshRuntimeSettings={onRefreshRuntimeSettings}
                onUpsertRuntimeSettings={onUpsertRuntimeSettings}
                onStartLogin={onStartLogin}
                onLogout={onLogout}
              />
            ) : section === "notifications" ? (
              agent ? (
                <NotificationsSection
                  agent={agent}
                  onRefreshNotificationRoutes={onRefreshNotificationRoutes}
                  onUpsertNotificationRoute={onUpsertNotificationRoute}
                />
              ) : (
                <ProjectBoundEmptyState
                  title="Notifications require a selected project"
                  body="Provider settings are app-global, but notification routes stay project-bound so Cadence never writes cross-project delivery state into the wrong repository view."
                />
              )
            ) : section === "development" ? (
              <DevelopmentSection
                platformOverride={platformOverride}
                onPlatformOverrideChange={onPlatformOverrideChange}
              />
            ) : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ProjectBoundEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-1 items-center justify-center py-16 text-center">
      <div className="max-w-md rounded-xl border border-border bg-card px-6 py-8 shadow-sm">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}

// ===========================================================================
// Providers Section — card-based UI matching notifications pattern
// ===========================================================================

type AuthPending = "login" | "logout" | "configure" | null
type RuntimeProviderId = RuntimeSettingsDto["providerId"]

const PROVIDERS: Array<{
  id: RuntimeProviderId
  label: string
  description: string
  Icon: React.ElementType
}> = [
  {
    id: "openrouter",
    label: "OpenRouter",
    description: "App-global API key provider",
    Icon: KeyRound,
  },
  {
    id: "openai_codex",
    label: "OpenAI Codex",
    description: "Browser-based OAuth for desktop runtime",
    Icon: OpenAIIcon,
  },
]

function ProvidersSection({
  agent,
  runtimeSettings,
  runtimeSettingsLoadStatus,
  runtimeSettingsLoadError,
  runtimeSettingsSaveStatus,
  runtimeSettingsSaveError,
  onRefreshRuntimeSettings,
  onUpsertRuntimeSettings,
  onStartLogin,
  onLogout,
}: {
  agent: AgentPaneView | null
  runtimeSettings: RuntimeSettingsDto | null
  runtimeSettingsLoadStatus: RuntimeSettingsLoadStatus
  runtimeSettingsLoadError: OperatorActionErrorView | null
  runtimeSettingsSaveStatus: RuntimeSettingsSaveStatus
  runtimeSettingsSaveError: OperatorActionErrorView | null
  onRefreshRuntimeSettings?: (options?: { force?: boolean }) => Promise<RuntimeSettingsDto>
  onUpsertRuntimeSettings?: (request: UpsertRuntimeSettingsRequestDto) => Promise<RuntimeSettingsDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
}) {
  // Per-provider state
  const [configuringId, setConfiguringId] = useState<RuntimeProviderId | null>(null)
  const [openrouterModelId, setOpenrouterModelId] = useState("")
  const [openrouterApiKey, setOpenrouterApiKey] = useState("")
  const [clearOpenrouterApiKey, setClearOpenrouterApiKey] = useState(false)
  const [pending, setPending] = useState<AuthPending>(null)
  const [formError, setFormError] = useState<string | null>(null)

  const hasSelectedProject = Boolean(agent?.repositoryPath?.trim())
  const runtimeSession = agent?.runtimeSession ?? null
  const isOpenaiConnected = Boolean(runtimeSession?.isAuthenticated)
  const isOpenaiInProgress = Boolean(runtimeSession?.isLoginInProgress)
  const isSaving = runtimeSettingsSaveStatus === "running"

  const isProviderActive = runtimeSettings?.providerId

  // OpenRouter state
  const isOpenrouterActive = isProviderActive === "openrouter"
  const openrouterConfigured = runtimeSettings?.openrouterApiKeyConfigured ?? false

  // OpenAI state
  const isOpenaiActive = isProviderActive === "openai_codex"

  useEffect(() => {
    if (!runtimeSettings) return
    setOpenrouterModelId(runtimeSettings.providerId === "openrouter" ? runtimeSettings.modelId : "")
    setFormError(null)
  }, [runtimeSettings])

  async function handleOpenrouterSave() {
    if (!onUpsertRuntimeSettings) return

    if (!openrouterModelId.trim()) {
      setFormError("Model ID is required.")
      return
    }

    setFormError(null)

    const request: UpsertRuntimeSettingsRequestDto = {
      providerId: "openrouter",
      modelId: openrouterModelId.trim(),
      ...(clearOpenrouterApiKey
        ? { openrouterApiKey: "" }
        : openrouterApiKey.length > 0
          ? { openrouterApiKey }
          : {}),
    }

    try {
      await onUpsertRuntimeSettings(request)
      setConfiguringId(null)
      setClearOpenrouterApiKey(false)
    } catch {
      // Hook state surfaces the typed error while the form preserves the last-known-good snapshot.
    } finally {
      setOpenrouterApiKey("")
    }
  }

  async function handleOpenaiConnect() {
    if (!hasSelectedProject || !onStartLogin) return
    setPending("login")
    try {
      const next = await onStartLogin()
      if (next?.authorizationUrl) {
        try {
          await openUrl(next.authorizationUrl)
        } catch {
          // Browser open failed — the login flow still started in the desktop runtime.
        }
      }
      // Switch to OpenAI as active provider on successful connect
      if (onUpsertRuntimeSettings) {
        await onUpsertRuntimeSettings({ providerId: "openai_codex", modelId: "openai_codex" })
      }
    } catch (error) {
      setFormError(errMsg(error, "Could not start login."))
    } finally {
      setPending(null)
    }
  }

  async function handleOpenaiDisconnect() {
    if (!onLogout) return
    setPending("logout")
    try {
      await onLogout()
    } catch (error) {
      setFormError(errMsg(error, "Could not sign out."))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Providers</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Configure AI model providers for Cadence
        </p>
      </div>

      {runtimeSettingsLoadError && (
        <Alert variant="destructive" className="py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[12px]">
            {errorViewMessage(runtimeSettingsLoadError, "Failed to load provider settings.")}
            <Button
              variant="outline"
              size="sm"
              className="mt-2 h-6 text-[10px]"
              onClick={() => void onRefreshRuntimeSettings?.({ force: true }).catch(() => undefined)}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {runtimeSettingsSaveError && (
        <Alert variant="destructive" className="py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[12px]">
            {errorViewMessage(runtimeSettingsSaveError, "Failed to save provider settings.")}
          </AlertDescription>
        </Alert>
      )}

      {formError && (
        <Alert variant="destructive" className="py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[12px]">{formError}</AlertDescription>
        </Alert>
      )}

      <div className="grid gap-2">
        {PROVIDERS.map(({ id, label, description, Icon }) => {
          const isOpenrouter = id === "openrouter"
          const isOpenai = id === "openai_codex"
          const configOpen = configuringId === id

          // OpenRouter: configured if API key is set
          const isConfigured = isOpenrouter
            ? openrouterConfigured
            : isOpenai
              ? isOpenaiConnected
              : false

          const isActive = isProviderActive === id

          return (
            <div key={id} className="rounded-lg border border-border bg-card px-4 py-3">
              <div className="flex items-center gap-3">
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
                  <Icon className="h-4 w-4 text-foreground/70" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <p className="text-[13px] font-medium text-foreground">{label}</p>
                    {isActive && (
                      <Badge variant="default" className="h-4 px-1 text-[9px]">Active</Badge>
                    )}
                  </div>
                  <p className="text-[11px] text-muted-foreground">{description}</p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {!isConfigured && !configOpen ? (
                    <Badge variant="outline" className="text-[10px]">Not configured</Badge>
                  ) : !configOpen ? (
                    <Badge variant="secondary" className="text-[10px]">Configured</Badge>
                  ) : null}

                  {isOpenrouter ? (
                    !configOpen ? (
                      <Button
                        size="sm"
                        className="h-7 text-[11px]"
                        disabled={isSaving}
                        onClick={() => {
                          setConfiguringId(id)
                          setFormError(null)
                        }}
                      >
                        Configure
                      </Button>
                    ) : null
                  ) : isOpenai ? (
                    !configOpen ? (
                      isOpenaiConnected ? (
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-7 text-[11px]"
                          disabled={pending !== null}
                          onClick={() => void handleOpenaiDisconnect()}
                        >
                          {pending === "logout" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogOut className="h-3 w-3" />}
                          Sign out
                        </Button>
                      ) : isOpenaiInProgress ? (
                        <Badge variant="secondary" className="gap-1 text-[10px]">
                          <LoaderCircle className="h-3 w-3 animate-spin" />
                          Connecting…
                        </Badge>
                      ) : !hasSelectedProject ? (
                        <Badge variant="outline" className="text-[10px]">Select a project</Badge>
                      ) : (
                        <Button
                          size="sm"
                          className="h-7 text-[11px]"
                          disabled={pending !== null || !onStartLogin}
                          onClick={() => void handleOpenaiConnect()}
                        >
                          {pending === "login" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogIn className="h-3 w-3" />}
                          Sign in
                        </Button>
                      )
                    ) : null
                  ) : null}
                </div>
              </div>

              {/* OpenRouter Configuration Form */}
              {isOpenrouter && configOpen && (
                <div className="border-t border-border pt-3 mt-3">
                  <div className="grid gap-3">
                    <div className="space-y-1.5">
                      <Label htmlFor={`or-model-${id}`} className="text-[11px]">Model ID</Label>
                      <Input
                        id={`or-model-${id}`}
                        className="h-8 text-[12px]"
                        disabled={isSaving}
                        onChange={(e) => setOpenrouterModelId(e.target.value)}
                        placeholder="openai/gpt-4.1-mini"
                        value={openrouterModelId}
                      />
                      <p className="text-[10px] text-muted-foreground">
                        Use the exact OpenRouter model slug
                      </p>
                    </div>

                    <div className="space-y-1.5">
                      <div className="flex items-center justify-between gap-3">
                        <Label htmlFor={`or-key-${id}`} className="text-[11px]">API Key</Label>
                        {openrouterConfigured && (
                          <Badge variant="secondary" className="text-[10px]">Key saved</Badge>
                        )}
                      </div>
                      <div className="flex gap-2">
                        <Input
                          id={`or-key-${id}`}
                          type="password"
                          autoComplete="off"
                          spellCheck={false}
                          className="flex-1 h-8 text-[12px]"
                          disabled={isSaving}
                          onChange={(e) => {
                            setOpenrouterApiKey(e.target.value)
                            if (e.target.value.trim().length > 0) setClearOpenrouterApiKey(false)
                          }}
                          placeholder={openrouterConfigured ? "Leave blank to keep current" : "Paste API key"}
                          value={openrouterApiKey}
                        />
                        {openrouterConfigured && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-8 px-2 text-[11px]"
                            disabled={isSaving}
                            onClick={() => {
                              setClearOpenrouterApiKey((v) => !v)
                              setOpenrouterApiKey("")
                            }}
                          >
                            {clearOpenrouterApiKey ? "Keep" : "Clear"}
                          </Button>
                        )}
                      </div>
                      <p className="text-[10px] text-muted-foreground">
                        {clearOpenrouterApiKey
                          ? "Saved key will be removed"
                          : openrouterConfigured
                            ? "Blank keeps current key"
                            : "Required for OpenRouter"}
                      </p>
                    </div>

                    <div className="flex items-center gap-2">
                      <Button
                        size="sm"
                        className="h-7 text-[11px]"
                        disabled={!onUpsertRuntimeSettings || isSaving}
                        onClick={() => void handleOpenrouterSave()}
                      >
                        {isSaving ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Check className="h-3 w-3" />}
                        Save
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 text-[11px]"
                        onClick={() => {
                          setConfiguringId(null)
                          setFormError(null)
                        }}
                      >
                        Cancel
                      </Button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ===========================================================================
// Notifications Section — channel-card layout mirroring providers
// ===========================================================================

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

  const kindOpt = ROUTE_KINDS.find((o) => o.value === form.routeKind) ?? ROUTE_KINDS[0]

  function setField<F extends keyof RouteFormValues>(field: F, value: RouteFormValues[F]) {
    setForm((p) => ({ ...p, [field]: value }))
    setFormErrors((p) => {
      const key = field as string as keyof RouteFormErrors
      if (!p[key] && !p.form) return p
      const n = { ...p }
      delete n[key]
      delete n.form
      return n
    })
  }

  function startNew(kind: NotificationRouteKindDto) {
    setEditingId(null)
    setForm(defaultRouteForm(kind))
    setFormErrors({})
    setFormError(null)
    setFormKind(kind)
  }

  function editRoute(r: AgentPaneView["notificationRoutes"][number]) {
    let target = r.routeTarget
    try {
      target = decomposeNotificationRouteTarget(r.routeKind, r.routeTarget).channelTarget
    } catch {
      // Keep the truthful stored target when decomposition fails.
    }
    setEditingId(r.routeId)
    setForm({ routeId: r.routeId, routeKind: r.routeKind, routeTarget: target, enabled: r.enabled })
    setFormErrors({})
    setFormError(null)
    setFormKind(r.routeKind)
  }

  function cancelForm() {
    setFormKind(null)
    setEditingId(null)
    setFormErrors({})
    setFormError(null)
  }

  async function save() {
    if (!canMutate || !onUpsertNotificationRoute) return
    let req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">
    try {
      req = toRouteRequest(form)
      setFormErrors({})
    } catch (error) {
      setFormErrors(parseFormErrors(error))
      return
    }
    setPending("save")
    setFormError(null)
    try {
      await onUpsertNotificationRoute(req)
      setFormKind(null)
      setEditingId(null)
    } catch (error) {
      setFormError(errMsg(error, "Could not save route."))
    } finally {
      setPending(null)
    }
  }

  async function toggleRoute(r: AgentPaneView["notificationRoutes"][number]) {
    if (!canMutate || !onUpsertNotificationRoute) return
    setPending("toggle")
    try {
      await onUpsertNotificationRoute({
        routeId: r.routeId,
        routeKind: r.routeKind,
        routeTarget: r.routeTarget,
        enabled: !r.enabled,
        metadataJson: r.metadataJson ?? null,
      })
    } catch (error) {
      setFormError(errMsg(error, `Could not update ${r.routeId}.`))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Notifications</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Route operator prompts to Telegram or Discord.
        </p>
      </div>

      <div className="grid gap-2">
        {CHANNELS.map(({ kind, label, description, Icon }) => {
          const channelRoutes = routes.filter((r) => r.routeKind === kind)
          const formOpen = formKind === kind
          const hasRoutes = channelRoutes.length > 0

          return (
            <div key={kind} className="rounded-lg border border-border bg-card px-4 py-3">
              <div className="flex items-center gap-3">
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
                  <Icon className="h-4 w-4 text-foreground/70" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-[13px] font-medium text-foreground">{label}</p>
                  <p className="text-[11px] text-muted-foreground">{description}</p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {!hasRoutes && !formOpen ? (
                    <>
                      <Badge variant="outline" className="text-[10px]">Not configured</Badge>
                      <Button
                        size="sm"
                        className="h-7 text-[11px]"
                        disabled={!canMutate}
                        onClick={() => startNew(kind)}
                      >
                        <Plus className="h-3 w-3" />
                        Add route
                      </Button>
                    </>
                  ) : !formOpen ? (
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-7 text-[11px]"
                      disabled={!canMutate}
                      onClick={() => startNew(kind)}
                    >
                      <Plus className="h-3 w-3" />
                      Add
                    </Button>
                  ) : null}
                </div>
              </div>

              {hasRoutes && (
                <div className="mt-2 grid gap-0.5 border-t border-border pt-2">
                  {channelRoutes.map((r) => {
                    const busy = pendingRouteId === r.routeId && (isMutating || pending === "toggle")
                    const isActiveEdit = editingId === r.routeId && formOpen
                    return (
                      <div
                        key={r.routeId}
                        className={cn(
                          "flex items-center gap-2 rounded-md px-1.5 py-1.5 -mx-1.5",
                          isActiveEdit && "bg-secondary/40",
                        )}
                      >
                        <div className="flex-1 min-w-0">
                          <p className="text-[12px] font-medium text-foreground leading-none">{r.routeId}</p>
                          <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
                            {routeTargetDisplay(r.routeKind, r.routeTarget)}
                          </p>
                        </div>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[11px]"
                          onClick={() => editRoute(r)}
                        >
                          Edit
                        </Button>
                        <div className="flex items-center gap-1.5">
                          <Label htmlFor={`rt-${r.routeId}`} className="text-[10px] text-muted-foreground w-4">
                            {r.enabled ? "On" : "Off"}
                          </Label>
                          <Switch
                            id={`rt-${r.routeId}`}
                            checked={r.enabled}
                            onCheckedChange={() => void toggleRoute(r)}
                            disabled={!canMutate || busy}
                          />
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}

              {formOpen && (
                <div className={cn("border-t border-border pt-3", hasRoutes ? "mt-1" : "mt-3")}>
                  <p className="mb-2.5 text-[12px] font-medium text-foreground">
                    {editingId ? `Edit — ${editingId}` : `New ${label} route`}
                  </p>
                  <div className="grid gap-3">
                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <Label htmlFor={`s-route-id-${kind}`} className="text-[11px]">Route name</Label>
                        <Input
                          id={`s-route-id-${kind}`}
                          className="h-8 text-[12px]"
                          disabled={isMutating || pending === "save"}
                          onChange={(e) => setField("routeId", e.target.value)}
                          placeholder="e.g. ops-alerts"
                          value={form.routeId}
                        />
                        <FieldError msg={formErrors.routeId} />
                      </div>
                      <div className="space-y-1.5">
                        <Label htmlFor={`s-route-target-${kind}`} className="text-[11px]">Target</Label>
                        <Input
                          id={`s-route-target-${kind}`}
                          className="h-8 text-[12px]"
                          disabled={isMutating || pending === "save"}
                          onChange={(e) => setField("routeTarget", e.target.value)}
                          placeholder={kindOpt.placeholder}
                          value={form.routeTarget}
                        />
                        <FieldError msg={formErrors.routeTarget} />
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Switch
                        id={`s-route-enabled-${kind}`}
                        checked={form.enabled}
                        onCheckedChange={(v) => setField("enabled", v)}
                        disabled={isMutating || pending === "save"}
                      />
                      <Label htmlFor={`s-route-enabled-${kind}`} className="text-[11px] text-muted-foreground">
                        Enable immediately
                      </Label>
                    </div>
                    {(formErrors.form || formError) && (
                      <p className="text-[12px] text-destructive">{formErrors.form ?? formError}</p>
                    )}
                    <div className="flex items-center gap-2">
                      <Button
                        size="sm"
                        className="h-7 text-[11px]"
                        disabled={!canMutate || isMutating || pending === "save"}
                        onClick={() => void save()}
                      >
                        {pending === "save" || isMutating
                          ? <LoaderCircle className="h-3 w-3 animate-spin" />
                          : <Check className="h-3 w-3" />}
                        {editingId ? "Save changes" : "Create route"}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 text-[11px]"
                        onClick={cancelForm}
                      >
                        Cancel
                      </Button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ===========================================================================
// Development Section — dev-only, toolbar platform preview
// ===========================================================================

const PLATFORM_OPTIONS: Array<{ value: PlatformVariant | null; label: string; hint: string }> = [
  { value: null, label: "Auto", hint: "Use detected OS" },
  { value: "macos", label: "macOS", hint: "Traffic lights · tabs right" },
  { value: "windows", label: "Windows", hint: "Tabs left · controls right" },
  { value: "linux", label: "Linux", hint: "Same as Windows, rounded" },
]

function DevelopmentSection({
  platformOverride,
  onPlatformOverrideChange,
}: {
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (v: PlatformVariant | null) => void
}) {
  const detected = detectPlatform()
  const current = platformOverride ?? null

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Development</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Developer tooling and preview options. Not visible in production builds.
        </p>
      </div>

      <div className="rounded-lg border border-border bg-card px-4 py-3">
        <p className="text-[13px] font-medium text-foreground">Toolbar platform</p>
        <p className="mt-0.5 text-[11px] text-muted-foreground">
          Override the detected platform to preview different toolbar layouts.{" "}
          Detected: <span className="font-mono text-foreground/70">{detected}</span>
        </p>

        <div className="mt-3 flex gap-1 rounded-lg border border-border bg-secondary/30 p-1">
          {PLATFORM_OPTIONS.map(({ value, label }) => (
            <button
              key={label}
              type="button"
              className={cn(
                "flex-1 rounded-md py-1.5 text-[12px] font-medium transition-colors",
                current === value
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => onPlatformOverrideChange?.(value)}
            >
              {label}
            </button>
          ))}
        </div>

        <p className="mt-2 text-[11px] text-muted-foreground">
          {PLATFORM_OPTIONS.find((o) => o.value === current)?.hint}
        </p>
      </div>
    </div>
  )
}
