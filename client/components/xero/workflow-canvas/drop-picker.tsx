'use client'

import { useCallback, useEffect, useMemo, useRef, useState, type UIEvent } from 'react'
import { AlertTriangle, Database, GitMerge, Loader2, Sparkles, Wrench } from 'lucide-react'

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import { cn } from '@/lib/utils'
import {
  getAgentDefinitionBaseCapabilityLabel,
  type AgentDefinitionBaseCapabilityProfileDto,
} from '@/src/lib/xero-model/agent-definition'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringAttachableSkillDto,
  AgentAuthoringConstraintExplanationDto,
  AgentAuthoringProfileAvailabilityDto,
  AgentAuthoringSkillSearchResultDto,
  AgentAuthoringDbTableDto,
  SearchAgentAuthoringSkillsResponseDto,
  AgentAuthoringToolCategoryDto,
  AgentAuthoringUpstreamArtifactDto,
} from '@/src/lib/xero-model/workflow-agents'

import { humanizeIdentifier } from './build-agent-graph'

export type DropPickerKind = 'skill' | 'tool-category' | 'db-table' | 'consumed-artifact'

interface DropPickerProps {
  kind: DropPickerKind
  // Screen position where the user released the drag — popover anchors here.
  screenX: number
  screenY: number
  catalog: AgentAuthoringCatalogDto | null
  // Active base capability profile from the agent header. Used to grey out
  // catalog entries the runtime would refuse to expose. Null disables the
  // filter (preview/read-only contexts).
  currentProfile?: AgentDefinitionBaseCapabilityProfileDto | null
  onSelectToolCategory?: (categoryId: string) => void
  onSelectSkill?: (skill: AgentAuthoringAttachableSkillDto) => void
  onSearchSkills?: (params: {
    query: string
    offset: number
    limit: number
  }) => Promise<SearchAgentAuthoringSkillsResponseDto>
  onResolveSkill?: (
    skill: AgentAuthoringSkillSearchResultDto,
  ) => Promise<AgentAuthoringAttachableSkillDto>
  onSelectDbTable?: (tableName: string) => void
  onSelectConsumedArtifact?: (key: string) => void
  onClose: () => void
}

interface AvailabilityEntry {
  status: AgentAuthoringProfileAvailabilityDto['status']
  reason: string
  requiredProfile: AgentDefinitionBaseCapabilityProfileDto | null
  // Constraint explanation, when one exists, gives the user-facing
  // "why and how to fix" text emitted by the catalog.
  resolution: string | null
}

interface AvailabilityIndex {
  forSubject(
    subjectKind: string,
    subjectId: string,
  ): AvailabilityEntry | null
}

function buildAvailabilityIndex(
  catalog: AgentAuthoringCatalogDto | null,
  currentProfile: AgentDefinitionBaseCapabilityProfileDto | null | undefined,
): AvailabilityIndex {
  if (!catalog || !currentProfile) {
    return { forSubject: () => null }
  }
  const availabilityByKey = new Map<string, AgentAuthoringProfileAvailabilityDto>()
  for (const entry of catalog.profileAvailability ?? []) {
    if (entry.baseCapabilityProfile !== currentProfile) continue
    availabilityByKey.set(`${entry.subjectKind}:${entry.subjectId}`, entry)
  }
  const explanationByKey = new Map<string, AgentAuthoringConstraintExplanationDto>()
  for (const explanation of catalog.constraintExplanations ?? []) {
    if (explanation.baseCapabilityProfile !== currentProfile) continue
    explanationByKey.set(
      `${explanation.subjectKind}:${explanation.subjectId}`,
      explanation,
    )
  }
  return {
    forSubject(subjectKind, subjectId) {
      const key = `${subjectKind}:${subjectId}`
      const availability = availabilityByKey.get(key)
      if (!availability) return null
      const explanation = explanationByKey.get(key)
      return {
        status: availability.status,
        reason: availability.reason,
        requiredProfile: availability.requiredProfile ?? null,
        resolution: explanation?.resolution ?? null,
      }
    },
  }
}

interface CategoryAvailability {
  status: 'available' | 'partial' | 'requires_profile_change' | 'unavailable'
  availableCount: number
  totalCount: number
  // The profile the user would need to switch to in order to unlock the
  // largest number of currently-blocked tools. Only set when at least one
  // tool is profile-gated.
  recommendedProfile: AgentDefinitionBaseCapabilityProfileDto | null
}

function categoryAvailabilityFor(
  category: AgentAuthoringToolCategoryDto,
  index: AvailabilityIndex,
): CategoryAvailability | null {
  if (category.tools.length === 0) return null
  let availableCount = 0
  let unavailableCount = 0
  const requiredProfileVotes = new Map<AgentDefinitionBaseCapabilityProfileDto, number>()
  let sawAny = false
  for (const tool of category.tools) {
    const entry = index.forSubject('tool', tool.name)
    if (!entry) continue
    sawAny = true
    if (entry.status === 'available') {
      availableCount += 1
    } else if (entry.status === 'requires_profile_change') {
      if (entry.requiredProfile) {
        requiredProfileVotes.set(
          entry.requiredProfile,
          (requiredProfileVotes.get(entry.requiredProfile) ?? 0) + 1,
        )
      }
    } else {
      unavailableCount += 1
    }
  }
  if (!sawAny) return null
  let recommendedProfile: AgentDefinitionBaseCapabilityProfileDto | null = null
  let topVotes = 0
  for (const [profile, votes] of requiredProfileVotes) {
    if (votes > topVotes) {
      topVotes = votes
      recommendedProfile = profile
    }
  }
  let status: CategoryAvailability['status']
  if (availableCount === category.tools.length) {
    status = 'available'
  } else if (availableCount > 0) {
    status = 'partial'
  } else if (recommendedProfile) {
    status = 'requires_profile_change'
  } else {
    status = 'unavailable'
  }
  return {
    status,
    availableCount,
    totalCount: category.tools.length,
    recommendedProfile,
  }
}

function profileLabel(profile: AgentDefinitionBaseCapabilityProfileDto): string {
  return getAgentDefinitionBaseCapabilityLabel(profile)
}

function badgeLabel(entry: AvailabilityEntry): string | null {
  if (entry.status === 'available') return null
  if (entry.status === 'requires_profile_change' && entry.requiredProfile) {
    return `Requires ${profileLabel(entry.requiredProfile)}`
  }
  return 'Not available'
}

const SKILL_PAGE_SIZE = 10

type SkillPickerItem =
  | {
      kind: 'attachable'
      key: string
      skill: AgentAuthoringAttachableSkillDto
    }
  | {
      kind: 'online'
      key: string
      skill: AgentAuthoringSkillSearchResultDto
    }

const TITLES: Record<DropPickerKind, string> = {
  skill: 'Attach skill',
  'tool-category': 'Add tool category',
  'db-table': 'Add database table',
  'consumed-artifact': 'Add upstream artifact',
}

const ICONS: Record<DropPickerKind, typeof Wrench> = {
  skill: Sparkles,
  'tool-category': Wrench,
  'db-table': Database,
  'consumed-artifact': GitMerge,
}

export function DropPicker({
  kind,
  screenX,
  screenY,
  catalog,
  currentProfile,
  onSelectToolCategory,
  onSelectSkill,
  onSearchSkills,
  onResolveSkill,
  onSelectDbTable,
  onSelectConsumedArtifact,
  onClose,
}: DropPickerProps) {
  const availabilityIndex = useMemo(
    () => buildAvailabilityIndex(catalog, currentProfile ?? null),
    [catalog, currentProfile],
  )
  const profileLabelText = currentProfile ? profileLabel(currentProfile) : null
  const containerRef = useRef<HTMLDivElement | null>(null)
  const [search, setSearch] = useState('')
  const [onlineSkills, setOnlineSkills] = useState<readonly AgentAuthoringSkillSearchResultDto[]>([])
  const [skillSearchLoading, setSkillSearchLoading] = useState(false)
  const [skillSearchError, setSkillSearchError] = useState<string | null>(null)
  const [skillSearchNextOffset, setSkillSearchNextOffset] = useState<number | null>(0)
  const [skillSearchHasMore, setSkillSearchHasMore] = useState(false)
  const [resolvingSkillKey, setResolvingSkillKey] = useState<string | null>(null)
  const skillSearchRequestRef = useRef(0)

  // Click-away closes the picker. Escape too. We listen on the document so
  // the picker dismisses for any out-of-bounds interaction.
  useEffect(() => {
    function handlePointer(event: PointerEvent) {
      if (!containerRef.current) return
      if (containerRef.current.contains(event.target as Node)) return
      onClose()
    }
    function handleKey(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('pointerdown', handlePointer)
    document.addEventListener('keydown', handleKey)
    return () => {
      document.removeEventListener('pointerdown', handlePointer)
      document.removeEventListener('keydown', handleKey)
    }
  }, [onClose])

  useEffect(() => {
    if (kind !== 'skill' || !onSearchSkills) {
      setOnlineSkills([])
      setSkillSearchLoading(false)
      setSkillSearchError(null)
      setSkillSearchNextOffset(0)
      setSkillSearchHasMore(false)
      return
    }

    let cancelled = false
    const query = search.trim()
    const requestId = skillSearchRequestRef.current + 1
    skillSearchRequestRef.current = requestId
    setSkillSearchLoading(true)
    setSkillSearchError(null)
    setSkillSearchNextOffset(null)
    setSkillSearchHasMore(false)

    const handle = window.setTimeout(
      () => {
        void onSearchSkills({ query, offset: 0, limit: SKILL_PAGE_SIZE })
          .then((response) => {
            if (cancelled || skillSearchRequestRef.current !== requestId) return
            setOnlineSkills(response.entries)
            setSkillSearchNextOffset(response.nextOffset)
            setSkillSearchHasMore(response.hasMore)
          })
          .catch(() => {
            if (cancelled || skillSearchRequestRef.current !== requestId) return
            setOnlineSkills([])
            setSkillSearchNextOffset(null)
            setSkillSearchHasMore(false)
            setSkillSearchError('Online skill search failed.')
          })
          .finally(() => {
            if (!cancelled && skillSearchRequestRef.current === requestId) {
              setSkillSearchLoading(false)
            }
          })
      },
      query.length > 0 ? 250 : 0,
    )

    return () => {
      cancelled = true
      window.clearTimeout(handle)
    }
  }, [kind, onSearchSkills, search])

  const loadNextSkillPage = useCallback(() => {
    if (kind !== 'skill' || !onSearchSkills) return
    if (skillSearchLoading || !skillSearchHasMore || skillSearchNextOffset == null) return

    const query = search.trim()
    const offset = skillSearchNextOffset
    const requestId = skillSearchRequestRef.current + 1
    skillSearchRequestRef.current = requestId
    setSkillSearchLoading(true)
    setSkillSearchError(null)
    void onSearchSkills({ query, offset, limit: SKILL_PAGE_SIZE })
      .then((response) => {
        if (skillSearchRequestRef.current !== requestId) return
        setOnlineSkills((current) => mergeOnlineSkills(current, response.entries))
        setSkillSearchNextOffset(response.nextOffset)
        setSkillSearchHasMore(response.hasMore)
      })
      .catch(() => {
        if (skillSearchRequestRef.current !== requestId) return
        setSkillSearchError('Online skill search failed.')
      })
      .finally(() => {
        if (skillSearchRequestRef.current === requestId) {
          setSkillSearchLoading(false)
        }
      })
  }, [
    kind,
    onSearchSkills,
    search,
    skillSearchHasMore,
    skillSearchLoading,
    skillSearchNextOffset,
  ])

  const handleListScroll = useCallback(
    (event: UIEvent<HTMLDivElement>) => {
      if (kind !== 'skill') return
      const element = event.currentTarget
      const distanceToBottom = element.scrollHeight - element.scrollTop - element.clientHeight
      if (distanceToBottom <= 32) {
        loadNextSkillPage()
      }
    },
    [kind, loadNextSkillPage],
  )

  const skillItems = useMemo(() => {
    if (kind !== 'skill') return []
    const items: SkillPickerItem[] = []
    const installedSkillIds = new Set<string>()
    for (const skill of catalog?.attachableSkills ?? []) {
      installedSkillIds.add(skill.skillId)
      items.push({ kind: 'attachable', key: skill.sourceId, skill })
    }
    for (const skill of onlineSkills) {
      const key = onlineSkillKey(skill)
      if (installedSkillIds.has(skill.skillId)) continue
      items.push({ kind: 'online', key, skill })
    }
    return items
  }, [catalog?.attachableSkills, kind, onlineSkills])

  const Icon = ICONS[kind]

  return (
    <div
      ref={containerRef}
      style={{ position: 'fixed', left: screenX, top: screenY, zIndex: 100 }}
      className={cn(
        'w-[320px] rounded-md border border-border/70 bg-popover text-popover-foreground shadow-lg',
        '-translate-x-2 -translate-y-2',
      )}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <div className="flex items-center gap-2 border-b border-border/50 px-2.5 py-1.5 text-[11px] font-semibold text-muted-foreground">
        <Icon className="h-3 w-3" aria-hidden="true" />
        <span>{TITLES[kind]}</span>
      </div>
      <Command>
        <CommandInput
          placeholder={kind === 'skill' ? 'Search installed or online skills…' : 'Search…'}
          className="h-9"
          value={search}
          onValueChange={setSearch}
        />
        <CommandList className="max-h-[260px]" onScroll={handleListScroll}>
          {!catalog && kind !== 'skill' ? (
            <CommandEmpty>Loading catalog…</CommandEmpty>
          ) : kind === 'skill' ? (
            <SkillItems
              skills={skillItems}
              onlineSearchEnabled={Boolean(onSearchSkills)}
              loading={skillSearchLoading}
              error={skillSearchError}
              resolvingKey={resolvingSkillKey}
              onSelect={(skill) => onSelectSkill?.(skill)}
              onResolveSkill={onResolveSkill}
              onResolveStart={(key) => {
                setResolvingSkillKey(key)
                setSkillSearchError(null)
              }}
              onResolveFinish={() => setResolvingSkillKey(null)}
              onResolveError={() => setSkillSearchError('Online skill attachment failed.')}
            />
          ) : kind === 'tool-category' ? (
            <ToolCategoryItems
              categories={catalog?.toolCategories ?? []}
              availability={availabilityIndex}
              profileLabelText={profileLabelText}
              onSelect={(id) => onSelectToolCategory?.(id)}
            />
          ) : kind === 'db-table' ? (
            <DbTableItems
              tables={catalog?.dbTables ?? []}
              availability={availabilityIndex}
              profileLabelText={profileLabelText}
              onSelect={(name) => onSelectDbTable?.(name)}
            />
          ) : (
            <ConsumedArtifactItems
              artifacts={catalog?.upstreamArtifacts ?? []}
              availability={availabilityIndex}
              profileLabelText={profileLabelText}
              onSelect={(key) => onSelectConsumedArtifact?.(key)}
            />
          )}
        </CommandList>
      </Command>
    </div>
  )
}

function onlineSkillKey(skill: AgentAuthoringSkillSearchResultDto): string {
  return `${skill.source}:${skill.skillId}`
}

function mergeOnlineSkills(
  current: readonly AgentAuthoringSkillSearchResultDto[],
  incoming: readonly AgentAuthoringSkillSearchResultDto[],
): AgentAuthoringSkillSearchResultDto[] {
  const byKey = new Map<string, AgentAuthoringSkillSearchResultDto>()
  for (const skill of current) {
    byKey.set(onlineSkillKey(skill), skill)
  }
  for (const skill of incoming) {
    byKey.set(onlineSkillKey(skill), skill)
  }
  return [...byKey.values()]
}

function SkillItems({
  skills,
  onlineSearchEnabled,
  loading,
  error,
  resolvingKey,
  onSelect,
  onResolveSkill,
  onResolveStart,
  onResolveFinish,
  onResolveError,
}: {
  skills: readonly SkillPickerItem[]
  onlineSearchEnabled: boolean
  loading: boolean
  error: string | null
  resolvingKey: string | null
  onSelect: (skill: AgentAuthoringAttachableSkillDto) => void
  onResolveSkill?: (
    skill: AgentAuthoringSkillSearchResultDto,
  ) => Promise<AgentAuthoringAttachableSkillDto>
  onResolveStart: (key: string) => void
  onResolveFinish: () => void
  onResolveError: () => void
}) {
  if (skills.length === 0) {
    if (loading) {
      return (
        <CommandEmpty>
          <span className="inline-flex items-center justify-center gap-2 text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            <span>Loading online skills…</span>
          </span>
        </CommandEmpty>
      )
    }
    const emptyMessage = error
      ? error
      : onlineSearchEnabled
        ? 'No attachable skills found in configured sources.'
        : 'No attachable skills available.'
    return <CommandEmpty>{emptyMessage}</CommandEmpty>
  }
  const handleSelectOnline = (item: Extract<SkillPickerItem, { kind: 'online' }>) => {
    if (!onResolveSkill || resolvingKey) return
    onResolveStart(item.key)
    void onResolveSkill(item.skill)
      .then((resolvedSkill) => onSelect(resolvedSkill))
      .catch(() => onResolveError())
      .finally(onResolveFinish)
  }
  return (
    <>
      <CommandGroup heading="Skills">
        {skills.map((item) => {
          const isResolving = resolvingKey === item.key
          if (item.kind === 'online') {
            const skill = item.skill
            const valueText =
              `${skill.name} ${skill.skillId} ${skill.source} ${skill.description} github`.toLowerCase()
            return (
              <CommandItem
                key={item.key}
                value={`${item.key} ${valueText}`}
                disabled={isResolving || !onResolveSkill}
                onSelect={() => handleSelectOnline(item)}
                className="flex flex-col items-start gap-0.5 px-2 py-1.5"
              >
                <div className="flex w-full items-center gap-2">
                  {isResolving ? (
                    <Loader2 className="h-3 w-3 shrink-0 animate-spin text-muted-foreground" aria-hidden="true" />
                  ) : (
                    <Sparkles className="h-3 w-3 shrink-0 text-rose-500" aria-hidden="true" />
                  )}
                  <span className="truncate text-[11.5px] font-medium">{skill.name}</span>
                  <span className="ml-auto text-[10px] text-muted-foreground">
                    {skill.source}
                  </span>
                </div>
                <span className="ml-5 truncate font-mono text-[10px] text-muted-foreground/75">
                  {skill.skillId}
                </span>
                {skill.installs != null ? (
                  <span className="ml-5 text-[10px] text-muted-foreground/70">
                    {skill.installs.toLocaleString()} installs
                  </span>
                ) : null}
                {skill.description ? (
                  <span className="ml-5 text-[10px] text-muted-foreground/85 leading-snug line-clamp-2">
                    {skill.description}
                  </span>
                ) : null}
              </CommandItem>
            )
          }

          const skill = item.skill
          const valueText =
            `${skill.name} ${skill.skillId} ${skill.sourceId} ${skill.description} ${skill.sourceKind}`.toLowerCase()
          return (
            <CommandItem
              key={item.key}
              value={`${item.key} ${valueText}`}
              onSelect={() => onSelect(item.skill)}
              className="flex flex-col items-start gap-0.5 px-2 py-1.5"
            >
              <div className="flex w-full items-center gap-2">
                <Sparkles className="h-3 w-3 shrink-0 text-rose-500" aria-hidden="true" />
                <span className="truncate text-[11.5px] font-medium">{skill.name}</span>
                <span className="ml-auto text-[10px] text-muted-foreground">
                  {humanizeIdentifier(skill.sourceKind)}
                </span>
              </div>
              <span className="ml-5 truncate font-mono text-[10px] text-muted-foreground/75">
                {skill.skillId}
              </span>
              {skill.description ? (
                <span className="ml-5 text-[10px] text-muted-foreground/85 leading-snug line-clamp-2">
                  {skill.description}
                </span>
              ) : null}
            </CommandItem>
          )
        })}
      </CommandGroup>
      {loading && skills.length > 0 ? (
        <div
          role="status"
          aria-live="polite"
          className="flex items-center gap-2 px-3 py-2 text-[11px] text-muted-foreground"
        >
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          <span>Loading more skills…</span>
        </div>
      ) : null}
      {error ? (
        <div className="border-t border-border/40 px-3 py-2 text-[11px] text-destructive">
          {error}
        </div>
      ) : null}
    </>
  )
}

function ToolCategoryItems({
  categories,
  availability,
  profileLabelText,
  onSelect,
}: {
  categories: readonly AgentAuthoringToolCategoryDto[]
  availability: AvailabilityIndex
  profileLabelText: string | null
  onSelect: (id: string) => void
}) {
  if (categories.length === 0) {
    return <CommandEmpty>No tool categories available.</CommandEmpty>
  }
  return (
    <CommandGroup heading="Categories">
      {categories.map((category) => {
        const valueText = `${category.id} ${category.label} ${category.description} ${category.tools
          .map((tool) => tool.name)
          .join(' ')}`.toLowerCase()
        const summary = categoryAvailabilityFor(category, availability)
        const blocked =
          summary != null &&
          (summary.status === 'unavailable' ||
            summary.status === 'requires_profile_change')
        const partial = summary?.status === 'partial'
        return (
          <CommandItem
            key={category.id}
            value={`${category.id} ${valueText}`}
            disabled={blocked}
            onSelect={() => {
              if (blocked) return
              onSelect(category.id)
            }}
            data-testid={`drop-picker-category-${category.id}`}
            data-availability={summary?.status ?? 'available'}
            className="flex flex-col items-start gap-0.5 px-2 py-1.5"
          >
            <div className="flex w-full items-center gap-2">
              <Wrench className="h-3 w-3 shrink-0 text-sky-500" aria-hidden="true" />
              <span className="truncate text-[11.5px] font-medium">{category.label}</span>
              <span className="ml-auto text-[10px] text-muted-foreground">
                {category.tools.length} {category.tools.length === 1 ? 'tool' : 'tools'}
              </span>
            </div>
            {category.description ? (
              <span className="ml-5 text-[10px] text-muted-foreground/85 leading-snug line-clamp-2">
                {category.description}
              </span>
            ) : null}
            {summary && summary.status !== 'available' ? (
              <CategoryAvailabilityBadge
                summary={summary}
                profileLabelText={profileLabelText}
                blocked={blocked}
                partial={partial}
              />
            ) : null}
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}

function CategoryAvailabilityBadge({
  summary,
  profileLabelText,
  blocked,
  partial,
}: {
  summary: CategoryAvailability
  profileLabelText: string | null
  blocked: boolean
  partial: boolean
}) {
  const profileText = profileLabelText ?? 'this profile'
  let label: string
  if (blocked && summary.recommendedProfile) {
    label = `Requires ${profileLabel(summary.recommendedProfile)} profile`
  } else if (blocked) {
    label = `Not available on ${profileText}`
  } else if (partial) {
    label = `${summary.availableCount} of ${summary.totalCount} tools available on ${profileText}`
  } else {
    label = `Limited on ${profileText}`
  }
  return (
    <span
      role="note"
      className="ml-5 mt-0.5 inline-flex items-center gap-1 text-[10px] text-amber-500"
    >
      <AlertTriangle className="h-2.5 w-2.5 shrink-0" aria-hidden="true" />
      <span className="leading-snug line-clamp-2">{label}</span>
    </span>
  )
}

function DbTableItems({
  tables,
  availability,
  profileLabelText,
  onSelect,
}: {
  tables: readonly AgentAuthoringDbTableDto[]
  availability: AvailabilityIndex
  profileLabelText: string | null
  onSelect: (table: string) => void
}) {
  if (tables.length === 0) {
    return <CommandEmpty>No tables available.</CommandEmpty>
  }
  return (
    <CommandGroup heading="Tables">
      {tables.map((table) => {
        const valueText = `${table.table} ${table.purpose} ${table.columns.join(' ')}`.toLowerCase()
        const entry = availability.forSubject('db_touchpoint', table.table)
        const blocked =
          entry != null &&
          (entry.status === 'unavailable' || entry.status === 'requires_profile_change')
        return (
          <CommandItem
            key={table.table}
            value={`${table.table} ${valueText}`}
            disabled={blocked}
            onSelect={() => {
              if (blocked) return
              onSelect(table.table)
            }}
            data-testid={`drop-picker-db-${table.table}`}
            data-availability={entry?.status ?? 'available'}
            className="flex flex-col items-start gap-0.5 px-2 py-1.5"
          >
            <div className="flex w-full items-center gap-2">
              <Database className="h-3 w-3 shrink-0 text-emerald-500" aria-hidden="true" />
              <span className="truncate text-[11.5px] font-medium">
                {humanizeIdentifier(table.table)}
              </span>
            </div>
            {table.purpose ? (
              <span className="ml-5 text-[10px] text-muted-foreground/85 leading-snug line-clamp-2">
                {table.purpose}
              </span>
            ) : null}
            {entry && entry.status !== 'available' ? (
              <SubjectAvailabilityBadge entry={entry} profileLabelText={profileLabelText} />
            ) : null}
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}

function SubjectAvailabilityBadge({
  entry,
  profileLabelText,
}: {
  entry: AvailabilityEntry
  profileLabelText: string | null
}) {
  const label = badgeLabel(entry) ?? 'Not available'
  const explanation =
    entry.resolution ??
    (profileLabelText ? `${entry.reason} (profile: ${profileLabelText})` : entry.reason)
  return (
    <span
      role="note"
      title={explanation}
      className="ml-5 mt-0.5 inline-flex items-center gap-1 text-[10px] text-amber-500"
    >
      <AlertTriangle className="h-2.5 w-2.5 shrink-0" aria-hidden="true" />
      <span className="leading-snug line-clamp-2">{label}</span>
    </span>
  )
}

function ConsumedArtifactItems({
  artifacts,
  availability,
  profileLabelText,
  onSelect,
}: {
  artifacts: readonly AgentAuthoringUpstreamArtifactDto[]
  availability: AvailabilityIndex
  profileLabelText: string | null
  onSelect: (key: string) => void
}) {
  if (artifacts.length === 0) {
    return <CommandEmpty>No upstream artifacts available.</CommandEmpty>
  }
  return (
    <CommandGroup heading="Upstream agents">
      {artifacts.map((artifact) => {
        const key = `${artifact.sourceAgent}::${artifact.contract}`
        const subjectId = `${artifact.sourceAgent}:${artifact.contract}`
        const entry = availability.forSubject('upstream_artifact', subjectId)
        const blocked =
          entry != null &&
          (entry.status === 'unavailable' || entry.status === 'requires_profile_change')
        const valueText =
          `${artifact.sourceAgent} ${artifact.sourceAgentLabel} ${artifact.contract} ${artifact.label} ${artifact.description}`.toLowerCase()
        return (
          <CommandItem
            key={key}
            value={`${key} ${valueText}`}
            disabled={blocked}
            onSelect={() => {
              if (blocked) return
              onSelect(key)
            }}
            data-testid={`drop-picker-upstream-${artifact.sourceAgent}-${artifact.contract}`}
            data-availability={entry?.status ?? 'available'}
            className="flex flex-col items-start gap-0.5 px-2 py-1.5"
          >
            <div className="flex w-full items-center gap-2">
              <GitMerge className="h-3 w-3 shrink-0 text-teal-500" aria-hidden="true" />
              <span className="truncate text-[11.5px] font-medium">
                {artifact.sourceAgentLabel} → {artifact.contractLabel}
              </span>
            </div>
            {artifact.description ? (
              <span className="ml-5 text-[10px] text-muted-foreground/85 leading-snug line-clamp-2">
                {artifact.description}
              </span>
            ) : null}
            {entry && entry.status !== 'available' ? (
              <SubjectAvailabilityBadge entry={entry} profileLabelText={profileLabelText} />
            ) : null}
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}
