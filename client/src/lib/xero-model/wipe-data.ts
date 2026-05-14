import { z } from 'zod'
import { projectSummarySchema } from './project'

export const wipeProjectDataRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const wipeProjectDataResponseSchema = z
  .object({
    schema: z.literal('xero.wipe_project_data_command.v1'),
    projectId: z.string().trim().min(1),
    directoryRemoved: z.boolean(),
    projects: z.array(projectSummarySchema),
  })
  .strict()

export const wipeAllDataResponseSchema = z
  .object({
    schema: z.literal('xero.wipe_all_data_command.v1'),
    directoryRemoved: z.boolean(),
  })
  .strict()

export type WipeProjectDataRequestDto = z.infer<typeof wipeProjectDataRequestSchema>
export type WipeProjectDataResponseDto = z.infer<typeof wipeProjectDataResponseSchema>
export type WipeAllDataResponseDto = z.infer<typeof wipeAllDataResponseSchema>
