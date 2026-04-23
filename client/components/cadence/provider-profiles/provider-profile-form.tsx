import { useEffect, useState, type ElementType } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  AlertCircle,
  Check,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
  Lock,
} from "lucide-react"
import {
  AnthropicIcon,
  GoogleIcon,
  OpenAIIcon,
} from "@/components/cadence/brand-icons"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import type {
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import {
  getProviderMismatchCopy,
  resolveSelectedRuntimeProvider,
} from "@/src/features/cadence/use-cadence-desktop-state/runtime-provider"
import {
  getActiveProviderProfile,
  getProviderModelCatalogFetchedAt,
  type ProviderModelCatalogDto,
  type ProviderModelDto,
  type ProviderProfilesDto,
  type ProviderProfileDto,
  type RuntimeSessionView,
  type UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"

type SupportedProviderId = ProviderProfileDto["providerId"]
type ProviderCatalogId = SupportedProviderId | "anthropic" | "google"
type AuthPending = "login" | "logout" | null

type ProviderDraft = {
  label: string
  modelId: string
  openrouterApiKey: string
  clearOpenrouterApiKey: boolean
  anthropicApiKey: string
  clearAnthropicApiKey: boolean
}

interface ProviderCatalogEntry {
  id: ProviderCatalogId
  label: string
  description: string
  Icon: ElementType
  supported: boolean
  defaultProfileId?: string
  defaultModelId?: string
}

interface ProviderProfileCard {
  key: string
  catalog: ProviderCatalogEntry
  profile: ProviderProfileDto | null
}

interface ProviderModelChoice {
  modelId: string
  label: string
  groupId: string
  groupLabel: string
  availability: "available" | "orphaned"
  availabilityLabel: string
}

interface ProviderModelChoiceGroup {
  id: string
  label: string
  items: ProviderModelChoice[]
}

interface ProviderModelCatalogState {
  profileId: string | null
  catalog: ProviderModelCatalogDto | null
  loadStatus: ProviderModelCatalogLoadStatus
  refreshError: OperatorActionErrorView | null
  stateLabel: string
  detail: string
  tone: "default" | "warning"
  fetchedAt: string | null
  lastSuccessAt: string | null
  choices: ProviderModelChoice[]
  selectedChoice: ProviderModelChoice | null
}

const PROVIDER_CATALOG: ProviderCatalogEntry[] = [
  {
    id: "openai_codex",
    label: "OpenAI Codex",
    description: "App-global provider profile. Browser sign-in happens when you bind a runtime session.",
    Icon: OpenAIIcon,
    supported: true,
    defaultProfileId: "openai_codex-default",
    defaultModelId: "openai_codex",
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    description: "App-global provider profile backed by a saved API key.",
    Icon: KeyRound,
    supported: true,
    defaultProfileId: "openrouter-default",
    defaultModelId: "openai/gpt-4.1-mini",
  },
  {
    id: "anthropic",
    label: "Anthropic",
    description: "App-global provider profile backed by a saved API key and live Claude model discovery.",
    Icon: AnthropicIcon,
    supported: true,
    defaultProfileId: "anthropic-default",
    defaultModelId: "claude-3-7-sonnet-latest",
  },
  {
    id: "google",
    label: "Google",
    description: "Coming soon.",
    Icon: GoogleIcon,
    supported: false,
  },
]

const MODEL_GROUP_LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  deepseek: "DeepSeek",
  google: "Google",
  meta: "Meta",
  "meta-llama": "Meta Llama",
  mistral: "Mistral",
  moonshot: "Moonshot",
  moonshotai: "Moonshot",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  "x-ai": "xAI",
  xai: "xAI",
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

function createDraft(card: ProviderProfileCard): ProviderDraft {
  return {
    label: card.profile?.label ?? card.catalog.label,
    modelId: card.profile?.modelId ?? card.catalog.defaultModelId ?? "",
    openrouterApiKey: "",
    clearOpenrouterApiKey: false,
    anthropicApiKey: "",
    clearAnthropicApiKey: false,
  }
}

function getProfileCards(providerProfiles: ProviderProfilesDto | null): ProviderProfileCard[] {
  const cards: ProviderProfileCard[] = []
  const activeProfileId = providerProfiles?.activeProfileId ?? null

  for (const catalogEntry of PROVIDER_CATALOG) {
    if (!catalogEntry.supported) continue

    const matches = (providerProfiles?.profiles ?? [])
      .filter((profile) => profile.providerId === catalogEntry.id)
      .sort((left, right) => {
        const leftActive = left.profileId === activeProfileId
        const rightActive = right.profileId === activeProfileId

        if (leftActive !== rightActive) return leftActive ? -1 : 1
        return left.label.localeCompare(right.label)
      })

    if (matches.length === 0) {
      cards.push({
        key: `${catalogEntry.id}-placeholder`,
        catalog: catalogEntry,
        profile: null,
      })
      continue
    }

    cards.push(
      ...matches.map((profile) => ({
        key: profile.profileId,
        catalog: catalogEntry,
        profile,
      })),
    )
  }

  return cards
}

function getApiKeyProviderReadinessBadge(profile: ProviderProfileDto | null) {
  if (!profile || (profile.providerId !== "openrouter" && profile.providerId !== "anthropic")) return null

  if (profile.readiness.status === "ready") {
    return {
      label: "Ready",
      className: "border border-emerald-500/30 bg-emerald-500/10 text-emerald-500 dark:text-emerald-400",
    }
  }

  if (profile.readiness.status === "malformed") {
    return {
      label: "Needs repair",
      className: "border border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-300",
    }
  }

  return {
    label: "Needs key",
    className: "border border-border bg-secondary text-muted-foreground",
  }
}

function getProfileId(card: ProviderProfileCard): string {
  return card.profile?.profileId ?? card.catalog.defaultProfileId ?? `${card.catalog.id}-default`
}

function getProfileReference(profile: Pick<ProviderProfileDto, "profileId" | "label"> | null): string {
  if (!profile) return "the selected profile"

  const profileId = profile.profileId.trim()
  const label = profile.label.trim()

  if (label.length === 0) return profileId || "the selected profile"
  if (profileId.length === 0 || profileId === label) return label
  return `${label} (${profileId})`
}

function isCardSelected(providerProfiles: ProviderProfilesDto | null, card: ProviderProfileCard): boolean {
  const activeProfileId = providerProfiles?.activeProfileId?.trim() ?? ""
  if (activeProfileId.length === 0) return false
  return activeProfileId === getProfileId(card)
}

function buildUpsertRequest(
  card: ProviderProfileCard,
  draft: ProviderDraft,
  activate: boolean,
): UpsertProviderProfileRequestDto {
  return {
    profileId: getProfileId(card),
    providerId: card.catalog.id as SupportedProviderId,
    label: draft.label.trim(),
    modelId:
      card.catalog.id === "openai_codex"
        ? card.catalog.defaultModelId ?? "openai_codex"
        : draft.modelId.trim(),
    ...(card.catalog.id === "openrouter"
      ? draft.clearOpenrouterApiKey
        ? { openrouterApiKey: "" }
        : draft.openrouterApiKey.trim().length > 0
          ? { openrouterApiKey: draft.openrouterApiKey.trim() }
          : {}
      : card.catalog.id === "anthropic"
        ? draft.clearAnthropicApiKey
          ? { anthropicApiKey: "" }
          : draft.anthropicApiKey.trim().length > 0
            ? { anthropicApiKey: draft.anthropicApiKey.trim() }
            : {}
        : {}),
    activate,
  }
}

function getCatalogRefreshError(
  catalog: ProviderModelCatalogDto | null,
  loadError: OperatorActionErrorView | null,
): OperatorActionErrorView | null {
  if (catalog?.lastRefreshError) {
    return {
      code: catalog.lastRefreshError.code,
      message: catalog.lastRefreshError.message,
      retryable: catalog.lastRefreshError.retryable,
    }
  }

  return loadError
}

function getModelGroupLabel(modelId: string, providerLabel: string): { groupId: string; groupLabel: string } {
  const trimmedModelId = modelId.trim()
  const namespace = trimmedModelId.includes("/") ? trimmedModelId.split("/")[0]?.trim() ?? "" : ""
  if (namespace.length === 0) {
    return {
      groupId: providerLabel.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_") || "provider_models",
      groupLabel: providerLabel,
    }
  }

  const normalizedNamespace = namespace.toLowerCase()
  const knownLabel = MODEL_GROUP_LABELS[normalizedNamespace]
  if (knownLabel) {
    return {
      groupId: normalizedNamespace.replace(/[^a-z0-9]+/g, "_"),
      groupLabel: knownLabel,
    }
  }

  return {
    groupId: normalizedNamespace.replace(/[^a-z0-9]+/g, "_"),
    groupLabel: namespace,
  }
}

function buildProviderModelChoice(model: ProviderModelDto, providerLabel: string): ProviderModelChoice | null {
  const modelId = model.modelId.trim()
  if (modelId.length === 0) {
    return null
  }

  const displayName = model.displayName.trim() || modelId
  const { groupId, groupLabel } = getModelGroupLabel(modelId, providerLabel)

  return {
    modelId,
    label: displayName === modelId ? modelId : `${displayName} · ${modelId}`,
    groupId,
    groupLabel,
    availability: "available",
    availabilityLabel: "Available",
  }
}

function buildOrphanedProviderModelChoice(modelId: string): ProviderModelChoice | null {
  const trimmedModelId = modelId.trim()
  if (trimmedModelId.length === 0) {
    return null
  }

  return {
    modelId: trimmedModelId,
    label: `${trimmedModelId} · unavailable`,
    groupId: "current_selection",
    groupLabel: "Current selection",
    availability: "orphaned",
    availabilityLabel: "Unavailable",
  }
}

function groupProviderModelChoices(choices: ProviderModelChoice[]): ProviderModelChoiceGroup[] {
  const groups = new Map<string, ProviderModelChoiceGroup>()

  for (const choice of choices) {
    const existingGroup = groups.get(choice.groupId)
    if (existingGroup) {
      existingGroup.items.push(choice)
      continue
    }

    groups.set(choice.groupId, {
      id: choice.groupId,
      label: choice.groupLabel,
      items: [choice],
    })
  }

  return Array.from(groups.values())
}

function getCardCatalogState(options: {
  card: ProviderProfileCard
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
  selectedModelId: string | null
}): ProviderModelCatalogState {
  const profileId = options.card.profile?.profileId ?? options.card.catalog.defaultProfileId ?? null
  const catalog = profileId ? options.providerModelCatalogs[profileId] ?? null : null
  const loadStatus: ProviderModelCatalogLoadStatus = profileId
    ? options.providerModelCatalogLoadStatuses[profileId] ?? "idle"
    : "idle"
  const refreshError = getCatalogRefreshError(
    catalog,
    profileId ? options.providerModelCatalogLoadErrors[profileId] ?? null : null,
  )
  const discoveredChoices: ProviderModelChoice[] = []
  const seenModelIds = new Set<string>()

  for (const model of catalog?.models ?? []) {
    const nextChoice = buildProviderModelChoice(model, options.card.catalog.label)
    if (!nextChoice || seenModelIds.has(nextChoice.modelId)) {
      continue
    }

    seenModelIds.add(nextChoice.modelId)
    discoveredChoices.push(nextChoice)
  }

  const selectedModelId = options.selectedModelId?.trim() ?? ""
  const selectedDiscoveredChoice = selectedModelId
    ? discoveredChoices.find((choice) => choice.modelId === selectedModelId) ?? null
    : null
  const selectedChoice =
    selectedDiscoveredChoice ?? (selectedModelId ? buildOrphanedProviderModelChoice(selectedModelId) : null)
  const choices =
    selectedChoice && selectedChoice.availability === "orphaned"
      ? [selectedChoice, ...discoveredChoices]
      : discoveredChoices

  if (catalog?.source === "live" && discoveredChoices.length > 0) {
    return {
      profileId,
      catalog,
      loadStatus,
      refreshError,
      stateLabel: "Live catalog",
      detail:
        loadStatus === "loading"
          ? `Refreshing ${options.card.catalog.label} model discovery while keeping ${discoveredChoices.length} discovered model${
              discoveredChoices.length === 1 ? "" : "s"
            } visible.`
          : `Showing ${discoveredChoices.length} discovered model${
              discoveredChoices.length === 1 ? "" : "s"
            } for ${options.card.profile?.label ?? options.card.catalog.label}.`,
      tone: "default",
      fetchedAt: getProviderModelCatalogFetchedAt(catalog),
      lastSuccessAt: catalog.lastSuccessAt ?? null,
      choices,
      selectedChoice,
    }
  }

  if (discoveredChoices.length > 0) {
    return {
      profileId,
      catalog,
      loadStatus,
      refreshError,
      stateLabel: catalog?.source === "cache" ? "Cached catalog" : "Stale catalog",
      detail: refreshError?.message?.trim()
        ? `${refreshError.message} Cadence is keeping the last successful model catalog for ${options.card.profile?.label ?? options.card.catalog.label} visible.`
        : `Cadence is keeping the last successful model catalog for ${options.card.profile?.label ?? options.card.catalog.label} visible.`,
      tone: "warning",
      fetchedAt: getProviderModelCatalogFetchedAt(catalog),
      lastSuccessAt: catalog?.lastSuccessAt ?? null,
      choices,
      selectedChoice,
    }
  }

  if (loadStatus === "loading") {
    return {
      profileId,
      catalog,
      loadStatus,
      refreshError,
      stateLabel: "Catalog unavailable",
      detail: `Loading the ${options.card.catalog.label} model catalog. Cadence is keeping configured model truth visible without reopening free-text editing.`,
      tone: "default",
      fetchedAt: getProviderModelCatalogFetchedAt(catalog),
      lastSuccessAt: catalog?.lastSuccessAt ?? null,
      choices,
      selectedChoice,
    }
  }

  const unavailableDetail = selectedChoice
    ? `${selectedChoice.modelId} remains visible as the saved model, but discovery cannot confirm it right now.`
    : `Cadence does not have a discovered model catalog for ${options.card.profile?.label ?? options.card.catalog.label} yet.`

  return {
    profileId,
    catalog,
    loadStatus,
    refreshError,
    stateLabel: "Catalog unavailable",
    detail: refreshError?.message?.trim()
      ? `${refreshError.message} ${unavailableDetail}`
      : unavailableDetail,
    tone: "warning",
    fetchedAt: getProviderModelCatalogFetchedAt(catalog),
    lastSuccessAt: catalog?.lastSuccessAt ?? null,
    choices,
    selectedChoice,
  }
}

export interface ProviderProfileFormProps {
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors?: Record<string, OperatorActionErrorView | null>
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
  runtimeSession?: RuntimeSessionView | null
  hasSelectedProject?: boolean
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  openAiMissingProjectLabel?: string
  openAiMissingProjectDescription?: string
  showUnavailableProviders?: boolean
}

export function ProviderProfileForm({
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs = {},
  providerModelCatalogLoadStatuses = {},
  providerModelCatalogLoadErrors = {},
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  runtimeSession,
  hasSelectedProject = false,
  onStartLogin,
  onLogout,
  openAiMissingProjectLabel = "Open a project",
  openAiMissingProjectDescription = "Select an imported project to sign in the selected OpenAI profile.",
  showUnavailableProviders = false,
}: ProviderProfileFormProps) {
  const [editingCardKey, setEditingCardKey] = useState<string | null>(null)
  const [drafts, setDrafts] = useState<Record<string, ProviderDraft>>({})
  const [pendingAuth, setPendingAuth] = useState<AuthPending>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [authError, setAuthError] = useState<string | null>(null)

  const cards = getProfileCards(providerProfiles)
  const unavailableProviders = showUnavailableProviders
    ? PROVIDER_CATALOG.filter((entry) => !entry.supported)
    : []

  const isRefreshing = providerProfilesLoadStatus === "loading"
  const isSaving = providerProfilesSaveStatus === "running"
  const selectedProfile = getActiveProviderProfile(providerProfiles)
  const selectedProfileReference = getProfileReference(selectedProfile)
  const selectedProvider = resolveSelectedRuntimeProvider(providerProfiles, null, runtimeSession ?? null)
  const providerMismatchCopy = getProviderMismatchCopy(selectedProvider, runtimeSession ?? null)
  const selectedProfileUnavailableMessage =
    providerProfiles &&
    providerProfilesLoadStatus !== "loading" &&
    selectedProvider.providerId === "openai_codex" &&
    (!selectedProfile || selectedProfile.providerId !== "openai_codex")
      ? "Cadence could not start OpenAI login because the selected provider profile is unavailable. Refresh Settings and retry."
      : null

  useEffect(() => {
    setAuthError(null)
  }, [providerProfiles?.activeProfileId])

  useEffect(() => {
    if (!onRefreshProviderModelCatalog) {
      return
    }

    for (const card of cards) {
      if (!card.profile) {
        continue
      }

      const profileId = card.profile.profileId
      const loadStatus = providerModelCatalogLoadStatuses[profileId] ?? "idle"
      const hasCatalog = Boolean(providerModelCatalogs[profileId])
      if (loadStatus === "idle" && !hasCatalog) {
        void onRefreshProviderModelCatalog(profileId, { force: false }).catch(() => undefined)
      }
    }
  }, [cards, onRefreshProviderModelCatalog, providerModelCatalogLoadStatuses, providerModelCatalogs])

  function getDraft(card: ProviderProfileCard): ProviderDraft {
    return drafts[card.key] ?? createDraft(card)
  }

  function setDraft(card: ProviderProfileCard, next: ProviderDraft) {
    setDrafts((current) => ({
      ...current,
      [card.key]: next,
    }))
  }

  function openEditor(card: ProviderProfileCard) {
    setEditingCardKey(card.key)
    setDrafts((current) => ({
      ...current,
      [card.key]: current[card.key] ?? createDraft(card),
    }))
    setFormError(null)
    setAuthError(null)
  }

  function closeEditor(cardKey: string) {
    setEditingCardKey((current) => (current === cardKey ? null : current))
    setFormError(null)
    setDrafts((current) => {
      const next = { ...current }
      delete next[cardKey]
      return next
    })
  }

  async function handleSave(card: ProviderProfileCard) {
    if (!onUpsertProviderProfile) return

    const draft = getDraft(card)

    if (!draft.label.trim()) {
      setFormError("Profile label is required.")
      return
    }

    if (card.catalog.id === "openrouter" || card.catalog.id === "anthropic") {
      const hasSavedKey = card.profile?.providerId === card.catalog.id && card.profile.readiness.ready
      const isClearingKey =
        card.catalog.id === "openrouter" ? draft.clearOpenrouterApiKey : draft.clearAnthropicApiKey
      const apiKeyValue =
        card.catalog.id === "openrouter" ? draft.openrouterApiKey : draft.anthropicApiKey

      if (!draft.modelId.trim()) {
        setFormError("Choose a discovered model before saving.")
        return
      }

      if (!hasSavedKey && !isClearingKey && !apiKeyValue.trim()) {
        setFormError(`${card.catalog.label} requires an API key.`)
        return
      }
    }

    setFormError(null)

    try {
      const activate = providerProfiles?.activeProfileId?.trim()
        ? providerProfiles.activeProfileId === getProfileId(card)
        : card.profile?.active ?? false
      await onUpsertProviderProfile(buildUpsertRequest(card, draft, activate))
      closeEditor(card.key)
    } catch {
      setDraft(card, {
        ...draft,
        openrouterApiKey: "",
        anthropicApiKey: "",
      })
    }
  }

  async function handleActivate(card: ProviderProfileCard) {
    if (isCardSelected(providerProfiles, card)) return

    setFormError(null)
    setAuthError(null)

    try {
      if (card.profile) {
        await onSetActiveProviderProfile?.(card.profile.profileId)
        return
      }

      const draft = createDraft(card)
      await onUpsertProviderProfile?.({
        profileId: getProfileId(card),
        providerId: card.catalog.id as SupportedProviderId,
        label: draft.label,
        modelId: draft.modelId,
        activate: true,
      })
    } catch {
      // Hook state surfaces the typed save error while the last truthful snapshot remains visible.
    }
  }

  async function handleRefreshCatalog(card: ProviderProfileCard) {
    const profileId = card.profile?.profileId
    if (!profileId || !onRefreshProviderModelCatalog) {
      return
    }

    setFormError(null)
    await onRefreshProviderModelCatalog(profileId, { force: true }).catch(() => undefined)
  }

  async function handleOpenAiConnect() {
    if (!hasSelectedProject || !onStartLogin) return

    if (!selectedProfile || selectedProfile.providerId !== "openai_codex") {
      setAuthError(
        selectedProfileUnavailableMessage ??
          "Cadence could not start OpenAI login because the selected provider profile is unavailable. Refresh Settings and retry.",
      )
      return
    }

    setPendingAuth("login")
    setFormError(null)
    setAuthError(null)

    try {
      const next = await onStartLogin()
      if (next?.authorizationUrl) {
        try {
          await openUrl(next.authorizationUrl)
        } catch {
          // Browser open failed — the runtime flow still started in the desktop backend.
        }
      }
    } catch (error) {
      setAuthError(errMsg(error, "Could not start login."))
    } finally {
      setPendingAuth(null)
    }
  }

  async function handleOpenAiDisconnect() {
    if (!onLogout) return

    setPendingAuth("logout")
    setFormError(null)
    setAuthError(null)

    try {
      await onLogout()
    } catch (error) {
      setAuthError(errMsg(error, "Could not sign out."))
    } finally {
      setPendingAuth(null)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      {providerProfilesLoadError ? (
        <Alert variant="destructive" className="border-destructive/30 bg-destructive/5 py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[13px]">
            {errorViewMessage(providerProfilesLoadError, "Failed to load app-local provider profiles.")}
            {onRefreshProviderProfiles ? (
              <Button
                variant="outline"
                size="sm"
                className="mt-2.5 h-7 gap-1 text-[11px]"
                disabled={isRefreshing}
                onClick={() => void onRefreshProviderProfiles({ force: true }).catch(() => undefined)}
              >
                {isRefreshing ? <LoaderCircle className="h-3 w-3 animate-spin" /> : null}
                Retry
              </Button>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {providerProfilesSaveError ? (
        <Alert variant="destructive" className="border-destructive/30 bg-destructive/5 py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[13px]">
            {errorViewMessage(providerProfilesSaveError, "Failed to save the selected provider profile.")}
          </AlertDescription>
        </Alert>
      ) : null}

      {selectedProfileUnavailableMessage ? (
        <Alert variant="destructive" className="border-destructive/30 bg-destructive/5 py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[13px]">{selectedProfileUnavailableMessage}</AlertDescription>
        </Alert>
      ) : null}

      {formError ? (
        <Alert variant="destructive" className="border-destructive/30 bg-destructive/5 py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[13px]">{formError}</AlertDescription>
        </Alert>
      ) : null}

      <div className="grid gap-3">
        {cards.map((card) => {
          const draft = getDraft(card)
          const isEditing = editingCardKey === card.key
          const isOpenRouter = card.catalog.id === "openrouter"
          const isAnthropic = card.catalog.id === "anthropic"
          const isApiKeyProvider = isOpenRouter || isAnthropic
          const isOpenAi = card.catalog.id === "openai_codex"
          const isSelected = isCardSelected(providerProfiles, card)
          const readinessBadge = getApiKeyProviderReadinessBadge(card.profile)
          const hasSavedApiKey = Boolean(
            isApiKeyProvider && card.profile?.providerId === card.catalog.id && card.profile.readiness.ready,
          )
          const shouldRenderOpenAiAuth = isOpenAi && isSelected && Boolean(onStartLogin && onLogout)
          const isSelectedRuntimeProvider = runtimeSession?.providerId === selectedProvider.providerId
          const selectedRuntimeErrorMessage = runtimeSession?.lastError?.message?.trim() || null
          const isOpenAiConnected = Boolean(
            shouldRenderOpenAiAuth &&
              selectedProvider.providerId === "openai_codex" &&
              runtimeSession?.providerId === "openai_codex" &&
              runtimeSession.isAuthenticated,
          )
          const isOpenAiInProgress = Boolean(
            shouldRenderOpenAiAuth &&
              selectedProvider.providerId === "openai_codex" &&
              runtimeSession?.providerId === "openai_codex" &&
              runtimeSession.isLoginInProgress,
          )
          const inlineStatus = isSelected
            ? providerMismatchCopy
              ? {
                  tone: "warning" as const,
                  message: providerMismatchCopy.reason,
                  recovery: providerMismatchCopy.sessionRecoveryCopy,
                }
              : authError && isOpenAi
                ? {
                    tone: "error" as const,
                    message: authError,
                    recovery: null,
                  }
                : isSelectedRuntimeProvider && selectedRuntimeErrorMessage
                  ? {
                      tone: "error" as const,
                      message: selectedRuntimeErrorMessage,
                      recovery: null,
                    }
                  : null
            : null
          const selectedModelId = (draft.modelId.trim() || card.profile?.modelId || card.catalog.defaultModelId || "").trim() || null
          const cardCatalogState = getCardCatalogState({
            card,
            providerModelCatalogs,
            providerModelCatalogLoadStatuses,
            providerModelCatalogLoadErrors,
            selectedModelId,
          })
          const modelChoiceGroups = groupProviderModelChoices(cardCatalogState.choices)
          const isCatalogRefreshing = cardCatalogState.loadStatus === "loading"
          const canRefreshCatalog = Boolean(onRefreshProviderModelCatalog && card.profile)

          return (
            <div
              key={card.key}
              className={cn(
                "rounded-lg border bg-card px-5 py-4 transition-colors",
                isSelected ? "border-primary/30 bg-primary/[0.03]" : "border-border",
              )}
            >
              <div className="flex items-start gap-3.5">
                <div
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-secondary/60",
                    isSelected ? "border-primary/40 text-primary" : "border-border",
                  )}
                >
                  <card.catalog.Icon className="h-4 w-4 text-foreground/70" />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="text-[14px] font-medium text-foreground">
                      {card.profile?.label ?? card.catalog.label}
                    </p>
                    {isSelected ? (
                      <Badge variant="secondary" className="px-2 py-0 text-[11px]">
                        Active
                      </Badge>
                    ) : null}
                    {readinessBadge ? (
                      <Badge className={cn("px-2 py-0 text-[11px] font-medium", readinessBadge.className)}>
                        {readinessBadge.label}
                      </Badge>
                    ) : null}
                    {isOpenAi && isOpenAiConnected ? (
                      <Badge
                        variant="secondary"
                        className="gap-1.5 border border-emerald-500/30 bg-emerald-500/10 px-2 py-0 text-[11px] font-medium text-emerald-500 dark:text-emerald-400"
                      >
                        <span className="h-1.5 w-1.5 rounded-full bg-emerald-500 dark:bg-emerald-400" />
                        Connected
                      </Badge>
                    ) : null}
                  </div>

                  <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
                    {card.catalog.description}
                  </p>

                  <p className="mt-1.5 text-[11.5px] text-muted-foreground">
                    Model:{" "}
                    <span className="font-medium text-foreground/80">
                      {card.profile?.modelId ?? card.catalog.defaultModelId ?? "Not configured"}
                    </span>
                  </p>

                  {inlineStatus ? (
                    <Alert
                      variant={inlineStatus.tone === "error" ? "destructive" : "default"}
                      className={cn(
                        "mt-2.5 py-3",
                        inlineStatus.tone === "warning"
                          ? "border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-200"
                          : "border-destructive/30 bg-destructive/5",
                      )}
                    >
                      <AlertCircle className="h-4 w-4" />
                      <AlertDescription className="text-[13px] leading-relaxed">
                        <span>{inlineStatus.message}</span>
                        {inlineStatus.recovery ? <span className="mt-1 block">{inlineStatus.recovery}</span> : null}
                      </AlertDescription>
                    </Alert>
                  ) : null}
                </div>

                <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                  {isSelected ? null : (
                    <Button
                      size="sm"
                      variant="outline"
                      className="h-8 text-[12px]"
                      disabled={isSaving || isRefreshing || !onUpsertProviderProfile}
                      onClick={() => void handleActivate(card)}
                    >
                      Use this
                    </Button>
                  )}

                  {isEditing ? null : isOpenAi ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      className="h-8 text-[12px]"
                      disabled={isSaving || isRefreshing}
                      onClick={() => openEditor(card)}
                    >
                      Edit label
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant={hasSavedApiKey ? "secondary" : "outline"}
                      className="h-8 text-[12px]"
                      disabled={isSaving || isRefreshing}
                      onClick={() => openEditor(card)}
                    >
                      {hasSavedApiKey ? "Edit" : "Set up"}
                    </Button>
                  )}

                  {shouldRenderOpenAiAuth ? (
                    isOpenAiConnected ? (
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-8 gap-1.5 text-[12px]"
                        disabled={pendingAuth !== null || isRefreshing || isSaving}
                        onClick={() => void handleOpenAiDisconnect()}
                      >
                        {pendingAuth === "logout" ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <LogOut className="h-3.5 w-3.5" />
                        )}
                        Sign out
                      </Button>
                    ) : isOpenAiInProgress ? (
                      <Badge variant="secondary" className="gap-1.5 text-[11px]">
                        <LoaderCircle className="h-3 w-3 animate-spin" />
                        Connecting…
                      </Badge>
                    ) : !hasSelectedProject ? (
                      <Badge variant="outline" className="text-[11px]">
                        {openAiMissingProjectLabel}
                      </Badge>
                    ) : (
                      <Button
                        size="sm"
                        className="h-8 gap-1.5 text-[12px]"
                        disabled={pendingAuth !== null || isRefreshing || isSaving}
                        onClick={() => void handleOpenAiConnect()}
                      >
                        {pendingAuth === "login" ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <LogIn className="h-3.5 w-3.5" />
                        )}
                        Sign in
                      </Button>
                    )
                  ) : null}
                </div>
              </div>

              {isEditing ? (
                <div className="mt-3.5 grid gap-3.5 rounded-md border border-dashed border-border/80 bg-background/80 p-3.5">
                  <div className="space-y-2">
                    <Label htmlFor={`${card.key}-label`} className="text-[12px]">
                      Profile label
                    </Label>
                    <Input
                      id={`${card.key}-label`}
                      className="h-9 text-[13px]"
                      disabled={isSaving || isRefreshing}
                      onChange={(event) =>
                        setDraft(card, {
                          ...draft,
                          label: event.target.value,
                        })
                      }
                      placeholder={card.catalog.label}
                      value={draft.label}
                    />
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <Label htmlFor={`${card.key}-model`} className="text-[12px]">
                        Model
                      </Label>
                      {canRefreshCatalog ? (
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="h-7 gap-1.5 px-2.5 text-[11px]"
                          disabled={isSaving || isRefreshing || isCatalogRefreshing}
                          onClick={() => void handleRefreshCatalog(card)}
                        >
                          {isCatalogRefreshing ? <LoaderCircle className="h-3 w-3 animate-spin" /> : null}
                          Refresh models
                        </Button>
                      ) : null}
                    </div>

                    {isOpenAi ? (
                      <div className="rounded-md border border-border/80 bg-muted/25 px-3.5 py-3">
                        <p className="text-[13px] font-medium text-foreground">OpenAI Codex</p>
                        <p className="mt-1 font-mono text-[12px] text-muted-foreground">
                          {card.catalog.defaultModelId ?? "openai_codex"}
                        </p>
                      </div>
                    ) : (
                      <Select
                        disabled={isSaving || isRefreshing || cardCatalogState.choices.length === 0}
                        value={draft.modelId}
                        onValueChange={(value) =>
                          setDraft(card, {
                            ...draft,
                            modelId: value,
                          })
                        }
                      >
                        <SelectTrigger id={`${card.key}-model`} className="h-9 w-full text-[13px]" size="sm">
                          <SelectValue placeholder="No models available" />
                        </SelectTrigger>
                        <SelectContent>
                          {modelChoiceGroups.map((group, index) => (
                            <div key={group.id}>
                              {index > 0 ? <SelectSeparator /> : null}
                              <SelectGroup>
                                <SelectLabel>{group.label}</SelectLabel>
                                {group.items.map((choice) => (
                                  <SelectItem key={choice.modelId} value={choice.modelId}>
                                    {choice.label}
                                  </SelectItem>
                                ))}
                              </SelectGroup>
                            </div>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  </div>

                  {isApiKeyProvider ? (
                    <div className="space-y-2">
                      <div className="flex items-center justify-between gap-3">
                        <Label htmlFor={`${card.key}-api-key`} className="text-[12px]">
                          API Key
                        </Label>
                        {hasSavedApiKey ? (
                          <Badge variant="secondary" className="gap-1.5 text-[11px]">
                            <Check className="h-3 w-3" strokeWidth={3} />
                            Key saved
                          </Badge>
                        ) : null}
                      </div>
                      <div className="flex gap-2">
                        <Input
                          id={`${card.key}-api-key`}
                          type="password"
                          autoComplete="off"
                          spellCheck={false}
                          className="h-9 flex-1 font-mono text-[13px]"
                          disabled={isSaving || isRefreshing}
                          onChange={(event) =>
                            setDraft(card, {
                              ...draft,
                              ...(isOpenRouter
                                ? {
                                    openrouterApiKey: event.target.value,
                                    clearOpenrouterApiKey:
                                      event.target.value.trim().length > 0 ? false : draft.clearOpenrouterApiKey,
                                  }
                                : {
                                    anthropicApiKey: event.target.value,
                                    clearAnthropicApiKey:
                                      event.target.value.trim().length > 0 ? false : draft.clearAnthropicApiKey,
                                  }),
                            })
                          }
                          placeholder={hasSavedApiKey ? "Leave blank to keep current key" : "Paste API key"}
                          value={isOpenRouter ? draft.openrouterApiKey : draft.anthropicApiKey}
                        />
                        {hasSavedApiKey ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-9 px-2.5 text-[12px]"
                            disabled={isSaving || isRefreshing}
                            onClick={() =>
                              setDraft(card, {
                                ...draft,
                                ...(isOpenRouter
                                  ? {
                                      openrouterApiKey: "",
                                      clearOpenrouterApiKey: !draft.clearOpenrouterApiKey,
                                    }
                                  : {
                                      anthropicApiKey: "",
                                      clearAnthropicApiKey: !draft.clearAnthropicApiKey,
                                    }),
                              })
                            }
                          >
                            {(isOpenRouter ? draft.clearOpenrouterApiKey : draft.clearAnthropicApiKey)
                              ? "Keep"
                              : "Clear"}
                          </Button>
                        ) : null}
                      </div>
                      <p
                        className={cn(
                          "text-[11px]",
                          (isOpenRouter ? draft.clearOpenrouterApiKey : draft.clearAnthropicApiKey)
                            ? "text-destructive/80"
                            : "text-muted-foreground",
                        )}
                      >
                        {(isOpenRouter ? draft.clearOpenrouterApiKey : draft.clearAnthropicApiKey)
                          ? "Saved key will be removed"
                          : hasSavedApiKey
                            ? "Blank keeps the current key"
                            : `Required for ${card.catalog.label}`}
                      </p>
                    </div>
                  ) : null}

                  <div className="flex items-center gap-2.5">
                    <Button
                      size="sm"
                      className="h-8 gap-1.5 text-[12px]"
                      disabled={isSaving || isRefreshing || !onUpsertProviderProfile}
                      onClick={() => void handleSave(card)}
                    >
                      {isSaving ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}
                      Save
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-8 text-[12px]"
                      disabled={isSaving || isRefreshing}
                      onClick={() => closeEditor(card.key)}
                    >
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : null}
            </div>
          )
        })}

        {unavailableProviders.map((provider) => (
          <div key={provider.id} className="rounded-lg border border-border/70 bg-card/30 px-5 py-4">
            <div className="flex items-center gap-3.5">
              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/40">
                <provider.Icon className="h-4 w-4 text-muted-foreground" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-[14px] font-medium text-muted-foreground">{provider.label}</p>
                  <Badge variant="outline" className="gap-1.5 text-[11px]">
                    <Lock className="h-3 w-3" />
                    Unavailable
                  </Badge>
                </div>
                <p className="mt-1 text-[12px] text-muted-foreground">{provider.description}</p>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
