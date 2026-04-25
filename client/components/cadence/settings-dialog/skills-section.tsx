import { useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  CheckCircle2,
  FolderPlus,
  LoaderCircle,
  RefreshCcw,
  Search,
  ShieldAlert,
  ShieldCheck,
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
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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

function stateTone(entry: SkillRegistryEntryDto): string {
  switch (entry.sourceState) {
    case 'enabled':
      return 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-100'
    case 'discoverable':
    case 'installed':
      return 'bg-sky-100 text-sky-800 dark:bg-sky-900/40 dark:text-sky-100'
    case 'disabled':
    case 'stale':
      return 'bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-100'
    case 'failed':
    case 'blocked':
      return 'bg-destructive/15 text-destructive'
  }
}

function trustTone(entry: SkillRegistryEntryDto): string {
  switch (entry.trustState) {
    case 'trusted':
    case 'user_approved':
      return 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-100'
    case 'approval_required':
    case 'untrusted':
      return 'bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-100'
    case 'blocked':
      return 'bg-destructive/15 text-destructive'
  }
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

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader
        title="Skills"
        description="Inspect installed and discoverable skills, then choose which sources Cadence can load."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
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

      {skillRegistryLoadError ? (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Skill registry unavailable</AlertTitle>
          <AlertDescription>{skillRegistryLoadError.message}</AlertDescription>
        </Alert>
      ) : null}

      {skillRegistryMutationError ? (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Skill update failed</AlertTitle>
          <AlertDescription>{skillRegistryMutationError.message}</AlertDescription>
        </Alert>
      ) : null}

      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="h-8 pl-8 text-[12px]"
              placeholder="Search skills"
              aria-label="Search skills"
            />
          </div>
          <Select value={sourceFilter} onValueChange={(value) => setSourceFilter(value as SourceFilter)}>
            <SelectTrigger className="h-8 w-full text-[12px] sm:w-40" aria-label="Filter skills by source">
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

        <div className="rounded-md border border-border/70">
          {loading && !skillRegistry ? (
            <div className="flex items-center justify-center gap-2 px-4 py-12 text-[12px] text-muted-foreground">
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              Loading skills
            </div>
          ) : filteredEntries.length === 0 ? (
            <div className="px-4 py-12 text-center">
              <p className="text-[13px] font-medium text-foreground">No skills found</p>
              <p className="mt-1 text-[12px] text-muted-foreground">
                {query || sourceFilter !== 'all' ? 'Adjust the search or source filter.' : 'Add a local root or enable a project source.'}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border/70">
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
          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-900 dark:text-amber-100">
            {skillRegistry.diagnostics.map((diagnostic) => (
              <p key={`${diagnostic.code}:${diagnostic.relativePath ?? 'root'}`}>
                {diagnostic.relativePath ? `${diagnostic.relativePath}: ` : ''}
                {diagnostic.message}
              </p>
            ))}
          </div>
        ) : null}
      </section>

      <section className="grid gap-3 lg:grid-cols-[1fr_1fr]">
        <div className="rounded-md border border-border/70 p-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h4 className="text-[12px] font-semibold text-foreground">Project source</h4>
              <p className="mt-0.5 text-[11.5px] text-muted-foreground">{agent?.repositoryPath ?? 'No project selected'}</p>
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

        <div className="rounded-md border border-border/70 p-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h4 className="text-[12px] font-semibold text-foreground">GitHub source</h4>
              <p className="mt-0.5 text-[11.5px] text-muted-foreground">
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
          <div className="mt-3 grid gap-2 sm:grid-cols-[1fr_0.65fr_0.65fr_auto]">
            <Input
              value={githubForm.repo}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, repo: event.target.value }))
              }}
              className="h-8 text-[12px]"
              aria-label="GitHub skill repository"
              placeholder="owner/repo"
            />
            <Input
              value={githubForm.reference}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, reference: event.target.value }))
              }}
              className="h-8 text-[12px]"
              aria-label="GitHub skill reference"
              placeholder="ref"
            />
            <Input
              value={githubForm.root}
              onChange={(event) => {
                setGithubDirty(true)
                setGithubForm((current) => ({ ...current, root: event.target.value }))
              }}
              className="h-8 text-[12px]"
              aria-label="GitHub skill root"
              placeholder="root"
            />
            <Button
              type="button"
              size="sm"
              disabled={mutating || !githubDirty || !onUpdateGithubSkillSource}
              onClick={() => void handleSaveGithub()}
            >
              Save
            </Button>
          </div>
        </div>
      </section>

      <section className="rounded-md border border-border/70 p-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h4 className="text-[12px] font-semibold text-foreground">Local roots</h4>
            <p className="mt-0.5 text-[11.5px] text-muted-foreground">
              {skillRegistry?.sources.localRoots.length ?? 0} configured
            </p>
          </div>
        </div>

        <div className="mt-3 grid gap-2 sm:grid-cols-[0.8fr_1.4fr_auto_auto]">
          <div>
            <Label htmlFor="skill-root-id" className="sr-only">Root id</Label>
            <Input
              id="skill-root-id"
              value={localRootForm.rootId}
              onChange={(event) => setLocalRootForm((current) => ({ ...current, rootId: event.target.value }))}
              className="h-8 text-[12px]"
              placeholder="root id"
              aria-invalid={Boolean(localRootErrors.rootId)}
            />
            {localRootErrors.rootId ? <p className="mt-1 text-[11px] text-destructive">{localRootErrors.rootId}</p> : null}
          </div>
          <div>
            <Label htmlFor="skill-root-path" className="sr-only">Local root path</Label>
            <Input
              id="skill-root-path"
              value={localRootForm.path}
              onChange={(event) => setLocalRootForm((current) => ({ ...current, path: event.target.value }))}
              className="h-8 text-[12px]"
              placeholder="/absolute/path/to/skills"
              aria-invalid={Boolean(localRootErrors.path)}
            />
            {localRootErrors.path ? <p className="mt-1 text-[11px] text-destructive">{localRootErrors.path}</p> : null}
          </div>
          <label className="flex h-8 items-center gap-2 text-[12px] text-muted-foreground">
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
            disabled={mutating || !onUpsertSkillLocalRoot}
            onClick={() => void handleAddLocalRoot()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            Add
          </Button>
        </div>

        {skillRegistry?.sources.localRoots.length ? (
          <div className="mt-3 divide-y divide-border/70 rounded-md border border-border/70">
            {skillRegistry.sources.localRoots.map((root) => (
              <div key={root.rootId} className="flex items-center justify-between gap-3 px-3 py-2">
                <div className="min-w-0">
                  <p className="truncate text-[12px] font-medium text-foreground">{root.rootId}</p>
                  <p className="truncate text-[11.5px] text-muted-foreground">{root.path}</p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className="text-[11.5px] text-muted-foreground">
                    {root.enabled ? 'Enabled' : 'Disabled'}
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
              </div>
            ))}
          </div>
        ) : null}
      </section>
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

  return (
    <div className="px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="truncate text-[13px] font-semibold text-foreground">{entry.name}</p>
            <Badge variant="secondary" className="text-[10px]">
              {getSkillSourceKindLabel(entry.sourceKind)}
            </Badge>
            <Badge className={cn('text-[10px]', stateTone(entry))}>
              {getSkillSourceStateLabel(entry.sourceState)}
            </Badge>
            <Badge className={cn('text-[10px]', trustTone(entry))}>
              {getSkillTrustStateLabel(entry.trustState)}
            </Badge>
          </div>
          <p className="mt-1 line-clamp-2 text-[12px] leading-[1.45] text-muted-foreground">
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

      <div className="mt-2 grid gap-2 text-[11.5px] text-muted-foreground sm:grid-cols-3">
        <div className="min-w-0">
          <span className="font-medium text-foreground">Hash </span>
          <span className="font-mono">{formatHash(entry.versionHash)}</span>
        </div>
        <div className="min-w-0">
          <span className="font-medium text-foreground">Last used </span>
          <span>{formatTimestamp(entry.lastUsedAt)}</span>
        </div>
        <div className="min-w-0">
          <span className="font-medium text-foreground">Scope </span>
          <span>{entry.scope === 'project' ? 'Project' : 'Global'}</span>
        </div>
      </div>

      {entry.lastDiagnostic ? (
        <div className="mt-2 flex items-start gap-2 rounded-md bg-destructive/10 px-2 py-1.5 text-[11.5px] text-destructive">
          <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>{entry.lastDiagnostic.message}</span>
        </div>
      ) : (
        <div className="mt-2 flex items-center gap-2 text-[11.5px] text-emerald-700 dark:text-emerald-300">
          {entry.trustState === 'trusted' || entry.trustState === 'user_approved' ? (
            <ShieldCheck className="h-3.5 w-3.5" />
          ) : (
            <CheckCircle2 className="h-3.5 w-3.5" />
          )}
          <span>No recorded diagnostic</span>
        </div>
      )}

      <details className="mt-2 group">
        <summary className="cursor-pointer select-none text-[11.5px] font-medium text-muted-foreground hover:text-foreground">
          Source metadata
        </summary>
        <dl className="mt-2 grid gap-x-3 gap-y-1 rounded-md bg-muted/40 p-2 text-[11px] sm:grid-cols-[110px_1fr]">
          {rows.map(([label, value]) => (
            <div key={label} className="contents">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="min-w-0 break-words font-mono text-foreground">{value}</dd>
            </div>
          ))}
        </dl>
      </details>
    </div>
  )
}
