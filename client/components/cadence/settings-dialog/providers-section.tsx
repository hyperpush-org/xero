import { KeyRound } from "lucide-react"
import type {
  AgentPaneView,
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  ProviderModelCatalogDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
import { ProviderProfileForm } from "@/components/cadence/provider-profiles/provider-profile-form"
import { SectionHeader } from "./section-header"

export interface ProvidersSectionProps {
  active?: boolean
  agent: AgentPaneView | null
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
}

export function ProvidersSection({
  active = true,
  agent,
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  onStartLogin,
  onLogout,
}: ProvidersSectionProps) {
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        icon={KeyRound}
        title="Providers"
        description="Pick a provider, manage its API key, and choose a model."
        scope="app-wide"
      />

      <ProviderProfileForm
        providerProfiles={providerProfiles}
        providerProfilesLoadStatus={providerProfilesLoadStatus}
        providerProfilesLoadError={providerProfilesLoadError}
        providerProfilesSaveStatus={providerProfilesSaveStatus}
        providerProfilesSaveError={providerProfilesSaveError}
        providerModelCatalogs={providerModelCatalogs}
        providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
        providerModelCatalogLoadErrors={providerModelCatalogLoadErrors}
        onRefreshProviderProfiles={onRefreshProviderProfiles}
        onRefreshProviderModelCatalog={active ? onRefreshProviderModelCatalog : undefined}
        onUpsertProviderProfile={onUpsertProviderProfile}
        onSetActiveProviderProfile={onSetActiveProviderProfile}
        runtimeSession={agent?.runtimeSession ?? null}
        hasSelectedProject={Boolean(agent?.repositoryPath?.trim())}
        onStartLogin={onStartLogin}
        onLogout={onLogout}
        openAiMissingProjectLabel="Open a project"
      />
    </div>
  )
}
