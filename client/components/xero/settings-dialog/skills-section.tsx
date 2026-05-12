import { useDeferredValue, useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  Github,
  LoaderCircle,
  RefreshCcw,
  Search,
  ShieldAlert,
  Sparkles,
  Trash2,
} from 'lucide-react'
import type {
  AgentPaneView,
  OperatorActionErrorView,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
} from '@/src/features/xero/use-xero-desktop-state'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import {
  createSearchIndex,
  filterSearchIndex,
  useDeferredFilterQuery,
} from '@/lib/input-priority'
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
  path: string
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
    path: '',
  }
}

function deriveLocalRootLabel(rootId: string, path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  const lastSlash = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))
  const basename = lastSlash >= 0 ? trimmed.slice(lastSlash + 1) : trimmed
  return basename || rootId
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

function sourceFilterLabel(filter: SourceFilter): string {
  return filter === 'all' ? 'All sources' : getSkillSourceKindLabel(filter)
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path)
}

function validateLocalRootForm(form: LocalRootForm): LocalRootErrors {
  const errors: LocalRootErrors = {}
  const path = form.path.trim()

  if (!path) {
    errors.path = 'Path is required.'
  } else if (!isAbsolutePath(path)) {
    errors.path = 'Use an absolute directory path.'
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
    'border-success/30 bg-success/[0.08] text-success dark:border-success/40 dark:bg-success/[0.08] dark:text-success',
  info:
    'border-info/30 bg-info/[0.08] text-info dark:border-info/40 dark:bg-info/[0.08] dark:text-info',
  warn:
    'border-warning/30 bg-warning/[0.08] text-warning dark:border-warning/40 dark:bg-warning/[0.08] dark:text-warning',
  bad: 'border-destructive/40 bg-destructive/[0.08] text-destructive',
  neutral: 'border-border bg-secondary/60 text-foreground/70',
}

function Pill({ tone, children }: { tone: Tone; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        'inline-flex h-[20px] items-center rounded-full border px-2 text-[11px] font-medium',
        TONE_CLASS[tone],
      )}
    >
      {children}
    </span>
  )
}

function metadataRows(entry: SkillRegistryEntryDto): Array<[string, string]> {
  const rows: Array<[string, string | null | undefined]> = [
    ['Source', entry.source.label],
    ['Repository', entry.source.repo],
    ['Reference', entry.source.reference],
    ['Path', entry.source.path],
    ['Last used', formatTimestamp(entry.lastUsedAt)],
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
  const [githubExpanded, setGithubExpanded] = useState(false)

  const loading = skillRegistryLoadStatus === 'loading'
  const mutating = skillRegistryMutationStatus === 'running'
  const deferredQuery = useDeferredFilterQuery(query)
  const deferredSourceFilter = useDeferredValue(sourceFilter)

  useEffect(() => {
    if (!githubDirty) {
      setGithubForm(githubFormFromRegistry(skillRegistry))
    }
  }, [githubDirty, skillRegistry])

  const skillSearchIndex = useMemo(
    () =>
      createSearchIndex(skillRegistry?.entries ?? [], (entry) => [
        entry.name,
        entry.skillId,
        entry.description,
        entry.source.label,
        entry.sourceId,
      ]),
    [skillRegistry?.entries],
  )

  const filteredEntries = useMemo(
    () =>
      filterSearchIndex(skillSearchIndex, deferredQuery, (entry) =>
        deferredSourceFilter === 'all' || entry.sourceKind === deferredSourceFilter,
      ),
    [deferredQuery, deferredSourceFilter, skillSearchIndex],
  )

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
        rootId: null,
        path: localRootForm.path.trim(),
        enabled: true,
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
        description="Inspect installed and discoverable skills, then choose which sources Xero can load."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12.5px]"
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
      <section className="flex flex-col gap-3">
        <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">Sources</h4>

        <div className="overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40">
          <div className="flex items-center gap-3 px-4 py-3">
            <div className={cn(
              'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-card/60',
              projectSourceEnabled ? 'border-primary/30 text-primary' : 'border-border/60 text-muted-foreground',
            )}>
              <Sparkles className="h-[18px] w-[18px]" />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-[13.5px] font-semibold tracking-tight text-foreground">Project source</p>
              <p className="mt-0.5 truncate font-mono text-[12px] text-muted-foreground">
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

          <div className="flex flex-col">
            <div className="flex items-center gap-3 px-4 py-3">
              <div className={cn(
                'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-card/60',
                githubForm.enabled ? 'border-primary/30 text-primary' : 'border-border/60 text-muted-foreground',
              )}>
                <Github className="h-[18px] w-[18px]" />
              </div>
              <div className="min-w-0 flex-1">
                <p className="text-[13.5px] font-semibold tracking-tight text-foreground">GitHub source</p>
                <p className="mt-0.5 truncate font-mono text-[12px] text-muted-foreground">
                  {(skillRegistry?.sources.github.repo ?? githubForm.repo)}
                  {githubForm.reference ? <span className="text-muted-foreground/60"> · {githubForm.reference}</span> : null}
                </p>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-8 gap-1.5 px-2.5 text-[12px] text-muted-foreground hover:text-foreground"
                aria-label="Edit source"
                aria-expanded={githubExpanded}
                aria-controls="skill-github-advanced"
                onClick={() => setGithubExpanded((current) => !current)}
              >
                <ChevronDown
                  className={cn(
                    'h-3.5 w-3.5 transition-transform motion-fast',
                    githubExpanded ? 'rotate-0' : '-rotate-90',
                  )}
                  aria-hidden
                />
                Edit
              </Button>
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
            {githubExpanded ? (
              <div
                id="skill-github-advanced"
                className="grid gap-3 border-t border-border/40 bg-secondary/[0.07] px-4 py-3.5 sm:grid-cols-[1.4fr_0.8fr_0.8fr_auto] sm:items-end"
              >
                <FieldStack label="Repository" htmlFor="skill-gh-repo">
	                  <Input
	                    id="skill-gh-repo"
	                    aria-label="GitHub skill repository"
	                    value={githubForm.repo}
                    onChange={(event) => {
                      setGithubDirty(true)
                      setGithubForm((current) => ({ ...current, repo: event.target.value }))
                    }}
                    className="h-9 font-mono text-[12.5px]"
                    placeholder="owner/repo"
                  />
                </FieldStack>
                <FieldStack label="Branch" htmlFor="skill-gh-ref">
	                  <Input
	                    id="skill-gh-ref"
	                    aria-label="GitHub skill reference"
	                    value={githubForm.reference}
                    onChange={(event) => {
                      setGithubDirty(true)
                      setGithubForm((current) => ({ ...current, reference: event.target.value }))
                    }}
                    className="h-9 font-mono text-[12.5px]"
                    placeholder="main"
                  />
                </FieldStack>
                <FieldStack label="Root path" htmlFor="skill-gh-root">
	                  <Input
	                    id="skill-gh-root"
	                    aria-label="GitHub skill root"
	                    value={githubForm.root}
                    onChange={(event) => {
                      setGithubDirty(true)
                      setGithubForm((current) => ({ ...current, root: event.target.value }))
                    }}
                    className="h-9 font-mono text-[12.5px]"
                    placeholder="skills"
                  />
                </FieldStack>
                <Button
                  type="button"
                  size="sm"
                  className="h-9 text-[12.5px]"
                  disabled={mutating || !githubDirty || !onUpdateGithubSkillSource}
                  onClick={() => void handleSaveGithub()}
                >
                  Save
                </Button>
              </div>
            ) : null}
          </div>
        </div>
      </section>

      {/* Local roots */}
      <section className="flex flex-col gap-3">
        <div>
          <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">
            Local roots
            {localRoots.length > 0 ? (
              <span className="ml-2 text-[12px] font-normal text-muted-foreground">{localRoots.length}</span>
            ) : null}
          </h4>
          <p className="mt-1 text-[12.5px] leading-[1.5] text-muted-foreground">
            Point Xero at directories on disk that contain skill folders.
          </p>
        </div>

        <div className="flex items-start gap-2">
          <div className="flex-1">
            <Label htmlFor="skill-root-path" className="sr-only">
              Local root path
            </Label>
            <Input
              id="skill-root-path"
              value={localRootForm.path}
              onChange={(event) => setLocalRootForm((current) => ({ ...current, path: event.target.value }))}
              className="h-9 font-mono text-[12.5px]"
              placeholder="/absolute/path/to/skills"
              aria-invalid={Boolean(localRootErrors.path)}
            />
            {localRootErrors.path ? <p className="mt-1.5 text-[12px] text-destructive">{localRootErrors.path}</p> : null}
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-9 gap-1.5 px-3.5 text-[12.5px]"
            disabled={mutating || !onUpsertSkillLocalRoot}
            onClick={() => void handleAddLocalRoot()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            Add
          </Button>
        </div>

        {localRoots.length > 0 ? (
          <div className="overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40">
            {localRoots.map((root) => (
              <div
                key={root.rootId}
                className="flex items-center gap-3 px-4 py-3"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] font-semibold text-foreground">
                    {deriveLocalRootLabel(root.rootId, root.path)}
                  </p>
                  <p className="mt-0.5 truncate font-mono text-[12px] text-muted-foreground" title={root.path}>
                    {root.path}
                  </p>
                </div>
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
                  className="h-8 w-8 text-muted-foreground hover:text-destructive"
                  disabled={mutating || !onRemoveSkillLocalRoot}
                  aria-label={`Remove local skill root ${root.rootId}`}
                  onClick={() => void onRemoveSkillLocalRoot?.({ rootId: root.rootId, projectId }).catch(() => undefined)}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        ) : null}
      </section>

      {/* Skills list */}
      <section className="flex flex-col gap-3">
        <div className="flex items-baseline justify-between gap-3">
          <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">
            Discoverable skills
            {totalSkills > 0 ? (
              <span className="ml-2 text-[12px] font-normal text-muted-foreground">
                {filteredEntries.length === totalSkills
                  ? totalSkills
                  : `${filteredEntries.length} of ${totalSkills}`}
              </span>
            ) : null}
          </h4>
        </div>

        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="h-9 pl-9 text-[12.5px]"
              placeholder="Search skills"
              aria-label="Search skills"
            />
          </div>
          <Select value={sourceFilter} onValueChange={(value) => setSourceFilter(value as SourceFilter)}>
            <SelectTrigger className="h-9 w-full text-[12.5px] sm:w-48" aria-label="Filter skills by source" size="sm">
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

        {loading && !skillRegistry ? (
          <div className="flex items-center justify-center gap-2 rounded-lg border border-border/60 px-4 py-10 text-[12.5px] text-muted-foreground">
            <LoaderCircle className="h-4 w-4 animate-spin" />
            Loading skills
          </div>
        ) : filteredEntries.length === 0 ? (
          <div className="flex flex-col items-center gap-3 rounded-lg border border-border/60 bg-secondary/10 px-6 py-10 text-center">
            <div className="flex h-11 w-11 items-center justify-center rounded-full border border-border/60 bg-card/60">
              <Sparkles className="h-5 w-5 text-muted-foreground" />
            </div>
            <div className="flex max-w-sm flex-col gap-1">
              <p className="text-[14px] font-semibold tracking-tight text-foreground">
                {totalSkills > 0 ? 'No matches' : 'No skills found'}
              </p>
              <p className="text-[12.5px] leading-[1.5] text-muted-foreground">
                {totalSkills > 0
                  ? `Nothing matches "${query}"${sourceFilter !== 'all' ? ` in ${sourceFilterLabel(sourceFilter)}` : ''}.`
                  : 'Enable a source above or add a local root to discover skills.'}
              </p>
            </div>
            {totalSkills > 0 && (query || sourceFilter !== 'all') ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-8 gap-1.5 text-[12px]"
                onClick={() => {
                  setQuery('')
                  setSourceFilter('all')
                }}
              >
                Clear filters
              </Button>
            ) : null}
          </div>
        ) : (
          <div className="overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40">
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

        {skillRegistry?.diagnostics.length ? (
          <ul className="flex flex-col gap-2 rounded-md border border-warning/30 bg-warning/[0.06] px-3.5 py-3 text-warning dark:text-warning">
            {skillRegistry.diagnostics.map((diagnostic) => (
              <li
                key={`${diagnostic.code}:${diagnostic.relativePath ?? 'root'}`}
                className="flex items-start gap-2.5"
              >
                <AlertCircle className="mt-[1px] h-4 w-4 shrink-0" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="text-[12.5px] leading-[1.5]">{diagnostic.message}</p>
                  {diagnostic.relativePath ? (
                    <p className="mt-1 break-all font-mono text-[11.5px] text-warning/75 dark:text-warning/75">
                      {diagnostic.relativePath}
                    </p>
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        ) : null}
      </section>
    </div>
  )
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-2.5 text-[12.5px] leading-[1.5] text-destructive">
      <AlertCircle className="mt-[1px] h-4 w-4 shrink-0" />
      <span>{message}</span>
    </div>
  )
}

function FieldStack({
  label,
  htmlFor,
  children,
}: {
  label: string
  htmlFor: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={htmlFor} className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground/80">
        {label}
      </Label>
      {children}
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
  const showTrustPill = entry.trustState !== 'trusted' && entry.trustState !== 'user_approved'

  return (
    <div className="px-4 py-3.5">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate text-[13.5px] font-semibold tracking-tight text-foreground">{entry.name}</p>
            <Pill tone="neutral">{getSkillSourceKindLabel(entry.sourceKind)}</Pill>
            <Pill tone={stateTone(entry)}>{getSkillSourceStateLabel(entry.sourceState)}</Pill>
            {showTrustPill ? (
              <Pill tone={trustTone(entry)}>{getSkillTrustStateLabel(entry.trustState)}</Pill>
            ) : null}
          </div>
          <p className="mt-1.5 line-clamp-2 text-[12.5px] leading-[1.5] text-muted-foreground">
            {entry.description || entry.skillId}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {pending ? <LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" /> : null}
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
                  className="h-8 w-8 text-muted-foreground hover:text-destructive"
                  disabled={disabled}
                  aria-label={`Remove ${entry.name}`}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle className="text-[15px] font-semibold tracking-tight">Remove installed skill</AlertDialogTitle>
                  <AlertDialogDescription className="text-[12.5px] leading-[1.55]">
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

      <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-muted-foreground">
        <span>
          <span className="text-muted-foreground/60">Last used </span>
          <span className="text-foreground/80">{formatTimestamp(entry.lastUsedAt)}</span>
        </span>
        <span>
          <span className="text-muted-foreground/60">Scope </span>
          <span className="text-foreground/80">{entry.scope === 'project' ? 'Project' : 'Global'}</span>
        </span>
      </div>

      {entry.lastDiagnostic ? (
        <div className="mt-2.5 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12.5px] leading-[1.5] text-destructive">
          <ShieldAlert className="mt-[1px] h-4 w-4 shrink-0" />
          <span className="min-w-0">{entry.lastDiagnostic.message}</span>
        </div>
      ) : null}

      <details className="mt-2 group">
        <summary className="inline-flex cursor-pointer select-none items-center gap-1.5 rounded-md text-[12px] font-medium text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden [&::marker]:hidden">
          <ChevronRight className="h-3.5 w-3.5 transition-transform group-open:rotate-90" />
          Source metadata
        </summary>
        <dl className="mt-2 grid gap-x-4 gap-y-1.5 rounded-md border border-border/50 bg-secondary/20 px-3 py-2.5 text-[12px] sm:grid-cols-[120px_1fr]">
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
