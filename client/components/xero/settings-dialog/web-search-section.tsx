import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type ElementType,
  type FormEvent,
  type ReactNode,
  type SetStateAction,
} from "react"
import {
  AlertTriangle,
  Check,
  ChevronDown,
  LoaderCircle,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react"

import {
  BraveSearchIcon,
  CustomEndpointIcon,
  ExaIcon,
  FirecrawlIcon,
  GoogleIcon,
  KagiIcon,
  LinkupIcon,
  SearchApiIcon,
  SearxngIcon,
  SerpApiIcon,
  TavilyIcon,
  YouComIcon,
} from "@/components/xero/brand-icons"
import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  AutonomousWebSearchModeDto,
  AutonomousWebSearchProviderKindMetadataDto,
  AutonomousWebSearchProviderKindDto,
  AutonomousWebSearchProviderProfileDto,
  AutonomousWebSearchSettingsDto,
  UpsertAutonomousWebSearchProviderRequestDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export type WebSearchSettingsAdapter = Pick<
  XeroDesktopAdapter,
  | "isDesktopRuntime"
  | "autonomousWebSearchSettings"
  | "autonomousWebSearchUpdateSettings"
  | "autonomousWebSearchUpsertProvider"
  | "autonomousWebSearchDeleteProvider"
  | "autonomousWebSearchSetActiveProvider"
  | "autonomousWebSearchCheckProvider"
>

type LoadState = "idle" | "loading" | "ready" | "error"
type SaveState = "idle" | "saving"

const FALLBACK_SETTINGS: AutonomousWebSearchSettingsDto = {
  mode: "auto",
  activeProviderId: null,
  providers: [],
  providerKinds: [],
  providerManaged: {
    modeAvailable: true,
    status: "depends_on_selected_model",
    message: "Provider-managed search is evaluated when a run starts with the selected provider and model.",
    supportedSources: [],
  },
  updatedAt: null,
}

const MODE_OPTIONS: readonly { value: AutonomousWebSearchModeDto; label: string; summary: string }[] = [
  { value: "auto", label: "Auto", summary: "Provider-managed first, configured provider second." },
  { value: "provider_managed_only", label: "Provider-managed only", summary: "Use the selected model provider." },
  { value: "configured_provider_only", label: "Configured provider only", summary: "Use the active fallback provider." },
  { value: "disabled", label: "Disabled", summary: "Disable the web_search tool." },
]

const PROVIDER_ICON_BY_KIND: Record<AutonomousWebSearchProviderKindDto, ElementType> = {
  custom_endpoint: CustomEndpointIcon,
  brave_search: BraveSearchIcon,
  tavily_search: TavilyIcon,
  exa_search: ExaIcon,
  firecrawl_search: FirecrawlIcon,
  you_search: YouComIcon,
  linkup_search: LinkupIcon,
  kagi_search: KagiIcon,
  searxng_json: SearxngIcon,
  serpapi_google: SerpApiIcon,
  searchapi_google: SearchApiIcon,
  google_cse: GoogleIcon,
}

interface ProviderFormState {
  profileId: string | null
  kind: AutonomousWebSearchProviderKindDto
  endpoint: string
  apiKey: string
  googleCseCx: string
}

const EMPTY_FORM: ProviderFormState = {
  profileId: null,
  kind: "brave_search",
  endpoint: "",
  apiKey: "",
  googleCseCx: "",
}

export function WebSearchSection({ adapter }: { adapter?: WebSearchSettingsAdapter }) {
  const [settings, setSettings] = useState<AutonomousWebSearchSettingsDto>(FALLBACK_SETTINGS)
  const [loadState, setLoadState] = useState<LoadState>("idle")
  const [saveState, setSaveState] = useState<SaveState>("idle")
  const [pendingProviderId, setPendingProviderId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState<ProviderFormState>(EMPTY_FORM)
  const [openProviderKey, setOpenProviderKey] = useState<string | null>(null)

  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() &&
      adapter.autonomousWebSearchSettings &&
      adapter.autonomousWebSearchUpdateSettings &&
      adapter.autonomousWebSearchUpsertProvider &&
      adapter.autonomousWebSearchDeleteProvider &&
      adapter.autonomousWebSearchSetActiveProvider &&
      adapter.autonomousWebSearchCheckProvider,
  )

  const kindMetadata = useMemo(() => {
    const map = new Map(settings.providerKinds.map((kind) => [kind.kind, kind]))
    return map
  }, [settings.providerKinds])

  const availableProviderKinds = useMemo(() => {
    const configuredKinds = new Set(settings.providers.map((provider) => provider.kind))
    return settings.providerKinds.filter((kind) => !configuredKinds.has(kind.kind))
  }, [settings.providerKinds, settings.providers])

  const load = useCallback(() => {
    if (!canUseAdapter || !adapter?.autonomousWebSearchSettings) {
      setSettings(FALLBACK_SETTINGS)
      setLoadState("ready")
      return
    }
    setLoadState("loading")
    setError(null)
    adapter
      .autonomousWebSearchSettings()
      .then((next) => {
        setSettings(next)
        setLoadState("ready")
      })
      .catch((loadError) => {
        setSettings(FALLBACK_SETTINGS)
        setError(getErrorMessage(loadError, "Xero could not load Web Search settings."))
        setLoadState("error")
      })
  }, [adapter, canUseAdapter])

  useEffect(() => {
    load()
  }, [load])

  const updateMode = useCallback(
    (mode: AutonomousWebSearchModeDto) => {
      if (!adapter?.autonomousWebSearchUpdateSettings || mode === settings.mode) return
      const previous = settings
      setSettings((current) => ({ ...current, mode }))
      setSaveState("saving")
      setError(null)
      void adapter
        .autonomousWebSearchUpdateSettings({ mode })
        .then(setSettings)
        .catch((saveError) => {
          setSettings(previous)
          setError(getErrorMessage(saveError, "Xero could not save Web Search mode."))
        })
        .finally(() => setSaveState("idle"))
    },
    [adapter, settings],
  )

  const submitProvider = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      if (!adapter?.autonomousWebSearchUpsertProvider) return
      const metadata = kindMetadata.get(form.kind)
      const request: UpsertAutonomousWebSearchProviderRequestDto = {
        profileId: form.profileId,
        kind: form.kind,
        displayName: metadata?.label || form.kind,
        endpoint: form.endpoint.trim() || null,
        apiKey: form.apiKey.trim() || null,
        googleCseCx: form.googleCseCx.trim() || null,
        enabled: true,
      }

      setSaveState("saving")
      setPendingProviderId(form.profileId ?? form.kind)
      setError(null)
      try {
        const next = await adapter.autonomousWebSearchUpsertProvider(request)
        setSettings(next)
        setForm(EMPTY_FORM)
        setOpenProviderKey(null)
      } catch (saveError) {
        setError(getErrorMessage(saveError, "Xero could not save the web-search provider."))
      } finally {
        setSaveState("idle")
        setPendingProviderId(null)
      }
    },
    [adapter, form, kindMetadata],
  )

  const updateProviderEnabled = useCallback(
    async (provider: AutonomousWebSearchProviderProfileDto, enabled: boolean) => {
      if (!adapter?.autonomousWebSearchUpsertProvider) return
      setPendingProviderId(provider.profileId)
      setError(null)
      try {
        const next = await adapter.autonomousWebSearchUpsertProvider({
          profileId: provider.profileId,
          kind: provider.kind,
          enabled,
        })
        setSettings(next)
      } catch (saveError) {
        setError(getErrorMessage(saveError, "Xero could not update the web-search provider."))
      } finally {
        setPendingProviderId(null)
      }
    },
    [adapter],
  )

  const setActive = useCallback(
    async (providerId: string) => {
      if (!adapter?.autonomousWebSearchSetActiveProvider) return
      setPendingProviderId(providerId)
      setError(null)
      try {
        const next = await adapter.autonomousWebSearchSetActiveProvider({ providerId })
        setSettings(next)
      } catch (saveError) {
        setError(getErrorMessage(saveError, "Xero could not select the active web-search provider."))
      } finally {
        setPendingProviderId(null)
      }
    },
    [adapter],
  )

  const checkProvider = useCallback(
    async (providerId: string) => {
      if (!adapter?.autonomousWebSearchCheckProvider) return
      setPendingProviderId(providerId)
      setError(null)
      try {
        const next = await adapter.autonomousWebSearchCheckProvider({ providerId })
        setSettings(next)
      } catch (checkError) {
        setError(getErrorMessage(checkError, "Xero could not test the web-search provider."))
      } finally {
        setPendingProviderId(null)
      }
    },
    [adapter],
  )

  const deleteProvider = useCallback(
    async (providerId: string) => {
      if (!adapter?.autonomousWebSearchDeleteProvider) return
      setPendingProviderId(providerId)
      setError(null)
      try {
        const next = await adapter.autonomousWebSearchDeleteProvider({ providerId })
        setSettings(next)
      } catch (deleteError) {
        setError(getErrorMessage(deleteError, "Xero could not delete the web-search provider."))
      } finally {
        setPendingProviderId(null)
      }
    },
    [adapter],
  )

  const configureProviderKind = useCallback((metadata: AutonomousWebSearchProviderKindMetadataDto) => {
    const key = providerKindKey(metadata.kind)
    if (openProviderKey === key) {
      setOpenProviderKey(null)
      setForm(EMPTY_FORM)
      return
    }
    setError(null)
    setOpenProviderKey(key)
    setForm({
      profileId: null,
      kind: metadata.kind,
      endpoint: "",
      apiKey: "",
      googleCseCx: "",
    })
  }, [openProviderKey])

  const editProvider = useCallback((provider: AutonomousWebSearchProviderProfileDto) => {
    const key = providerProfileKey(provider.profileId)
    if (openProviderKey === key) {
      setOpenProviderKey(null)
      setForm(EMPTY_FORM)
      return
    }
    setError(null)
    setOpenProviderKey(key)
    setForm({
      profileId: provider.profileId,
      kind: provider.kind,
      endpoint: provider.endpoint ?? provider.baseUrl ?? "",
      apiKey: "",
      googleCseCx: provider.googleCseCx ?? "",
    })
  }, [openProviderKey])

  const isBusy = loadState === "loading" || saveState === "saving" || pendingProviderId !== null

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Web Search"
        description="Configure the source that powers agent web_search calls."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12.5px]"
            disabled={isBusy || !canUseAdapter}
            onClick={load}
          >
            {loadState === "loading" ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
        }
      />

      {!canUseAdapter ? (
        <UnavailableCard />
      ) : (
        <>
          {error ? (
            <Alert variant="destructive" className="rounded-md px-3.5 py-2.5 text-[12.5px]">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle className="text-[12.5px] font-semibold">Web Search needs attention</AlertTitle>
              <AlertDescription className="text-[12.5px] leading-[1.5]">{error}</AlertDescription>
            </Alert>
          ) : null}

          <ModePanel value={settings.mode} disabled={isBusy} saving={saveState === "saving"} onChange={updateMode} />

          <ProviderManagedPanel settings={settings} />

          <ProviderCards
            providers={settings.providers}
            availableProviderKinds={availableProviderKinds}
            kindMetadata={kindMetadata}
            activeProviderId={settings.activeProviderId ?? null}
            pendingProviderId={pendingProviderId}
            openProviderKey={openProviderKey}
            form={form}
            isBusy={isBusy}
            isSaving={saveState === "saving"}
            onEdit={editProvider}
            onConfigureKind={configureProviderKind}
            onFormChange={setForm}
            onSubmit={submitProvider}
            onEnabledChange={updateProviderEnabled}
            onSetActive={setActive}
            onCheck={checkProvider}
            onDelete={deleteProvider}
          />
        </>
      )}
    </div>
  )
}

function ModePanel({
  value,
  disabled,
  saving,
  onChange,
}: {
  value: AutonomousWebSearchModeDto
  disabled: boolean
  saving: boolean
  onChange: (value: AutonomousWebSearchModeDto) => void
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">Mode</h4>
        {saving ? (
          <span className="inline-flex items-center gap-1.5 text-[12px] text-muted-foreground">
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            Saving
          </span>
        ) : null}
      </div>
      <RadioGroup
        value={value}
        disabled={disabled}
        onValueChange={(next) => onChange(next as AutonomousWebSearchModeDto)}
        className="grid gap-2 sm:grid-cols-2"
      >
        {MODE_OPTIONS.map((option) => (
          <label
            key={option.value}
            htmlFor={`web-search-mode-${option.value}`}
            className={cn(
              "flex cursor-pointer items-start gap-3 rounded-lg border px-3.5 py-3 transition-colors",
              option.value === value ? "border-primary/45 bg-primary/5" : "border-border/70 bg-background hover:bg-accent/30",
              disabled && "cursor-default opacity-70",
            )}
          >
            <RadioGroupItem id={`web-search-mode-${option.value}`} value={option.value} className="mt-1" />
            <span className="min-w-0">
              <span className="block text-[13px] font-semibold text-foreground">{option.label}</span>
              <span className="mt-1 block text-[12px] leading-[1.45] text-muted-foreground">{option.summary}</span>
            </span>
          </label>
        ))}
      </RadioGroup>
    </section>
  )
}

function ProviderManagedPanel({ settings }: { settings: AutonomousWebSearchSettingsDto }) {
  return (
    <section className="flex items-center justify-between gap-4 rounded-lg border border-border/60 bg-secondary/10 px-4 py-3.5">
      <div className="min-w-0">
        <h4 className="text-[13px] font-semibold tracking-tight text-foreground">Provider-managed search</h4>
        <p className="mt-1 text-[12.5px] leading-[1.5] text-muted-foreground">{settings.providerManaged.message}</p>
      </div>
      <Badge variant="outline" className="shrink-0 text-[11px]">
        {formatStatus(settings.providerManaged.status)}
      </Badge>
    </section>
  )
}

function ProviderCards({
  providers,
  availableProviderKinds,
  kindMetadata,
  activeProviderId,
  pendingProviderId,
  openProviderKey,
  form,
  isBusy,
  isSaving,
  onEdit,
  onConfigureKind,
  onFormChange,
  onSubmit,
  onEnabledChange,
  onSetActive,
  onCheck,
  onDelete,
}: {
  providers: AutonomousWebSearchProviderProfileDto[]
  availableProviderKinds: AutonomousWebSearchProviderKindMetadataDto[]
  kindMetadata: Map<AutonomousWebSearchProviderKindDto, AutonomousWebSearchProviderKindMetadataDto>
  activeProviderId: string | null
  pendingProviderId: string | null
  openProviderKey: string | null
  form: ProviderFormState
  isBusy: boolean
  isSaving: boolean
  onEdit: (provider: AutonomousWebSearchProviderProfileDto) => void
  onConfigureKind: (metadata: AutonomousWebSearchProviderKindMetadataDto) => void
  onFormChange: Dispatch<SetStateAction<ProviderFormState>>
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onEnabledChange: (provider: AutonomousWebSearchProviderProfileDto, enabled: boolean) => void
  onSetActive: (providerId: string) => void
  onCheck: (providerId: string) => void
  onDelete: (providerId: string) => void
}) {
  return (
    <div className="flex flex-col gap-6">
      {providers.length > 0 ? (
        <ProviderGroup title="Configured" count={providers.length}>
          {providers.map((provider) => {
            const metadata = kindMetadata.get(provider.kind)
            const isOpen = openProviderKey === providerProfileKey(provider.profileId)
            return (
              <ConfiguredProviderCard
                key={provider.profileId}
                provider={provider}
                metadata={metadata}
                active={provider.profileId === activeProviderId}
                pending={pendingProviderId === provider.profileId}
                isOpen={isOpen}
                form={form}
                isBusy={isBusy}
                isSaving={isSaving}
                onEdit={onEdit}
                onFormChange={onFormChange}
                onSubmit={onSubmit}
                onEnabledChange={onEnabledChange}
                onSetActive={onSetActive}
                onCheck={onCheck}
                onDelete={onDelete}
              />
            )
          })}
        </ProviderGroup>
      ) : null}

      <ProviderGroup
        title={providers.length > 0 ? "Available" : "All providers"}
        count={availableProviderKinds.length}
      >
        {availableProviderKinds.map((metadata) => (
          <AvailableProviderCard
            key={metadata.kind}
            metadata={metadata}
            isOpen={openProviderKey === providerKindKey(metadata.kind)}
            form={form}
            pending={pendingProviderId === metadata.kind}
            isBusy={isBusy}
            isSaving={isSaving}
            onConfigure={onConfigureKind}
            onFormChange={onFormChange}
            onSubmit={onSubmit}
          />
        ))}
      </ProviderGroup>
    </div>
  )
}

function ConfiguredProviderCard({
  provider,
  metadata,
  active,
  pending,
  isOpen,
  form,
  isBusy,
  isSaving,
  onEdit,
  onFormChange,
  onSubmit,
  onEnabledChange,
  onSetActive,
  onCheck,
  onDelete,
}: {
  provider: AutonomousWebSearchProviderProfileDto
  metadata?: AutonomousWebSearchProviderKindMetadataDto
  active: boolean
  pending: boolean
  isOpen: boolean
  form: ProviderFormState
  isBusy: boolean
  isSaving: boolean
  onEdit: (provider: AutonomousWebSearchProviderProfileDto) => void
  onFormChange: Dispatch<SetStateAction<ProviderFormState>>
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onEnabledChange: (provider: AutonomousWebSearchProviderProfileDto, enabled: boolean) => void
  onSetActive: (providerId: string) => void
  onCheck: (providerId: string) => void
  onDelete: (providerId: string) => void
}) {
  const Icon = PROVIDER_ICON_BY_KIND[provider.kind]
  const status = configuredProviderStatus(provider, active)
  const detail = providerDetail(provider, metadata)
  const providerLabel = metadata?.label ?? provider.displayName

  return (
    <div
      className={cn(
        "rounded-lg border bg-card/40 transition-colors",
        isOpen ? "border-border" : "border-border/60 hover:border-border",
      )}
    >
      <div className="flex items-center gap-3 px-3.5 py-2.5">
        <ProviderIcon icon={Icon} />

        <div className="flex min-w-0 flex-1 items-center gap-3">
          <span className="truncate text-[13px] font-medium text-foreground">{providerLabel}</span>
          <span className="hidden min-w-0 items-center gap-1.5 truncate text-[12px] text-muted-foreground sm:flex">
            <span className={cn("size-1.5 shrink-0 rounded-full", status.dotClassName)} aria-hidden />
            <span className="shrink-0 text-foreground/80">{status.label}</span>
            {detail ? <span className="truncate text-muted-foreground/70">- {detail}</span> : null}
          </span>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" /> : null}
          {!active && provider.readiness.ready ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 text-[12px]"
              disabled={pending}
              onClick={() => onSetActive(provider.profileId)}
            >
              Select
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 w-8 p-0"
            disabled={pending}
            aria-label={`Test ${providerLabel}`}
            onClick={() => onCheck(provider.profileId)}
          >
            <Search className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
            disabled={pending}
            aria-expanded={isOpen}
            onClick={() => onEdit(provider)}
          >
            Edit
            <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", isOpen && "rotate-180")} />
          </Button>
        </div>
      </div>

      {!provider.readiness.ready ? (
        <div className="border-t border-border/60 px-3.5 py-2 text-[12px] leading-[1.45] text-muted-foreground">
          {provider.readiness.message}
        </div>
      ) : null}

      {isOpen ? (
        <ProviderEditor
          form={form}
          metadata={metadata}
          provider={provider}
          isBusy={isBusy}
          isSaving={isSaving}
          onFormChange={onFormChange}
          onSubmit={onSubmit}
          footerLeft={
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-8 gap-1.5 text-[12px] text-destructive hover:bg-destructive/10 hover:text-destructive"
              disabled={isBusy}
              onClick={() => onDelete(provider.profileId)}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Remove
            </Button>
          }
          footerMiddle={
            <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
              <Switch
                checked={provider.enabled}
                disabled={isBusy}
                aria-label={`Enable ${providerLabel}`}
                onCheckedChange={(checked) => onEnabledChange(provider, checked)}
              />
              Enabled
            </div>
          }
        />
      ) : null}
    </div>
  )
}

function AvailableProviderCard({
  metadata,
  isOpen,
  form,
  pending,
  isBusy,
  isSaving,
  onConfigure,
  onFormChange,
  onSubmit,
}: {
  metadata: AutonomousWebSearchProviderKindMetadataDto
  isOpen: boolean
  form: ProviderFormState
  pending: boolean
  isBusy: boolean
  isSaving: boolean
  onConfigure: (metadata: AutonomousWebSearchProviderKindMetadataDto) => void
  onFormChange: Dispatch<SetStateAction<ProviderFormState>>
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
}) {
  const Icon = PROVIDER_ICON_BY_KIND[metadata.kind]

  return (
    <div
      className={cn(
        "rounded-lg border bg-card/40 transition-colors",
        isOpen ? "border-border" : "border-border/60 hover:border-border",
      )}
    >
      <div className="flex items-center gap-3 px-3.5 py-2.5">
        <ProviderIcon icon={Icon} />
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <span className="truncate text-[13px] font-medium text-foreground">{metadata.label}</span>
          <span className="hidden truncate text-[12px] text-muted-foreground/70 sm:inline">
            {providerKindDetail(metadata)}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" /> : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={isBusy && !isOpen}
            aria-expanded={isOpen}
            onClick={() => onConfigure(metadata)}
          >
            Configure
            <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", isOpen && "rotate-180")} />
          </Button>
        </div>
      </div>

      {isOpen ? (
        <ProviderEditor
          form={form}
          metadata={metadata}
          isBusy={isBusy}
          isSaving={isSaving}
          onFormChange={onFormChange}
          onSubmit={onSubmit}
        />
      ) : null}
    </div>
  )
}

function ProviderEditor({
  form,
  metadata,
  provider,
  isBusy,
  isSaving,
  footerLeft,
  footerMiddle,
  onFormChange,
  onSubmit,
}: {
  form: ProviderFormState
  metadata?: AutonomousWebSearchProviderKindMetadataDto
  provider?: AutonomousWebSearchProviderProfileDto
  isBusy: boolean
  isSaving: boolean
  footerLeft?: ReactNode
  footerMiddle?: ReactNode
  onFormChange: Dispatch<SetStateAction<ProviderFormState>>
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
}) {
  const shouldShowEndpoint = Boolean(metadata?.requiresEndpoint)
  const shouldShowSearchEngineId = Boolean(metadata?.requiresGoogleCseCx)
  const shouldShowApiKey = Boolean(metadata?.requiresApiKey || form.kind === "custom_endpoint" || form.kind === "searxng_json")
  const editorFieldCount = [shouldShowEndpoint, shouldShowSearchEngineId, shouldShowApiKey].filter(Boolean).length

  return (
    <form className="border-t border-border/60 px-3.5 py-3.5" onSubmit={onSubmit}>
      <div className="flex flex-col gap-3 md:flex-row md:items-end">
        <div className={cn("grid min-w-0 flex-1 gap-3", editorFieldCount > 1 && "sm:grid-cols-2")}>
          {shouldShowEndpoint ? (
            <Field label="Endpoint">
              <Input
                value={form.endpoint}
                disabled={isBusy}
                placeholder={form.kind === "searxng_json" ? "https://search.example.com/search" : "https://search.example.com/api"}
                className="h-9"
                onChange={(event) => onFormChange((current) => ({ ...current, endpoint: event.target.value }))}
              />
            </Field>
          ) : null}
          {shouldShowSearchEngineId ? (
            <Field label="Search engine id">
              <Input
                value={form.googleCseCx}
                disabled={isBusy}
                className="h-9"
                onChange={(event) => onFormChange((current) => ({ ...current, googleCseCx: event.target.value }))}
              />
            </Field>
          ) : null}
          {shouldShowApiKey ? (
            <Field label="API key">
              <Input
                value={form.apiKey}
                disabled={isBusy}
                type="password"
                autoComplete="off"
                placeholder={provider?.hasApiKey ? "Leave empty to keep current key" : "Paste your API key"}
                className="h-9"
                onChange={(event) => onFormChange((current) => ({ ...current, apiKey: event.target.value }))}
              />
            </Field>
          ) : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 md:self-end">
          {footerLeft}
          {footerMiddle}
          <Button type="submit" size="sm" className="h-8 gap-1.5 text-[12px]" disabled={isBusy}>
            {isSaving ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}
            Save
          </Button>
        </div>
      </div>
    </form>
  )
}

function ProviderGroup({
  title,
  count,
  children,
}: {
  title: string
  count: number
  children: ReactNode
}) {
  return (
    <section className="flex flex-col gap-2">
      <header className="flex items-baseline gap-2 px-1">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          {title}
        </h4>
        <span className="text-[11px] tabular-nums text-muted-foreground/60">{count}</span>
      </header>
      {count > 0 ? (
        <div className="flex flex-col gap-1.5">{children}</div>
      ) : (
        <div className="rounded-lg border border-dashed border-border/60 bg-card/30 px-3.5 py-3 text-[12.5px] text-muted-foreground">
          No providers available.
        </div>
      )}
    </section>
  )
}

function ProviderIcon({ icon: Icon }: { icon: ElementType }) {
  return (
    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60">
      <Icon className="h-4 w-4" />
    </div>
  )
}

function Field({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label className="text-[12.5px] font-medium text-foreground">{label}</Label>
      {children}
    </div>
  )
}

function UnavailableCard() {
  return (
    <div className="rounded-lg border border-border/60 bg-secondary/10 px-4 py-5 text-[12.5px] leading-[1.5] text-muted-foreground">
      Web Search settings are available in the desktop app.
    </div>
  )
}

function providerProfileKey(profileId: string): string {
  return `profile:${profileId}`
}

function providerKindKey(kind: AutonomousWebSearchProviderKindDto): string {
  return `kind:${kind}`
}

function configuredProviderStatus(
  provider: AutonomousWebSearchProviderProfileDto,
  active: boolean,
): { label: string; dotClassName: string } {
  if (!provider.enabled) {
    return { label: "Disabled", dotClassName: "bg-muted-foreground/40" }
  }
  if (active) {
    return { label: "Active", dotClassName: "bg-success dark:bg-success" }
  }
  if (provider.readiness.ready) {
    return { label: "Ready", dotClassName: "bg-success dark:bg-success" }
  }
  return { label: formatStatus(provider.readiness.status), dotClassName: "bg-warning dark:bg-warning" }
}

function providerDetail(
  provider: AutonomousWebSearchProviderProfileDto,
  metadata?: AutonomousWebSearchProviderKindMetadataDto,
): string | null {
  if (provider.lastCheck) {
    return `last test ${provider.lastCheck.sampleResultCount} results, ${provider.lastCheck.latencyMs}ms`
  }
  const endpoint = provider.endpoint ?? provider.baseUrl
  if (endpoint) return endpoint.replace(/^https?:\/\//, "")
  return metadata?.label ?? formatStatus(provider.kind)
}

function providerKindDetail(metadata: AutonomousWebSearchProviderKindMetadataDto): string {
  const parts: string[] = []
  if (metadata.requiresApiKey) parts.push("API key")
  if (metadata.requiresEndpoint) parts.push("endpoint")
  if (metadata.requiresGoogleCseCx) parts.push("search engine id")
  if (metadata.selfHosted) parts.push("self-hosted")
  return parts.length > 0 ? parts.join(" - ") : "no key required"
}

function formatStatus(status: string): string {
  return status.split("_").join(" ")
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = String((error as { message?: unknown }).message ?? "").trim()
    if (message) return message
  }
  return fallback
}
