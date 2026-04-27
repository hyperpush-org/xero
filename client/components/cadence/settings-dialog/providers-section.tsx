import type {
  AgentPaneView,
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  ProviderModelCatalogDto,
  ProviderProfileDiagnosticsDto,
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
  onStartLogin?: (options?: { profileId?: string | null }) => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onLogoutProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
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
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onCheckProviderProfile,
  onUpsertProviderProfile,
  onStartLogin,
  onLogout,
  onLogoutProviderProfile,
}: ProvidersSectionProps) {
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Providers"
        description="Configure provider credentials, endpoints, readiness checks, and catalog discovery."
      />

      <ProviderProfileForm
        providerProfiles={providerProfiles}
        providerProfilesLoadStatus={providerProfilesLoadStatus}
        providerProfilesLoadError={providerProfilesLoadError}
        providerProfilesSaveStatus={providerProfilesSaveStatus}
        providerProfilesSaveError={providerProfilesSaveError}
        providerModelCatalogs={providerModelCatalogs}
        providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
        onRefreshProviderProfiles={onRefreshProviderProfiles}
        onRefreshProviderModelCatalog={active ? onRefreshProviderModelCatalog : undefined}
        onCheckProviderProfile={active ? onCheckProviderProfile : undefined}
        onUpsertProviderProfile={onUpsertProviderProfile}
        runtimeSession={agent?.runtimeSession ?? null}
        hasSelectedProject={Boolean(agent?.repositoryPath?.trim())}
        onStartLogin={onStartLogin}
        onLogout={onLogout}
        onLogoutProviderProfile={onLogoutProviderProfile}
      />
    </div>
  )
}
