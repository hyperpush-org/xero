import { useEffect, useMemo, useState, type ElementType } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  Activity,
  AlertCircle,
  Check,
  Cloud,
  KeyRound,
  LoaderCircle,
  LogIn,
  LogOut,
  Server,
  Sparkles,
} from "lucide-react"
import {
  AnthropicIcon,
  GitHubIcon,
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
import type { CloudProviderPreset } from "@/src/lib/cadence-model/provider-presets"
import {
  getProviderSetupRecipe,
  getProviderSetupRecipeDraftDefaults,
  isLocalOpenAiCompatibleBaseUrl,
  listProviderSetupRecipes,
  recommendProviderSetup,
  type CadenceDiagnosticCheckDto,
  type ProviderModelCatalogDto,
  type ProviderProfileDiagnosticsDto,
  type ProviderProfilesDto,
  type ProviderProfileDto,
  type ProviderRecommendationDto,
  type ProviderSetupRecipeApiKeyModeDto,
  type ProviderSetupRecipeDto,
  type ProviderSetupRecipeIdDto,
  type RuntimeSessionView,
  type UpsertProviderProfileRequestDto,
  upsertProviderProfileRequestSchema,
} from "@/src/lib/cadence-model"
import {
  isApiKeyCloudProvider,
  isLocalCloudProvider,
  listCloudProviderPresets,
  usesAmbientCloudProvider,
} from "@/src/lib/cadence-model/provider-presets"

type SupportedProviderId = ProviderProfileDto["providerId"]
type AuthPending = { cardKey: string } | null
type AuthErrorState = { cardKey: string; message: string }

type ProviderDraft = {
  label: string
  modelId: string
  apiKey: string
  clearApiKey: boolean
  baseUrl: string
  apiVersion: string
  region: string
  projectId: string
}

interface ProviderProfileCard {
  key: string
  preset: CloudProviderPreset
  profile: ProviderProfileDto | null
}

type ProviderProfileDiagnosticStatus = "idle" | "loading" | "ready" | "error"
type ProviderProfileApiKeyRequirement = ProviderSetupRecipeApiKeyModeDto

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

function getProviderDiagnosticChecks(
  report: ProviderProfileDiagnosticsDto,
): CadenceDiagnosticCheckDto[] {
  return [...report.validationChecks, ...report.reachabilityChecks]
}

function getActionableProviderDiagnosticChecks(
  report: ProviderProfileDiagnosticsDto,
): CadenceDiagnosticCheckDto[] {
  const checks = getProviderDiagnosticChecks(report)
  const actionable = checks.filter((check) => check.status === "failed" || check.status === "warning")
  if (actionable.length > 0) return actionable
  return checks.filter((check) => check.status === "skipped" && check.code !== "provider_profile_not_active")
}

function getProviderDiagnosticSummary(report: ProviderProfileDiagnosticsDto): string {
  const checks = getProviderDiagnosticChecks(report)
  const failed = checks.filter((check) => check.status === "failed").length
  const warnings = checks.filter((check) => check.status === "warning").length

  if (failed > 0) {
    return `Connection check found ${failed} issue${failed === 1 ? "" : "s"}.`
  }

  if (warnings > 0) {
    return `Connection check found ${warnings} warning${warnings === 1 ? "" : "s"}.`
  }

  return "Connection check passed."
}

function getDiagnosticRowClassName(check: CadenceDiagnosticCheckDto): string {
  if (check.status === "failed") {
    return "border-destructive/30 bg-destructive/5 text-destructive"
  }

  if (check.status === "warning") {
    return "border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-200"
  }

  if (check.status === "skipped") {
    return "border-border bg-muted/30 text-muted-foreground"
  }

  return "border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-200"
}

function normalizeOptionalText(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function createDraft(card: ProviderProfileCard): ProviderDraft {
  return {
    label: card.profile?.label ?? card.preset.defaultProfileLabel,
    modelId: card.profile?.modelId ?? card.preset.defaultModelId,
    apiKey: "",
    clearApiKey: false,
    baseUrl: card.profile?.baseUrl ?? "",
    apiVersion: card.profile?.apiVersion ?? "",
    region: card.profile?.region ?? "",
    projectId: card.profile?.projectId ?? "",
  }
}

function getProfileCards(providerProfiles: ProviderProfilesDto | null): ProviderProfileCard[] {
  const cards: ProviderProfileCard[] = []

  for (const preset of listCloudProviderPresets()) {
    const matches = (providerProfiles?.profiles ?? [])
      .filter((profile) => profile.providerId === preset.providerId)
      .sort((left, right) => left.label.localeCompare(right.label))

    if (matches.length === 0) {
      cards.push({
        key: `${preset.providerId}-placeholder`,
        preset,
        profile: null,
      })
      continue
    }

    if (preset.providerId === "openai_codex") {
      const profile =
        matches.find((candidate) => candidate.profileId === preset.defaultProfileId) ??
        matches.find((candidate) => candidate.label === preset.defaultProfileLabel) ??
        matches[0]

      cards.push({
        key: profile.profileId,
        preset,
        profile,
      })
      continue
    }

    cards.push(
      ...matches.map((profile) => ({
        key: profile.profileId,
        preset,
        profile,
      })),
    )
  }

  return cards
}

type ProviderGroupId = "oauth" | "hosted" | "cloud_ambient" | "local"

interface ProviderProfileCardGroup {
  id: ProviderGroupId
  label: string
  description: string
  cards: ProviderProfileCard[]
}

const PROVIDER_GROUP_ORDER: ProviderGroupId[] = ["oauth", "hosted", "cloud_ambient", "local"]

const PROVIDER_GROUP_META: Record<ProviderGroupId, { label: string; description: string }> = {
  oauth: {
    label: "Browser sign-in",
    description: "Sign in through your browser when binding a runtime session.",
  },
  hosted: {
    label: "Hosted (API key)",
    description: "Save an app-local API key for hosted inference endpoints.",
  },
  cloud_ambient: {
    label: "Cloud (ambient credentials)",
    description: "Reuse ambient cloud credentials from the desktop host.",
  },
  local: {
    label: "Local / self-hosted",
    description: "Connect to a local endpoint without storing an API key.",
  },
}

function getProviderGroupId(card: ProviderProfileCard): ProviderGroupId {
  switch (card.preset.authMode) {
    case "oauth":
      return "oauth"
    case "ambient":
      return "cloud_ambient"
    case "local":
      return "local"
    default:
      return "hosted"
  }
}

function groupProfileCards(cards: ProviderProfileCard[]): ProviderProfileCardGroup[] {
  const groups = new Map<ProviderGroupId, ProviderProfileCardGroup>()
  for (const groupId of PROVIDER_GROUP_ORDER) {
    groups.set(groupId, {
      id: groupId,
      label: PROVIDER_GROUP_META[groupId].label,
      description: PROVIDER_GROUP_META[groupId].description,
      cards: [],
    })
  }

  for (const card of cards) {
    groups.get(getProviderGroupId(card))!.cards.push(card)
  }

  return PROVIDER_GROUP_ORDER.map((id) => groups.get(id)!).filter(
    (group) => group.cards.length > 0,
  )
}

function getProviderReadinessBadge(profile: ProviderProfileDto | null) {
  if (!profile || profile.providerId === "openai_codex") return null

  if (profile.readiness.status === "ready") {
    if (profile.readiness.proof === "local") {
      return {
        label: "Local",
        className: "border border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-300",
      }
    }

    if (profile.readiness.proof === "ambient") {
      return {
        label: "Ambient auth",
        className: "border border-cyan-500/30 bg-cyan-500/10 text-cyan-600 dark:text-cyan-300",
      }
    }

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

  if (isLocalCloudProvider(profile.providerId)) {
    return {
      label: "Needs local setup",
      className: "border border-border bg-secondary text-muted-foreground",
    }
  }

  if (usesAmbientCloudProvider(profile.providerId)) {
    return {
      label: "Needs ambient setup",
      className: "border border-border bg-secondary text-muted-foreground",
    }
  }

  return {
    label: "Needs key",
    className: "border border-border bg-secondary text-muted-foreground",
  }
}

function getProviderSetupButtonLabel(card: ProviderProfileCard): string {
  switch (card.preset.authMode) {
    case "api_key":
      return "API key"
    case "local":
      return "Endpoint"
    case "ambient":
      return "Cloud config"
    case "oauth":
      return "Sign in"
  }
}

function hasSavedApiKeyCredential(card: ProviderProfileCard): boolean {
  return Boolean(
    card.profile &&
      isApiKeyCloudProvider(card.profile.providerId) &&
      card.profile.readiness.status !== "missing",
  )
}

function getProfileId(card: ProviderProfileCard): string {
  return card.profile?.profileId ?? card.preset.defaultProfileId
}

function getDraftApiKeyRequirement(
  card: ProviderProfileCard,
  draft: ProviderDraft,
  recipe: ProviderSetupRecipeDto | null,
): ProviderProfileApiKeyRequirement {
  if (card.preset.authMode !== "api_key") {
    return "none"
  }

  if (recipe) {
    return recipe.apiKeyMode
  }

  if (card.preset.providerId === "openai_api" && isLocalOpenAiCompatibleBaseUrl(draft.baseUrl)) {
    return "none"
  }

  return "required"
}

function getApiKeyHelpCopy(options: {
  card: ProviderProfileCard
  draft: ProviderDraft
  requirement: ProviderProfileApiKeyRequirement
  recipe: ProviderSetupRecipeDto | null
  hasSavedApiKey: boolean
}): string {
  if (options.requirement === "none") {
    return options.hasSavedApiKey
      ? "Saved key will be removed when this local endpoint is saved"
      : `No app-local API key is stored for ${options.recipe?.label ?? options.card.preset.label}`
  }

  if (options.draft.clearApiKey) {
    return "Saved key will be removed"
  }

  if (options.hasSavedApiKey) {
    return "Blank keeps the current key"
  }

  if (options.requirement === "optional") {
    return options.recipe
      ? `Optional for ${options.recipe.label}`
      : `Optional for ${options.card.preset.label}`
  }

  return `Required for ${options.card.preset.label}`
}

function buildUpsertRequest(
  card: ProviderProfileCard,
  draft: ProviderDraft,
  recipe: ProviderSetupRecipeDto | null,
  activate: boolean,
  apiKeyRequirement: ProviderProfileApiKeyRequirement,
): UpsertProviderProfileRequestDto {
  const baseUrl = card.preset.baseUrlMode === "none" ? null : normalizeOptionalText(draft.baseUrl)
  const apiVersion =
    card.preset.apiVersionMode === "none"
      ? null
      : baseUrl
        ? normalizeOptionalText(draft.apiVersion)
        : null
  const region = card.preset.regionMode === "none" ? null : normalizeOptionalText(draft.region)
  const projectId =
    card.preset.projectIdMode === "none" ? null : normalizeOptionalText(draft.projectId)

  const apiKey =
    card.preset.authMode !== "api_key"
      ? null
      : apiKeyRequirement === "none"
        ? hasSavedApiKeyCredential(card)
          ? ""
          : null
        : draft.clearApiKey
          ? ""
          : normalizeOptionalText(draft.apiKey)

  return {
    profileId: getProfileId(card),
    providerId: card.preset.providerId,
    runtimeKind: card.preset.runtimeKind,
    label: draft.label.trim(),
    modelId: resolveInternalModelId(card, draft, recipe),
    presetId: card.preset.presetId ?? null,
    baseUrl,
    apiVersion,
    region,
    projectId,
    apiKey,
    activate,
  }
}

function resolveInternalModelId(
  card: ProviderProfileCard,
  draft: ProviderDraft,
  recipe: ProviderSetupRecipeDto | null,
): string {
  if (card.preset.providerId === "openai_codex") {
    return card.preset.defaultModelId
  }

  return (
    draft.modelId.trim() ||
    card.profile?.modelId?.trim() ||
    recipe?.defaultModelId.trim() ||
    card.preset.defaultModelId
  )
}

export interface ProviderProfileFormProps {
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onCheckProviderProfile?: (
    profileId: string,
    options?: { includeNetwork?: boolean },
  ) => Promise<ProviderProfileDiagnosticsDto>
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  runtimeSession?: RuntimeSessionView | null
  hasSelectedProject?: boolean
  onStartLogin?: (options?: { profileId?: string | null }) => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onLogoutProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
}

export function ProviderProfileForm({
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs = {},
  providerModelCatalogLoadStatuses = {},
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onCheckProviderProfile,
  onUpsertProviderProfile,
  runtimeSession,
  hasSelectedProject = false,
  onStartLogin,
  onLogoutProviderProfile,
}: ProviderProfileFormProps) {
  const [editingCardKey, setEditingCardKey] = useState<string | null>(null)
  const [drafts, setDrafts] = useState<Record<string, ProviderDraft>>({})
  const [pendingAuth, setPendingAuth] = useState<AuthPending>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [authError, setAuthError] = useState<AuthErrorState | null>(null)
  const [profileDiagnostics, setProfileDiagnostics] = useState<Record<string, ProviderProfileDiagnosticsDto>>({})
  const [profileDiagnosticStatuses, setProfileDiagnosticStatuses] =
    useState<Record<string, ProviderProfileDiagnosticStatus>>({})
  const [profileDiagnosticErrors, setProfileDiagnosticErrors] = useState<Record<string, string | null>>({})
  const recipes = useMemo(() => listProviderSetupRecipes(), [])
  const [selectedRecipeId, setSelectedRecipeId] = useState<ProviderSetupRecipeIdDto>(
    recipes[0]?.recipeId ?? "custom_openai_compatible",
  )
  const [appliedRecipeIds, setAppliedRecipeIds] = useState<Record<string, ProviderSetupRecipeIdDto>>({})

  const cards = getProfileCards(providerProfiles)
  const cardGroups = useMemo(
    () => groupProfileCards(cards),
    [cards],
  )
  const recommendationSet = useMemo(() => recommendProviderSetup(providerProfiles), [providerProfiles])
  const primaryRecommendation =
    recommendationSet.primary?.action === "activate_profile" ? null : recommendationSet.primary
  const selectedRecipe = getProviderSetupRecipe(selectedRecipeId) ?? recipes[0] ?? null
  const isRefreshing = providerProfilesLoadStatus === "loading"
  const isSaving = providerProfilesSaveStatus === "running"
  const isMutationDisabled = isSaving || !onUpsertProviderProfile

  useEffect(() => {
    setAuthError(null)
  }, [providerProfiles])

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

  function getAppliedRecipe(card: ProviderProfileCard): ProviderSetupRecipeDto | null {
    return getProviderSetupRecipe(appliedRecipeIds[card.key] ?? null)
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

  function applyRecipe(recipe: ProviderSetupRecipeDto) {
    const openAiCompatibleCard =
      cards.find((card) => card.preset.providerId === "openai_api" && !card.profile) ??
      cards.find((card) => card.preset.providerId === "openai_api") ??
      null

    if (!openAiCompatibleCard) {
      setFormError("Cadence could not find the OpenAI-compatible provider card.")
      return
    }

    const defaults = getProviderSetupRecipeDraftDefaults(recipe.recipeId)
    setEditingCardKey(openAiCompatibleCard.key)
    setDrafts((current) => ({
      ...current,
      [openAiCompatibleCard.key]: {
        ...createDraft(openAiCompatibleCard),
        label: defaults.label,
        modelId: defaults.modelId,
        baseUrl: defaults.baseUrl,
        apiVersion: defaults.apiVersion,
        apiKey: "",
        clearApiKey: false,
      },
    }))
    setAppliedRecipeIds((current) => ({
      ...current,
      [openAiCompatibleCard.key]: recipe.recipeId,
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
    setAppliedRecipeIds((current) => {
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

    if (card.preset.baseUrlMode === "required" && !draft.baseUrl.trim()) {
      setFormError(`${card.preset.label} requires a base URL.`)
      return
    }

    if (card.preset.apiVersionMode === "required" && !draft.apiVersion.trim()) {
      setFormError(`${card.preset.label} requires an API version.`)
      return
    }

    if (card.preset.regionMode === "required" && !draft.region.trim()) {
      setFormError(`${card.preset.label} requires a region.`)
      return
    }

    if (card.preset.projectIdMode === "required" && !draft.projectId.trim()) {
      setFormError(`${card.preset.label} requires a project ID.`)
      return
    }

    const appliedRecipe = getAppliedRecipe(card)
    if (appliedRecipe?.requiredFields.includes("baseUrl") && !draft.baseUrl.trim()) {
      setFormError(`${appliedRecipe.label} requires a base URL.`)
      return
    }
    const apiKeyRequirement = getDraftApiKeyRequirement(card, draft, appliedRecipe)

    if (apiKeyRequirement === "required") {
      const hasSavedKey = hasSavedApiKeyCredential(card)
      if (!hasSavedKey && !draft.clearApiKey && !draft.apiKey.trim()) {
        setFormError(`${appliedRecipe?.label ?? card.preset.label} requires an API key.`)
        return
      }
    }

    const parsedRequest = upsertProviderProfileRequestSchema.safeParse(
      buildUpsertRequest(card, draft, appliedRecipe, false, apiKeyRequirement),
    )

    if (!parsedRequest.success) {
      setFormError(parsedRequest.error.issues[0]?.message ?? "Cadence rejected the provider profile request.")
      return
    }

    setFormError(null)

    try {
      await onUpsertProviderProfile(parsedRequest.data)
      closeEditor(card.key)
    } catch {
      setDraft(card, {
        ...draft,
        apiKey: "",
      })
    }
  }

  async function handleRecommendationAction(recommendation: ProviderRecommendationDto) {
    setFormError(null)
    setAuthError(null)

    if (recommendation.action === "apply_recipe") {
      const recipe = getProviderSetupRecipe(recommendation.recipeId)
      if (recipe) {
        applyRecipe(recipe)
      }
      return
    }

    const card = cards.find((candidate) => candidate.profile?.profileId === recommendation.profileId) ?? null
    if (!card) {
      setFormError("Cadence could not find the recommended provider profile.")
      return
    }

    openEditor(card)
  }

  async function handleCheckConnection(card: ProviderProfileCard) {
    const profileId = card.profile?.profileId
    if (!profileId || !onCheckProviderProfile) {
      return
    }

    setFormError(null)
    setAuthError(null)
    setProfileDiagnosticStatuses((currentStatuses) => ({
      ...currentStatuses,
      [profileId]: "loading",
    }))
    setProfileDiagnosticErrors((currentErrors) => ({
      ...currentErrors,
      [profileId]: null,
    }))

    try {
      const report = await onCheckProviderProfile(profileId, { includeNetwork: true })
      setProfileDiagnostics((currentReports) => ({
        ...currentReports,
        [profileId]: report,
      }))
      setProfileDiagnosticStatuses((currentStatuses) => ({
        ...currentStatuses,
        [profileId]: "ready",
      }))
    } catch (error) {
      setProfileDiagnosticStatuses((currentStatuses) => ({
        ...currentStatuses,
        [profileId]: "error",
      }))
      setProfileDiagnosticErrors((currentErrors) => ({
        ...currentErrors,
        [profileId]: errMsg(error, `Could not check ${card.preset.label}.`),
      }))
    }
  }

  async function handleOpenAiConnect(card: ProviderProfileCard) {
    if (!hasSelectedProject || !onStartLogin) return

    const profileId = card.profile?.profileId ?? null
    if (!profileId) {
      setAuthError({
        cardKey: card.key,
        message:
          "Cadence could not start OpenAI login because the OpenAI provider profile is unavailable. Refresh Settings and retry.",
      })
      return
    }

    setPendingAuth({ cardKey: card.key })
    setFormError(null)
    setAuthError(null)

    try {
      const next = await onStartLogin({ profileId })
      if (next?.authorizationUrl) {
        try {
          await openUrl(next.authorizationUrl)
        } catch {
          // Browser open failed — the runtime flow still started in the desktop backend.
        }
      }
    } catch (error) {
      setAuthError({ cardKey: card.key, message: errMsg(error, "Could not start login.") })
    } finally {
      setPendingAuth(null)
    }
  }

  async function handleOpenAiLogout(card: ProviderProfileCard) {
    if (!onLogoutProviderProfile) return

    const profileId = card.profile?.profileId ?? null
    if (!profileId) {
      setAuthError({
        cardKey: card.key,
        message:
          "Cadence could not sign out because the OpenAI provider profile is unavailable. Refresh Settings and retry.",
      })
      return
    }

    setPendingAuth({ cardKey: card.key })
    setFormError(null)
    setAuthError(null)

    try {
      await onLogoutProviderProfile(profileId)
      setProfileDiagnostics((currentReports) => {
        const next = { ...currentReports }
        delete next[profileId]
        return next
      })
      setProfileDiagnosticStatuses((currentStatuses) => {
        const next = { ...currentStatuses }
        delete next[profileId]
        return next
      })
      setProfileDiagnosticErrors((currentErrors) => {
        const next = { ...currentErrors }
        delete next[profileId]
        return next
      })
    } catch (error) {
      setAuthError({ cardKey: card.key, message: errMsg(error, "Could not sign out.") })
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
            {errorViewMessage(providerProfilesSaveError, "Failed to save the provider profile.")}
          </AlertDescription>
        </Alert>
      ) : null}

      {formError ? (
        <Alert variant="destructive" className="border-destructive/30 bg-destructive/5 py-3">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-[13px]">{formError}</AlertDescription>
        </Alert>
      ) : null}

      <div className="grid gap-3">
        {primaryRecommendation ? (
          <div className="rounded-md border border-primary/20 bg-primary/[0.035] px-3.5 py-3">
            <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <Sparkles className="h-3.5 w-3.5 text-primary" />
                  <p className="text-[12.5px] font-semibold text-foreground">
                    {primaryRecommendation.title}
                  </p>
                  <Badge variant="secondary" className="text-[10.5px]">
                    {primaryRecommendation.kind === "best_local_profile"
                      ? "Local"
                      : primaryRecommendation.kind === "fastest_ready_profile"
                        ? "Ready path"
                        : primaryRecommendation.kind === "missing_key_cloud_profile"
                          ? "Setup"
                          : "Repair"}
                  </Badge>
                </div>
                <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
                  {primaryRecommendation.message}
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-8 shrink-0 text-[12px]"
                onClick={() => void handleRecommendationAction(primaryRecommendation as ProviderRecommendationDto)}
              >
                {primaryRecommendation.actionLabel}
              </Button>
            </div>
          </div>
        ) : null}

        {selectedRecipe ? (
          <div className="rounded-md border border-border/70 bg-muted/20 px-3.5 py-3">
            <div className="grid gap-3 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)_auto] md:items-end">
              <div className="space-y-2">
                <Label htmlFor="provider-setup-recipe" className="text-[12px]">
                  Setup recipe
                </Label>
                <Select
                  value={selectedRecipeId}
                  onValueChange={(value) => setSelectedRecipeId(value as ProviderSetupRecipeIdDto)}
                >
                  <SelectTrigger id="provider-setup-recipe" className="h-9 w-full text-[13px]" size="sm">
                    <SelectValue placeholder="Choose recipe" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectLabel>OpenAI-compatible</SelectLabel>
                      {recipes.map((recipe) => (
                        <SelectItem key={recipe.recipeId} value={recipe.recipeId}>
                          {recipe.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-1.5">
                  <Badge variant="secondary" className="text-[10.5px]">
                    {selectedRecipe.apiKeyMode === "none"
                      ? "No key"
                      : selectedRecipe.apiKeyMode === "optional"
                        ? "Optional key"
                        : "API key"}
                  </Badge>
                  <Badge variant="outline" className="text-[10.5px]">
                    {selectedRecipe.modelCatalogExpectation === "live"
                      ? "Live catalog"
                      : selectedRecipe.modelCatalogExpectation === "manual"
                        ? "Manual catalog"
                        : "Catalog fallback"}
                  </Badge>
                </div>
                <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
                  {selectedRecipe.description}
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                className="h-8 gap-1.5 text-[12px]"
                onClick={() => applyRecipe(selectedRecipe)}
              >
                <Sparkles className="h-3.5 w-3.5" />
                Apply recipe
              </Button>
            </div>
          </div>
        ) : null}
      </div>

      <div className="flex flex-col gap-5">
        {cardGroups.map((group) => (
          <div key={group.id} className="flex flex-col gap-2">
            <div className="flex items-baseline justify-between gap-3 px-0.5">
              <div className="min-w-0">
                <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/80">
                  {group.label}
                </p>
                <p className="mt-0.5 text-[11.5px] leading-[1.45] text-muted-foreground">
                  {group.description}
                </p>
              </div>
              <Badge variant="outline" className="shrink-0 text-[10.5px]">
                {group.cards.length}
              </Badge>
            </div>
            <div className="grid gap-3">
              {group.cards.map((card) => {
          const draft = getDraft(card)
          const appliedRecipe = getAppliedRecipe(card)
          const apiKeyRequirement = getDraftApiKeyRequirement(card, draft, appliedRecipe)
          const Icon = PROVIDER_ICON_BY_ID[card.preset.providerId]
          const isEditing = editingCardKey === card.key
          const isApiKeyProvider = apiKeyRequirement !== "none"
          const isOpenAi = card.preset.providerId === "openai_codex"
          const readinessBadge = getProviderReadinessBadge(card.profile)
          const hasSavedApiKey = hasSavedApiKeyCredential(card)
          const shouldRenderOpenAiAuth = isOpenAi && Boolean(onStartLogin)
          const shouldRenderOpenAiLogout = isOpenAi && Boolean(onLogoutProviderProfile)
          const isRuntimeProvider = runtimeSession?.providerId === card.preset.providerId
          const selectedRuntimeErrorMessage = runtimeSession?.lastError?.message?.trim() || null
          const isOpenAiSignedIn = Boolean(isOpenAi && card.profile?.readiness.ready)
          const isOpenAiInProgress = Boolean(
            shouldRenderOpenAiAuth &&
              runtimeSession?.providerId === "openai_codex" &&
              runtimeSession.isLoginInProgress,
          )
          const cardAuthError = isOpenAi && authError?.cardKey === card.key ? authError.message : null
          const inlineStatus = cardAuthError
            ? {
                tone: "error" as const,
                message: cardAuthError,
                recovery: null,
              }
            : isRuntimeProvider && selectedRuntimeErrorMessage
              ? {
                  tone: "error" as const,
                  message: selectedRuntimeErrorMessage,
                  recovery: null,
                }
              : null
          const profileDiagnosticReport = card.profile ? profileDiagnostics[card.profile.profileId] ?? null : null
          const profileDiagnosticStatus: ProviderProfileDiagnosticStatus = card.profile
            ? profileDiagnosticStatuses[card.profile.profileId] ?? "idle"
            : "idle"
          const profileDiagnosticError = card.profile
            ? profileDiagnosticErrors[card.profile.profileId] ?? null
            : null
          const actionableDiagnosticChecks = profileDiagnosticReport
            ? getActionableProviderDiagnosticChecks(profileDiagnosticReport)
            : []
          const isCheckingConnection = profileDiagnosticStatus === "loading"
          const canCheckConnection = Boolean(!isOpenAi && onCheckProviderProfile && card.profile)
          const apiKeyHelpCopy = getApiKeyHelpCopy({
            card,
            draft,
            requirement: apiKeyRequirement,
            recipe: appliedRecipe,
            hasSavedApiKey,
          })

          const statusBadge = isOpenAi ? null : readinessBadge

          return (
            <div
              key={card.key}
              className="group rounded-lg border border-border/70 bg-card px-3.5 py-3 transition-[border-color,background-color,box-shadow] duration-150 ease-out hover:border-border hover:bg-secondary/30 hover:shadow-sm motion-reduce:transition-none"
            >
              <div className="flex items-center gap-3">
                <div
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border/70 bg-secondary/40 text-foreground/70 transition-colors duration-150 ease-out group-hover:border-border group-hover:bg-secondary/70 group-hover:text-foreground motion-reduce:transition-none"
                >
                  <Icon className="h-3.5 w-3.5" />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="truncate text-[13px] font-medium text-foreground">
                      {isOpenAi ? card.preset.label : card.profile?.label ?? card.preset.label}
                    </p>
                  </div>
                </div>

                <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
                  {statusBadge ? (
                    <span
                      className={cn(
                        "rounded-sm border px-1.5 py-px text-[10px] font-medium",
                        statusBadge.className,
                      )}
                    >
                      {statusBadge.label}
                    </span>
                  ) : null}

                  {canCheckConnection ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="h-7 gap-1.5 px-2.5 text-[11.5px]"
                      disabled={isSaving || isCheckingConnection}
                      onClick={() => void handleCheckConnection(card)}
                    >
                      {isCheckingConnection ? (
                        <LoaderCircle className="h-3 w-3 animate-spin" />
                      ) : (
                        <Activity className="h-3 w-3" />
                      )}
                      Check connection
                    </Button>
                  ) : null}

                  {isEditing || isOpenAi ? null : (
                    <Button
                      size="sm"
                      variant={hasSavedApiKey ? "ghost" : "default"}
                      className={cn(
                        "h-7 px-2.5 text-[11.5px]",
                        hasSavedApiKey ? "text-muted-foreground hover:text-foreground" : "",
                      )}
                      disabled={isSaving}
                      onClick={() => openEditor(card)}
                    >
                      {getProviderSetupButtonLabel(card)}
                    </Button>
                  )}

                  {shouldRenderOpenAiAuth || (isOpenAiSignedIn && shouldRenderOpenAiLogout) ? (
                    isOpenAiSignedIn ? (
                      <>
                        <span className="inline-flex items-center gap-1.5 rounded-sm border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-px text-[10.5px] font-medium text-emerald-600 dark:text-emerald-300">
                          Signed in
                        </span>
                        {shouldRenderOpenAiLogout ? (
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            className="h-7 gap-1.5 px-2.5 text-[11.5px]"
                            disabled={pendingAuth !== null || isSaving}
                            onClick={() => void handleOpenAiLogout(card)}
                          >
                            {pendingAuth?.cardKey === card.key ? (
                              <LoaderCircle className="h-3 w-3 animate-spin" />
                            ) : (
                              <LogOut className="h-3 w-3" />
                            )}
                            Sign out
                          </Button>
                        ) : null}
                      </>
                    ) : (
                      <Button
                        size="sm"
                        className="h-7 gap-1.5 px-2.5 text-[11.5px]"
                        disabled={pendingAuth !== null || isSaving || isOpenAiInProgress || !hasSelectedProject}
                        onClick={() => void handleOpenAiConnect(card)}
                      >
                        {pendingAuth?.cardKey === card.key || isOpenAiInProgress ? (
                          <LoaderCircle className="h-3 w-3 animate-spin" />
                        ) : (
                          <LogIn className="h-3 w-3" />
                        )}
                        Sign in
                      </Button>
                    )
                  ) : null}
                </div>
              </div>

              {inlineStatus ? (
                <Alert
                  variant={inlineStatus.tone === "error" ? "destructive" : "default"}
                  className={cn(
                    "mt-2.5 py-2.5",
                    "border-destructive/30 bg-destructive/5",
                  )}
                >
                  <AlertCircle className="h-3.5 w-3.5" />
                  <AlertDescription className="text-[12px] leading-relaxed">
                    <span>{inlineStatus.message}</span>
                    {inlineStatus.recovery ? <span className="mt-1 block">{inlineStatus.recovery}</span> : null}
                  </AlertDescription>
                </Alert>
              ) : null}

              {profileDiagnosticError ? (
                <Alert variant="destructive" className="mt-2.5 border-destructive/30 bg-destructive/5 py-2.5">
                  <AlertCircle className="h-3.5 w-3.5" />
                  <AlertDescription className="text-[12px] leading-relaxed">
                    {profileDiagnosticError}
                  </AlertDescription>
                </Alert>
              ) : profileDiagnosticReport ? (
                <div className="mt-2.5 rounded-md border border-border/80 bg-muted/20 px-3 py-2.5">
                  <div className="flex items-center gap-2">
                    <Activity className="h-3.5 w-3.5 text-muted-foreground" />
                    <p className="text-[12px] font-medium text-foreground">
                      {getProviderDiagnosticSummary(profileDiagnosticReport)}
                    </p>
                  </div>
                  {actionableDiagnosticChecks.length > 0 ? (
                    <div className="mt-2 grid gap-1.5">
                      {actionableDiagnosticChecks.map((check) => (
                        <div
                          key={check.checkId}
                          className={cn(
                            "rounded-md border px-2.5 py-2 text-[11.5px] leading-relaxed",
                            getDiagnosticRowClassName(check),
                          )}
                        >
                          <p className="font-medium">{check.message}</p>
                          {check.remediation ? (
                            <p className="mt-1 opacity-85">{check.remediation}</p>
                          ) : null}
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="mt-1.5 text-[11.5px] text-muted-foreground">
                      Validation and provider reachability checks completed without repair steps.
                    </p>
                  )}
                </div>
              ) : null}

              {isEditing ? (
                <div className="mt-3.5 grid gap-3.5 rounded-md border border-dashed border-border/80 bg-background/80 p-3.5">
                  {appliedRecipe ? (
                    <div className="rounded-md border border-primary/20 bg-primary/[0.035] px-3.5 py-3">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <Badge variant="secondary" className="text-[10.5px]">
                          {appliedRecipe.label}
                        </Badge>
                        <Badge variant="outline" className="text-[10.5px]">
                          {appliedRecipe.apiKeyMode === "none"
                            ? "No key"
                            : appliedRecipe.apiKeyMode === "optional"
                              ? "Optional key"
                              : "API key"}
                        </Badge>
                      </div>
                      <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
                        {appliedRecipe.guidance}
                      </p>
                      <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">
                        {appliedRecipe.repairSuggestion}
                      </p>
                    </div>
                  ) : null}

                  <div className="space-y-2">
                    <Label htmlFor={`${card.key}-label`} className="text-[12px]">
                      Profile label
                    </Label>
                    <Input
                      id={`${card.key}-label`}
                      className="h-9 text-[13px]"
                      disabled={isSaving}
                      onChange={(event) =>
                        setDraft(card, {
                          ...draft,
                          label: event.target.value,
                        })
                      }
                      placeholder={card.preset.defaultProfileLabel}
                      value={draft.label}
                    />
                  </div>

                  {card.preset.baseUrlMode !== "none" ||
                  card.preset.apiVersionMode !== "none" ||
                  card.preset.regionMode !== "none" ||
                  card.preset.projectIdMode !== "none" ? (
                    <div className="space-y-3 rounded-md border border-border/80 bg-muted/25 px-3.5 py-3">
                      <div>
                        <p className="text-[12px] font-medium text-foreground">Connection</p>
                        <p className="mt-1 text-[11px] text-muted-foreground">{card.preset.connectionHint}</p>
                      </div>

                      {card.preset.baseUrlMode !== "none" ? (
                        <div className="space-y-2">
                          <Label htmlFor={`${card.key}-base-url`} className="text-[12px]">
                            Base URL
                          </Label>
                          <Input
                            id={`${card.key}-base-url`}
                            className="h-9 font-mono text-[13px]"
                            disabled={isSaving}
                            onChange={(event) =>
                              setDraft(card, {
                                ...draft,
                                baseUrl: event.target.value,
                              })
                            }
                            placeholder={
                              appliedRecipe?.baseUrlPlaceholder ??
                              (card.preset.providerId === "ollama"
                                ? "http://127.0.0.1:11434/v1"
                                : card.preset.baseUrlMode === "required"
                                  ? "https://example-resource.openai.azure.com/openai/deployments/work"
                                  : "https://api.openai.com/v1")
                            }
                            value={draft.baseUrl}
                          />
                        </div>
                      ) : null}

                      {card.preset.apiVersionMode !== "none" ? (
                        <div className="space-y-2">
                          <Label htmlFor={`${card.key}-api-version`} className="text-[12px]">
                            API version
                          </Label>
                          <Input
                            id={`${card.key}-api-version`}
                            className="h-9 font-mono text-[13px]"
                            disabled={isSaving}
                            onChange={(event) =>
                              setDraft(card, {
                                ...draft,
                                apiVersion: event.target.value,
                              })
                            }
                            placeholder="2024-10-21"
                            value={draft.apiVersion}
                          />
                        </div>
                      ) : null}

                      {card.preset.regionMode !== "none" ? (
                        <div className="space-y-2">
                          <Label htmlFor={`${card.key}-region`} className="text-[12px]">
                            Region
                          </Label>
                          <Input
                            id={`${card.key}-region`}
                            className="h-9 font-mono text-[13px]"
                            disabled={isSaving}
                            onChange={(event) =>
                              setDraft(card, {
                                ...draft,
                                region: event.target.value,
                              })
                            }
                            placeholder={card.preset.providerId === "vertex" ? "us-central1" : "us-east-1"}
                            value={draft.region}
                          />
                        </div>
                      ) : null}

                      {card.preset.projectIdMode !== "none" ? (
                        <div className="space-y-2">
                          <Label htmlFor={`${card.key}-project-id`} className="text-[12px]">
                            Project ID
                          </Label>
                          <Input
                            id={`${card.key}-project-id`}
                            className="h-9 font-mono text-[13px]"
                            disabled={isSaving}
                            onChange={(event) =>
                              setDraft(card, {
                                ...draft,
                                projectId: event.target.value,
                              })
                            }
                            placeholder="vertex-project"
                            value={draft.projectId}
                          />
                        </div>
                      ) : null}
                    </div>
                  ) : null}

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
                          disabled={isSaving}
                          onChange={(event) =>
                            setDraft(card, {
                              ...draft,
                              apiKey: event.target.value,
                              clearApiKey:
                                event.target.value.trim().length > 0 ? false : draft.clearApiKey,
                            })
                          }
                          placeholder={
                            hasSavedApiKey
                              ? "Leave blank to keep current key"
                              : appliedRecipe?.apiKeyPlaceholder ??
                                (card.preset.providerId === "github_models"
                                  ? "Paste GitHub token"
                                  : "Paste API key")
                          }
                          value={draft.apiKey}
                        />
                        {hasSavedApiKey ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-9 px-2.5 text-[12px]"
                            disabled={isSaving}
                            onClick={() =>
                              setDraft(card, {
                                ...draft,
                                apiKey: "",
                                clearApiKey: !draft.clearApiKey,
                              })
                            }
                          >
                            {draft.clearApiKey ? "Keep" : "Clear"}
                          </Button>
                        ) : null}
                      </div>
                      <p
                        className={cn(
                          "text-[11px]",
                          draft.clearApiKey ? "text-destructive/80" : "text-muted-foreground",
                        )}
                      >
                        {apiKeyHelpCopy}
                      </p>
                    </div>
                  ) : card.preset.authMode === "api_key" && apiKeyRequirement === "none" ? (
                    <div className="rounded-md border border-sky-500/20 bg-sky-500/5 px-3.5 py-3 text-[12px] text-sky-700 dark:text-sky-200">
                      {apiKeyHelpCopy}
                    </div>
                  ) : card.preset.authMode === "local" ? (
                    <div className="rounded-md border border-sky-500/20 bg-sky-500/5 px-3.5 py-3 text-[12px] text-sky-700 dark:text-sky-200">
                      Cadence treats {card.preset.label} as a local endpoint. No app-local API key is stored for this provider profile.
                    </div>
                  ) : card.preset.authMode === "ambient" ? (
                    <div className="rounded-md border border-cyan-500/20 bg-cyan-500/5 px-3.5 py-3 text-[12px] text-cyan-700 dark:text-cyan-200">
                      Cadence uses ambient desktop credentials for {card.preset.label}. No app-local API key is stored for this provider profile.
                    </div>
                  ) : null}

                  <div className="flex items-center gap-2.5">
                    <Button
                      size="sm"
                      className="h-8 gap-1.5 text-[12px]"
                      disabled={isMutationDisabled}
                      onClick={() => void handleSave(card)}
                    >
                      {isSaving ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}
                      Save
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-8 text-[12px]"
                      disabled={isSaving}
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
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
