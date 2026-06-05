import type {
  OperatorActionErrorView,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
} from "@/src/features/xero/use-xero-desktop-state"
import type {
  ProviderCredentialsSnapshotDto,
  ProviderAuthSessionView,
  RuntimeProviderIdDto,
  RuntimeSessionView,
  UpsertProviderCredentialRequestDto,
} from "@/src/lib/xero-model"
import { ProviderCredentialsList } from "@/components/xero/provider-profiles/provider-credentials-list"

interface ProvidersStepProps {
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  providerCredentialsLoadError: OperatorActionErrorView | null
  providerCredentialsSaveStatus: ProviderCredentialsSaveStatus
  providerCredentialsSaveError: OperatorActionErrorView | null
  runtimeSession?: RuntimeSessionView | null
  onRefreshProviderCredentials?: (options?: {
    force?: boolean
  }) => Promise<ProviderCredentialsSnapshotDto>
  onUpsertProviderCredential: (
    request: UpsertProviderCredentialRequestDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onDeleteProviderCredential?: (
    providerId: RuntimeProviderIdDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onStartOAuthLogin?: (request: {
    providerId: RuntimeProviderIdDto
    originator?: string | null
  }) => Promise<ProviderAuthSessionView | null>
}

export function ProvidersStep({
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
}: ProvidersStepProps) {
  return (
    <div>
      <StepHeader
        title="Configure providers"
        description="Add credentials for any providers you want to use. The model picker in the agent composer determines which credential is used for each turn — there is no global active provider."
      />

      <div className="mt-7 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
        <ProviderCredentialsList
          providerCredentials={providerCredentials}
          providerCredentialsLoadStatus={providerCredentialsLoadStatus}
          providerCredentialsLoadError={providerCredentialsLoadError}
          providerCredentialsSaveStatus={providerCredentialsSaveStatus}
          providerCredentialsSaveError={providerCredentialsSaveError}
          runtimeSession={runtimeSession}
          onRefreshProviderCredentials={onRefreshProviderCredentials}
          onUpsertProviderCredential={onUpsertProviderCredential}
          onDeleteProviderCredential={onDeleteProviderCredential}
          onStartOAuthLogin={onStartOAuthLogin}
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
