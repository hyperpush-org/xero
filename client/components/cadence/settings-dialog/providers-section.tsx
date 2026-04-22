import type {
  AgentPaneView,
  OperatorActionErrorView,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
import { ProviderProfileForm } from "@/components/cadence/provider-profiles/provider-profile-form"

export interface ProvidersSectionProps {
  agent: AgentPaneView | null
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
}

export function ProvidersSection({
  agent,
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  onRefreshProviderProfiles,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  onStartLogin,
  onLogout,
}: ProvidersSectionProps) {
  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-[13px] font-semibold text-foreground">Providers</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Manage app-local provider profiles, readiness, and active selection. Projects are not assigned to a provider here.
        </p>
      </div>

      <ProviderProfileForm
        providerProfiles={providerProfiles}
        providerProfilesLoadStatus={providerProfilesLoadStatus}
        providerProfilesLoadError={providerProfilesLoadError}
        providerProfilesSaveStatus={providerProfilesSaveStatus}
        providerProfilesSaveError={providerProfilesSaveError}
        onRefreshProviderProfiles={onRefreshProviderProfiles}
        onUpsertProviderProfile={onUpsertProviderProfile}
        onSetActiveProviderProfile={onSetActiveProviderProfile}
        runtimeSession={agent?.runtimeSession ?? null}
        hasSelectedProject={Boolean(agent?.repositoryPath?.trim())}
        onStartLogin={onStartLogin}
        onLogout={onLogout}
      />
    </div>
  )
}
