import type {
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

interface ProvidersStepProps {
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
  runtimeSession?: RuntimeSessionView | null
  hasSelectedProject?: boolean
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onUpsertProviderProfile: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile: (profileId: string) => Promise<ProviderProfilesDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
}

export function ProvidersStep({
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
  runtimeSession = null,
  hasSelectedProject = false,
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  onStartLogin,
  onLogout,
}: ProvidersStepProps) {
  return (
    <div>
      <StepHeader
        title="Configure providers"
        description="Provider setup is app-wide. Choose the active profile for new runtime binds without rewriting project runtime history."
      />

      <div className="mt-7 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
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
          onRefreshProviderModelCatalog={onRefreshProviderModelCatalog}
          onUpsertProviderProfile={onUpsertProviderProfile}
          onSetActiveProviderProfile={onSetActiveProviderProfile}
          runtimeSession={runtimeSession}
          hasSelectedProject={hasSelectedProject}
          onStartLogin={onStartLogin}
          onLogout={onLogout}
          openAiMissingProjectLabel="Choose a project next"
        />
      </div>
    </div>
  )
}

interface StepHeaderProps {
  title: string
  description: string
}

export function StepHeader({ title, description }: StepHeaderProps) {
  return (
    <div>
      <h2 className="text-2xl font-semibold tracking-tight text-foreground">{title}</h2>
      <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">{description}</p>
    </div>
  )
}
