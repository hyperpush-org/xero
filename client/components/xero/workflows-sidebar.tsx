"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Bot,
  Compass,
  Copy,
  Hammer,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Search,
  ShieldCheck,
  Sparkles,
  TestTube,
  Trash2,
  Wand2,
  Wrench,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { useDeferredFilterQuery } from "@/lib/input-priority"
import { Badge } from "@/components/ui/badge"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { useSidebarWidthMotion } from "@/lib/sidebar-motion"
import {
  agentRefKey,
  agentRefsEqual,
  type AgentRefDto,
  type WorkflowAgentSummaryDto,
} from "@/src/lib/xero-model/workflow-agents"
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionScopeLabel,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionScopeDto,
} from "@/src/lib/xero-model/agent-definition"

const MIN_WIDTH = 280
const MAX_WIDTH = 1200
const DEFAULT_WIDTH = 380
const WIDTH_STORAGE_KEY = "xero.workflows.width"
const TAB_STORAGE_KEY = "xero.library.tab"

type LibraryTab = "workflows" | "agents"

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
  agents?: WorkflowAgentSummaryDto[]
  agentsLoading?: boolean
  agentsError?: Error | null
  selectedAgentRef?: AgentRefDto | null
  onSelectAgent?: (ref: AgentRefDto) => void
  onCreateAgent?: () => void
  onCreateAgentByHand?: () => void
  onCreateWorkflow?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
  onDuplicateAgent?: (ref: AgentRefDto) => void
  onDeleteAgent?: (ref: AgentRefDto) => void
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

const AGENT_PROFILE_ICON: Record<AgentDefinitionBaseCapabilityProfileDto, typeof Bot> = {
  observe_only: ShieldCheck,
  planning: Sparkles,
  repository_recon: Compass,
  engineering: Hammer,
  debugging: Wrench,
  agent_builder: Wand2,
  harness_test: TestTube,
}

const SCOPE_BADGE_VARIANT: Record<AgentDefinitionScopeDto, "default" | "secondary" | "outline"> = {
  built_in: "secondary",
  global_custom: "default",
  project_custom: "outline",
}

export function WorkflowsSidebar({
  open,
  agents: agentsProp,
  agentsLoading = false,
  agentsError = null,
  selectedAgentRef = null,
  onSelectAgent,
  onCreateAgent,
  onCreateAgentByHand,
  onCreateWorkflow,
  onEditAgent,
  onDuplicateAgent,
  onDeleteAgent,
}: WorkflowsSidebarProps) {
  const [tab, setTabState] = useState<LibraryTab>(() => readPersistedTab() ?? "workflows")
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
  const agents = useMemo(() => agentsProp ?? [], [agentsProp])

  const setTab = useCallback((next: LibraryTab) => {
    setTabState((current) => {
      if (current === next) return current
      writePersistedTab(next)
      return next
    })
    setQuery("")
  }, [])

  const filteredWorkflows = useMemo(() => {
    if (tab !== "workflows") return workflows
    const q = deferredQuery
    if (!q) return workflows
    return workflows.filter(
      (workflow) =>
        workflow.name.toLowerCase().includes(q) ||
        workflow.description.toLowerCase().includes(q) ||
        workflow.agents.some((agent) => agent.toLowerCase().includes(q)),
    )
  }, [deferredQuery, tab, workflows])

  const filteredAgents = useMemo(() => {
    if (tab !== "agents") return agents
    const q = deferredQuery
    if (!q) return agents
    return agents.filter(
      (agent) =>
        agent.displayName.toLowerCase().includes(q) ||
        agent.shortLabel.toLowerCase().includes(q) ||
        agent.description.toLowerCase().includes(q) ||
        agent.baseCapabilityProfile.toLowerCase().includes(q),
    )
  }, [agents, deferredQuery, tab])

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

  const activeCount = tab === "workflows" ? filteredWorkflows.length : filteredAgents.length
  const totalCount = tab === "workflows" ? workflows.length : agents.length
  const hasQuery = deferredQuery.length > 0
  const searchPlaceholder = tab === "workflows" ? "Search workflows" : "Search agents"

  return (
    <aside
      aria-hidden={!open}
      aria-label="Library"
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize library sidebar"
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
            tab={tab}
            workflowsCount={workflows.length}
            agentsCount={agents.length}
            onTabChange={setTab}
            searchOpen={searchOpen}
            onToggleSearch={() => {
              setSearchOpen((current) => {
                const next = !current
                if (!next) setQuery("")
                return next
              })
            }}
            onCreateAgent={onCreateAgent}
            onCreateAgentByHand={onCreateAgentByHand}
            onCreateWorkflow={onCreateWorkflow}
          />
          {searchOpen ? (
            <Toolbar
              query={query}
              placeholder={searchPlaceholder}
              onQueryChange={setQuery}
              onClose={() => {
                setQuery("")
                setSearchOpen(false)
              }}
            />
          ) : null}
          <LibraryList
            tab={tab}
            workflows={filteredWorkflows}
            agents={filteredAgents}
            activeCount={activeCount}
            totalCount={totalCount}
            hasQuery={hasQuery}
            agentsLoading={agentsLoading}
            agentsError={agentsError}
            selectedAgentRef={selectedAgentRef}
            onSelectAgent={onSelectAgent}
            onEditAgent={onEditAgent}
            onDuplicateAgent={onDuplicateAgent}
            onDeleteAgent={onDeleteAgent}
          />
        </div>
      </div>
    </aside>
  )
}

// ---------------------------------------------------------------------------
// Header / toolbar
// ---------------------------------------------------------------------------

function Header({
  tab,
  workflowsCount,
  agentsCount,
  onTabChange,
  searchOpen,
  onToggleSearch,
  onCreateAgent,
  onCreateAgentByHand,
  onCreateWorkflow,
}: {
  tab: LibraryTab
  workflowsCount: number
  agentsCount: number
  onTabChange: (next: LibraryTab) => void
  searchOpen: boolean
  onToggleSearch: () => void
  onCreateAgent?: () => void
  onCreateAgentByHand?: () => void
  onCreateWorkflow?: () => void
}) {
  const newLabel = tab === "workflows" ? "New workflow" : "New agent"
  const searchLabel = searchOpen
    ? "Close search"
    : tab === "workflows"
      ? "Search workflows"
      : "Search agents"
  const directCreate =
    tab === "agents"
      ? onCreateAgent ?? onCreateAgentByHand
      : onCreateWorkflow
  const createDisabled = !directCreate

  return (
    <div
      className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 px-2"
      role="tablist"
      aria-label="Library sections"
    >
      <div className="flex items-center gap-0.5">
        <TabPill
          active={tab === "workflows"}
          count={workflowsCount}
          label="Workflows"
          onSelect={() => onTabChange("workflows")}
        />
        <TabPill
          active={tab === "agents"}
          count={agentsCount}
          label="Agents"
          onSelect={() => onTabChange("agents")}
        />
      </div>
      <div className="flex items-center gap-0.5">
        <button
          aria-label={newLabel}
          className={cn(
            "flex h-6 w-6 items-center justify-center rounded-md transition-colors",
            createDisabled
              ? "text-muted-foreground/40"
              : "text-muted-foreground hover:bg-primary/10 hover:text-primary",
          )}
          disabled={createDisabled}
          onClick={directCreate}
          title={newLabel}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label={searchLabel}
          aria-pressed={searchOpen}
          className={cn(
            "flex h-6 w-6 items-center justify-center rounded-md transition-colors",
            searchOpen
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground hover:bg-primary/10 hover:text-primary",
          )}
          onClick={onToggleSearch}
          title={searchLabel}
          type="button"
        >
          <Search className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  )
}

function TabPill({
  active,
  count,
  label,
  onSelect,
}: {
  active: boolean
  count: number
  label: string
  onSelect: () => void
}) {
  return (
    <button
      aria-selected={active}
      className={cn(
        "flex h-6 items-center gap-1.5 rounded-md px-2 text-[10.5px] font-semibold uppercase tracking-[0.1em] transition-colors",
        active
          ? "bg-secondary/70 text-foreground"
          : "text-muted-foreground hover:bg-secondary/40 hover:text-foreground",
      )}
      onClick={onSelect}
      role="tab"
      tabIndex={active ? 0 : -1}
      type="button"
    >
      {label}
      {active ? (
        <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
          {count}
        </span>
      ) : null}
    </button>
  )
}

function Toolbar({
  query,
  placeholder,
  onQueryChange,
  onClose,
}: {
  query: string
  placeholder: string
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
          aria-label={placeholder}
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
// List
// ---------------------------------------------------------------------------

function LibraryList({
  tab,
  workflows,
  agents,
  activeCount,
  totalCount,
  hasQuery,
  agentsLoading,
  agentsError,
  selectedAgentRef,
  onSelectAgent,
  onEditAgent,
  onDuplicateAgent,
  onDeleteAgent,
}: {
  tab: LibraryTab
  workflows: Workflow[]
  agents: WorkflowAgentSummaryDto[]
  activeCount: number
  totalCount: number
  hasQuery: boolean
  agentsLoading: boolean
  agentsError: Error | null
  selectedAgentRef: AgentRefDto | null
  onSelectAgent?: (ref: AgentRefDto) => void
  onEditAgent?: (ref: AgentRefDto) => void
  onDuplicateAgent?: (ref: AgentRefDto) => void
  onDeleteAgent?: (ref: AgentRefDto) => void
}) {
  if (tab === "agents" && agentsError) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1 px-3 py-8 text-center text-[11px] leading-relaxed text-destructive">
        <span>Failed to load agents.</span>
        <span className="text-muted-foreground">{agentsError.message}</span>
      </div>
    )
  }
  if (tab === "agents" && agentsLoading && agents.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        Loading agents…
      </div>
    )
  }

  if (activeCount === 0) {
    const message = emptyStateMessage(tab, hasQuery, totalCount)
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        {message}
      </div>
    )
  }

  return (
    <ul className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin py-1">
      {tab === "workflows"
        ? workflows.map((workflow) => (
            <li key={workflow.id}>
              <WorkflowRow workflow={workflow} />
            </li>
          ))
        : agents.map((agent) => (
            <li key={agentRefKey(agent.ref)}>
              <AgentRow
                agent={agent}
                selected={
                  selectedAgentRef ? agentRefsEqual(agent.ref, selectedAgentRef) : false
                }
                onSelect={onSelectAgent}
                onEdit={onEditAgent}
                onDuplicate={onDuplicateAgent}
                onDelete={onDeleteAgent}
              />
            </li>
          ))}
    </ul>
  )
}

function emptyStateMessage(tab: LibraryTab, hasQuery: boolean, totalCount: number): string {
  if (hasQuery) {
    return tab === "workflows" ? "No workflows match." : "No agents match."
  }
  if (totalCount === 0) {
    return tab === "workflows" ? "No workflows yet." : "No agents yet."
  }
  return tab === "workflows" ? "No workflows match." : "No agents match."
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
        <RowMenu
          name={workflow.name}
          showEdit={showEdit}
          deleteDisabled={workflow.isDefault}
        />
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

function AgentRow({
  agent,
  selected,
  onSelect,
  onEdit,
  onDuplicate,
  onDelete,
}: {
  agent: WorkflowAgentSummaryDto
  selected: boolean
  onSelect?: (ref: AgentRefDto) => void
  onEdit?: (ref: AgentRefDto) => void
  onDuplicate?: (ref: AgentRefDto) => void
  onDelete?: (ref: AgentRefDto) => void
}) {
  const Icon = AGENT_PROFILE_ICON[agent.baseCapabilityProfile] ?? Bot
  const isBuiltIn = agent.scope === "built_in"
  const showEdit = !isBuiltIn

  const handleActivate = () => {
    onSelect?.(agent.ref)
  }

  return (
    <div
      className={cn(
        "group relative flex items-start gap-3 px-3 py-3 transition-colors",
        selected ? "bg-primary/10" : "hover:bg-secondary/30",
      )}
    >
      <button
        type="button"
        onClick={handleActivate}
        className="absolute inset-0 cursor-pointer"
        aria-label={`Inspect ${agent.displayName}`}
        aria-pressed={selected}
      />
      <Icon
        aria-hidden="true"
        className={cn(
          "mt-[3px] h-3.5 w-3.5 shrink-0",
          selected ? "text-primary" : "text-muted-foreground/70",
        )}
      />

      <div className="min-w-0 flex-1 relative pointer-events-none">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              "truncate text-[13px] font-medium leading-tight",
              selected ? "text-foreground" : "text-foreground",
            )}
          >
            {agent.displayName}
          </span>
          <Badge
            variant={SCOPE_BADGE_VARIANT[agent.scope]}
            className="text-[9px] px-1 py-0 leading-tight"
          >
            {getAgentDefinitionScopeLabel(agent.scope)}
          </Badge>
        </div>
        <p className="mt-0.5 line-clamp-1 text-[11.5px] leading-snug text-muted-foreground">
          {agent.description || getAgentDefinitionBaseCapabilityLabel(agent.baseCapabilityProfile)}
        </p>
      </div>

      <div className="relative flex shrink-0 items-center gap-0.5 self-center">
        <RowMenu
          name={agent.displayName}
          showEdit={showEdit}
          deleteDisabled={isBuiltIn}
          onEdit={onEdit ? () => onEdit(agent.ref) : undefined}
          onDuplicate={onDuplicate ? () => onDuplicate(agent.ref) : undefined}
          onDelete={onDelete ? () => onDelete(agent.ref) : undefined}
        />
      </div>
    </div>
  )
}

function RowMenu({
  name,
  showEdit,
  deleteDisabled,
  onEdit,
  onDuplicate,
  onDelete,
}: {
  name: string
  showEdit: boolean
  deleteDisabled: boolean
  onEdit?: () => void
  onDuplicate?: () => void
  onDelete?: () => void
}) {
  const editEnabled = showEdit && Boolean(onEdit)
  const duplicateEnabled = Boolean(onDuplicate)
  const deleteEnabled = !deleteDisabled && Boolean(onDelete)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          aria-label={`More actions for ${name}`}
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[background-color,color,opacity] hover:bg-secondary/70 hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100 data-[state=open]:bg-secondary/70 data-[state=open]:text-foreground data-[state=open]:opacity-100"
          title="More"
          type="button"
        >
          <MoreHorizontal className="h-3.5 w-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={4}>
        {showEdit ? (
          <DropdownMenuItem
            className="cursor-pointer text-[12px]"
            disabled={!editEnabled}
            onSelect={editEnabled ? onEdit : undefined}
          >
            <Pencil className="mr-2 h-3.5 w-3.5" />
            Edit
          </DropdownMenuItem>
        ) : null}
        <DropdownMenuItem
          className="cursor-pointer text-[12px]"
          disabled={!duplicateEnabled}
          onSelect={duplicateEnabled ? onDuplicate : undefined}
        >
          <Copy className="mr-2 h-3.5 w-3.5" />
          Duplicate
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          className="cursor-pointer text-[12px] text-destructive focus:text-destructive"
          disabled={!deleteEnabled}
          onSelect={deleteEnabled ? onDelete : undefined}
        >
          <Trash2 className="mr-2 h-3.5 w-3.5" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// ---------------------------------------------------------------------------
// Persistence
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

function readPersistedTab(): LibraryTab | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(TAB_STORAGE_KEY)
    if (raw === "workflows" || raw === "agents") return raw
    return null
  } catch {
    return null
  }
}

function writePersistedTab(tab: LibraryTab): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage.setItem(TAB_STORAGE_KEY, tab)
  } catch {
    /* storage unavailable */
  }
}
