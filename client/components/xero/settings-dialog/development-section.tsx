import {
  Apple,
  AppWindow,
  AlertTriangle,
  Cpu,
  Database,
  Eye,
  FlaskConical,
  Laptop,
  Loader2,
  PlayCircle,
  RefreshCw,
  Sparkles,
  Table2,
  Wand2,
} from "lucide-react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { useCallback, useEffect, useMemo, useState } from "react"
import type { PlatformVariant } from "@/components/xero/shell"
import { detectPlatform } from "@/components/xero/shell"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"
import {
  developerReadStorageTableRequestSchema,
  developerStorageOverviewSchema,
  developerStorageTableRowsSchema,
  type DeveloperStorageOverviewDto,
  type DeveloperStorageSourceDto,
  type DeveloperStorageTableRowsDto,
  type DeveloperStorageTableSummaryDto,
} from "@/src/lib/xero-model/developer-storage"
import { SectionHeader } from "./section-header"

interface PlatformOption {
  value: PlatformVariant | null
  label: string
  hint: string
  icon: React.ElementType
}

const PLATFORM_OPTIONS: PlatformOption[] = [
  { value: null, label: "Auto", hint: "Use detected OS", icon: Wand2 },
  { value: "macos", label: "macOS", hint: "Traffic lights · tabs right", icon: Apple },
  { value: "windows", label: "Windows", hint: "Tabs left · controls right", icon: AppWindow },
  { value: "linux", label: "Linux", hint: "Same as Windows, rounded", icon: Laptop },
]

export interface DevelopmentSectionProps {
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (value: PlatformVariant | null) => void
  onStartOnboarding?: () => void
}

export function DevelopmentSection({
  platformOverride,
  onPlatformOverrideChange,
  onStartOnboarding,
}: DevelopmentSectionProps) {
  const detected = detectPlatform()
  const current = platformOverride ?? null
  const currentOption = PLATFORM_OPTIONS.find((option) => option.value === current) ?? PLATFORM_OPTIONS[0]
  const overriding = current !== null && current !== detected
  const effectivePlatform = current ?? detected

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Development"
        description="Developer tooling and preview options. Not visible in production builds."
      />

      <PreviewCard
        currentOption={currentOption}
        detected={detected}
        effective={effectivePlatform}
        overriding={overriding}
      />

      <StorageInspector />

      <section className="flex flex-col gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Toolbar platform
        </h4>

        <div className="flex flex-col gap-2.5 rounded-lg border border-border/60 bg-card/30 p-3.5">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <span
                className="flex size-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground"
                aria-hidden
              >
                <Cpu className="h-3 w-3" />
              </span>
              <label className="text-[12px] font-medium text-foreground">Render toolbar as</label>
            </div>
            <span className="text-[11px] text-muted-foreground">
              Detected{" "}
              <span className="font-mono text-foreground/80">{detected}</span>
            </span>
          </div>

          <div className="flex gap-1 rounded-md border border-border/70 bg-secondary/30 p-1">
            {PLATFORM_OPTIONS.map((option) => {
              const active = current === option.value
              const Icon = option.icon
              return (
                <button
                  key={option.label}
                  type="button"
                  className={cn(
                    "flex flex-1 items-center justify-center gap-1.5 rounded-md py-1.5 text-[12.5px] font-medium transition-all motion-fast",
                    active
                      ? "bg-background text-foreground shadow-sm ring-1 ring-border/40"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  onClick={() => onPlatformOverrideChange?.(option.value)}
                  aria-pressed={active}
                >
                  <Icon className="h-3.5 w-3.5" />
                  {option.label}
                </button>
              )
            })}
          </div>

          <p className="text-[11.5px] leading-[1.5] text-muted-foreground">
            <span className="text-muted-foreground/70">Behavior:</span> {currentOption.hint}
          </p>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Tools
        </h4>
        <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
          <ToolRow
            icon={Sparkles}
            title="Onboarding flow"
            body="Reopen the first-run setup flow to test provider setup, project import, and notification routing."
            actionLabel="Start onboarding"
            actionIcon={PlayCircle}
            onAction={onStartOnboarding}
            disabled={!onStartOnboarding}
          />
        </ul>
      </section>
    </div>
  )
}

type StorageLoadState = "idle" | "loading" | "ready" | "error"

const STORAGE_ROW_LIMIT = 50
const GLOBAL_STORAGE_SOURCE_KEY = "global"

interface StorageSourceOption {
  key: string
  detail: string
  path: string
  badge: string
  source: DeveloperStorageSourceDto
  tables: DeveloperStorageTableSummaryDto[]
  exists: boolean
}

function StorageInspector() {
  const [overview, setOverview] = useState<DeveloperStorageOverviewDto | null>(null)
  const [rows, setRows] = useState<DeveloperStorageTableRowsDto | null>(null)
  const [overviewState, setOverviewState] = useState<StorageLoadState>("idle")
  const [rowsState, setRowsState] = useState<StorageLoadState>("idle")
  const [overviewError, setOverviewError] = useState<string | null>(null)
  const [rowsError, setRowsError] = useState<string | null>(null)
  const [selectedSourceKey, setSelectedSourceKey] = useState(GLOBAL_STORAGE_SOURCE_KEY)
  const [selectedTableName, setSelectedTableName] = useState("")
  const [offset, setOffset] = useState(0)
  const [revealSensitive, setRevealSensitive] = useState(false)

  const loadOverview = useCallback(async () => {
    if (!isTauri()) {
      setOverview(null)
      setOverviewState("error")
      setOverviewError("Storage inspection requires the Tauri desktop runtime.")
      return
    }

    setOverviewState("loading")
    setOverviewError(null)
    try {
      const response = await invoke<unknown>("developer_storage_overview")
      const parsed = developerStorageOverviewSchema.parse(response)
      setOverview(parsed)
      setOverviewState("ready")
    } catch (error) {
      setOverview(null)
      setRows(null)
      setOverviewState("error")
      setOverviewError(errorMessage(error, "Xero could not load the storage overview."))
    }
  }, [])

  useEffect(() => {
    void loadOverview()
  }, [loadOverview])

  const sourceOptions = useMemo(() => {
    if (!overview) return []
    return storageSourceOptions(overview)
  }, [overview])

  const selectedSource = sourceOptions.find((source) => source.key === selectedSourceKey) ?? sourceOptions[0] ?? null
  const selectedTables = selectedSource?.tables ?? []
  const selectedTable = selectedTables.find((table) => table.name === selectedTableName) ?? selectedTables[0] ?? null

  useEffect(() => {
    if (!overview || sourceOptions.length === 0) return
    if (!sourceOptions.some((source) => source.key === selectedSourceKey)) {
      setSelectedSourceKey(sourceOptions[0].key)
      setOffset(0)
      setRows(null)
    }
  }, [overview, selectedSourceKey, sourceOptions])

  useEffect(() => {
    if (!selectedSource) return
    const firstTable = selectedSource.tables[0]?.name ?? ""
    if (!selectedSource.tables.some((table) => table.name === selectedTableName)) {
      setSelectedTableName(firstTable)
      setOffset(0)
      setRows(null)
    }
  }, [selectedSource, selectedTableName])

  const loadRows = useCallback(async () => {
    if (!selectedSource || !selectedTable) {
      setRows(null)
      setRowsState("idle")
      return
    }

    if (!isTauri()) {
      setRows(null)
      setRowsState("error")
      setRowsError("Storage inspection requires the Tauri desktop runtime.")
      return
    }

    setRowsState("loading")
    setRowsError(null)
    try {
      const request = developerReadStorageTableRequestSchema.parse({
        source: selectedSource.source,
        tableName: selectedTable.name,
        limit: STORAGE_ROW_LIMIT,
        offset,
        revealSensitive,
      })
      const response = await invoke<unknown>("developer_storage_read_table", { request })
      const parsed = developerStorageTableRowsSchema.parse(response)
      setRows(parsed)
      setRowsState("ready")
    } catch (error) {
      setRows(null)
      setRowsState("error")
      setRowsError(errorMessage(error, "Xero could not read the selected storage table."))
    }
  }, [offset, revealSensitive, selectedSource, selectedTable])

  useEffect(() => {
    if (!selectedSource || !selectedTable) return
    void loadRows()
  }, [loadRows, selectedSource, selectedTable])

  const refreshing = overviewState === "loading"
  const loadingRows = rowsState === "loading"
  const totalSources = sourceOptions.length
  const totalTables = sourceOptions.reduce((count, source) => count + source.tables.length, 0)
  const canPageBack = Boolean(rows && rows.offset > 0)
  const canPageForward = Boolean(rows && rows.offset + rows.limit < rows.rowCount)

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Storage inspector
        </h4>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 gap-1.5 text-[12px]"
          disabled={refreshing}
          onClick={() => void loadOverview()}
          aria-label="Refresh storage overview"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "animate-spin")} />
          Refresh
        </Button>
      </div>

      <div className="overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <div className="flex items-start gap-3 border-b border-border/60 px-4 py-3">
          <div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
            <Database className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-[12.5px] font-medium text-foreground">Local storage</p>
              <Badge variant="outline" className="h-5 text-[10.5px]">
                {totalSources} source{totalSources === 1 ? "" : "s"}
              </Badge>
              <Badge variant="secondary" className="h-5 text-[10.5px]">
                {totalTables} table{totalTables === 1 ? "" : "s"}
              </Badge>
            </div>
            <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">
              App-local diagnostic tables.
            </p>
          </div>
        </div>

        {overviewState === "loading" && !overview ? (
          <StorageSkeleton />
        ) : overviewState === "error" ? (
          <StorageError message={overviewError ?? "Xero could not load local storage."} />
        ) : overview ? (
          <div className="flex flex-col gap-4 p-4">
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
              <label className="flex min-w-0 flex-col gap-1.5">
                <span className="text-[11.5px] font-medium text-muted-foreground">Source</span>
                <Select
                  value={selectedSource?.key ?? ""}
                  onValueChange={(value) => {
                    setSelectedSourceKey(value)
                    setOffset(0)
                    setRows(null)
                  }}
                >
                  <SelectTrigger aria-label="Storage source" className="h-9 w-full text-[12px]">
                    <SelectValue placeholder="Select source" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectLabel>Global</SelectLabel>
                      <SelectItem value={GLOBAL_STORAGE_SOURCE_KEY}>
                        Global SQLite
                      </SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </label>

              <label className="flex min-w-0 flex-col gap-1.5">
                <span className="text-[11.5px] font-medium text-muted-foreground">Table</span>
                <Select
                  value={selectedTable?.name ?? ""}
                  disabled={selectedTables.length === 0}
                  onValueChange={(value) => {
                    setSelectedTableName(value)
                    setOffset(0)
                    setRows(null)
                  }}
                >
                  <SelectTrigger aria-label="Storage table" className="h-9 w-full text-[12px]">
                    <SelectValue placeholder={selectedTables.length === 0 ? "No tables" : "Select table"} />
                  </SelectTrigger>
                  <SelectContent>
                    {selectedTables.map((table) => (
                      <SelectItem key={table.name} value={table.name}>
                        {table.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            </div>

            {selectedSource ? (
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md border border-border/50 bg-background/45 px-3 py-2 text-[11.5px] text-muted-foreground">
                <StorageMeta icon={Database} label="Kind" value={selectedSource.badge} />
                <StorageMeta icon={Table2} label="Rows" value={selectedTable ? formatCount(selectedTable.rowCount) : "0"} />
                <span className="min-w-0 flex flex-1 items-center gap-1.5">
                  <span className="text-muted-foreground/70">Path</span>
                  <span className="truncate font-mono text-[11px] text-foreground/80" title={selectedSource.path}>
                    {selectedSource.path}
                  </span>
                </span>
                {!selectedSource.exists ? (
                  <Badge variant="outline" className="border-warning/30 bg-warning/10 text-warning dark:text-warning">
                    Missing
                  </Badge>
                ) : null}
              </div>
            ) : null}

            <div className="flex flex-wrap items-center justify-between gap-3">
              <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <Switch
                  checked={revealSensitive}
                  onCheckedChange={(checked) => {
                    setRevealSensitive(Boolean(checked))
                    setOffset(0)
                  }}
                  aria-label="Reveal sensitive storage values"
                />
                Reveal sensitive values
              </label>

              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  className="h-8 gap-1.5 text-[12px]"
                  disabled={loadingRows || !selectedTable}
                  onClick={() => void loadRows()}
                >
                  {loadingRows ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <RefreshCw className="h-3.5 w-3.5" />
                  )}
                  Read table
                </Button>
              </div>
            </div>

            {rowsState === "error" ? (
              <StorageError message={rowsError ?? "Xero could not read the selected table."} compact />
            ) : selectedTables.length === 0 ? (
              <StorageEmptyState
                title="No tables found"
                body={selectedSource?.detail ?? "This storage source does not have inspectable tables yet."}
              />
            ) : loadingRows && !rows ? (
              <StorageRowsSkeleton />
            ) : rows ? (
              <StorageRowsTable
                rows={rows}
                onPrevious={() => setOffset(Math.max(0, rows.offset - rows.limit))}
                onNext={() => setOffset(rows.offset + rows.limit)}
                canPrevious={canPageBack && !loadingRows}
                canNext={canPageForward && !loadingRows}
                loading={loadingRows}
              />
            ) : null}
          </div>
        ) : null}
      </div>
    </section>
  )
}

function StorageRowsTable({
  rows,
  canPrevious,
  canNext,
  loading,
  onPrevious,
  onNext,
}: {
  rows: DeveloperStorageTableRowsDto
  canPrevious: boolean
  canNext: boolean
  loading: boolean
  onPrevious: () => void
  onNext: () => void
}) {
  const start = rows.rowCount === 0 ? 0 : rows.offset + 1
  const end = Math.min(rows.rowCount, rows.offset + rows.rows.length)

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Table2 className="h-3.5 w-3.5 text-muted-foreground" />
          <p className="truncate text-[12.5px] font-medium text-foreground">{rows.tableName}</p>
          <Badge variant="outline" className="h-5 text-[10.5px]">
            {rows.columns.length} column{rows.columns.length === 1 ? "" : "s"}
          </Badge>
          {rows.redacted ? (
            <Badge variant="secondary" className="h-5 text-[10.5px]">
              Redacted
            </Badge>
          ) : null}
        </div>
        <p className="text-[11.5px] text-muted-foreground">
          {formatCount(start)}-{formatCount(end)} of {formatCount(rows.rowCount)}
        </p>
      </div>

      <div className={cn("overflow-hidden rounded-md border border-border/60", loading && "opacity-70")}>
        <Table className="text-[12px]">
          <TableHeader>
            <TableRow className="bg-secondary/30 hover:bg-secondary/30">
              {rows.columns.map((column) => (
                <TableHead key={column.name} className="h-8 px-2 text-[11.5px]">
                  <span className="flex items-center gap-1.5">
                    <span>{column.name}</span>
                    {column.typeLabel ? (
                      <span className="font-mono text-[10px] font-normal text-muted-foreground/70">
                        {column.typeLabel}
                      </span>
                    ) : null}
                  </span>
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.rows.length === 0 ? (
              <TableRow>
                <TableCell colSpan={Math.max(1, rows.columns.length)} className="h-16 text-center text-[12px] text-muted-foreground">
                  This table has no rows.
                </TableCell>
              </TableRow>
            ) : (
              rows.rows.map((row, rowIndex) => (
                <TableRow key={`${rows.offset}-${rowIndex}`}>
                  {rows.columns.map((column) => {
                    const value = row.values[column.name]
                    const display = formatStorageValue(value)
                    return (
                      <TableCell
                        key={column.name}
                        className="max-w-[20rem] whitespace-pre-wrap break-words align-top font-mono text-[11.5px] leading-[1.45] text-foreground/85"
                        title={display}
                      >
                        {display}
                      </TableCell>
                    )
                  })}
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      <div className="flex items-center justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 text-[12px]"
          disabled={!canPrevious}
          onClick={onPrevious}
        >
          Previous
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 text-[12px]"
          disabled={!canNext}
          onClick={onNext}
        >
          Next
        </Button>
      </div>
    </div>
  )
}

function StorageMeta({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ElementType
  label: string
  value: string
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className="text-foreground/80">{value}</span>
    </span>
  )
}

function StorageError({ message, compact = false }: { message: string; compact?: boolean }) {
  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-3 border-destructive/35 bg-destructive/10",
        compact ? "rounded-md border px-3 py-2.5" : "border-b px-4 py-3",
      )}
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
      <p className="text-[12px] leading-[1.5] text-destructive/90">{message}</p>
    </div>
  )
}

function StorageEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-md border border-dashed border-border/70 bg-secondary/15 px-5 py-7 text-center">
      <div className="flex size-9 items-center justify-center rounded-full border border-border/60 bg-background/60 text-muted-foreground">
        <Table2 className="h-4 w-4" />
      </div>
      <p className="text-[12.5px] font-medium text-foreground">{title}</p>
      <p className="max-w-md text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
    </div>
  )
}

function StorageSkeleton() {
  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="grid gap-3 lg:grid-cols-2">
        <Skeleton className="h-9" />
        <Skeleton className="h-9" />
      </div>
      <Skeleton className="h-10" />
      <StorageRowsSkeleton />
    </div>
  )
}

function StorageRowsSkeleton() {
  return (
    <div className="flex flex-col gap-2">
      <Skeleton className="h-8 w-48" />
      <Skeleton className="h-32" />
    </div>
  )
}

function storageSourceOptions(overview: DeveloperStorageOverviewDto): StorageSourceOption[] {
  return [
    {
      key: GLOBAL_STORAGE_SOURCE_KEY,
      detail: "The app-global SQLite database does not have user tables yet.",
      path: overview.globalSqlite.path,
      badge: "SQLite",
      source: { kind: "global_sqlite", projectId: null },
      tables: overview.globalSqlite.tables,
      exists: true,
    },
  ]
}

function formatStorageValue(value: unknown): string {
  if (value === null || value === undefined) return "NULL"
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value)
  }
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function formatCount(value: number): string {
  return new Intl.NumberFormat().format(value)
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

function PreviewCard({
  currentOption,
  detected,
  effective,
  overriding,
}: {
  currentOption: PlatformOption
  detected: PlatformVariant
  effective: PlatformVariant
  overriding: boolean
}) {
  const tone = overriding ? "warn" : "muted"

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <div
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
            tone === "warn"
              ? "bg-warning/10 ring-warning/25"
              : "bg-muted/40 ring-border/60",
          )}
          aria-hidden
        >
          <FlaskConical
            className={cn(
              "h-5 w-5",
              tone === "warn"
                ? "text-warning dark:text-warning"
                : "text-muted-foreground",
            )}
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">
              Developer preview
            </p>
            {overriding ? (
              <PreviewPill tone="warn" label="Overriding" />
            ) : (
              <PreviewPill tone="muted" label="Auto" />
            )}
          </div>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">
            {overriding
              ? `Toolbar is rendering as ${formatPlatform(effective)} instead of the detected ${formatPlatform(detected)}. Switch back to Auto to use the real platform.`
              : "Xero is using the toolbar layout for the detected operating system. Override below to preview other platforms."}
          </p>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={Eye} label="Active" value={formatPlatform(effective)} />
        <MetaItem icon={Cpu} label="Detected" value={formatPlatform(detected)} mono />
        <MetaItem icon={currentOption.icon} label="Mode" value={currentOption.label} />
      </div>
    </div>
  )
}

function ToolRow({
  icon: Icon,
  title,
  body,
  actionLabel,
  actionIcon: ActionIcon,
  onAction,
  disabled,
}: {
  icon: React.ElementType
  title: string
  body: string
  actionLabel: string
  actionIcon: React.ElementType
  onAction?: () => void
  disabled?: boolean
}) {
  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
      <Button
        size="sm"
        variant="outline"
        className="h-8 shrink-0 gap-1.5 text-[12px]"
        disabled={disabled}
        onClick={onAction}
      >
        <ActionIcon className="h-3.5 w-3.5" />
        {actionLabel}
      </Button>
    </li>
  )
}

function PreviewPill({ tone, label }: { tone: "warn" | "muted"; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em] ring-1 ring-inset",
        tone === "warn"
          ? "bg-warning/10 text-warning ring-warning/25 dark:text-warning"
          : "bg-muted/40 text-muted-foreground ring-border/60",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          tone === "warn" ? "bg-warning dark:bg-warning" : "bg-muted-foreground/60",
        )}
        aria-hidden
      />
      {label}
    </span>
  )
}

function MetaItem({
  icon: Icon,
  label,
  value,
  mono = false,
}: {
  icon: React.ElementType
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className={cn("text-foreground/80", mono && "font-mono text-[11.5px]")}>{value}</span>
    </span>
  )
}

function formatPlatform(platform: PlatformVariant): string {
  switch (platform) {
    case "macos":
      return "macOS"
    case "windows":
      return "Windows"
    case "linux":
      return "Linux"
  }
}
