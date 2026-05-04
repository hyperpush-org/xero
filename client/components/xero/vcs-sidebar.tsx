"use client"

import { memo, useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent, type MutableRefObject } from "react"
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  ChevronRight,
  GitBranch,
  GitCommit,
  Loader2,
  Minus,
  Plus,
  RefreshCw,
  RotateCcw,
  Sparkles,
  X,
} from "lucide-react"
import {
  createRepositoryStatusDiffRevision,
  type GitGenerateCommitMessageRequestDto,
  type GitGenerateCommitMessageResponseDto,
  type GitCommitResponseDto,
  type GitFetchResponseDto,
  type GitPullResponseDto,
  type GitPushResponseDto,
  type RepositoryDiffResponseDto,
  type RepositoryDiffScope,
  type RepositoryStatusEntryView,
  type RepositoryStatusView,
} from "@/src/lib/xero-model/project"
import {
  estimateCodeBytes,
  getLangFromPath,
  hashCodeContent,
  shouldSkipTokenization,
  tokenizeCode,
  type TokenizedLine,
} from "@/lib/shiki"
import {
  createByteBudgetCache,
  estimateUtf16Bytes,
  type ByteBudgetCache,
  type ByteBudgetCacheStats,
} from "@/lib/byte-budget-cache"
import { useTheme } from "@/src/features/theme/theme-provider"
import { useFixedVirtualizer } from "@/hooks/use-fixed-virtualizer"
import { shouldVirtualizeRows } from "@/lib/virtual-list"
import { cn } from "@/lib/utils"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

const MIN_WIDTH = 600
const DEFAULT_WIDTH_RATIO = 0.7
const FILE_LIST_WIDTH = 300
const MAX_DIFF_CACHE_ENTRIES = 80
export const DIFF_PATCH_CACHE_MAX_BYTES = 4 * 1024 * 1024
const VCS_FILE_ROW_HEIGHT = 28
const VCS_FILE_VIRTUALIZATION_THRESHOLD = 180
const DIFF_ROW_HEIGHT = 22
const DIFF_VIRTUALIZATION_THRESHOLD = 320
export const DIFF_LINE_HIGHLIGHT_BYTE_LIMIT = 8 * 1024
export const DIFF_TOKENIZATION_BATCH_SIZE = 24
const DIFF_PARSE_CACHE_MAX_ENTRIES = 80
export const DIFF_PARSE_CACHE_MAX_BYTES = 4 * 1024 * 1024

type ChangeKind = RepositoryStatusEntryView["staged"]

export type VcsCommitMessageModel = Omit<
  GitGenerateCommitMessageRequestDto,
  "projectId"
> & {
  label?: string | null
}

export interface VcsSidebarProps {
  open: boolean
  projectId: string | null
  status: RepositoryStatusView | null
  branchLabel?: string | null
  onClose?: () => void
  onRefreshStatus: () => void | Promise<void>
  onLoadDiff: (
    projectId: string,
    scope: RepositoryDiffScope,
  ) => Promise<RepositoryDiffResponseDto>
  commitMessageModel?: VcsCommitMessageModel | null
  onGenerateCommitMessage?: (
    projectId: string,
    model: VcsCommitMessageModel,
  ) => Promise<GitGenerateCommitMessageResponseDto>
  onStage: (projectId: string, paths: string[]) => Promise<void>
  onUnstage: (projectId: string, paths: string[]) => Promise<void>
  onDiscard: (projectId: string, paths: string[]) => Promise<void>
  onCommit: (projectId: string, message: string) => Promise<GitCommitResponseDto>
  onFetch: (projectId: string) => Promise<GitFetchResponseDto>
  onPull: (projectId: string) => Promise<GitPullResponseDto>
  onPush: (projectId: string) => Promise<GitPushResponseDto>
}

export type VcsDiffScopeEntry = Pick<RepositoryStatusEntryView, "staged" | "unstaged" | "untracked">

interface FileEntry extends VcsDiffScopeEntry {
  path: string
}

type DiffPatchCache = ByteBudgetCache<string, string>

type ActionKind =
  | "stage"
  | "stage-all"
  | "unstage"
  | "unstage-all"
  | "discard"
  | "commit"
  | "generate-commit-message"
  | "fetch"
  | "pull"
  | "push"

function useDiffPatchCacheRef(): MutableRefObject<DiffPatchCache> {
  const ref = useRef<DiffPatchCache | null>(null)
  if (!ref.current) {
    ref.current = createDiffPatchCache()
  }
  return ref as MutableRefObject<DiffPatchCache>
}

function getFileEntrySignature(entry: RepositoryStatusEntryView): string {
  return `${entry.path}\u0000${entry.staged ?? ""}\u0000${entry.unstaged ?? ""}\u0000${entry.untracked ? "1" : "0"}`
}

function getStatusEntriesSignature(status: RepositoryStatusView | null): string {
  return status?.entries.map(getFileEntrySignature).join("\u0001") ?? ""
}

function defaultViewportWidth(): number {
  if (typeof window === "undefined") return 900
  return Math.max(MIN_WIDTH, Math.round(window.innerWidth * DEFAULT_WIDTH_RATIO))
}

function viewportMaxWidth(): number {
  if (typeof window === "undefined") return 1600
  return Math.max(MIN_WIDTH, Math.round(window.innerWidth * 0.95))
}

export const VcsSidebar = memo(function VcsSidebar(props: VcsSidebarProps) {
  const { onClose, open, projectId, status } = props
  const shouldRenderDiffPane = (status?.entries.length ?? 0) > 0
  const [width, setWidth] = useState<number>(() => defaultViewportWidth())
  const [isResizing, setIsResizing] = useState(false)
  const diffPatchCacheRef = useDiffPatchCacheRef()
  const widthRef = useRef(width)
  widthRef.current = width
  const renderedWidth = shouldRenderDiffPane ? width : FILE_LIST_WIDTH

  useEffect(() => {
    diffPatchCacheRef.current.clear()
  }, [diffPatchCacheRef, projectId])

  // Recompute the cap when the viewport resizes — keep the panel within
  // 95vw so users can always grab the resize handle.
  useEffect(() => {
    if (typeof window === "undefined") return
    const handle = () => {
      const max = viewportMaxWidth()
      setWidth((current) => Math.min(Math.max(MIN_WIDTH, current), max))
    }
    window.addEventListener("resize", handle)
    return () => window.removeEventListener("resize", handle)
  }, [])

  // Close on Escape so the panel stays keyboard-friendly without stealing
  // focus from the rest of the app.
  useEffect(() => {
    if (!open || !onClose) return
    const handle = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        onClose()
      }
    }
    window.addEventListener("keydown", handle)
    return () => window.removeEventListener("keydown", handle)
  }, [onClose, open])

  const handleClose = useCallback(() => {
    onClose?.()
  }, [onClose])

  const handleResizeStart = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    let latestWidth = startWidth
    const widthUpdates = createFrameCoalescer<number>({
      onFlush: setWidth,
    })
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"

    const handleMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX
      latestWidth = Math.max(MIN_WIDTH, Math.min(viewportMaxWidth(), startWidth + delta))
      widthUpdates.schedule(latestWidth)
    }
    const handleUp = () => {
      widthUpdates.flush()
      window.removeEventListener("pointermove", handleMove)
      window.removeEventListener("pointerup", handleUp)
      window.removeEventListener("pointercancel", handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
    }

    window.addEventListener("pointermove", handleMove)
    window.addEventListener("pointerup", handleUp)
    window.addEventListener("pointercancel", handleUp)
  }, [])

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 64 : 16
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      return Math.max(MIN_WIDTH, Math.min(viewportMaxWidth(), current + delta))
    })
  }, [])

  if (!open) {
    return null
  }

  // The panel overlays the main content area. `<main>` has `contain: paint`,
  // which makes it the containing block for fixed descendants, so `inset-y-0`
  // already fills exactly the area between the titlebar and the status footer.
  return (
    <>
      {/* Backdrop: dims the underlying app and dismisses the panel on click. */}
      <div
        aria-hidden="true"
        className="fixed inset-0 z-40 bg-black/30"
        onClick={handleClose}
      />
      <aside
        aria-hidden="false"
        aria-label="Source control panel"
        className="fixed inset-y-0 right-0 z-50 flex flex-col overflow-hidden border-l border-border/80 bg-sidebar shadow-2xl"
        style={{
          width: renderedWidth,
          contain: "layout paint style",
        }}
      >
        {shouldRenderDiffPane ? (
          <div
            aria-label="Resize source control sidebar"
            aria-orientation="vertical"
            aria-valuemax={viewportMaxWidth()}
            aria-valuemin={MIN_WIDTH}
            aria-valuenow={width}
            className={cn(
              "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
              "hover:bg-primary/30",
              isResizing && "bg-primary/40",
            )}
            onKeyDown={handleResizeKey}
            onPointerDown={handleResizeStart}
            role="separator"
            tabIndex={open ? 0 : -1}
          />
        ) : null}

        <div className="flex h-full min-w-0 flex-1 flex-col">
          <VcsSidebarBody {...props} diffPatchCacheRef={diffPatchCacheRef} />
        </div>
      </aside>
    </>
  )
})

interface VcsSidebarBodyProps extends VcsSidebarProps {
  diffPatchCacheRef: MutableRefObject<DiffPatchCache>
}

function VcsSidebarBody({
  projectId,
  status,
  branchLabel,
  onClose,
  onRefreshStatus,
  onLoadDiff,
  commitMessageModel,
  onGenerateCommitMessage,
  onStage,
  onUnstage,
  onDiscard,
  onCommit,
  onFetch,
  onPull,
  onPush,
  diffPatchCacheRef,
}: VcsSidebarBodyProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [diffPatch, setDiffPatch] = useState<string>("")
  const [diffLoading, setDiffLoading] = useState(false)
  const [diffError, setDiffError] = useState<string | null>(null)

  const [commitMessage, setCommitMessage] = useState("")
  const [actionKind, setActionKind] = useState<ActionKind | null>(null)
  const [actionMessage, setActionMessage] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const onLoadDiffRef = useRef(onLoadDiff)

  const statusEntriesSignature = useMemo(() => getStatusEntriesSignature(status), [status])
  const repositoryRevision = status?.diffRevision ?? createRepositoryStatusDiffRevision(status)
  const allEntries: FileEntry[] = useMemo(() => {
    if (!status) return []
    return status.entries.map((entry) => ({
      path: entry.path,
      staged: entry.staged,
      unstaged: entry.unstaged,
      untracked: entry.untracked,
    }))
  }, [statusEntriesSignature])
  const allEntriesRef = useRef<FileEntry[]>(allEntries)
  allEntriesRef.current = allEntries
  const selectedEntry = useMemo(
    () => allEntries.find((item) => item.path === selectedPath) ?? null,
    [allEntries, selectedPath],
  )
  const selectedScope = useMemo(() => deriveVcsDiffScope(selectedEntry), [selectedEntry])

  useEffect(() => {
    onLoadDiffRef.current = onLoadDiff
  }, [onLoadDiff])

  const { stagedFiles, unstagedFiles } = useMemo(() => {
    const staged: FileEntry[] = []
    const unstaged: FileEntry[] = []
    for (const entry of allEntries) {
      if (entry.staged !== null) staged.push(entry)
      if (entry.unstaged !== null || entry.untracked) unstaged.push(entry)
    }
    return { stagedFiles: staged, unstagedFiles: unstaged }
  }, [allEntries])

  const totalChanges = useMemo(() => {
    const set = new Set<string>()
    for (const entry of allEntries) set.add(entry.path)
    return set.size
  }, [allEntries])

  // ---------- Diff loading ----------

  useEffect(() => {
    if (!projectId || !selectedPath || !selectedScope) {
      setDiffPatch("")
      setDiffError(null)
      setDiffLoading(false)
      return
    }

    const cacheKey = createDiffPatchCacheKey(projectId, repositoryRevision, selectedScope, selectedPath)
    const cachedPatch = diffPatchCacheRef.current.get(cacheKey)
    if (cachedPatch !== null) {
      setDiffPatch(cachedPatch)
      setDiffError(null)
      setDiffLoading(false)
      return
    }

    let cancelled = false
    setDiffLoading(true)
    setDiffError(null)

    onLoadDiffRef.current(projectId, selectedScope)
      .then((response) => {
        if (cancelled) return
        cacheScopeDiffPatches(
          diffPatchCacheRef.current,
          projectId,
          repositoryRevision,
          selectedScope,
          response.patch,
          allEntriesRef.current,
        )
        const patch = extractFilePatch(response.patch, selectedPath)
        setCachedDiffPatch(diffPatchCacheRef.current, cacheKey, patch)
        setDiffPatch(patch)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        setDiffError(error instanceof Error ? error.message : "Failed to load diff.")
        setDiffPatch("")
      })
      .finally(() => {
        if (!cancelled) {
          setDiffLoading(false)
        }
      })

    return () => {
      cancelled = true
    }
  }, [diffPatchCacheRef, projectId, repositoryRevision, selectedPath, selectedScope])

  useEffect(() => {
    if (selectedPath && allEntries.some((entry) => entry.path === selectedPath)) {
      return
    }
    setSelectedPath(allEntries[0]?.path ?? null)
  }, [allEntries, selectedPath])

  const runAction = useCallback(
    async <T,>(kind: ActionKind, fn: () => Promise<T>, successLabel?: string): Promise<T | null> => {
      setActionKind(kind)
      setActionError(null)
      setActionMessage(null)
      try {
        const result = await fn()
        if (successLabel) setActionMessage(successLabel)
        await onRefreshStatus()
        return result
      } catch (error) {
        setActionError(error instanceof Error ? error.message : "Action failed.")
        return null
      } finally {
        setActionKind(null)
      }
    },
    [onRefreshStatus],
  )

  const handleStageOne = useCallback((path: string) => {
    if (!projectId) return
    void runAction("stage", () => onStage(projectId, [path]))
  }, [onStage, projectId, runAction])
  const handleUnstageOne = useCallback((path: string) => {
    if (!projectId) return
    void runAction("unstage", () => onUnstage(projectId, [path]))
  }, [onUnstage, projectId, runAction])
  const handleDiscardOne = useCallback((path: string) => {
    if (!projectId) return
    void runAction("discard", () => onDiscard(projectId, [path]))
  }, [onDiscard, projectId, runAction])
  const handleStageAll = useCallback(() => {
    if (!projectId || unstagedFiles.length === 0) return
    void runAction("stage-all", () => onStage(projectId, unstagedFiles.map((entry) => entry.path)))
  }, [onStage, projectId, runAction, unstagedFiles])
  const handleUnstageAll = useCallback(() => {
    if (!projectId || stagedFiles.length === 0) return
    void runAction(
      "unstage-all",
      () => onUnstage(projectId, stagedFiles.map((entry) => entry.path)),
    )
  }, [onUnstage, projectId, runAction, stagedFiles])
  const handleCommit = useCallback(() => {
    if (!projectId) return
    const trimmed = commitMessage.trim()
    if (!trimmed) {
      setActionError("Enter a commit message.")
      return
    }
    void runAction("commit", () => onCommit(projectId, trimmed), "Commit created.").then(
      (result) => {
        if (result) setCommitMessage("")
      },
    )
  }, [commitMessage, onCommit, projectId, runAction])
  const handleGenerateCommitMessage = useCallback(() => {
    if (!projectId || !commitMessageModel || !onGenerateCommitMessage) return
    if (stagedFiles.length === 0) {
      setActionError("Stage changes before generating a commit message.")
      return
    }
    void runAction(
      "generate-commit-message",
      () => onGenerateCommitMessage(projectId, commitMessageModel),
    ).then((result) => {
      if (!result) return
      setCommitMessage(result.message)
      setActionMessage(
        result.diffTruncated
          ? "Commit message generated from truncated staged diff."
          : "Commit message generated.",
      )
    })
  }, [commitMessageModel, onGenerateCommitMessage, projectId, runAction, stagedFiles.length])
  const handleFetch = useCallback(() => {
    if (!projectId) return
    void runAction("fetch", () => onFetch(projectId), "Fetched from remote.")
  }, [onFetch, projectId, runAction])
  const handlePull = useCallback(() => {
    if (!projectId) return
    void runAction("pull", () => onPull(projectId)).then((result) => {
      if (result) setActionMessage(result.summary)
    })
  }, [onPull, projectId, runAction])
  const handlePush = useCallback(() => {
    if (!projectId) return
    void runAction("push", () => onPush(projectId)).then((result) => {
      if (result) {
        const allOk = result.updates.every((upd) => upd.ok)
        setActionMessage(allOk ? `Pushed ${result.branch}` : "Push completed with warnings.")
      }
    })
  }, [onPush, projectId, runAction])
  const handleCommitMessageChange = useCallback((event: ChangeEvent<HTMLTextAreaElement>) => {
    setCommitMessage(event.target.value)
  }, [])

  const isBusy = actionKind !== null
  const shouldRenderDiffPane = allEntries.length > 0
  const canGenerateCommitMessage = Boolean(
    projectId &&
      onGenerateCommitMessage &&
      commitMessageModel?.modelId &&
      stagedFiles.length > 0 &&
      !isBusy,
  )
  const generateCommitMessageLabel = commitMessageModel?.label
    ? `Generate commit message with ${commitMessageModel.label}`
    : "Generate commit message"

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex h-10 items-center justify-between gap-2 border-b border-border/70 px-3">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Source Control
          </span>
          <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
            {totalChanges}
          </span>
          {status ? (
            <span className="ml-1 flex items-center gap-1 font-mono text-[10px] tabular-nums">
              <span className="text-success">+{status.additions}</span>
              <span className="text-destructive">−{status.deletions}</span>
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-0.5">
          {onClose ? (
            <button
              aria-label="Close source control"
              className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
              onClick={onClose}
              type="button"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          ) : null}
        </div>
      </div>

      {/* Action banner */}
      {actionError ? (
        <div className="border-b border-destructive/30 bg-destructive/10 px-3 py-1.5 text-[11px] text-destructive">
          {actionError}
        </div>
      ) : actionMessage ? (
        <div className="border-b border-success/20 bg-success/10 px-3 py-1.5 text-[11px] text-success">
          {actionMessage}
        </div>
      ) : null}

      {/* Two-column body */}
      <div className="flex min-h-0 flex-1">
        {/* Left column: branch + actions + file list + commit */}
        <div
          className={cn("flex h-full shrink-0 flex-col", shouldRenderDiffPane && "border-r border-border/70")}
          style={{ width: FILE_LIST_WIDTH }}
        >
          {/* Branch + remote actions */}
          <div className="flex h-9 items-center gap-1 border-b border-border/70 px-3">
            <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground" />
            <span className="min-w-0 truncate text-[12px] font-medium text-foreground/90">
              {branchLabel ?? status?.branchLabel ?? "No branch"}
            </span>
            <div className="ml-auto flex items-center gap-0.5">
              <ToolbarButton
                busy={actionKind === "fetch"}
                disabled={isBusy || !projectId}
                icon={<RefreshCw className="h-3 w-3" />}
                label="Fetch"
                onClick={handleFetch}
              />
              <ToolbarButton
                busy={actionKind === "pull"}
                disabled={isBusy || !projectId}
                icon={<ArrowDownToLine className="h-3 w-3" />}
                label="Pull"
                onClick={handlePull}
              />
              <ToolbarButton
                busy={actionKind === "push"}
                disabled={isBusy || !projectId}
                icon={<ArrowUpFromLine className="h-3 w-3" />}
                label="Push"
                onClick={handlePush}
              />
            </div>
          </div>

          <VcsFileList
            actionKind={actionKind}
            busy={isBusy}
            stagedFiles={stagedFiles}
            unstagedFiles={unstagedFiles}
            onDiscard={handleDiscardOne}
            onSelect={setSelectedPath}
            onStageAll={handleStageAll}
            onStageOne={handleStageOne}
            onUnstageAll={handleUnstageAll}
            onUnstageOne={handleUnstageOne}
            selectedPath={selectedPath}
          />

          {/* Commit input */}
          <div className="shrink-0 border-t border-border/70 bg-sidebar px-3 py-2">
            <textarea
              aria-label="Commit message"
              className="block h-16 w-full resize-none rounded-md border border-border/70 bg-background/50 px-2 py-1.5 text-[12px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
              disabled={isBusy || !projectId}
              onChange={handleCommitMessageChange}
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                  event.preventDefault()
                  handleCommit()
                }
              }}
              placeholder="Commit message (⌘⏎ to commit)"
              value={commitMessage}
            />
            <div className="mt-1.5 flex items-center justify-between gap-2">
              <span className="text-[10.5px] text-muted-foreground">
                {stagedFiles.length} staged · {unstagedFiles.length} unstaged
              </span>
              <div className="ml-auto flex items-center gap-1">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      aria-label={generateCommitMessageLabel}
                      className="h-7 w-7 text-muted-foreground hover:text-foreground"
                      disabled={!canGenerateCommitMessage}
                      onClick={handleGenerateCommitMessage}
                      size="icon-sm"
                      title={generateCommitMessageLabel}
                      type="button"
                      variant="ghost"
                    >
                      {actionKind === "generate-commit-message" ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Sparkles className="h-3.5 w-3.5" />
                      )}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{generateCommitMessageLabel}</TooltipContent>
                </Tooltip>
                <Button
                  className="h-7 gap-1.5 px-2.5 text-[11.5px]"
                  disabled={
                    isBusy || !projectId || stagedFiles.length === 0 || commitMessage.trim().length === 0
                  }
                  onClick={handleCommit}
                  size="sm"
                  type="button"
                >
                  {actionKind === "commit" ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <GitCommit className="h-3 w-3" />
                  )}
                  Commit
                  <kbd className="ml-0.5 hidden rounded border border-primary-foreground/20 bg-primary-foreground/10 px-1 py-px font-mono text-[9px] sm:inline-flex">
                    ⌘⏎
                  </kbd>
                </Button>
              </div>
            </div>
          </div>
        </div>

        {shouldRenderDiffPane ? (
          <div className="flex min-w-0 flex-1 flex-col bg-background/40">
            <div className="flex h-9 shrink-0 items-center justify-between gap-2 border-b border-border/70 bg-sidebar px-3">
              <span className="truncate text-[12px] font-medium text-foreground/85">
                {selectedPath ?? "Select a file"}
              </span>
              {diffLoading ? (
                <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
              ) : null}
            </div>
            <div className="min-h-0 flex-1">
              {diffError ? (
                <div className="px-4 py-4 text-[12px] text-destructive">{diffError}</div>
              ) : diffPatch ? (
                <DiffView patch={diffPatch} path={selectedPath ?? ""} />
              ) : (
                <div className="px-4 py-4 text-[12px] text-muted-foreground/70">
                  {selectedPath ? "No diff available." : "Select a file to view its diff."}
                </div>
              )}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Virtualized changed-file list
// ---------------------------------------------------------------------------

interface VcsFileListProps {
  actionKind: ActionKind | null
  busy: boolean
  onDiscard: (path: string) => void
  onSelect: (path: string) => void
  onStageAll: () => void
  onStageOne: (path: string) => void
  onUnstageAll: () => void
  onUnstageOne: (path: string) => void
  selectedPath: string | null
  stagedFiles: FileEntry[]
  unstagedFiles: FileEntry[]
}

type VcsFileListRow =
  | {
      kind: "group"
      groupKind: "staged" | "unstaged"
      groupLabel: string
      count: number
    }
  | {
      kind: "empty"
      groupKind: "staged" | "unstaged"
      label: string
    }
  | {
      kind: "file"
      groupKind: "staged" | "unstaged"
      entry: FileEntry
    }

function createVcsFileListRows({
  collapsedGroups,
  stagedFiles,
  unstagedFiles,
}: {
  collapsedGroups: Record<"staged" | "unstaged", boolean>
  stagedFiles: FileEntry[]
  unstagedFiles: FileEntry[]
}): VcsFileListRow[] {
  const rows: VcsFileListRow[] = []

  rows.push({ kind: "group", groupKind: "staged", groupLabel: "Staged Changes", count: stagedFiles.length })
  if (!collapsedGroups.staged) {
    if (stagedFiles.length === 0) {
      rows.push({ kind: "empty", groupKind: "staged", label: "No staged changes" })
    } else {
      for (const entry of stagedFiles) rows.push({ kind: "file", groupKind: "staged", entry })
    }
  }

  rows.push({ kind: "group", groupKind: "unstaged", groupLabel: "Changes", count: unstagedFiles.length })
  if (!collapsedGroups.unstaged) {
    if (unstagedFiles.length === 0) {
      rows.push({ kind: "empty", groupKind: "unstaged", label: "Working tree is clean" })
    } else {
      for (const entry of unstagedFiles) rows.push({ kind: "file", groupKind: "unstaged", entry })
    }
  }

  return rows
}

const VcsFileList = memo(function VcsFileList({
  actionKind,
  busy,
  onDiscard,
  onSelect,
  onStageAll,
  onStageOne,
  onUnstageAll,
  onUnstageOne,
  selectedPath,
  stagedFiles,
  unstagedFiles,
}: VcsFileListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Record<"staged" | "unstaged", boolean>>({
    staged: false,
    unstaged: false,
  })
  const rows = useMemo(
    () => createVcsFileListRows({ collapsedGroups, stagedFiles, unstagedFiles }),
    [collapsedGroups, stagedFiles, unstagedFiles],
  )
  const selectedRowIndex = useMemo(
    () => rows.findIndex((row) => row.kind === "file" && row.entry.path === selectedPath),
    [rows, selectedPath],
  )
  const shouldVirtualize = shouldVirtualizeRows(rows.length, VCS_FILE_VIRTUALIZATION_THRESHOLD)
  const virtualizer = useFixedVirtualizer({
    enabled: shouldVirtualize,
    itemCount: rows.length,
    itemSize: VCS_FILE_ROW_HEIGHT,
    overscan: 10,
    scrollToIndex: selectedRowIndex >= 0 ? selectedRowIndex : null,
  })
  const renderedRowIndexes = shouldVirtualize
    ? virtualizer.indexes
    : rows.map((_, index) => index)
  const toggleGroup = useCallback((groupKind: "staged" | "unstaged") => {
    setCollapsedGroups((current) => ({
      ...current,
      [groupKind]: !current[groupKind],
    }))
  }, [])

  return (
    <div
      aria-label="Changed files"
      className="flex flex-1 min-h-0 flex-col overflow-y-auto scrollbar-thin"
      onScroll={virtualizer.onScroll}
      ref={virtualizer.scrollRef}
      role="listbox"
    >
      {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.beforeSize }} /> : null}
      {renderedRowIndexes.map((rowIndex) => {
        const row = rows[rowIndex]
        if (row.kind === "group") {
          const collapsed = collapsedGroups[row.groupKind]
          const groupAction =
            row.groupKind === "unstaged"
              ? {
                  busy: actionKind === "stage-all",
                  icon: <Plus className="h-3 w-3" />,
                  label: "Stage all",
                  onClick: onStageAll,
                }
              : {
                  busy: actionKind === "unstage-all",
                  icon: <Minus className="h-3 w-3" />,
                  label: "Unstage all",
                  onClick: onUnstageAll,
                }

          return (
            <div
              className="group flex h-[28px] w-full items-center gap-2 border-b border-border/40 px-3 text-left"
              key={`${row.groupKind}:group`}
              role="presentation"
            >
              <button
                aria-expanded={!collapsed}
                aria-label={collapsed ? `Expand ${row.groupLabel}` : `Collapse ${row.groupLabel}`}
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                onClick={() => toggleGroup(row.groupKind)}
                type="button"
              >
                <ChevronRight
                  className={cn(
                    "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
                    !collapsed && "rotate-90",
                  )}
                />
                <span className="truncate text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
                  {row.groupLabel}
                </span>
                <span className="rounded-full bg-muted/70 px-1.5 py-[1px] font-mono text-[10px] tabular-nums text-muted-foreground">
                  {row.count}
                </span>
              </button>
              {row.count > 0 ? (
                <span className="invisible flex items-center gap-0.5 group-hover:visible">
                  <GroupActionButton disabled={busy} {...groupAction} />
                </span>
              ) : null}
            </div>
          )
        }

        if (row.kind === "empty") {
          return (
            <div
              className="flex h-[28px] items-center border-b border-border/40 px-7 text-[11px] text-muted-foreground/70"
              key={`${row.groupKind}:empty`}
              role="presentation"
            >
              {row.label}
            </div>
          )
        }

        const onToggle = row.groupKind === "staged" ? onUnstageOne : onStageOne
        return (
          <FileRow
            busy={busy}
            entry={row.entry}
            groupKind={row.groupKind}
            key={`${row.groupKind}-${row.entry.path}`}
            onDiscard={onDiscard}
            onSelect={onSelect}
            onToggle={onToggle}
            selected={selectedPath === row.entry.path}
          />
        )
      })}
      {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.afterSize }} /> : null}
    </div>
  )
})

interface FileRowProps {
  busy: boolean
  entry: FileEntry
  groupKind: "staged" | "unstaged"
  onDiscard: (path: string) => void
  onSelect: (path: string) => void
  onToggle: (path: string) => void
  selected: boolean
}

function FileRow({ busy, entry, groupKind, onDiscard, onSelect, onToggle, selected }: FileRowProps) {
  const fileName = entry.path.split("/").pop() ?? entry.path
  const dir = entry.path.includes("/")
    ? entry.path.slice(0, entry.path.length - fileName.length - 1)
    : ""
  const kind: ChangeKind =
    groupKind === "staged" ? entry.staged : (entry.unstaged ?? (entry.untracked ? "added" : null))

  return (
    <div
      aria-selected={selected}
      className={cn(
        "group/row flex h-[28px] cursor-pointer items-center gap-1.5 px-3 py-0 text-left transition-colors",
        selected ? "bg-primary/10" : "hover:bg-secondary/40",
      )}
      onClick={() => onSelect(entry.path)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault()
          onSelect(entry.path)
        }
      }}
      role="option"
      tabIndex={0}
    >
      <ChangeBadge entry={entry} kind={kind} untracked={groupKind === "unstaged" && entry.untracked} />
      <span className="min-w-0 flex-1 truncate text-[12px] text-foreground/90">{fileName}</span>
      {dir ? (
        <span className="hidden min-w-0 truncate text-[10.5px] text-muted-foreground sm:inline">{dir}</span>
      ) : null}
      <span className="invisible flex items-center gap-0.5 group-hover/row:visible">
        {groupKind === "unstaged" ? (
          <RowIconButton
            disabled={busy}
            icon={<RotateCcw className="h-3 w-3" />}
            label={`Discard ${fileName}`}
            onClick={(event) => {
              event.stopPropagation()
              onDiscard(entry.path)
            }}
            tone="danger"
          />
        ) : null}
        <RowIconButton
          disabled={busy}
          icon={groupKind === "staged" ? <Minus className="h-3 w-3" /> : <Plus className="h-3 w-3" />}
          label={groupKind === "staged" ? `Unstage ${fileName}` : `Stage ${fileName}`}
          onClick={(event) => {
            event.stopPropagation()
            onToggle(entry.path)
          }}
        />
      </span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Bits
// ---------------------------------------------------------------------------

interface ToolbarButtonProps {
  busy?: boolean
  disabled?: boolean
  icon: React.ReactNode
  label: string
  onClick: () => void
}

function ToolbarButton({ busy, disabled, icon, label, onClick }: ToolbarButtonProps) {
  return (
    <button
      aria-label={label}
      className={cn(
        "rounded p-1 text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground",
        "disabled:cursor-not-allowed disabled:opacity-50",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : icon}
    </button>
  )
}

interface GroupActionButtonProps {
  busy?: boolean
  disabled?: boolean
  icon: React.ReactNode
  label: string
  onClick: () => void
}

function GroupActionButton({ busy, disabled, icon, label, onClick }: GroupActionButtonProps) {
  return (
    <button
      aria-label={label}
      className={cn(
        "rounded p-0.5 text-muted-foreground transition-colors hover:bg-secondary/70 hover:text-foreground",
        "disabled:cursor-not-allowed disabled:opacity-50",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : icon}
    </button>
  )
}

interface RowIconButtonProps {
  disabled?: boolean
  icon: React.ReactNode
  label: string
  onClick: (event: React.MouseEvent) => void
  tone?: "default" | "danger"
}

function RowIconButton({ disabled, icon, label, onClick, tone = "default" }: RowIconButtonProps) {
  return (
    <button
      aria-label={label}
      className={cn(
        "rounded p-0.5 transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        tone === "danger"
          ? "text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
          : "text-muted-foreground hover:bg-secondary/70 hover:text-foreground",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
    </button>
  )
}

function ChangeBadge({ entry, kind, untracked }: { entry: FileEntry; kind: ChangeKind; untracked: boolean }) {
  let letter = "?"
  let color = "text-muted-foreground"

  if (untracked && !entry.staged) {
    letter = "U"
    color = "text-success"
  } else if (kind === "added") {
    letter = "A"
    color = "text-success"
  } else if (kind === "modified") {
    letter = "M"
    color = "text-warning"
  } else if (kind === "deleted") {
    letter = "D"
    color = "text-destructive"
  } else if (kind === "renamed") {
    letter = "R"
    color = "text-info"
  } else if (kind === "copied") {
    letter = "C"
    color = "text-info"
  } else if (kind === "type_change") {
    letter = "T"
    color = "text-purple-400"
  } else if (kind === "conflicted") {
    letter = "!"
    color = "text-destructive"
  }

  return (
    <span
      aria-hidden="true"
      className={cn("inline-flex h-4 w-4 shrink-0 items-center justify-center font-mono text-[10px]", color)}
    >
      {letter}
    </span>
  )
}

// ---------------------------------------------------------------------------
// Diff view with shiki syntax highlighting
// ---------------------------------------------------------------------------

export interface DiffLine {
  kind: "context" | "add" | "remove" | "hunk" | "header"
  prefix: string
  text: string
  oldNo: number | null
  newNo: number | null
}

interface DiffParseCacheEntry {
  lines: DiffLine[]
}

interface DiffParsingStats extends ByteBudgetCacheStats {
  parses: number
}

interface DiffTokenizationStats {
  batchCount: number
  lineRequestCount: number
  skippedLargeLineCount: number
}

export interface DiffTokenizationBatchOptions {
  batchSize?: number
  indexes: number[]
  lines: DiffLine[]
  maxLineBytes?: number
  tokenizedLineIndexes?: ReadonlySet<number>
}

const diffParseCache = createByteBudgetCache<string, DiffParseCacheEntry>({
  maxBytes: DIFF_PARSE_CACHE_MAX_BYTES,
  maxEntries: DIFF_PARSE_CACHE_MAX_ENTRIES,
})
const diffParsingStats = {
  parses: 0,
}
const diffTokenizationStats = {
  batchCount: 0,
  lineRequestCount: 0,
  skippedLargeLineCount: 0,
}

function createDiffParseKey(path: string, patch: string): string {
  return [path, patch.length, hashCodeContent(patch)].join("\u0000")
}

function estimateDiffLinesBytes(patchKey: string, patch: string, lines: DiffLine[]): number {
  let bytes = estimateUtf16Bytes(patchKey) + estimateUtf16Bytes(patch) + 32
  for (const line of lines) {
    bytes += 40
    bytes += estimateUtf16Bytes(line.prefix)
    bytes += estimateUtf16Bytes(line.text)
  }
  return bytes
}

export function parseDiffLinesForPatchKey(patchKey: string, patch: string): DiffLine[] {
  const cached = diffParseCache.get(patchKey)
  if (cached) {
    return cached.lines
  }

  diffParsingStats.parses += 1
  const lines = parseDiffLines(patch)
  diffParseCache.set(patchKey, { lines }, estimateDiffLinesBytes(patchKey, patch, lines))

  return lines
}

export function getDiffParsingStats(): DiffParsingStats {
  const cacheStats = diffParseCache.getStats()
  return {
    ...cacheStats,
    parses: diffParsingStats.parses,
  }
}

export function getDiffTokenizationStats(): DiffTokenizationStats {
  return { ...diffTokenizationStats }
}

export function resetDiffPerformanceStatsForTests(): void {
  diffParseCache.clear()
  diffParsingStats.parses = 0
  diffTokenizationStats.batchCount = 0
  diffTokenizationStats.lineRequestCount = 0
  diffTokenizationStats.skippedLargeLineCount = 0
}

function isTokenizableDiffLine(line: DiffLine): boolean {
  return line.kind === "add" || line.kind === "remove" || line.kind === "context"
}

export function createDiffTokenizationBatches({
  batchSize = DIFF_TOKENIZATION_BATCH_SIZE,
  indexes,
  lines,
  maxLineBytes = DIFF_LINE_HIGHLIGHT_BYTE_LIMIT,
  tokenizedLineIndexes,
}: DiffTokenizationBatchOptions): number[][] {
  const batches: number[][] = []
  let current: number[] = []

  for (const index of indexes) {
    if (tokenizedLineIndexes?.has(index)) continue
    const line = lines[index]
    if (!line || !isTokenizableDiffLine(line) || line.text.length === 0) continue
    if (estimateCodeBytes(line.text) > maxLineBytes) {
      diffTokenizationStats.skippedLargeLineCount += 1
      continue
    }

    current.push(index)
    if (current.length >= batchSize) {
      batches.push(current)
      current = []
    }
  }

  if (current.length > 0) {
    batches.push(current)
  }

  return batches
}

function recordDiffTokenizationBatch(lineCount: number): void {
  diffTokenizationStats.batchCount += 1
  diffTokenizationStats.lineRequestCount += lineCount
}

function scheduleDiffTokenizationWork(callback: () => void): () => void {
  if (typeof window === "undefined") {
    const id = setTimeout(callback, 0)
    return () => clearTimeout(id)
  }

  const idleWindow = window as Window & {
    cancelIdleCallback?: (handle: number) => void
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number
  }

  if (typeof idleWindow.requestIdleCallback === "function") {
    const handle = idleWindow.requestIdleCallback(callback, { timeout: 48 })
    return () => idleWindow.cancelIdleCallback?.(handle)
  }

  if (typeof window.requestAnimationFrame === "function") {
    const handle = window.requestAnimationFrame(callback)
    return () => window.cancelAnimationFrame(handle)
  }

  const id = window.setTimeout(callback, 0)
  return () => window.clearTimeout(id)
}

export function parseDiffLines(patch: string): DiffLine[] {
  const lines = patch.split(/\r?\n/)
  const result: DiffLine[] = []
  let oldNo = 0
  let newNo = 0
  let inHunk = false

  for (const raw of lines) {
    if (raw.startsWith("diff ") || raw.startsWith("index ") || raw.startsWith("---") || raw.startsWith("+++") || raw.startsWith("similarity ") || raw.startsWith("rename ") || raw.startsWith("new file") || raw.startsWith("deleted file")) {
      result.push({ kind: "header", prefix: "", text: raw, oldNo: null, newNo: null })
      continue
    }
    if (raw.startsWith("@@")) {
      // @@ -oldStart,oldLines +newStart,newLines @@
      const match = /@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/.exec(raw)
      if (match) {
        oldNo = parseInt(match[1], 10)
        newNo = parseInt(match[2], 10)
      }
      inHunk = true
      result.push({ kind: "hunk", prefix: "", text: raw, oldNo: null, newNo: null })
      continue
    }
    if (!inHunk) {
      result.push({ kind: "header", prefix: "", text: raw, oldNo: null, newNo: null })
      continue
    }

    const first = raw.charAt(0)
    if (first === "+") {
      result.push({
        kind: "add",
        prefix: "+",
        text: raw.slice(1),
        oldNo: null,
        newNo: newNo,
      })
      newNo += 1
    } else if (first === "-") {
      result.push({
        kind: "remove",
        prefix: "-",
        text: raw.slice(1),
        oldNo: oldNo,
        newNo: null,
      })
      oldNo += 1
    } else if (first === "\\") {
      // "\ No newline at end of file"
      result.push({ kind: "context", prefix: " ", text: raw, oldNo: null, newNo: null })
    } else {
      result.push({
        kind: "context",
        prefix: " ",
        text: raw.length > 0 ? raw.slice(1) : "",
        oldNo: oldNo,
        newNo: newNo,
      })
      oldNo += 1
      newNo += 1
    }
  }

  return result
}

function DiffView({ patch, path }: { patch: string; path: string }) {
  const { theme } = useTheme()
  const lang = useMemo(() => getLangFromPath(path), [path])
  const patchKey = useMemo(() => createDiffParseKey(path, patch), [path, patch])
  const lines = useMemo(() => parseDiffLinesForPatchKey(patchKey, patch), [patchKey, patch])
  const shouldVirtualize = shouldVirtualizeRows(lines.length, DIFF_VIRTUALIZATION_THRESHOLD)
  const virtualizer = useFixedVirtualizer({
    enabled: shouldVirtualize,
    itemCount: lines.length,
    itemSize: DIFF_ROW_HEIGHT,
    overscan: 24,
    initialViewportSize: 640,
  })
  const renderedLineIndexes = useMemo(
    () => (shouldVirtualize ? virtualizer.indexes : lines.map((_, index) => index)),
    [lines, shouldVirtualize, virtualizer.indexes],
  )

  // Per-line shiki tokenization. We only hydrate visible/overscan rows, in
  // bounded batches, so large diffs paint plain text first instead of
  // launching one highlighter task per line.
  const tokenizationKey = `${patchKey}\u0000${lang ?? "text"}\u0000${theme.shiki}`
  const tokenizedLinesRef = useRef<{
    attempted: Set<number>
    key: string
    lines: Map<number, TokenizedLine>
  }>({
    attempted: new Set(),
    key: tokenizationKey,
    lines: new Map(),
  })
  const [tokenizedState, setTokenizedState] = useState<{
    key: string
    lines: Map<number, TokenizedLine>
  }>(() => ({
    key: tokenizationKey,
    lines: new Map(),
  }))
  const tokenizedLines = tokenizedState.key === tokenizationKey ? tokenizedState.lines : new Map<number, TokenizedLine>()

  useEffect(() => {
    tokenizedLinesRef.current = { attempted: new Set(), key: tokenizationKey, lines: new Map() }
    setTokenizedState({ key: tokenizationKey, lines: new Map() })
  }, [tokenizationKey])

  useEffect(() => {
    if (!lang) return
    let cancelled = false
    let cancelScheduled: (() => void) | null = null

    const runNextBatch = () => {
      if (cancelled) return
      const current = tokenizedLinesRef.current.key === tokenizationKey
        ? tokenizedLinesRef.current
        : { attempted: new Set<number>(), key: tokenizationKey, lines: new Map<number, TokenizedLine>() }
      const batches = createDiffTokenizationBatches({
        indexes: renderedLineIndexes,
        lines,
        tokenizedLineIndexes: current.attempted,
      })
      const batch = batches[0]
      if (!batch || batch.length === 0) return

      recordDiffTokenizationBatch(batch.length)
      void Promise.all(
        batch.map((lineIndex) => {
          const line = lines[lineIndex]
          if (!line) return Promise.resolve<[number, TokenizedLine | null]>([lineIndex, null])
          return tokenizeCode(line.text, lang, theme.shiki, {
            maxBytes: DIFF_LINE_HIGHLIGHT_BYTE_LIMIT,
          }).then((result) => [lineIndex, result?.[0] ?? null] as [number, TokenizedLine | null])
        }),
      ).then((results) => {
        if (cancelled) return
        const latest = tokenizedLinesRef.current.key === tokenizationKey
          ? tokenizedLinesRef.current
          : { attempted: new Set<number>(), key: tokenizationKey, lines: new Map<number, TokenizedLine>() }
        const next = new Map(latest.lines)
        const attempted = new Set(latest.attempted)
        let changed = false
        for (const [lineIndex, tokens] of results) {
          attempted.add(lineIndex)
          if (!tokens || next.has(lineIndex)) continue
          next.set(lineIndex, tokens)
          changed = true
        }
        tokenizedLinesRef.current = { attempted, key: tokenizationKey, lines: next }
        if (changed) {
          setTokenizedState({ key: tokenizationKey, lines: next })
        }

        if (!cancelled) {
          cancelScheduled = scheduleDiffTokenizationWork(runNextBatch)
        }
      })
    }

    cancelScheduled = scheduleDiffTokenizationWork(runNextBatch)
    return () => {
      cancelled = true
      cancelScheduled?.()
    }
  }, [lang, lines, renderedLineIndexes, theme.shiki, tokenizationKey])

  return (
    <div
      aria-label="Unified diff"
      className="h-full overflow-auto font-mono text-[12px] leading-[1.55] scrollbar-thin"
      onScroll={virtualizer.onScroll}
      ref={virtualizer.scrollRef}
      role="table"
    >
      {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.beforeSize }} /> : null}
      {renderedLineIndexes.map((lineIndex) => (
        <DiffLineRow
          key={lineIndex}
          line={lines[lineIndex]}
          tokens={tokenizedLines.get(lineIndex) ?? null}
        />
      ))}
      {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.afterSize }} /> : null}
    </div>
  )
}

function DiffLineRow({ line, tokens }: { line: DiffLine; tokens: TokenizedLine | null }) {
  if (line.kind === "hunk") {
    return (
      <div className="flex h-[22px] items-stretch border-y border-info/20 bg-info/10 text-[11px] text-info">
        <span className="w-12 shrink-0 select-none border-r border-info/20" />
        <span className="min-w-0 truncate px-3 py-0.5">{line.text}</span>
      </div>
    )
  }
  if (line.kind === "header") {
    return (
      <div className="flex h-[22px] items-stretch text-[11px] text-muted-foreground/60">
        <span className="w-12 shrink-0 select-none" />
        <span className="min-w-0 truncate px-3 py-0.5">{line.text}</span>
      </div>
    )
  }

  // For unified diffs we show a single line-number column. Added/context
  // rows display the *new* file's line number; removed rows show the *old*
  // file's line number — matching VS Code / GitHub unified view.
  const lineNo = line.kind === "remove" ? line.oldNo : line.newNo
  const rowTone =
    line.kind === "add"
      ? "bg-success/70"
      : line.kind === "remove"
        ? "bg-destructive/70"
        : ""
  const gutterTone =
    line.kind === "add"
      ? "border-r border-success/70 text-success"
      : line.kind === "remove"
        ? "border-r border-destructive/70 text-destructive"
        : "text-muted-foreground/40 border-r border-border/40"
  const prefixClass =
    line.kind === "add"
      ? "text-success"
      : line.kind === "remove"
        ? "text-destructive"
        : "text-muted-foreground/30"
  const renderedPlainBecauseLarge = isTokenizableDiffLine(line) &&
    shouldSkipTokenization(line.text, DIFF_LINE_HIGHLIGHT_BYTE_LIMIT)

  return (
    <div className={cn("flex h-[22px] min-w-0 items-stretch", rowTone)}>
      <span
        className={cn(
          "w-12 shrink-0 select-none px-2 text-right font-mono text-[10.5px] tabular-nums",
          gutterTone,
        )}
      >
        {lineNo ?? ""}
      </span>
      <span className={cn("w-5 shrink-0 select-none text-center", prefixClass)}>{line.prefix}</span>
      <pre
        className="m-0 min-w-0 flex-1 whitespace-pre py-px pr-3"
        title={renderedPlainBecauseLarge ? "Large line rendered as plain text" : undefined}
      >
        {tokens ? <ShikiTokens tokens={tokens} /> : line.text}
      </pre>
    </div>
  )
}

function ShikiTokens({ tokens }: { tokens: TokenizedLine }) {
  return (
    <>
      {tokens.map((token, index) => (
        <span
          key={index}
          style={{
            color: token.color,
            fontStyle: token.fontStyle === 1 ? "italic" : undefined,
            fontWeight: token.fontStyle === 2 ? 600 : undefined,
            textDecoration: token.fontStyle === 4 ? "underline" : undefined,
          }}
        >
          {token.content}
        </span>
      ))}
    </>
  )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function deriveVcsDiffScope(
  entry: VcsDiffScopeEntry | null,
): RepositoryDiffScope | null {
  if (!entry) return null

  if (entry.staged && !entry.unstaged && !entry.untracked) {
    return "staged"
  }

  if (entry.staged && (entry.unstaged || entry.untracked)) {
    return "worktree"
  }

  return "unstaged"
}

function createDiffPatchCacheKey(
  projectId: string,
  revision: string,
  scope: RepositoryDiffScope,
  path: string,
): string {
  return [projectId, revision, scope, path].join("\u0000")
}

export function createDiffPatchCache(): DiffPatchCache {
  return createByteBudgetCache<string, string>({
    maxBytes: DIFF_PATCH_CACHE_MAX_BYTES,
    maxEntries: MAX_DIFF_CACHE_ENTRIES,
  })
}

function estimateDiffPatchBytes(key: string, patch: string): number {
  return estimateUtf16Bytes(key) + estimateUtf16Bytes(patch) + 32
}

export function getDiffPatchCacheStats(cache: DiffPatchCache): ByteBudgetCacheStats {
  return cache.getStats()
}

export function setCachedDiffPatch(cache: DiffPatchCache, key: string, patch: string): void {
  cache.set(key, patch, estimateDiffPatchBytes(key, patch))
}

function cacheScopeDiffPatches(
  cache: DiffPatchCache,
  projectId: string,
  revision: string,
  scope: RepositoryDiffScope,
  patch: string,
  entries: FileEntry[],
): void {
  for (const entry of entries) {
    if (deriveVcsDiffScope(entry) !== scope) {
      continue
    }

    setCachedDiffPatch(
      cache,
      createDiffPatchCacheKey(projectId, revision, scope, entry.path),
      extractFilePatch(patch, entry.path),
    )
  }
}

/** Slice a multi-file unified patch down to just the section for `path`. */
function extractFilePatch(patch: string, path: string): string {
  if (!patch) return ""
  const headerMarker = "diff --git "
  const sections = patch.split(/\n(?=diff --git )/)
  const matched = sections.find((section) => {
    if (!section.startsWith(headerMarker) && sections.length > 1) return false
    return section.includes(` a/${path}`) || section.includes(` b/${path}`) || section.includes(path)
  })
  return matched ?? patch
}

export function computeRepositoryDiffCount(status: RepositoryStatusView | null): number {
  if (!status) return 0
  return status.statusCount
}
