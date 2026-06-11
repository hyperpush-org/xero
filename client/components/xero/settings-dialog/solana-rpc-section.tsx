"use client"

import { invoke, isTauri } from "@tauri-apps/api/core"
import {
  CircleCheckBig,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  RadioTower,
  Save,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"

import { BaseDialog } from "@xero/ui/components/base-dialog"

import { Button } from "@/components/ui/button"
import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import type {
  ClusterKind,
  ProviderProfileUpsert,
  ProviderProfileView,
  ProviderProfilesResponse,
  SecretPlacement,
  SolanaProviderKind,
} from "@/src/features/solana/use-solana-workbench"
import {
  EmptyPanel,
  ErrorBanner,
  ListContainer,
  Pill,
  SubHeading,
  SuccessBanner,
} from "./_shared"
import { SectionHeader } from "./section-header"

type LoadState = "idle" | "loading" | "ready" | "error"
type MutationState = "idle" | "saving" | "selecting" | "deleting"

interface ProfileForm {
  id: string
  cluster: ClusterKind
  label: string
  provider: SolanaProviderKind
  rpcUrl: string
  websocketUrl: string
  customSecretPlacement: SecretPlacement
  apiKey: string
  hasExistingSecret: boolean
  allowPublicFallback: boolean
  enabled: boolean
}

const CLUSTER_OPTIONS: Array<{ value: ClusterKind; label: string }> = [
  { value: "localnet", label: "Localnet" },
  { value: "mainnet_fork", label: "Mainnet fork" },
  { value: "devnet", label: "Devnet" },
  { value: "mainnet", label: "Mainnet" },
]

const PROVIDER_OPTIONS: Array<{ value: SolanaProviderKind; label: string }> = [
  { value: "custom", label: "Custom" },
  { value: "helius", label: "Helius" },
  { value: "quick_node", label: "QuickNode" },
  { value: "alchemy", label: "Alchemy" },
  { value: "triton", label: "Triton" },
  { value: "chainstack", label: "Chainstack" },
  { value: "solana_public", label: "Solana public" },
  { value: "localnet", label: "Localnet" },
]

const SECRET_PLACEMENT_OPTIONS: Array<{ value: SecretPlacement; label: string }> = [
  { value: "none", label: "No key" },
  { value: "query_parameter", label: "Query parameter" },
  { value: "header", label: "Header" },
  { value: "embedded_url", label: "Embedded URL" },
]

const EMPTY_FORM: ProfileForm = {
  id: "",
  cluster: "localnet",
  label: "",
  provider: "custom",
  rpcUrl: "",
  websocketUrl: "",
  customSecretPlacement: "none",
  apiKey: "",
  hasExistingSecret: false,
  allowPublicFallback: true,
  enabled: true,
}

const PROVIDER_AUTH_DEFAULTS: Record<
  SolanaProviderKind,
  { placement: SecretPlacement; secretName: string | null; help: string }
> = {
  custom: {
    placement: "none",
    secretName: null,
    help: "Choose the authentication mode for this custom endpoint.",
  },
  helius: {
    placement: "query_parameter",
    secretName: "api-key",
    help: "Helius keys are added as an api-key query parameter.",
  },
  quick_node: {
    placement: "embedded_url",
    secretName: null,
    help: "QuickNode endpoints usually include the token in the endpoint URL.",
  },
  alchemy: {
    placement: "embedded_url",
    secretName: null,
    help: "Alchemy Solana endpoints usually include the key in the /v2 URL path.",
  },
  triton: {
    placement: "embedded_url",
    secretName: null,
    help: "Triton endpoints usually include the token in the endpoint URL.",
  },
  chainstack: {
    placement: "embedded_url",
    secretName: null,
    help: "Chainstack endpoints usually include access in the dedicated URL.",
  },
  solana_public: {
    placement: "none",
    secretName: null,
    help: "Public Solana RPC endpoints do not use an API key.",
  },
  localnet: {
    placement: "none",
    secretName: null,
    help: "Local validator endpoints do not use an API key.",
  },
}

export function SolanaRpcSection() {
  const [profilesResponse, setProfilesResponse] =
    useState<ProviderProfilesResponse | null>(null)
  const [loadState, setLoadState] = useState<LoadState>("idle")
  const [mutationState, setMutationState] = useState<MutationState>("idle")
  const [pendingProfileId, setPendingProfileId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [form, setForm] = useState<ProfileForm>(EMPTY_FORM)
  const [profileDialogOpen, setProfileDialogOpen] = useState(false)

  const profiles = profilesResponse?.profiles ?? []
  const busy = loadState === "loading" || mutationState !== "idle"

  const loadProfiles = useCallback(async () => {
    if (!isTauri()) {
      setProfilesResponse({ profiles: [], selectedProfileIds: {}, inventory: [] })
      setLoadState("ready")
      return
    }

    setLoadState("loading")
    setError(null)
    try {
      const response = await invoke<ProviderProfilesResponse>(
        "solana_provider_profiles_list",
      )
      setProfilesResponse(response)
      setLoadState("ready")
    } catch (loadError) {
      setLoadState("error")
      setError(getErrorMessage(loadError, "Xero could not load Solana RPC profiles."))
    }
  }, [])

  useEffect(() => {
    void loadProfiles()
  }, [loadProfiles])

  const groupedProfiles = useMemo(() => {
    const grouped = new Map<ClusterKind, ProviderProfileView[]>()
    for (const profile of profiles) {
      const list = grouped.get(profile.cluster) ?? []
      list.push(profile)
      grouped.set(profile.cluster, list)
    }

    return CLUSTER_OPTIONS.map((cluster) => ({
      cluster,
      profiles: (grouped.get(cluster.value) ?? []).sort(
        (a, b) =>
          Number(b.selected) - Number(a.selected) ||
          Number(b.enabled) - Number(a.enabled) ||
          a.label.localeCompare(b.label),
      ),
    }))
  }, [profiles])

  const selectedCount = profiles.filter((profile) => profile.selected).length
  const keyedCount = profiles.filter((profile) => profile.hasSecret).length
  const customCount = profiles.filter((profile) => !profile.managed).length

  const resetForm = useCallback(() => {
    setForm(EMPTY_FORM)
    setFormError(null)
    setError(null)
  }, [])

  const openNewProfile = useCallback(() => {
    resetForm()
    setSuccess(null)
    setProfileDialogOpen(true)
  }, [resetForm])

  const editProfile = useCallback((profile: ProviderProfileView) => {
    if (profile.managed) return
    setForm({
      id: profile.id,
      cluster: profile.cluster,
      label: profile.label,
      provider: profile.provider,
      rpcUrl: profile.rpcUrl,
      websocketUrl: profile.websocketUrl ?? "",
      customSecretPlacement: profile.provider === "custom" ? profile.secretPlacement : "query_parameter",
      apiKey: "",
      hasExistingSecret: profile.hasSecret,
      allowPublicFallback: profile.allowPublicFallback,
      enabled: profile.enabled,
    })
    setError(null)
    setFormError(null)
    setSuccess(null)
    setProfileDialogOpen(true)
  }, [])

  const handleProfileDialogOpenChange = useCallback(
    (open: boolean) => {
      if (mutationState === "saving") return
      setProfileDialogOpen(open)
      if (!open) {
        resetForm()
      }
    },
    [mutationState, resetForm],
  )

  const selectProfile = useCallback(
    async (profile: ProviderProfileView) => {
      if (!isTauri() || profile.selected || !profile.enabled) return
      setMutationState("selecting")
      setPendingProfileId(profile.id)
      setError(null)
      setSuccess(null)
      try {
        const response = await invoke<ProviderProfilesResponse>(
          "solana_provider_profile_select",
          { request: { cluster: profile.cluster, profileId: profile.id } },
        )
        setProfilesResponse(response)
        setSuccess(`${profile.label} is now selected for ${profile.cluster}.`)
      } catch (selectError) {
        setError(getErrorMessage(selectError, "Xero could not select that Solana RPC profile."))
      } finally {
        setMutationState("idle")
        setPendingProfileId(null)
      }
    },
    [],
  )

  const deleteProfile = useCallback(
    async (profile: ProviderProfileView) => {
      if (!isTauri() || profile.managed) return
      setMutationState("deleting")
      setPendingProfileId(profile.id)
      setError(null)
      setSuccess(null)
      try {
        const response = await invoke<ProviderProfilesResponse>(
          "solana_provider_profile_delete",
          { request: { profileId: profile.id } },
        )
        setProfilesResponse(response)
        if (form.id === profile.id) {
          setForm(EMPTY_FORM)
        }
        setSuccess(`${profile.label} was deleted.`)
      } catch (deleteError) {
        setError(getErrorMessage(deleteError, "Xero could not delete that Solana RPC profile."))
      } finally {
        setMutationState("idle")
        setPendingProfileId(null)
      }
    },
    [form.id],
  )

  const saveProfile = useCallback(async () => {
    const validationError = profileFormValidationMessage(form)
    if (validationError) {
      setFormError(validationError)
      setSuccess(null)
      return
    }

    const profile = buildProfileRequest(form)
    if (!profile.ok) {
      setFormError(profile.error)
      setSuccess(null)
      return
    }

    if (!isTauri()) return

    setMutationState("saving")
    setPendingProfileId(form.id.trim())
    setError(null)
    setFormError(null)
    setSuccess(null)
    try {
      const response = await invoke<ProviderProfilesResponse>(
        "solana_provider_profile_upsert",
        { request: { profile: profile.value } },
      )
      setProfilesResponse(response)
      setForm(EMPTY_FORM)
      setProfileDialogOpen(false)
      setSuccess(`${profile.value.label} was saved.`)
    } catch (saveError) {
      setFormError(getErrorMessage(saveError, "Xero could not save that Solana RPC profile."))
    } finally {
      setMutationState("idle")
      setPendingProfileId(null)
    }
  }, [form])

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Solana RPC"
        description="Manage per-cluster RPC provider profiles. API keys are stored in app-data and profile URLs are shown redacted."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={busy}
            onClick={() => void loadProfiles()}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loadState === "loading" && "animate-spin")} />
            Refresh
          </Button>
        }
      />

      {error ? <ErrorBanner message={error} /> : null}
      {success ? <SuccessBanner message={success} /> : null}

      <section className="flex min-w-0 flex-col gap-3">
        <div className="flex flex-wrap items-end justify-between gap-2">
          <div>
            <SubHeading count={profiles.length}>Provider profiles</SubHeading>
            <ProfileSummary
              selectedCount={selectedCount}
              keyedCount={keyedCount}
              customCount={customCount}
            />
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 shrink-0 gap-1.5 text-[12px]"
            disabled={busy}
            onClick={openNewProfile}
          >
            <Plus className="h-3.5 w-3.5" />
            New profile
          </Button>
        </div>

        {loadState === "loading" && profiles.length === 0 ? (
          <div
            role="status"
            className="flex min-h-[180px] items-center justify-center rounded-lg border border-border/60 bg-secondary/10 text-[12.5px] text-muted-foreground"
          >
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            Loading Solana RPC profiles...
          </div>
        ) : profiles.length === 0 ? (
          <EmptyPanel
            icon={<RadioTower className="h-5 w-5 text-muted-foreground" />}
            title="No RPC profiles"
            body="Add a provider profile for devnet, mainnet, localnet, or a forked mainnet endpoint."
          />
        ) : (
          <div className="flex flex-col gap-5">
            {groupedProfiles.map(({ cluster, profiles: clusterProfiles }) =>
              clusterProfiles.length > 0 ? (
                <div key={cluster.value} className="flex flex-col gap-2">
                  <div className="flex items-center justify-between">
                    <span className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                      {cluster.label}
                    </span>
                    <span className="font-mono text-[11px] tabular-nums text-muted-foreground/75">
                      {clusterProfiles.filter((profile) => profile.selected).length}/
                      {clusterProfiles.length}
                    </span>
                  </div>
                  <ListContainer className="bg-background">
                    {clusterProfiles.map((profile) => (
                      <ProfileRow
                        key={profile.id}
                        profile={profile}
                        pending={pendingProfileId === profile.id}
                        mutating={mutationState}
                        onEdit={editProfile}
                        onSelect={selectProfile}
                        onDelete={deleteProfile}
                      />
                    ))}
                  </ListContainer>
                </div>
              ) : null,
            )}
          </div>
        )}
      </section>

      <ProfileEditorDialog
        open={profileDialogOpen}
        form={form}
        error={formError}
        saving={mutationState === "saving"}
        onOpenChange={handleProfileDialogOpenChange}
        onChange={(nextForm) => {
          setForm(nextForm)
          setFormError(null)
        }}
        onSave={saveProfile}
      />
    </div>
  )
}

function ProfileSummary({
  selectedCount,
  keyedCount,
  customCount,
}: {
  selectedCount: number
  keyedCount: number
  customCount: number
}) {
  const parts = [
    `${selectedCount} selected`,
    `${keyedCount} with keys`,
    `${customCount} custom`,
  ]

  return (
    <p className="mt-1 text-[12.5px] leading-[1.5] text-muted-foreground">
      {parts.join(" / ")}
    </p>
  )
}

function ProfileRow({
  profile,
  pending,
  mutating,
  onEdit,
  onSelect,
  onDelete,
}: {
  profile: ProviderProfileView
  pending: boolean
  mutating: MutationState
  onEdit: (profile: ProviderProfileView) => void
  onSelect: (profile: ProviderProfileView) => void
  onDelete: (profile: ProviderProfileView) => void
}) {
  const actionBusy = pending && mutating !== "idle"

  return (
    <div className="flex min-w-0 items-start gap-4 px-4 py-3.5">
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">
            {profile.label}
          </span>
          {profile.selected ? <Pill tone="good">Selected</Pill> : null}
          {profile.hasSecret ? <Pill tone="info">Key</Pill> : null}
          {profile.managed ? <Pill tone="neutral">Built-in</Pill> : null}
          {!profile.enabled ? <Pill tone="warn">Disabled</Pill> : null}
        </div>
        <div className="mt-1 truncate font-mono text-[11.5px] text-muted-foreground">
          {profile.rpcUrl}
        </div>
        {profile.websocketUrl ? (
          <div className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground/80">
            {profile.websocketUrl}
          </div>
        ) : null}
        <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          <span>{providerLabel(profile.provider)}</span>
          <span aria-hidden="true">/</span>
          <span>{profile.id}</span>
          <span aria-hidden="true">/</span>
          <span>{secretPlacementLabel(profile.secretPlacement)}</span>
          {profile.allowPublicFallback ? (
            <>
              <span aria-hidden="true">/</span>
              <span>public fallback</span>
            </>
          ) : null}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        {!profile.managed ? (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            title="Edit provider profile"
            aria-label={`Edit ${profile.label}`}
            onClick={() => onEdit(profile)}
          >
            <Pencil className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        <Button
          type="button"
          size="icon-xs"
          variant={profile.selected ? "secondary" : "ghost"}
          title="Select provider profile"
          aria-label={`Select ${profile.label}`}
          disabled={profile.selected || !profile.enabled || actionBusy}
          onClick={() => onSelect(profile)}
        >
          {pending && mutating === "selecting" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <CircleCheckBig className="h-3.5 w-3.5" />
          )}
        </Button>
        {!profile.managed ? (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            title="Delete provider profile"
            aria-label={`Delete ${profile.label}`}
            disabled={actionBusy}
            onClick={() => onDelete(profile)}
          >
            {pending && mutating === "deleting" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Trash2 className="h-3.5 w-3.5" />
            )}
          </Button>
        ) : null}
      </div>
    </div>
  )
}

function ProfileEditorDialog({
  open,
  form,
  error,
  saving,
  onOpenChange,
  onChange,
  onSave,
}: {
  open: boolean
  form: ProfileForm
  error: string | null
  saving: boolean
  onOpenChange: (open: boolean) => void
  onChange: (form: ProfileForm) => void
  onSave: () => void
}) {
  const setField = <K extends keyof ProfileForm>(key: K, value: ProfileForm[K]) => {
    onChange({ ...form, [key]: value })
  }
  const authPlacement = resolveSecretPlacement(form)
  const apiKeyDisabled =
    authPlacement === "none" || authPlacement === "embedded_url"
  const rpcPlaceholder = rpcUrlPlaceholder(form.provider, form.cluster)
  const websocketPlaceholder = websocketUrlPlaceholder(form.provider, form.cluster)
  const validationMessage = profileFormValidationMessage(form)
  const saveDisabled = saving || validationMessage !== null
  const isEditing = form.id.trim().length > 0

  const setCluster = (cluster: ClusterKind) => {
    const nextLabel = shouldReplaceSuggestedLabel(form)
      ? suggestedProfileLabel(form.provider, cluster)
      : form.label
    onChange({ ...form, cluster, label: nextLabel })
  }

  const setProvider = (provider: SolanaProviderKind) => {
    const nextLabel = shouldReplaceSuggestedLabel(form)
      ? suggestedProfileLabel(provider, form.cluster)
      : form.label
    const nextPlacement =
      provider === "custom"
        ? form.customSecretPlacement
        : PROVIDER_AUTH_DEFAULTS[provider].placement

    onChange({
      ...form,
      provider,
      label: nextLabel,
      customSecretPlacement: nextPlacement,
      hasExistingSecret: provider === form.provider ? form.hasExistingSecret : false,
      apiKey:
        PROVIDER_AUTH_DEFAULTS[provider].placement === "none" ||
        PROVIDER_AUTH_DEFAULTS[provider].placement === "embedded_url"
          ? ""
          : form.apiKey,
    })
  }

  return (
    <BaseDialog
      open={open}
      onOpenChange={onOpenChange}
      variant="custom"
      title={isEditing ? "Edit Solana RPC profile" : "Add Solana RPC profile"}
      busy={saving}
      contentClassName="max-h-[min(760px,calc(100vh-4rem))] gap-0 overflow-hidden p-0 sm:max-w-[680px]"
      leading={
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-32 bg-gradient-to-b from-primary/[0.06] to-transparent"
        />
      }
      header={
        <div className="relative px-6 pb-2 pt-6">
          <DialogHeader className="space-y-2">
            <div className="flex items-center gap-2.5">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
                <RadioTower className="h-4 w-4" />
              </span>
              <DialogTitle className="text-[15px]">
                {isEditing ? "Edit Solana RPC profile" : "Add Solana RPC profile"}
              </DialogTitle>
            </div>
            <DialogDescription className="text-[12.5px] leading-relaxed">
              Save one profile per provider and cluster. Leave the API key blank
              when editing to keep the stored secret.
            </DialogDescription>
          </DialogHeader>
        </div>
      }
      bodyClassName="relative min-h-0 overflow-y-auto px-6 pb-5 pt-3"
      footerClassName="border-t border-border/60 bg-secondary/20 px-6 py-3"
      footer={
          <div className="ml-auto flex gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-9 text-[12.5px] text-muted-foreground hover:text-foreground"
              disabled={saving}
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              className="h-9 gap-1.5 text-[12.5px]"
              disabled={saveDisabled}
              title={validationMessage ?? undefined}
              onClick={() => void onSave()}
            >
              {saving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Save className="h-3.5 w-3.5" />
              )}
              Save profile
            </Button>
          </div>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-3 sm:grid-cols-3">
          <FormField label="Display name" htmlFor="solana-rpc-profile-label">
            <Input
              id="solana-rpc-profile-label"
              className="h-9 text-[12.5px]"
              value={form.label}
              onChange={(event) => setField("label", event.target.value)}
              placeholder={suggestedProfileLabel(form.provider, form.cluster)}
            />
          </FormField>
          <FormField label="Cluster" htmlFor="solana-rpc-profile-cluster">
            <Select
              value={form.cluster}
              onValueChange={(value) => setCluster(value as ClusterKind)}
            >
              <SelectTrigger id="solana-rpc-profile-cluster" className="h-9 w-full text-[12.5px]" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {CLUSTER_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </FormField>

          <FormField label="Provider" htmlFor="solana-rpc-profile-provider">
            <Select
              value={form.provider}
              onValueChange={(value) => setProvider(value as SolanaProviderKind)}
            >
              <SelectTrigger id="solana-rpc-profile-provider" className="h-9 w-full text-[12.5px]" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PROVIDER_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </FormField>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <FormField label="RPC URL" htmlFor="solana-rpc-profile-rpc-url">
            <Input
              id="solana-rpc-profile-rpc-url"
              className="h-9 font-mono text-[12.5px]"
              value={form.rpcUrl}
              onChange={(event) => setField("rpcUrl", event.target.value)}
              placeholder={rpcPlaceholder}
            />
          </FormField>

          <FormField label="WebSocket URL" htmlFor="solana-rpc-profile-ws-url">
            <Input
              id="solana-rpc-profile-ws-url"
              className="h-9 font-mono text-[12.5px]"
              value={form.websocketUrl}
              onChange={(event) => setField("websocketUrl", event.target.value)}
              placeholder={websocketPlaceholder}
            />
          </FormField>
        </div>

        <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(240px,0.8fr)]">
          {form.provider === "custom" ? (
            <FormField label="Authentication" htmlFor="solana-rpc-profile-authentication">
              <Select
                value={form.customSecretPlacement}
                onValueChange={(value) => setField("customSecretPlacement", value as SecretPlacement)}
              >
                <SelectTrigger id="solana-rpc-profile-authentication" className="h-9 w-full text-[12.5px]" size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SECRET_PLACEMENT_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </FormField>
          ) : (
            <div className="grid gap-1.5">
              <Label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Authentication
              </Label>
              <div className="flex min-h-9 items-center rounded-md border border-border/60 bg-secondary/10 px-3 text-[12.5px] text-muted-foreground">
                {secretPlacementLabel(authPlacement)}
              </div>
            </div>
          )}
          <FormField label="API key" htmlFor="solana-rpc-profile-api-key">
            <Input
              id="solana-rpc-profile-api-key"
              className="h-9 text-[12.5px]"
              value={form.apiKey}
              disabled={apiKeyDisabled}
              type="password"
              onChange={(event) => setField("apiKey", event.target.value)}
              placeholder={
                authPlacement === "none"
                  ? "Not needed"
                  : authPlacement === "embedded_url"
                    ? "Paste key in endpoint URL"
                    : "New key or blank to preserve"
              }
            />
          </FormField>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-border/50 pt-3">
          <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
            <SwitchRow
              id="solana-rpc-profile-enabled"
              label="Enabled"
              checked={form.enabled}
              onCheckedChange={(checked) => setField("enabled", checked)}
            />
            <SwitchRow
              id="solana-rpc-profile-fallback"
              label="Allow public fallback"
              checked={form.allowPublicFallback}
              onCheckedChange={(checked) => setField("allowPublicFallback", checked)}
            />
          </div>
          {validationMessage ? (
            <p className="ml-auto text-right text-[12px] leading-[1.5] text-muted-foreground">
              {validationMessage}
            </p>
          ) : null}
        </div>
      </div>

      {error ? (
        <ErrorBanner message={error} />
      ) : null}
    </BaseDialog>
  )
}

function FormField({
  label,
  htmlFor,
  children,
}: {
  label: string
  htmlFor: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-1.5">
      <Label
        htmlFor={htmlFor}
        className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground"
      >
        {label}
      </Label>
      {children}
    </div>
  )
}

function SwitchRow({
  id,
  label,
  checked,
  onCheckedChange,
}: {
  id: string
  label: string
  checked: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <div className="flex items-center gap-2.5">
      <Label htmlFor={id} className="text-[12.5px] font-medium text-foreground">
        {label}
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}

function buildProfileRequest(
  form: ProfileForm,
): { ok: true; value: ProviderProfileUpsert } | { ok: false; error: string } {
  const label = form.label.trim() || suggestedProfileLabel(form.provider, form.cluster)
  const id = form.id.trim() || buildProfileId(form.provider, form.cluster, label)
  const rpcUrl = form.rpcUrl.trim()
  const websocketUrl = form.websocketUrl.trim()
  const secretPlacement = resolveSecretPlacement(form)
  const secretName = resolveSecretName(form)

  if (rpcUrl.length === 0) return { ok: false, error: "RPC URL is required." }

  return {
    ok: true,
    value: {
      id,
      cluster: form.cluster,
      label,
      provider: form.provider,
      rpcUrl,
      websocketUrl: websocketUrl.length > 0 ? websocketUrl : null,
      secretPlacement,
      secretName,
      apiKey: form.apiKey.length > 0 ? form.apiKey : null,
      priority: 0,
      enabled: form.enabled,
      allowPublicFallback: form.allowPublicFallback,
    },
  }
}

function profileFormValidationMessage(form: ProfileForm): string | null {
  if (form.rpcUrl.trim().length === 0) {
    return "RPC URL is required."
  }

  const secretPlacement = resolveSecretPlacement(form)
  if (
    (secretPlacement === "query_parameter" || secretPlacement === "header") &&
    !form.hasExistingSecret &&
    form.apiKey.trim().length === 0
  ) {
    return `API key is required for ${providerLabel(form.provider)}.`
  }

  return null
}

function providerLabel(provider: SolanaProviderKind): string {
  return PROVIDER_OPTIONS.find((option) => option.value === provider)?.label ?? provider
}

function secretPlacementLabel(secretPlacement: SecretPlacement): string {
  return SECRET_PLACEMENT_OPTIONS.find((option) => option.value === secretPlacement)?.label ?? secretPlacement
}

function resolveSecretPlacement(form: ProfileForm): SecretPlacement {
  if (form.provider === "custom") return form.customSecretPlacement
  return PROVIDER_AUTH_DEFAULTS[form.provider].placement
}

function resolveSecretName(form: ProfileForm): string | null {
  const placement = resolveSecretPlacement(form)
  if (placement === "none" || placement === "embedded_url") return null
  if (form.provider === "custom") {
    return placement === "header" ? "Authorization" : "api-key"
  }
  return PROVIDER_AUTH_DEFAULTS[form.provider].secretName
}

function suggestedProfileLabel(provider: SolanaProviderKind, cluster: ClusterKind): string {
  if (provider === "custom") return `Custom ${clusterLabel(cluster).toLowerCase()}`
  if (provider === "localnet") return "Local validator"
  if (provider === "solana_public") return `Solana public ${clusterLabel(cluster).toLowerCase()}`
  return `${providerLabel(provider)} ${clusterLabel(cluster).toLowerCase()}`
}

function shouldReplaceSuggestedLabel(form: ProfileForm): boolean {
  const current = form.label.trim()
  if (current.length === 0) return true
  return PROVIDER_OPTIONS.some((provider) =>
    CLUSTER_OPTIONS.some(
      (cluster) => current === suggestedProfileLabel(provider.value, cluster.value),
    ),
  )
}

function buildProfileId(
  provider: SolanaProviderKind,
  cluster: ClusterKind,
  label: string,
): string {
  const base =
    provider === "custom"
      ? label
      : `${providerLabel(provider)}-${cluster}`
  return slugify(base)
}

function slugify(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
  return slug.length > 0 ? slug : "solana-rpc-profile"
}

function clusterLabel(cluster: ClusterKind): string {
  return CLUSTER_OPTIONS.find((option) => option.value === cluster)?.label ?? cluster
}

function rpcUrlPlaceholder(provider: SolanaProviderKind, cluster: ClusterKind): string {
  if (provider === "localnet" || cluster === "localnet") return "http://127.0.0.1:8899"
  if (provider === "helius") {
    return cluster === "devnet"
      ? "https://devnet.helius-rpc.com"
      : "https://mainnet.helius-rpc.com"
  }
  if (provider === "alchemy") {
    return cluster === "devnet"
      ? "https://solana-devnet.g.alchemy.com/v2/<key>"
      : "https://solana-mainnet.g.alchemy.com/v2/<key>"
  }
  if (provider === "solana_public") {
    return cluster === "devnet"
      ? "https://api.devnet.solana.com"
      : "https://api.mainnet-beta.solana.com"
  }
  return "https://rpc.example.com"
}

function websocketUrlPlaceholder(provider: SolanaProviderKind, cluster: ClusterKind): string {
  if (provider === "localnet" || cluster === "localnet") return "ws://127.0.0.1:8900"
  if (provider === "helius") {
    return cluster === "devnet"
      ? "wss://devnet.helius-rpc.com"
      : "wss://mainnet.helius-rpc.com"
  }
  return "wss://rpc.example.com"
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === "string" && message.length > 0) return message
  }
  if (typeof error === "string" && error.length > 0) return error
  return fallback
}
