import type {
  AgentPaneView,
  OperatorActionErrorView,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
} from "@/src/features/xero/use-xero-desktop-state"
import type {
  ProviderCredentialsSnapshotDto,
  ProviderAuthSessionView,
  RuntimeProviderIdDto,
  UpsertProviderCredentialRequestDto,
} from "@/src/lib/xero-model"
import { ProviderCredentialsList } from "@/components/xero/provider-profiles/provider-credentials-list"
import { SectionHeader } from "./section-header"

export interface ProvidersSectionProps {
  active?: boolean
  agent: AgentPaneView | null
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  providerCredentialsLoadError: OperatorActionErrorView | null
  providerCredentialsSaveStatus: ProviderCredentialsSaveStatus
  providerCredentialsSaveError: OperatorActionErrorView | null
  onRefreshProviderCredentials?: (options?: {
    force?: boolean
  }) => Promise<ProviderCredentialsSnapshotDto>
  onUpsertProviderCredential?: (
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

export function ProvidersSection({
  active = true,
  agent,
  providerCredentials,
  providerCredentialsLoadStatus,
  providerCredentialsLoadError,
  providerCredentialsSaveStatus,
  providerCredentialsSaveError,
  onRefreshProviderCredentials,
  onUpsertProviderCredential,
  onDeleteProviderCredential,
  onStartOAuthLogin,
}: ProvidersSectionProps) {
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Providers"
        description="Configure provider credentials. Your active model in the agent composer determines which credential is used for each turn — there is no global active provider."
      />

      <ProviderCredentialsList
        providerCredentials={providerCredentials}
        providerCredentialsLoadStatus={providerCredentialsLoadStatus}
        providerCredentialsLoadError={providerCredentialsLoadError}
        providerCredentialsSaveStatus={providerCredentialsSaveStatus}
        providerCredentialsSaveError={providerCredentialsSaveError}
        runtimeSession={active ? agent?.runtimeSession ?? null : null}
        onRefreshProviderCredentials={onRefreshProviderCredentials}
        onUpsertProviderCredential={onUpsertProviderCredential}
        onDeleteProviderCredential={onDeleteProviderCredential}
        onStartOAuthLogin={onStartOAuthLogin}
      />
    </div>
  )
}
