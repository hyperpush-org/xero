import { z } from 'zod'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  optionalIsoTimestampSchema,
} from './shared'

export const mcpTransportKindSchema = z.enum(['stdio', 'http', 'sse'])

const mcpStdioTransportSchema = z
  .object({
    kind: z.literal('stdio'),
    command: z.string().trim().min(1),
    args: z.array(z.string().trim().min(1)).default([]),
  })
  .strict()

const mcpHttpTransportSchema = z
  .object({
    kind: z.literal('http'),
    url: z.string().trim().url(),
  })
  .strict()

const mcpSseTransportSchema = z
  .object({
    kind: z.literal('sse'),
    url: z.string().trim().url(),
  })
  .strict()

export const mcpTransportSchema = z.discriminatedUnion('kind', [
  mcpStdioTransportSchema,
  mcpHttpTransportSchema,
  mcpSseTransportSchema,
])

export const mcpConnectionStatusSchema = z.enum([
  'connected',
  'failed',
  'blocked',
  'misconfigured',
  'stale',
])

export const mcpConnectionDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()

export const mcpConnectionStateSchema = z
  .object({
    status: mcpConnectionStatusSchema,
    diagnostic: mcpConnectionDiagnosticSchema.nullable().optional(),
    lastCheckedAt: optionalIsoTimestampSchema,
    lastHealthyAt: optionalIsoTimestampSchema,
  })
  .strict()
  .superRefine((connection, ctx) => {
    if (connection.status === 'connected' && connection.diagnostic) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostic'],
        message: 'Connected MCP servers must not include failure diagnostics.',
      })
    }

    if (connection.status !== 'connected' && !connection.diagnostic) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostic'],
        message:
          'Non-connected MCP servers must include typed diagnostics for failure localization.',
      })
    }

    if (connection.lastHealthyAt && connection.lastCheckedAt) {
      const lastHealthyAt = Date.parse(connection.lastHealthyAt)
      const lastCheckedAt = Date.parse(connection.lastCheckedAt)
      if (Number.isFinite(lastHealthyAt) && Number.isFinite(lastCheckedAt) && lastHealthyAt > lastCheckedAt) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['lastHealthyAt'],
          message: 'lastHealthyAt cannot be newer than lastCheckedAt.',
        })
      }
    }
  })

export const mcpEnvironmentReferenceSchema = z
  .object({
    key: z.string().trim().min(1),
    fromEnv: z.string().trim().min(1),
  })
  .strict()

export const mcpServerSchema = z
  .object({
    id: z.string().trim().min(1),
    name: z.string().trim().min(1),
    transport: mcpTransportSchema,
    env: z.array(mcpEnvironmentReferenceSchema).default([]),
    cwd: nonEmptyOptionalTextSchema,
    connection: mcpConnectionStateSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const mcpRegistrySchema = z
  .object({
    servers: z.array(mcpServerSchema).default([]),
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((registry, ctx) => {
    const seen = new Set<string>()
    registry.servers.forEach((server, index) => {
      if (seen.has(server.id)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['servers', index, 'id'],
          message: `MCP registry cannot include duplicate server id \`${server.id}\`.`,
        })
      }
      seen.add(server.id)
    })
  })

export const mcpImportDiagnosticSchema = z
  .object({
    index: z.number().int().nonnegative(),
    serverId: nonEmptyOptionalTextSchema,
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const upsertMcpServerRequestSchema = z
  .object({
    id: z.string().trim().min(1),
    name: z.string().trim().min(1),
    transport: mcpTransportSchema,
    env: z.array(mcpEnvironmentReferenceSchema).default([]),
    cwd: nonEmptyOptionalTextSchema,
  })
  .strict()

export const removeMcpServerRequestSchema = z
  .object({
    serverId: z.string().trim().min(1),
  })
  .strict()

export const importMcpServersRequestSchema = z
  .object({
    path: z.string().trim().min(1),
  })
  .strict()

export const importMcpServersResponseSchema = z
  .object({
    registry: mcpRegistrySchema,
    diagnostics: z.array(mcpImportDiagnosticSchema),
  })
  .strict()

export const refreshMcpServerStatusesRequestSchema = z
  .object({
    serverIds: z.array(z.string().trim().min(1)).default([]),
  })
  .strict()

export type McpTransportKindDto = z.infer<typeof mcpTransportKindSchema>
export type McpTransportDto = z.infer<typeof mcpTransportSchema>
export type McpConnectionStatusDto = z.infer<typeof mcpConnectionStatusSchema>
export type McpConnectionDiagnosticDto = z.infer<typeof mcpConnectionDiagnosticSchema>
export type McpConnectionStateDto = z.infer<typeof mcpConnectionStateSchema>
export type McpEnvironmentReferenceDto = z.infer<typeof mcpEnvironmentReferenceSchema>
export type McpServerDto = z.infer<typeof mcpServerSchema>
export type McpRegistryDto = z.infer<typeof mcpRegistrySchema>
export type McpImportDiagnosticDto = z.infer<typeof mcpImportDiagnosticSchema>
export type UpsertMcpServerRequestDto = z.infer<typeof upsertMcpServerRequestSchema>
export type RemoveMcpServerRequestDto = z.infer<typeof removeMcpServerRequestSchema>
export type ImportMcpServersRequestDto = z.infer<typeof importMcpServersRequestSchema>
export type ImportMcpServersResponseDto = z.infer<typeof importMcpServersResponseSchema>
export type RefreshMcpServerStatusesRequestDto = z.infer<typeof refreshMcpServerStatusesRequestSchema>

export function getMcpConnectionStatusLabel(status: McpConnectionStatusDto): string {
  switch (status) {
    case 'connected':
      return 'Connected'
    case 'failed':
      return 'Failed'
    case 'blocked':
      return 'Blocked'
    case 'misconfigured':
      return 'Misconfigured'
    case 'stale':
      return 'Stale'
  }
}

export function getMcpTransportKindLabel(kind: McpTransportKindDto): string {
  switch (kind) {
    case 'stdio':
      return 'stdio'
    case 'http':
      return 'HTTP'
    case 'sse':
      return 'SSE'
  }
}
