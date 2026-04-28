import { useEffect, useMemo, useState, type ElementType } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  AlertCircle,
  Check,
  ChevronDown,
  Cloud,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
  Server,
} from "lucide-react"
import {
  AnthropicIcon,
  GitHubIcon,
  GoogleIcon,
  OpenAIIcon,
} from "@/components/cadence/brand-icons"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { cn } from "@/lib/utils"
import type {
  OperatorActionErrorView,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type { CloudProviderPreset } from "@/src/lib/cadence-model/provider-presets"
import {
  type ProviderCredentialDto,
  type ProviderCredentialsSnapshotDto,
  type ProviderAuthSessionView,
  type RuntimeProviderIdDto,
  type RuntimeSessionView,
  type UpsertProviderCredentialRequestDto,
} from "@/src/lib/cadence-model"
import { listCloudProviderPresets } from "@/src/lib/cadence-model/provider-presets"

type SupportedProviderId = RuntimeProviderIdDto

type AuthPending = { providerId: SupportedProviderId } | null
type SaveErrorState = { providerId: SupportedProviderId; message: string } | null

interface CredentialDraft {
  apiKey: string
  baseUrl: string
  apiVersion: string
  region: string
  projectId: string
}

const PROVIDER_ICON_BY_ID: Record<SupportedProviderId, ElementType> = {
  openai_codex: OpenAIIcon,
  openrouter: KeyRound,
  anthropic: AnthropicIcon,
  github_models: GitHubIcon,
  openai_api: OpenAIIcon,
  ollama: Server,
  azure_openai: OpenAIIcon,
  gemini_ai_studio: GoogleIcon,
  bedrock: Cloud,
  vertex: GoogleIcon,
}

function errMsg(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  if (typeof error === "string" && error.trim().length > 0) return error
  return fallback
}

function errorViewMessage(error: OperatorActionErrorView | null, fallback: string): string {
  if (error?.message?.trim()) return error.message
  return fallback
}

function isSupportedProviderId(value: string | null | undefined): value is SupportedProviderId {
  return (
    value === "openai_codex" ||
    value === "openrouter" ||
    value === "anthropic" ||
    value === "github_models" ||
    value === "openai_api" ||
    value === "ollama" ||
    value === "azure_openai" ||
    value === "gemini_ai_studio" ||
    value === "bedrock" ||
    value === "vertex"
  )
}

function findCredential(
  snapshot: ProviderCredentialsSnapshotDto | null,
  providerId: SupportedProviderId,
): ProviderCredentialDto | null {
  if (!snapshot) return null
  return snapshot.credentials.find((c) => c.providerId === providerId) ?? null
}

interface StatusInfo {
  label: string
  detail: string | null
}

function getStatus(credential: ProviderCredentialDto): StatusInfo {
  switch (credential.readinessProof) {
    case "oauth_session":
      return {
        label: "Signed in",
        detail: credential.oauthAccountId ? `@${credential.oauthAccountId}` : null,
      }
    case "stored_secret":
      return {
        label: "Ready",
        detail: credential.baseUrl ?? null,
      }
    case "local":
      return {
        label: "Local",
        detail: credential.baseUrl ?? null,
      }
    case "ambient":
      return {
        label: "Ambient",
        detail: ambientDetail(credential),
      }
  }
}

function ambientDetail(credential: ProviderCredentialDto): string | null {
  const parts = [credential.region, credential.projectId].filter(
    (value): value is string => typeof value === "string" && value.length > 0,
  )
  return parts.length > 0 ? parts.join(" · ") : null
}

function createDraft(
  preset: CloudProviderPreset,
  credential: ProviderCredentialDto | null,
): CredentialDraft {
  return {
    apiKey: "",
    baseUrl: credential?.baseUrl ?? "",
    apiVersion: credential?.apiVersion ?? "",
    region: credential?.region ?? "",
    projectId: credential?.projectId ?? "",
  }
}

function buildUpsertRequest(
  preset: CloudProviderPreset,
  draft: CredentialDraft,
): UpsertProviderCredentialRequestDto {
  const trimOrNull = (value: string): string | null => {
    const trimmed = value.trim()
    return trimmed.length > 0 ? trimmed : null
  }

  switch (preset.authMode) {
    case "api_key":
      return {
        providerId: preset.providerId,
        kind: "api_key",
        apiKey: draft.apiKey.trim(),
        baseUrl: trimOrNull(draft.baseUrl),
        apiVersion: trimOrNull(draft.apiVersion),
        region: trimOrNull(draft.region),
        projectId: trimOrNull(draft.projectId),
      }
    case "local":
      return {
        providerId: preset.providerId,
        kind: "local",
        baseUrl: trimOrNull(draft.baseUrl),
      }
    case "ambient":
      return {
        providerId: preset.providerId,
        kind: "ambient",
        region: trimOrNull(draft.region),
        projectId: trimOrNull(draft.projectId),
      }
    case "oauth":
      throw new Error(
        `Cadence persists ${preset.providerId} credentials through OAuth, not the upsert command.`,
      )
  }
}

function validateDraft(
  preset: CloudProviderPreset,
  draft: CredentialDraft,
): string | null {
  if (preset.authMode === "api_key") {
    if (draft.apiKey.trim().length === 0) {
      return "API key is required."
    }
    if (preset.baseUrlMode === "required" && draft.baseUrl.trim().length === 0) {
      return "Base URL is required for this provider."
    }
    if (preset.apiVersionMode === "required" && draft.apiVersion.trim().length === 0) {
      return "API version is required for this provider."
    }
  }
  if (preset.authMode === "local" && draft.baseUrl.trim().length === 0) {
    return "Local endpoint URL is required."
  }
  if (preset.authMode === "ambient") {
    if (preset.regionMode === "required" && draft.region.trim().length === 0) {
      return "Region is required for this provider."
    }
    if (preset.projectIdMode === "required" && draft.projectId.trim().length === 0) {
      return "Project ID is required for this provider."
    }
  }
  return null
}

export interface ProviderCredentialsListProps {
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  providerCredentialsLoadError: OperatorActionErrorView | null
  providerCredentialsSaveStatus: ProviderCredentialsSaveStatus
  providerCredentialsSaveError: OperatorActionErrorView | null
  runtimeSession?: RuntimeSessionView | null
  onRefreshProviderCredentials?: (options?: { force?: boolean }) => Promise<ProviderCredentialsSnapshotDto>
  onUpsertProviderCredential?: (
    request: UpsertProviderCredentialRequestDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onDeleteProviderCredential?: (
    providerId: SupportedProviderId,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onStartOAuthLogin?: (request: {
    providerId: SupportedProviderId
    originator?: string | null
  }) => Promise<ProviderAuthSessionView | null>
}

export function ProviderCredentialsList({
  providerCredentials,
  providerCredentialsLoadStatus,
  providerCredentialsLoadError,
  providerCredentialsSaveStatus,
  providerCredentialsSaveError,
  runtimeSession = null,
  onRefreshProviderCredentials,
  onUpsertProviderCredential,
  onDeleteProviderCredential,
  onStartOAuthLogin,
}: ProviderCredentialsListProps) {
  const presets = useMemo(() => listCloudProviderPresets(), [])
  const [openProviderId, setOpenProviderId] = useState<SupportedProviderId | null>(null)
  const [drafts, setDrafts] = useState<Record<SupportedProviderId, CredentialDraft>>(
    () => ({}) as Record<SupportedProviderId, CredentialDraft>,
  )
  const [authPending, setAuthPending] = useState<AuthPending>(null)
  const [saveError, setSaveError] = useState<SaveErrorState>(null)
  const [openAuthError, setOpenAuthError] = useState<SaveErrorState>(null)

  useEffect(() => {
    if (providerCredentialsLoadStatus === "idle" && onRefreshProviderCredentials) {
      onRefreshProviderCredentials().catch(() => {
        // Surface error through providerCredentialsLoadError
      })
    }
  }, [providerCredentialsLoadStatus, onRefreshProviderCredentials])

  const updateDraft = (providerId: SupportedProviderId, patch: Partial<CredentialDraft>) => {
    setDrafts((prev) => ({
      ...prev,
      [providerId]: { ...prev[providerId], ...patch },
    }))
  }

  const ensureDraft = (
    providerId: SupportedProviderId,
    preset: CloudProviderPreset,
    credential: ProviderCredentialDto | null,
  ) => {
    setDrafts((prev) =>
      prev[providerId] !== undefined
        ? prev
        : { ...prev, [providerId]: createDraft(preset, credential) },
    )
  }

  const handleToggle = (
    providerId: SupportedProviderId,
    preset: CloudProviderPreset,
    credential: ProviderCredentialDto | null,
  ) => {
    if (openProviderId === providerId) {
      setOpenProviderId(null)
      return
    }
    ensureDraft(providerId, preset, credential)
    setOpenProviderId(providerId)
    setSaveError(null)
  }

  const handleSave = async (preset: CloudProviderPreset) => {
    const providerId = preset.providerId
    if (!isSupportedProviderId(providerId)) return
    if (!onUpsertProviderCredential) return
    const draft = drafts[providerId] ?? createDraft(preset, findCredential(providerCredentials, providerId))

    const validation = validateDraft(preset, draft)
    if (validation) {
      setSaveError({ providerId, message: validation })
      return
    }

    setSaveError(null)
    try {
      await onUpsertProviderCredential(buildUpsertRequest(preset, draft))
      updateDraft(providerId, { apiKey: "" })
      setOpenProviderId(null)
    } catch (error) {
      setSaveError({
        providerId,
        message: errMsg(error, "Cadence could not save the provider credential."),
      })
    }
  }

  const handleDelete = async (providerId: SupportedProviderId) => {
    if (!onDeleteProviderCredential) return
    setSaveError(null)
    try {
      await onDeleteProviderCredential(providerId)
      setOpenProviderId(null)
    } catch (error) {
      setSaveError({
        providerId,
        message: errMsg(error, "Cadence could not remove the provider credential."),
      })
    }
  }

  const handleOAuthLogin = async (providerId: SupportedProviderId) => {
    if (!onStartOAuthLogin) return
    setAuthPending({ providerId })
    setOpenAuthError(null)
    try {
      const session = await onStartOAuthLogin({ providerId })
      if (session?.authorizationUrl) {
        try {
          await openUrl(session.authorizationUrl)
        } catch (urlError) {
          setOpenAuthError({
            providerId,
            message: errMsg(urlError, "Cadence could not open the browser for sign-in."),
          })
        }
      }
    } catch (error) {
      setOpenAuthError({
        providerId,
        message: errMsg(error, "Cadence could not start the sign-in flow."),
      })
    } finally {
      setAuthPending(null)
    }
  }

  const showLoadingState =
    providerCredentialsLoadStatus === "loading" && !providerCredentials
  const showLoadError = providerCredentialsLoadStatus === "error"

  const supportedPresets = useMemo(
    () => presets.filter((preset) => isSupportedProviderId(preset.providerId)),
    [presets],
  )

  const { connected, available } = useMemo(() => {
    const connectedRows: CloudProviderPreset[] = []
    const availableRows: CloudProviderPreset[] = []
    for (const preset of supportedPresets) {
      const credential = findCredential(providerCredentials, preset.providerId)
      if (credential) {
        connectedRows.push(preset)
      } else {
        availableRows.push(preset)
      }
    }
    return { connected: connectedRows, available: availableRows }
  }, [providerCredentials, supportedPresets])

  const renderRow = (preset: CloudProviderPreset) => {
    const providerId = preset.providerId
    if (!isSupportedProviderId(providerId)) return null
    const credential = findCredential(providerCredentials, providerId)
    const Icon = PROVIDER_ICON_BY_ID[providerId]
    const isOpen = openProviderId === providerId
    const draft = drafts[providerId] ?? createDraft(preset, credential)
    const isSaving =
      providerCredentialsSaveStatus === "running" && openProviderId === providerId
    const localSaveError = saveError?.providerId === providerId ? saveError.message : null
    const localSaveErrorFromAdapter =
      providerCredentialsSaveError && openProviderId === providerId
        ? providerCredentialsSaveError.message
        : null
    const localOpenAuthError =
      openAuthError?.providerId === providerId ? openAuthError.message : null
    const isOAuth = preset.authMode === "oauth"
    const isAuthenticated =
      credential?.kind === "oauth_session" && credential?.hasOauthAccessToken
    const showAuthInProgress =
      isOAuth &&
      !!runtimeSession?.isLoginInProgress &&
      runtimeSession.providerId === providerId
    const status = credential ? getStatus(credential) : null

    return (
      <div
        key={providerId}
        className={cn(
          "rounded-lg border bg-card/40 transition-colors",
          isOpen ? "border-border" : "border-border/60 hover:border-border",
        )}
      >
        <div className="flex items-center gap-3 px-3.5 py-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60">
            <Icon className="h-4 w-4" />
          </div>

          <div className="flex min-w-0 flex-1 items-center gap-3">
            <span className="truncate text-[13px] font-medium text-foreground">{preset.label}</span>
            {status ? (
              <span className="hidden items-center gap-1.5 truncate text-[12px] text-muted-foreground sm:flex">
                <span
                  className="size-1.5 shrink-0 rounded-full bg-emerald-500 dark:bg-emerald-400"
                  aria-hidden
                />
                <span className="text-foreground/80">{status.label}</span>
                {status.detail ? (
                  <span className="truncate text-muted-foreground/70">· {status.detail}</span>
                ) : null}
              </span>
            ) : null}
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            {isOAuth ? (
              isAuthenticated ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
                  onClick={() => handleDelete(providerId)}
                  disabled={!onDeleteProviderCredential}
                >
                  <LogOut className="h-3.5 w-3.5" />
                  Sign out
                </Button>
              ) : (
                <Button
                  variant="outline"
                  size="sm"
                  className="h-8 gap-1.5 text-[12px]"
                  onClick={() => handleOAuthLogin(providerId)}
                  disabled={
                    !onStartOAuthLogin ||
                    authPending?.providerId === providerId ||
                    showAuthInProgress
                  }
                >
                  {authPending?.providerId === providerId || showAuthInProgress ? (
                    <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <LogIn className="h-3.5 w-3.5" />
                  )}
                  Sign in
                </Button>
              )
            ) : (
              <Button
                variant={credential ? "ghost" : "outline"}
                size="sm"
                className={cn(
                  "h-8 gap-1.5 text-[12px]",
                  credential && "text-muted-foreground hover:text-foreground",
                )}
                onClick={() => handleToggle(providerId, preset, credential)}
                aria-expanded={isOpen}
              >
                {credential ? "Edit" : "Configure"}
                <ChevronDown
                  className={cn(
                    "h-3.5 w-3.5 transition-transform",
                    isOpen && "rotate-180",
                  )}
                />
              </Button>
            )}
          </div>
        </div>

        {localOpenAuthError ? (
          <div className="border-t border-border/60 px-3.5 py-2.5">
            <Alert variant="destructive" className="border-destructive/40">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>{localOpenAuthError}</AlertDescription>
            </Alert>
          </div>
        ) : null}

        {!isOAuth && isOpen ? (
          <div className="space-y-3 border-t border-border/60 px-3.5 py-3.5">
            {preset.authMode === "api_key" ? (
              <FieldRow>
                <Label htmlFor={`${providerId}-api-key`} className="text-[11.5px] text-muted-foreground">
                  API key
                  {credential?.hasApiKey ? (
                    <span className="ml-2 text-[11px] text-muted-foreground/70">
                      (saved — leave empty to keep current)
                    </span>
                  ) : null}
                </Label>
                <Input
                  id={`${providerId}-api-key`}
                  type="password"
                  autoComplete="off"
                  value={draft.apiKey}
                  onChange={(e) => updateDraft(providerId, { apiKey: e.target.value })}
                  placeholder={credential?.hasApiKey ? "••••••••" : "Paste your API key"}
                  className="h-9"
                />
              </FieldRow>
            ) : null}

            {preset.baseUrlMode !== "none" || preset.authMode === "local" ? (
              <FieldRow>
                <Label htmlFor={`${providerId}-base-url`} className="text-[11.5px] text-muted-foreground">
                  Base URL
                  {preset.baseUrlMode === "required" || preset.authMode === "local" ? (
                    <span className="ml-1 text-destructive">*</span>
                  ) : null}
                </Label>
                <Input
                  id={`${providerId}-base-url`}
                  value={draft.baseUrl}
                  onChange={(e) => updateDraft(providerId, { baseUrl: e.target.value })}
                  placeholder={preset.connectionHint}
                  className="h-9"
                />
              </FieldRow>
            ) : null}

            {preset.apiVersionMode !== "none" ? (
              <FieldRow>
                <Label htmlFor={`${providerId}-api-version`} className="text-[11.5px] text-muted-foreground">
                  API version
                  {preset.apiVersionMode === "required" ? (
                    <span className="ml-1 text-destructive">*</span>
                  ) : null}
                </Label>
                <Input
                  id={`${providerId}-api-version`}
                  value={draft.apiVersion}
                  onChange={(e) => updateDraft(providerId, { apiVersion: e.target.value })}
                  className="h-9"
                />
              </FieldRow>
            ) : null}

            {preset.regionMode === "required" ? (
              <FieldRow>
                <Label htmlFor={`${providerId}-region`} className="text-[11.5px] text-muted-foreground">
                  Region <span className="text-destructive">*</span>
                </Label>
                <Input
                  id={`${providerId}-region`}
                  value={draft.region}
                  onChange={(e) => updateDraft(providerId, { region: e.target.value })}
                  className="h-9"
                />
              </FieldRow>
            ) : null}

            {preset.projectIdMode === "required" ? (
              <FieldRow>
                <Label htmlFor={`${providerId}-project-id`} className="text-[11.5px] text-muted-foreground">
                  Project ID <span className="text-destructive">*</span>
                </Label>
                <Input
                  id={`${providerId}-project-id`}
                  value={draft.projectId}
                  onChange={(e) => updateDraft(providerId, { projectId: e.target.value })}
                  className="h-9"
                />
              </FieldRow>
            ) : null}

            {localSaveError || localSaveErrorFromAdapter ? (
              <Alert variant="destructive" className="border-destructive/40">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  {localSaveError ?? localSaveErrorFromAdapter}
                </AlertDescription>
              </Alert>
            ) : null}

            <div className="flex items-center justify-between gap-2 pt-1">
              {credential ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => handleDelete(providerId)}
                  disabled={isSaving || !onDeleteProviderCredential}
                  className="h-8 text-[12px] text-destructive hover:bg-destructive/10 hover:text-destructive"
                >
                  Remove
                </Button>
              ) : (
                <span />
              )}
              <Button
                size="sm"
                className="h-8 gap-1.5 text-[12px]"
                onClick={() => handleSave(preset)}
                disabled={isSaving || !onUpsertProviderCredential}
              >
                {isSaving ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Check className="h-3.5 w-3.5" />
                )}
                Save
              </Button>
            </div>
          </div>
        ) : null}
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      {showLoadError ? (
        <Alert variant="destructive" className="border-destructive/40">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {errorViewMessage(
              providerCredentialsLoadError,
              "Cadence could not load provider credentials.",
            )}
          </AlertDescription>
        </Alert>
      ) : null}

      {showLoadingState ? (
        <div className="flex items-center gap-2 rounded-md border border-border/60 bg-card/40 px-3 py-2.5 text-[12.5px] text-muted-foreground">
          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          Loading provider credentials…
        </div>
      ) : null}

      {connected.length > 0 ? (
        <Group title="Connected" count={connected.length}>
          {connected.map(renderRow)}
        </Group>
      ) : null}

      <Group
        title={connected.length > 0 ? "Available" : "All providers"}
        count={available.length}
      >
        {available.map(renderRow)}
      </Group>
    </div>
  )
}

function Group({
  title,
  count,
  children,
}: {
  title: string
  count: number
  children: React.ReactNode
}) {
  return (
    <section className="flex flex-col gap-2">
      <header className="flex items-baseline gap-2 px-1">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          {title}
        </h4>
        <span className="text-[11px] tabular-nums text-muted-foreground/60">{count}</span>
      </header>
      <div className="flex flex-col gap-1.5">{children}</div>
    </section>
  )
}

function FieldRow({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-col gap-1.5">{children}</div>
}
