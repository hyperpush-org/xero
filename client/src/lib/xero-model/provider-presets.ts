import type { RuntimeProviderIdDto } from './runtime'

export type ProviderBaseUrlMode = 'none' | 'optional' | 'required'
export type ProviderApiVersionMode = 'none' | 'optional' | 'required'
export type ProviderRegionMode = 'none' | 'required'
export type ProviderProjectIdMode = 'none' | 'required'
export type ProviderAuthMode = 'oauth' | 'api_key' | 'local' | 'ambient'

// Runtime kinds and preset ids used to be derived from ProviderProfileDto.
// After Phase 4 (provider profiles deletion) they are inlined as closed
// string unions to keep CloudProviderPreset self-contained.
export type ProviderRuntimeKind =
  | 'openai_codex'
  | 'openrouter'
  | 'anthropic'
  | 'openai_compatible'
  | 'deepseek'
  | 'gemini'
export type ProviderPresetId =
  | 'openrouter'
  | 'anthropic'
  | 'github_models'
  | 'openai_api'
  | 'deepseek'
  | 'ollama'
  | 'azure_openai'
  | 'gemini_ai_studio'
  | 'bedrock'
  | 'vertex'

export interface CloudProviderPreset {
  providerId: RuntimeProviderIdDto
  runtimeKind: ProviderRuntimeKind
  label: string
  description: string
  defaultProfileId: string
  defaultProfileLabel: string
  defaultModelId: string
  presetId: ProviderPresetId | null
  authMode: ProviderAuthMode
  baseUrlMode: ProviderBaseUrlMode
  apiVersionMode: ProviderApiVersionMode
  regionMode: ProviderRegionMode
  projectIdMode: ProviderProjectIdMode
  manualModelAllowed: boolean
  supportsCatalogRefresh: boolean
  connectionHint: string
}

const CLOUD_PROVIDER_PRESETS: CloudProviderPreset[] = [
  {
    providerId: 'openai_codex',
    runtimeKind: 'openai_codex',
    label: 'OpenAI Codex',
    description: 'Xero-managed OpenAI OAuth profile used for desktop-authenticated runtime binds.',
    defaultProfileId: 'openai_codex-default',
    defaultProfileLabel: 'OpenAI Codex',
    defaultModelId: 'gpt-5.4',
    presetId: null,
    authMode: 'oauth',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: false,
    supportsCatalogRefresh: false,
    connectionHint: 'Browser sign-in happens when you bind a runtime session.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses the built-in OpenRouter endpoint preset.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses the built-in Anthropic endpoint preset.',
  },
  {
    providerId: 'deepseek',
    runtimeKind: 'deepseek',
    label: 'DeepSeek',
    description: 'Use DeepSeek-hosted V4 models with a saved app-local API key and live model discovery.',
    defaultProfileId: 'deepseek-default',
    defaultProfileLabel: 'DeepSeek',
    defaultModelId: 'deepseek-v4-pro',
    presetId: 'deepseek',
    authMode: 'api_key',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses the built-in DeepSeek endpoint preset.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses the built-in GitHub Models inference endpoint with an app-local token. GitHub device-flow onboarding is intentionally not enabled in Xero yet.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Leave base URL blank to use the default OpenAI API endpoint, or supply a custom OpenAI-compatible endpoint.',
  },
  {
    providerId: 'ollama',
    runtimeKind: 'openai_compatible',
    label: 'Ollama',
    description: 'Use a local Ollama endpoint without storing a fake app-local API key.',
    defaultProfileId: 'ollama-default',
    defaultProfileLabel: 'Ollama',
    defaultModelId: 'llama3.2',
    presetId: 'ollama',
    authMode: 'local',
    baseUrlMode: 'optional',
    apiVersionMode: 'none',
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Leave base URL blank to use the default Ollama endpoint at http://127.0.0.1:11434/v1.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Provide the Azure deployment base URL and required api-version value.',
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
    regionMode: 'none',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses the built-in Gemini AI Studio compatibility endpoint.',
  },
  {
    providerId: 'bedrock',
    runtimeKind: 'anthropic',
    label: 'Amazon Bedrock',
    description: 'Use Anthropic-family models via Amazon Bedrock with ambient AWS credentials and required region metadata.',
    defaultProfileId: 'bedrock-default',
    defaultProfileLabel: 'Amazon Bedrock',
    defaultModelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
    presetId: 'bedrock',
    authMode: 'ambient',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    regionMode: 'required',
    projectIdMode: 'none',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses ambient AWS credentials from the desktop host; region is required and no app-local API key is stored.',
  },
  {
    providerId: 'vertex',
    runtimeKind: 'anthropic',
    label: 'Google Vertex AI',
    description: 'Use Anthropic-family models via Vertex AI with ambient ADC credentials plus required region and project metadata.',
    defaultProfileId: 'vertex-default',
    defaultProfileLabel: 'Google Vertex AI',
    defaultModelId: 'claude-3-7-sonnet@20250219',
    presetId: 'vertex',
    authMode: 'ambient',
    baseUrlMode: 'none',
    apiVersionMode: 'none',
    regionMode: 'required',
    projectIdMode: 'required',
    manualModelAllowed: true,
    supportsCatalogRefresh: true,
    connectionHint: 'Uses ambient Google ADC credentials from the desktop host; region and project ID are required and no app-local API key is stored.',
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

export function getCloudProviderAuthMode(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): ProviderAuthMode | null {
  return getCloudProviderPreset(providerId)?.authMode ?? null
}

export function isApiKeyCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderAuthMode(providerId) === 'api_key'
}

export function usesOauthCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderAuthMode(providerId) === 'oauth'
}

export function isLocalCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderAuthMode(providerId) === 'local'
}

export function usesAmbientCloudProvider(
  providerId: RuntimeProviderIdDto | string | null | undefined,
): boolean {
  return getCloudProviderAuthMode(providerId) === 'ambient'
}

export interface ProviderConnectionLabelInput {
  providerId: RuntimeProviderIdDto | string
  baseUrl?: string | null
  region?: string | null
  projectId?: string | null
}

export function formatProviderConnectionLabel(
  profile: ProviderConnectionLabelInput,
): string {
  if (profile.baseUrl?.trim()) {
    return `Custom endpoint · ${profile.baseUrl.trim()}`
  }

  const metadataParts = [
    profile.region?.trim() ? `Region ${profile.region.trim()}` : null,
    profile.projectId?.trim() ? `Project ${profile.projectId.trim()}` : null,
  ].filter((value): value is string => Boolean(value))

  if (metadataParts.length > 0) {
    return metadataParts.join(' · ')
  }

  return getCloudProviderPreset(profile.providerId)?.connectionHint ?? getCloudProviderLabel(profile.providerId)
}

export function formatProviderEndpointLabel(
  profile: ProviderConnectionLabelInput,
): string {
  return formatProviderConnectionLabel(profile)
}
