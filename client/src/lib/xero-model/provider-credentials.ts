import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema } from './shared'
import { runtimeProviderIdSchema, type RuntimeProviderIdDto } from './runtime'

export const providerCredentialKindSchema = z.preprocess(
  (value) => (value === 'o_auth_session' ? 'oauth_session' : value),
  z.enum(['api_key', 'oauth_session', 'local', 'ambient']),
)

export const providerCredentialReadinessProofSchema = z.preprocess(
  (value) => (value === 'o_auth_session' ? 'oauth_session' : value),
  z.enum(['oauth_session', 'stored_secret', 'local', 'ambient']),
)

export const providerCredentialSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    kind: providerCredentialKindSchema,
    hasApiKey: z.boolean(),
    oauthAccountId: nonEmptyOptionalTextSchema,
    oauthSessionId: nonEmptyOptionalTextSchema,
    hasOauthAccessToken: z.boolean(),
    oauthExpiresAt: z.number().int().nullable().optional(),
    baseUrl: z.string().url().nullable().optional(),
    apiVersion: nonEmptyOptionalTextSchema,
    region: nonEmptyOptionalTextSchema,
    projectId: nonEmptyOptionalTextSchema,
    defaultModelId: nonEmptyOptionalTextSchema,
    readinessProof: providerCredentialReadinessProofSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const providerCredentialsSnapshotSchema = z
  .object({
    credentials: z.array(providerCredentialSchema),
  })
  .strict()
  .superRefine((snapshot, ctx) => {
    const seen = new Set<string>()
    for (const [index, credential] of snapshot.credentials.entries()) {
      if (seen.has(credential.providerId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', index, 'providerId'],
          message: `Provider credential entries must be unique per provider; saw duplicate providerId \`${credential.providerId}\`.`,
        })
      }
      seen.add(credential.providerId)
    }
  })

export const upsertProviderCredentialRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    kind: providerCredentialKindSchema,
    apiKey: z.string().nullable().optional(),
    baseUrl: z.string().url().nullable().optional(),
    apiVersion: z.string().trim().min(1).nullable().optional(),
    region: z.string().trim().min(1).nullable().optional(),
    projectId: z.string().trim().min(1).nullable().optional(),
    defaultModelId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    if (payload.providerId === 'openai_codex') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['providerId'],
        message:
          'Xero persists OpenAI Codex credentials through the OAuth login flow, not the credential upsert command.',
      })
    }

    if (payload.kind === 'oauth_session') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['kind'],
        message:
          'Xero persists OAuth credentials through the OAuth login flow, not the credential upsert command.',
      })
    }
  })

export const deleteProviderCredentialRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
  })
  .strict()

export const startOAuthLoginRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    originator: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const completeOAuthCallbackRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    flowId: z.string().trim().min(1),
    manualInput: z.string().nullable().optional(),
  })
  .strict()

export type ProviderCredentialKindDto = z.infer<typeof providerCredentialKindSchema>
export type ProviderCredentialReadinessProofDto = z.infer<typeof providerCredentialReadinessProofSchema>
export type ProviderCredentialDto = z.infer<typeof providerCredentialSchema>
export type ProviderCredentialsSnapshotDto = z.infer<typeof providerCredentialsSnapshotSchema>
export type UpsertProviderCredentialRequestDto = z.infer<typeof upsertProviderCredentialRequestSchema>
export type DeleteProviderCredentialRequestDto = z.infer<typeof deleteProviderCredentialRequestSchema>
export type StartOAuthLoginRequestDto = z.infer<typeof startOAuthLoginRequestSchema>
export type CompleteOAuthCallbackRequestDto = z.infer<typeof completeOAuthCallbackRequestSchema>

export function findProviderCredential(
  snapshot: ProviderCredentialsSnapshotDto | null | undefined,
  providerId: RuntimeProviderIdDto,
): ProviderCredentialDto | null {
  return snapshot?.credentials.find((credential) => credential.providerId === providerId) ?? null
}

export function hasReadyProviderCredential(
  snapshot: ProviderCredentialsSnapshotDto | null | undefined,
  providerId: RuntimeProviderIdDto,
): boolean {
  return findProviderCredential(snapshot, providerId) !== null
}
