import { z } from 'zod'

const sameStringMembers = (left: string[], right: string[]) => {
  if (left.length !== right.length) return false
  const rightValues = new Set(right)
  return left.every((value) => rightValues.has(value))
}

export const toolExtensionEffectClassSchema = z.enum([
  'observe',
  'file_read',
  'search',
  'metadata',
  'retrieval',
  'diagnostics',
  'workspace_mutation',
  'app_state_mutation',
  'command_execution',
  'external_service',
  'browser_control',
  'device_control',
])
export type ToolExtensionEffectClassDto = z.infer<typeof toolExtensionEffectClassSchema>

export const toolExtensionMutabilitySchema = z.enum(['read_only', 'mutating'])
export type ToolExtensionMutabilityDto = z.infer<typeof toolExtensionMutabilitySchema>

export const toolExtensionSandboxRequirementSchema = z.enum([
  'none',
  'read_only',
  'workspace_write',
  'network',
  'full_local',
])
export type ToolExtensionSandboxRequirementDto = z.infer<
  typeof toolExtensionSandboxRequirementSchema
>

export const toolExtensionApprovalRequirementSchema = z.enum([
  'never',
  'policy',
  'always',
])
export type ToolExtensionApprovalRequirementDto = z.infer<
  typeof toolExtensionApprovalRequirementSchema
>

export const toolExtensionPermissionManifestSchema = z
  .object({
    permissionId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    effectClass: toolExtensionEffectClassSchema,
    riskClass: z.string().trim().min(1),
    auditLabel: z.string().trim().min(1),
  })
  .strict()
export type ToolExtensionPermissionManifestDto = z.infer<
  typeof toolExtensionPermissionManifestSchema
>

export const toolExtensionTestFixtureSchema = z
  .object({
    fixtureId: z.string().trim().min(1),
    input: z.record(z.string(), z.unknown()),
    expectedSummaryContains: z.string().trim().min(1).optional(),
  })
  .strict()
export type ToolExtensionTestFixtureDto = z.infer<typeof toolExtensionTestFixtureSchema>

export const toolExtensionManifestSchema = z
  .object({
    contractVersion: z.literal(1),
    extensionId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    inputSchema: z.record(z.string(), z.unknown()),
    permission: toolExtensionPermissionManifestSchema,
    mutability: toolExtensionMutabilitySchema,
    sandboxRequirement: toolExtensionSandboxRequirementSchema,
    approvalRequirement: toolExtensionApprovalRequirementSchema,
    capabilityTags: z.array(z.string().trim().min(1)).default([]),
    testFixtures: z.array(toolExtensionTestFixtureSchema).min(1),
  })
  .strict()
  .superRefine((manifest, ctx) => {
    const fixtureIds = new Set<string>()
    manifest.testFixtures.forEach((fixture, index) => {
      if (fixtureIds.has(fixture.fixtureId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['testFixtures', index, 'fixtureId'],
          message: 'Tool extension fixture ids must be unique.',
        })
      }
      fixtureIds.add(fixture.fixtureId)
    })
  })
export type ToolExtensionManifestDto = z.infer<typeof toolExtensionManifestSchema>

export const validateAgentToolExtensionManifestRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    manifest: z.unknown(),
  })
  .strict()
export type ValidateAgentToolExtensionManifestRequestDto = z.infer<
  typeof validateAgentToolExtensionManifestRequestSchema
>

export const agentToolExtensionDescriptorSchema = z
  .object({
    name: z.string().trim().min(1),
    description: z.string().trim().min(1),
    inputSchema: z.record(z.string(), z.unknown()),
    capabilityTags: z.array(z.string().trim().min(1)).default([]),
    effectClass: toolExtensionEffectClassSchema,
    mutability: toolExtensionMutabilitySchema,
    sandboxRequirement: toolExtensionSandboxRequirementSchema,
    approvalRequirement: toolExtensionApprovalRequirementSchema,
    telemetryAttributes: z.record(z.string(), z.string()),
    resultTruncation: z
      .object({
        maxOutputBytes: z.number().int().positive(),
        preserveJsonShape: z.boolean(),
      })
      .strict(),
  })
  .strict()
export type AgentToolExtensionDescriptorDto = z.infer<
  typeof agentToolExtensionDescriptorSchema
>

export const agentToolExtensionPermissionSummarySchema = z
  .object({
    permissionId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    effectClass: toolExtensionEffectClassSchema,
    riskClass: z.string().trim().min(1),
    auditLabel: z.string().trim().min(1),
    mutability: toolExtensionMutabilitySchema,
    sandboxRequirement: toolExtensionSandboxRequirementSchema,
    approvalRequirement: toolExtensionApprovalRequirementSchema,
    capabilityTags: z.array(z.string().trim().min(1)),
  })
  .strict()
export type AgentToolExtensionPermissionSummaryDto = z.infer<
  typeof agentToolExtensionPermissionSummarySchema
>

export const agentToolExtensionValidationDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()
export type AgentToolExtensionValidationDiagnosticDto = z.infer<
  typeof agentToolExtensionValidationDiagnosticSchema
>

const agentToolExtensionManifestValidationBaseSchema = z
  .object({
    schema: z.literal('xero.agent_tool_extension_manifest_validation.v1'),
    projectId: z.string().trim().min(1),
    fixtureCount: z.number().int().nonnegative(),
    fixtureIds: z.array(z.string().trim().min(1)),
    uiDeferred: z.literal(true),
  })
  .strict()

export const agentToolExtensionManifestValidationSchema = z
  .discriminatedUnion('valid', [
    agentToolExtensionManifestValidationBaseSchema
      .extend({
        valid: z.literal(true),
        extensionId: z.string().trim().min(1),
        toolName: z.string().trim().min(1),
        descriptor: agentToolExtensionDescriptorSchema,
        permission: agentToolExtensionPermissionSummarySchema,
        diagnostics: z.array(agentToolExtensionValidationDiagnosticSchema),
      })
      .strict(),
    agentToolExtensionManifestValidationBaseSchema
      .extend({
        valid: z.literal(false),
        extensionId: z.string().trim().min(1).nullable().optional(),
        toolName: z.string().trim().min(1).nullable().optional(),
        descriptor: agentToolExtensionDescriptorSchema.nullable().optional(),
        permission: agentToolExtensionPermissionSummarySchema.nullable().optional(),
        diagnostics: z.array(agentToolExtensionValidationDiagnosticSchema).min(1),
      })
      .strict(),
  ])
  .superRefine((report, ctx) => {
    if (report.fixtureCount !== report.fixtureIds.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fixtureCount'],
        message: 'Tool extension validation fixture count must match fixture ids.',
      })
    }
    if (!report.valid) return
    if (report.descriptor.name !== report.toolName) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'name'],
        message: 'Valid tool extension descriptor name must match toolName.',
      })
    }
    if (report.descriptor.telemetryAttributes['xero.extension.id'] !== report.extensionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'telemetryAttributes', 'xero.extension.id'],
        message: 'Valid tool extension telemetry must include the extension id.',
      })
    }
    if (
      report.descriptor.telemetryAttributes['xero.extension.permission_id'] !==
      report.permission.permissionId
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'telemetryAttributes', 'xero.extension.permission_id'],
        message: 'Valid tool extension telemetry must include the permission id.',
      })
    }
    if (report.descriptor.effectClass !== report.permission.effectClass) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'effectClass'],
        message: 'Valid tool extension descriptor effect class must match permission summary.',
      })
    }
    if (report.descriptor.mutability !== report.permission.mutability) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'mutability'],
        message: 'Valid tool extension descriptor mutability must match permission summary.',
      })
    }
    if (report.descriptor.sandboxRequirement !== report.permission.sandboxRequirement) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'sandboxRequirement'],
        message: 'Valid tool extension descriptor sandbox must match permission summary.',
      })
    }
    if (report.descriptor.approvalRequirement !== report.permission.approvalRequirement) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'approvalRequirement'],
        message: 'Valid tool extension descriptor approval policy must match permission summary.',
      })
    }
    if (!sameStringMembers(report.descriptor.capabilityTags, report.permission.capabilityTags)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['descriptor', 'capabilityTags'],
        message: 'Valid tool extension descriptor capabilities must match permission summary.',
      })
    }
  })
export type AgentToolExtensionManifestValidationDto = z.infer<
  typeof agentToolExtensionManifestValidationSchema
>
