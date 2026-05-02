import { useMemo, useState } from 'react'
import {
  AlertCircle,
  Check,
  LoaderCircle,
  Plus,
  RefreshCcw,
  Server,
  Trash2,
} from 'lucide-react'
import { z } from 'zod'
import type { McpRegistryLoadStatus, McpRegistryMutationStatus, OperatorActionErrorView } from '@/src/features/xero/use-xero-desktop-state'
import {
  getMcpConnectionStatusLabel,
  getMcpTransportKindLabel,
  type ImportMcpServersResponseDto,
  type McpImportDiagnosticDto,
  type McpRegistryDto,
  type McpServerDto,
  type UpsertMcpServerRequestDto,
} from '@/src/lib/xero-model'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import { SectionHeader } from './section-header'

interface McpSectionProps {
  mcpRegistry: McpRegistryDto | null
  mcpImportDiagnostics: McpImportDiagnosticDto[]
  mcpRegistryLoadStatus: McpRegistryLoadStatus
  mcpRegistryLoadError: OperatorActionErrorView | null
  mcpRegistryMutationStatus: McpRegistryMutationStatus
  pendingMcpServerId: string | null
  mcpRegistryMutationError: OperatorActionErrorView | null
  onRefreshMcpRegistry?: (options?: { force?: boolean }) => Promise<McpRegistryDto>
  onUpsertMcpServer?: (request: UpsertMcpServerRequestDto) => Promise<McpRegistryDto>
  onRemoveMcpServer?: (serverId: string) => Promise<McpRegistryDto>
  onImportMcpServers?: (path: string) => Promise<ImportMcpServersResponseDto>
  onRefreshMcpServerStatuses?: (options?: { serverIds?: string[] }) => Promise<McpRegistryDto>
}

type McpFormValues = {
  id: string
  name: string
  transportKind: 'stdio' | 'http' | 'sse'
  command: string
  argsText: string
  url: string
  cwd: string
  envText: string
}

type McpFormErrors = Partial<Record<'id' | 'name' | 'command' | 'url' | 'envText' | 'form', string>>

const mcpFormSchema = z
  .object({
    id: z.string().trim().min(1, 'Server id is required.'),
    name: z.string().trim().min(1, 'Server name is required.'),
    transportKind: z.enum(['stdio', 'http', 'sse']),
    command: z.string().trim(),
    argsText: z.string(),
    url: z.string().trim(),
    cwd: z.string(),
    envText: z.string(),
  })
  .superRefine((values, ctx) => {
    if (values.transportKind === 'stdio' && values.command.trim().length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['command'],
        message: 'stdio transport requires a command.',
      })
    }

    if ((values.transportKind === 'http' || values.transportKind === 'sse') && values.url.trim().length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['url'],
        message: `${values.transportKind.toUpperCase()} transport requires a URL.`,
      })
      return
    }

    if (values.transportKind === 'http' || values.transportKind === 'sse') {
      try {
        const parsed = new URL(values.url.trim())
        if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['url'],
            message: 'Transport URL must use http:// or https://.',
          })
        }
      } catch {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['url'],
          message: 'Transport URL must be a valid absolute URL.',
        })
      }
    }

    for (const [index, line] of values.envText.split('\n').entries()) {
      const trimmed = line.trim()
      if (!trimmed) {
        continue
      }
      const equalsIndex = trimmed.indexOf('=')
      const key = equalsIndex >= 0 ? trimmed.slice(0, equalsIndex).trim() : ''
      const fromEnv = equalsIndex >= 0 ? trimmed.slice(equalsIndex + 1).trim() : ''
      if (!key || !fromEnv) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['envText'],
          message: `Env mapping line ${index + 1} must use KEY=ENV_VAR format.`,
        })
        return
      }
    }
  })

function parseFormErrors(error: unknown): McpFormErrors {
  if (!(error instanceof z.ZodError)) {
    return {
      form: error instanceof Error ? error.message : 'Could not save MCP server.',
    }
  }

  const flattened = error.flatten().fieldErrors
  return {
    id: flattened.id?.[0],
    name: flattened.name?.[0],
    command: flattened.command?.[0],
    url: flattened.url?.[0],
    envText: flattened.envText?.[0],
    form: error.issues.find((issue) => issue.path.length === 0)?.message,
  }
}

function defaultMcpForm(): McpFormValues {
  return {
    id: '',
    name: '',
    transportKind: 'stdio',
    command: '',
    argsText: '',
    url: '',
    cwd: '',
    envText: '',
  }
}

function argsTextFromServer(server: McpServerDto): string {
  if (server.transport.kind !== 'stdio') {
    return ''
  }

  return server.transport.args.join('\n')
}

function envTextFromServer(server: McpServerDto): string {
  return server.env.map((entry) => `${entry.key}=${entry.fromEnv}`).join('\n')
}

function formFromServer(server: McpServerDto): McpFormValues {
  return {
    id: server.id,
    name: server.name,
    transportKind: server.transport.kind,
    command: server.transport.kind === 'stdio' ? server.transport.command : '',
    argsText: argsTextFromServer(server),
    url: server.transport.kind === 'stdio' ? '' : server.transport.url,
    cwd: server.cwd ?? '',
    envText: envTextFromServer(server),
  }
}

function toMcpRequest(values: McpFormValues): UpsertMcpServerRequestDto {
  const parsed = mcpFormSchema.parse(values)
  const env = parsed.envText
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const [key, fromEnv] = line.split('=', 2)
      return {
        key: key.trim(),
        fromEnv: fromEnv.trim(),
      }
    })

  const cwd = parsed.cwd.trim().length > 0 ? parsed.cwd.trim() : null

  if (parsed.transportKind === 'stdio') {
    return {
      id: parsed.id,
      name: parsed.name,
      transport: {
        kind: 'stdio',
        command: parsed.command.trim(),
        args: parsed.argsText
          .split('\n')
          .map((line) => line.trim())
          .filter((line) => line.length > 0),
      },
      env,
      cwd,
    }
  }

  return {
    id: parsed.id,
    name: parsed.name,
    transport: {
      kind: parsed.transportKind,
      url: parsed.url.trim(),
    },
    env,
    cwd,
  }
}

function formatTimestamp(value: string | null | undefined): string {
  if (!value) {
    return 'Never'
  }
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) {
    return value
  }
  return new Date(parsed).toLocaleString()
}

type StatusTone = 'good' | 'warn' | 'bad'

function statusTone(status: McpServerDto['connection']['status']): StatusTone {
  switch (status) {
    case 'connected':
      return 'good'
    case 'stale':
      return 'warn'
    default:
      return 'bad'
  }
}

const STATUS_DOT: Record<StatusTone, string> = {
  good: 'bg-success',
  warn: 'bg-warning',
  bad: 'bg-destructive',
}

const STATUS_TEXT: Record<StatusTone, string> = {
  good: 'text-success dark:text-success',
  warn: 'text-warning dark:text-warning',
  bad: 'text-destructive',
}

function transportSummary(server: McpServerDto): string {
  switch (server.transport.kind) {
    case 'stdio':
      return `${server.transport.command}${server.transport.args.length > 0 ? ` ${server.transport.args.join(' ')}` : ''}`
    case 'http':
    case 'sse':
      return server.transport.url
  }
}

export function McpSection({
  mcpRegistry,
  mcpImportDiagnostics,
  mcpRegistryLoadStatus,
  mcpRegistryLoadError,
  mcpRegistryMutationStatus,
  pendingMcpServerId,
  mcpRegistryMutationError,
  onRefreshMcpRegistry,
  onUpsertMcpServer,
  onRemoveMcpServer,
  onImportMcpServers,
  onRefreshMcpServerStatuses,
}: McpSectionProps) {
  const [editingServerId, setEditingServerId] = useState<string | null>(null)
  const [formValues, setFormValues] = useState<McpFormValues>(() => defaultMcpForm())
  const [formErrors, setFormErrors] = useState<McpFormErrors>({})
  const [importPath, setImportPath] = useState('')
  const [importError, setImportError] = useState<string | null>(null)

  const registryServers = mcpRegistry?.servers ?? []
  const canMutate =
    typeof onUpsertMcpServer === 'function' &&
    typeof onRemoveMcpServer === 'function' &&
    typeof onRefreshMcpServerStatuses === 'function' &&
    typeof onImportMcpServers === 'function'

  const isMutating = mcpRegistryMutationStatus === 'running'
  const isLoading = mcpRegistryLoadStatus === 'loading'
  const activeServer = useMemo(
    () => registryServers.find((server) => server.id === editingServerId) ?? null,
    [editingServerId, registryServers],
  )

  function openCreateForm() {
    setEditingServerId('')
    setFormValues(defaultMcpForm())
    setFormErrors({})
  }

  function openEditForm(server: McpServerDto) {
    setEditingServerId(server.id)
    setFormValues(formFromServer(server))
    setFormErrors({})
  }

  function closeForm() {
    setEditingServerId(null)
    setFormValues(defaultMcpForm())
    setFormErrors({})
  }

  async function handleSave() {
    if (!onUpsertMcpServer) {
      return
    }

    try {
      const request = toMcpRequest(formValues)
      setFormErrors({})
      await onUpsertMcpServer(request)
      closeForm()
    } catch (error) {
      setFormErrors(parseFormErrors(error))
    }
  }

  async function handleRemove(serverId: string) {
    if (!onRemoveMcpServer) {
      return
    }

    try {
      await onRemoveMcpServer(serverId)
      if (editingServerId === serverId) {
        closeForm()
      }
    } catch {
      // Typed error surfaces via state projection.
    }
  }

  async function handleRefreshStatuses(serverIds: string[] = []) {
    if (!onRefreshMcpServerStatuses) {
      return
    }

    try {
      await onRefreshMcpServerStatuses({ serverIds })
    } catch {
      // Typed error surfaces via state projection.
    }
  }

  async function handleImport() {
    if (!onImportMcpServers) {
      return
    }

    const trimmedPath = importPath.trim()
    if (!trimmedPath) {
      setImportError('Import path is required.')
      return
    }

    setImportError(null)
    try {
      await onImportMcpServers(trimmedPath)
      setImportPath('')
    } catch (error) {
      setImportError(error instanceof Error ? error.message : 'Could not import MCP servers.')
    }
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="MCP Servers"
        description="Manage app-local MCP server definitions and inspect connection diagnostics."
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              disabled={!onRefreshMcpRegistry || isLoading}
              onClick={() => void onRefreshMcpRegistry?.({ force: true })}
            >
              {isLoading ? (
                <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCcw className="h-3.5 w-3.5" />
              )}
              Reload
            </Button>
            <Button
              type="button"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              disabled={!canMutate || isMutating}
              onClick={openCreateForm}
            >
              <Plus className="h-3.5 w-3.5" />
              Add server
            </Button>
          </>
        }
      />

      {mcpRegistryLoadError ? <ErrorBanner message={mcpRegistryLoadError.message} /> : null}
      {mcpRegistryMutationError ? <ErrorBanner message={mcpRegistryMutationError.message} /> : null}

      {editingServerId !== null ? (
        <section className="rounded-md border border-border/70 bg-secondary/20 px-4 py-3.5 animate-in fade-in-0 slide-in-from-top-1 motion-enter">
          <div className="flex items-baseline justify-between gap-3">
            <h4 className="text-[12.5px] font-semibold text-foreground">
              {editingServerId === '' ? 'New MCP server' : `Edit ${activeServer?.name ?? editingServerId}`}
            </h4>
            {editingServerId !== '' ? (
              <span className="font-mono text-[11px] text-muted-foreground">{editingServerId}</span>
            ) : null}
          </div>

          <div className="mt-3 grid gap-3">
            <div className="grid grid-cols-2 gap-3">
              <FormField label="Server id" htmlFor="mcp-form-id" error={formErrors.id}>
                <Input
                  id="mcp-form-id"
                  className="h-8 font-mono text-[12.5px]"
                  value={formValues.id}
                  disabled={isMutating || editingServerId !== ''}
                  onChange={(event) => setFormValues((current) => ({ ...current, id: event.target.value }))}
                  placeholder="docs"
                />
              </FormField>
              <FormField label="Display name" htmlFor="mcp-form-name" error={formErrors.name}>
                <Input
                  id="mcp-form-name"
                  className="h-8 text-[12.5px]"
                  value={formValues.name}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, name: event.target.value }))}
                  placeholder="Docs Server"
                />
              </FormField>
            </div>

            <FormField label="Transport" htmlFor="mcp-form-transport">
              <Select
                value={formValues.transportKind}
                onValueChange={(value) =>
                  setFormValues((current) => ({
                    ...current,
                    transportKind: value as McpFormValues['transportKind'],
                  }))
                }
                disabled={isMutating}
              >
                <SelectTrigger id="mcp-form-transport" className="h-8 text-[12.5px]" size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="stdio">stdio</SelectItem>
                  <SelectItem value="http">HTTP</SelectItem>
                  <SelectItem value="sse">SSE</SelectItem>
                </SelectContent>
              </Select>
            </FormField>

            {formValues.transportKind === 'stdio' ? (
              <>
                <FormField label="Command" htmlFor="mcp-form-command" error={formErrors.command}>
                  <Input
                    id="mcp-form-command"
                    className="h-8 font-mono text-[12.5px]"
                    value={formValues.command}
                    disabled={isMutating}
                    onChange={(event) => setFormValues((current) => ({ ...current, command: event.target.value }))}
                    placeholder="npx"
                  />
                </FormField>
                <FormField label="Args (one per line)" htmlFor="mcp-form-args">
                  <Textarea
                    id="mcp-form-args"
                    className="min-h-16 font-mono text-[12px]"
                    value={formValues.argsText}
                    disabled={isMutating}
                    onChange={(event) => setFormValues((current) => ({ ...current, argsText: event.target.value }))}
                    placeholder="@modelcontextprotocol/server-filesystem"
                  />
                </FormField>
              </>
            ) : (
              <FormField
                label="URL"
                htmlFor="mcp-form-url"
                error={formErrors.url}
                hint={
                  formValues.transportKind === 'http'
                    ? 'Use an http(s) endpoint that serves MCP over HTTP transport.'
                    : 'Use an http(s) endpoint that serves MCP over SSE transport.'
                }
              >
                <Input
                  id="mcp-form-url"
                  className="h-8 font-mono text-[12.5px]"
                  value={formValues.url}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, url: event.target.value }))}
                  placeholder={formValues.transportKind === 'http' ? 'https://example.com/mcp' : 'https://example.com/sse'}
                />
              </FormField>
            )}

            <div className="grid grid-cols-2 gap-3">
              <FormField label="Working directory (optional)" htmlFor="mcp-form-cwd">
                <Input
                  id="mcp-form-cwd"
                  className="h-8 font-mono text-[12.5px]"
                  value={formValues.cwd}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, cwd: event.target.value }))}
                  placeholder="Absolute path to server project"
                />
              </FormField>
              <FormField label="Env mappings (KEY=ENV_VAR)" htmlFor="mcp-form-env" error={formErrors.envText}>
                <Textarea
                  id="mcp-form-env"
                  className="min-h-16 font-mono text-[12px]"
                  value={formValues.envText}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, envText: event.target.value }))}
                  placeholder={'OPENAI_API_KEY=OPENAI_API_KEY\nANTHROPIC_API_KEY=ANTHROPIC_API_KEY'}
                />
              </FormField>
            </div>

            {formErrors.form ? <p className="text-[12px] text-destructive">{formErrors.form}</p> : null}

            <div className="flex items-center gap-2">
              <Button
                type="button"
                size="sm"
                className="h-8 gap-1.5 text-[12px]"
                disabled={!onUpsertMcpServer || isMutating}
                onClick={() => void handleSave()}
              >
                {isMutating ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}
                {editingServerId === '' ? 'Create server' : 'Save changes'}
              </Button>
              <Button type="button" size="sm" variant="ghost" className="h-8 text-[12px]" onClick={closeForm}>
                Cancel
              </Button>
            </div>
          </div>
        </section>
      ) : null}

      <section className="flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">
            Servers
            <span className="ml-1.5 font-normal text-muted-foreground">{registryServers.length}</span>
          </h4>
          {registryServers.length > 0 ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 gap-1.5 px-2 text-[11.5px] text-muted-foreground hover:text-foreground"
              disabled={!onRefreshMcpServerStatuses || isMutating}
              onClick={() => void handleRefreshStatuses()}
            >
              <RefreshCcw className="h-3 w-3" />
              Refresh statuses
            </Button>
          ) : null}
        </div>

        {registryServers.length === 0 ? (
          <EmptyState
            icon={Server}
            title="No MCP servers configured"
            hint="Add a server manually or import a JSON file below."
          />
        ) : (
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
            {registryServers.map((server) => {
              const busy = isMutating && (pendingMcpServerId === null || pendingMcpServerId === server.id)
              const tone = statusTone(server.connection.status)
              return (
                <div key={server.id} className="px-3.5 py-3">
                  <div className="flex items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="text-[13px] font-medium text-foreground">{server.name}</p>
                        <span className={cn('inline-flex items-center gap-1 text-[11px]', STATUS_TEXT[tone])}>
                          <span className={cn('h-1.5 w-1.5 rounded-full', STATUS_DOT[tone])} />
                          {getMcpConnectionStatusLabel(server.connection.status)}
                        </span>
                        <span className="font-mono text-[11px] text-muted-foreground">{server.id}</span>
                      </div>
                      <p className="mt-1 text-[12px] text-muted-foreground">
                        <span className="text-muted-foreground/70">{getMcpTransportKindLabel(server.transport.kind)}</span>{' '}
                        <span className="font-mono text-foreground/80">{transportSummary(server)}</span>
                      </p>
                    </div>
                    <div className="flex shrink-0 items-center">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-7 gap-1 px-2 text-[11.5px] text-muted-foreground hover:text-foreground"
                        disabled={!onRefreshMcpServerStatuses || busy}
                        onClick={() => void handleRefreshStatuses([server.id])}
                      >
                        <RefreshCcw className="h-3 w-3" />
                        Refresh
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-7 px-2 text-[11.5px] text-muted-foreground hover:text-foreground"
                        disabled={!onUpsertMcpServer || busy}
                        onClick={() => openEditForm(server)}
                      >
                        Edit
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-muted-foreground hover:text-destructive"
                        disabled={!onRemoveMcpServer || busy}
                        onClick={() => void handleRemove(server.id)}
                        aria-label={`Remove ${server.name}`}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>

                  {server.connection.diagnostic ? (
                    <div className="mt-2.5 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-2.5 py-1.5 text-[11.5px] text-destructive">
                      <AlertCircle className="mt-px h-3.5 w-3.5 shrink-0" />
                      <span className="min-w-0">
                        {server.connection.diagnostic.message}{' '}
                        <span className="font-mono text-[11px] opacity-80">({server.connection.diagnostic.code})</span>
                      </span>
                    </div>
                  ) : null}

                  <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
                    <span>
                      <span className="text-muted-foreground/60">Last checked </span>
                      <span className="text-foreground/80">{formatTimestamp(server.connection.lastCheckedAt)}</span>
                    </span>
                    <span>
                      <span className="text-muted-foreground/60">Last healthy </span>
                      <span className="text-foreground/80">{formatTimestamp(server.connection.lastHealthyAt)}</span>
                    </span>
                    <span>
                      <span className="text-muted-foreground/60">Env refs </span>
                      <span className="text-foreground/80">{server.env.length}</span>
                    </span>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </section>

      <section className="flex flex-col gap-2.5 border-t border-border/50 pt-5">
        <div>
          <h4 className="text-[12.5px] font-semibold text-foreground">Import from JSON</h4>
          <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">
            Paste an absolute path to a JSON file that defines one or more MCP servers.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Label htmlFor="mcp-import-path" className="sr-only">
            Import JSON file
          </Label>
          <Input
            id="mcp-import-path"
            className="h-8 font-mono text-[12px]"
            placeholder="/absolute/path/to/mcp-import.json"
            value={importPath}
            onChange={(event) => setImportPath(event.target.value)}
            disabled={!onImportMcpServers || isMutating}
          />
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 text-[12px]"
            disabled={!onImportMcpServers || isMutating}
            onClick={() => void handleImport()}
          >
            Import
          </Button>
        </div>
        {importError ? <p className="text-[12px] text-destructive">{importError}</p> : null}

        {mcpImportDiagnostics.length > 0 ? (
          <div className="rounded-md border border-warning/30 bg-warning/[0.06] px-3 py-2 text-[12px] text-warning dark:text-warning">
            <p className="font-medium">Import diagnostics</p>
            <ul className="mt-1.5 space-y-1 pl-1">
              {mcpImportDiagnostics.map((diagnostic) => (
                <li key={`${diagnostic.index}:${diagnostic.code}`} className="flex gap-1.5">
                  <span className="text-warning/70">·</span>
                  <span className="min-w-0">
                    {diagnostic.serverId ? <span className="font-mono">{diagnostic.serverId}</span> : null}
                    {diagnostic.serverId ? ' — ' : ''}
                    {diagnostic.message}{' '}
                    <span className="font-mono text-[11px] opacity-70">({diagnostic.code})</span>
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>
    </div>
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

function FormField({
  label,
  htmlFor,
  error,
  hint,
  children,
}: {
  label: string
  htmlFor: string
  error?: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={htmlFor} className="text-[11.5px]">
        {label}
      </Label>
      {children}
      {hint ? <p className="text-[11px] text-muted-foreground">{hint}</p> : null}
      {error ? <p className="text-[12px] text-destructive">{error}</p> : null}
    </div>
  )
}

function EmptyState({
  icon: Icon,
  title,
  hint,
}: {
  icon: React.ElementType
  title: string
  hint: string
}) {
  return (
    <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-8 text-center">
      <Icon className="mx-auto h-4 w-4 text-muted-foreground" />
      <p className="mt-2 text-[12.5px] font-medium text-foreground">{title}</p>
      <p className="mt-0.5 text-[11.5px] text-muted-foreground">{hint}</p>
    </div>
  )
}
