import type { ProviderProfileDto } from './provider-profiles'
import type { RuntimeProviderIdDto } from './runtime'

export type ProviderBaseUrlMode = 'none' | 'optional' | 'required'
export type ProviderApiVersionMode = 'none' | 'optional' | 'required'
export type ProviderAuthMode = 'oauth' | 'api_key'

export interface CloudProviderPreset {
  providerId: RuntimeProviderIdDto
  runtimeKind: ProviderProfileDto['runtimeKind']
  label: string
  description: string
  defaultProfileId: string
  defaultProfileLabel: string
  defaultModelId: string
  presetId: ProviderProfileDto['presetId']
  authMode: ProviderAuthMode
  baseUrlMode: ProviderBaseUrlMode
  apiVersionMode: ProviderApiVersionMode
  manualModelAllowed: boolean
  supportsCatalogRefresh: boolean
  endpointHint: string
}

const CLOUD_PROVIDER_PRESETS: CloudProviderPreset[] = [
  {
    providerId: 'openai_codex',
    runtimeKind: 'openai_codex',
    label: 'OpenAI Codex',
    description: 'Cadence-managed OpenAI OAuth profile used for desktop-authenticated runtime binds.',
    defaultProfileId: 'openai_codex-default',
    defaultProfileLabel: 'OpenAI Codex',
    defaultModelId: 'openai_codex',
    presetId: null,
    authMode: 'oauth',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    manualModelAllowed: false,
    supportsCatalogRefresh: false,
    endpointHint: 'Browser sign-in happens when you bind a runtime session.',
  },
  {
    providerId: 'openrouter',
    runtimeKind: 'openrouter',
    label: 'OpenRouter',
    description: 'Use the hosted OpenRouter API with a saved app-local API key and live model discovery.',
    defaultProfileId: 'openrouter-default',
    defaultProfileLabel: 'OpenRouter',
    defaultModelId: 'openai/gpt-4.1-mini',
    presetId: 'openrouter',
    authMode: 'api_key',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Uses the built-in OpenRouter endpoint preset.',
  },
  {
    providerId: 'anthropic',
    runtimeKind: 'anthropic',
    label: 'Anthropic',
    description: 'Use Anthropic-hosted Claude models with a saved app-local API key and live model discovery.',
    defaultProfileId: 'anthropic-default',
    defaultProfileLabel: 'Anthropic',
    defaultModelId: 'claude-3-7-sonnet-latest',
    presetId: 'anthropic',
    authMode: 'api_key',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Uses the built-in Anthropic endpoint preset.',
  },
  {
    providerId: 'github_models',
    runtimeKind: 'openai_compatible',
    label: 'GitHub Models',
    description: 'Use GitHub Models-hosted inference with a saved app-local GitHub token and truthful catalog discovery.',
    defaultProfileId: 'github_models-default',
    defaultProfileLabel: 'GitHub Models',
    defaultModelId: 'openai/gpt-4.1',
    presetId: 'github_models',
    authMode: 'api_key',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Uses the built-in GitHub Models inference endpoint.',
  },
  {
    providerId: 'openai_api',
    runtimeKind: 'openai_compatible',
    label: 'OpenAI-compatible',
    description: 'Use the default OpenAI API endpoint or a custom OpenAI-compatible base URL with one shared API-key flow.',
    defaultProfileId: 'openai_api-default',
    defaultProfileLabel: 'OpenAI-compatible',
    defaultModelId: 'gpt-4.1-mini',
    presetId: 'openai_api',
    authMode: 'api_key',
    baseUrlMode: 'optional',
    apiVersionMode: 'optional',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Leave base URL blank to use the default OpenAI API endpoint, or supply a custom OpenAI-compatible endpoint.',
  },
  {
    providerId: 'azure_openai',
    runtimeKind: 'openai_compatible',
    label: 'Azure OpenAI',
    description: 'Use Azure OpenAI deployments with required base URL and API version metadata plus manual model truth.',
    defaultProfileId: 'azure_openai-default',
    defaultProfileLabel: 'Azure OpenAI',
    defaultModelId: 'gpt-4.1',
    presetId: 'azure_openai',
    authMode: 'api_key',
    baseUrlMode: 'required',
    apiVersionMode: 'required',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Provide the Azure deployment base URL and required api-version value.',
  },
  {
    providerId: 'gemini_ai_studio',
    runtimeKind: 'gemini',
    label: 'Gemini AI Studio',
    description: 'Use the Gemini AI Studio compatibility endpoint with a saved app-local API key and live model discovery.',
    defaultProfileId: 'gemini_ai_studio-default',
    defaultProfileLabel: 'Gemini AI Studio',
    defaultModelId: 'gemini-2.5-flash',
    presetId: 'gemini_ai_studio',
    authMode: 'api_key',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    endpointHint: 'Uses the built-in Gemini AI Studio compatibility endpoint.',
  },
]

const CLOUD_PROVIDER_PRESET_BY_ID = new Map(
  CLOUD_PROVIDER_PRESETS.map((preset) => [preset.providerId, preset]),
)

export function listCloudProviderPresets(): CloudProviderPreset[] {
  return CLOUD_PROVIDER_PRESETS
}

export function getCloudProviderPreset(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): CloudProviderPreset | null {
  if (typeof providerId !== 'string') {
    return null
  }

  return CLOUD_PROVIDER_PRESET_BY_ID.get(providerId as RuntimeProviderIdDto) ?? null
}

export function isKnownCloudProviderId(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): providerId is RuntimeProviderIdDto {
  return getCloudProviderPreset(providerId) !== null
}

export function getCloudProviderLabel(providerId: RuntimeProviderIdDto | string | null | undefined): string {
  return getCloudProviderPreset(providerId)?.label ?? 'Runtime provider'
}

export function getCloudProviderDefaultModelId(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): string {
  return getCloudProviderPreset(providerId)?.defaultModelId ?? ''
}

export function getCloudProviderDefaultProfileId(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): string | null {
  return getCloudProviderPreset(providerId)?.defaultProfileId ?? null
}

export function isApiKeyCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderPreset(providerId)?.authMode === 'api_key'
}

export function usesOauthCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderPreset(providerId)?.authMode === 'oauth'
}

export function formatProviderEndpointLabel(profile: Pick<ProviderProfileDto, 'providerId' | 'baseUrl'>): string {
  if (profile.baseUrl?.trim()) {
    return `Custom endpoint · ${profile.baseUrl.trim()}`
  }

  return getCloudProviderPreset(profile.providerId)?.endpointHint ?? getCloudProviderLabel(profile.providerId)
}
