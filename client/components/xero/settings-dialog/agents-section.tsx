import { useCallback, useEffect, useMemo, useState } from "react"
import { Archive, Bot, Bug, History, Loader2, MessageCircle, RefreshCw, Sparkles, Wrench } from "lucide-react"

import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionLifecycleLabel,
  getAgentDefinitionScopeLabel,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionSummaryDto,
  type AgentDefinitionVersionSummaryDto,
} from "@/src/lib/xero-model/agent-definition"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
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
      <div className="flex flex-1 flex-col gap-4 p-4">
        <SectionHeader
          title="Agents"
          description="Manage built-in and custom agents available to this workspace."
        />
        <p className="text-[12px] text-muted-foreground">
          Open a project to inspect or manage agent definitions.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-1 flex-col gap-4 p-4">
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
            disabled={state.status === "loading"}
            className="h-7 gap-1.5"
          >
            {state.status === "loading" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
        }
      />

      <div className="flex items-center justify-between gap-2">
        <p className="text-[12px] text-muted-foreground">
          Use Agent Create to design new custom agents. Activation, save and update happen there;
          this surface is for management.
        </p>
        <label className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <input
            type="checkbox"
            checked={includeArchived}
            onChange={(event) => setIncludeArchived(event.target.checked)}
            className="size-3.5 accent-primary"
          />
          Include archived
        </label>
      </div>

      {state.status === "error" ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-[12px] text-destructive">
          {state.errorMessage}
        </div>
      ) : null}

      {archiveError ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-[12px] text-destructive">
          {archiveError}
        </div>
      ) : null}

      <AgentDefinitionGroup
        title="Built-in agents"
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
        emptyMessage="No global custom agents yet."
        definitions={groups.globalAgents}
        pendingArchiveId={pendingArchiveId}
        onArchive={handleArchive}
        onViewHistory={handleViewVersion}
        canMutate={Boolean(onArchiveAgentDefinition)}
        showHistory={Boolean(onGetAgentDefinitionVersion)}
      />

      {versionPanel ? (
        <Card className="border-border/60">
          <CardHeader className="pb-3">
            <CardTitle className="flex items-center gap-2 text-[13px] font-semibold">
              <History className="h-4 w-4 text-muted-foreground" />
              Version history · {versionPanel.definitionId}
            </CardTitle>
            <CardDescription className="text-[12px]">
              {versionPanel.status === "loading"
                ? "Loading recent versions..."
                : versionPanel.status === "error"
                  ? versionPanel.errorMessage
                  : versionPanel.versions.length > 0
                    ? `Showing the most recent ${versionPanel.versions.length} version snapshot(s).`
                    : "No version snapshots are available."}
            </CardDescription>
          </CardHeader>
          {versionPanel.status === "ready" && versionPanel.versions.length > 0 ? (
            <CardContent className="flex flex-col gap-2">
              {versionPanel.versions.map((version) => (
                <div
                  key={`${version.definitionId}-${version.version}`}
                  className="rounded-md border border-border/60 bg-card/50 p-3"
                >
                  <div className="flex items-center justify-between text-[12px] text-foreground">
                    <span className="font-semibold">Version {version.version}</span>
                    <span className="text-muted-foreground">
                      {new Date(version.createdAt).toLocaleString()}
                    </span>
                  </div>
                  <div className="mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground">
                    <span>
                      Validation: {version.validationStatus ?? "unknown"}
                      {version.validationDiagnosticCount > 0
                        ? ` · ${version.validationDiagnosticCount} diagnostic${
                            version.validationDiagnosticCount === 1 ? "" : "s"
                          }`
                        : ""}
                    </span>
                  </div>
                </div>
              ))}
            </CardContent>
          ) : null}
          <CardFooter className="pt-3">
            <Button variant="ghost" size="sm" onClick={() => setVersionPanel(null)}>
              Close
            </Button>
          </CardFooter>
        </Card>
      ) : null}
    </div>
  )
}

interface AgentDefinitionGroupProps {
  title: string
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
  emptyMessage,
  definitions,
  pendingArchiveId,
  onArchive,
  onViewHistory,
  canMutate,
  showHistory,
}: AgentDefinitionGroupProps) {
  return (
    <section className="flex flex-col gap-2">
      <h4 className="text-[12px] font-semibold uppercase tracking-wider text-muted-foreground">
        {title}
      </h4>
      {definitions.length === 0 ? (
        <p className="rounded-md border border-dashed border-border/60 bg-muted/30 px-3 py-2 text-[12px] text-muted-foreground">
          {emptyMessage}
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          {definitions.map((definition) => {
            const Icon = profileIcon(definition.baseCapabilityProfile)
            const isArchiving = pendingArchiveId === definition.definitionId
            const isArchived = definition.lifecycleState === "archived"
            return (
              <Card
                key={definition.definitionId}
                className={cn(
                  "border-border/60 transition-opacity",
                  isArchived ? "opacity-70" : null,
                )}
              >
                <CardHeader className="flex flex-row items-start gap-3 pb-2">
                  <Icon className="mt-0.5 h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  <div className="min-w-0 flex-1">
                    <CardTitle className="flex flex-wrap items-center gap-2 text-[13px] font-semibold">
                      <span className="truncate">{definition.displayName}</span>
                      <span className="font-mono text-[10.5px] font-normal text-muted-foreground">
                        {definition.definitionId}
                      </span>
                    </CardTitle>
                    <CardDescription className="mt-1 line-clamp-2 text-[12px]">
                      {definition.description}
                    </CardDescription>
                  </div>
                  <div className="flex shrink-0 flex-col items-end gap-1">
                    <Badge variant="outline" className="text-[10px]">
                      {getAgentDefinitionScopeLabel(definition.scope)}
                    </Badge>
                    <Badge
                      variant={isArchived ? "destructive" : "secondary"}
                      className="text-[10px]"
                    >
                      {getAgentDefinitionLifecycleLabel(definition.lifecycleState)}
                    </Badge>
                  </div>
                </CardHeader>
                <Separator className="bg-border/40" />
                <CardContent className="grid grid-cols-2 gap-2 py-2 text-[11px] text-muted-foreground">
                  <div>
                    <span className="font-medium text-foreground">Capability:</span>{" "}
                    {getAgentDefinitionBaseCapabilityLabel(definition.baseCapabilityProfile)}
                  </div>
                  <div>
                    <span className="font-medium text-foreground">Version:</span>{" "}
                    {definition.currentVersion}
                  </div>
                  <div className="col-span-2">
                    <span className="font-medium text-foreground">Updated:</span>{" "}
                    {new Date(definition.updatedAt).toLocaleString()}
                  </div>
                </CardContent>
                {(canMutate && !definition.isBuiltIn) || showHistory ? (
                  <CardFooter className="flex justify-end gap-1.5 pt-2">
                    {showHistory ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 gap-1 text-[11.5px]"
                        onClick={() => void onViewHistory(definition)}
                      >
                        <History className="h-3 w-3" />
                        Version history
                      </Button>
                    ) : null}
                    {canMutate && !definition.isBuiltIn && !isArchived ? (
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 gap-1 text-[11.5px]"
                        onClick={() => void onArchive(definition)}
                        disabled={isArchiving}
                      >
                        {isArchiving ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                          <Archive className="h-3 w-3" />
                        )}
                        Archive
                      </Button>
                    ) : null}
                    {definition.isBuiltIn ? (
                      <span className="flex items-center gap-1 text-[11px] text-muted-foreground">
                        <Bot className="h-3 w-3" /> Immutable built-in
                      </span>
                    ) : null}
                  </CardFooter>
                ) : null}
              </Card>
            )
          })}
        </div>
      )}
    </section>
  )
}
