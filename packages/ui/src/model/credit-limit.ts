// ---------------------------------------------------------------------------
// Credit / billing limit classification.
//
// When an agent run fails because the provider account is out of credits or has
// hit a spending limit (typically HTTP 402), the backend routes it to a
// dedicated `provider_credit_limit` diagnostic. Rather than surface that as a
// red "run failed" error, the UI presents a purpose-built billing card docked
// above the composer with "add credits" / "upgrade" links and a "switch model"
// action.
//
// This module is the single source of truth for (a) recognizing a credit-limit
// failure from a run diagnostic and (b) resolving the billing/upgrade links to
// offer. It is intentionally dependency-free so it can be shared by the
// transcript (to suppress the red error), the composer placeholder, and the
// docked card.
// ---------------------------------------------------------------------------

/** The dedicated diagnostic code the backend emits for credit/billing limits. */
export const PROVIDER_CREDIT_LIMIT_CODE = 'provider_credit_limit'

export interface CreditLimitBillingLink {
  /** Button label, e.g. "Add credits". */
  label: string
  /** Absolute https URL opened in the system browser. */
  url: string
}

export interface CreditLimitNoticeView {
  /** Short headline, e.g. "Out of credits". */
  title: string
  /** One-line explanation rendered in the card body. */
  description: string
  /** Human-friendly provider label, e.g. "xAI" (null if unknown). */
  providerLabel: string | null
  /** Human-friendly model label, e.g. "Grok 4.5" (null if unknown). */
  modelLabel: string | null
  /** Billing/upgrade links to surface as buttons (may be empty). */
  links: CreditLimitBillingLink[]
}

export interface CreditLimitFailureInput {
  /** The diagnostic code (`runtimeRun.lastError.code` / `lastErrorCode`). */
  code: string | null | undefined
  /** The diagnostic message (`runtimeRun.lastError.message`). */
  message: string | null | undefined
  /** The provider that failed (`runtimeRun.providerId`). */
  providerId?: string | null
  /** Pre-resolved human-friendly provider label (e.g. "xAI"). */
  providerLabel?: string | null
  /** Pre-resolved human-friendly model label (e.g. "Grok 4.5"). */
  modelLabel?: string | null
}

// Raw phrasing providers use when an account is out of credit. Kept in sync
// with the backend detector `provider_preflight_message_indicates_credit_limit`.
const CREDIT_SIGNAL_PATTERNS: readonly string[] = [
  'credit_limit',
  'out of credits',
  'insufficient credit',
  'insufficient_quota',
  'insufficient funds',
  'spending limit',
  'spending-limit',
  'spending_limit',
  'add credits',
  'personal-team-blocked',
  'http 402',
  'payment required',
  'need a grok subscription',
]

// Known provider billing/upgrade destinations, keyed by provider id. Preferred
// over URLs scraped from the error message because the labels are clearer.
const PROVIDER_BILLING_LINKS: Record<string, readonly CreditLimitBillingLink[]> = {
  xai: [
    { label: 'Add credits', url: 'https://grok.com/?_s=usage' },
    { label: 'Upgrade to SuperGrok', url: 'https://grok.com/supergrok' },
  ],
  openrouter: [{ label: 'Add credits', url: 'https://openrouter.ai/credits' }],
  anthropic: [
    { label: 'Manage billing', url: 'https://console.anthropic.com/settings/billing' },
  ],
  openai_api: [
    { label: 'Manage billing', url: 'https://platform.openai.com/account/billing/overview' },
  ],
  openai_codex: [
    { label: 'Manage billing', url: 'https://platform.openai.com/account/billing/overview' },
  ],
  deepseek: [{ label: 'Top up balance', url: 'https://platform.deepseek.com/top_up' }],
  gemini_ai_studio: [
    { label: 'Manage billing', url: 'https://aistudio.google.com/app/billing' },
  ],
}

/** True when the message names a credit/spending/billing limit. */
export function messageIndicatesCreditLimit(message: string | null | undefined): boolean {
  if (!message) return false
  const text = message.toLowerCase()
  return CREDIT_SIGNAL_PATTERNS.some((pattern) => text.includes(pattern))
}

/** True when a run diagnostic represents a provider credit/billing limit. */
export function isCreditLimitFailure(
  code: string | null | undefined,
  message: string | null | undefined,
): boolean {
  if (code === PROVIDER_CREDIT_LIMIT_CODE) return true
  return messageIndicatesCreditLimit(message)
}

// Pull absolute https URLs out of a raw provider error message, de-duplicated
// and stripped of trailing punctuation. Used as a fallback for providers with
// no static billing map entry.
function extractBillingLinksFromMessage(message: string | null | undefined): CreditLimitBillingLink[] {
  if (!message) return []
  const matches = message.match(/https?:\/\/[^\s"'<>)\]}]+/gi) ?? []
  const seen = new Set<string>()
  const links: CreditLimitBillingLink[] = []
  for (const raw of matches) {
    const url = raw.replace(/[.,;:]+$/, '')
    if (seen.has(url)) continue
    seen.add(url)
    links.push({ label: 'Open billing page', url })
    if (links.length >= 3) break
  }
  return links
}

function resolveBillingLinks(
  providerId: string | null | undefined,
  message: string | null | undefined,
): CreditLimitBillingLink[] {
  const known = providerId ? PROVIDER_BILLING_LINKS[providerId] : undefined
  if (known && known.length > 0) return known.map((link) => ({ ...link }))
  return extractBillingLinksFromMessage(message)
}

/**
 * Classify a run diagnostic as a credit/billing limit and produce the card copy
 * + billing links. Returns `null` when the diagnostic is not a credit limit.
 */
export function classifyCreditLimitFailure(
  input: CreditLimitFailureInput,
): CreditLimitNoticeView | null {
  if (!isCreditLimitFailure(input.code, input.message)) return null

  const providerLabel = input.providerLabel?.trim() || null
  const modelLabel = input.modelLabel?.trim() || null
  const account = providerLabel ? `Your ${providerLabel} account` : 'This provider account'

  return {
    title: 'Out of credits',
    description: `${account} is out of credits or has hit its spending limit. Add credits or switch to another model to keep going.`,
    providerLabel,
    modelLabel,
    links: resolveBillingLinks(input.providerId, input.message),
  }
}
