import { useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  CheckCircle2,
  ChevronRight,
  FolderPlus,
  Github,
  LoaderCircle,
  RefreshCcw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Trash2,
} from 'lucide-react'
import type {
  AgentPaneView,
  OperatorActionErrorView,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
} from '@/src/features/cadence/use-cadence-desktop-state'
import {
  getSkillSourceKindLabel,
  getSkillSourceStateLabel,
  getSkillTrustStateLabel,
  type RemoveSkillLocalRootRequestDto,
  type RemoveSkillRequestDto,
  type SetSkillEnabledRequestDto,
  type SkillRegistryDto,
  type SkillRegistryEntryDto,
  type SkillSourceKindDto,
  type UpdateGithubSkillSourceRequestDto,
  type UpdateProjectSkillSourceRequestDto,
  type UpsertSkillLocalRootRequestDto,
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { SectionHeader } from './section-header'

interface SkillsSectionProps {
  agent: AgentPaneView | null
  skillRegistry: SkillRegistryDto | null
  skillRegistryLoadStatus: SkillRegistryLoadStatus
  skillRegistryLoadError: OperatorActionErrorView | null
  skillRegistryMutationStatus: SkillRegistryMutationStatus
  pendingSkillSourceId: string | null
  skillRegistryMutationError: OperatorActionErrorView | null
  onRefreshSkillRegistry?: (options?: { force?: boolean }) => Promise<SkillRegistryDto>
  onReloadSkillRegistry?: (options?: { projectId?: string | null; includeUnavailable?: boolean }) => Promise<SkillRegistryDto>
  onSetSkillEnabled?: (request: SetSkillEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemoveSkill?: (request: RemoveSkillRequestDto) => Promise<SkillRegistryDto>
  onUpsertSkillLocalRoot?: (request: UpsertSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  onRemoveSkillLocalRoot?: (request: RemoveSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  onUpdateProjectSkillSource?: (request: UpdateProjectSkillSourceRequestDto) => Promise<SkillRegistryDto>
  onUpdateGithubSkillSource?: (request: UpdateGithubSkillSourceRequestDto) => Promise<SkillRegistryDto>
}

type SourceFilter = SkillSourceKindDto | 'all'

type LocalRootForm = {
  rootId: string
  path: string
  enabled: boolean
}

type LocalRootErrors = Partial<Record<keyof LocalRootForm | 'form', string>>

type GithubForm = {
  repo: string
  reference: string
  root: string
  enabled: boolean
}

const SOURCE_FILTERS: SourceFilter[] = [
  'all',
  'project',
  'local',
  'github',
  'bundled',
  'dynamic',
  'mcp',
  'plugin',
]

function defaultLocalRootForm(): LocalRootForm {
  return {
    rootId: '',
    path: '',
    enabled: true,
  }
}

function githubFormFromRegistry(registry: SkillRegistryDto | null): GithubForm {
  return {
    repo: registry?.sources.github.repo ?? 'vercel-labs/skills',
    reference: registry?.sources.github.reference ?? 'main',
    root: registry?.sources.github.root ?? 'skills',
    enabled: registry?.sources.github.enabled ?? true,
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

function sourceFilterLabel(filter: SourceFilter): string {
  return filter === 'all' ? 'All sources' : getSkillSourceKindLabel(filter)
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path)
}

function validateLocalRootForm(form: LocalRootForm): LocalRootErrors {
  const errors: LocalRootErrors = {}
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

function hasErrors(errors: LocalRootErrors): boolean {
  return Object.values(errors).some(Boolean)
}

type Tone = 'good' | 'info' | 'warn' | 'bad' | 'neutral'

function stateTone(entry: SkillRegistryEntryDto): Tone {
  switch (entry.sourceState) {
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

function trustTone(entry: SkillRegistryEntryDto): Tone {
  switch (entry.trustState) {
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

function metadataRows(entry: SkillRegistryEntryDto): Array<[string, string]> {
  const rows: Array<[string, string | null | undefined]> = [
    ['Source id', entry.sourceId],
    ['Source', entry.source.label],
    ['Version', entry.versionHash ? formatHash(entry.versionHash) : null],
    ['Last used', formatTimestamp(entry.lastUsedAt)],
    ['Repository', entry.source.repo],
    ['Reference', entry.source.reference],
    ['Path', entry.source.path],
    ['Root id', entry.source.rootId],
    ['Root path', entry.source.rootPath],
    ['Relative path', entry.source.relativePath],
    ['Bundle', entry.source.bundleId],
    ['Plugin', entry.source.pluginId],
    ['Server', entry.source.serverId],
  ]

  return rows
    .filter(([, value]) => value && value.trim().length > 0)
    .map(([label, value]) => [label, value ?? ''])
}

export function SkillsSection({
  agent,
  skillRegistry,
  skillRegistryLoadStatus,
  skillRegistryLoadError,
  skillRegistryMutationStatus,
  pendingSkillSourceId,
  skillRegistryMutationError,
  onRefreshSkillRegistry,
  onReloadSkillRegistry,
  onSetSkillEnabled,
  onRemoveSkill,
  onUpsertSkillLocalRoot,
  onRemoveSkillLocalRoot,
  onUpdateProjectSkillSource,
  onUpdateGithubSkillSource,
}: SkillsSectionProps) {
  const projectId = agent?.project.id ?? skillRegistry?.projectId ?? null
  const [query, setQuery] = useState('')
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all')
  const [localRootForm, setLocalRootForm] = useState<LocalRootForm>(() => defaultLocalRootForm())
  const [localRootErrors, setLocalRootErrors] = useState<LocalRootErrors>({})
  const [githubForm, setGithubForm] = useState<GithubForm>(() => githubFormFromRegistry(skillRegistry))
  const [githubDirty, setGithubDirty] = useState(false)

  const loading = skillRegistryLoadStatus === 'loading'
  const mutating = skillRegistryMutationStatus === 'running'

  useEffect(() => {
    if (!githubDirty) {
      setGithubForm(githubFormFromRegistry(skillRegistry))
    }
  }, [githubDirty, skillRegistry])

  const filteredEntries = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return (skillRegistry?.entries ?? []).filter((entry) => {
      if (sourceFilter !== 'all' && entry.sourceKind !== sourceFilter) {
        return false
      }

      if (!normalizedQuery) {
        return true
      }

      return [
        entry.name,
        entry.skillId,
        entry.description,
        entry.source.label,
        entry.sourceId,
      ].some((value) => value.toLowerCase().includes(normalizedQuery))
    })
  }, [query, skillRegistry?.entries, sourceFilter])

  const projectSourceEnabled = projectId
    ? skillRegistry?.sources.projects.find((project) => project.projectId === projectId)?.enabled ?? true
    : false

  const handleAddLocalRoot = async () => {
    const errors = validateLocalRootForm(localRootForm)
    setLocalRootErrors(errors)
    if (hasErrors(errors)) {
      return
    }

    try {
      await onUpsertSkillLocalRoot?.({
        rootId: localRootForm.rootId.trim() || null,
        path: localRootForm.path.trim(),
        enabled: localRootForm.enabled,
        projectId,
      })
      setLocalRootForm(defaultLocalRootForm())
      setLocalRootErrors({})
    } catch {
      // The mutation error surface is rendered from shared state.
    }
  }

  const handleSaveGithub = async () => {
    try {
      await onUpdateGithubSkillSource?.({
        repo: githubForm.repo.trim(),
        reference: githubForm.reference.trim(),
        root: githubForm.root.trim(),
        enabled: githubForm.enabled,
        projectId,
      })
      setGithubDirty(false)
    } catch {
      // The mutation error surface is rendered from shared state.
    }
  }

  const canMutateProjectSkills = Boolean(projectId && onSetSkillEnabled)
  const localRoots = skillRegistry?.sources.localRoots ?? []
  const totalSkills = skillRegistry?.entries.length ?? 0

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        title="Skills"
        description="Inspect installed and discoverable skills, then choose which sources Cadence can load."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={loading || (!onReloadSkillRegistry && !onRefreshSkillRegistry)}
            onClick={() => {
              if (onReloadSkillRegistry) {
                void onReloadSkillRegistry({ projectId, includeUnavailable: true }).catch(() => undefined)
                return
              }
              void onRefreshSkillRegistry?.({ force: true }).catch(() => undefined)
            }}
          >
            {loading ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RefreshCcw className="h-3.5 w-3.5" />}
            Reload
          </Button>
        }
      />

      {skillRegistryLoadError ? <ErrorBanner message={skillRegistryLoadError.message} /> : null}
      {skillRegistryMutationError ? <ErrorBanner message={skillRegistryMutationError.message} /> : null}

      {/* Sources */}
      <div className="grid gap-3 lg:grid-cols-2">
        <div className="rounded-lg border border-border bg-card px-5 py-4">
          <div className="flex items-center gap-3.5">
            <div
              className={cn(
                'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors',
                projectSourceEnabled ? 'border-primary/30 bg-primary/[0.08]' : 'border-border bg-secondary/60',
              )}
            >
              <Sparkles className={cn('h-4 w-4', projectSourceEnabled ? 'text-primary' : 'text-foreground/70')} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium text-foreground">Project source</p>
              <p className="mt-0.5 truncate text-[12px] text-muted-foreground">
                {agent?.repositoryPath ?? 'No project selected'}
              </p>
            </div>
            <Switch
              checked={projectSourceEnabled}
              disabled={!projectId || mutating || !onUpdateProjectSkillSource}
              aria-label="Enable project skill discovery"
              onCheckedChange={(enabled) => {
                if (!projectId) {
                  return
                }
                void onUpdateProjectSkillSource?.({ projectId, enabled }).catch(() => undefined)
              }}
            />
          </div>
        </div>

        <div className="rounded-lg border border-border bg-card px-5 py-4">
          <div className="flex items-center gap-3.5">
            <div
              className={cn(
                'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors',
                githubForm.enabled ? 'border-primary/30 bg-primary/[0.08]' : 'border-border bg-secondary/60',
              )}
            >
              <Github className={cn('h-4 w-4', githubForm.enabled ? 'text-primary' : 'text-foreground/70')} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium text-foreground">GitHub source</p>
              <p className="mt-0.5 truncate font-mono text-[12px] text-muted-foreground">
                {skillRegistry?.sources.github.repo ?? githubForm.repo}
              </p>
            </div>
            <Switch
              checked={githubForm.enabled}
              disabled={mutating}
              aria-label="Enable GitHub skill source"
              onCheckedChange={(enabled) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, enabled }))
              }}
            />
          </div>

          <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_0.6fr_0.6fr_auto]">
            <Input
              value={githubForm.repo}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, repo: event.target.value }))
              }}
              className="h-9 font-mono text-[12px]"
              aria-label="GitHub skill repository"
              placeholder="owner/repo"
            />
            <Input
              value={githubForm.reference}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, reference: event.target.value }))
              }}
              className="h-9 font-mono text-[12px]"
              aria-label="GitHub skill reference"
              placeholder="ref"
            />
            <Input
              value={githubForm.root}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, root: event.target.value }))
              }}
              className="h-9 font-mono text-[12px]"
              aria-label="GitHub skill root"
              placeholder="root"
            />
            <Button
              type="button"
              size="sm"
              className="h-9 text-[12px]"
              disabled={mutating || !githubDirty || !onUpdateGithubSkillSource}
              onClick={() => void handleSaveGithub()}
            >
              Save
            </Button>
          </div>
        </div>
      </div>

      {/* Local roots */}
      <div className="rounded-lg border border-border bg-card px-5 py-4">
        <div className="flex items-start gap-3.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <FolderPlus className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-foreground">Local roots</p>
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              Point Cadence at directories on disk that contain skill folders.
              {localRoots.length > 0 ? ` ${localRoots.length} configured.` : ''}
            </p>
          </div>
        </div>

        <div className="mt-4 grid gap-2 sm:grid-cols-[0.7fr_1.4fr_auto_auto]">
          <div>
            <Label htmlFor="skill-root-id" className="sr-only">
              Root id
            </Label>
            <Input
              id="skill-root-id"
              value={localRootForm.rootId}
              onChange={(event) => setLocalRootForm((current) => ({ ...current, rootId: event.target.value }))}
              className="h-9 font-mono text-[12px]"
              placeholder="root-id"
              aria-invalid={Boolean(localRootErrors.rootId)}
            />
            {localRootErrors.rootId ? <p className="mt-1 text-[11px] text-destructive">{localRootErrors.rootId}</p> : null}
          </div>
          <div>
            <Label htmlFor="skill-root-path" className="sr-only">
              Local root path
            </Label>
            <Input
              id="skill-root-path"
              value={localRootForm.path}
              onChange={(event) => setLocalRootForm((current) => ({ ...current, path: event.target.value }))}
              className="h-9 font-mono text-[12px]"
              placeholder="/absolute/path/to/skills"
              aria-invalid={Boolean(localRootErrors.path)}
            />
            {localRootErrors.path ? <p className="mt-1 text-[11px] text-destructive">{localRootErrors.path}</p> : null}
          </div>
          <label className="flex h-9 items-center gap-2 px-1 text-[12px] text-muted-foreground">
            <Switch
              checked={localRootForm.enabled}
              onCheckedChange={(enabled) => setLocalRootForm((current) => ({ ...current, enabled }))}
              aria-label="Enable new local skill root"
            />
            Enabled
          </label>
          <Button
            type="button"
            size="sm"
            className="h-9 gap-1.5 text-[12px]"
            disabled={mutating || !onUpsertSkillLocalRoot}
            onClick={() => void handleAddLocalRoot()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            Add
          </Button>
        </div>

        {localRoots.length > 0 ? (
          <div className="mt-3.5 grid gap-0.5 border-t border-border pt-2.5">
            {localRoots.map((root) => (
              <div
                key={root.rootId}
                className="-mx-1.5 flex items-center gap-2 rounded-md px-2.5 py-2 transition-colors hover:bg-secondary/30"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] font-medium text-foreground">{root.rootId}</p>
                  <p className="mt-0.5 truncate font-mono text-[11.5px] text-muted-foreground">{root.path}</p>
                </div>
                <span className="shrink-0 text-[11px] text-muted-foreground">
                  {root.enabled ? 'On' : 'Off'}
                </span>
                <Switch
                  checked={root.enabled}
                  disabled={mutating || !onUpsertSkillLocalRoot}
                  aria-label={`${root.enabled ? 'Disable' : 'Enable'} local skill root ${root.rootId}`}
                  onCheckedChange={(enabled) => {
                    void onUpsertSkillLocalRoot?.({
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
                  disabled={mutating || !onRemoveSkillLocalRoot}
                  aria-label={`Remove local skill root ${root.rootId}`}
                  onClick={() => void onRemoveSkillLocalRoot?.({ rootId: root.rootId, projectId }).catch(() => undefined)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      {/* Skills list */}
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <h4 className="text-[12.5px] font-semibold text-foreground">Discoverable skills</h4>
            <span className="text-[11.5px] text-muted-foreground">
              {filteredEntries.length} of {totalSkills}
            </span>
          </div>
        </div>

        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="h-9 pl-8 text-[12.5px]"
              placeholder="Search skills"
              aria-label="Search skills"
            />
          </div>
          <Select value={sourceFilter} onValueChange={(value) => setSourceFilter(value as SourceFilter)}>
            <SelectTrigger className="h-9 w-full text-[12.5px] sm:w-44" aria-label="Filter skills by source" size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SOURCE_FILTERS.map((filter) => (
                <SelectItem key={filter} value={filter}>
                  {sourceFilterLabel(filter)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="rounded-lg border border-border bg-card">
          {loading && !skillRegistry ? (
            <div className="flex items-center justify-center gap-2 px-4 py-12 text-[12px] text-muted-foreground">
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              Loading skills
            </div>
          ) : filteredEntries.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <div className="mx-auto flex h-9 w-9 items-center justify-center rounded-md border border-border/70 bg-secondary/40">
                <Sparkles className="h-4 w-4 text-muted-foreground" />
              </div>
              <p className="mt-3 text-[13px] font-medium text-foreground">No skills found</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                {query || sourceFilter !== 'all'
                  ? 'Adjust the search or source filter.'
                  : 'Add a local root or enable a project source.'}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border/60">
              {filteredEntries.map((entry) => (
                <SkillRow
                  key={entry.sourceId}
                  entry={entry}
                  projectId={projectId}
                  disabled={mutating}
                  pending={pendingSkillSourceId === entry.sourceId}
                  canSetEnabled={canMutateProjectSkills}
                  onSetSkillEnabled={onSetSkillEnabled}
                  onRemoveSkill={onRemoveSkill}
                />
              ))}
            </div>
          )}
        </div>

        {skillRegistry?.diagnostics.length ? (
          <div className="rounded-md border border-amber-500/30 bg-amber-500/[0.06] px-3 py-2 text-[12px] text-amber-900 dark:text-amber-200">
            {skillRegistry.diagnostics.map((diagnostic) => (
              <p key={`${diagnostic.code}:${diagnostic.relativePath ?? 'root'}`}>
                {diagnostic.relativePath ? <span className="font-mono">{diagnostic.relativePath}: </span> : ''}
                {diagnostic.message}
              </p>
            ))}
          </div>
        ) : null}
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

interface SkillRowProps {
  entry: SkillRegistryEntryDto
  projectId: string | null
  disabled: boolean
  pending: boolean
  canSetEnabled: boolean
  onSetSkillEnabled?: (request: SetSkillEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemoveSkill?: (request: RemoveSkillRequestDto) => Promise<SkillRegistryDto>
}

function SkillRow({
  entry,
  projectId,
  disabled,
  pending,
  canSetEnabled,
  onSetSkillEnabled,
  onRemoveSkill,
}: SkillRowProps) {
  const rows = metadataRows(entry)
  const removable = entry.installed && Boolean(projectId && onRemoveSkill)
  const trustedShield = entry.trustState === 'trusted' || entry.trustState === 'user_approved'

  return (
    <div className="px-4 py-3.5">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="truncate text-[13.5px] font-medium text-foreground">{entry.name}</p>
            <Pill tone="neutral">{getSkillSourceKindLabel(entry.sourceKind)}</Pill>
            <Pill tone={stateTone(entry)}>{getSkillSourceStateLabel(entry.sourceState)}</Pill>
            <Pill tone={trustTone(entry)}>{getSkillTrustStateLabel(entry.trustState)}</Pill>
          </div>
          <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">
            {entry.description || entry.skillId}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" /> : null}
          <Switch
            checked={entry.enabled}
            disabled={disabled || !canSetEnabled || entry.trustState === 'blocked'}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.name}`}
            onCheckedChange={(enabled) => {
              if (!projectId) {
                return
              }
              void onSetSkillEnabled?.({ projectId, sourceId: entry.sourceId, enabled }).catch(() => undefined)
            }}
          />
          {removable ? (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-muted-foreground hover:text-destructive"
                  disabled={disabled}
                  aria-label={`Remove ${entry.name}`}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Remove installed skill</AlertDialogTitle>
                  <AlertDialogDescription>
                    {entry.name} will be removed from this project. Discoverable source metadata will remain visible if the source is still enabled.
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
                      void onRemoveSkill?.({ projectId, sourceId: entry.sourceId }).catch(() => undefined)
                    }}
                  >
                    Remove
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          ) : null}
        </div>
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11.5px] text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Hash</span>
          <span className="font-mono text-foreground/80">{formatHash(entry.versionHash)}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Last used</span>
          <span className="text-foreground/80">{formatTimestamp(entry.lastUsedAt)}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="text-muted-foreground/60">Scope</span>
          <span className="text-foreground/80">{entry.scope === 'project' ? 'Project' : 'Global'}</span>
        </span>
      </div>

      {entry.lastDiagnostic ? (
        <div className="mt-2.5 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-2.5 py-1.5 text-[11.5px] text-destructive">
          <ShieldAlert className="mt-px h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0">{entry.lastDiagnostic.message}</span>
        </div>
      ) : (
        <div className="mt-2.5 inline-flex items-center gap-1.5 text-[11.5px] text-emerald-700 dark:text-emerald-300">
          {trustedShield ? <ShieldCheck className="h-3.5 w-3.5" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
          <span>No recorded diagnostic</span>
        </div>
      )}

      <details className="mt-2 group">
        <summary className="inline-flex cursor-pointer select-none items-center gap-1 rounded-md text-[11.5px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
          <ChevronRight className="h-3 w-3 transition-transform group-open:rotate-90" />
          Source metadata
        </summary>
        <dl className="mt-2 grid gap-x-4 gap-y-1 rounded-md border border-border/70 bg-secondary/30 p-3 text-[11.5px] sm:grid-cols-[120px_1fr]">
          {rows.map(([label, value]) => (
            <div key={label} className="contents">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="min-w-0 break-words font-mono text-foreground/85">{value}</dd>
            </div>
          ))}
        </dl>
      </details>
    </div>
  )
}
