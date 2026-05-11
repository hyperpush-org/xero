import { useCallback, useEffect, useMemo, useState } from "react"
import {
  AlertTriangle,
  ArrowRight,
  Check,
  EyeOff,
  FileText,
  Lightbulb,
  ListChecks,
  Loader2,
  Pin,
  RefreshCw,
  ShieldAlert,
  Sparkles,
  Trash2,
  Wrench,
  X,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import type {
  DeleteProjectContextRecordRequestDto,
  DeleteProjectContextRecordResponseDto,
  ListProjectContextRecordsRequestDto,
  ListProjectContextRecordsResponseDto,
  ProjectContextRecordSummaryDto,
  SupersedeProjectContextRecordRequestDto,
  SupersedeProjectContextRecordResponseDto,
} from "@/src/lib/xero-model/project-records"

import { SectionHeader } from "./section-header"
import {
  EmptyPanel,
  ErrorBanner,
  InlineCounts,
  Pill,
  SubHeading,
  type Tone,
} from "./_shared"

export interface ProjectRecordsAdapter {
  listRecords: (
    request: ListProjectContextRecordsRequestDto,
  ) => Promise<ListProjectContextRecordsResponseDto>
  deleteRecord: (
    request: DeleteProjectContextRecordRequestDto,
  ) => Promise<DeleteProjectContextRecordResponseDto>
  supersedeRecord: (
    request: SupersedeProjectContextRecordRequestDto,
  ) => Promise<SupersedeProjectContextRecordResponseDto>
}

interface ProjectRecordsSectionProps {
  projectId: string | null
  projectLabel: string | null
  adapter?: ProjectRecordsAdapter | null
}

type LoadStatus = "idle" | "loading" | "ready" | "error"

interface SupersedeDialogState {
  open: boolean
  superseded: ProjectContextRecordSummaryDto | null
  supersedingId: string
  error: string | null
}

const INITIAL_SUPERSEDE_DIALOG: SupersedeDialogState = {
  open: false,
  superseded: null,
  supersedingId: "",
  error: null,
}

const FRESHNESS_TONE: Record<string, Tone> = {
  current: "good",
  stale: "warn",
  superseded: "neutral",
  blocked: "bad",
}

const IMPORTANCE_TONE: Record<string, Tone> = {
  low: "neutral",
  normal: "neutral",
  high: "info",
  critical: "bad",
}

const RECORD_KIND_ICON: Record<string, React.ComponentType<{ className?: string }>> = {
  finding: Lightbulb,
  decision: Pin,
  plan: ListChecks,
  fact: Sparkles,
  diagnostic: Wrench,
}

export function ProjectRecordsSection({
  projectId,
  projectLabel,
  adapter,
}: ProjectRecordsSectionProps) {
  const [status, setStatus] = useState<LoadStatus>("idle")
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [records, setRecords] = useState<ProjectContextRecordSummaryDto[]>([])
  const [pendingRecordId, setPendingRecordId] = useState<string | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<ProjectContextRecordSummaryDto | null>(null)
  const [supersedeDialog, setSupersedeDialog] = useState<SupersedeDialogState>(
    INITIAL_SUPERSEDE_DIALOG,
  )

  const recordsById = useMemo(() => {
    const map = new Map<string, ProjectContextRecordSummaryDto>()
    for (const record of records) {
      map.set(record.recordId, record)
    }
    return map
  }, [records])

  const counts = useMemo(() => {
    let current = 0
    let stale = 0
    let superseded = 0
    let blocked = 0
    for (const record of records) {
      if (record.redactionState === "blocked") blocked += 1
      if (record.supersededById !== null) {
        superseded += 1
        continue
      }
      if (record.freshnessState === "stale") stale += 1
      else current += 1
    }
    return { total: records.length, current, stale, superseded, blocked }
  }, [records])

  const loadRecords = useCallback(async () => {
    if (!projectId || !adapter) return
    setStatus("loading")
    setErrorMessage(null)
    try {
      const response = await adapter.listRecords({ projectId })
      setRecords(response.records)
      setStatus("ready")
    } catch (caught) {
      setStatus("error")
      setErrorMessage(toErrorMessage(caught, "Xero could not load project records."))
    }
  }, [adapter, projectId])

  useEffect(() => {
    void loadRecords()
  }, [loadRecords])

  const handleDelete = useCallback(async () => {
    if (!projectId || !adapter || !deleteTarget) return
    const recordId = deleteTarget.recordId
    setPendingRecordId(recordId)
    setErrorMessage(null)
    try {
      await adapter.deleteRecord({ projectId, recordId })
      setRecords((current) => current.filter((record) => record.recordId !== recordId))
      setDeleteTarget(null)
    } catch (caught) {
      setErrorMessage(toErrorMessage(caught, "Xero could not delete the project record."))
    } finally {
      setPendingRecordId(null)
    }
  }, [adapter, deleteTarget, projectId])

  const handleSupersede = useCallback(async () => {
    if (!projectId || !adapter || !supersedeDialog.superseded) return
    const supersedingId = supersedeDialog.supersedingId.trim()
    if (!supersedingId) {
      setSupersedeDialog((current) => ({
        ...current,
        error: "Enter the id of the record that supersedes this one.",
      }))
      return
    }
    if (supersedingId === supersedeDialog.superseded.recordId) {
      setSupersedeDialog((current) => ({
        ...current,
        error: "Superseding records must be different from this record.",
      }))
      return
    }
    setPendingRecordId(supersedeDialog.superseded.recordId)
    setErrorMessage(null)
    try {
      await adapter.supersedeRecord({
        projectId,
        supersededRecordId: supersedeDialog.superseded.recordId,
        supersedingRecordId: supersedingId,
      })
      await loadRecords()
      setSupersedeDialog(INITIAL_SUPERSEDE_DIALOG)
    } catch (caught) {
      setSupersedeDialog((current) => ({
        ...current,
        error: toErrorMessage(caught, "Xero could not supersede the project record."),
      }))
    } finally {
      setPendingRecordId(null)
    }
  }, [adapter, loadRecords, projectId, supersedeDialog])

  const isLoading = status === "loading"

  if (!projectId) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Project Records"
          description="Delete or supersede stale workspace records."
        />
        <EmptyPanel
          icon={<FileText className="h-4 w-4 text-muted-foreground/70" />}
          title="Select a project"
          body="Project records are stored per-project in Xero app data."
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Project Records"
        description={
          projectLabel
            ? `Correct or remove records that retrieval surfaces for ${projectLabel}.`
            : "Correct or remove records that retrieval surfaces for this project."
        }
        actions={
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            onClick={() => void loadRecords()}
            disabled={isLoading || !adapter}
            aria-label="Refresh project records"
          >
            {isLoading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
        }
      />

      {records.length > 0 ? (
        <InlineCounts
          items={[
            { label: "Total", value: counts.total },
            { label: "Current", value: counts.current, tone: "good" },
            { label: "Stale", value: counts.stale, tone: counts.stale > 0 ? "warn" : "neutral" },
            {
              label: "Superseded",
              value: counts.superseded,
              tone: counts.superseded > 0 ? "info" : "neutral",
            },
            ...(counts.blocked > 0
              ? [{ label: "Blocked", value: counts.blocked, tone: "bad" as Tone }]
              : []),
          ]}
        />
      ) : null}

      {errorMessage ? <ErrorBanner message={errorMessage} /> : null}

      {isLoading && records.length === 0 ? (
        <div
          aria-busy="true"
          aria-label="Loading project records"
          className="flex min-h-[160px] flex-col gap-2"
        >
          <div className="h-14 rounded-md bg-secondary/40" />
          <div className="h-14 rounded-md bg-secondary/30" />
          <div className="h-14 rounded-md bg-secondary/20" />
        </div>
      ) : null}

      {status === "ready" && records.length === 0 ? (
        <EmptyPanel
          icon={<FileText className="h-4 w-4 text-muted-foreground/70" />}
          title="No project records"
          body="Agent runs will populate this list as they produce findings, plans, and decisions."
        />
      ) : null}

      {records.length > 0 ? (
        <section className="flex flex-col gap-2.5">
          <SubHeading count={records.length}>Records</SubHeading>
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
            {records.map((record) => {
              const successor = record.supersededById
                ? (recordsById.get(record.supersededById) ?? null)
                : null
              const predecessor = record.supersedesId
                ? (recordsById.get(record.supersedesId) ?? null)
                : null
              const isSuperseded = record.supersededById !== null
              const pending = pendingRecordId === record.recordId
              return (
                <ProjectRecordRow
                  key={record.recordId}
                  record={record}
                  predecessor={predecessor}
                  successor={successor}
                  isSuperseded={isSuperseded}
                  pending={pending}
                  canMutate={Boolean(adapter)}
                  onSupersede={() =>
                    setSupersedeDialog({
                      open: true,
                      superseded: record,
                      supersedingId: "",
                      error: null,
                    })
                  }
                  onDelete={() => setDeleteTarget(record)}
                />
              )
            })}
          </div>
        </section>
      ) : null}

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 text-destructive" />
              Delete project record
            </DialogTitle>
            <DialogDescription>
              Retrieval will stop surfacing this record. Workflow runs that already used it remain
              pinned to the prior version.
            </DialogDescription>
          </DialogHeader>
          {deleteTarget ? (
            <div className="space-y-1.5 rounded-md border border-border/60 bg-secondary/20 px-3 py-2 text-[12.5px]">
              <p className="font-medium text-foreground">{deleteTarget.title}</p>
              <p className="font-mono text-[11.5px] text-muted-foreground">
                {deleteTarget.recordId}
              </p>
            </div>
          ) : null}
          <DialogFooter>
            <Button
              variant="ghost"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={() => setDeleteTarget(null)}
              disabled={!!pendingRecordId}
            >
              <X className="h-3.5 w-3.5" />
              Cancel
            </Button>
            <Button
              variant="destructive"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={() => void handleDelete()}
              disabled={!!pendingRecordId}
            >
              {pendingRecordId === deleteTarget?.recordId ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Trash2 className="h-3.5 w-3.5" />
              )}
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={supersedeDialog.open}
        onOpenChange={(open) => {
          if (!open) setSupersedeDialog(INITIAL_SUPERSEDE_DIALOG)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ArrowRight className="h-4 w-4" />
              Supersede project record
            </DialogTitle>
            <DialogDescription>
              Retrieval will prefer the superseding record and treat this one as historical.
            </DialogDescription>
          </DialogHeader>
          {supersedeDialog.superseded ? (
            <div className="space-y-3 text-[12.5px]">
              <div className="space-y-1.5 rounded-md border border-border/60 bg-secondary/20 px-3 py-2">
                <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground/80">
                  Replacing
                </p>
                <p className="font-medium text-foreground">{supersedeDialog.superseded.title}</p>
                <p className="font-mono text-[11.5px] text-muted-foreground">
                  {supersedeDialog.superseded.recordId}
                </p>
              </div>
              <div className="space-y-1.5">
                <label
                  htmlFor="supersede-record-id"
                  className="block text-[12px] font-medium text-foreground"
                >
                  Superseding record id
                </label>
                <Input
                  id="supersede-record-id"
                  value={supersedeDialog.supersedingId}
                  placeholder="project-record-..."
                  className="font-mono text-[12px]"
                  onChange={(event) =>
                    setSupersedeDialog((current) => ({
                      ...current,
                      supersedingId: event.target.value,
                      error: null,
                    }))
                  }
                />
              </div>
              {supersedeDialog.error ? (
                <p className="flex items-start gap-1.5 text-[12px] text-destructive">
                  <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
                  {supersedeDialog.error}
                </p>
              ) : null}
            </div>
          ) : null}
          <DialogFooter>
            <Button
              variant="ghost"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={() => setSupersedeDialog(INITIAL_SUPERSEDE_DIALOG)}
              disabled={!!pendingRecordId}
            >
              <X className="h-3.5 w-3.5" />
              Cancel
            </Button>
            <Button
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={() => void handleSupersede()}
              disabled={!!pendingRecordId}
            >
              {pendingRecordId === supersedeDialog.superseded?.recordId ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Check className="h-3.5 w-3.5" />
              )}
              Supersede
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

interface ProjectRecordRowProps {
  record: ProjectContextRecordSummaryDto
  predecessor: ProjectContextRecordSummaryDto | null
  successor: ProjectContextRecordSummaryDto | null
  isSuperseded: boolean
  pending: boolean
  canMutate: boolean
  onSupersede: () => void
  onDelete: () => void
}

function ProjectRecordRow({
  record,
  predecessor,
  successor,
  isSuperseded,
  pending,
  canMutate,
  onSupersede,
  onDelete,
}: ProjectRecordRowProps) {
  const isBlocked = record.redactionState === "blocked"
  const Icon = RECORD_KIND_ICON[record.recordKind] ?? FileText
  const showImportance = record.importance === "high" || record.importance === "critical"
  const hasChain =
    record.supersedesId !== null ||
    record.supersededById !== null ||
    predecessor !== null ||
    successor !== null

  return (
    <div
      data-record-id={record.recordId}
      data-superseded={isSuperseded ? "true" : "false"}
      className={cn(
        "group flex flex-col gap-2 px-3.5 py-3 transition-opacity",
        isSuperseded && "opacity-65",
      )}
    >
      <div className="flex items-start gap-3">
        <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground">
          <Icon className="h-3.5 w-3.5" aria-hidden="true" />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[13px] font-medium text-foreground">{record.title}</p>
            <Pill tone="neutral">{record.recordKind.replace(/_/g, " ")}</Pill>
            <Pill tone={FRESHNESS_TONE[record.freshnessState] ?? "neutral"}>
              {record.freshnessState}
            </Pill>
            {showImportance ? (
              <Pill tone={IMPORTANCE_TONE[record.importance] ?? "neutral"}>{record.importance}</Pill>
            ) : null}
            {isBlocked ? (
              <Pill tone="bad">
                <ShieldAlert className="h-2.5 w-2.5" />
                redacted
              </Pill>
            ) : null}
          </div>
          {record.summary ? (
            <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">
              {record.summary}
            </p>
          ) : isBlocked ? (
            <p className="mt-1 flex items-center gap-1.5 text-[12px] text-muted-foreground">
              <EyeOff className="h-3.5 w-3.5" />
              Content withheld — redacted record.
            </p>
          ) : null}
          <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-muted-foreground/80">
            <span className="font-mono">{record.recordId}</span>
            {record.relatedPaths.length > 0 ? (
              <>
                <span aria-hidden>·</span>
                <span className="truncate font-mono">
                  {record.relatedPaths.slice(0, 2).join(", ")}
                  {record.relatedPaths.length > 2
                    ? ` +${record.relatedPaths.length - 2}`
                    : ""}
                </span>
              </>
            ) : null}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-0.5">
          {canMutate ? (
            <>
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7 text-muted-foreground/70 opacity-0 transition-opacity hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
                aria-label="Supersede"
                title="Supersede"
                onClick={onSupersede}
                disabled={pending || isSuperseded}
              >
                {pending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <ArrowRight className="h-3.5 w-3.5" />
                )}
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7 text-muted-foreground/70 opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
                aria-label="Delete"
                title="Delete"
                onClick={onDelete}
                disabled={pending}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </>
          ) : null}
        </div>
      </div>

      {hasChain && (record.supersedesId || record.supersededById) ? (
        <div
          data-testid={`supersede-chain-${record.recordId}`}
          className="ml-9 flex flex-col gap-1 rounded-md border border-border/40 bg-secondary/20 px-2.5 py-1.5"
        >
          {record.supersedesId ? (
            <ChainRow
              direction="from"
              label="Supersedes"
              recordId={record.supersedesId}
              title={predecessor?.title ?? null}
            />
          ) : null}
          {record.supersededById ? (
            <ChainRow
              direction="to"
              label="Superseded by"
              recordId={record.supersededById}
              title={successor?.title ?? null}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

interface ChainRowProps {
  direction: "from" | "to"
  label: string
  recordId: string
  title: string | null
}

function ChainRow({ direction, label, recordId, title }: ChainRowProps) {
  return (
    <div
      data-direction={direction}
      className="flex items-center gap-2 text-[11px] text-muted-foreground"
    >
      <Pill tone="neutral">{label}</Pill>
      <span className="truncate font-mono">{recordId}</span>
      {title ? <span className="truncate text-foreground/80">· {title}</span> : null}
    </div>
  )
}

function toErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
