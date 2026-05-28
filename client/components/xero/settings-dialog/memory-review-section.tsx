import { memo, useCallback, useEffect, useState } from "react"
import {
  AlertTriangle,
  Brain,
  Check,
  ChevronDown,
  EyeOff,
  Loader2,
  MoreHorizontal,
  Pencil,
  PowerOff,
  RefreshCw,
  ShieldAlert,
  Trash2,
  X,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from "@/components/ui/pagination"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"
import type {
  AgentMemoryReviewQueueItemDto,
  CorrectSessionMemoryRequestDto,
  CorrectSessionMemoryResponseDto,
  DeleteSessionMemoryRequestDto,
  GetSessionMemoryReviewQueueRequestDto,
  GetSessionMemoryReviewQueueResponseDto,
  SessionMemoryRecordDto,
  UpdateSessionMemoryRequestDto,
} from "@/src/lib/xero-model/session-context"

import { SectionHeader } from "./section-header"
import { EmptyPanel, ErrorBanner, Pill, SubHeading, type Tone } from "./_shared"

type MemoryReviewItem = AgentMemoryReviewQueueItemDto

export interface MemoryReviewAdapter {
  getQueue: (request: GetSessionMemoryReviewQueueRequestDto) => Promise<GetSessionMemoryReviewQueueResponseDto>
  updateMemory: (request: UpdateSessionMemoryRequestDto) => Promise<SessionMemoryRecordDto>
  correctMemory: (request: CorrectSessionMemoryRequestDto) => Promise<CorrectSessionMemoryResponseDto>
  deleteMemory: (request: DeleteSessionMemoryRequestDto) => Promise<void>
}

interface MemoryReviewSectionProps {
  projectId: string | null
  projectLabel: string | null
  agentSessionId?: string | null
  adapter?: MemoryReviewAdapter | null
}

type LoadStatus = "idle" | "loading" | "ready" | "error"
type ActionKind = "approve" | "reject" | "disable" | "delete" | "edit"

interface QueueState {
  status: LoadStatus
  errorMessage: string | null
  response: GetSessionMemoryReviewQueueResponseDto | null
}

const INITIAL_QUEUE_STATE: QueueState = {
  status: "idle",
  errorMessage: null,
  response: null,
}

const MEMORY_REVIEW_PAGE_SIZE = 10

const REVIEW_STATE_TONE: Record<MemoryReviewItem["reviewState"], Tone> = {
  candidate: "info",
  approved: "good",
  rejected: "bad",
}

const METRIC_TONE: Record<Tone, string> = {
  good: "text-success",
  info: "text-info",
  warn: "text-warning",
  bad: "text-destructive",
  neutral: "text-foreground",
}

const METRIC_DOT: Record<Tone, string> = {
  good: "bg-success",
  info: "bg-info",
  warn: "bg-warning",
  bad: "bg-destructive",
  neutral: "bg-muted-foreground/50",
}

export function MemoryReviewSection({
  projectId,
  projectLabel,
  agentSessionId,
  adapter,
}: MemoryReviewSectionProps) {
  const [queueState, setQueueState] = useState<QueueState>(INITIAL_QUEUE_STATE)
  const [pageOffset, setPageOffset] = useState(0)
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set())
  const [pendingAction, setPendingAction] = useState<{ memoryId: string; kind: ActionKind } | null>(null)
  const [editing, setEditing] = useState<MemoryReviewItem | null>(null)
  const [editText, setEditText] = useState("")
  const [editError, setEditError] = useState<string | null>(null)

  const loadQueue = useCallback(
    async (requestedOffset: number) => {
      if (!projectId || !adapter) return
      setQueueState((current) => ({ ...current, status: "loading", errorMessage: null }))
      try {
        const response = await adapter.getQueue({
          projectId,
          agentSessionId: agentSessionId ?? null,
          offset: requestedOffset,
          limit: MEMORY_REVIEW_PAGE_SIZE,
        })
        setQueueState({ status: "ready", errorMessage: null, response })
        if (response.offset !== requestedOffset) {
          setPageOffset(response.offset)
        }
      } catch (caught) {
        setQueueState((current) => ({
          ...current,
          status: "error",
          errorMessage: errorMessage(caught, "Xero could not load the memory review queue."),
        }))
      }
    },
    [adapter, agentSessionId, projectId],
  )

  useEffect(() => {
    setPageOffset(0)
    setExpandedIds(new Set())
  }, [agentSessionId, projectId])

  useEffect(() => {
    void loadQueue(pageOffset)
  }, [loadQueue, pageOffset])

  const items = queueState.response?.items ?? []
  const counts = queueState.response?.counts
  const totalItems = queueState.response?.total ?? items.length
  const responseOffset = queueState.response?.offset ?? pageOffset
  const pageStart = totalItems === 0 ? 0 : responseOffset + 1
  const pageEnd = totalItems === 0 ? 0 : responseOffset + items.length
  const pageCount = Math.max(1, Math.ceil(totalItems / MEMORY_REVIEW_PAGE_SIZE))
  const pageNumber = Math.floor(responseOffset / MEMORY_REVIEW_PAGE_SIZE) + 1
  const pageSummary = totalItems === 0 ? "0" : `${pageStart}-${pageEnd} of ${totalItems}`

  const runUpdate = useCallback(
    async (item: MemoryReviewItem, kind: ActionKind, payload: Omit<UpdateSessionMemoryRequestDto, "projectId" | "memoryId">) => {
      if (!projectId || !adapter) return
      setPendingAction({ memoryId: item.memoryId, kind })
      try {
        await adapter.updateMemory({
          projectId,
          memoryId: item.memoryId,
          ...payload,
        })
        await loadQueue(pageOffset)
      } catch (caught) {
        setQueueState((current) => ({
          ...current,
          errorMessage: errorMessage(caught, "Xero could not update this memory."),
        }))
      } finally {
        setPendingAction(null)
      }
    },
    [adapter, loadQueue, pageOffset, projectId],
  )

  const handleApprove = useCallback(
    (item: MemoryReviewItem) => runUpdate(item, "approve", { reviewState: "approved", enabled: true }),
    [runUpdate],
  )

  const handleReject = useCallback(
    (item: MemoryReviewItem) => runUpdate(item, "reject", { reviewState: "rejected" }),
    [runUpdate],
  )

  const handleDisable = useCallback(
    (item: MemoryReviewItem) => runUpdate(item, "disable", { enabled: false }),
    [runUpdate],
  )

  const handleDelete = useCallback(
    async (item: MemoryReviewItem) => {
      if (!projectId || !adapter) return
      setPendingAction({ memoryId: item.memoryId, kind: "delete" })
      try {
        await adapter.deleteMemory({ projectId, memoryId: item.memoryId })
        const nextOffset =
          items.length === 1 && pageOffset > 0 ? Math.max(0, pageOffset - MEMORY_REVIEW_PAGE_SIZE) : pageOffset
        if (nextOffset !== pageOffset) {
          setPageOffset(nextOffset)
        } else {
          await loadQueue(nextOffset)
        }
      } catch (caught) {
        setQueueState((current) => ({
          ...current,
          errorMessage: errorMessage(caught, "Xero could not delete this memory."),
        }))
      } finally {
        setPendingAction(null)
      }
    },
    [adapter, items.length, loadQueue, pageOffset, projectId],
  )

  const openEditor = useCallback((item: MemoryReviewItem) => {
    setEditing(item)
    setEditText(item.textPreview ?? "")
    setEditError(null)
  }, [])

  const closeEditor = useCallback(() => {
    setEditing(null)
    setEditText("")
    setEditError(null)
  }, [])

  const submitCorrection = useCallback(async () => {
    if (!projectId || !adapter || !editing) return
    const corrected = editText.trim()
    if (!corrected) {
      setEditError("Corrected text must not be empty.")
      return
    }
    setPendingAction({ memoryId: editing.memoryId, kind: "edit" })
    try {
      await adapter.correctMemory({
        projectId,
        memoryId: editing.memoryId,
        correctedText: corrected,
      })
      closeEditor()
      await loadQueue(pageOffset)
    } catch (caught) {
      setEditError(errorMessage(caught, "Xero could not save the corrected memory."))
    } finally {
      setPendingAction(null)
    }
  }, [adapter, closeEditor, editText, editing, loadQueue, pageOffset, projectId])

  const handleAction = useCallback(
    (item: MemoryReviewItem, kind: ActionKind) => {
      if (kind === "approve") {
        void handleApprove(item)
      } else if (kind === "reject") {
        void handleReject(item)
      } else if (kind === "disable") {
        void handleDisable(item)
      } else if (kind === "delete") {
        void handleDelete(item)
      } else {
        openEditor(item)
      }
    },
    [handleApprove, handleDelete, handleDisable, handleReject, openEditor],
  )

  const handleExpandedChange = useCallback((memoryId: string, open: boolean) => {
    setExpandedIds((current) => {
      const next = new Set(current)
      if (open) {
        next.add(memoryId)
      } else {
        next.delete(memoryId)
      }
      return next
    })
  }, [])

  const handlePageChange = useCallback((offset: number) => {
    setExpandedIds(new Set())
    setPageOffset(offset)
  }, [])

  const isBusy = queueState.status === "loading"

  const headerActions = (
    <Button
      size="sm"
      variant="outline"
      className="h-8 gap-1.5 text-[12.5px]"
      onClick={() => void loadQueue(pageOffset)}
      disabled={isBusy || !projectId || !adapter}
      aria-label="Refresh memory review queue"
    >
      {isBusy ? (
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
      ) : (
        <RefreshCw className="h-3.5 w-3.5" />
      )}
      Refresh
    </Button>
  )

  if (!projectId) {
    return (
      <div className="flex flex-col gap-6">
        <SectionHeader
          title="Memory Review"
          description="Approve, edit, or reject memory candidates extracted from agent sessions."
        />
        <EmptyPanel
          icon={<Brain className="h-5 w-5 text-muted-foreground/70" />}
          title="Select a project"
          body="Memory review is scoped to the active project."
        />
      </div>
    )
  }

  if (!adapter) {
    return (
      <div className="flex flex-col gap-6">
        <SectionHeader
          title="Memory Review"
          description={
            projectLabel ? `Memory candidates for ${projectLabel}.` : "Memory candidates for the active project."
          }
          actions={headerActions}
        />
        <EmptyPanel
          icon={<Brain className="h-5 w-5 text-muted-foreground/70" />}
          title="Memory review unavailable"
          body="The desktop adapter did not provide memory review commands. Restart Xero or upgrade to enable this surface."
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Memory Review"
        description={
          projectLabel
            ? `Memory candidates extracted from agent sessions for ${projectLabel}.`
            : "Memory candidates extracted from agent sessions."
        }
        actions={headerActions}
      />

      {counts ? <MemoryMetricStrip counts={counts} /> : null}

      {queueState.errorMessage ? <ErrorBanner message={queueState.errorMessage} /> : null}

      {queueState.status === "loading" && items.length === 0 ? (
        <div
          aria-busy="true"
          aria-label="Loading memory review queue"
          className="flex min-h-[200px] flex-col gap-2.5"
        >
          <div className="h-[76px] rounded-lg border border-border/40 bg-secondary/40" />
          <div className="h-[76px] rounded-lg border border-border/30 bg-secondary/30" />
          <div className="h-[76px] rounded-lg border border-border/20 bg-secondary/20" />
        </div>
      ) : null}

      {queueState.status === "ready" && items.length === 0 ? (
        <EmptyPanel
          icon={<Brain className="h-5 w-5 text-muted-foreground/70" />}
          title="No memory to review"
          body="Memory candidates appear here when agent sessions complete, pause, fail, or hand off."
        />
      ) : null}

      {items.length > 0 ? (
        <section className="flex flex-col gap-3" data-testid="memory-review-items">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <SubHeading count={pageSummary}>Memory</SubHeading>
            <span className="text-[12px] text-muted-foreground">
              Page {pageNumber} of {pageCount}
            </span>
          </div>
          <div className="grid gap-2.5">
            {items.map((item) => (
              <MemoryReviewRow
                key={item.memoryId}
                item={item}
                expanded={expandedIds.has(item.memoryId)}
                pendingKind={pendingAction?.memoryId === item.memoryId ? pendingAction.kind : null}
                onAction={handleAction}
                onExpandedChange={handleExpandedChange}
              />
            ))}
          </div>
          <MemoryReviewPager
            offset={responseOffset}
            total={totalItems}
            pageSize={MEMORY_REVIEW_PAGE_SIZE}
            disabled={isBusy}
            onPageChange={handlePageChange}
          />
        </section>
      ) : null}

      <Dialog open={editing !== null} onOpenChange={(open) => (open ? null : closeEditor())}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="text-[15px] font-semibold tracking-tight">Correct memory</DialogTitle>
            <DialogDescription className="text-[12.5px] leading-[1.55]">
              Submitting a correction creates a new approved memory that cites this one. The original record stays in the audit
              trail.
            </DialogDescription>
          </DialogHeader>
          <Textarea
            aria-label="Corrected memory text"
            value={editText}
            onChange={(event) => {
              setEditText(event.target.value)
              if (editError) setEditError(null)
            }}
            rows={6}
            className="font-mono text-[12.5px]"
          />
          {editError ? (
            <p role="alert" className="text-[12.5px] text-destructive">
              {editError}
            </p>
          ) : null}
          <DialogFooter>
            <Button
              variant="ghost"
              size="sm"
              className="h-9 gap-1.5 text-[12.5px]"
              onClick={closeEditor}
              disabled={pendingAction?.kind === "edit"}
            >
              <X className="h-3.5 w-3.5" />
              Cancel
            </Button>
            <Button
              size="sm"
              className="h-9 gap-1.5 text-[12.5px]"
              onClick={() => void submitCorrection()}
              disabled={pendingAction?.kind === "edit"}
            >
              {pendingAction?.kind === "edit" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Check className="h-3.5 w-3.5" />
              )}
              Save correction
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

interface MemoryReviewRowProps {
  item: MemoryReviewItem
  expanded: boolean
  pendingKind: ActionKind | null
  onAction: (item: MemoryReviewItem, kind: ActionKind) => void
  onExpandedChange: (memoryId: string, open: boolean) => void
}

const MemoryReviewRow = memo(function MemoryReviewRow({
  item,
  expanded,
  pendingKind,
  onAction,
  onExpandedChange,
}: MemoryReviewRowProps) {
  const redacted = item.redaction.textPreviewRedacted
  const factKeyRedacted = item.redaction.factKeyRedacted
  const freshnessReason = freshnessExplanation(item)
  const ariaBusy = pendingKind !== null
  const canShowPrimaryApprove = item.reviewState !== "approved"
  const retrievalLabel = item.retrieval.eligible ? "Eligible" : reasonLabel(item.retrieval.reason)

  return (
    <Collapsible open={expanded} onOpenChange={(open) => onExpandedChange(item.memoryId, open)}>
      <article
        data-testid="memory-review-item"
        data-memory-id={item.memoryId}
        aria-busy={ariaBusy}
        className={cn(
          "overflow-hidden rounded-lg border border-border/60 bg-card/25 shadow-xs [contain-intrinsic-size:96px] [content-visibility:auto]",
          redacted && "border-warning/30 bg-warning/[0.035]",
        )}
      >
        <div className="flex items-start gap-2 p-3.5">
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="group flex min-w-0 flex-1 items-start gap-3 rounded-md text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50"
              aria-label={`Toggle memory details for ${item.memoryId}`}
            >
              <span className="mt-[1px] flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground transition-colors group-hover:text-foreground">
                <Brain className="h-4 w-4" aria-hidden="true" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <Pill tone="neutral">{item.scope}</Pill>
                  <Pill tone="neutral">{formatEnum(item.kind)}</Pill>
                  <Pill tone={REVIEW_STATE_TONE[item.reviewState]}>{item.reviewState}</Pill>
                  {!item.enabled ? <Pill tone="neutral">Disabled</Pill> : null}
                  {redacted ? (
                    <Pill tone="warn" className="gap-1">
                      <span data-testid="redaction-badge" className="inline-flex items-center gap-1">
                        <ShieldAlert className="h-3 w-3" />
                        Redacted
                      </span>
                    </Pill>
                  ) : null}
                  {item.confidence != null ? <Pill tone="neutral">{item.confidence}%</Pill> : null}
                </span>

                {redacted ? (
                  <span
                    data-testid="memory-redacted-notice"
                    className="mt-2 flex items-start gap-2 rounded-md border border-warning/30 bg-warning/[0.06] px-3 py-2 text-[12px] leading-[1.45] text-warning"
                  >
                    <EyeOff className="mt-[1px] h-3.5 w-3.5 shrink-0" />
                    <span>Preview hidden because this memory contains secret-shaped content.</span>
                  </span>
                ) : (
                  <span
                    data-testid="memory-preview"
                    className="mt-2 block whitespace-pre-wrap text-[13px] leading-[1.5] text-foreground line-clamp-2"
                  >
                    {item.textPreview}
                  </span>
                )}

                <span className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11.5px] text-muted-foreground">
                  <span>Updated {formatTimestamp(item.updatedAt)}</span>
                  <span>Freshness {formatEnum(item.freshness.state)}</span>
                  <span>Retrieval {retrievalLabel}</span>
                </span>
              </span>
              <ChevronDown
                className={cn(
                  "mt-1 h-4 w-4 shrink-0 text-muted-foreground transition-transform",
                  expanded && "rotate-180 text-foreground",
                )}
                aria-hidden="true"
              />
            </button>
          </CollapsibleTrigger>

          <div className="flex shrink-0 items-center gap-1.5">
            {canShowPrimaryApprove ? (
              <Button
                size="sm"
                variant="default"
                className="h-8 gap-1.5 text-[12px]"
                onClick={() => onAction(item, "approve")}
                disabled={ariaBusy || !item.availableActions.canApprove}
                aria-label="Approve memory"
              >
                {pendingKind === "approve" ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Check className="h-3.5 w-3.5" />
                )}
                Approve
              </Button>
            ) : null}
            <MemoryActionsMenu item={item} pendingKind={pendingKind} onAction={onAction} />
          </div>
        </div>

        <CollapsibleContent>
          <div className="border-t border-border/50 px-4 pb-4 pt-3">
            <div className="ml-0 flex flex-col gap-3 sm:ml-11">
              {!redacted ? (
                <div className="rounded-md border border-border/50 bg-secondary/20 px-3 py-2.5">
                  <p className="mb-1 text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70">
                    Full preview
                  </p>
                  <p
                    data-testid="memory-full-preview"
                    className="whitespace-pre-wrap text-[12.5px] leading-[1.55] text-foreground"
                  >
                    {item.textPreview}
                  </p>
                </div>
              ) : null}

              <dl className="grid grid-cols-1 gap-2.5 text-[12px] sm:grid-cols-2">
                <MemoryDetail label="Freshness" value={formatEnum(item.freshness.state)} />
                <MemoryDetail label="Retrieval" value={retrievalLabel} />
                <MemoryDetail label="Created" value={formatTimestamp(item.createdAt)} />
                <MemoryDetail label="Updated" value={formatTimestamp(item.updatedAt)} />
                <MemoryDetail label="Source run" value={item.provenance.sourceRunId ?? "Unknown"} />
                <MemoryDetail
                  label="Source items"
                  value={item.provenance.sourceItemIds.length > 0 ? item.provenance.sourceItemIds.join(", ") : "None"}
                />
                {item.freshness.factKey ? <MemoryDetail label="Fact key" value={item.freshness.factKey} /> : null}
                {factKeyRedacted ? <MemoryDetail label="Fact key" value="Redacted in this preview" tone="warn" /> : null}
              </dl>

              {freshnessReason ? (
                <p className="flex items-start gap-2 rounded-md border border-warning/25 bg-warning/[0.05] px-3 py-2 text-[12px] leading-[1.5] text-warning">
                  <AlertTriangle className="mt-[2px] h-3.5 w-3.5 shrink-0" />
                  <span>{freshnessReason}</span>
                </p>
              ) : null}

              {item.provenance.diagnostic ? (
                <p className="rounded-md border border-border/50 bg-secondary/20 px-3 py-2 text-[12px] leading-[1.5] text-muted-foreground">
                  <span className="font-medium text-foreground/80">Diagnostic </span>
                  {item.provenance.diagnostic.message}
                </p>
              ) : null}
            </div>
          </div>
        </CollapsibleContent>
      </article>
    </Collapsible>
  )
})

function MemoryActionsMenu({
  item,
  pendingKind,
  onAction,
}: {
  item: MemoryReviewItem
  pendingKind: ActionKind | null
  onAction: (item: MemoryReviewItem, kind: ActionKind) => void
}) {
  const ariaBusy = pendingKind !== null

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="icon-sm"
          variant="ghost"
          className="text-muted-foreground hover:text-foreground"
          disabled={ariaBusy}
          aria-label="Memory actions"
          title="Memory actions"
        >
          {ariaBusy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <MoreHorizontal className="h-4 w-4" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuItem
          disabled={!item.availableActions.canApprove}
          onSelect={() => onAction(item, "approve")}
        >
          <Check className="h-4 w-4" />
          Approve memory
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!item.availableActions.canReject}
          onSelect={() => onAction(item, "reject")}
        >
          <X className="h-4 w-4" />
          Reject memory
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!item.availableActions.canDisable}
          onSelect={() => onAction(item, "disable")}
        >
          <PowerOff className="h-4 w-4" />
          Disable memory
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!item.availableActions.canEditByCorrection}
          onSelect={() => onAction(item, "edit")}
        >
          <Pencil className="h-4 w-4" />
          Edit memory
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          variant="destructive"
          disabled={!item.availableActions.canDelete}
          onSelect={() => onAction(item, "delete")}
        >
          <Trash2 className="h-4 w-4" />
          Delete memory
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function MemoryMetricStrip({ counts }: { counts: GetSessionMemoryReviewQueueResponseDto["counts"] }) {
  const metrics = [
    { label: "Candidates", value: counts.candidate, tone: "info" as Tone },
    { label: "Approved", value: counts.approved, tone: "good" as Tone },
    { label: "Retrievable", value: counts.retrievableApproved, tone: "good" as Tone },
    { label: "Disabled", value: counts.disabled, tone: "neutral" as Tone },
    { label: "Rejected", value: counts.rejected, tone: "warn" as Tone },
  ]

  return (
    <dl
      data-testid="memory-review-counts"
      className="grid grid-cols-2 overflow-hidden rounded-lg border border-border/60 bg-card/20 sm:grid-cols-5"
    >
      {metrics.map((metric) => {
        const isZero = metric.value === 0
        return (
          <div
            key={metric.label}
            aria-label={`${metric.label}: ${metric.value}`}
            className="flex min-w-0 items-center gap-2 border-b border-r border-border/45 px-3 py-2.5 last:border-r-0 sm:border-b-0"
          >
            <span
              className={cn(
                "h-2 w-2 shrink-0 rounded-full",
                isZero ? "bg-muted-foreground/30" : METRIC_DOT[metric.tone],
              )}
              aria-hidden="true"
            />
            <dt className="truncate text-[11.5px] font-medium text-muted-foreground">{metric.label}</dt>
            <dd
              className={cn(
                "ml-auto text-[14px] font-semibold tabular-nums",
                isZero ? "text-foreground/40" : METRIC_TONE[metric.tone],
              )}
            >
              {metric.value}
            </dd>
          </div>
        )
      })}
    </dl>
  )
}

function MemoryReviewPager({
  offset,
  total,
  pageSize,
  disabled,
  onPageChange,
}: {
  offset: number
  total: number
  pageSize: number
  disabled: boolean
  onPageChange: (offset: number) => void
}) {
  if (total <= pageSize) return null

  const page = Math.floor(offset / pageSize) + 1
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  const previousOffset = Math.max(0, offset - pageSize)
  const nextOffset = Math.min((pageCount - 1) * pageSize, offset + pageSize)
  const canPrevious = offset > 0 && !disabled
  const canNext = offset + pageSize < total && !disabled

  return (
    <Pagination className="justify-between">
      <PaginationContent className="w-full justify-between">
        <PaginationItem>
          <PaginationPrevious
            href="#"
            aria-disabled={!canPrevious}
            tabIndex={canPrevious ? undefined : -1}
            className={cn("h-8 text-[12px]", !canPrevious && "pointer-events-none opacity-50")}
            onClick={(event) => {
              event.preventDefault()
              if (canPrevious) onPageChange(previousOffset)
            }}
          />
        </PaginationItem>
        <PaginationItem>
          <span className="px-2 text-[12px] text-muted-foreground">
            Page {page} of {pageCount}
          </span>
        </PaginationItem>
        <PaginationItem>
          <PaginationNext
            href="#"
            aria-disabled={!canNext}
            tabIndex={canNext ? undefined : -1}
            className={cn("h-8 text-[12px]", !canNext && "pointer-events-none opacity-50")}
            onClick={(event) => {
              event.preventDefault()
              if (canNext) onPageChange(nextOffset)
            }}
          />
        </PaginationItem>
      </PaginationContent>
    </Pagination>
  )
}

function MemoryDetail({
  label,
  value,
  tone = "neutral",
}: {
  label: string
  value: string
  tone?: Tone
}) {
  return (
    <div className="min-w-0 rounded-md border border-border/45 bg-secondary/15 px-3 py-2">
      <dt className="text-[10.5px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70">{label}</dt>
      <dd className={cn("mt-1 truncate text-[12px] text-foreground/85", tone === "warn" && "text-warning")}>{value}</dd>
    </div>
  )
}

function reasonLabel(reason: MemoryReviewItem["retrieval"]["reason"]): string {
  switch (reason) {
    case "pending_or_rejected_review":
      return "Pending review"
    case "disabled":
      return "Disabled"
    case "superseded":
      return "Superseded"
    case "invalidated":
      return "Invalidated"
    case "stale":
      return "Stale"
    case "source_missing":
      return "Source missing"
    case "blocked":
      return "Blocked"
    case "retrievable":
      return "Eligible"
  }
}

function freshnessExplanation(item: MemoryReviewItem): string | null {
  const { freshness } = item
  if (freshness.staleReason) return freshness.staleReason
  if (freshness.state === "stale") return "Source content has changed since this memory was captured."
  if (freshness.state === "source_missing") return "Source content is no longer reachable."
  if (freshness.state === "superseded") return "A newer memory replaces this one."
  if (freshness.state === "blocked") return "Retrieval is blocked for this memory."
  return null
}

function formatTimestamp(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) return value
  return new Date(parsed).toLocaleString()
}

function formatEnum(value: string): string {
  return value.replace(/_/g, " ")
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
