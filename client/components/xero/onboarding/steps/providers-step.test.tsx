import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ProvidersStep } from '@/components/xero/onboarding/steps/providers-step'

// NOTE: Phase 3.5 of the provider-layer refactor replaced the legacy
// ProviderProfileForm-driven onboarding step with a credentials-list-driven
// step. The previous test suite exercised legacy UI affordances (active
// profile radio, mismatch banner, provider profile cards) that no longer
// exist. Phase 3.7 will write a new comprehensive suite for the rewritten
// component. The smoke test below confirms the new step renders so the
// build catches regressions in the wiring.

describe('ProvidersStep (credentials-list)', () => {
  it('renders the credentials list with the new step header copy', () => {
    render(
      <ProvidersStep
        providerCredentials={{ credentials: [] }}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        runtimeSession={null}
        onUpsertProviderCredential={vi.fn(async () => ({ credentials: [] }))}
      />,
    )

    expect(screen.getByText('Configure providers')).toBeInTheDocument()
    expect(
      screen.getByText(/model picker in the agent composer/i),
    ).toBeInTheDocument()
  })
})
