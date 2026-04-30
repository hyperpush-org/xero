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
} from '@/src/features/xero/use-xero-desktop-state'
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
} from '@/src/lib/xero-model'
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
  path: string
}

type PluginRootErrors = Partial<Record<keyof PluginRootForm, string>>

type Tone = 'good' | 'info' | 'warn' | 'bad' | 'neutral'

function defaultPluginRootForm(): PluginRootForm {
  return {
    path: '',
  }
}

function derivePluginRootLabel(rootId: string, path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  const lastSlash = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))
  const basename = lastSlash >= 0 ? trimmed.slice(lastSlash + 1) : trimmed
  return basename || rootId
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

function isAbsolutePath(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path)
}

function validatePluginRootForm(form: PluginRootForm): PluginRootErrors {
  const errors: PluginRootErrors = {}
  const path = form.path.trim()

  if (!path) {
    errors.path = 'Path is required.'
  } else if (!isAbsolutePath(path)) {
    errors.path = 'Use an absolute directory path.'
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
        rootId: null,
        path: rootForm.path.trim(),
        enabled: true,
        projectId,
      })
      setRootForm(defaultPluginRootForm())
      setRootErrors({})
    } catch {
      // The shared mutation error surface renders the backend diagnostic.
    }
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Plugins"
        description="Manage plugin sources that contribute skills and commands into the Xero runtime."
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
      <section className="flex flex-col gap-2.5">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Plugin roots
          <span className="ml-1.5 font-normal text-muted-foreground">{pluginRoots.length}</span>
        </h4>
        <p className="-mt-1 text-[12px] text-muted-foreground">
          Directories Xero scans for plugin manifests.
        </p>

        <div className="flex items-start gap-1.5">
          <div className="flex-1">
            <Label htmlFor="plugin-root-path" className="sr-only">
              Plugin root path
            </Label>
            <Input
              id="plugin-root-path"
              value={rootForm.path}
              onChange={(event) => setRootForm((current) => ({ ...current, path: event.target.value }))}
              className="h-8 font-mono text-[12px]"
              placeholder="/absolute/path/to/plugins"
              aria-invalid={Boolean(rootErrors.path)}
            />
            {rootErrors.path ? <p className="mt-1 text-[11px] text-destructive">{rootErrors.path}</p> : null}
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-[12px]"
            disabled={mutating || !onUpsertPluginRoot}
            onClick={() => void handleAddRoot()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            Add
          </Button>
        </div>

        {pluginRoots.length > 0 ? (
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
            {pluginRoots.map((root) => (
              <div
                key={root.rootId}
                className="flex items-center gap-2 px-3 py-2"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] font-medium text-foreground">
                    {derivePluginRootLabel(root.rootId, root.path)}
                  </p>
                  <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground" title={root.path}>
                    {root.path}
                  </p>
                </div>
                {pendingSkillSourceId === root.rootId ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                ) : null}
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
      </section>

      {/* Plugins list */}
      <section className="flex flex-col gap-2.5">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Plugins
          <span className="ml-1.5 font-normal text-muted-foreground">
            {totalPlugins} · {totalCommands} {totalCommands === 1 ? 'command' : 'commands'}
          </span>
        </h4>

        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="h-8 pl-8 text-[12.5px]"
            placeholder="Search plugins"
            aria-label="Search plugins"
          />
        </div>

        {loading && !skillRegistry ? (
          <div className="flex items-center justify-center gap-2 rounded-md border border-border/60 px-4 py-10 text-[12px] text-muted-foreground">
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            Loading plugins
          </div>
        ) : filteredPlugins.length === 0 ? (
          <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-8 text-center">
            <Plug className="mx-auto h-4 w-4 text-muted-foreground" />
            <p className="mt-2 text-[12.5px] font-medium text-foreground">No plugins found</p>
            <p className="mt-0.5 text-[11.5px] text-muted-foreground">
              {query ? 'Adjust the search query.' : 'Add a plugin root or reload configured roots.'}
            </p>
          </div>
        ) : (
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
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
      </section>

      {/* Plugin commands */}
      <section className="flex flex-col gap-2.5">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Plugin commands
          <span className="ml-1.5 font-normal text-muted-foreground">{totalCommands} projected</span>
        </h4>

        {skillRegistry?.pluginCommands.length ? (
          <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
            {skillRegistry.pluginCommands.map((command) => (
              <PluginCommandRow key={command.commandId} command={command} />
            ))}
          </div>
        ) : (
          <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-8 text-center">
            <Plug className="mx-auto h-4 w-4 text-muted-foreground" />
            <p className="mt-2 text-[12.5px] font-medium text-foreground">No plugin commands</p>
            <p className="mt-0.5 text-[11.5px] text-muted-foreground">
              Enabled plugins with command contributions appear here.
            </p>
          </div>
        )}
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
  const showTrustPill = plugin.trust !== 'trusted' && plugin.trust !== 'user_approved'

  return (
    <div className="px-3.5 py-3">
      <div className="flex items-start gap-3">
        <Plug className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="truncate text-[13px] font-medium text-foreground">{plugin.name}</p>
            <Pill tone="neutral">{plugin.version}</Pill>
            <Pill tone={stateTone(plugin.state)}>{getSkillSourceStateLabel(plugin.state)}</Pill>
            {showTrustPill ? (
              <Pill tone={trustTone(plugin.trust)}>{getSkillTrustStateLabel(plugin.trust)}</Pill>
            ) : null}
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

      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
        <span>
          <span className="text-muted-foreground/60">Skills </span>
          <span className="text-foreground/80">{plugin.skillCount}</span>
        </span>
        <span>
          <span className="text-muted-foreground/60">Commands </span>
          <span className="text-foreground/80">{plugin.commandCount}</span>
        </span>
        <span>
          <span className="text-muted-foreground/60">Reloaded </span>
          <span className="text-foreground/80">{formatTimestamp(plugin.lastReloadedAt)}</span>
        </span>
      </div>

      {plugin.lastDiagnostic ? (
        <div className="mt-2 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-2.5 py-1.5 text-[11.5px] text-destructive">
          <ShieldAlert className="mt-px h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0">{plugin.lastDiagnostic.message}</span>
        </div>
      ) : null}

      <details className="mt-1.5 group">
        <summary className="inline-flex cursor-pointer select-none items-center gap-1 rounded-md text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
          <ChevronRight className="h-3 w-3 transition-transform group-open:rotate-90" />
          Plugin metadata
        </summary>
        <dl className="mt-1.5 grid gap-x-4 gap-y-1 rounded-md border border-border/50 bg-secondary/20 p-2.5 text-[11px] sm:grid-cols-[110px_1fr]">
          <div className="contents">
            <dt className="text-muted-foreground">Manifest</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.manifestPath}</dd>
          </div>
          <div className="contents">
            <dt className="text-muted-foreground">Plugin path</dt>
            <dd className="min-w-0 break-words font-mono text-foreground/85">{plugin.pluginRootPath}</dd>
          </div>
        </dl>
      </details>

      {plugin.skills.length || plugin.commands.length ? (
        <details className="mt-1.5 group">
          <summary className="inline-flex cursor-pointer select-none items-center gap-1 rounded-md text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
            <ChevronRight className="h-3 w-3 transition-transform group-open:rotate-90" />
            Contributions
          </summary>
          <div className="mt-1.5 grid gap-1.5 lg:grid-cols-2">
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
    <div className="rounded-md border border-border/50 bg-secondary/20 p-2.5 text-[11px]">
      <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/70">{title}</p>
      {rows.length ? (
        <div className="mt-1.5 space-y-1.5">
          {rows.map((row) => (
            <div key={row.id} className="min-w-0">
              <p className="truncate font-medium text-foreground/90">{row.label}</p>
              <p className="break-words font-mono text-muted-foreground">{row.value}</p>
            </div>
          ))}
        </div>
      ) : (
        <p className="mt-1.5 text-muted-foreground">{emptyLabel}</p>
      )}
    </div>
  )
}

function PluginCommandRow({ command }: { command: PluginCommandContributionDto }) {
  const showTrustPill = command.trust !== 'trusted' && command.trust !== 'user_approved'
  return (
    <div className="px-3.5 py-2.5">
      <div className="flex flex-wrap items-center gap-1.5">
        <p className="text-[12.5px] font-medium text-foreground">{command.label}</p>
        <Pill tone="neutral">{getPluginCommandAvailabilityLabel(command.availability)}</Pill>
        <Pill tone={stateTone(command.state)}>{getSkillSourceStateLabel(command.state)}</Pill>
        {showTrustPill ? (
          <Pill tone={trustTone(command.trust)}>{getSkillTrustStateLabel(command.trust)}</Pill>
        ) : null}
      </div>
      <p className="mt-1 line-clamp-2 text-[11.5px] leading-[1.5] text-muted-foreground">{command.description}</p>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
        <span>
          <span className="text-muted-foreground/60">Command </span>
          <span className="font-mono text-foreground/80">{command.commandId}</span>
        </span>
        <span>
          <span className="text-muted-foreground/60">Entry </span>
          <span className="font-mono text-foreground/80">{command.entry}</span>
        </span>
      </div>
    </div>
  )
}
