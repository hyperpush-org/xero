import { z } from 'zod'
import { isoTimestampSchema } from './shared'

export const projectStateBackupIdSchema = z
  .string()
  .trim()
  .min(1)
  .regex(/^[A-Za-z0-9_.-]+$/)

export const createProjectStateBackupRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    backupId: projectStateBackupIdSchema.nullable().optional(),
  })
  .strict()

export const restoreProjectStateBackupRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    backupId: projectStateBackupIdSchema,
  })
  .strict()

export const repairProjectStateRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const listProjectStateBackupsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const projectStateBackupListingEntrySchema = z
  .object({
    backupId: projectStateBackupIdSchema,
    createdAt: isoTimestampSchema.nullable().optional(),
    fileCount: z.number().int().nonnegative().nullable().optional(),
    byteCount: z.number().int().nonnegative().nullable().optional(),
    manifestPresent: z.boolean(),
    preRestore: z.boolean(),
    backupLocation: z.string().trim().min(1),
    manifestLocation: z.string().trim().min(1),
  })
  .strict()
  .superRefine((entry, ctx) => {
    const expectedBackupLocation = `app-data/backups/${entry.backupId}`
    const expectedManifestLocation = `${expectedBackupLocation}/manifest.json`
    if (entry.backupLocation !== expectedBackupLocation) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['backupLocation'],
        message: 'Project-state backup locations must be app-data relative.',
      })
    }
    if (entry.manifestLocation !== expectedManifestLocation) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['manifestLocation'],
        message: 'Project-state backup manifests must stay under the backup directory.',
      })
    }
  })

export const listProjectStateBackupsResponseSchema = z
  .object({
    schema: z.literal('xero.project_state_backup_list_command.v1'),
    projectId: z.string().trim().min(1),
    storageScope: z.literal('os_app_data'),
    backups: z.array(projectStateBackupListingEntrySchema),
    uiDeferred: z.literal(true),
  })
  .strict()

export const projectStateBackupResponseSchema = z
  .object({
    schema: z.literal('xero.project_state_backup_command.v1'),
    projectId: z.string().trim().min(1),
    backupId: projectStateBackupIdSchema,
    createdAt: isoTimestampSchema,
    fileCount: z.number().int().nonnegative(),
    byteCount: z.number().int().nonnegative(),
    storageScope: z.literal('os_app_data'),
    backupLocation: z.string().trim().min(1),
    manifestLocation: z.string().trim().min(1),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    const expectedBackupLocation = `app-data/backups/${response.backupId}`
    const expectedManifestLocation = `${expectedBackupLocation}/manifest.json`
    if (response.backupLocation !== expectedBackupLocation) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['backupLocation'],
        message: 'Project-state backup locations must be app-data relative.',
      })
    }
    if (response.manifestLocation !== expectedManifestLocation) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['manifestLocation'],
        message: 'Project-state backup manifests must stay under the backup directory.',
      })
    }
  })

export const projectStateRestoreResponseSchema = z
  .object({
    schema: z.literal('xero.project_state_restore_command.v1'),
    projectId: z.string().trim().min(1),
    backupId: projectStateBackupIdSchema,
    restoredAt: isoTimestampSchema,
    preRestoreBackupId: projectStateBackupIdSchema,
    storageScope: z.literal('os_app_data'),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.preRestoreBackupId === response.backupId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['preRestoreBackupId'],
        message: 'Project-state restore pre-restore backup must be distinct from the restored backup.',
      })
    }
  })

export const projectStateRepairDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    severity: z.enum(['info', 'warning', 'error']),
  })
  .strict()

export const projectStateRepairResponseSchema = z
  .object({
    schema: z.literal('xero.project_state_repair_command.v1'),
    projectId: z.string().trim().min(1),
    checkedAt: isoTimestampSchema,
    sqliteCheckpointed: z.boolean(),
    outboxInspectedCount: z.number().int().nonnegative(),
    outboxReconciledCount: z.number().int().nonnegative(),
    outboxFailedCount: z.number().int().nonnegative(),
    handoffInspectedCount: z.number().int().nonnegative(),
    handoffRepairedCount: z.number().int().nonnegative(),
    handoffFailedCount: z.number().int().nonnegative(),
    projectRecordHealthStatus: z.string().trim().min(1),
    agentMemoryHealthStatus: z.string().trim().min(1),
    diagnostics: z.array(projectStateRepairDiagnosticSchema),
    storageScope: z.literal('os_app_data'),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (
      response.outboxReconciledCount + response.outboxFailedCount >
      response.outboxInspectedCount
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['outboxReconciledCount'],
        message: 'Project-state repair outbox outcomes cannot exceed inspected count.',
      })
    }
    if (
      response.handoffRepairedCount + response.handoffFailedCount >
      response.handoffInspectedCount
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['handoffRepairedCount'],
        message: 'Project-state repair handoff outcomes cannot exceed inspected count.',
      })
    }
  })

export type CreateProjectStateBackupRequestDto = z.infer<typeof createProjectStateBackupRequestSchema>
export type RestoreProjectStateBackupRequestDto = z.infer<typeof restoreProjectStateBackupRequestSchema>
export type RepairProjectStateRequestDto = z.infer<typeof repairProjectStateRequestSchema>
export type ListProjectStateBackupsRequestDto = z.infer<typeof listProjectStateBackupsRequestSchema>
export type ProjectStateBackupIdDto = z.infer<typeof projectStateBackupIdSchema>
export type ProjectStateBackupResponseDto = z.infer<typeof projectStateBackupResponseSchema>
export type ProjectStateRestoreResponseDto = z.infer<typeof projectStateRestoreResponseSchema>
export type ProjectStateRepairDiagnosticDto = z.infer<typeof projectStateRepairDiagnosticSchema>
export type ProjectStateRepairResponseDto = z.infer<typeof projectStateRepairResponseSchema>
export type ProjectStateBackupListingEntryDto = z.infer<typeof projectStateBackupListingEntrySchema>
export type ListProjectStateBackupsResponseDto = z.infer<typeof listProjectStateBackupsResponseSchema>
