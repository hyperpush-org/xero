"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Copy,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Search,
  Sparkles,
  Trash2,
  Workflow as WorkflowIcon,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { useDeferredFilterQuery } from "@/lib/input-priority"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { useSidebarWidthMotion } from "@/lib/sidebar-motion"

const MIN_WIDTH = 280
const MAX_WIDTH = 1200
const DEFAULT_WIDTH = 380
const WIDTH_STORAGE_KEY = "xero.workflows.width"

type WorkflowStatus = "idle" | "running" | "succeeded" | "failed"

interface Workflow {
  id: string
  name: string
  description: string
  agents: string[]
  isDefault: boolean
  status: WorkflowStatus
  lastRunLabel: string | null
  runCount: number
}

interface WorkflowsSidebarProps {
  open: boolean
}

const DEFAULT_WORKFLOWS: Workflow[] = [
  {
    id: "code-review",
    name: "Code Review",
    description: "Audit changed files for bugs, security, and style.",
    agents: ["Reviewer", "Security", "Style"],
    isDefault: true,
    status: "idle",
    lastRunLabel: "2h ago",
    runCount: 14,
  },
  {
    id: "generate-tests",
    name: "Generate Tests",
    description: "Plan UAT cases and write unit + integration tests.",
    agents: ["Planner", "Test Writer"],
    isDefault: true,
    status: "idle",
    lastRunLabel: "yesterday",
    runCount: 6,
  },
  {
    id: "refactor-document",
    name: "Refactor & Document",
    description: "Tighten implementation, then document the public surface.",
    agents: ["Refactor", "Docs"],
    isDefault: true,
    status: "idle",
    lastRunLabel: null,
    runCount: 0,
  },
  {
    id: "bug-triage",
    name: "Bug Triage",
    description: "Reproduce, classify, and propose fixes for open issues.",
    agents: ["Reproducer", "Classifier", "Fixer"],
    isDefault: true,
    status: "succeeded",
    lastRunLabel: "12m ago",
    runCount: 3,
  },
  {
    id: "security-audit",
    name: "Security Audit",
    description: "Threat-model the diff and verify mitigations land in code.",
    agents: ["Modeler", "Verifier"],
    isDefault: true,
    status: "failed",
    lastRunLabel: "3d ago",
    runCount: 2,
  },
]

const STATUS_STYLES: Record<WorkflowStatus, { label: string; className: string; dotClassName: string }> = {
  idle: {
    label: "Idle",
    className: "text-muted-foreground",
    dotClassName: "bg-muted-foreground/50",
  },
  running: {
    label: "Running",
    className: "text-primary",
    dotClassName: "bg-primary animate-pulse",
  },
  succeeded: {
    label: "Succeeded",
    className: "text-success",
    dotClassName: "bg-success",
  },
  failed: {
    label: "Failed",
    className: "text-destructive",
    dotClassName: "bg-destructive",
  },
}

export function WorkflowsSidebar({ open }: WorkflowsSidebarProps) {
  const [query, setQuery] = useState("")
  const [searchOpen, setSearchOpen] = useState(false)
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const targetWidth = open ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { animate: false, isResizing })
  const widthRef = useRef(width)
  widthRef.current = width
  const deferredQuery = useDeferredFilterQuery(query)

  const workflows = DEFAULT_WORKFLOWS

  const filtered = useMemo(() => {
    const q = deferredQuery
    if (!q) return workflows
    return workflows.filter(
      (workflow) =>
        workflow.name.toLowerCase().includes(q) ||
        workflow.description.toLowerCase().includes(q) ||
        workflow.agents.some((agent) => agent.toLowerCase().includes(q)),
    )
  }, [deferredQuery, workflows])

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
      latestWidth = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
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
      writePersistedWidth(latestWidth)
    }

    window.addEventListener("pointermove", handleMove)
    window.addEventListener("pointerup", handleUp)
    window.addEventListener("pointercancel", handleUp)
  }, [])

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      const next = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, current + delta))
      writePersistedWidth(next)
      return next
    })
  }, [])

  return (
    <aside
      aria-hidden={!open}
      aria-label="Workflows"
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize workflows sidebar"
        aria-orientation="vertical"
        aria-valuemax={MAX_WIDTH}
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

      <div className="flex h-full min-w-0 shrink-0 flex-col" style={{ width }}>
        <div className="flex min-h-0 flex-1 flex-col">
          <Header
            total={workflows.length}
            searchOpen={searchOpen}
            onToggleSearch={() => {
              setSearchOpen((current) => {
                const next = !current
                if (!next) setQuery("")
                return next
              })
            }}
          />
          {searchOpen ? (
            <Toolbar
              query={query}
              onQueryChange={setQuery}
              onClose={() => {
                setQuery("")
                setSearchOpen(false)
              }}
            />
          ) : null}
          <WorkflowsTable workflows={filtered} />
        </div>
      </div>
    </aside>
  )
}

// ---------------------------------------------------------------------------
// Header / toolbar
// ---------------------------------------------------------------------------

function Header({
  total,
  searchOpen,
  onToggleSearch,
}: {
  total: number
  searchOpen: boolean
  onToggleSearch: () => void
}) {
  return (
    <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 px-3">
      <div className="flex items-center gap-1.5">
        <WorkflowIcon className="h-3.5 w-3.5 text-muted-foreground" />
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
          Workflows
        </span>
        <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
          {total}
        </span>
      </div>
      <div className="flex items-center gap-0.5">
        <button
          aria-label="New workflow"
          className={cn(
            "flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors",
            "hover:bg-primary/10 hover:text-primary",
          )}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label={searchOpen ? "Close search" : "Search workflows"}
          aria-pressed={searchOpen}
          className={cn(
            "flex h-6 w-6 items-center justify-center rounded-md transition-colors",
            searchOpen
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground hover:bg-primary/10 hover:text-primary",
          )}
          onClick={onToggleSearch}
          type="button"
        >
          <Search className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  )
}

function Toolbar({
  query,
  onQueryChange,
  onClose,
}: {
  query: string
  onQueryChange: (value: string) => void
  onClose: () => void
}) {
  const inputRef = useRef<HTMLInputElement | null>(null)
  useEffect(() => {
    inputRef.current?.focus()
  }, [])
  return (
    <div className="border-b border-border/70 px-3 py-2">
      <div className="relative">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground"
        />
        <input
          aria-label="Search workflows"
          className="h-7 w-full rounded-md border border-border/70 bg-background/40 pl-7 pr-2 text-[11.5px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault()
              onClose()
            }
          }}
          placeholder="Search"
          ref={inputRef}
          type="search"
          value={query}
        />
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

function WorkflowsTable({ workflows }: { workflows: Workflow[] }) {
  if (workflows.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        No workflows match.
      </div>
    )
  }

  return (
    <ul className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin py-1">
      {workflows.map((workflow) => (
        <li key={workflow.id}>
          <WorkflowRow workflow={workflow} />
        </li>
      ))}
    </ul>
  )
}

function WorkflowRow({ workflow }: { workflow: Workflow }) {
  const status = STATUS_STYLES[workflow.status]
  // Edit only when the workflow is not user-created. Type tracks `isDefault`,
  // so user-created === !isDefault, and edit visibility is therefore isDefault.
  const showEdit = workflow.isDefault

  return (
    <div className="group relative flex items-start gap-3 px-3 py-3 transition-colors hover:bg-secondary/30">
      <span
        aria-label={status.label}
        className={cn("mt-[7px] h-1.5 w-1.5 shrink-0 rounded-full", status.dotClassName)}
        title={`${status.label}${workflow.lastRunLabel ? ` · last run ${workflow.lastRunLabel}` : ""}`}
      />

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-[13px] font-medium leading-tight text-foreground">
            {workflow.name}
          </span>
          {workflow.isDefault ? (
            <Sparkles
              aria-hidden="true"
              className="h-2.5 w-2.5 shrink-0 text-muted-foreground/45"
            />
          ) : null}
        </div>
        <p className="mt-0.5 line-clamp-1 text-[11.5px] leading-snug text-muted-foreground">
          {workflow.description}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-0.5 self-center">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-label={`More actions for ${workflow.name}`}
              className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[background-color,color,opacity] hover:bg-secondary/70 hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100 data-[state=open]:bg-secondary/70 data-[state=open]:text-foreground data-[state=open]:opacity-100"
              title="More"
              type="button"
            >
              <MoreHorizontal className="h-3.5 w-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" sideOffset={4}>
            {showEdit ? (
              <DropdownMenuItem className="cursor-pointer text-[12px]">
                <Pencil className="mr-2 h-3.5 w-3.5" />
                Edit
              </DropdownMenuItem>
            ) : null}
            <DropdownMenuItem className="cursor-pointer text-[12px]">
              <Copy className="mr-2 h-3.5 w-3.5" />
              Duplicate
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="cursor-pointer text-[12px] text-destructive focus:text-destructive"
              disabled={workflow.isDefault}
            >
              <Trash2 className="mr-2 h-3.5 w-3.5" />
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <button
          aria-label={`Run ${workflow.name}`}
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-primary/15 hover:text-primary"
          title="Run workflow"
          type="button"
        >
          <Play className="h-3.5 w-3.5 fill-current" />
        </button>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Width persistence
// ---------------------------------------------------------------------------

function readPersistedWidth(): number | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(WIDTH_STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed)) return null
    return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, parsed))
  } catch {
    return null
  }
}

function writePersistedWidth(width: number): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage.setItem(WIDTH_STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable */
  }
}
