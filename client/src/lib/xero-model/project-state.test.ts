import { describe, expect, it } from 'vitest'
import {
  createProjectStateBackupRequestSchema,
  projectStateBackupResponseSchema,
  projectStateRepairResponseSchema,
  projectStateRestoreResponseSchema,
  repairProjectStateRequestSchema,
  restoreProjectStateBackupRequestSchema,
} from './project-state'

const projectId = 'project-state'
const backupId = 'backup-2026-05-09T10.00.00Z'
const createdAt = '2026-05-09T10:00:00Z'

describe('project state command contracts', () => {
  it('validates backup, restore, and repair command payloads', () => {
    expect(
      createProjectStateBackupRequestSchema.parse({
        projectId,
        backupId,
      }),
    ).toEqual({ projectId, backupId })
    expect(
      restoreProjectStateBackupRequestSchema.parse({
        projectId,
        backupId,
      }).backupId,
    ).toBe(backupId)
    expect(repairProjectStateRequestSchema.parse({ projectId }).projectId).toBe(projectId)

    const backup = projectStateBackupResponseSchema.parse({
      schema: 'xero.project_state_backup_command.v1',
      projectId,
      backupId,
      createdAt,
      fileCount: 12,
      byteCount: 4096,
      storageScope: 'os_app_data',
      backupLocation: `app-data/backups/${backupId}`,
      manifestLocation: `app-data/backups/${backupId}/manifest.json`,
      uiDeferred: true,
    })
    expect(backup.backupLocation).not.toContain('/Users/')

    const restore = projectStateRestoreResponseSchema.parse({
      schema: 'xero.project_state_restore_command.v1',
      projectId,
      backupId,
      restoredAt: createdAt,
      preRestoreBackupId: 'pre-restore-2026-05-09T10.01.00Z',
      storageScope: 'os_app_data',
      uiDeferred: true,
    })
    expect(restore.uiDeferred).toBe(true)
    expect(() =>
      projectStateRestoreResponseSchema.parse({
        ...restore,
        preRestoreBackupId: backupId,
      }),
    ).toThrow(/distinct/)

    const repair = projectStateRepairResponseSchema.parse({
      schema: 'xero.project_state_repair_command.v1',
      projectId,
      checkedAt: createdAt,
      sqliteCheckpointed: true,
      outboxInspectedCount: 1,
      outboxReconciledCount: 1,
      outboxFailedCount: 0,
      handoffInspectedCount: 1,
      handoffRepairedCount: 0,
      handoffFailedCount: 0,
      projectRecordHealthStatus: 'healthy',
      agentMemoryHealthStatus: 'healthy',
      diagnostics: [],
      storageScope: 'os_app_data',
      uiDeferred: true,
    })
    expect(repair.diagnostics).toHaveLength(0)

    expect(
      createProjectStateBackupRequestSchema.safeParse({
        projectId,
        backupId: '../outside',
      }).success,
    ).toBe(false)
    expect(() =>
      projectStateBackupResponseSchema.parse({
        ...backup,
        backupLocation: `/Users/sn0w/Library/Application Support/xero/${backupId}`,
      }),
    ).toThrow(/app-data relative/)
    expect(() =>
      projectStateRepairResponseSchema.parse({
        ...repair,
        diagnostics: [
          {
            code: 'project_state_repair_unknown',
            message: 'Unknown repair state.',
            severity: 'critical',
          },
        ],
      }),
    ).toThrow()
    expect(() =>
      projectStateRepairResponseSchema.parse({
        ...repair,
        outboxInspectedCount: 1,
        outboxReconciledCount: 1,
        outboxFailedCount: 1,
      }),
    ).toThrow(/outbox outcomes/)
    expect(() =>
      projectStateRepairResponseSchema.parse({
        ...repair,
        handoffInspectedCount: 1,
        handoffRepairedCount: 1,
        handoffFailedCount: 1,
      }),
    ).toThrow(/handoff outcomes/)
  })
})
