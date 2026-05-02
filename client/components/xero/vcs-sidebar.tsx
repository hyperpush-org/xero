"use client"

import { memo, useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject } from "react"
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
import { motion } from "motion/react"
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
import { getLangFromPath, tokenizeCode, type TokenizedLine } from "@/lib/shiki"
import { useTheme } from "@/src/features/theme/theme-provider"
import { cn } from "@/lib/utils"
import { useSidebarMotion } from "@/lib/sidebar-motion"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

const MIN_WIDTH = 600
const DEFAULT_WIDTH_RATIO = 0.7
const FILE_LIST_WIDTH = 300
const MAX_DIFF_CACHE_ENTRIES = 80

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

type DiffPatchCache = Map<string, string>

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
  const { onClose, open, status } = props
  const shouldRenderDiffPane = (status?.entries.length ?? 0) > 0
  const [width, setWidth] = useState<number>(() => defaultViewportWidth())
  const [isResizing, setIsResizing] = useState(false)
  const { contentTransition, widthTransition } = useSidebarMotion(isResizing)
  const diffPatchCacheRef = useRef<DiffPatchCache>(new Map())
  const widthRef = useRef(width)
  widthRef.current = width
  const renderedWidth = shouldRenderDiffPane ? width : FILE_LIST_WIDTH

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
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"

    const handleMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX
      const next = Math.max(MIN_WIDTH, Math.min(viewportMaxWidth(), startWidth + delta))
      setWidth(next)
    }
    const handleUp = () => {
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

  // The panel overlays the main content area. `<main>` has `contain: paint`,
  // which makes it the containing block for fixed descendants, so `inset-y-0`
  // already fills exactly the area between the titlebar and the status footer.
  return (
    <>
      {/* Backdrop: dims the underlying app and dismisses the panel on click. */}
      <motion.div
        animate={{ opacity: open ? 1 : 0 }}
        aria-hidden="true"
        className={cn(
          "fixed inset-0 z-40 bg-black/30",
          open ? "pointer-events-auto" : "pointer-events-none",
        )}
        initial={false}
        onClick={handleClose}
        transition={contentTransition}
      />
      <motion.aside
        animate={{ x: open ? 0 : renderedWidth }}
        aria-hidden={!open}
        aria-label="Source control panel"
        className={cn(
          "gpu-layer fixed inset-y-0 right-0 z-50 flex flex-col overflow-hidden border-l border-border/80 bg-sidebar shadow-2xl",
          !open && "invisible",
        )}
        initial={false}
        inert={!open ? true : undefined}
        style={{
          width: renderedWidth,
          contain: "layout paint style",
          willChange: "transform",
        }}
        transition={widthTransition}
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

        {open ? <VcsSidebarBody {...props} diffPatchCacheRef={diffPatchCacheRef} /> : null}
      </motion.aside>
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

  const stagedFiles = useMemo(
    () => allEntries.filter((entry) => entry.staged !== null),
    [allEntries],
  )
  const unstagedFiles = useMemo(
    () => allEntries.filter((entry) => entry.unstaged !== null || entry.untracked),
    [allEntries],
  )

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
    if (cachedPatch !== undefined) {
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

  const handleStageOne = (path: string) => {
    if (!projectId) return
    void runAction("stage", () => onStage(projectId, [path]))
  }
  const handleUnstageOne = (path: string) => {
    if (!projectId) return
    void runAction("unstage", () => onUnstage(projectId, [path]))
  }
  const handleDiscardOne = (path: string) => {
    if (!projectId) return
    void runAction("discard", () => onDiscard(projectId, [path]))
  }
  const handleStageAll = () => {
    if (!projectId || unstagedFiles.length === 0) return
    void runAction("stage-all", () => onStage(projectId, unstagedFiles.map((entry) => entry.path)))
  }
  const handleUnstageAll = () => {
    if (!projectId || stagedFiles.length === 0) return
    void runAction(
      "unstage-all",
      () => onUnstage(projectId, stagedFiles.map((entry) => entry.path)),
    )
  }
  const handleCommit = () => {
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
  }
  const handleGenerateCommitMessage = () => {
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
  }
  const handleFetch = () => {
    if (!projectId) return
    void runAction("fetch", () => onFetch(projectId), "Fetched from remote.")
  }
  const handlePull = () => {
    if (!projectId) return
    void runAction("pull", () => onPull(projectId)).then((result) => {
      if (result) setActionMessage(result.summary)
    })
  }
  const handlePush = () => {
    if (!projectId) return
    void runAction("push", () => onPush(projectId)).then((result) => {
      if (result) {
        const allOk = result.updates.every((upd) => upd.ok)
        setActionMessage(allOk ? `Pushed ${result.branch}` : "Push completed with warnings.")
      }
    })
  }

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

          {/* File groups */}
          <div className="flex flex-1 min-h-0 flex-col overflow-y-auto scrollbar-thin">
            <FileGroup
              actionKind={actionKind}
              busy={isBusy}
              emptyLabel="No staged changes"
              entries={stagedFiles}
              groupKind="staged"
              groupLabel="Staged Changes"
              onDiscard={handleDiscardOne}
              onSelect={setSelectedPath}
              onStageAll={handleStageAll}
              onToggle={handleUnstageOne}
              onUnstageAll={handleUnstageAll}
              selectedPath={selectedPath}
            />
            <FileGroup
              actionKind={actionKind}
              busy={isBusy}
              emptyLabel="Working tree is clean"
              entries={unstagedFiles}
              groupKind="unstaged"
              groupLabel="Changes"
              onDiscard={handleDiscardOne}
              onSelect={setSelectedPath}
              onStageAll={handleStageAll}
              onToggle={handleStageOne}
              onUnstageAll={handleUnstageAll}
              selectedPath={selectedPath}
            />
          </div>

          {/* Commit input */}
          <div className="shrink-0 border-t border-border/70 bg-sidebar px-3 py-2">
            <textarea
              aria-label="Commit message"
              className="block h-16 w-full resize-none rounded-md border border-border/70 bg-background/50 px-2 py-1.5 text-[12px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
              disabled={isBusy || !projectId}
              onChange={(event) => setCommitMessage(event.target.value)}
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
            <div className="flex-1 overflow-auto scrollbar-thin">
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
// File group (staged / unstaged)
// ---------------------------------------------------------------------------

interface FileGroupProps {
  actionKind: ActionKind | null
  busy: boolean
  emptyLabel: string
  entries: FileEntry[]
  groupKind: "staged" | "unstaged"
  groupLabel: string
  onDiscard: (path: string) => void
  onSelect: (path: string) => void
  onStageAll: () => void
  onToggle: (path: string) => void
  onUnstageAll: () => void
  selectedPath: string | null
}

function FileGroup({
  actionKind,
  busy,
  emptyLabel,
  entries,
  groupKind,
  groupLabel,
  onDiscard,
  onSelect,
  onStageAll,
  onToggle,
  onUnstageAll,
  selectedPath,
}: FileGroupProps) {
  const [collapsed, setCollapsed] = useState(false)

  return (
    <section className="border-b border-border/40 last:border-b-0">
      <div className="group flex w-full items-center gap-2 px-3 py-1.5 text-left">
        <button
          aria-expanded={!collapsed}
          aria-label={collapsed ? `Expand ${groupLabel}` : `Collapse ${groupLabel}`}
          className="flex flex-1 items-center gap-2 text-left"
          onClick={() => setCollapsed((prev) => !prev)}
          type="button"
        >
          <ChevronRight
            className={cn(
              "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
              !collapsed && "rotate-90",
            )}
          />
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            {groupLabel}
          </span>
          <span className="rounded-full bg-muted/70 px-1.5 py-[1px] font-mono text-[10px] tabular-nums text-muted-foreground">
            {entries.length}
          </span>
        </button>
        {entries.length > 0 ? (
          <span className="invisible flex items-center gap-0.5 group-hover:visible">
            {groupKind === "unstaged" ? (
              <GroupActionButton
                busy={actionKind === "stage-all"}
                disabled={busy}
                icon={<Plus className="h-3 w-3" />}
                label="Stage all"
                onClick={onStageAll}
              />
            ) : (
              <GroupActionButton
                busy={actionKind === "unstage-all"}
                disabled={busy}
                icon={<Minus className="h-3 w-3" />}
                label="Unstage all"
                onClick={onUnstageAll}
              />
            )}
          </span>
        ) : null}
      </div>

      {!collapsed ? (
        entries.length === 0 ? (
          <div className="px-7 pb-2 pt-0.5 text-[11px] text-muted-foreground/70">{emptyLabel}</div>
        ) : (
          <ul className="flex flex-col">
            {entries.map((entry) => (
              <FileRow
                busy={busy}
                entry={entry}
                groupKind={groupKind}
                key={`${groupKind}-${entry.path}`}
                onDiscard={onDiscard}
                onSelect={onSelect}
                onToggle={onToggle}
                selected={selectedPath === entry.path}
              />
            ))}
          </ul>
        )
      ) : null}
    </section>
  )
}

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
    <li
      className={cn(
        "group/row flex cursor-pointer items-center gap-1.5 px-3 py-1 text-left transition-colors",
        selected ? "bg-primary/10" : "hover:bg-secondary/40",
      )}
      onClick={() => onSelect(entry.path)}
      role="button"
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
    </li>
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

interface DiffLine {
  kind: "context" | "add" | "remove" | "hunk" | "header"
  prefix: string
  text: string
  oldNo: number | null
  newNo: number | null
}

function parseDiffLines(patch: string): DiffLine[] {
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
  const lines = useMemo(() => parseDiffLines(patch), [patch])

  // Per-line shiki tokenization. We pass each non-header line through shiki
  // independently — losing cross-line context (multiline strings, JSX, …)
  // but matching what GitHub / VS Code do for unified-diff coloring.
  const [tokenizedLines, setTokenizedLines] = useState<(TokenizedLine | null)[]>([])

  useEffect(() => {
    setTokenizedLines([])
    if (!lang) return
    let cancelled = false

    const sources = lines.map((line) =>
      line.kind === "add" || line.kind === "remove" || line.kind === "context" ? line.text : null,
    )

    Promise.all(
      sources.map((src) =>
        src === null || src.length === 0
          ? Promise.resolve(null)
          : tokenizeCode(src, lang, theme.shiki).then((result) => result?.[0] ?? null),
      ),
    ).then((rendered) => {
      if (cancelled) return
      setTokenizedLines(rendered)
    })

    return () => {
      cancelled = true
    }
  }, [lines, lang, theme.shiki])

  return (
    <div className="font-mono text-[12px] leading-[1.55]">
      {lines.map((line, index) => (
        <DiffLineRow key={index} line={line} tokens={tokenizedLines[index] ?? null} />
      ))}
    </div>
  )
}

function DiffLineRow({ line, tokens }: { line: DiffLine; tokens: TokenizedLine | null }) {
  if (line.kind === "hunk") {
    return (
      <div className="flex items-stretch border-y border-info/20 bg-info/10 text-[11px] text-info">
        <span className="w-12 shrink-0 select-none border-r border-info/20" />
        <span className="px-3 py-0.5">{line.text}</span>
      </div>
    )
  }
  if (line.kind === "header") {
    return (
      <div className="flex items-stretch text-[11px] text-muted-foreground/60">
        <span className="w-12 shrink-0 select-none" />
        <span className="px-3 py-0.5">{line.text}</span>
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

  return (
    <div className={cn("flex min-w-0 items-stretch", rowTone)}>
      <span
        className={cn(
          "w-12 shrink-0 select-none px-2 text-right font-mono text-[10.5px] tabular-nums",
          gutterTone,
        )}
      >
        {lineNo ?? ""}
      </span>
      <span className={cn("w-5 shrink-0 select-none text-center", prefixClass)}>{line.prefix}</span>
      <pre className="m-0 min-w-0 flex-1 whitespace-pre-wrap break-all py-px pr-3">
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

function setCachedDiffPatch(cache: DiffPatchCache, key: string, patch: string): void {
  if (cache.has(key)) {
    cache.delete(key)
  }

  cache.set(key, patch)
  while (cache.size > MAX_DIFF_CACHE_ENTRIES) {
    const oldestKey = cache.keys().next().value
    if (oldestKey === undefined) return
    cache.delete(oldestKey)
  }
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
