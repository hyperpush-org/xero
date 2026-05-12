import { useCallback, useEffect, useState } from "react"
import {
  AlertTriangle,
  Brain,
  Check,
  EyeOff,
  Loader2,
  Pencil,
  PowerOff,
  RefreshCw,
  ShieldAlert,
  Trash2,
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

const REVIEW_STATE_TONE: Record<MemoryReviewItem["reviewState"], Tone> = {
  candidate: "info",
  approved: "good",
  rejected: "bad",
}

export function MemoryReviewSection({
  projectId,
  projectLabel,
  agentSessionId,
  adapter,
}: MemoryReviewSectionProps) {
  const [queueState, setQueueState] = useState<QueueState>(INITIAL_QUEUE_STATE)
  const [pendingAction, setPendingAction] = useState<{ memoryId: string; kind: ActionKind } | null>(null)
  const [editing, setEditing] = useState<MemoryReviewItem | null>(null)
  const [editText, setEditText] = useState("")
  const [editError, setEditError] = useState<string | null>(null)

  const loadQueue = useCallback(async () => {
    if (!projectId || !adapter) return
    setQueueState((current) => ({ ...current, status: "loading", errorMessage: null }))
    try {
      const response = await adapter.getQueue({
        projectId,
        agentSessionId: agentSessionId ?? null,
        limit: 50,
      })
      setQueueState({ status: "ready", errorMessage: null, response })
    } catch (caught) {
      setQueueState((current) => ({
        ...current,
        status: "error",
        errorMessage: errorMessage(caught, "Xero could not load the memory review queue."),
      }))
    }
  }, [adapter, agentSessionId, projectId])

  useEffect(() => {
    void loadQueue()
  }, [loadQueue])

  const items = queueState.response?.items ?? []
  const counts = queueState.response?.counts

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
        await loadQueue()
      } catch (caught) {
        setQueueState((current) => ({
          ...current,
          errorMessage: errorMessage(caught, "Xero could not update this memory."),
        }))
      } finally {
        setPendingAction(null)
      }
    },
    [adapter, loadQueue, projectId],
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
        await loadQueue()
      } catch (caught) {
        setQueueState((current) => ({
          ...current,
          errorMessage: errorMessage(caught, "Xero could not delete this memory."),
        }))
      } finally {
        setPendingAction(null)
      }
    },
    [adapter, loadQueue, projectId],
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
      await loadQueue()
    } catch (caught) {
      setEditError(errorMessage(caught, "Xero could not save the corrected memory."))
    } finally {
      setPendingAction(null)
    }
  }, [adapter, closeEditor, editText, editing, loadQueue, projectId])

  const isBusy = queueState.status === "loading"

  const headerActions = (
    <Button
      size="sm"
      variant="outline"
      className="h-8 gap-1.5 text-[12.5px]"
      onClick={() => void loadQueue()}
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

      {counts ? (
        <div
          data-testid="memory-review-counts"
          className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-5"
        >
          <CountCard label="Candidates" value={counts.candidate} tone="info" />
          <CountCard label="Approved" value={counts.approved} tone="good" />
          <CountCard label="Retrievable" value={counts.retrievableApproved} tone="good" />
          <CountCard label="Disabled" value={counts.disabled} />
          <CountCard label="Rejected" value={counts.rejected} tone="warn" />
        </div>
      ) : null}

      {queueState.errorMessage ? <ErrorBanner message={queueState.errorMessage} /> : null}

      {queueState.status === "loading" && items.length === 0 ? (
        <div
          aria-busy="true"
          aria-label="Loading memory review queue"
          className="flex min-h-[200px] flex-col gap-2.5"
        >
          <div className="h-16 rounded-lg bg-secondary/40" />
          <div className="h-16 rounded-lg bg-secondary/30" />
          <div className="h-16 rounded-lg bg-secondary/20" />
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
          <SubHeading count={items.length}>Candidates</SubHeading>
          <div className="overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40">
            {items.map((item) => (
              <MemoryReviewRow
                key={item.memoryId}
                item={item}
                pendingKind={pendingAction?.memoryId === item.memoryId ? pendingAction.kind : null}
                onApprove={() => void handleApprove(item)}
                onReject={() => void handleReject(item)}
                onDisable={() => void handleDisable(item)}
                onDelete={() => void handleDelete(item)}
                onEdit={() => openEditor(item)}
              />
            ))}
          </div>
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
  pendingKind: ActionKind | null
  onApprove: () => void
  onReject: () => void
  onDisable: () => void
  onDelete: () => void
  onEdit: () => void
}

function MemoryReviewRow({
  item,
  pendingKind,
  onApprove,
  onReject,
  onDisable,
  onDelete,
  onEdit,
}: MemoryReviewRowProps) {
  const redacted = item.redaction.textPreviewRedacted
  const factKeyRedacted = item.redaction.factKeyRedacted
  const freshnessReason = freshnessExplanation(item)
  const ariaBusy = pendingKind !== null

  return (
    <div
      data-testid="memory-review-item"
      data-memory-id={item.memoryId}
      aria-busy={ariaBusy}
      className={cn(
        "flex flex-col gap-3 px-4 py-3.5",
        redacted && "bg-warning/[0.04]",
      )}
    >
      <div className="flex items-start gap-3">
        <div className="mt-[1px] flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground">
          <Brain className="h-4 w-4" aria-hidden="true" />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <Pill tone="neutral">{item.scope}</Pill>
            <Pill tone="neutral">{item.kind.replace(/_/g, " ")}</Pill>
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
            {item.confidence != null ? (
              <Pill tone="neutral">{item.confidence}%</Pill>
            ) : null}
            <span className="ml-auto text-[12px] text-muted-foreground">
              {formatTimestamp(item.updatedAt)}
            </span>
          </div>

          {redacted ? (
            <p
              data-testid="memory-redacted-notice"
              className="mt-2.5 flex items-start gap-2 rounded-md border border-warning/30 bg-warning/[0.06] px-3 py-2 text-[12.5px] leading-[1.5] text-warning"
            >
              <EyeOff className="mt-[1px] h-4 w-4 shrink-0" />
              <span>
                Xero hid this memory's preview because it contains secret-shaped content. Edit it to a sanitized form before
                approving.
              </span>
            </p>
          ) : (
            <p
              data-testid="memory-preview"
              className="mt-2.5 whitespace-pre-wrap text-[13px] leading-[1.55] text-foreground"
            >
              {item.textPreview}
            </p>
          )}

          <dl className="mt-2.5 grid grid-cols-1 gap-x-4 gap-y-1 text-[12px] text-muted-foreground sm:grid-cols-2">
            <div className="flex items-center gap-1.5">
              <span className="text-muted-foreground/60">Freshness</span>
              <span className="capitalize text-foreground/80">
                {item.freshness.state.replace(/_/g, " ")}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="text-muted-foreground/60">Retrieval</span>
              <span className="text-foreground/80">
                {item.retrieval.eligible ? "Eligible" : reasonLabel(item.retrieval.reason)}
              </span>
            </div>
            {freshnessReason ? (
              <div className="col-span-full flex items-start gap-1.5">
                <AlertTriangle className="mt-[2px] h-3.5 w-3.5 shrink-0 text-warning" />
                <span>{freshnessReason}</span>
              </div>
            ) : null}
            {factKeyRedacted ? (
              <div className="col-span-full">
                Fact key redacted in this preview.
              </div>
            ) : null}
            {item.provenance.diagnostic ? (
              <div className="col-span-full">
                <span className="text-muted-foreground/60">Diagnostic </span>
                <span className="text-foreground/80">{item.provenance.diagnostic.message}</span>
              </div>
            ) : null}
          </dl>
        </div>
      </div>

      <div className="ml-11 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="default"
          className="h-8 gap-1.5 text-[12px]"
          onClick={onApprove}
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
        <Button
          size="sm"
          variant="outline"
          className="h-8 gap-1.5 text-[12px]"
          onClick={onReject}
          disabled={ariaBusy || !item.availableActions.canReject}
          aria-label="Reject memory"
        >
          {pendingKind === "reject" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <X className="h-3.5 w-3.5" />
          )}
          Reject
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-8 gap-1.5 text-[12px]"
          onClick={onDisable}
          disabled={ariaBusy || !item.availableActions.canDisable}
          aria-label="Disable memory"
        >
          {pendingKind === "disable" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <PowerOff className="h-3.5 w-3.5" />
          )}
          Disable
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-8 gap-1.5 text-[12px]"
          onClick={onEdit}
          disabled={ariaBusy || !item.availableActions.canEditByCorrection}
          aria-label="Edit memory"
        >
          {pendingKind === "edit" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Pencil className="h-3.5 w-3.5" />
          )}
          Edit
        </Button>
        <Button
          size="sm"
          variant="ghost"
          className="ml-auto h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-destructive"
          onClick={onDelete}
          disabled={ariaBusy || !item.availableActions.canDelete}
          aria-label="Delete memory"
        >
          {pendingKind === "delete" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Trash2 className="h-3.5 w-3.5" />
          )}
          Delete
        </Button>
      </div>
    </div>
  )
}

const COUNT_TONE: Record<Tone, string> = {
  good: "text-success",
  info: "text-info",
  warn: "text-warning",
  bad: "text-destructive",
  neutral: "text-foreground",
}

const COUNT_DOT: Record<Tone, string> = {
  good: "bg-success",
  info: "bg-info",
  warn: "bg-warning",
  bad: "bg-destructive",
  neutral: "bg-muted-foreground/50",
}

function CountCard({
  label,
  value,
  tone = "neutral",
}: {
  label: string
  value: number
  tone?: Tone
}) {
  const isZero = value === 0
  return (
    <div
      aria-label={`${label}: ${value}`}
      className="flex flex-col gap-2 rounded-lg border border-border/60 bg-secondary/10 px-3.5 py-3"
    >
      <div className="flex items-center gap-2">
        <span
          className={cn(
            "h-2 w-2 shrink-0 rounded-full",
            isZero ? "bg-muted-foreground/30" : COUNT_DOT[tone],
          )}
          aria-hidden
        />
        <span className="truncate text-[12px] font-medium text-muted-foreground">{label}</span>
      </div>
      <span
        className={cn(
          "text-[22px] font-semibold tabular-nums leading-none tracking-tight",
          isZero ? "text-foreground/40" : COUNT_TONE[tone],
        )}
      >
        {value}
      </span>
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

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
