import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema, optionalIsoTimestampSchema } from './shared'

export const skillSourceKindSchema = z.enum([
  'bundled',
  'local',
  'project',
  'github',
  'dynamic',
  'mcp',
  'plugin',
])

export const skillSourceScopeSchema = z.enum(['global', 'project'])

export const skillSourceStateSchema = z.enum([
  'discoverable',
  'installed',
  'enabled',
  'disabled',
  'stale',
  'failed',
  'blocked',
])

export const skillTrustStateSchema = z.enum([
  'trusted',
  'user_approved',
  'approval_required',
  'untrusted',
  'blocked',
])

export const skillSourceMetadataSchema = z
  .object({
    label: z.string().trim().min(1),
    repo: nonEmptyOptionalTextSchema,
    reference: nonEmptyOptionalTextSchema,
    path: nonEmptyOptionalTextSchema,
    rootId: nonEmptyOptionalTextSchema,
    rootPath: nonEmptyOptionalTextSchema,
    relativePath: nonEmptyOptionalTextSchema,
    bundleId: nonEmptyOptionalTextSchema,
    pluginId: nonEmptyOptionalTextSchema,
    serverId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const installedSkillDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
    recordedAt: isoTimestampSchema,
  })
  .strict()

export const skillRegistryEntrySchema = z
  .object({
    sourceId: z.string().trim().min(1),
    skillId: z.string().trim().min(1),
    name: z.string().trim().min(1),
    description: z.string(),
    sourceKind: skillSourceKindSchema,
    scope: skillSourceScopeSchema,
    projectId: nonEmptyOptionalTextSchema,
    sourceState: skillSourceStateSchema,
    trustState: skillTrustStateSchema,
    enabled: z.boolean(),
    installed: z.boolean(),
    userInvocable: z.boolean().nullable().optional(),
    versionHash: nonEmptyOptionalTextSchema,
    lastUsedAt: optionalIsoTimestampSchema,
    lastDiagnostic: installedSkillDiagnosticSchema.nullable().optional(),
    source: skillSourceMetadataSchema,
  })
  .strict()

export const skillDiscoveryDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    relativePath: nonEmptyOptionalTextSchema,
  })
  .strict()

export const skillLocalRootSchema = z
  .object({
    rootId: z.string().trim().min(1),
    path: z.string().trim().min(1),
    enabled: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const skillGithubSourceSchema = z
  .object({
    repo: z.string().trim().min(1),
    reference: z.string().trim().min(1),
    root: z.string().trim().min(1),
    enabled: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const skillProjectSourceSchema = z
  .object({
    projectId: z.string().trim().min(1),
    enabled: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const skillSourceSettingsSchema = z
  .object({
    localRoots: z.array(skillLocalRootSchema).default([]),
    github: skillGithubSourceSchema,
    projects: z.array(skillProjectSourceSchema).default([]),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const skillRegistrySchema = z
  .object({
    projectId: nonEmptyOptionalTextSchema,
    entries: z.array(skillRegistryEntrySchema).default([]),
    sources: skillSourceSettingsSchema,
    diagnostics: z.array(skillDiscoveryDiagnosticSchema).default([]),
    reloadedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((registry, ctx) => {
    const seen = new Set<string>()
    registry.entries.forEach((entry, index) => {
      if (seen.has(entry.sourceId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['entries', index, 'sourceId'],
          message: `Skill registry cannot include duplicate source id \`${entry.sourceId}\`.`,
        })
      }
      seen.add(entry.sourceId)
    })
  })

export const listSkillRegistryRequestSchema = z
  .object({
    projectId: nonEmptyOptionalTextSchema,
    query: nonEmptyOptionalTextSchema,
    includeUnavailable: z.boolean().default(true),
  })
  .strict()

export const setSkillEnabledRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    sourceId: z.string().trim().min(1),
    enabled: z.boolean(),
  })
  .strict()

export const removeSkillRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    sourceId: z.string().trim().min(1),
  })
  .strict()

export const upsertSkillLocalRootRequestSchema = z
  .object({
    rootId: nonEmptyOptionalTextSchema,
    path: z.string().trim().min(1),
    enabled: z.boolean().default(true),
    projectId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const removeSkillLocalRootRequestSchema = z
  .object({
    rootId: z.string().trim().min(1),
    projectId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const updateProjectSkillSourceRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    enabled: z.boolean(),
  })
  .strict()

export const updateGithubSkillSourceRequestSchema = z
  .object({
    repo: z.string().trim().min(1),
    reference: z.string().trim().min(1),
    root: z.string().trim().min(1),
    enabled: z.boolean(),
    projectId: nonEmptyOptionalTextSchema,
  })
  .strict()

export type SkillSourceKindDto = z.infer<typeof skillSourceKindSchema>
export type SkillSourceScopeDto = z.infer<typeof skillSourceScopeSchema>
export type SkillSourceStateDto = z.infer<typeof skillSourceStateSchema>
export type SkillTrustStateDto = z.infer<typeof skillTrustStateSchema>
export type SkillSourceMetadataDto = z.infer<typeof skillSourceMetadataSchema>
export type InstalledSkillDiagnosticDto = z.infer<typeof installedSkillDiagnosticSchema>
export type SkillRegistryEntryDto = z.infer<typeof skillRegistryEntrySchema>
export type SkillDiscoveryDiagnosticDto = z.infer<typeof skillDiscoveryDiagnosticSchema>
export type SkillLocalRootDto = z.infer<typeof skillLocalRootSchema>
export type SkillGithubSourceDto = z.infer<typeof skillGithubSourceSchema>
export type SkillProjectSourceDto = z.infer<typeof skillProjectSourceSchema>
export type SkillSourceSettingsDto = z.infer<typeof skillSourceSettingsSchema>
export type SkillRegistryDto = z.infer<typeof skillRegistrySchema>
export type ListSkillRegistryRequestDto = z.infer<typeof listSkillRegistryRequestSchema>
export type SetSkillEnabledRequestDto = z.infer<typeof setSkillEnabledRequestSchema>
export type RemoveSkillRequestDto = z.infer<typeof removeSkillRequestSchema>
export type UpsertSkillLocalRootRequestDto = z.infer<typeof upsertSkillLocalRootRequestSchema>
export type RemoveSkillLocalRootRequestDto = z.infer<typeof removeSkillLocalRootRequestSchema>
export type UpdateProjectSkillSourceRequestDto = z.infer<typeof updateProjectSkillSourceRequestSchema>
export type UpdateGithubSkillSourceRequestDto = z.infer<typeof updateGithubSkillSourceRequestSchema>

export function getSkillSourceKindLabel(kind: SkillSourceKindDto): string {
  switch (kind) {
    case 'bundled':
      return 'Bundled'
    case 'local':
      return 'Local'
    case 'project':
      return 'Project'
    case 'github':
      return 'GitHub'
    case 'dynamic':
      return 'Dynamic'
    case 'mcp':
      return 'MCP'
    case 'plugin':
      return 'Plugin'
  }
}

export function getSkillSourceStateLabel(state: SkillSourceStateDto): string {
  switch (state) {
    case 'discoverable':
      return 'Discoverable'
    case 'installed':
      return 'Installed'
    case 'enabled':
      return 'Enabled'
    case 'disabled':
      return 'Disabled'
    case 'stale':
      return 'Stale'
    case 'failed':
      return 'Failed'
    case 'blocked':
      return 'Blocked'
  }
}

export function getSkillTrustStateLabel(state: SkillTrustStateDto): string {
  switch (state) {
    case 'trusted':
      return 'Trusted'
    case 'user_approved':
      return 'User approved'
    case 'approval_required':
      return 'Approval required'
    case 'untrusted':
      return 'Untrusted'
    case 'blocked':
      return 'Blocked'
  }
}
