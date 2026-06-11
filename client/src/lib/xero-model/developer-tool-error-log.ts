import { z } from 'zod'

export const developerToolErrorLogListRequestSchema = z
  .object({
    limit: z.number().int().positive().max(500).optional(),
    offset: z.number().int().nonnegative().optional(),
    projectId: z.string().trim().min(1).optional(),
    toolName: z.string().trim().min(1).optional(),
    errorCode: z.string().trim().min(1).optional(),
    query: z.string().trim().optional(),
  })
  .strict()
export type DeveloperToolErrorLogListRequestDto = z.infer<
  typeof developerToolErrorLogListRequestSchema
>

export const developerToolErrorLogEntrySchema = z
  .object({
    id: z.string().trim().min(1),
    occurredAt: z.string().trim().min(1),
    source: z.string().trim().min(1),
    projectId: z.string().trim().min(1).nullable().optional(),
    agentSessionId: z.string().trim().min(1).nullable().optional(),
    runId: z.string().trim().min(1).nullable().optional(),
    turnIndex: z.number().int().nullable().optional(),
    toolCallId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    inputSha256: z.string().regex(/^[0-9a-f]{64}$/),
    inputJson: z.unknown(),
    inputRedacted: z.boolean(),
    errorCode: z.string().trim().min(1),
    errorClass: z.string().trim().min(1),
    errorCategory: z.string().trim().min(1).nullable().optional(),
    errorMessage: z.string().trim().min(1),
    modelMessage: z.string().nullable().optional(),
    retryable: z.boolean(),
    dispatchJson: z.unknown(),
    contextJson: z.unknown(),
    messagePreview: z.string(),
  })
  .strict()
export type DeveloperToolErrorLogEntryDto = z.infer<
  typeof developerToolErrorLogEntrySchema
>

export const developerToolErrorLogListResponseSchema = z
  .object({
    databasePath: z.string(),
    entries: z.array(developerToolErrorLogEntrySchema),
    projectIds: z.array(z.string().trim().min(1)),
    totalCount: z.number().int().nonnegative(),
    limit: z.number().int().positive(),
    offset: z.number().int().nonnegative(),
  })
  .strict()
export type DeveloperToolErrorLogListResponseDto = z.infer<
  typeof developerToolErrorLogListResponseSchema
>

export const developerToolErrorLogClearResponseSchema = z
  .object({
    databasePath: z.string(),
    clearedCount: z.number().int().nonnegative(),
  })
  .strict()
export type DeveloperToolErrorLogClearResponseDto = z.infer<
  typeof developerToolErrorLogClearResponseSchema
>
