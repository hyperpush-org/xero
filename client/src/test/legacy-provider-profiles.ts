// Legacy provider-profile types kept around so the still-skipped legacy
// fixtures and `*.test.tsx` builders stay typeable. Production code does
// not use these — the runtime is now driven by the flat
// `provider_credentials` slice. Once the skipped tests are rewritten or
// deleted, this file can be removed.

import { z } from 'zod'
import type { RuntimeProviderIdDto } from '@/src/lib/cadence-model'

export type ProviderProfileReadinessStatusDto = 'ready' | 'missing' | 'malformed'
export type ProviderProfileReadinessProofDto =
  | 'oauth_session'
  | 'stored_secret'
  | 'local'
  | 'ambient'

export interface ProviderProfileReadinessDto {
  ready: boolean
  status: ProviderProfileReadinessStatusDto
  proof?: ProviderProfileReadinessProofDto | null
  proofUpdatedAt?: string | null
}

export interface ProviderProfileDto {
  profileId: string
  providerId: RuntimeProviderIdDto
  runtimeKind: 'openai_codex' | 'openrouter' | 'anthropic' | 'openai_compatible' | 'gemini'
  label: string
  modelId: string
  presetId?:
    | 'openrouter'
    | 'anthropic'
    | 'github_models'
    | 'openai_api'
    | 'ollama'
    | 'azure_openai'
    | 'gemini_ai_studio'
    | 'bedrock'
    | 'vertex'
    | null
  baseUrl?: string | null
  apiVersion?: string | null
  region?: string | null
  projectId?: string | null
  active: boolean
  readiness: ProviderProfileReadinessDto
  migratedFromLegacy: boolean
  migratedAt?: string | null
}

export interface ProviderProfilesMigrationDto {
  source: string
  migratedAt: string
  runtimeSettingsUpdatedAt?: string | null
  openrouterCredentialsUpdatedAt?: string | null
  openaiAuthUpdatedAt?: string | null
  openrouterModelInferred?: boolean | null
}

export interface ProviderProfilesDto {
  activeProfileId: string
  profiles: ProviderProfileDto[]
  migration?: ProviderProfilesMigrationDto | null
}

export interface UpsertProviderProfileRequestDto {
  profileId: string
  providerId: RuntimeProviderIdDto
  runtimeKind: ProviderProfileDto['runtimeKind']
  label: string
  modelId: string
  presetId?: ProviderProfileDto['presetId']
  baseUrl?: string | null
  apiVersion?: string | null
  region?: string | null
  projectId?: string | null
  apiKey?: string | null
  activate?: boolean
}

export const providerProfileReadinessSchema = z.unknown() as unknown as z.ZodType<
  ProviderProfileReadinessDto
>
export const providerProfileSchema = z.unknown() as unknown as z.ZodType<ProviderProfileDto>
export const providerProfilesSchema = z.unknown() as unknown as z.ZodType<ProviderProfilesDto>
export const upsertProviderProfileRequestSchema = z.unknown() as unknown as z.ZodType<
  UpsertProviderProfileRequestDto
>
export const setActiveProviderProfileRequestSchema = z.unknown() as unknown as z.ZodType<{
  profileId: string
}>
export const logoutProviderProfileRequestSchema = z.unknown() as unknown as z.ZodType<{
  profileId: string
}>
export type SetActiveProviderProfileRequestDto = { profileId: string }
export type LogoutProviderProfileRequestDto = { profileId: string }

export function getActiveProviderProfile(
  providerProfiles: ProviderProfilesDto | null | undefined,
): ProviderProfileDto | null {
  if (!providerProfiles) return null
  return (
    providerProfiles.profiles.find(
      (profile) => profile.profileId === providerProfiles.activeProfileId,
    ) ?? null
  )
}

export function projectRuntimeSettingsFromProviderProfiles(
  _providerProfiles: ProviderProfilesDto | null | undefined,
): null {
  return null
}
