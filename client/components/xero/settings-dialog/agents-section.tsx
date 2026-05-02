import { useCallback, useEffect, useMemo, useState } from "react"
import {
  AlertCircle,
  Archive,
  Bot,
  Bug,
  History,
  Loader2,
  MessageCircle,
  RefreshCw,
  Sparkles,
  Wrench,
  X,
} from "lucide-react"

import {
  getAgentDefinitionLifecycleLabel,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionLifecycleStateDto,
  type AgentDefinitionSummaryDto,
  type AgentDefinitionVersionSummaryDto,
} from "@/src/lib/xero-model/agent-definition"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

import { SectionHeader } from "./section-header"

interface AgentsSectionProps {
  projectId: string | null
  projectLabel: string | null
  onListAgentDefinitions?: (request: {
    projectId: string
    includeArchived: boolean
  }) => Promise<{ definitions: AgentDefinitionSummaryDto[] }>
  onArchiveAgentDefinition?: (request: {
    projectId: string
    definitionId: string
  }) => Promise<AgentDefinitionSummaryDto>
  onGetAgentDefinitionVersion?: (request: {
    projectId: string
    definitionId: string
    version: number
  }) => Promise<AgentDefinitionVersionSummaryDto | null>
  onRegistryChanged?: () => void
}

interface DefinitionsState {
  status: "idle" | "loading" | "ready" | "error"
  errorMessage: string | null
  definitions: AgentDefinitionSummaryDto[]
}

const INITIAL_STATE: DefinitionsState = {
  status: "idle",
  errorMessage: null,
  definitions: [],
}

type Tone = "good" | "info" | "warn" | "bad" | "neutral"

const TONE_CLASS: Record<Tone, string> = {
  good: "border-success/30 bg-success/[0.08] text-success",
  info: "border-info/30 bg-info/[0.08] text-info",
  warn: "border-warning/30 bg-warning/[0.08] text-warning",
  bad: "border-destructive/40 bg-destructive/[0.08] text-destructive",
  neutral: "border-border bg-secondary/60 text-foreground/70",
}

function Pill({ tone, children }: { tone: Tone; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex h-[18px] items-center rounded-full border px-1.5 text-[10.5px] font-medium",
        TONE_CLASS[tone],
      )}
    >
      {children}
    </span>
  )
}

function profileIcon(profile: AgentDefinitionBaseCapabilityProfileDto) {
  switch (profile) {
    case "engineering":
      return Wrench
    case "debugging":
      return Bug
    case "agent_builder":
      return Sparkles
    case "observe_only":
    default:
      return MessageCircle
  }
}

function lifecycleTone(state: AgentDefinitionLifecycleStateDto): Tone {
  switch (state) {
    case "active":
      return "good"
    case "draft":
      return "info"
    case "archived":
      return "warn"
  }
}

function formatTimestamp(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) {
    return value
  }
  return new Date(parsed).toLocaleString()
}

export function AgentsSection({
  projectId,
  projectLabel,
  onListAgentDefinitions,
  onArchiveAgentDefinition,
  onGetAgentDefinitionVersion,
  onRegistryChanged,
}: AgentsSectionProps) {
  const [state, setState] = useState<DefinitionsState>(INITIAL_STATE)
  const [includeArchived, setIncludeArchived] = useState(false)
  const [pendingArchiveId, setPendingArchiveId] = useState<string | null>(null)
  const [archiveError, setArchiveError] = useState<string | null>(null)
  const [revision, setRevision] = useState(0)
  const [versionPanel, setVersionPanel] = useState<{
    definitionId: string
    versions: AgentDefinitionVersionSummaryDto[]
    status: "loading" | "ready" | "error"
    errorMessage: string | null
  } | null>(null)

  useEffect(() => {
    if (!projectId || !onListAgentDefinitions) {
      setState({ ...INITIAL_STATE, status: "ready" })
      return
    }

    let cancelled = false
    setState((current) => ({ ...current, status: "loading", errorMessage: null }))
    void onListAgentDefinitions({ projectId, includeArchived })
      .then((response) => {
        if (cancelled) return
        setState({
          status: "ready",
          errorMessage: null,
          definitions: response.definitions,
        })
      })
      .catch((error) => {
        if (cancelled) return
        setState({
          status: "error",
          errorMessage:
            error instanceof Error && error.message.trim().length > 0
              ? error.message
              : "Xero could not load agent definitions.",
          definitions: [],
        })
      })

    return () => {
      cancelled = true
    }
  }, [includeArchived, projectId, onListAgentDefinitions, revision])

  const refresh = useCallback(() => setRevision((current) => current + 1), [])

  const handleArchive = useCallback(
    async (definition: AgentDefinitionSummaryDto) => {
      if (!projectId || !onArchiveAgentDefinition) return
      setPendingArchiveId(definition.definitionId)
      setArchiveError(null)
      try {
        await onArchiveAgentDefinition({
          projectId,
          definitionId: definition.definitionId,
        })
        refresh()
        onRegistryChanged?.()
      } catch (error) {
        setArchiveError(
          error instanceof Error && error.message.trim().length > 0
            ? error.message
            : "Xero could not archive that agent definition.",
        )
      } finally {
        setPendingArchiveId(null)
      }
    },
    [onArchiveAgentDefinition, onRegistryChanged, projectId, refresh],
  )

  const handleViewVersion = useCallback(
    async (definition: AgentDefinitionSummaryDto) => {
      if (!projectId || !onGetAgentDefinitionVersion) return
      setVersionPanel({
        definitionId: definition.definitionId,
        versions: [],
        status: "loading",
        errorMessage: null,
      })
      try {
        const versions: AgentDefinitionVersionSummaryDto[] = []
        for (let v = definition.currentVersion; v >= 1 && v >= definition.currentVersion - 4; v -= 1) {
          const record = await onGetAgentDefinitionVersion({
            projectId,
            definitionId: definition.definitionId,
            version: v,
          })
          if (record) {
            versions.push(record)
          }
        }
        setVersionPanel({
          definitionId: definition.definitionId,
          versions,
          status: "ready",
          errorMessage: null,
        })
      } catch (error) {
        setVersionPanel({
          definitionId: definition.definitionId,
          versions: [],
          status: "error",
          errorMessage:
            error instanceof Error && error.message.trim().length > 0
              ? error.message
              : "Xero could not load this agent's version history.",
        })
      }
    },
    [onGetAgentDefinitionVersion, projectId],
  )

  const groups = useMemo(() => {
    const builtIns: AgentDefinitionSummaryDto[] = []
    const projectAgents: AgentDefinitionSummaryDto[] = []
    const globalAgents: AgentDefinitionSummaryDto[] = []
    for (const def of state.definitions) {
      if (def.isBuiltIn) {
        builtIns.push(def)
      } else if (def.scope === "project_custom") {
        projectAgents.push(def)
      } else if (def.scope === "global_custom") {
        globalAgents.push(def)
      }
    }
    return { builtIns, projectAgents, globalAgents }
  }, [state.definitions])

  if (!projectId) {
    return (
      <div className="flex flex-col gap-4">
        <SectionHeader
          title="Agents"
          description="Manage built-in and custom agents available to this workspace."
        />
        <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-8 text-center">
          <Bot className="mx-auto h-4 w-4 text-muted-foreground" />
          <p className="mt-2 text-[12.5px] font-medium text-foreground">No project open</p>
          <p className="mt-0.5 text-[11.5px] text-muted-foreground">
            Open a project to inspect or manage agent definitions.
          </p>
        </div>
      </div>
    )
  }

  const totalCount = state.definitions.length
  const isLoading = state.status === "loading"

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Agents"
        description={
          projectLabel
            ? `Built-in and custom agents available to ${projectLabel}.`
            : "Built-in and custom agents available to this project."
        }
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={refresh}
            disabled={isLoading}
            className="h-8 gap-1.5 text-[12px]"
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

      <div className="flex items-start justify-between gap-4">
        <p className="max-w-prose text-[12px] leading-[1.5] text-muted-foreground">
          Use Agent Create to design new custom agents. Activation, save and update happen
          there; this surface is for management.
        </p>
        <label className="flex shrink-0 cursor-pointer items-center gap-1.5 text-[11.5px] text-muted-foreground select-none">
          <input
            type="checkbox"
            checked={includeArchived}
            onChange={(event) => setIncludeArchived(event.target.checked)}
            className="size-3.5 cursor-pointer accent-primary"
          />
          Include archived
        </label>
      </div>

      {state.status === "error" ? <ErrorBanner message={state.errorMessage ?? ""} /> : null}
      {archiveError ? <ErrorBanner message={archiveError} /> : null}

      {isLoading && totalCount === 0 ? (
        <div className="flex items-center justify-center gap-2 rounded-md border border-border/60 px-4 py-10 text-[12px] text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Loading agents
        </div>
      ) : (
        <>
          <AgentDefinitionGroup
            title="Built-in"
            count={groups.builtIns.length}
            emptyMessage="Built-in agents have not been seeded for this project."
            definitions={groups.builtIns}
            pendingArchiveId={pendingArchiveId}
            onArchive={handleArchive}
            onViewHistory={handleViewVersion}
            canMutate={false}
            showHistory={Boolean(onGetAgentDefinitionVersion)}
          />

          <AgentDefinitionGroup
            title="Project agents"
            count={groups.projectAgents.length}
            emptyMessage="No project-scoped custom agents yet. Pick Agent Create in the composer to design one."
            definitions={groups.projectAgents}
            pendingArchiveId={pendingArchiveId}
            onArchive={handleArchive}
            onViewHistory={handleViewVersion}
            canMutate={Boolean(onArchiveAgentDefinition)}
            showHistory={Boolean(onGetAgentDefinitionVersion)}
          />

          <AgentDefinitionGroup
            title="Global agents"
            count={groups.globalAgents.length}
            emptyMessage="No global custom agents yet."
            definitions={groups.globalAgents}
            pendingArchiveId={pendingArchiveId}
            onArchive={handleArchive}
            onViewHistory={handleViewVersion}
            canMutate={Boolean(onArchiveAgentDefinition)}
            showHistory={Boolean(onGetAgentDefinitionVersion)}
          />
        </>
      )}

      {versionPanel ? (
        <VersionHistoryPanel panel={versionPanel} onClose={() => setVersionPanel(null)} />
      ) : null}
    </div>
  )
}

interface AgentDefinitionGroupProps {
  title: string
  count: number
  emptyMessage: string
  definitions: AgentDefinitionSummaryDto[]
  pendingArchiveId: string | null
  onArchive: (definition: AgentDefinitionSummaryDto) => Promise<void>
  onViewHistory: (definition: AgentDefinitionSummaryDto) => Promise<void>
  canMutate: boolean
  showHistory: boolean
}

function AgentDefinitionGroup({
  title,
  count,
  emptyMessage,
  definitions,
  pendingArchiveId,
  onArchive,
  onViewHistory,
  canMutate,
  showHistory,
}: AgentDefinitionGroupProps) {
  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex items-baseline justify-between gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          {title}
          <span className="ml-1.5 font-normal text-muted-foreground">{count}</span>
        </h4>
      </div>
      {definitions.length === 0 ? (
        <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-3.5 py-3 text-[11.5px] text-muted-foreground">
          {emptyMessage}
        </div>
      ) : (
        <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
          {definitions.map((definition) => (
            <AgentDefinitionRow
              key={definition.definitionId}
              definition={definition}
              isArchiving={pendingArchiveId === definition.definitionId}
              onArchive={onArchive}
              onViewHistory={onViewHistory}
              canMutate={canMutate}
              showHistory={showHistory}
            />
          ))}
        </div>
      )}
    </section>
  )
}

interface AgentDefinitionRowProps {
  definition: AgentDefinitionSummaryDto
  isArchiving: boolean
  onArchive: (definition: AgentDefinitionSummaryDto) => Promise<void>
  onViewHistory: (definition: AgentDefinitionSummaryDto) => Promise<void>
  canMutate: boolean
  showHistory: boolean
}

function AgentDefinitionRow({
  definition,
  isArchiving,
  onArchive,
  onViewHistory,
  canMutate,
  showHistory,
}: AgentDefinitionRowProps) {
  const Icon = profileIcon(definition.baseCapabilityProfile)
  const isArchived = definition.lifecycleState === "archived"
  const showArchive = canMutate && !definition.isBuiltIn && !isArchived
  const showLifecyclePill = definition.lifecycleState !== "active"

  return (
    <div
      className={cn(
        "group flex items-start gap-3 px-3.5 py-3 transition-opacity",
        isArchived && "opacity-60",
      )}
    >
      <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" aria-hidden="true" />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="truncate text-[13px] font-medium text-foreground">
            {definition.displayName}
          </p>
          {showLifecyclePill ? (
            <Pill tone={lifecycleTone(definition.lifecycleState)}>
              {getAgentDefinitionLifecycleLabel(definition.lifecycleState)}
            </Pill>
          ) : null}
        </div>
        <p className="mt-0.5 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">
          {definition.description}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-0.5">
        {showHistory ? (
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 text-muted-foreground/70 opacity-0 transition-opacity hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
            aria-label="Version history"
            title="Version history"
            onClick={() => void onViewHistory(definition)}
          >
            <History className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        {showArchive ? (
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 text-muted-foreground/70 opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
            aria-label="Archive"
            title="Archive"
            onClick={() => void onArchive(definition)}
            disabled={isArchiving}
          >
            {isArchiving ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Archive className="h-3.5 w-3.5" />
            )}
          </Button>
        ) : null}
      </div>
    </div>
  )
}

interface VersionHistoryPanelProps {
  panel: {
    definitionId: string
    versions: AgentDefinitionVersionSummaryDto[]
    status: "loading" | "ready" | "error"
    errorMessage: string | null
  }
  onClose: () => void
}

function VersionHistoryPanel({ panel, onClose }: VersionHistoryPanelProps) {
  return (
    <section className="overflow-hidden rounded-md border border-border/60 bg-secondary/10">
      <header className="flex items-center justify-between gap-3 border-b border-border/40 px-3.5 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          <History className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <p className="truncate text-[12.5px] font-semibold text-foreground">
            Version history
          </p>
          <span className="truncate font-mono text-[11px] text-muted-foreground">
            {panel.definitionId}
          </span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 text-muted-foreground hover:text-foreground"
          onClick={onClose}
          aria-label="Close version history"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </header>

      <div className="px-3.5 py-2.5">
        {panel.status === "loading" ? (
          <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Loading recent versions
          </div>
        ) : panel.status === "error" ? (
          <p className="text-[12px] text-destructive">
            {panel.errorMessage ?? "Could not load version history."}
          </p>
        ) : panel.versions.length === 0 ? (
          <p className="text-[12px] text-muted-foreground">
            No version snapshots are available.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {panel.versions.map((version) => {
              const validationTone: Tone =
                version.validationStatus === "valid"
                  ? "good"
                  : version.validationStatus === "invalid"
                    ? "bad"
                    : "neutral"
              return (
                <li
                  key={`${version.definitionId}-${version.version}`}
                  className="flex items-center justify-between gap-3 rounded-md border border-border/40 bg-background/40 px-3 py-2"
                >
                  <div className="flex items-center gap-2 text-[12px]">
                    <span className="font-medium text-foreground">
                      Version {version.version}
                    </span>
                    <Pill tone={validationTone}>{version.validationStatus ?? "unknown"}</Pill>
                    {version.validationDiagnosticCount > 0 ? (
                      <span className="text-[11px] text-muted-foreground">
                        {version.validationDiagnosticCount} diagnostic
                        {version.validationDiagnosticCount === 1 ? "" : "s"}
                      </span>
                    ) : null}
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    {formatTimestamp(version.createdAt)}
                  </span>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </section>
  )
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12.5px] text-destructive">
      <AlertCircle className="mt-px h-3.5 w-3.5 shrink-0" />
      <span>{message}</span>
    </div>
  )
}
