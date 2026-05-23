"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Bot,
  Compass,
  Hammer,
  MessageCircle,
  MoreHorizontal,
  Package,
  Pencil,
  Plus,
  Search,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Wand2,
  Workflow as WorkflowIcon,
  Wrench,
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
  selectedAgentRef?: AgentRefDto | null
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
  const [defaultModelTarget, setDefaultModelTarget] =
    useState<WorkflowAgentSummaryDto | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<WorkflowAgentSummaryDto | null>(null)

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

  const activeCount = filteredAgents.length
  const totalCount = agents.length
  const hasQuery = deferredQuery.length > 0
  const searchPlaceholder = "Search agents"

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
          />
          {searchOpen && tab === "agents" ? (
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
            activeCount={activeCount}
            totalCount={totalCount}
            hasQuery={hasQuery}
            agentsLoading={agentsLoading}
            agentsError={agentsError}
            selectedAgentRef={selectedAgentRef}
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
}: {
  tab: LibraryTab
  agentsCount: number
  onTabChange: (next: LibraryTab) => void
  searchOpen: boolean
  onToggleSearch: () => void
  onCreateAgent?: () => void
  onCreateAgentByHand?: () => void
}) {
  const isWorkflowsTab = tab === "workflows"
  const newLabel = isWorkflowsTab ? "New workflow (Coming soon)" : "New agent"
  const searchLabel = searchOpen ? "Close search" : "Search agents"
  const directCreate = isWorkflowsTab ? undefined : onCreateAgent ?? onCreateAgentByHand
  const createDisabled = isWorkflowsTab || !directCreate

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
        {isWorkflowsTab ? null : (
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
        )}
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
  activeCount,
  totalCount,
  hasQuery,
  agentsLoading,
  agentsError,
  selectedAgentRef,
  onSelectAgent,
  onEditAgent,
  onRequestDeleteAgent,
  onUseAgentInChat,
  onConfigureDefaultModel,
}: {
  tab: LibraryTab
  agents: WorkflowAgentSummaryDto[]
  activeCount: number
  totalCount: number
  hasQuery: boolean
  agentsLoading: boolean
  agentsError: Error | null
  selectedAgentRef: AgentRefDto | null
  onSelectAgent?: (ref: AgentRefDto) => void
  onEditAgent?: (ref: AgentRefDto) => void
  onRequestDeleteAgent?: (agent: WorkflowAgentSummaryDto) => void
  onUseAgentInChat?: (ref: AgentRefDto) => void
  onConfigureDefaultModel?: (agent: WorkflowAgentSummaryDto) => void
}) {
  if (tab === "workflows") {
    return <WorkflowsComingSoon />
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

function WorkflowsComingSoon() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-10 text-center">
      <div className="flex h-11 w-11 items-center justify-center rounded-2xl border border-border/70 bg-card/80 shadow-sm">
        <WorkflowIcon className="h-5 w-5 text-foreground/70" aria-hidden="true" />
      </div>
      <Badge
        variant="outline"
        className="text-[9.5px] uppercase tracking-[0.14em] font-semibold text-muted-foreground"
      >
        Coming soon
      </Badge>
    </div>
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
          onDelete={onRequestDelete ? () => onRequestDelete(agent) : undefined}
          onUseInChat={onUseInChat ? () => onUseInChat(agent.ref) : undefined}
          onConfigureDefaultModel={
            onConfigureDefaultModel ? () => onConfigureDefaultModel(agent) : undefined
          }
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
        <DropdownMenuItem
          className="cursor-pointer text-[12px]"
          disabled={!useInChatEnabled}
          onSelect={useInChatEnabled ? onUseInChat : undefined}
        >
          <MessageCircle className="mr-2 h-3.5 w-3.5" />
          Use in Chat
        </DropdownMenuItem>
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
          disabled={!defaultModelEnabled}
          onSelect={defaultModelEnabled ? onConfigureDefaultModel : undefined}
        >
          <SlidersHorizontal className="mr-2 h-3.5 w-3.5" />
          Default model
        </DropdownMenuItem>
        {deleteEnabled ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="cursor-pointer text-[12px] text-destructive focus:text-destructive"
              onSelect={onDelete}
            >
              <Trash2 className="mr-2 h-3.5 w-3.5" />
              Delete
            </DropdownMenuItem>
          </>
        ) : null}
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
