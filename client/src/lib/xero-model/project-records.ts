import { z } from 'zod'

export const projectContextRecordSummarySchema = z
  .object({
    recordId: z.string().trim().min(1),
    recordKind: z.string().trim().min(1),
    title: z.string(),
    summary: z.string().nullable(),
    textPreview: z.string().nullable(),
    importance: z.string().trim().min(1),
    redactionState: z.string().trim().min(1),
    visibility: z.string().trim().min(1),
    freshnessState: z.string().trim().min(1),
    tags: z.array(z.string()),
    relatedPaths: z.array(z.string()),
    supersedesId: z.string().nullable(),
    supersededById: z.string().nullable(),
    invalidatedAt: z.string().nullable(),
    runtimeAgentId: z.string(),
    agentDefinitionId: z.string(),
    agentDefinitionVersion: z.number().int().nonnegative(),
    runId: z.string(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict()

export const listProjectContextRecordsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const listProjectContextRecordsResponseSchema = z
  .object({
    schema: z.literal('xero.project_context_record_list_command.v1'),
    projectId: z.string().trim().min(1),
    records: z.array(projectContextRecordSummarySchema),
    uiDeferred: z.literal(true),
  })
  .strict()

export const deleteProjectContextRecordRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    recordId: z.string().trim().min(1),
  })
  .strict()

export const deleteProjectContextRecordResponseSchema = z
  .object({
    schema: z.literal('xero.project_context_record_delete_command.v1'),
    projectId: z.string().trim().min(1),
    recordId: z.string().trim().min(1),
    retrievalRemoved: z.literal(true),
    uiDeferred: z.literal(true),
  })
  .strict()

export const supersedeProjectContextRecordRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    supersededRecordId: z.string().trim().min(1),
    supersedingRecordId: z.string().trim().min(1),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.supersededRecordId === request.supersedingRecordId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['supersedingRecordId'],
        message: 'Superseding project records must be distinct from stale records.',
      })
    }
  })

export const supersedeProjectContextRecordResponseSchema = z
  .object({
    schema: z.literal('xero.project_context_record_supersede_command.v1'),
    projectId: z.string().trim().min(1),
    supersededRecordId: z.string().trim().min(1),
    supersedingRecordId: z.string().trim().min(1),
    retrievalChanged: z.literal(true),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.supersededRecordId === response.supersedingRecordId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['supersedingRecordId'],
        message: 'Supersede responses must identify distinct project records.',
      })
    }
  })

export type ProjectContextRecordSummaryDto = z.infer<typeof projectContextRecordSummarySchema>
export type ListProjectContextRecordsRequestDto = z.infer<typeof listProjectContextRecordsRequestSchema>
export type ListProjectContextRecordsResponseDto = z.infer<typeof listProjectContextRecordsResponseSchema>
export type DeleteProjectContextRecordRequestDto = z.infer<typeof deleteProjectContextRecordRequestSchema>
export type DeleteProjectContextRecordResponseDto = z.infer<typeof deleteProjectContextRecordResponseSchema>
export type SupersedeProjectContextRecordRequestDto = z.infer<
  typeof supersedeProjectContextRecordRequestSchema
>
export type SupersedeProjectContextRecordResponseDto = z.infer<
  typeof supersedeProjectContextRecordResponseSchema
>
