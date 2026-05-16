import { z } from 'zod'

export const changeKindSchema = z.enum([
  'added',
  'modified',
  'deleted',
  'renamed',
  'copied',
  'type_change',
  'conflicted',
])

export const phaseStatusSchema = z.enum(['complete', 'active', 'pending', 'blocked'])
export const nullableTextSchema = z.string().nullable().optional()
export const nonEmptyOptionalTextSchema = z.string().trim().min(1).nullable().optional()
export const isoTimestampSchema = z.string().datetime({ offset: true })
export const optionalIsoTimestampSchema = isoTimestampSchema.nullable().optional()

export const payloadBudgetDiagnosticSchema = z
  .object({
    key: z.string().trim().min(1),
    budgetBytes: z.number().int().positive(),
    observedBytes: z.number().int().nonnegative(),
    truncated: z.boolean(),
    dropped: z.boolean(),
    message: z.string().trim().min(1),
  })
  .strict()

export const gitToolResultScopeSchema = z.enum(['staged', 'unstaged', 'worktree'])
export const webToolResultContentKindSchema = z.enum(['html', 'plain_text'])
export const browserComputerUseSurfaceSchema = z.enum(['browser', 'computer_use'])
export const browserComputerUseActionStatusSchema = z.enum([
  'pending',
  'running',
  'succeeded',
  'failed',
  'blocked',
])
export const mcpCapabilityKindSchema = z.enum(['tool', 'resource', 'prompt', 'command'])

export const toolResultSummarySchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('command'),
      exitCode: z.number().int().nullable().optional(),
      timedOut: z.boolean(),
      stdoutTruncated: z.boolean(),
      stderrTruncated: z.boolean(),
      stdoutRedacted: z.boolean(),
      stderrRedacted: z.boolean(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('file'),
      path: nonEmptyOptionalTextSchema,
      scope: nonEmptyOptionalTextSchema,
      lineCount: z.number().int().nonnegative().nullable().optional(),
      matchCount: z.number().int().nonnegative().nullable().optional(),
      truncated: z.boolean(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('git'),
      scope: gitToolResultScopeSchema.nullable().optional(),
      changedFiles: z.number().int().nonnegative(),
      truncated: z.boolean(),
      baseRevision: nonEmptyOptionalTextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('web'),
      target: z.string().trim().min(1),
      resultCount: z.number().int().nonnegative().nullable().optional(),
      finalUrl: nonEmptyOptionalTextSchema,
      contentKind: webToolResultContentKindSchema.nullable().optional(),
      contentType: nonEmptyOptionalTextSchema,
      truncated: z.boolean(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('browser_computer_use'),
      surface: browserComputerUseSurfaceSchema,
      action: z.string().trim().min(1),
      status: browserComputerUseActionStatusSchema,
      target: nonEmptyOptionalTextSchema,
      outcome: nonEmptyOptionalTextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('mcp_capability'),
      serverId: z.string().trim().min(1),
      capabilityKind: mcpCapabilityKindSchema,
      capabilityId: z.string().trim().min(1),
      capabilityName: nonEmptyOptionalTextSchema,
    })
    .strict(),
])

export type GitToolResultScopeDto = z.infer<typeof gitToolResultScopeSchema>
export type WebToolResultContentKindDto = z.infer<typeof webToolResultContentKindSchema>
export type BrowserComputerUseSurfaceDto = z.infer<typeof browserComputerUseSurfaceSchema>
export type BrowserComputerUseActionStatusDto = z.infer<typeof browserComputerUseActionStatusSchema>
export type McpCapabilityKindDto = z.infer<typeof mcpCapabilityKindSchema>
export type PayloadBudgetDiagnosticDto = z.infer<typeof payloadBudgetDiagnosticSchema>
export type ToolResultSummaryDto = z.infer<typeof toolResultSummarySchema>

export function sortByNewest<T>(
  items: readonly T[],
  getTimestamp: (item: T) => string | null | undefined,
): T[] {
  return [...items]
    .map((item, index) => ({ item, index }))
    .sort((left, right) => {
      const leftTime = Date.parse(getTimestamp(left.item) ?? '')
      const rightTime = Date.parse(getTimestamp(right.item) ?? '')
      const normalizedLeftTime = Number.isFinite(leftTime) ? leftTime : 0
      const normalizedRightTime = Number.isFinite(rightTime) ? rightTime : 0

      if (normalizedLeftTime === normalizedRightTime) {
        return left.index - right.index
      }

      return normalizedRightTime - normalizedLeftTime
    })
    .map(({ item }) => item)
}

export function safePercent(completed: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) {
    return 0
  }

  const ratio = completed / total
  if (!Number.isFinite(ratio) || ratio <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round(ratio * 100)))
}

export function normalizeText(value: string | null | undefined, fallback: string): string {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : fallback
}

export function normalizeOptionalText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

export function humanizeSegmentedLabel(value: string): string {
  return value
    .split(/[_-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}
