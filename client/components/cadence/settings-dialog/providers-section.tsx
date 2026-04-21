import { useEffect, useState } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  AlertCircle,
  Check,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
} from "lucide-react"
import { OpenAIIcon } from "@/components/cadence/brand-icons"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type {
  AgentPaneView,
  OperatorActionErrorView,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  RuntimeSessionView,
  RuntimeSettingsDto,
  UpsertRuntimeSettingsRequestDto,
} from "@/src/lib/cadence-model"

function errMsg(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  if (typeof error === "string" && error.trim().length > 0) return error
  return fallback
}

function errorViewMessage(error: OperatorActionErrorView | null, fallback: string): string {
  if (error?.message?.trim()) return error.message
  return fallback
}

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

export interface ProvidersSectionProps {
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
}

export function ProvidersSection({
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
}: ProvidersSectionProps) {
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
  const isRefreshing = runtimeSettingsLoadStatus === "running"

  const isProviderActive = runtimeSettings?.providerId
  const openrouterConfigured = runtimeSettings?.openrouterApiKeyConfigured ?? false

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
              disabled={isRefreshing}
              onClick={() => void onRefreshRuntimeSettings?.({ force: true }).catch(() => undefined)}
            >
              {isRefreshing ? <LoaderCircle className="h-3 w-3 animate-spin" /> : null}
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
          const isConfigured = isOpenrouter ? openrouterConfigured : isOpenai ? isOpenaiConnected : false
          const isActive = isProviderActive === id

          return (
            <div key={id} className="rounded-lg border border-border bg-card px-4 py-3">
              <div className="flex items-center gap-3">
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
                  <Icon className="h-4 w-4 text-foreground/70" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="text-[13px] font-medium text-foreground">{label}</p>
                    {isActive ? (
                      <Badge variant="default" className="h-4 px-1 text-[9px]">
                        Active
                      </Badge>
                    ) : null}
                  </div>
                  <p className="text-[11px] text-muted-foreground">{description}</p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {!isConfigured && !configOpen ? (
                    <Badge variant="outline" className="text-[10px]">
                      Not configured
                    </Badge>
                  ) : !configOpen ? (
                    <Badge variant="secondary" className="text-[10px]">
                      Configured
                    </Badge>
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
                          {pending === "logout" ? (
                            <LoaderCircle className="h-3 w-3 animate-spin" />
                          ) : (
                            <LogOut className="h-3 w-3" />
                          )}
                          Sign out
                        </Button>
                      ) : isOpenaiInProgress ? (
                        <Badge variant="secondary" className="gap-1 text-[10px]">
                          <LoaderCircle className="h-3 w-3 animate-spin" />
                          Connecting…
                        </Badge>
                      ) : !hasSelectedProject ? (
                        <Badge variant="outline" className="text-[10px]">
                          Select a project
                        </Badge>
                      ) : (
                        <Button
                          size="sm"
                          className="h-7 text-[11px]"
                          disabled={pending !== null || !onStartLogin}
                          onClick={() => void handleOpenaiConnect()}
                        >
                          {pending === "login" ? (
                            <LoaderCircle className="h-3 w-3 animate-spin" />
                          ) : (
                            <LogIn className="h-3 w-3" />
                          )}
                          Sign in
                        </Button>
                      )
                    ) : null
                  ) : null}
                </div>
              </div>

              {isOpenrouter && configOpen ? (
                <div className="mt-3 border-t border-border pt-3">
                  <div className="grid gap-3">
                    <div className="space-y-1.5">
                      <Label htmlFor={`or-model-${id}`} className="text-[11px]">
                        Model ID
                      </Label>
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
                        <Label htmlFor={`or-key-${id}`} className="text-[11px]">
                          API Key
                        </Label>
                        {openrouterConfigured ? (
                          <Badge variant="secondary" className="text-[10px]">
                            Key saved
                          </Badge>
                        ) : null}
                      </div>
                      <div className="flex gap-2">
                        <Input
                          id={`or-key-${id}`}
                          type="password"
                          autoComplete="off"
                          spellCheck={false}
                          className="h-8 flex-1 text-[12px]"
                          disabled={isSaving}
                          onChange={(e) => {
                            setOpenrouterApiKey(e.target.value)
                            if (e.target.value.trim().length > 0) setClearOpenrouterApiKey(false)
                          }}
                          placeholder={openrouterConfigured ? "Leave blank to keep current" : "Paste API key"}
                          value={openrouterApiKey}
                        />
                        {openrouterConfigured ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-8 px-2 text-[11px]"
                            disabled={isSaving}
                            onClick={() => {
                              setClearOpenrouterApiKey((value) => !value)
                              setOpenrouterApiKey("")
                            }}
                          >
                            {clearOpenrouterApiKey ? "Keep" : "Clear"}
                          </Button>
                        ) : null}
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
              ) : null}
            </div>
          )
        })}
      </div>
    </div>
  )
}
