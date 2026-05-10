'use client'

import { useCallback, useEffect, useMemo, useRef, useState, type UIEvent } from 'react'
import { Database, GitMerge, Loader2, Sparkles, Wrench } from 'lucide-react'

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import { cn } from '@/lib/utils'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringAttachableSkillDto,
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
  onSelectToolCategory,
  onSelectSkill,
  onSearchSkills,
  onResolveSkill,
  onSelectDbTable,
  onSelectConsumedArtifact,
  onClose,
}: DropPickerProps) {
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
              onSelect={(id) => onSelectToolCategory?.(id)}
            />
          ) : kind === 'db-table' ? (
            <DbTableItems
              tables={catalog?.dbTables ?? []}
              onSelect={(name) => onSelectDbTable?.(name)}
            />
          ) : (
            <ConsumedArtifactItems
              artifacts={catalog?.upstreamArtifacts ?? []}
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
  onSelect,
}: {
  categories: readonly AgentAuthoringToolCategoryDto[]
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
        return (
          <CommandItem
            key={category.id}
            value={`${category.id} ${valueText}`}
            onSelect={() => onSelect(category.id)}
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
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}

function DbTableItems({
  tables,
  onSelect,
}: {
  tables: readonly AgentAuthoringDbTableDto[]
  onSelect: (table: string) => void
}) {
  if (tables.length === 0) {
    return <CommandEmpty>No tables available.</CommandEmpty>
  }
  return (
    <CommandGroup heading="Tables">
      {tables.map((table) => {
        const valueText = `${table.table} ${table.purpose} ${table.columns.join(' ')}`.toLowerCase()
        return (
          <CommandItem
            key={table.table}
            value={`${table.table} ${valueText}`}
            onSelect={() => onSelect(table.table)}
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
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}

function ConsumedArtifactItems({
  artifacts,
  onSelect,
}: {
  artifacts: readonly AgentAuthoringUpstreamArtifactDto[]
  onSelect: (key: string) => void
}) {
  if (artifacts.length === 0) {
    return <CommandEmpty>No upstream artifacts available.</CommandEmpty>
  }
  return (
    <CommandGroup heading="Upstream agents">
      {artifacts.map((artifact) => {
        const key = `${artifact.sourceAgent}::${artifact.contract}`
        const valueText =
          `${artifact.sourceAgent} ${artifact.sourceAgentLabel} ${artifact.contract} ${artifact.label} ${artifact.description}`.toLowerCase()
        return (
          <CommandItem
            key={key}
            value={`${key} ${valueText}`}
            onSelect={() => onSelect(key)}
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
          </CommandItem>
        )
      })}
    </CommandGroup>
  )
}
