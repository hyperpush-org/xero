'use client'

import { useEffect, useRef } from 'react'
import { Database, GitMerge, Wrench } from 'lucide-react'

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
  AgentAuthoringDbTableDto,
  AgentAuthoringToolCategoryDto,
  AgentAuthoringUpstreamArtifactDto,
} from '@/src/lib/xero-model/workflow-agents'

import { humanizeIdentifier } from './build-agent-graph'

export type DropPickerKind = 'tool-category' | 'db-table' | 'consumed-artifact'

interface DropPickerProps {
  kind: DropPickerKind
  // Screen position where the user released the drag — popover anchors here.
  screenX: number
  screenY: number
  catalog: AgentAuthoringCatalogDto | null
  onSelectToolCategory?: (categoryId: string) => void
  onSelectDbTable?: (tableName: string) => void
  onSelectConsumedArtifact?: (key: string) => void
  onClose: () => void
}

const TITLES: Record<DropPickerKind, string> = {
  'tool-category': 'Add tool category',
  'db-table': 'Add database table',
  'consumed-artifact': 'Add upstream artifact',
}

const ICONS: Record<DropPickerKind, typeof Wrench> = {
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
  onSelectDbTable,
  onSelectConsumedArtifact,
  onClose,
}: DropPickerProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)

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
        <CommandInput placeholder="Search…" className="h-9" />
        <CommandList className="max-h-[260px]">
          {!catalog ? (
            <CommandEmpty>Loading catalog…</CommandEmpty>
          ) : kind === 'tool-category' ? (
            <ToolCategoryItems
              categories={catalog.toolCategories}
              onSelect={(id) => onSelectToolCategory?.(id)}
            />
          ) : kind === 'db-table' ? (
            <DbTableItems
              tables={catalog.dbTables}
              onSelect={(name) => onSelectDbTable?.(name)}
            />
          ) : (
            <ConsumedArtifactItems
              artifacts={catalog.upstreamArtifacts}
              onSelect={(key) => onSelectConsumedArtifact?.(key)}
            />
          )}
        </CommandList>
      </Command>
    </div>
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
