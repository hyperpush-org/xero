import { AlertTriangle, ChevronRight, Loader2, Search } from "lucide-react"
import { useCallback, useEffect, useMemo, useState, type ElementType } from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"
import { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type { ProjectSummaryDto } from "@/src/lib/xero-model"
import type {
  DeveloperToolErrorLogEntryDto,
  DeveloperToolErrorLogListRequestDto,
  DeveloperToolErrorLogListResponseDto,
} from "@/src/lib/xero-model/developer-tool-error-log"

type LoadState = "idle" | "loading" | "ready" | "error"

interface Filters {
  query: string
  projectId: string
}

interface ProjectOption {
  id: string
  label: string
}

const EMPTY_FILTERS: Filters = {
  query: "",
  projectId: "",
}

const ALL_PROJECTS_VALUE = "__all_projects__"

export function ToolErrorLog() {
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS)
  const [response, setResponse] = useState<DeveloperToolErrorLogListResponseDto | null>(null)
  const [state, setState] = useState<LoadState>("idle")
  const [error, setError] = useState<string | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [projects, setProjects] = useState<ProjectSummaryDto[]>([])

  const load = useCallback(async () => {
    if (!XeroDesktopAdapter.isDesktopRuntime()) {
      setResponse(null)
      setState("error")
      setError("Developer tool-call error logging requires the Tauri desktop runtime.")
      return
    }

    const list = XeroDesktopAdapter.developerToolErrorLogList
    if (!list) {
      setResponse(null)
      setState("error")
      setError("Developer tool-call error logging is unavailable in this desktop build.")
      return
    }

    setState("loading")
    setError(null)
    try {
      const next = await list(requestFromFilters(filters))
      setResponse(next)
      setState("ready")
      setSelectedId((current) => {
        if (next.entries.some((entry) => entry.id === current)) {
          return current
        }
        return next.entries[0]?.id ?? null
      })
    } catch (err) {
      setResponse(null)
      setState("error")
      setError(errorMessage(err, "Xero could not load developer tool-call failures."))
      setSelectedId(null)
    }
  }, [filters])

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      void load()
    }, 180)
    return () => window.clearTimeout(timeout)
  }, [load])

  useEffect(() => {
    if (!XeroDesktopAdapter.isDesktopRuntime()) {
      setProjects([])
      return
    }

    let cancelled = false
    XeroDesktopAdapter.listProjects()
      .then((next) => {
        if (!cancelled) {
          setProjects(next.projects)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setProjects([])
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  const entries = response?.entries ?? []
  const selected = useMemo(
    () => entries.find((entry) => entry.id === selectedId) ?? entries[0] ?? null,
    [entries, selectedId],
  )
  const projectOptions = useMemo(
    () => mergeProjectOptions(projects, response?.projectIds ?? []),
    [projects, response?.projectIds],
  )

  const updateFilter = useCallback((key: keyof Filters, value: string) => {
    setFilters((current) => ({ ...current, [key]: value }))
  }, [])

  const isLoading = state === "loading"

  return (
    <section className="flex flex-col gap-3">
      <div className="flex min-w-0 items-center gap-2">
        <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Tool-call failures
        </h4>
        <Badge variant="outline" className="h-5 text-[10.5px]">
          {response ? response.totalCount : 0}
        </Badge>
      </div>

      <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_minmax(12rem,18rem)]">
        <FilterInput
          icon={Search}
          label="Fuzzy search"
          value={filters.query}
          onChange={(value) => updateFilter("query", value)}
        />
        <ProjectSelect
          projects={projectOptions}
          value={filters.projectId}
          onChange={(value) => updateFilter("projectId", value)}
        />
      </div>

      {state === "error" ? (
        <Alert variant="destructive">
          <AlertTriangle />
          <AlertTitle>Tool-call failures unavailable</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <div className="overflow-hidden rounded-lg border border-border/60 bg-card/30">
        {isLoading && !response ? (
          <div className="flex items-center gap-2 px-4 py-8 text-[12px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Loading tool-call failures...
          </div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8 text-center text-[12px] text-muted-foreground">
            No tool-call failures logged.
          </div>
        ) : (
          <Table className="text-[11.5px]">
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead className="h-8">Time</TableHead>
                <TableHead className="h-8">Tool</TableHead>
                <TableHead className="h-8">Error</TableHead>
                <TableHead className="h-8">Project / run</TableHead>
                <TableHead className="h-8">Retry</TableHead>
                <TableHead className="h-8">Message</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {entries.map((entry) => (
                <FailureRow
                  key={entry.id}
                  entry={entry}
                  selected={selected?.id === entry.id}
                  onSelect={() => setSelectedId(entry.id)}
                />
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      {response ? (
        <div className="flex min-w-0 items-center gap-2 text-[10.5px] text-muted-foreground">
          <span>Database</span>
          <code className="truncate rounded bg-muted/40 px-1.5 py-0.5 font-mono">
            {response.databasePath}
          </code>
        </div>
      ) : null}

      {selected ? <FailureDetails entry={selected} /> : null}
    </section>
  )
}

function FilterInput({
  icon: Icon,
  label,
  value,
  onChange,
}: {
  icon?: ElementType
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="relative">
      {Icon ? (
        <Icon className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
      ) : null}
      <Input
        aria-label={label}
        value={value}
        placeholder="Fuzzy search"
        className={cn("h-9 text-[12px]", Icon && "pl-8")}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  )
}

function ProjectSelect({
  projects,
  value,
  onChange,
}: {
  projects: ProjectOption[]
  value: string
  onChange: (value: string) => void
}) {
  return (
    <Select
      value={value || ALL_PROJECTS_VALUE}
      onValueChange={(next) => onChange(next === ALL_PROJECTS_VALUE ? "" : next)}
    >
      <SelectTrigger className="h-9 w-full text-[12px]" aria-label="Project">
        <SelectValue placeholder="All projects" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={ALL_PROJECTS_VALUE}>All projects</SelectItem>
        {projects.map((project) => (
          <SelectItem key={project.id} value={project.id}>
            {project.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function FailureRow({
  entry,
  selected,
  onSelect,
}: {
  entry: DeveloperToolErrorLogEntryDto
  selected: boolean
  onSelect: () => void
}) {
  return (
    <TableRow data-state={selected ? "selected" : undefined}>
      <TableCell className="max-w-[9rem] text-muted-foreground">
        {formatTimestamp(entry.occurredAt)}
      </TableCell>
      <TableCell>
        <button
          type="button"
          className="flex max-w-[10rem] items-center gap-1.5 truncate font-mono text-foreground hover:text-primary"
          onClick={onSelect}
          aria-label={`Inspect ${entry.toolName} failure`}
        >
          <ChevronRight
            className={cn("h-3 w-3 shrink-0", selected && "rotate-90 text-primary")}
          />
          <span className="truncate">{entry.toolName}</span>
        </button>
      </TableCell>
      <TableCell>
        <div className="flex max-w-[13rem] flex-wrap items-center gap-1">
          <Badge variant="outline" className="h-4 max-w-full truncate text-[10px]">
            {entry.errorCode}
          </Badge>
          {entry.errorCategory ? (
            <Badge variant="outline" className="h-4 max-w-full truncate text-[10px]">
              {entry.errorCategory}
            </Badge>
          ) : null}
        </div>
      </TableCell>
      <TableCell className="max-w-[12rem]">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate font-mono text-[10.5px] text-foreground">
            {entry.projectId ?? "unknown project"}
          </span>
          <span className="truncate font-mono text-[10.5px] text-muted-foreground">
            {entry.runId ?? "unknown run"}
          </span>
        </div>
      </TableCell>
      <TableCell>
        <Badge
          variant="outline"
          className={cn(
            "h-4 text-[10px]",
            entry.retryable
              ? "border-amber-400/45 bg-amber-500/10 text-amber-700 dark:text-amber-300"
              : "border-border/70 text-muted-foreground",
          )}
        >
          {entry.retryable ? "Retryable" : "No"}
        </Badge>
      </TableCell>
      <TableCell className="max-w-[18rem] truncate text-muted-foreground">
        {entry.messagePreview}
      </TableCell>
    </TableRow>
  )
}

function FailureDetails({ entry }: { entry: DeveloperToolErrorLogEntryDto }) {
  return (
    <div className="rounded-lg border border-border/60 bg-background/40">
      <div className="flex flex-wrap items-start justify-between gap-3 px-3.5 py-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="font-mono text-[12px] font-medium text-foreground">
              {entry.toolName}
            </span>
            <Badge variant="outline" className="h-5 text-[10.5px]">
              {entry.errorClass}
            </Badge>
            {entry.inputRedacted ? (
              <Badge variant="outline" className="h-5 text-[10.5px]">
                Redacted input
              </Badge>
            ) : null}
          </div>
          <p className="mt-1 text-[12px] leading-[1.5] text-muted-foreground">
            {entry.errorMessage}
          </p>
          {entry.modelMessage ? (
            <p className="mt-1 text-[11.5px] leading-[1.5] text-muted-foreground">
              {entry.modelMessage}
            </p>
          ) : null}
        </div>
        <code className="max-w-[18rem] truncate rounded bg-muted/40 px-1.5 py-0.5 font-mono text-[10.5px] text-muted-foreground">
          {entry.toolCallId}
        </code>
      </div>
      <Separator />
      <div className="grid gap-0 md:grid-cols-3 md:divide-x md:divide-border/50">
        <JsonPanel title="Input" value={entry.inputJson} />
        <JsonPanel title="Dispatch" value={entry.dispatchJson} />
        <JsonPanel title="Context" value={entry.contextJson} />
      </div>
    </div>
  )
}

function JsonPanel({ title, value }: { title: string; value: unknown }) {
  return (
    <div className="min-w-0 p-3">
      <h5 className="mb-2 text-[11.5px] font-semibold text-foreground">{title}</h5>
      <ScrollArea className="h-64 rounded-md border border-border/50 bg-muted/25">
        <pre className="p-2.5 font-mono text-[10.5px] leading-[1.5] text-foreground/85">
          {formatJson(value)}
        </pre>
      </ScrollArea>
    </div>
  )
}

function requestFromFilters(filters: Filters): DeveloperToolErrorLogListRequestDto {
  const request: DeveloperToolErrorLogListRequestDto = { limit: 100 }
  const query = optionalText(filters.query)
  const projectId = optionalText(filters.projectId)

  if (query) request.query = query
  if (projectId) request.projectId = projectId

  return request
}

function optionalText(value: string): string | undefined {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : undefined
}

function mergeProjectOptions(
  projects: ProjectSummaryDto[],
  loggedProjectIds: string[],
): ProjectOption[] {
  const seen = new Set<string>()
  const options: ProjectOption[] = []

  for (const project of projects) {
    const id = project.id.trim()
    if (!id || seen.has(id)) {
      continue
    }

    seen.add(id)
    options.push({
      id,
      label: project.name.trim() || id,
    })
  }

  for (const projectId of loggedProjectIds) {
    const id = projectId.trim()
    if (!id || seen.has(id)) {
      continue
    }

    seen.add(id)
    options.push({ id, label: id })
  }

  return options
}

function formatTimestamp(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  })
}

function formatJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof (error as { message?: unknown }).message === "string"
  ) {
    return (error as { message: string }).message
  }
  return fallback
}
