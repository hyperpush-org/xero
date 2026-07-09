import { describe, expect, it } from 'vitest'

import {
  classifyCreditLimitFailure,
  isCreditLimitFailure,
  messageIndicatesCreditLimit,
  PROVIDER_CREDIT_LIMIT_CODE,
} from './credit-limit'

// The verbatim diagnostic message xAI/Grok produces on HTTP 402, as seen in the
// persisted run failure. The backend routes this to `provider_credit_limit`.
const GROK_402_MESSAGE =
  'Xero cannot start an owned agent turn with provider `xai` and model `grok-4.5` because provider preflight `provider_preflight_provider_error` failed: Provider preflight failed with credit_limit: Provider returned HTTP 402: {"code":"personal-team-blocked:spending-limit","error":"You have run out of credits or need a Grok subscription. Add credits at https://grok.com/?_s=usage or upgrade at https://grok.com/supergrok."}'

describe('messageIndicatesCreditLimit', () => {
  it('matches provider credit-limit phrasing', () => {
    expect(messageIndicatesCreditLimit(GROK_402_MESSAGE)).toBe(true)
    expect(messageIndicatesCreditLimit('You are out of credits')).toBe(true)
    expect(messageIndicatesCreditLimit('personal-team-blocked:spending-limit')).toBe(true)
    expect(messageIndicatesCreditLimit('Provider returned HTTP 402: Payment Required')).toBe(true)
  })

  it('does not match unrelated failures', () => {
    expect(messageIndicatesCreditLimit(null)).toBe(false)
    expect(messageIndicatesCreditLimit('Provider returned HTTP 401: unauthorized')).toBe(false)
    expect(messageIndicatesCreditLimit('model not found')).toBe(false)
  })
})

describe('isCreditLimitFailure', () => {
  it('is true for the dedicated backend code regardless of message', () => {
    expect(isCreditLimitFailure(PROVIDER_CREDIT_LIMIT_CODE, 'anything')).toBe(true)
  })

  it('falls back to message signals for the generic preflight code', () => {
    expect(isCreditLimitFailure('provider_preflight_blocked', GROK_402_MESSAGE)).toBe(true)
    expect(isCreditLimitFailure('provider_preflight_blocked', 'model unavailable')).toBe(false)
  })
})

describe('classifyCreditLimitFailure', () => {
  it('returns null for non-credit failures', () => {
    expect(
      classifyCreditLimitFailure({
        code: 'openai_codex_auth_failed',
        message: "Provider 'openai_codex' returned HTTP 401",
        providerId: 'openai_codex',
      }),
    ).toBeNull()
  })

  it('produces a card with provider billing links and labels', () => {
    const notice = classifyCreditLimitFailure({
      code: PROVIDER_CREDIT_LIMIT_CODE,
      message: GROK_402_MESSAGE,
      providerId: 'xai',
      providerLabel: 'xAI',
      modelLabel: 'Grok 4.5',
    })
    expect(notice).not.toBeNull()
    expect(notice?.title).toBe('Out of credits')
    expect(notice?.providerLabel).toBe('xAI')
    expect(notice?.modelLabel).toBe('Grok 4.5')
    expect(notice?.description).toContain('xAI')
    // xAI resolves to its known static billing destinations.
    expect(notice?.links).toEqual([
      { label: 'Add credits', url: 'https://grok.com/?_s=usage' },
      { label: 'Upgrade to SuperGrok', url: 'https://grok.com/supergrok' },
    ])
  })

  it('extracts URLs from the message for providers without a static map', () => {
    const notice = classifyCreditLimitFailure({
      code: PROVIDER_CREDIT_LIMIT_CODE,
      message:
        'You are out of credits. Add credits at https://example.com/billing to continue.',
      providerId: 'some_unknown_provider',
      providerLabel: 'Example',
    })
    expect(notice?.links).toEqual([
      { label: 'Open billing page', url: 'https://example.com/billing' },
    ])
  })
})
