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
// Providers Section — app-global provider settings plus selected-project auth
// ===========================================================================

type AuthPending = "login" | "logout" | null
type RuntimeProviderId = RuntimeSettingsDto["providerId"]

const PROVIDER_OPTIONS: Array<{
  value: RuntimeProviderId
  label: string
  description: string
  Icon: React.ElementType
  fixedModelId?: string
}> = [
  {
    value: "openrouter",
    label: "OpenRouter",
    description: "App-global API key with configurable model routing.",
    Icon: KeyRound,
  },
  {
    value: "openai_codex",
    label: "OpenAI Codex",
    description: "Project-bound browser login for the desktop runtime.",
    Icon: OpenAIIcon,
    fixedModelId: "openai_codex",
  },
]

const COMING_SOON_PROVIDERS: Array<{
  label: string
  description: string
  Icon: React.ElementType
}> = [
  { label: "Anthropic", description: "Claude agent runtime", Icon: AnthropicIcon },
  { label: "Google", description: "Gemini agent runtime", Icon: GoogleIcon },
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
  const [providerId, setProviderId] = useState<RuntimeProviderId>("openrouter")
  const [openrouterModelId, setOpenrouterModelId] = useState("")
  const [openrouterApiKey, setOpenrouterApiKey] = useState("")
  const [clearOpenrouterApiKey, setClearOpenrouterApiKey] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const [pending, setPending] = useState<AuthPending>(null)
  const [authError, setAuthError] = useState<string | null>(null)

  const providerOption = PROVIDER_OPTIONS.find((option) => option.value === providerId) ?? PROVIDER_OPTIONS[0]
  const hasSelectedProject = Boolean(agent?.repositoryPath?.trim())
  const runtimeSession = agent?.runtimeSession ?? null
  const isConnected = Boolean(runtimeSession?.isAuthenticated)
  const isInProgress = Boolean(runtimeSession?.isLoginInProgress)
  const isSaving = runtimeSettingsSaveStatus === "running"
  const openrouterConfigured = runtimeSettings?.openrouterApiKeyConfigured ?? false
  const needsOpenrouterKey =
    providerId === "openrouter" && !openrouterConfigured && openrouterApiKey.trim().length === 0 && !clearOpenrouterApiKey

  useEffect(() => {
    if (!runtimeSettings) {
      return
    }

    setProviderId(runtimeSettings.providerId)
    setOpenrouterModelId(runtimeSettings.providerId === "openrouter" ? runtimeSettings.modelId : "")
    setOpenrouterApiKey("")
    setClearOpenrouterApiKey(false)
    setFormError(null)
  }, [runtimeSettings])

  useEffect(() => {
    setAuthError(null)
  }, [runtimeSession?.isAuthenticated, runtimeSession?.updatedAt])

  function selectProvider(nextProviderId: RuntimeProviderId) {
    setProviderId(nextProviderId)
    setFormError(null)
  }

  async function handleSave() {
    if (!onUpsertRuntimeSettings) return

    const normalizedModelId = providerOption.fixedModelId ?? openrouterModelId.trim()
    if (!normalizedModelId) {
      setFormError("Model ID is required.")
      return
    }

    setFormError(null)

    const request: UpsertRuntimeSettingsRequestDto = {
      providerId,
      modelId: normalizedModelId,
      ...(clearOpenrouterApiKey
        ? { openrouterApiKey: "" }
        : openrouterApiKey.length > 0
          ? { openrouterApiKey }
          : {}),
    }

    try {
      await onUpsertRuntimeSettings(request)
      setClearOpenrouterApiKey(false)
    } catch {
      // Hook state surfaces the typed error while the form preserves the last-known-good snapshot.
    } finally {
      setOpenrouterApiKey("")
    }
  }

  async function handleConnect() {
    if (!hasSelectedProject || !onStartLogin) return
    setPending("login")
    setAuthError(null)
    try {
      const next = await onStartLogin()
      if (next?.authorizationUrl) {
        try {
          await openUrl(next.authorizationUrl)
        } catch {
          // Browser open failed — the login flow still started in the desktop runtime.
        }
      }
    } catch (error) {
      setAuthError(errMsg(error, "Could not start login."))
    } finally {
      setPending(null)
    }
  }

  async function handleDisconnect() {
    if (!onLogout) return
    setPending("logout")
    setAuthError(null)
    try {
      await onLogout()
    } catch (error) {
      setAuthError(errMsg(error, "Could not sign out."))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Providers</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Configure the app-global runtime provider, model, and OpenRouter key without requiring a selected project.
        </p>
      </div>

      {runtimeSettingsLoadError && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Settings load failed</AlertTitle>
          <AlertDescription className="space-y-3">
            <p>{errorViewMessage(runtimeSettingsLoadError, "Cadence could not load app-global runtime settings.")}</p>
            <div>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[11px]"
                onClick={() => void onRefreshRuntimeSettings?.({ force: true }).catch(() => undefined)}
              >
                Retry
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {runtimeSettingsSaveError && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Settings save failed</AlertTitle>
          <AlertDescription>{errorViewMessage(runtimeSettingsSaveError, "Cadence could not save app-global runtime settings.")}</AlertDescription>
        </Alert>
      )}

      {authError && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Connection error</AlertTitle>
          <AlertDescription>{authError}</AlertDescription>
        </Alert>
      )}

      {runtimeSettingsLoadStatus === "loading" && !runtimeSettings ? (
        <div className="flex items-center gap-2 rounded-lg border border-border bg-card px-4 py-3 text-[12px] text-muted-foreground">
          <LoaderCircle className="h-4 w-4 animate-spin" />
          Loading app-global provider settings…
        </div>
      ) : null}

      {!runtimeSettings && runtimeSettingsLoadStatus === "error" ? null : !runtimeSettings ? (
        <div className="rounded-lg border border-border bg-card px-4 py-3 text-[12px] text-muted-foreground">
          Open the dialog to load the app-global provider settings.
        </div>
      ) : (
        <>
          <div className="grid gap-2">
            {PROVIDER_OPTIONS.map(({ value, label, description, Icon }) => {
              const selected = providerId === value
              const openrouterCard = value === "openrouter"

              return (
                <div key={value} className="rounded-lg border border-border bg-card px-4 py-3">
                  <div className="flex items-center gap-3">
                    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
                      <Icon className="h-4 w-4 text-foreground/70" />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <p className="text-[13px] font-medium text-foreground">{label}</p>
                        {selected ? <Badge variant="default" className="text-[10px]">Selected</Badge> : null}
                      </div>
                      <p className="mt-0.5 text-[11px] text-muted-foreground">{description}</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      {openrouterCard ? (
                        <Badge variant={openrouterConfigured ? "default" : "outline"} className="text-[10px]">
                          {openrouterConfigured ? "Key configured" : "Key missing"}
                        </Badge>
                      ) : hasSelectedProject ? (
                        isConnected ? (
                          <Badge variant="default" className="gap-1 text-[10px]">
                            <Check className="h-3 w-3" />
                            Connected
                          </Badge>
                        ) : isInProgress ? (
                          <Badge variant="secondary" className="gap-1 text-[10px]">
                            <LoaderCircle className="h-3 w-3 animate-spin" />
                            Connecting…
                          </Badge>
                        ) : (
                          <Badge variant="outline" className="text-[10px]">Not connected</Badge>
                        )
                      ) : (
                        <Badge variant="outline" className="text-[10px]">Select project</Badge>
                      )}
                      <Button
                        type="button"
                        size="sm"
                        variant={selected ? "secondary" : "outline"}
                        className="h-7 text-[11px]"
                        onClick={() => selectProvider(value)}
                      >
                        {selected ? "Selected" : "Use provider"}
                      </Button>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>

          <div className="rounded-lg border border-border bg-card px-4 py-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-[13px] font-medium text-foreground">Selected provider settings</p>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {providerId === "openrouter"
                    ? "Choose the OpenRouter model Cadence should bind against and manage the saved app-global API key state."
                    : "OpenAI Codex uses the fixed modelId `openai_codex`; browser login remains project-bound."}
                </p>
              </div>
              <Badge variant="outline" className="text-[10px]">App-global</Badge>
            </div>

            <div className="mt-4 grid gap-4">
              <div className="space-y-1.5">
                <Label htmlFor="runtime-model-id" className="text-[11px]">Model ID</Label>
                <Input
                  id="runtime-model-id"
                  className="h-8 text-[12px]"
                  disabled={Boolean(providerOption.fixedModelId) || isSaving}
                  onChange={(event) => setOpenrouterModelId(event.target.value)}
                  placeholder={providerOption.fixedModelId ?? "openai/gpt-4.1-mini"}
                  value={providerOption.fixedModelId ?? openrouterModelId}
                />
                <p className="text-[11px] text-muted-foreground">
                  {providerOption.fixedModelId
                    ? "OpenAI Codex keeps a fixed model identifier so the desktop adapter never drifts from the closed provider catalog."
                    : "Use the exact OpenRouter model slug that Cadence should validate during runtime bind/reconcile."}
                </p>
              </div>

              <div className="space-y-2 rounded-lg border border-border/70 bg-secondary/20 px-3 py-3">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <Label htmlFor="openrouter-api-key" className="text-[11px]">OpenRouter API key</Label>
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      Cadence only stores the app-global key in app-local Tauri storage and only projects configured-state back into the UI.
                    </p>
                  </div>
                  <Badge variant={openrouterConfigured ? "default" : "outline"} className="text-[10px]">
                    {openrouterConfigured ? "Configured" : "Not configured"}
                  </Badge>
                </div>
                <Input
                  id="openrouter-api-key"
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  className="h-8 text-[12px]"
                  disabled={isSaving}
                  onChange={(event) => {
                    setOpenrouterApiKey(event.target.value)
                    if (event.target.value.trim().length > 0) {
                      setClearOpenrouterApiKey(false)
                    }
                  }}
                  placeholder={openrouterConfigured ? "Leave blank to keep the saved key" : "Paste a new OpenRouter API key"}
                  value={openrouterApiKey}
                />
                <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                  <span>
                    {clearOpenrouterApiKey
                      ? "The saved OpenRouter key will be cleared on the next save."
                      : openrouterConfigured
                        ? "Leaving this blank preserves the saved key."
                        : "Saving without a key leaves OpenRouter unconfigured until a key is added."}
                  </span>
                  {openrouterConfigured ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-[11px]"
                      disabled={isSaving}
                      onClick={() => {
                        setClearOpenrouterApiKey((current) => !current)
                        setOpenrouterApiKey("")
                      }}
                    >
                      {clearOpenrouterApiKey ? "Keep saved key" : "Clear saved key"}
                    </Button>
                  ) : null}
                </div>
              </div>

              {needsOpenrouterKey ? (
                <Alert>
                  <AlertCircle className="h-4 w-4" />
                  <AlertTitle>OpenRouter needs a saved key</AlertTitle>
                  <AlertDescription>
                    Add an OpenRouter API key before expecting Cadence to start or reconcile an OpenRouter runtime session.
                  </AlertDescription>
                </Alert>
              ) : null}

              {providerId === "openrouter" ? (
                <Alert>
                  <KeyRound className="h-4 w-4" />
                  <AlertTitle>OpenRouter uses saved app-global credentials</AlertTitle>
                  <AlertDescription>
                    Selecting OpenRouter changes the provider/model truth immediately after save, but it does not rewrite the selected project runtime session optimistically.
                  </AlertDescription>
                </Alert>
              ) : null}

              {(formError || runtimeSettingsSaveError) ? (
                <p className="text-[12px] text-destructive">{formError ?? runtimeSettingsSaveError?.message}</p>
              ) : null}

              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  className="h-7 text-[11px]"
                  disabled={!onUpsertRuntimeSettings || isSaving}
                  onClick={() => void handleSave()}
                >
                  {isSaving ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Check className="h-3 w-3" />}
                  Save provider settings
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 text-[11px]"
                  disabled={!onRefreshRuntimeSettings || runtimeSettingsLoadStatus === "loading"}
                  onClick={() => void onRefreshRuntimeSettings?.({ force: true }).catch(() => undefined)}
                >
                  Refresh
                </Button>
              </div>
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card px-4 py-4">
            <div>
              <p className="text-[13px] font-medium text-foreground">Selected project runtime</p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                Provider settings are global, but runtime auth and notification routes stay grounded in the selected project.
              </p>
            </div>

            {providerId === "openai_codex" ? (
              <div className="mt-4 flex items-center justify-between gap-3 rounded-lg border border-border/70 bg-secondary/20 px-3 py-3">
                <div>
                  <p className="text-[12px] font-medium text-foreground">OpenAI login</p>
                  <p className="mt-0.5 text-[11px] text-muted-foreground">
                    {hasSelectedProject
                      ? "Use browser-based OpenAI auth for the currently selected project runtime session."
                      : "Select a project before starting or signing out of an OpenAI runtime session."}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  {isConnected ? (
                    <>
                      <Badge variant="default" className="gap-1 text-[10px]">
                        <Check className="h-3 w-3" />
                        Connected
                      </Badge>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-7 text-[11px]"
                        disabled={pending !== null}
                        onClick={() => void handleDisconnect()}
                      >
                        {pending === "logout" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogOut className="h-3 w-3" />}
                        Disconnect
                      </Button>
                    </>
                  ) : isInProgress ? (
                    <Badge variant="secondary" className="gap-1 text-[10px]">
                      <LoaderCircle className="h-3 w-3 animate-spin" />
                      Connecting…
                    </Badge>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      className="h-7 text-[11px]"
                      disabled={!hasSelectedProject || pending !== null || !onStartLogin}
                      onClick={() => void handleConnect()}
                    >
                      {pending === "login" ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <LogIn className="h-3 w-3" />}
                      Connect
                    </Button>
                  )}
                </div>
              </div>
            ) : (
              <div className="mt-4 rounded-lg border border-border/70 bg-secondary/20 px-3 py-3 text-[11px] text-muted-foreground">
                OpenRouter runtime sessions bind from the saved app-global provider settings, so there is no project-specific browser login action here.
              </div>
            )}
          </div>

          <div className="grid gap-2">
            {COMING_SOON_PROVIDERS.map(({ label, description, Icon }) => (
              <div key={label} className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 opacity-45">
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
                  <Icon className="h-4 w-4 text-foreground/70" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-[13px] font-medium text-foreground">{label}</p>
                  <p className="text-[11px] text-muted-foreground">{description}</p>
                </div>
                <Badge variant="outline" className="text-[10px]">Coming soon</Badge>
              </div>
            ))}
          </div>
        </>
      )}
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
