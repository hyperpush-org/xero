import { z } from 'zod'

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

export type DeleteProjectContextRecordRequestDto = z.infer<typeof deleteProjectContextRecordRequestSchema>
export type DeleteProjectContextRecordResponseDto = z.infer<typeof deleteProjectContextRecordResponseSchema>
export type SupersedeProjectContextRecordRequestDto = z.infer<
  typeof supersedeProjectContextRecordRequestSchema
>
export type SupersedeProjectContextRecordResponseDto = z.infer<
  typeof supersedeProjectContextRecordResponseSchema
>
