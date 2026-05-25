"use client"

import { Fragment, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Bot,
  Compass,
  Hammer,
  History,
  MessageCircle,
  Monitor,
  MoreHorizontal,
  Package,
  Pencil,
  Plus,
  Search,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Play,
  Wand2,
  Workflow as WorkflowIcon,
  Wrench,
  type LucideIcon,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { useDeferredFilterQuery } from "@/lib/input-priority"
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
import { Badge } from "@/components/ui/badge"
import { Button, buttonVariants } from "@/components/ui/button"
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
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { useSidebarOpenMotion, useSidebarWidthMotion } from "@/lib/sidebar-motion"
import {
  agentRefKey,
  agentRefsEqual,
  type AgentRefDto,
  type WorkflowAgentSummaryDto,
} from "@/src/lib/xero-model/workflow-agents"
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionScopeLabel,
  type AgentDefaultModelDto,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionScopeDto,
} from "@/src/lib/xero-model/agent-definition"
import type { ProviderModelThinkingEffortDto } from "@/src/lib/xero-model"
import type { ComposerModelOptionView } from "@/src/features/xero/use-xero-desktop-state/runtime-provider"
import type { WorkflowDefinitionSummaryDto } from "@/src/lib/xero-model/workflow-definition"
import type { WorkflowRunDto } from "@/src/lib/xero-model/workflow-run"
import {
  WORKFLOW_TEMPLATE_LIBRARY,
  type WorkflowTemplateIdDto,
} from "@/src/lib/xero-model/workflow-templates"
import { CreateWorkflowDialog } from "./create-workflow-dialog"
import type { CreateEntityDialogView } from "./create-entity-dialog"

const MIN_WIDTH = 280
const MAX_WIDTH = 1200
const DEFAULT_WIDTH = 380
const WIDTH_STORAGE_KEY = "xero.workflows.width"
const TAB_STORAGE_KEY = "xero.library.tab"

type LibraryTab = "workflows" | "agents"

interface WorkflowsSidebarProps {
  open: boolean
  agents?: WorkflowAgentSummaryDto[]
  agentsLoading?: boolean
  agentsError?: Error | null
  workflowDefinitions?: WorkflowDefinitionSummaryDto[]
  workflowRuns?: WorkflowRunDto[]
  workflowsLoading?: boolean
  workflowsError?: Error | null
  selectedWorkflowId?: string | null
  selectedWorkflowTemplateId?: WorkflowTemplateIdDto | null
  selectedWorkflowRunId?: string | null
  selectedAgentRef?: AgentRefDto | null
  onSelectWorkflow?: (workflowId: string) => void
  onSelectWorkflowTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onSelectWorkflowRun?: (runId: string) => void
  onCreateWorkflow?: () => void
  onCreateWorkflowWithAgentCreate?: () => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onStartWorkflowRun?: (workflowId: string) => void
  onCancelWorkflowRun?: (runId: string) => void
  onResumeWorkflowRun?: (runId: string, nodeRunId: string, decision: string) => void
  onSelectAgent?: (ref: AgentRefDto) => void
  onCreateAgent?: () => void
  onCreateAgentByHand?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
  onDeleteAgent?: (ref: AgentRefDto) => Promise<void> | void
  onUseAgentInChat?: (ref: AgentRefDto) => void
  modelOptions?: readonly ComposerModelOptionView[]
  onSetAgentDefaultModel?: (
    agent: WorkflowAgentSummaryDto,
    defaultModel: AgentDefaultModelDto | null,
  ) => Promise<void> | void
}

const AGENT_PROFILE_ICON: Record<AgentDefinitionBaseCapabilityProfileDto, typeof Bot> = {
  observe_only: ShieldCheck,
  planning: Sparkles,
  repository_recon: Compass,
  engineering: Hammer,
  debugging: Wrench,
  agent_builder: Wand2,
  computer_use: Monitor,
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
  workflowDefinitions: workflowDefinitionsProp,
  workflowRuns: workflowRunsProp,
  workflowsLoading = false,
  workflowsError = null,
  selectedWorkflowId = null,
  selectedWorkflowTemplateId = null,
  selectedWorkflowRunId = null,
  selectedAgentRef = null,
  onSelectWorkflow,
  onSelectWorkflowTemplate,
  onSelectWorkflowRun,
  onCreateWorkflow,
  onCreateWorkflowWithAgentCreate,
  onCreateWorkflowFromTemplate,
  onStartWorkflowRun,
  onCancelWorkflowRun,
  onResumeWorkflowRun,
  onSelectAgent,
  onCreateAgent,
  onCreateAgentByHand,
  onEditAgent,
  onDeleteAgent,
  onUseAgentInChat,
  modelOptions = [],
  onSetAgentDefaultModel,
}: WorkflowsSidebarProps) {
  const [tab, setTabState] = useState<LibraryTab>(() => readPersistedTab() ?? "workflows")
  const [query, setQuery] = useState("")
  const [searchOpen, setSearchOpen] = useState(false)
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const motionOpen = useSidebarOpenMotion(open)
  const targetWidth = motionOpen ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const widthRef = useRef(width)
  widthRef.current = width
  const deferredQuery = useDeferredFilterQuery(query)

  const agents = useMemo(() => agentsProp ?? [], [agentsProp])
  const workflowDefinitions = useMemo(
    () => workflowDefinitionsProp ?? [],
    [workflowDefinitionsProp],
  )
  const workflowRuns = useMemo(() => workflowRunsProp ?? [], [workflowRunsProp])
  const [defaultModelTarget, setDefaultModelTarget] =
    useState<WorkflowAgentSummaryDto | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<WorkflowAgentSummaryDto | null>(null)
  const [createWorkflowDialogOpen, setCreateWorkflowDialogOpen] = useState(false)
  const [createWorkflowDialogView, setCreateWorkflowDialogView] =
    useState<CreateEntityDialogView>("choice")
  const canCreateWorkflow = Boolean(
    onCreateWorkflow || onCreateWorkflowWithAgentCreate || onCreateWorkflowFromTemplate,
  )

  const closeCreateWorkflowDialog = useCallback(() => {
    setCreateWorkflowDialogOpen(false)
    setCreateWorkflowDialogView("choice")
  }, [])

  const handleCreateWorkflowDialogOpenChange = useCallback(
    (nextOpen: boolean) => {
      if (nextOpen) {
        setCreateWorkflowDialogOpen(true)
        return
      }
      closeCreateWorkflowDialog()
    },
    [closeCreateWorkflowDialog],
  )

  const handleCreateWorkflowBlank = useCallback(() => {
    if (!onCreateWorkflow) return
    closeCreateWorkflowDialog()
    onCreateWorkflow()
  }, [closeCreateWorkflowDialog, onCreateWorkflow])

  const handleCreateWorkflowWithAgentCreate = useCallback(() => {
    if (!onCreateWorkflowWithAgentCreate) return
    closeCreateWorkflowDialog()
    onCreateWorkflowWithAgentCreate()
  }, [closeCreateWorkflowDialog, onCreateWorkflowWithAgentCreate])

  const handleCreateWorkflowFromTemplate = useCallback(
    (templateId: WorkflowTemplateIdDto) => {
      if (!onCreateWorkflowFromTemplate) return
      closeCreateWorkflowDialog()
      onCreateWorkflowFromTemplate(templateId)
    },
    [closeCreateWorkflowDialog, onCreateWorkflowFromTemplate],
  )

  const setTab = useCallback((next: LibraryTab) => {
    setTabState((current) => {
      if (current === next) return current
      writePersistedTab(next)
      return next
    })
    setQuery("")
  }, [])

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
  const filteredWorkflowDefinitions = useMemo(() => {
    if (tab !== "workflows") return workflowDefinitions
    const q = deferredQuery
    if (!q) return workflowDefinitions
    return workflowDefinitions.filter(
      (definition) =>
        definition.name.toLowerCase().includes(q) ||
        definition.description.toLowerCase().includes(q),
    )
  }, [deferredQuery, tab, workflowDefinitions])

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

  const activeCount = tab === "workflows" ? filteredWorkflowDefinitions.length : filteredAgents.length
  const totalCount = tab === "workflows" ? workflowDefinitions.length : agents.length
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
            onCreateWorkflow={
              canCreateWorkflow ? () => handleCreateWorkflowDialogOpenChange(true) : undefined
            }
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
            agents={filteredAgents}
            workflowDefinitions={filteredWorkflowDefinitions}
            workflowRuns={workflowRuns}
            activeCount={activeCount}
            totalCount={totalCount}
            hasQuery={hasQuery}
            agentsLoading={agentsLoading}
            agentsError={agentsError}
            workflowsLoading={workflowsLoading}
            workflowsError={workflowsError}
            selectedWorkflowId={selectedWorkflowId}
            selectedWorkflowTemplateId={selectedWorkflowTemplateId}
            selectedWorkflowRunId={selectedWorkflowRunId}
            selectedAgentRef={selectedAgentRef}
            onSelectWorkflow={onSelectWorkflow}
            onSelectWorkflowTemplate={onSelectWorkflowTemplate}
            onSelectWorkflowRun={onSelectWorkflowRun}
            onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
            onStartWorkflowRun={onStartWorkflowRun}
            onCancelWorkflowRun={onCancelWorkflowRun}
            onResumeWorkflowRun={onResumeWorkflowRun}
            onSelectAgent={onSelectAgent}
            onEditAgent={onEditAgent}
            onRequestDeleteAgent={setDeleteTarget}
            onUseAgentInChat={onUseAgentInChat}
            onConfigureDefaultModel={setDefaultModelTarget}
          />
        </div>
      </div>
      <AgentDefaultModelDialog
        agent={defaultModelTarget}
        modelOptions={modelOptions}
        open={Boolean(defaultModelTarget)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDefaultModelTarget(null)
        }}
        onSave={async (defaultModel) => {
          if (!defaultModelTarget || !onSetAgentDefaultModel) return
          await onSetAgentDefaultModel(defaultModelTarget, defaultModel)
          setDefaultModelTarget(null)
        }}
      />
      <DeleteAgentConfirmationDialog
        agent={deleteTarget}
        open={Boolean(deleteTarget)}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDeleteTarget(null)
        }}
        onDelete={async (agent) => {
          if (!onDeleteAgent) return
          await onDeleteAgent(agent.ref)
          setDeleteTarget(null)
        }}
      />
      {canCreateWorkflow ? (
        <CreateWorkflowDialog
          open={createWorkflowDialogOpen}
          onOpenChange={handleCreateWorkflowDialogOpenChange}
          view={createWorkflowDialogView}
          onSetView={setCreateWorkflowDialogView}
          canStartBlank={Boolean(onCreateWorkflow)}
          canUseAgentCreate={Boolean(onCreateWorkflowWithAgentCreate)}
          canPickTemplate={Boolean(onCreateWorkflowFromTemplate)}
          onStartBlank={handleCreateWorkflowBlank}
          onUseAgentCreate={handleCreateWorkflowWithAgentCreate}
          onPickTemplate={handleCreateWorkflowFromTemplate}
        />
      ) : null}
    </aside>
  )
}

// ---------------------------------------------------------------------------
// Header / toolbar
// ---------------------------------------------------------------------------

function Header({
  tab,
  agentsCount,
  onTabChange,
  searchOpen,
  onToggleSearch,
  onCreateAgent,
  onCreateAgentByHand,
  onCreateWorkflow,
}: {
  tab: LibraryTab
  agentsCount: number
  onTabChange: (next: LibraryTab) => void
  searchOpen: boolean
  onToggleSearch: () => void
  onCreateAgent?: () => void
  onCreateAgentByHand?: () => void
  onCreateWorkflow?: () => void
}) {
  const isWorkflowsTab = tab === "workflows"
  const newLabel = isWorkflowsTab ? "New workflow" : "New agent"
  const searchLabel = searchOpen
    ? "Close search"
    : isWorkflowsTab
      ? "Search workflows"
      : "Search agents"
  const directCreate = isWorkflowsTab ? onCreateWorkflow : onCreateAgent ?? onCreateAgentByHand
  const createDisabled = !directCreate

  return (
    <div
      className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 px-2"
      role="tablist"
      aria-label="Library sections"
    >
      <div className="flex items-center gap-0.5">
        <TabPill
          active={isWorkflowsTab}
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
              ? "cursor-not-allowed text-muted-foreground/40"
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
  count?: number
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
      {active && count !== undefined ? (
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
  agents,
  workflowDefinitions,
  workflowRuns,
  activeCount,
  totalCount,
  hasQuery,
  agentsLoading,
  agentsError,
  workflowsLoading,
  workflowsError,
  selectedWorkflowId,
  selectedWorkflowTemplateId,
  selectedWorkflowRunId,
  selectedAgentRef,
  onSelectWorkflow,
  onSelectWorkflowTemplate,
  onSelectWorkflowRun,
  onCreateWorkflowFromTemplate,
  onStartWorkflowRun,
  onCancelWorkflowRun,
  onResumeWorkflowRun,
  onSelectAgent,
  onEditAgent,
  onRequestDeleteAgent,
  onUseAgentInChat,
  onConfigureDefaultModel,
}: {
  tab: LibraryTab
  agents: WorkflowAgentSummaryDto[]
  workflowDefinitions: WorkflowDefinitionSummaryDto[]
  workflowRuns: WorkflowRunDto[]
  activeCount: number
  totalCount: number
  hasQuery: boolean
  agentsLoading: boolean
  agentsError: Error | null
  workflowsLoading: boolean
  workflowsError: Error | null
  selectedWorkflowId: string | null
  selectedWorkflowTemplateId: WorkflowTemplateIdDto | null
  selectedWorkflowRunId: string | null
  selectedAgentRef: AgentRefDto | null
  onSelectWorkflow?: (workflowId: string) => void
  onSelectWorkflowTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onSelectWorkflowRun?: (runId: string) => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onStartWorkflowRun?: (workflowId: string) => void
  onCancelWorkflowRun?: (runId: string) => void
  onResumeWorkflowRun?: (runId: string, nodeRunId: string, decision: string) => void
  onSelectAgent?: (ref: AgentRefDto) => void
  onEditAgent?: (ref: AgentRefDto) => void
  onRequestDeleteAgent?: (agent: WorkflowAgentSummaryDto) => void
  onUseAgentInChat?: (ref: AgentRefDto) => void
  onConfigureDefaultModel?: (agent: WorkflowAgentSummaryDto) => void
}) {
  if (tab === "workflows") {
    return (
      <WorkflowsList
        definitions={workflowDefinitions}
        runs={workflowRuns}
        activeCount={activeCount}
        totalCount={totalCount}
        hasQuery={hasQuery}
        loading={workflowsLoading}
        error={workflowsError}
        selectedWorkflowId={selectedWorkflowId}
        selectedWorkflowTemplateId={selectedWorkflowTemplateId}
        selectedWorkflowRunId={selectedWorkflowRunId}
        onSelectWorkflowTemplate={onSelectWorkflowTemplate}
        onSelectWorkflow={onSelectWorkflow}
        onSelectWorkflowRun={onSelectWorkflowRun}
        onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
        onStartWorkflowRun={onStartWorkflowRun}
        onCancelWorkflowRun={onCancelWorkflowRun}
        onResumeWorkflowRun={onResumeWorkflowRun}
      />
    )
  }
  if (agentsError) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1 px-3 py-8 text-center text-[11px] leading-relaxed text-destructive">
        <span>Failed to load agents.</span>
        <span className="text-muted-foreground">{agentsError.message}</span>
      </div>
    )
  }
  if (agentsLoading && agents.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        Loading agents…
      </div>
    )
  }

  if (activeCount === 0) {
    const message = agentsEmptyStateMessage(hasQuery, totalCount)
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        {message}
      </div>
    )
  }

  return (
    <ul className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin py-1">
      {agents.map((agent) => (
        <li key={agentRefKey(agent.ref)}>
          <AgentRow
            agent={agent}
            selected={
              selectedAgentRef ? agentRefsEqual(agent.ref, selectedAgentRef) : false
            }
            onSelect={onSelectAgent}
            onEdit={onEditAgent}
            onRequestDelete={onRequestDeleteAgent}
            onUseInChat={onUseAgentInChat}
            onConfigureDefaultModel={onConfigureDefaultModel}
          />
        </li>
      ))}
    </ul>
  )
}

function agentsEmptyStateMessage(hasQuery: boolean, totalCount: number): string {
  if (hasQuery) return "No agents match."
  if (totalCount === 0) return "No agents yet."
  return "No agents match."
}

function humanizeWorkflowStatus(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function workflowRunTone(status: WorkflowRunDto["status"]): string {
  switch (status) {
    case "running":
    case "completed":
      return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
    case "paused":
      return "border-amber-500/35 bg-amber-500/10 text-amber-700 dark:text-amber-300"
    case "failed":
      return "border-destructive/35 bg-destructive/10 text-destructive"
    case "cancelled":
      return "border-muted-foreground/25 bg-muted text-muted-foreground"
    case "queued":
    default:
      return "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300"
  }
}

function WorkflowsList({
  definitions,
  runs,
  activeCount,
  totalCount,
  hasQuery,
  loading,
  error,
  selectedWorkflowId,
  selectedWorkflowTemplateId,
  selectedWorkflowRunId,
  onSelectWorkflow,
  onSelectWorkflowTemplate,
  onSelectWorkflowRun,
  onCreateWorkflowFromTemplate,
  onStartWorkflowRun,
  onCancelWorkflowRun,
  onResumeWorkflowRun,
}: {
  definitions: WorkflowDefinitionSummaryDto[]
  runs: WorkflowRunDto[]
  activeCount: number
  totalCount: number
  hasQuery: boolean
  loading: boolean
  error: Error | null
  selectedWorkflowId: string | null
  selectedWorkflowTemplateId: WorkflowTemplateIdDto | null
  selectedWorkflowRunId: string | null
  onSelectWorkflow?: (workflowId: string) => void
  onSelectWorkflowTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onSelectWorkflowRun?: (runId: string) => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onStartWorkflowRun?: (workflowId: string) => void
  onCancelWorkflowRun?: (runId: string) => void
  onResumeWorkflowRun?: (runId: string, nodeRunId: string, decision: string) => void
}) {
  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1 px-3 py-8 text-center text-[11px] leading-relaxed text-destructive">
        <span>Failed to load workflows.</span>
        <span className="text-muted-foreground">{error.message}</span>
      </div>
    )
  }
  if (loading && definitions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        Loading workflows…
      </div>
    )
  }
  if (activeCount === 0 && (totalCount > 0 || hasQuery)) {
    return (
      <div className="flex flex-1 items-center justify-center px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground/80">
        No workflows match.
      </div>
    )
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin">
      {definitions.length === 0 && !hasQuery ? (
        <WorkflowTemplates
          selectedTemplateId={selectedWorkflowTemplateId}
          onSelectWorkflowTemplate={onSelectWorkflowTemplate}
          onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
        />
      ) : (
        <ul className="flex flex-col py-1">
          {definitions.map((definition) => {
            const latestRun = runs.find((run) => run.workflowId === definition.id) ?? null
            return (
              <li key={definition.id}>
                <WorkflowRow
                  definition={definition}
                  latestRun={latestRun}
                  selected={definition.id === selectedWorkflowId}
                  onSelect={onSelectWorkflow}
                  onStart={onStartWorkflowRun}
                />
              </li>
            )
          })}
        </ul>
      )}
      {definitions.length > 0 ? (
        <WorkflowTemplates
          compact
          selectedTemplateId={selectedWorkflowTemplateId}
          onSelectWorkflowTemplate={onSelectWorkflowTemplate}
          onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
        />
      ) : null}
      {runs.length > 0 ? (
        <WorkflowRunsTimeline
          runs={runs}
          selectedWorkflowRunId={selectedWorkflowRunId}
          onSelectWorkflowRun={onSelectWorkflowRun}
          onCancelWorkflowRun={onCancelWorkflowRun}
          onResumeWorkflowRun={onResumeWorkflowRun}
        />
      ) : null}
    </div>
  )
}

interface LibraryEntityRowProps {
  name: string
  description?: ReactNode
  icon: LucideIcon
  selected?: boolean
  disabled?: boolean
  ariaLabel: string
  ariaPressed?: boolean
  badges?: ReactNode
  action?: ReactNode
  onActivate?: () => void
}

function LibraryEntityRow({
  name,
  description,
  icon: Icon,
  selected = false,
  disabled = false,
  ariaLabel,
  ariaPressed,
  badges,
  action,
  onActivate,
}: LibraryEntityRowProps) {
  const clickable = Boolean(onActivate) && !disabled
  return (
    <div
      className={cn(
        "group relative flex items-start gap-3 px-3 py-3 transition-colors",
        selected ? "bg-primary/10" : clickable && "hover:bg-secondary/30",
        disabled && "opacity-60",
      )}
    >
      {onActivate ? (
        <button
          type="button"
          onClick={clickable ? onActivate : undefined}
          className={cn("absolute inset-0", clickable ? "cursor-pointer" : "cursor-default")}
          aria-label={ariaLabel}
          aria-pressed={ariaPressed}
          disabled={!clickable}
        />
      ) : null}
      <Icon
        aria-hidden="true"
        className={cn(
          "mt-[3px] h-3.5 w-3.5 shrink-0",
          selected ? "text-primary" : "text-muted-foreground/70",
        )}
      />

      <div className="pointer-events-none relative min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-[13px] font-medium leading-tight text-foreground">
            {name}
          </span>
          {badges ? (
            <span className="flex min-w-0 shrink-0 items-center gap-1">{badges}</span>
          ) : null}
        </div>
        {description ? (
          <p className="mt-0.5 line-clamp-1 text-[11.5px] leading-snug text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>

      {action ? (
        <div className="relative flex shrink-0 items-center gap-0.5 self-center">{action}</div>
      ) : null}
    </div>
  )
}

function WorkflowTemplates({
  compact = false,
  selectedTemplateId = null,
  onSelectWorkflowTemplate,
  onCreateWorkflowFromTemplate,
}: {
  compact?: boolean
  selectedTemplateId?: WorkflowTemplateIdDto | null
  onSelectWorkflowTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
}) {
  return (
    <section className={cn("border-b border-border/60", compact ? "py-1" : "py-2")}>
      <div className="flex items-center gap-2 px-3 py-2">
        <WorkflowIcon className="h-3.5 w-3.5 text-muted-foreground/70" aria-hidden="true" />
        <h3 className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Templates
        </h3>
      </div>
      <div className="flex flex-col">
        {WORKFLOW_TEMPLATE_LIBRARY.map((template) => (
          <LibraryEntityRow
            key={template.id}
            name={template.name}
            description={compact ? undefined : template.description}
            icon={Sparkles}
            selected={template.id === selectedTemplateId}
            ariaLabel={`Inspect workflow template ${template.name}`}
            ariaPressed={template.id === selectedTemplateId}
            disabled={!onSelectWorkflowTemplate}
            onActivate={() => onSelectWorkflowTemplate?.(template.id)}
            badges={
              <>
                <Badge variant="secondary" className="px-1 py-0 text-[9px] leading-tight">
                  {humanizeWorkflowStatus(template.difficulty)}
                </Badge>
                <Badge variant="outline" className="px-1 py-0 text-[9px] leading-tight">
                  {template.nodeCount} nodes
                </Badge>
              </>
            }
            action={
              <LibraryEntityRowMenu
                name={template.name}
                actions={[
                  {
                    label: "Use template",
                    icon: <Sparkles className="mr-2 h-3.5 w-3.5" />,
                    onSelect: onCreateWorkflowFromTemplate
                      ? () => onCreateWorkflowFromTemplate(template.id)
                      : undefined,
                  },
                ]}
              />
            }
          />
        ))}
      </div>
    </section>
  )
}

function WorkflowRow({
  definition,
  latestRun,
  selected,
  onSelect,
  onStart,
}: {
  definition: WorkflowDefinitionSummaryDto
  latestRun: WorkflowRunDto | null
  selected: boolean
  onSelect?: (workflowId: string) => void
  onStart?: (workflowId: string) => void
}) {
  const runBadge = latestRun ? (
    <Badge
      variant="outline"
      className={cn("px-1 py-0 text-[9px] leading-tight", workflowRunTone(latestRun.status))}
    >
      {humanizeWorkflowStatus(latestRun.status)}
    </Badge>
  ) : null

  return (
    <LibraryEntityRow
      name={definition.name}
      description={definition.description}
      icon={WorkflowIcon}
      selected={selected}
      ariaLabel={`Open workflow ${definition.name}`}
      ariaPressed={selected}
      onActivate={() => onSelect?.(definition.id)}
      badges={
        <>
          <Badge variant="outline" className="px-1 py-0 text-[9px] leading-tight">
            v{definition.activeVersionNumber}
          </Badge>
          {runBadge}
        </>
      }
      action={
        <LibraryEntityRowMenu
          name={definition.name}
          actions={[
            {
              label: "Open workflow",
              icon: <WorkflowIcon className="mr-2 h-3.5 w-3.5" />,
              onSelect: onSelect ? () => onSelect(definition.id) : undefined,
            },
            {
              label: "Start run",
              icon: <Play className="mr-2 h-3.5 w-3.5" />,
              onSelect: onStart ? () => onStart(definition.id) : undefined,
            },
          ]}
        />
      }
    />
  )
}

function WorkflowRunsTimeline({
  runs,
  selectedWorkflowRunId,
  onSelectWorkflowRun,
  onCancelWorkflowRun,
  onResumeWorkflowRun,
}: {
  runs: WorkflowRunDto[]
  selectedWorkflowRunId: string | null
  onSelectWorkflowRun?: (runId: string) => void
  onCancelWorkflowRun?: (runId: string) => void
  onResumeWorkflowRun?: (runId: string, nodeRunId: string, decision: string) => void
}) {
  return (
    <section className="px-3 py-3">
      <div className="mb-2 flex items-center gap-2">
        <History className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
        <h3 className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Runs
        </h3>
      </div>
      <div className="space-y-2">
        {runs.slice(0, 12).map((run) => {
          const waiting = run.nodes.find((node) => node.status === "waiting_on_gate") ?? null
          const activeNode =
            run.nodes.find((node) => node.status === "running") ??
            waiting ??
            run.nodes.at(-1) ??
            null
          return (
            <div
              key={run.id}
              className={cn(
                "rounded-md border px-2.5 py-2 text-[11.5px]",
                selectedWorkflowRunId === run.id
                  ? "border-primary/35 bg-primary/5"
                  : "border-border/50 bg-card/55",
              )}
            >
              <button
                type="button"
                className="flex w-full items-center justify-between gap-2 text-left"
                onClick={() => onSelectWorkflowRun?.(run.id)}
              >
                <span className="min-w-0 truncate font-medium text-foreground/90">
                  {run.definitionSnapshot.name}
                </span>
                <span className={cn("shrink-0 rounded border px-1.5 py-[1px] text-[10px]", workflowRunTone(run.status))}>
                  {humanizeWorkflowStatus(run.status)}
                </span>
              </button>
              {activeNode ? (
                <p className="mt-1 truncate text-[10.5px] text-muted-foreground">
                  {activeNode.nodeId} · {humanizeWorkflowStatus(activeNode.status)}
                </p>
              ) : null}
              {run.edgeDecisions.length > 0 ? (
                <p className="mt-1 truncate text-[10.5px] text-muted-foreground/80">
                  Route: {run.edgeDecisions.at(-1)?.edgeId}
                </p>
              ) : null}
              {waiting && onResumeWorkflowRun ? (
                <div className="mt-2 flex gap-1.5">
                  <Button
                    type="button"
                    size="sm"
                    className="h-6 px-2 text-[10.5px]"
                    onClick={() => onResumeWorkflowRun(run.id, waiting.id, "continue")}
                  >
                    Continue
                  </Button>
                </div>
              ) : null}
              {(run.status === "running" || run.status === "paused" || run.status === "queued") &&
              onCancelWorkflowRun ? (
                <button
                  type="button"
                  className="mt-2 text-[10.5px] font-medium text-muted-foreground hover:text-destructive"
                  onClick={() => onCancelWorkflowRun(run.id)}
                >
                  Cancel run
                </button>
              ) : null}
            </div>
          )
        })}
      </div>
    </section>
  )
}

function AgentRow({
  agent,
  selected,
  onSelect,
  onEdit,
  onRequestDelete,
  onUseInChat,
  onConfigureDefaultModel,
}: {
  agent: WorkflowAgentSummaryDto
  selected: boolean
  onSelect?: (ref: AgentRefDto) => void
  onEdit?: (ref: AgentRefDto) => void
  onRequestDelete?: (agent: WorkflowAgentSummaryDto) => void
  onUseInChat?: (ref: AgentRefDto) => void
  onConfigureDefaultModel?: (agent: WorkflowAgentSummaryDto) => void
}) {
  const isBuiltIn = agent.scope === "built_in"
  const Icon = isBuiltIn ? (AGENT_PROFILE_ICON[agent.baseCapabilityProfile] ?? Bot) : Package
  const showEdit = !isBuiltIn

  const handleActivate = () => {
    onSelect?.(agent.ref)
  }

  return (
    <LibraryEntityRow
      name={agent.displayName}
      description={
        agent.description || getAgentDefinitionBaseCapabilityLabel(agent.baseCapabilityProfile)
      }
      icon={Icon}
      selected={selected}
      ariaLabel={`Inspect ${agent.displayName}`}
      ariaPressed={selected}
      onActivate={handleActivate}
      badges={
        <Badge
          variant={SCOPE_BADGE_VARIANT[agent.scope]}
          className="px-1 py-0 text-[9px] leading-tight"
        >
          {getAgentDefinitionScopeLabel(agent.scope)}
        </Badge>
      }
      action={
        <AgentRowMenu
          name={agent.displayName}
          showEdit={showEdit}
          deleteDisabled={isBuiltIn}
          onEdit={onEdit ? () => onEdit(agent.ref) : undefined}
          onDelete={onRequestDelete ? () => onRequestDelete(agent) : undefined}
          onUseInChat={onUseInChat ? () => onUseInChat(agent.ref) : undefined}
          onConfigureDefaultModel={
            onConfigureDefaultModel ? () => onConfigureDefaultModel(agent) : undefined
          }
        />
      }
    />
  )
}

function AgentRowMenu({
  name,
  showEdit,
  deleteDisabled,
  onEdit,
  onDelete,
  onUseInChat,
  onConfigureDefaultModel,
}: {
  name: string
  showEdit: boolean
  deleteDisabled: boolean
  onEdit?: () => void
  onDelete?: () => void
  onUseInChat?: () => void
  onConfigureDefaultModel?: () => void
}) {
  const useInChatEnabled = Boolean(onUseInChat)
  const editEnabled = showEdit && Boolean(onEdit)
  const defaultModelEnabled = Boolean(onConfigureDefaultModel)
  const deleteEnabled = !deleteDisabled && Boolean(onDelete)
  const actions: LibraryEntityRowMenuAction[] = [
    {
      label: "Use in Chat",
      icon: <MessageCircle className="mr-2 h-3.5 w-3.5" />,
      onSelect: onUseInChat,
      disabled: !useInChatEnabled,
    },
    ...(showEdit
      ? [
          {
            label: "Edit",
            icon: <Pencil className="mr-2 h-3.5 w-3.5" />,
            onSelect: onEdit,
            disabled: !editEnabled,
          },
        ]
      : []),
    {
      label: "Default model",
      icon: <SlidersHorizontal className="mr-2 h-3.5 w-3.5" />,
      onSelect: onConfigureDefaultModel,
      disabled: !defaultModelEnabled,
    },
    ...(deleteEnabled
      ? [
          {
            label: "Delete",
            icon: <Trash2 className="mr-2 h-3.5 w-3.5" />,
            onSelect: onDelete,
            destructive: true,
            separatorBefore: true,
          },
        ]
      : []),
  ]

  return <LibraryEntityRowMenu name={name} actions={actions} />
}

interface LibraryEntityRowMenuAction {
  label: string
  icon: ReactNode
  onSelect?: () => void
  disabled?: boolean
  destructive?: boolean
  separatorBefore?: boolean
}

function LibraryEntityRowMenu({
  name,
  actions,
}: {
  name: string
  actions: LibraryEntityRowMenuAction[]
}) {
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
        {actions.map((action) => {
          const disabled = action.disabled ?? !action.onSelect
          return (
            <Fragment key={action.label}>
              {action.separatorBefore ? <DropdownMenuSeparator /> : null}
              <DropdownMenuItem
                className={cn(
                  "cursor-pointer text-[12px]",
                  action.destructive && "text-destructive focus:text-destructive",
                )}
                disabled={disabled}
                onSelect={disabled ? undefined : action.onSelect}
              >
                {action.icon}
                {action.label}
              </DropdownMenuItem>
            </Fragment>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

const INHERIT_MODEL_VALUE = "__provider_default__"

function DeleteAgentConfirmationDialog({
  agent,
  open,
  onOpenChange,
  onDelete,
}: {
  agent: WorkflowAgentSummaryDto | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onDelete: (agent: WorkflowAgentSummaryDto) => Promise<void>
}) {
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) {
      setDeleting(false)
      setError(null)
    }
  }, [open])

  const handleDelete = async () => {
    if (!agent) return
    setDeleting(true)
    setError(null)
    try {
      await onDelete(agent)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Failed to delete the agent.")
      setDeleting(false)
    }
  }

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Delete {agent?.displayName ?? "agent"}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            This removes the user-created agent from the agents list. Existing chat history stays
            available.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error ? (
          <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
            {error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className={buttonVariants({ variant: "destructive" })}
            disabled={deleting || !agent}
            onClick={(event) => {
              event.preventDefault()
              void handleDelete()
            }}
          >
            {deleting ? "Deleting..." : "Delete"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function AgentDefaultModelDialog({
  agent,
  modelOptions,
  open,
  onOpenChange,
  onSave,
}: {
  agent: WorkflowAgentSummaryDto | null
  modelOptions: readonly ComposerModelOptionView[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onSave: (defaultModel: AgentDefaultModelDto | null) => Promise<void>
}) {
  const [selectionKey, setSelectionKey] = useState(INHERIT_MODEL_VALUE)
  const [thinkingEffort, setThinkingEffort] =
    useState<ProviderModelThinkingEffortDto | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!agent || !open) return
    const key =
      agent.defaultModel?.selectionKey?.trim() ||
      (agent.defaultModel
        ? `${agent.defaultModel.providerId}:${agent.defaultModel.modelId}`
        : INHERIT_MODEL_VALUE)
    setSelectionKey(key)
    setThinkingEffort(agent.defaultModel?.thinkingEffort ?? null)
    setError(null)
    setSaving(false)
  }, [agent, open])

  const selectedModel = useMemo(() => {
    if (selectionKey === INHERIT_MODEL_VALUE) return null
    return (
      modelOptions.find((option) => option.selectionKey === selectionKey) ??
      modelOptions.find(
        (option) =>
          agent?.defaultModel &&
          option.providerId === agent.defaultModel.providerId &&
          option.modelId === agent.defaultModel.modelId,
      ) ??
      null
    )
  }, [agent, modelOptions, selectionKey])

  const groupedOptions = useMemo(() => {
    const groups = new Map<string, ComposerModelOptionView[]>()
    for (const option of modelOptions) {
      const list = groups.get(option.providerLabel) ?? []
      list.push(option)
      groups.set(option.providerLabel, list)
    }
    return Array.from(groups.entries())
  }, [modelOptions])

  const thinkingOptions = selectedModel?.thinkingEffortOptions ?? []

  useEffect(() => {
    if (!selectedModel) {
      setThinkingEffort(null)
      return
    }
    if (thinkingEffort && thinkingOptions.includes(thinkingEffort)) {
      return
    }
    setThinkingEffort(selectedModel.defaultThinkingEffort ?? null)
  }, [selectedModel, thinkingEffort, thinkingOptions])

  const handleSave = async () => {
    if (!agent) return
    setSaving(true)
    setError(null)
    try {
      if (selectionKey === INHERIT_MODEL_VALUE) {
        await onSave(null)
        return
      }
      const model = selectedModel
      if (!model) {
        throw new Error("Select a model before saving.")
      }
      await onSave({
        providerId: model.providerId,
        providerProfileId: model.profileId ?? null,
        modelId: model.modelId,
        selectionKey: model.selectionKey,
        thinkingEffort:
          thinkingEffort && model.thinkingEffortOptions.includes(thinkingEffort)
            ? thinkingEffort
            : null,
      })
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Failed to save the default model.")
      setSaving(false)
    }
  }

  const title = agent ? `${agent.displayName} default model` : "Default model"
  const hasModels = modelOptions.length > 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            Pick the model new runs should use when this agent is selected.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="agent-default-model-select">Model</Label>
            <Select
              value={selectionKey}
              onValueChange={(value) => setSelectionKey(value)}
              disabled={saving || !hasModels}
            >
              <SelectTrigger id="agent-default-model-select" className="w-full">
                <SelectValue placeholder={hasModels ? "Select model" : "No models available"} />
              </SelectTrigger>
              <SelectContent className="max-h-[320px]">
                <SelectItem value={INHERIT_MODEL_VALUE}>Use provider default</SelectItem>
                {groupedOptions.map(([providerLabel, options]) => (
                  <SelectGroup key={providerLabel}>
                    <SelectLabel>{providerLabel}</SelectLabel>
                    {options.map((option) => (
                      <SelectItem key={option.selectionKey} value={option.selectionKey}>
                        {option.displayName}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                ))}
              </SelectContent>
            </Select>
          </div>

          {thinkingOptions.length > 0 ? (
            <div className="grid gap-2">
              <Label htmlFor="agent-default-thinking-select">Thinking effort</Label>
              <Select
                value={thinkingEffort ?? selectedModel?.defaultThinkingEffort ?? thinkingOptions[0]}
                onValueChange={(value) =>
                  setThinkingEffort(value as ProviderModelThinkingEffortDto)
                }
                disabled={saving}
              >
                <SelectTrigger id="agent-default-thinking-select" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {thinkingOptions.map((effort) => (
                    <SelectItem key={effort} value={effort}>
                      {formatThinkingEffort(effort)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : null}

          {error ? (
            <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
              {error}
            </p>
          ) : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()} disabled={saving || !agent}>
            {saving ? "Saving..." : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function formatThinkingEffort(effort: ProviderModelThinkingEffortDto): string {
  switch (effort) {
    case "none":
      return "None"
    case "minimal":
      return "Minimal"
    case "low":
      return "Low"
    case "medium":
      return "Medium"
    case "high":
      return "High"
    case "x_high":
      return "Extra high"
  }
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
