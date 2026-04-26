import { useMemo, useState } from 'react'
import {
  AlertCircle,
  ChevronRight,
  FolderPlus,
  LoaderCircle,
  Plug,
  RefreshCcw,
  Search,
  ShieldAlert,
  Trash2,
} from 'lucide-react'
import type {
  AgentPaneView,
  OperatorActionErrorView,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
} from '@/src/features/cadence/use-cadence-desktop-state'
import {
  getPluginCommandAvailabilityLabel,
  getSkillSourceStateLabel,
  getSkillTrustStateLabel,
  type PluginCommandContributionDto,
  type PluginRegistryEntryDto,
  type RemovePluginRequestDto,
  type RemovePluginRootRequestDto,
  type SetPluginEnabledRequestDto,
  type SkillRegistryDto,
  type UpsertPluginRootRequestDto,
} from '@/src/lib/cadence-model'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { SectionHeader } from './section-header'

interface PluginsSectionProps {
  agent: AgentPaneView | null
  skillRegistry: SkillRegistryDto | null
  skillRegistryLoadStatus: SkillRegistryLoadStatus
  skillRegistryLoadError: OperatorActionErrorView | null
  skillRegistryMutationStatus: SkillRegistryMutationStatus
  pendingSkillSourceId: string | null
  skillRegistryMutationError: OperatorActionErrorView | null
  onRefreshSkillRegistry?: (options?: { force?: boolean }) => Promise<SkillRegistryDto>
  onReloadSkillRegistry?: (options?: { projectId?: string | null; includeUnavailable?: boolean }) => Promise<SkillRegistryDto>
  onUpsertPluginRoot?: (request: UpsertPluginRootRequestDto) => Promise<SkillRegistryDto>
  onRemovePluginRoot?: (request: RemovePluginRootRequestDto) => Promise<SkillRegistryDto>
  onSetPluginEnabled?: (request: SetPluginEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemovePlugin?: (request: RemovePluginRequestDto) => Promise<SkillRegistryDto>
}

type PluginRootForm = {
  rootId: string
  path: string
  enabled: boolean
}

type PluginRootErrors = Partial<Record<keyof PluginRootForm, string>>

type Tone = 'good' | 'info' | 'warn' | 'bad' | 'neutral'

function defaultPluginRootForm(): PluginRootForm {
  return {
    rootId: '',
    path: '',
    enabled: true,
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

function formatHash(value: string | null | undefined): string {
  if (!value) {
    return 'None'
  }
  return value.length > 12 ? value.slice(0, 12) : value
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path)
}

function validatePluginRootForm(form: PluginRootForm): PluginRootErrors {
  const errors: PluginRootErrors = {}
  const path = form.path.trim()
  const rootId = form.rootId.trim()

  if (!path) {
    errors.path = 'Path is required.'
  } else if (!isAbsolutePath(path)) {
    errors.path = 'Use an absolute directory path.'
  }

  if (rootId && !/^[a-z0-9-]+$/.test(rootId)) {
    errors.rootId = 'Root id must be lowercase kebab-case.'
  }

  return errors
}

function hasErrors(errors: PluginRootErrors): boolean {
  return Object.values(errors).some(Boolean)
}

function stateTone(state: PluginRegistryEntryDto['state']): Tone {
  switch (state) {
    case 'enabled':
      return 'good'
    case 'discoverable':
    case 'installed':
      return 'info'
    case 'disabled':
    case 'stale':
      return 'warn'
    case 'failed':
    case 'blocked':
      return 'bad'
  }
}

function trustTone(trust: PluginRegistryEntryDto['trust']): Tone {
  switch (trust) {
    case 'trusted':
    case 'user_approved':
      return 'good'
    case 'approval_required':
    case 'untrusted':
      return 'warn'
    case 'blocked':
      return 'bad'
  }
}

const TONE_CLASS: Record<Tone, string> = {
  good:
    'border-emerald-500/30 bg-emerald-500/[0.08] text-emerald-700 dark:border-emerald-400/40 dark:bg-emerald-400/[0.08] dark:text-emerald-200',
  info:
    'border-sky-500/30 bg-sky-500/[0.08] text-sky-700 dark:border-sky-400/40 dark:bg-sky-400/[0.08] dark:text-sky-200',
  warn:
    'border-amber-500/30 bg-amber-500/[0.08] text-amber-800 dark:border-amber-400/40 dark:bg-amber-400/[0.08] dark:text-amber-200',
  bad: 'border-destructive/40 bg-destructive/[0.08] text-destructive',
  neutral: 'border-border bg-secondary/60 text-foreground/70',
}

function Pill({ tone, children }: { tone: Tone; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        'inline-flex h-[18px] items-center rounded-full border px-1.5 text-[10.5px] font-medium',
        TONE_CLASS[tone],
      )}
    >
      {children}
    </span>
  )
}

function pluginPendingId(pluginId: string): string {
  return `plugin:${pluginId}`
}

function pluginMatchesQuery(plugin: PluginRegistryEntryDto, query: string): boolean {
  const haystack = [
    plugin.name,
    plugin.pluginId,
    plugin.description,
    plugin.version,
    plugin.rootId,
    plugin.rootPath,
    plugin.pluginRootPath,
    ...plugin.skills.map((skill) => `${skill.contributionId} ${skill.skillId} ${skill.path}`),
    ...plugin.commands.map((command) => `${command.contributionId} ${command.label} ${command.entry}`),
  ]
    .join(' ')
    .toLowerCase()

  return haystack.includes(query)
}

export function PluginsSection({
  agent: _agent,
  skillRegistry,
  skillRegistryLoadStatus,
  skillRegistryLoadError,
  skillRegistryMutationStatus,
  pendingSkillSourceId,
  skillRegistryMutationError,
  onRefreshSkillRegistry,
  onReloadSkillRegistry,
  onUpsertPluginRoot,
  onRemovePluginRoot,
  onSetPluginEnabled,
  onRemovePlugin,
}: PluginsSectionProps) {
  const projectId = _agent?.project.id ?? skillRegistry?.projectId ?? null
  const [query, setQuery] = useState('')
  const [rootForm, setRootForm] = useState<PluginRootForm>(() => defaultPluginRootForm())
  const [rootErrors, setRootErrors] = useState<PluginRootErrors>({})
  const loading = skillRegistryLoadStatus === 'loading'
  const mutating = skillRegistryMutationStatus === 'running'

  const filteredPlugins = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    const plugins = skillRegistry?.plugins ?? []
    if (!normalizedQuery) {
      return plugins
    }
    return plugins.filter((plugin) => pluginMatchesQuery(plugin, normalizedQuery))
  }, [query, skillRegistry?.plugins])

  const totalPlugins = skillRegistry?.plugins.length ?? 0
  const totalCommands = skillRegistry?.pluginCommands.length ?? 0
  const pluginRoots = skillRegistry?.sources.pluginRoots ?? []

  const handleReload = () => {
    if (onReloadSkillRegistry) {
      void onReloadSkillRegistry({ projectId, includeUnavailable: true }).catch(() => undefined)
      return
    }
    void onRefreshSkillRegistry?.({ force: true }).catch(() => undefined)
  }

  const handleAddRoot = async () => {
    const errors = validatePluginRootForm(rootForm)
    setRootErrors(errors)
    if (hasErrors(errors)) {
      return
    }

    try {
      await onUpsertPluginRoot?.({
        rootId: rootForm.rootId.trim() || null,
        path: rootForm.path.trim(),
        enabled: rootForm.enabled,
        projectId,
      })
      setRootForm(defaultPluginRootForm())
      setRootErrors({})
    } catch {
      // The shared mutation error surface renders the backend diagnostic.
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Plugins"
        description="Manage plugin sources that contribute skills and commands into the Cadence runtime."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={loading || (!onReloadSkillRegistry && !onRefreshSkillRegistry)}
            onClick={handleReload}
          >
            {loading ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RefreshCcw className="h-3.5 w-3.5" />}
            Reload
          </Button>
        }
      />

      {skillRegistryLoadError ? <ErrorBanner message={skillRegistryLoadError.message} /> : null}
      {skillRegistryMutationError ? <ErrorBanner message={skillRegistryMutationError.message} /> : null}

      {/* Plugin roots */}
      <div className="rounded-lg border border-border bg-card px-5 py-4">
        <div className="flex items-start gap-3.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <FolderPlus className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-foreground">Plugin roots</p>
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              Directories Cadence scans for plugin manifests.
              {pluginRoots.length > 0 ? ` ${pluginRoots.length} configured.` : ''}
            </p>
          </div>
        </div>

        <div className="mt-4 grid gap-2 sm:grid-cols-[0.7fr_1.4fr_auto_auto]">
          <div>
            <Label htmlFor="plugin-root-id" className="sr-only">
              Plugin root id
            </Label>
            <Input
              id="plugin-root-id"
              value={rootForm.rootId}
              onChange={(event) => setRootForm((current) => ({ ...current, rootId: event.target.value }))}
              className="h-9 font-mono text-[12px]"
              placeholder="root-id"
              aria-invalid={Boolean(rootErrors.rootId)}
            />
            {rootErrors.rootId ? <p className="mt-1 text-[11px] text-destructive">{rootErrors.rootId}</p> : null}
          </div>
          <div>
            <Label htmlFor="plugin-root-path" className="sr-only">
              Plugin root path
            </Label>
            <Input
              id="plugin-root-path"
              value={rootForm.path}
              onChange={(event) => setRootForm((current) => ({ ...current, path: event.target.value }))}
              className="h-9 font-mono text-[12px]"
              placeholder="/absolute/path/to/plugins"
              aria-invalid={Boolean(rootErrors.path)}
            />
            {rootErrors.path ? <p className="mt-1 text-[11px] text-destructive">{rootErrors.path}</p> : null}
          </div>
          <label className="flex h-9 items-center gap-2 px-1 text-[12px] text-muted-foreground">
            <Switch
              checked={rootForm.enabled}
              onCheckedChange={(enabled) => setRootForm((current) => ({ ...current, enabled }))}
              aria-label="Enable new plugin root"
            />
            Enabled
          </label>
          <Button
            type="button"
            size="sm"
            className="h-9 gap-1.5 text-[12px]"
            disabled={mutating || !onUpsertPluginRoot}
            onClick={() => void handleAddRoot()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            Add
          </Button>
        </div>

        {pluginRoots.length > 0 ? (
          <div className="mt-3.5 grid gap-0.5 border-t border-border pt-2.5">
            {pluginRoots.map((root) => (
              <div
                key={root.rootId}
                className="-mx-1.5 flex items-center gap-2 rounded-md px-2.5 py-2 transition-colors hover:bg-secondary/30"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] font-medium text-foreground">{root.rootId}</p>
                  <p className="mt-0.5 truncate font-mono text-[11.5px] text-muted-foreground">{root.path}</p>
                </div>
                {pendingSkillSourceId === root.rootId ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                ) : null}
                <span className="shrink-0 text-[11px] text-muted-foreground">{root.enabled ? 'On' : 'Off'}</span>
                <Switch
                  checked={root.enabled}
                  disabled={mutating || !onUpsertPluginRoot}
                  aria-label={`${root.enabled ? 'Disable' : 'Enable'} plugin root ${root.rootId}`}
                  onCheckedChange={(enabled) => {
                    void onUpsertPluginRoot?.({
                      rootId: root.rootId,
                      path: root.path,
                      enabled,
                      projectId,
                    }).catch(() => undefined)
                  }}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-muted-foreground hover:text-destructive"
                  disabled={mutating || !onRemovePluginRoot}
                  aria-label={`Remove plugin root ${root.rootId}`}
                  onClick={() => void onRemovePluginRoot?.({ rootId: root.rootId, projectId }).catch(() => undefined)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      {/* Plugins list */}
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">Plugins</h4>
          <span className="text-[11.5px] text-muted-foreground">
            {totalPlugins} {totalPlugins === 1 ? 'plugins' : 'plugins'} · {totalCommands} {totalCommands === 1 ? 'commands' : 'commands'}
          </span>
        </div>

        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="h-9 pl-8 text-[12.5px]"
            placeholder="Search plugins"
            aria-label="Search plugins"
          />
        </div>

        <div className="rounded-lg border border-border bg-card">
          {loading && !skillRegistry ? (
            <div className="flex items-center justify-center gap-2 px-4 py-12 text-[12px] text-muted-foreground">
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              Loading plugins
            </div>
          ) : filteredPlugins.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <div className="mx-auto flex h-9 w-9 items-center justify-center rounded-md border border-border/70 bg-secondary/40">
                <Plug className="h-4 w-4 text-muted-foreground" />
              </div>
              <p className="mt-3 text-[13px] font-medium text-foreground">No plugins found</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                {query ? 'Adjust the search query.' : 'Add a plugin root or reload configured roots.'}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border/60">
              {filteredPlugins.map((plugin) => (
                <PluginRow
                  key={plugin.pluginId}
                  plugin={plugin}
                  projectId={projectId}
                  disabled={mutating}
                  pending={pendingSkillSourceId === pluginPendingId(plugin.pluginId)}
                  onSetPluginEnabled={onSetPluginEnabled}
                  onRemovePlugin={onRemovePlugin}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Plugin commands */}
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">Plugin commands</h4>
          <span className="text-[11.5px] text-muted-foreground">{totalCommands} projected</span>
        </div>

        {skillRegistry?.pluginCommands.length ? (
          <div className="rounded-lg border border-border bg-card divide-y divide-border/60">
            {skillRegistry.pluginCommands.map((command) => (
              <PluginCommandRow key={command.commandId} command={command} />
            ))}
          </div>
        ) : (
          <div className="rounded-lg border border-dashed border-border/70 bg-card/40 px-5 py-10 text-center">
            <div className="mx-auto flex h-9 w-9 items-center justify-center rounded-md border border-border/70 bg-secondary/40">
              <Plug className="h-4 w-4 text-muted-foreground" />
            </div>
            <p className="mt-3 text-[13px] font-medium text-foreground">No plugin commands</p>
            <p className="mt-1 text-[12px] text-muted-foreground">
              Enabled plugins with command contributions appear here.
            </p>
          </div>
        )}
      </div>
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

interface PluginRowProps {
  plugin: PluginRegistryEntryDto
  projectId: string | null
  disabled: boolean
  pending: boolean
  onSetPluginEnabled?: (request: SetPluginEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemovePlugin?: (request: RemovePluginRequestDto) => Promise<SkillRegistryDto>
}

function PluginRow({
  plugin,
  projectId,
  disabled,
  pending,
  onSetPluginEnabled,
  onRemovePlugin,
}: PluginRowProps) {
  const canMutate = Boolean(projectId)
  const blocked = plugin.trust === 'blocked' || plugin.state === 'blocked'

  return (
    <div className="px-4 py-3.5">
      <div className="flex items-start gap-3">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
          <Plug className="h-4 w-4 text-foreground/70" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="truncate text-[13.5px] font-medium text-foreground">{plugin.name}</p>
            <Pill tone="neutral">{plugin.version}</Pill>
            <Pill tone={stateTone(plugin.state)}>{getSkillSourceStateLabel(plugin.state)}</Pill>
            <Pill tone={trustTone(plugin.trust)}>{getSkillTrustStateLabel(plugin.trust)}</Pill>
          </div>
          <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">
            {plugin.description || plugin.pluginId}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" /> : null}
          <Switch
            checked={plugin.enabled}
            disabled={disabled || !canMutate || !onSetPluginEnabled || blocked}
            aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} plugin ${plugin.name}`}
            onCheckedChange={(enabled) => {
              if (!projectId) {
                return
              }
              void onSetPluginEnabled?.({ projectId, pluginId: plugin.pluginId, enabled }).catch(() => undefined)
            }}
          />
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-muted-foreground hover:text-destructive"
                disabled={disabled || !canMutate || !onRemovePlugin}
                aria-label={`Remove plugin ${plugin.name}`}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Remove plugin</AlertDialogTitle>
                <AlertDialogDescription>
                  {plugin.name} will be marked unavailable for this project. Its contributed skills and commands will stop loading.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                  onClick={() => {
                    if (!projectId) {
                      return
                    }
                    void onRemovePlugin?.({ projectId, pluginId: plugin.pluginId }).catch(() => undefined)
                  }}
                >
                  Remove
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11.5px] text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Plugin</span>
          <span className="font-mono text-foreground/80">{plugin.pluginId}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Skills</span>
          <span className="text-foreground/80">{plugin.skillCount}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Commands</span>
          <span className="text-foreground/80">{plugin.commandCount}</span>
        </span>
      </div>

      {plugin.lastDiagnostic ? (
        <div className="mt-2.5 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-2.5 py-1.5 text-[11.5px] text-destructive">
          <ShieldAlert className="mt-px h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0">{plugin.lastDiagnostic.message}</span>
        </div>
      ) : null}

      <details className="mt-2 group">
        <summary className="inline-flex cursor-pointer select-none items-center gap-1 rounded-md text-[11.5px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
          <ChevronRight className="h-3 w-3 transition-transform group-open:rotate-90" />
          Plugin metadata
        </summary>
        <dl className="mt-2 grid gap-x-4 gap-y-1 rounded-md border border-border/70 bg-secondary/30 p-3 text-[11.5px] sm:grid-cols-[120px_1fr]">
          <div className="contents">
            <dt className="text-muted-foreground">Root id</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.rootId}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Root path</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.rootPath}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Plugin path</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.pluginRootPath}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Manifest</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.manifestPath}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Hash</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{formatHash(plugin.manifestHash)}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Reloaded</dt>
            <dd className="min-w-0 break-words text-foreground/85">{formatTimestamp(plugin.lastReloadedAt)}</dd>
          </div>
        </dl>
      </details>

      {plugin.skills.length || plugin.commands.length ? (
        <details className="mt-2 group">
          <summary className="inline-flex cursor-pointer select-none items-center gap-1 rounded-md text-[11.5px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
            <ChevronRight className="h-3 w-3 transition-transform group-open:rotate-90" />
            Contributions
          </summary>
          <div className="mt-2 grid gap-2 lg:grid-cols-2">
            <ContributionList
              title="Skills"
              emptyLabel="No skill contributions"
              rows={plugin.skills.map((skill) => ({
                id: skill.contributionId,
                label: skill.skillId,
                value: skill.path,
              }))}
            />
            <ContributionList
              title="Commands"
              emptyLabel="No command contributions"
              rows={plugin.commands.map((command) => ({
                id: command.contributionId,
                label: command.label,
                value: command.entry,
              }))}
            />
          </div>
        </details>
      ) : null}
    </div>
  )
}

interface ContributionListProps {
  title: string
  emptyLabel: string
  rows: Array<{ id: string; label: string; value: string }>
}

function ContributionList({ title, emptyLabel, rows }: ContributionListProps) {
  return (
    <div className="rounded-md border border-border/70 bg-secondary/30 p-2.5 text-[11.5px]">
      <p className="text-[10.5px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/70">{title}</p>
      {rows.length ? (
        <div className="mt-2 space-y-1.5">
          {rows.map((row) => (
            <div key={row.id} className="min-w-0">
              <p className="truncate font-medium text-foreground/90">{row.label}</p>
              <p className="break-words font-mono text-muted-foreground">{row.value}</p>
            </div>
          ))}
        </div>
      ) : (
        <p className="mt-2 text-muted-foreground">{emptyLabel}</p>
      )}
    </div>
  )
}

function PluginCommandRow({ command }: { command: PluginCommandContributionDto }) {
  return (
    <div className="px-4 py-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <p className="text-[13px] font-medium text-foreground">{command.label}</p>
        <Pill tone="neutral">{getPluginCommandAvailabilityLabel(command.availability)}</Pill>
        <Pill tone={stateTone(command.state)}>{getSkillSourceStateLabel(command.state)}</Pill>
        <Pill tone={trustTone(command.trust)}>{getSkillTrustStateLabel(command.trust)}</Pill>
      </div>
      <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">{command.description}</p>
      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11.5px] text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Command</span>
          <span className="font-mono text-foreground/80">{command.commandId}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Plugin</span>
          <span className="font-mono text-foreground/80">{command.pluginId}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Entry</span>
          <span className="font-mono text-foreground/80">{command.entry}</span>
        </span>
      </div>
    </div>
  )
}
