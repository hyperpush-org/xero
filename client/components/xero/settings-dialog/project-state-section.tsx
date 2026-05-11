import { useCallback, useEffect, useMemo, useState } from "react"
import {
  AlertTriangle,
  Archive,
  CheckCircle2,
  Database,
  HardDriveDownload,
  History,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Wrench,
} from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type {
  CreateProjectStateBackupRequestDto,
  ListProjectStateBackupsRequestDto,
  ListProjectStateBackupsResponseDto,
  ProjectStateBackupListingEntryDto,
  ProjectStateBackupResponseDto,
  ProjectStateRepairResponseDto,
  ProjectStateRestoreResponseDto,
  RepairProjectStateRequestDto,
  RestoreProjectStateBackupRequestDto,
} from "@/src/lib/xero-model/project-state"

import { SectionHeader } from "./section-header"
import {
  EmptyPanel,
  ErrorBanner,
  InlineCounts,
  Pill,
  SubHeading,
  SuccessBanner,
} from "./_shared"

export interface ProjectStateAdapter {
  listBackups: (request: ListProjectStateBackupsRequestDto) => Promise<ListProjectStateBackupsResponseDto>
  createBackup: (request: CreateProjectStateBackupRequestDto) => Promise<ProjectStateBackupResponseDto>
  restoreBackup: (request: RestoreProjectStateBackupRequestDto) => Promise<ProjectStateRestoreResponseDto>
  repairProjectState: (request: RepairProjectStateRequestDto) => Promise<ProjectStateRepairResponseDto>
}

interface ProjectStateSectionProps {
  projectId: string | null
  projectLabel: string | null
  adapter?: ProjectStateAdapter | null
}

type LoadStatus = "idle" | "loading" | "ready" | "error"
type ActionKind = "create" | "restore" | "repair"

interface ListState {
  status: LoadStatus
  errorMessage: string | null
  response: ListProjectStateBackupsResponseDto | null
}

const INITIAL_LIST_STATE: ListState = {
  status: "idle",
  errorMessage: null,
  response: null,
}

export function ProjectStateSection({ projectId, projectLabel, adapter }: ProjectStateSectionProps) {
  const [listState, setListState] = useState<ListState>(INITIAL_LIST_STATE)
  const [pendingAction, setPendingAction] = useState<ActionKind | null>(null)
  const [pendingBackupId, setPendingBackupId] = useState<string | null>(null)
  const [restoreTarget, setRestoreTarget] = useState<ProjectStateBackupListingEntryDto | null>(null)
  const [actionMessage, setActionMessage] = useState<string | null>(null)
  const [repairReport, setRepairReport] = useState<ProjectStateRepairResponseDto | null>(null)

  const loadList = useCallback(async () => {
    if (!projectId || !adapter) return
    setListState((current) => ({ ...current, status: "loading", errorMessage: null }))
    try {
      const response = await adapter.listBackups({ projectId })
      setListState({ status: "ready", errorMessage: null, response })
    } catch (caught) {
      setListState((current) => ({
        ...current,
        status: "error",
        errorMessage: errorMessage(caught, "Xero could not load project-state backups."),
      }))
    }
  }, [adapter, projectId])

  useEffect(() => {
    void loadList()
  }, [loadList])

  const backups = listState.response?.backups ?? []
  const userBackups = useMemo(() => backups.filter((entry) => !entry.preRestore), [backups])
  const preRestoreBackups = useMemo(() => backups.filter((entry) => entry.preRestore), [backups])

  const totalBytes = useMemo(
    () => userBackups.reduce((sum, entry) => sum + (entry.byteCount ?? 0), 0),
    [userBackups],
  )
  const latestBackup = useMemo(
    () => userBackups[0] ?? null,
    [userBackups],
  )

  const handleCreate = useCallback(async () => {
    if (!projectId || !adapter) return
    setPendingAction("create")
    setActionMessage(null)
    setRepairReport(null)
    try {
      const response = await adapter.createBackup({ projectId })
      setActionMessage(
        `Created backup ${response.backupId} (${formatByteCount(response.byteCount)}, ${response.fileCount} files).`,
      )
      await loadList()
    } catch (caught) {
      setListState((current) => ({
        ...current,
        errorMessage: errorMessage(caught, "Xero could not create the project-state backup."),
      }))
    } finally {
      setPendingAction(null)
    }
  }, [adapter, loadList, projectId])

  const handleRestore = useCallback(
    async (entry: ProjectStateBackupListingEntryDto) => {
      if (!projectId || !adapter) return
      setPendingAction("restore")
      setPendingBackupId(entry.backupId)
      setActionMessage(null)
      setRepairReport(null)
      try {
        const response = await adapter.restoreBackup({ projectId, backupId: entry.backupId })
        setActionMessage(
          `Restored ${response.backupId}. Pre-restore snapshot saved as ${response.preRestoreBackupId}.`,
        )
        await loadList()
      } catch (caught) {
        setListState((current) => ({
          ...current,
          errorMessage: errorMessage(caught, "Xero could not restore the project-state backup."),
        }))
      } finally {
        setPendingAction(null)
        setPendingBackupId(null)
        setRestoreTarget(null)
      }
    },
    [adapter, loadList, projectId],
  )

  const handleRepair = useCallback(async () => {
    if (!projectId || !adapter) return
    setPendingAction("repair")
    setActionMessage(null)
    try {
      const response = await adapter.repairProjectState({ projectId })
      setRepairReport(response)
      setActionMessage(`Repair completed at ${formatTimestamp(response.checkedAt)}.`)
    } catch (caught) {
      setListState((current) => ({
        ...current,
        errorMessage: errorMessage(caught, "Xero could not repair project state."),
      }))
    } finally {
      setPendingAction(null)
    }
  }, [adapter, projectId])

  const isLoading = listState.status === "loading"

  const headerActions = (
    <div className="flex items-center gap-1.5">
      <Button
        size="sm"
        variant="outline"
        className="h-8 gap-1.5 text-[12px]"
        onClick={() => void loadList()}
        disabled={isLoading || !projectId || !adapter}
        aria-label="Refresh project state backups"
      >
        {isLoading ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <RefreshCw className="h-3.5 w-3.5" />
        )}
        Refresh
      </Button>
      <Button
        size="sm"
        variant="outline"
        className="h-8 gap-1.5 text-[12px]"
        onClick={() => void handleRepair()}
        disabled={pendingAction !== null || !projectId || !adapter}
        aria-label="Repair project state"
      >
        {pendingAction === "repair" ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <Wrench className="h-3.5 w-3.5" />
        )}
        Repair
      </Button>
      <Button
        size="sm"
        className="h-8 gap-1.5 text-[12px]"
        onClick={() => void handleCreate()}
        disabled={pendingAction !== null || !projectId || !adapter}
        aria-label="Create project state backup"
      >
        {pendingAction === "create" ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <HardDriveDownload className="h-3.5 w-3.5" />
        )}
        Create backup
      </Button>
    </div>
  )

  if (!projectId) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Project State"
          description="Back up, restore, and repair the per-project SQLite + Lance store."
        />
        <EmptyPanel
          icon={<Database className="h-4 w-4 text-muted-foreground/70" />}
          title="Select a project"
          body="Project-state backups are scoped to the active project."
        />
      </div>
    )
  }

  if (!adapter) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Project State"
          description={
            projectLabel
              ? `Back up, restore, and repair project state for ${projectLabel}.`
              : "Back up, restore, and repair project state."
          }
        />
        <EmptyPanel
          icon={<Database className="h-4 w-4 text-muted-foreground/70" />}
          title="Project state controls unavailable"
          body="The desktop adapter did not provide project-state commands. Restart Xero or upgrade to enable this surface."
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Project State"
        description={
          projectLabel
            ? `Back up, restore, and repair project state for ${projectLabel}.`
            : "Back up, restore, and repair project state."
        }
        actions={headerActions}
      />

      {userBackups.length > 0 ? (
        <InlineCounts
          items={[
            { label: "Backups", value: userBackups.length, tone: "info" },
            { label: "Total size", value: formatByteCount(totalBytes) },
            {
              label: "Latest",
              value: latestBackup?.createdAt ? formatRelative(latestBackup.createdAt) : "—",
              tone: latestBackup ? "good" : "neutral",
            },
          ]}
        />
      ) : null}

      {actionMessage ? (
        <SuccessBanner message={actionMessage} testId="project-state-action-message" />
      ) : null}

      {listState.errorMessage ? <ErrorBanner message={listState.errorMessage} /> : null}

      {repairReport ? <RepairReport report={repairReport} /> : null}

      <section className="flex flex-col gap-2.5" data-testid="project-state-backups">
        <SubHeading count={userBackups.length > 0 ? userBackups.length : undefined}>
          Backups
        </SubHeading>

        {isLoading && backups.length === 0 ? (
          <div
            aria-busy="true"
            aria-label="Loading project state backups"
            className="flex min-h-[120px] flex-col gap-2"
          >
            <div className="h-12 rounded-md bg-secondary/40" />
            <div className="h-12 rounded-md bg-secondary/30" />
          </div>
        ) : null}

        {listState.status === "ready" && userBackups.length === 0 ? (
          <EmptyPanel
            icon={<HardDriveDownload className="h-4 w-4 text-muted-foreground/70" />}
            title="No backups yet"
            body="Use Create backup to snapshot the current SQLite + Lance state before risky changes."
          />
        ) : null}

        {userBackups.length > 0 ? (
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
            {userBackups.map((entry) => (
              <BackupRow
                key={entry.backupId}
                entry={entry}
                pending={pendingAction === "restore" && pendingBackupId === entry.backupId}
                disabled={pendingAction !== null}
                onRestore={() => setRestoreTarget(entry)}
              />
            ))}
          </div>
        ) : null}

        {preRestoreBackups.length > 0 ? (
          <details className="overflow-hidden rounded-md border border-border/60 bg-secondary/10">
            <summary className="flex cursor-pointer items-center gap-2 px-3.5 py-2 text-[12px] text-foreground/80 transition-colors hover:bg-secondary/30">
              <History className="h-3.5 w-3.5 text-muted-foreground" />
              <span>Pre-restore snapshots ({preRestoreBackups.length})</span>
            </summary>
            <div className="divide-y divide-border/40 border-t border-border/40">
              {preRestoreBackups.map((entry) => (
                <BackupRow
                  key={entry.backupId}
                  entry={entry}
                  pending={pendingAction === "restore" && pendingBackupId === entry.backupId}
                  disabled={pendingAction !== null}
                  onRestore={() => setRestoreTarget(entry)}
                />
              ))}
            </div>
          </details>
        ) : null}
      </section>

      <AlertDialog
        open={restoreTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRestoreTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restore project state?</AlertDialogTitle>
            <AlertDialogDescription>
              This replaces the current SQLite + Lance store with{" "}
              <span className="font-medium text-foreground">{restoreTarget?.backupId}</span>. Xero
              will snapshot the current state as a pre-restore backup first, but anything not yet
              backed up will be overwritten.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pendingAction === "restore"}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (restoreTarget) void handleRestore(restoreTarget)
              }}
              disabled={pendingAction === "restore"}
            >
              {pendingAction === "restore" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RotateCcw className="h-3.5 w-3.5" />
              )}
              Restore
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

interface BackupRowProps {
  entry: ProjectStateBackupListingEntryDto
  pending: boolean
  disabled: boolean
  onRestore: () => void
}

function BackupRow({ entry, pending, disabled, onRestore }: BackupRowProps) {
  return (
    <div
      data-testid="project-state-backup"
      data-backup-id={entry.backupId}
      className="group flex items-start gap-3 px-3.5 py-3"
    >
      <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground">
        <Archive className="h-3.5 w-3.5" aria-hidden="true" />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="truncate font-mono text-[12.5px] text-foreground">
            {entry.backupId}
          </span>
          {entry.preRestore ? <Pill tone="info">Pre-restore</Pill> : null}
          {!entry.manifestPresent ? (
            <Pill tone="warn">
              <AlertTriangle className="h-2.5 w-2.5" />
              No manifest
            </Pill>
          ) : null}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-muted-foreground">
          {entry.createdAt ? <span>Created {formatTimestamp(entry.createdAt)}</span> : null}
          {entry.fileCount != null ? (
            <>
              <span aria-hidden>·</span>
              <span>{entry.fileCount} files</span>
            </>
          ) : null}
          {entry.byteCount != null ? (
            <>
              <span aria-hidden>·</span>
              <span>{formatByteCount(entry.byteCount)}</span>
            </>
          ) : null}
        </div>
        {entry.backupLocation ? (
          <p className="mt-0.5 truncate font-mono text-[10.5px] text-muted-foreground/70">
            {entry.backupLocation}
          </p>
        ) : null}
      </div>

      <Button
        size="sm"
        variant="outline"
        className="h-8 gap-1.5 text-[12px]"
        onClick={onRestore}
        disabled={disabled || !entry.manifestPresent}
        aria-label={`Restore backup ${entry.backupId}`}
      >
        {pending ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <RotateCcw className="h-3.5 w-3.5" />
        )}
        Restore
      </Button>
    </div>
  )
}

function RepairReport({ report }: { report: ProjectStateRepairResponseDto }) {
  const healthy =
    report.diagnostics.length === 0 &&
    report.projectRecordHealthStatus === "healthy" &&
    report.agentMemoryHealthStatus === "healthy"

  return (
    <section
      data-testid="project-state-repair-report"
      className={cn(
        "overflow-hidden rounded-md border",
        healthy ? "border-success/30 bg-success/[0.06]" : "border-warning/30 bg-warning/[0.06]",
      )}
    >
      <header
        className={cn(
          "flex items-center gap-2 px-3.5 py-2",
          healthy ? "text-success" : "text-warning",
        )}
      >
        {healthy ? (
          <ShieldCheck className="h-3.5 w-3.5" />
        ) : (
          <AlertTriangle className="h-3.5 w-3.5" />
        )}
        <span className="text-[12.5px] font-semibold">
          {healthy ? "Project state healthy" : "Repair attention needed"}
        </span>
        <span className="ml-auto text-[11px] text-current/80">
          Checked {formatTimestamp(report.checkedAt)}
        </span>
      </header>

      <dl className="grid grid-cols-2 gap-x-3 gap-y-1 border-t border-current/20 bg-background/40 px-3.5 py-2 text-[11.5px] text-foreground/80 sm:grid-cols-3">
        <DescPair label="SQLite" value={report.sqliteCheckpointed ? "Checkpointed" : "Skipped"} />
        <DescPair
          label="Outbox"
          value={`${report.outboxReconciledCount}/${report.outboxInspectedCount} reconciled`}
        />
        <DescPair
          label="Handoff"
          value={`${report.handoffRepairedCount}/${report.handoffInspectedCount} repaired`}
        />
        <DescPair label="Project records" value={report.projectRecordHealthStatus} />
        <DescPair label="Agent memory" value={report.agentMemoryHealthStatus} />
        {report.outboxFailedCount + report.handoffFailedCount > 0 ? (
          <DescPair
            label="Failures"
            value={`${report.outboxFailedCount} outbox, ${report.handoffFailedCount} handoff`}
          />
        ) : null}
      </dl>

      {report.diagnostics.length > 0 ? (
        <ul
          className="flex flex-col gap-1 border-t border-current/20 px-3.5 py-2"
          data-testid="project-state-repair-diagnostics"
        >
          {report.diagnostics.map((diagnostic, index) => (
            <li
              key={`${diagnostic.code}-${index}`}
              className="flex items-start gap-1.5 text-[11.5px] text-foreground/80"
            >
              <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0 text-warning" />
              <span>
                <span className="font-mono text-[11px] text-warning">{diagnostic.code}</span>{" "}
                <span>— {diagnostic.message}</span>
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="flex items-center gap-1.5 border-t border-current/20 px-3.5 py-2 text-[11.5px] text-success/90">
          <CheckCircle2 className="h-3.5 w-3.5" />
          No diagnostics reported.
        </p>
      )}
    </section>
  )
}

function DescPair({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center gap-1.5">
      <dt className="font-medium text-foreground/80">{label}</dt>
      <dd className="capitalize">{value.replace(/_/g, " ")}</dd>
    </div>
  )
}

function formatTimestamp(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) return value
  return new Date(parsed).toLocaleString()
}

function formatRelative(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) return value
  const diffMs = Date.now() - parsed
  if (diffMs < 0) return new Date(parsed).toLocaleDateString()
  const minutes = Math.floor(diffMs / 60000)
  if (minutes < 1) return "just now"
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(parsed).toLocaleDateString()
}

function formatByteCount(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return `${bytes} B`
  const units = ["B", "KB", "MB", "GB", "TB"]
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  const formatted = unitIndex === 0 ? `${value}` : value.toFixed(value >= 10 ? 0 : 1)
  return `${formatted} ${units[unitIndex]}`
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
