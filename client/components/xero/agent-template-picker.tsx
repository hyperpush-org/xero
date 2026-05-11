import { useMemo } from 'react'
import {
  Bug,
  ChevronLeft,
  ListChecks,
  Loader2,
  MessageCircle,
  Plus,
  Search,
  Sparkles,
  Wrench,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import type {
  AgentDefinitionBaseCapabilityProfileDto,
} from '@/src/lib/xero-model/agent-definition'
import type {
  AgentRefDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'

export interface AgentTemplatePickerProps {
  agents: WorkflowAgentSummaryDto[]
  loading?: boolean
  error?: Error | null
  onSelectTemplate: (ref: AgentRefDto) => void
  onStartBlank: () => void
  onCancel?: () => void
  headless?: boolean
  hideStartBlank?: boolean
  className?: string
}

const PROFILE_ICONS: Record<AgentDefinitionBaseCapabilityProfileDto, typeof Wrench> = {
  engineering: Wrench,
  debugging: Bug,
  planning: ListChecks,
  repository_recon: Search,
  agent_builder: Sparkles,
  observe_only: MessageCircle,
}

function profileIcon(profile: AgentDefinitionBaseCapabilityProfileDto) {
  return PROFILE_ICONS[profile] ?? MessageCircle
}

export function AgentTemplatePicker({
  agents,
  loading = false,
  error = null,
  onSelectTemplate,
  onStartBlank,
  onCancel,
  headless = false,
  hideStartBlank = false,
  className,
}: AgentTemplatePickerProps) {
  const { builtIns, customs } = useMemo(() => {
    const builtInList: WorkflowAgentSummaryDto[] = []
    const customList: WorkflowAgentSummaryDto[] = []
    for (const agent of agents) {
      if (agent.lifecycleState === 'archived') continue
      if (agent.ref.kind === 'built_in') {
        builtInList.push(agent)
      } else {
        customList.push(agent)
      }
    }
    customList.sort((a, b) => {
      const left = a.lastUsedAt ? Date.parse(a.lastUsedAt) : 0
      const right = b.lastUsedAt ? Date.parse(b.lastUsedAt) : 0
      if (left !== right) return right - left
      return a.displayName.localeCompare(b.displayName)
    })
    return { builtIns: builtInList, customs: customList }
  }, [agents])

  return (
    <div
      aria-label="Choose a starting template"
      className={cn(
        'flex w-full max-w-xl flex-col rounded-xl border border-border/70 bg-card/95 shadow-sm',
        className,
      )}
      onPointerDown={(event) => event.stopPropagation()}
    >
      {headless ? null : (
        <header className="flex items-center gap-2 border-b border-border/60 px-4 py-3">
          {onCancel ? (
            <button
              type="button"
              onClick={onCancel}
              aria-label="Back"
              className="grid h-6 w-6 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            >
              <ChevronLeft className="h-3.5 w-3.5" />
            </button>
          ) : null}
          <div className="flex min-w-0 flex-1 flex-col">
            <h3 className="text-[13px] font-semibold text-foreground">Choose a starting template</h3>
            <p className="text-[11.5px] text-muted-foreground">
              Templates open on the canvas with “(copy)” appended so you can edit freely.
            </p>
          </div>
        </header>
      )}

      <div className={cn('flex flex-col gap-3', headless ? 'px-0 py-0' : 'px-3 py-3')}>
        {hideStartBlank ? null : (
          <button
            type="button"
            onClick={onStartBlank}
            className="group flex items-center gap-3 rounded-lg border border-border/70 bg-background/60 px-3 py-2.5 text-left transition-colors hover:border-primary/40 hover:bg-secondary/40 focus-visible:border-primary/40 focus-visible:bg-secondary/40 focus-visible:outline-none"
          >
            <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border/70 bg-secondary/50 text-muted-foreground group-hover:text-primary">
              <Plus className="h-3.5 w-3.5" />
            </span>
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="text-[12.5px] font-medium text-foreground">Start blank</span>
              <span className="truncate text-[11.5px] text-muted-foreground">
                Open the canvas with an empty agent header.
              </span>
            </span>
          </button>
        )}

        {loading ? (
          <div className="flex items-center gap-2 px-1 py-1 text-[11.5px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Loading templates
          </div>
        ) : error ? (
          <p className="px-1 text-[11.5px] text-destructive">
            {error.message || 'Xero could not load templates.'}
          </p>
        ) : (
          <>
            {builtIns.length > 0 ? (
              <TemplateGroup
                title="Built-in"
                description="Read-only base agents. Picking one creates a copy you can edit."
                agents={builtIns}
                onSelectTemplate={onSelectTemplate}
              />
            ) : null}
            {customs.length > 0 ? (
              <TemplateGroup
                title="Your agents"
                description="Start from one of your saved custom agents."
                agents={customs}
                onSelectTemplate={onSelectTemplate}
              />
            ) : null}
            {builtIns.length === 0 && customs.length === 0 ? (
              <p className="px-1 text-[11.5px] text-muted-foreground">
                No templates available yet — start blank to author your first agent.
              </p>
            ) : null}
          </>
        )}
      </div>
    </div>
  )
}

interface TemplateGroupProps {
  title: string
  description: string
  agents: WorkflowAgentSummaryDto[]
  onSelectTemplate: (ref: AgentRefDto) => void
}

function TemplateGroup({ title, description, agents, onSelectTemplate }: TemplateGroupProps) {
  return (
    <section className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2 px-1">
        <h4 className="shrink-0 whitespace-nowrap text-[11.5px] font-semibold uppercase tracking-wider text-muted-foreground">
          {title}
          <span className="ml-1.5 font-normal normal-case tracking-normal text-muted-foreground/70">
            {agents.length}
          </span>
        </h4>
        <p className="hidden min-w-0 flex-1 truncate text-right text-[10.5px] text-muted-foreground/80 sm:block">
          {description}
        </p>
      </div>
      <ul className="flex flex-col gap-1">
        {agents.map((agent) => (
          <TemplateRow
            key={
              agent.ref.kind === 'built_in'
                ? `builtin:${agent.ref.runtimeAgentId}`
                : `custom:${agent.ref.definitionId}`
            }
            agent={agent}
            onSelect={() => onSelectTemplate(agent.ref)}
          />
        ))}
      </ul>
    </section>
  )
}

interface TemplateRowProps {
  agent: WorkflowAgentSummaryDto
  onSelect: () => void
}

function TemplateRow({ agent, onSelect }: TemplateRowProps) {
  const Icon = profileIcon(agent.baseCapabilityProfile)
  const draft = agent.lifecycleState === 'draft'
  return (
    <li>
      <button
        type="button"
        onClick={onSelect}
        className="group flex w-full items-start gap-3 rounded-md border border-transparent bg-background/40 px-3 py-2 text-left transition-colors hover:border-border/70 hover:bg-secondary/40 focus-visible:border-border/70 focus-visible:bg-secondary/40 focus-visible:outline-none"
      >
        <span className="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground group-hover:text-primary">
          <Icon className="h-3.5 w-3.5" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-[12.5px] font-medium text-foreground">
              {agent.displayName}
            </span>
            {draft ? (
              <Badge
                variant="outline"
                className="h-[16px] rounded px-1 py-0 text-[9.5px] font-semibold uppercase tracking-wider text-muted-foreground"
              >
                draft
              </Badge>
            ) : null}
          </span>
          {agent.description ? (
            <span className="mt-0.5 line-clamp-2 text-[11.5px] leading-snug text-muted-foreground">
              {agent.description}
            </span>
          ) : null}
        </span>
      </button>
    </li>
  )
}
