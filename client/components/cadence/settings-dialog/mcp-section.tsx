import { useMemo, useState } from 'react'
import { Check, LoaderCircle, Plus, RefreshCcw, Trash2 } from 'lucide-react'
import { z } from 'zod'
import type { McpRegistryLoadStatus, McpRegistryMutationStatus, OperatorActionErrorView } from '@/src/features/cadence/use-cadence-desktop-state'
import {
  getMcpConnectionStatusLabel,
  getMcpTransportKindLabel,
  type ImportMcpServersResponseDto,
  type McpImportDiagnosticDto,
  type McpRegistryDto,
  type McpServerDto,
  type UpsertMcpServerRequestDto,
} from '@/src/lib/cadence-model'
import { Badge } from '@/components/ui/badge'
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

function statusTone(status: McpServerDto['connection']['status']): string {
  switch (status) {
    case 'connected':
      return 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-100'
    case 'stale':
      return 'bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-100'
    case 'failed':
    case 'blocked':
    case 'misconfigured':
      return 'bg-destructive/15 text-destructive'
  }
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
    <div className="flex flex-col gap-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="text-[14px] font-semibold text-foreground">MCP Servers</h3>
          <p className="mt-1.5 text-[13px] text-muted-foreground">
            Manage app-local MCP server definitions and inspect typed connection diagnostics.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={!onRefreshMcpRegistry || mcpRegistryLoadStatus === 'loading'}
            onClick={() => void onRefreshMcpRegistry?.({ force: true })}
          >
            {mcpRegistryLoadStatus === 'loading' ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCcw className="h-3.5 w-3.5" />
            )}
            Reload registry
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={!onRefreshMcpServerStatuses || isMutating}
            onClick={() => void handleRefreshStatuses()}
          >
            <RefreshCcw className="h-3.5 w-3.5" />
            Refresh statuses
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
        </div>
      </div>

      {mcpRegistryLoadError ? (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-[12.5px] text-destructive">
          {mcpRegistryLoadError.message}
        </p>
      ) : null}

      {mcpRegistryMutationError ? (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-[12.5px] text-destructive">
          {mcpRegistryMutationError.message}
        </p>
      ) : null}

      <div className="rounded-lg border border-border bg-card px-5 py-4">
        <div className="grid gap-2">
          <Label htmlFor="mcp-import-path" className="text-[12px]">
            Import JSON file
          </Label>
          <div className="flex items-center gap-2">
            <Input
              id="mcp-import-path"
              className="h-9 font-mono text-[12px]"
              placeholder="/absolute/path/to/mcp-import.json"
              value={importPath}
              onChange={(event) => setImportPath(event.target.value)}
              disabled={!onImportMcpServers || isMutating}
            />
            <Button
              type="button"
              size="sm"
              className="h-9 text-[12px]"
              disabled={!onImportMcpServers || isMutating}
              onClick={() => void handleImport()}
            >
              Import
            </Button>
          </div>
          {importError ? <p className="text-[12px] text-destructive">{importError}</p> : null}
          {mcpImportDiagnostics.length > 0 ? (
            <div className="rounded-md border border-amber-200 bg-amber-50/60 px-3 py-2 text-[12px] text-amber-900 dark:border-amber-900/70 dark:bg-amber-900/20 dark:text-amber-100">
              <p className="font-medium">Import diagnostics</p>
              <ul className="mt-1.5 list-disc space-y-1 pl-4">
                {mcpImportDiagnostics.map((diagnostic) => (
                  <li key={`${diagnostic.index}:${diagnostic.code}`}>
                    {diagnostic.serverId ? `${diagnostic.serverId} — ` : ''}
                    {diagnostic.message} <span className="font-mono">({diagnostic.code})</span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      </div>

      {editingServerId !== null ? (
        <div className="rounded-lg border border-border bg-card px-5 py-4">
          <p className="text-[13px] font-medium text-foreground">
            {editingServerId === '' ? 'New MCP server' : `Edit MCP server — ${editingServerId}`}
          </p>
          <div className="mt-3.5 grid gap-3.5">
            <div className="grid grid-cols-2 gap-3.5">
              <div className="space-y-1.5">
                <Label htmlFor="mcp-form-id" className="text-[12px]">Server id</Label>
                <Input
                  id="mcp-form-id"
                  className="h-9 font-mono text-[13px]"
                  value={formValues.id}
                  disabled={isMutating || editingServerId !== ''}
                  onChange={(event) => setFormValues((current) => ({ ...current, id: event.target.value }))}
                  placeholder="memory"
                />
                {formErrors.id ? <p className="text-[12px] text-destructive">{formErrors.id}</p> : null}
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="mcp-form-name" className="text-[12px]">Display name</Label>
                <Input
                  id="mcp-form-name"
                  className="h-9 text-[13px]"
                  value={formValues.name}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, name: event.target.value }))}
                  placeholder="Memory Server"
                />
                {formErrors.name ? <p className="text-[12px] text-destructive">{formErrors.name}</p> : null}
              </div>
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="mcp-form-transport" className="text-[12px]">Transport</Label>
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
                <SelectTrigger id="mcp-form-transport" className="h-9 text-[13px]" size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="stdio">stdio</SelectItem>
                  <SelectItem value="http">HTTP</SelectItem>
                  <SelectItem value="sse">SSE</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {formValues.transportKind === 'stdio' ? (
              <>
                <div className="space-y-1.5">
                  <Label htmlFor="mcp-form-command" className="text-[12px]">Command</Label>
                  <Input
                    id="mcp-form-command"
                    className="h-9 font-mono text-[13px]"
                    value={formValues.command}
                    disabled={isMutating}
                    onChange={(event) => setFormValues((current) => ({ ...current, command: event.target.value }))}
                    placeholder="npx"
                  />
                  {formErrors.command ? <p className="text-[12px] text-destructive">{formErrors.command}</p> : null}
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="mcp-form-args" className="text-[12px]">Args (one per line)</Label>
                  <Textarea
                    id="mcp-form-args"
                    className="min-h-20 font-mono text-[12px]"
                    value={formValues.argsText}
                    disabled={isMutating}
                    onChange={(event) => setFormValues((current) => ({ ...current, argsText: event.target.value }))}
                    placeholder="@modelcontextprotocol/server-memory"
                  />
                </div>
              </>
            ) : (
              <div className="space-y-1.5">
                <Label htmlFor="mcp-form-url" className="text-[12px]">URL</Label>
                <Input
                  id="mcp-form-url"
                  className="h-9 font-mono text-[13px]"
                  value={formValues.url}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, url: event.target.value }))}
                  placeholder={formValues.transportKind === 'http' ? 'https://example.com/mcp' : 'https://example.com/sse'}
                />
                <p className="text-[11px] text-muted-foreground">
                  {formValues.transportKind === 'http'
                    ? 'Use an http(s) endpoint that serves MCP over HTTP transport.'
                    : 'Use an http(s) endpoint that serves MCP over SSE transport.'}
                </p>
                {formErrors.url ? <p className="text-[12px] text-destructive">{formErrors.url}</p> : null}
              </div>
            )}

            <div className="grid grid-cols-2 gap-3.5">
              <div className="space-y-1.5">
                <Label htmlFor="mcp-form-cwd" className="text-[12px]">Working directory (optional)</Label>
                <Input
                  id="mcp-form-cwd"
                  className="h-9 font-mono text-[13px]"
                  value={formValues.cwd}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, cwd: event.target.value }))}
                  placeholder="/Users/example/projects/cadence"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="mcp-form-env" className="text-[12px]">Env mappings (KEY=ENV_VAR)</Label>
                <Textarea
                  id="mcp-form-env"
                  className="min-h-20 font-mono text-[12px]"
                  value={formValues.envText}
                  disabled={isMutating}
                  onChange={(event) => setFormValues((current) => ({ ...current, envText: event.target.value }))}
                  placeholder={'OPENAI_API_KEY=OPENAI_API_KEY\nANTHROPIC_API_KEY=ANTHROPIC_API_KEY'}
                />
                {formErrors.envText ? <p className="text-[12px] text-destructive">{formErrors.envText}</p> : null}
              </div>
            </div>

            {formErrors.form ? <p className="text-[12.5px] text-destructive">{formErrors.form}</p> : null}

            <div className="flex items-center gap-2.5">
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
        </div>
      ) : null}

      <div className="grid gap-3">
        {registryServers.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border bg-card px-5 py-8 text-center">
            <p className="text-[13px] text-muted-foreground">
              No MCP servers configured yet. Add one manually or import from a JSON file.
            </p>
          </div>
        ) : (
          registryServers.map((server) => {
            const busy = isMutating && (pendingMcpServerId === null || pendingMcpServerId === server.id)
            return (
              <div key={server.id} className="rounded-lg border border-border bg-card px-5 py-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-[14px] font-medium text-foreground">{server.name}</p>
                    <p className="mt-1 font-mono text-[11.5px] text-muted-foreground">{server.id}</p>
                    <p className="mt-1.5 font-mono text-[11.5px] text-muted-foreground">
                      {getMcpTransportKindLabel(server.transport.kind)} · {transportSummary(server)}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge className={cn('text-[11px] font-medium', statusTone(server.connection.status))}>
                      {getMcpConnectionStatusLabel(server.connection.status)}
                    </Badge>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      className="h-7 gap-1.5 px-2.5 text-[11px]"
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
                      className="h-7 px-2.5 text-[11px]"
                      disabled={!onUpsertMcpServer || busy}
                      onClick={() => openEditForm(server)}
                    >
                      Edit
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-7 gap-1 px-2.5 text-[11px] text-destructive hover:text-destructive"
                      disabled={!onRemoveMcpServer || busy}
                      onClick={() => void handleRemove(server.id)}
                    >
                      <Trash2 className="h-3 w-3" />
                      Remove
                    </Button>
                  </div>
                </div>

                <div className="mt-3 grid gap-1 text-[12px] text-muted-foreground">
                  <p>Last checked: {formatTimestamp(server.connection.lastCheckedAt)}</p>
                  <p>Last healthy: {formatTimestamp(server.connection.lastHealthyAt)}</p>
                  <p>Env refs: {server.env.length}</p>
                  {server.connection.diagnostic ? (
                    <p className="text-[12px] text-destructive">
                      {server.connection.diagnostic.message}{' '}
                      <span className="font-mono">({server.connection.diagnostic.code})</span>
                    </p>
                  ) : null}
                </div>
              </div>
            )
          })
        )}
      </div>

      {activeServer && editingServerId === activeServer.id ? (
        <p className="text-[11.5px] text-muted-foreground">
          Editing {activeServer.name} ({activeServer.id}).
        </p>
      ) : null}
    </div>
  )
}
