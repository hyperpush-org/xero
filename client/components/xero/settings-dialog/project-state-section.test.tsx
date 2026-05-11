import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  ProjectStateSection,
  type ProjectStateAdapter,
} from '@/components/xero/settings-dialog/project-state-section'
import type {
  ListProjectStateBackupsResponseDto,
  ProjectStateBackupListingEntryDto,
  ProjectStateBackupResponseDto,
  ProjectStateRepairResponseDto,
  ProjectStateRestoreResponseDto,
} from '@/src/lib/xero-model/project-state'

const PROJECT_ID = 'project-x'
const CHECKED_AT = '2026-05-10T08:30:00Z'

function makeBackup(
  overrides: Partial<ProjectStateBackupListingEntryDto> &
    Pick<ProjectStateBackupListingEntryDto, 'backupId'>,
): ProjectStateBackupListingEntryDto {
  return {
    backupId: overrides.backupId,
    createdAt: overrides.createdAt ?? '2026-05-09T10:00:00Z',
    fileCount: overrides.fileCount ?? 12,
    byteCount: overrides.byteCount ?? 4096,
    manifestPresent: overrides.manifestPresent ?? true,
    preRestore: overrides.preRestore ?? false,
    backupLocation: overrides.backupLocation ?? `app-data/backups/${overrides.backupId}`,
    manifestLocation:
      overrides.manifestLocation ?? `app-data/backups/${overrides.backupId}/manifest.json`,
  }
}

function makeListResponse(
  backups: ProjectStateBackupListingEntryDto[],
): ListProjectStateBackupsResponseDto {
  return {
    schema: 'xero.project_state_backup_list_command.v1',
    projectId: PROJECT_ID,
    storageScope: 'os_app_data',
    backups,
    uiDeferred: true,
  }
}

function makeAdapter(initial: ListProjectStateBackupsResponseDto) {
  const listBackups = vi.fn<ProjectStateAdapter['listBackups']>().mockResolvedValue(initial)
  const createBackup = vi.fn<ProjectStateAdapter['createBackup']>()
  const restoreBackup = vi.fn<ProjectStateAdapter['restoreBackup']>()
  const repairProjectState = vi.fn<ProjectStateAdapter['repairProjectState']>()
  return {
    adapter: { listBackups, createBackup, restoreBackup, repairProjectState } satisfies ProjectStateAdapter,
    listBackups,
    createBackup,
    restoreBackup,
    repairProjectState,
  }
}

function makeBackupResponse(backupId: string): ProjectStateBackupResponseDto {
  return {
    schema: 'xero.project_state_backup_command.v1',
    projectId: PROJECT_ID,
    backupId,
    createdAt: CHECKED_AT,
    fileCount: 8,
    byteCount: 2048,
    storageScope: 'os_app_data',
    backupLocation: `app-data/backups/${backupId}`,
    manifestLocation: `app-data/backups/${backupId}/manifest.json`,
    uiDeferred: true,
  }
}

function makeRestoreResponse(
  backupId: string,
  preRestoreBackupId: string,
): ProjectStateRestoreResponseDto {
  return {
    schema: 'xero.project_state_restore_command.v1',
    projectId: PROJECT_ID,
    backupId,
    restoredAt: CHECKED_AT,
    preRestoreBackupId,
    storageScope: 'os_app_data',
    uiDeferred: true,
  }
}

function makeRepairResponse(
  diagnostics: ProjectStateRepairResponseDto['diagnostics'] = [],
  overrides: Partial<ProjectStateRepairResponseDto> = {},
): ProjectStateRepairResponseDto {
  return {
    schema: 'xero.project_state_repair_command.v1',
    projectId: PROJECT_ID,
    checkedAt: CHECKED_AT,
    sqliteCheckpointed: true,
    outboxInspectedCount: overrides.outboxInspectedCount ?? 2,
    outboxReconciledCount: overrides.outboxReconciledCount ?? 2,
    outboxFailedCount: overrides.outboxFailedCount ?? 0,
    handoffInspectedCount: overrides.handoffInspectedCount ?? 1,
    handoffRepairedCount: overrides.handoffRepairedCount ?? 0,
    handoffFailedCount: overrides.handoffFailedCount ?? 0,
    projectRecordHealthStatus: overrides.projectRecordHealthStatus ?? 'healthy',
    agentMemoryHealthStatus: overrides.agentMemoryHealthStatus ?? 'healthy',
    diagnostics,
    storageScope: 'os_app_data',
    uiDeferred: true,
  }
}

describe('ProjectStateSection', () => {
  it('renders the project-bound empty state when no project is selected', () => {
    render(<ProjectStateSection projectId={null} projectLabel={null} adapter={null} />)
    expect(screen.getByText('Select a project')).toBeInTheDocument()
  })

  it('renders the unavailable state when no adapter is provided', () => {
    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel="Xero" adapter={null} />)
    expect(screen.getByText('Project state controls unavailable')).toBeInTheDocument()
  })

  it('lists backups returned by the adapter and groups pre-restore snapshots', async () => {
    const userBackup = makeBackup({ backupId: 'backup-2026-05-09T10.00.00Z' })
    const preRestore = makeBackup({
      backupId: 'pre-restore-2026-05-09T10.05.00Z',
      preRestore: true,
    })
    const { adapter, listBackups } = makeAdapter(makeListResponse([userBackup, preRestore]))

    render(
      <ProjectStateSection projectId={PROJECT_ID} projectLabel="Xero" adapter={adapter} />,
    )

    await waitFor(() => {
      expect(listBackups).toHaveBeenCalledWith({ projectId: PROJECT_ID })
    })

    const rows = await screen.findAllByTestId('project-state-backup')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveAttribute('data-backup-id', userBackup.backupId)
    expect(screen.getByText(/Pre-restore snapshots \(1\)/)).toBeInTheDocument()
  })

  it('shows the empty state when no user backups exist', async () => {
    const { adapter } = makeAdapter(makeListResponse([]))
    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    expect(await screen.findByText('No backups yet')).toBeInTheDocument()
  })

  it('creates a backup and reloads the list', async () => {
    const before = makeListResponse([])
    const created = makeBackupResponse('backup-2026-05-10T08.30.00Z')
    const after = makeListResponse([
      makeBackup({
        backupId: created.backupId,
        createdAt: created.createdAt,
        fileCount: created.fileCount,
        byteCount: created.byteCount,
      }),
    ])
    const { adapter, listBackups, createBackup } = makeAdapter(before)
    listBackups.mockResolvedValueOnce(before).mockResolvedValueOnce(after)
    createBackup.mockResolvedValue(created)

    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)
    await screen.findByText('No backups yet')

    fireEvent.click(screen.getByRole('button', { name: 'Create project state backup' }))

    await waitFor(() => {
      expect(createBackup).toHaveBeenCalledWith({ projectId: PROJECT_ID })
    })
    await waitFor(() => {
      expect(listBackups).toHaveBeenCalledTimes(2)
    })
    expect(await screen.findByTestId('project-state-action-message')).toHaveTextContent(
      created.backupId,
    )
  })

  it('confirms before restoring a backup and surfaces the pre-restore id', async () => {
    const userBackup = makeBackup({ backupId: 'backup-2026-05-09T10.00.00Z' })
    const before = makeListResponse([userBackup])
    const { adapter, listBackups, restoreBackup } = makeAdapter(before)
    const restored = makeRestoreResponse(userBackup.backupId, 'pre-restore-2026-05-10T08.30.00Z')
    listBackups
      .mockResolvedValueOnce(before)
      .mockResolvedValueOnce(
        makeListResponse([
          userBackup,
          makeBackup({ backupId: restored.preRestoreBackupId, preRestore: true }),
        ]),
      )
    restoreBackup.mockResolvedValue(restored)

    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const row = await screen.findByTestId('project-state-backup')
    fireEvent.click(within(row).getByRole('button', { name: `Restore backup ${userBackup.backupId}` }))

    // Confirmation dialog appears
    expect(await screen.findByRole('alertdialog')).toHaveTextContent('Restore project state?')
    fireEvent.click(screen.getByRole('button', { name: /^Restore$/ }))

    await waitFor(() => {
      expect(restoreBackup).toHaveBeenCalledWith({
        projectId: PROJECT_ID,
        backupId: userBackup.backupId,
      })
    })
    expect(await screen.findByTestId('project-state-action-message')).toHaveTextContent(
      restored.preRestoreBackupId,
    )
  })

  it('runs repair and renders a healthy report when no diagnostics are returned', async () => {
    const { adapter, repairProjectState } = makeAdapter(makeListResponse([]))
    repairProjectState.mockResolvedValue(makeRepairResponse())

    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)
    await screen.findByText('No backups yet')

    fireEvent.click(screen.getByRole('button', { name: 'Repair project state' }))

    await waitFor(() => {
      expect(repairProjectState).toHaveBeenCalledWith({ projectId: PROJECT_ID })
    })
    const report = await screen.findByTestId('project-state-repair-report')
    expect(within(report).getByText('Project state healthy')).toBeInTheDocument()
  })

  it('renders repair diagnostics when the report flags failures', async () => {
    const { adapter, repairProjectState } = makeAdapter(makeListResponse([]))
    repairProjectState.mockResolvedValue(
      makeRepairResponse(
        [
          {
            code: 'project_state_repair_outbox_failed',
            message: '1 outbox operation still needs manual repair.',
            severity: 'error',
          },
        ],
        { outboxFailedCount: 1, outboxReconciledCount: 1, outboxInspectedCount: 2 },
      ),
    )

    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    fireEvent.click(screen.getByRole('button', { name: 'Repair project state' }))

    const diagnostics = await screen.findByTestId('project-state-repair-diagnostics')
    expect(within(diagnostics).getByText(/project_state_repair_outbox_failed/)).toBeInTheDocument()
    expect(screen.getByText('Repair attention needed')).toBeInTheDocument()
  })

  it('surfaces adapter errors without losing the existing list', async () => {
    const userBackup = makeBackup({ backupId: 'backup-existing' })
    const { adapter, createBackup } = makeAdapter(makeListResponse([userBackup]))
    createBackup.mockRejectedValue(new Error('Disk full'))

    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    await screen.findByTestId('project-state-backup')
    fireEvent.click(screen.getByRole('button', { name: 'Create project state backup' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Disk full')
    expect(screen.getByTestId('project-state-backup')).toBeInTheDocument()
  })

  it('disables Restore for backups missing a manifest', async () => {
    const broken = makeBackup({ backupId: 'backup-broken', manifestPresent: false })
    const { adapter } = makeAdapter(makeListResponse([broken]))
    render(<ProjectStateSection projectId={PROJECT_ID} projectLabel={null} adapter={adapter} />)

    const row = await screen.findByTestId('project-state-backup')
    expect(within(row).getByRole('button', { name: `Restore backup ${broken.backupId}` })).toBeDisabled()
    expect(within(row).getByText('No manifest')).toBeInTheDocument()
  })
})
