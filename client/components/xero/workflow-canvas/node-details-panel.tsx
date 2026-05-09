'use client'

import type { ReactNode } from 'react'
import {
  ArrowDownLeft,
  CircleDot,
  Eye,
  Pencil,
  Sparkles,
  Tag,
  Workflow,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionScopeLabel,
} from '@/src/lib/xero-model/agent-definition'
import { getRuntimeRunApprovalModeLabel } from '@/src/lib/xero-model/runtime'
import type {
  AgentDbTouchpointKindDto,
  AgentTriggerRefDto,
} from '@/src/lib/xero-model/workflow-agents'

import type {
  AgentGraphNode,
  AgentHeaderNodeData,
  ConsumedArtifactNodeData,
  DbTableNodeData,
  OutputNodeData,
  OutputSectionNodeData,
  PromptNodeData,
  ToolNodeData,
} from './build-agent-graph'
import { humanizeIdentifier, lifecycleEventLabel } from './build-agent-graph'
import { panelMetaForNode } from './node-properties-panel'

interface NodeDetailsPanelProps {
  selectedNode: AgentGraphNode | null
  onClose: () => void
}

// Read-only counterpart to NodePropertiesPanel. Shares the panel shell so
// view-mode and edit-mode feel like the same affordance, with edit fields
// swapped for plain labelled rows. Lane-label and tool-group-frame nodes
// have no user-facing data, so they fall through to nothing.
export function NodeDetailsPanel({ selectedNode, onClose }: NodeDetailsPanelProps) {
  if (!selectedNode) return null
  if (selectedNode.type === 'lane-label' || selectedNode.type === 'tool-group-frame') return null

  const meta = panelMetaForNode(selectedNode)

  return (
    <div
      className="agent-properties-panel pointer-events-auto absolute bottom-4 left-4 top-14 z-30 flex w-[272px] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[12px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
      onPointerDown={(event) => event.stopPropagation()}
      onWheel={(event) => event.stopPropagation()}
    >
      <PanelHeader meta={meta} onClose={onClose} />
      <div className="flex-1 overflow-y-auto">
        <div className="space-y-4 px-3 py-3">{renderDetails(selectedNode)}</div>
      </div>
    </div>
  )
}

function PanelHeader({
  meta,
  onClose,
}: {
  meta: ReturnType<typeof panelMetaForNode>
  onClose: () => void
}) {
  return (
    <header className="flex items-center gap-2 border-b border-border/50 px-3 py-1.5">
      <span
        className={cn(
          'inline-flex h-5 w-5 shrink-0 items-center justify-center rounded',
          meta.iconWrap,
        )}
      >
        <meta.Icon className="h-3 w-3" />
      </span>
      <p
        className="min-w-0 flex-1 truncate text-[12px] font-semibold leading-none text-foreground"
        title={`${meta.title} · ${meta.subtitle}`}
      >
        {meta.title}
      </p>
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        onClick={onClose}
        className="size-5 shrink-0 text-muted-foreground hover:text-foreground"
        aria-label="Close details"
      >
        <X className="h-3 w-3" />
      </Button>
    </header>
  )
}

function renderDetails(node: AgentGraphNode): ReactNode {
  switch (node.type) {
    case 'agent-header':
      return <AgentHeaderDetails data={node.data as AgentHeaderNodeData} />
    case 'prompt':
      return <PromptDetails data={node.data as PromptNodeData} />
    case 'tool':
      return <ToolDetails data={node.data as ToolNodeData} />
    case 'db-table':
      return <DbTableDetails data={node.data as DbTableNodeData} />
    case 'agent-output':
      return <OutputDetails data={node.data as OutputNodeData} />
    case 'output-section':
      return <OutputSectionDetails data={node.data as OutputSectionNodeData} />
    case 'consumed-artifact':
      return <ConsumedArtifactDetails data={node.data as ConsumedArtifactNodeData} />
    default:
      return null
  }
}

// ──────────────────────────────────────────────────────────────────────────
// Building blocks
// ──────────────────────────────────────────────────────────────────────────

function Section({
  label,
  count,
  children,
}: {
  label: string
  count?: number
  children: ReactNode
}) {
  return (
    <section className="space-y-1.5">
      <div className="flex items-baseline gap-2">
        <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
          {label}
        </h3>
        {typeof count === 'number' ? (
          <span className="text-[10px] font-medium tabular-nums text-muted-foreground/55">
            {count}
          </span>
        ) : null}
      </div>
      <div className="text-[12px] leading-relaxed text-foreground/90">{children}</div>
    </section>
  )
}

function Prose({ children }: { children: ReactNode }) {
  return (
    <p className="text-[12px] leading-relaxed text-muted-foreground">{children}</p>
  )
}

function Body({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-md border border-border/45 bg-background/40 px-2.5 py-2 text-[12px] leading-[1.55] text-foreground/90 [white-space:pre-wrap] [word-break:break-word]">
      {children}
    </div>
  )
}

interface MetaItem {
  label: string
  value: string
}

function MetaRow({ items }: { items: MetaItem[] }) {
  return (
    <dl className="grid grid-cols-2 gap-x-3 gap-y-2.5">
      {items.map((item) => (
        <div key={item.label} className="min-w-0 space-y-0.5">
          <dt className="text-[9.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground/70">
            {item.label}
          </dt>
          <dd className="truncate text-[12px] capitalize text-foreground/95">
            {item.value || '—'}
          </dd>
        </div>
      ))}
    </dl>
  )
}

function Chip({
  children,
  variant = 'default',
  mono = false,
}: {
  children: ReactNode
  variant?: 'default' | 'mono' | 'soft'
  mono?: boolean
}) {
  return (
    <span
      className={cn(
        'inline-flex max-w-full items-center gap-1 truncate rounded-md border px-1.5 py-0.5 text-[10.5px] leading-none',
        variant === 'soft'
          ? 'border-border/40 bg-muted/35 text-foreground/85'
          : 'border-border/55 bg-background/55 text-foreground/85',
        (mono || variant === 'mono') && 'font-mono text-[10px] tracking-tight',
      )}
    >
      {children}
    </span>
  )
}

function ChipRow({ children }: { children: ReactNode }) {
  return <div className="flex flex-wrap gap-1">{children}</div>
}

// ──────────────────────────────────────────────────────────────────────────
// Touchpoint pill — surfaces the read/write/encouraged distinction visually
// ──────────────────────────────────────────────────────────────────────────

const TOUCHPOINT_STYLES: Record<
  AgentDbTouchpointKindDto,
  { label: string; icon: LucideIcon; cls: string; dot: string }
> = {
  read: {
    label: 'Read',
    icon: Eye,
    cls: 'border-sky-500/30 bg-sky-500/12 text-sky-700 dark:text-sky-300',
    dot: 'bg-sky-500',
  },
  write: {
    label: 'Write',
    icon: Pencil,
    cls: 'border-rose-500/30 bg-rose-500/12 text-rose-700 dark:text-rose-300',
    dot: 'bg-rose-500',
  },
  encouraged: {
    label: 'Encouraged',
    icon: Sparkles,
    cls: 'border-amber-500/30 bg-amber-500/12 text-amber-700 dark:text-amber-400',
    dot: 'bg-amber-500',
  },
}

function TouchpointPill({ kind }: { kind: AgentDbTouchpointKindDto }) {
  const style = TOUCHPOINT_STYLES[kind]
  const Icon = style.icon
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium leading-none',
        style.cls,
      )}
    >
      <Icon className="h-3 w-3" />
      {style.label}
    </span>
  )
}

function GatePill({ on, label }: { on: boolean; label: string }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium leading-none',
        on
          ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
          : 'border-border/45 bg-muted/30 text-muted-foreground/75',
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          'h-1.5 w-1.5 rounded-full',
          on ? 'bg-emerald-500' : 'bg-muted-foreground/40',
        )}
      />
      {label}
    </span>
  )
}

// ──────────────────────────────────────────────────────────────────────────
// Stat strip — used on agent header to surface counts visually
// ──────────────────────────────────────────────────────────────────────────

function StatStrip({
  items,
}: {
  items: { label: string; value: number }[]
}) {
  return (
    <div className="grid grid-cols-2 gap-1.5">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex items-baseline justify-between rounded-md border border-border/45 bg-background/40 px-2.5 py-1.5"
        >
          <span className="text-[10.5px] text-muted-foreground/80">{item.label}</span>
          <span className="text-[13px] font-semibold tabular-nums text-foreground">
            {item.value}
          </span>
        </div>
      ))}
    </div>
  )
}

// ──────────────────────────────────────────────────────────────────────────
// Trigger row — gives each trigger kind an icon + role label
// ──────────────────────────────────────────────────────────────────────────

function triggerInfo(trigger: AgentTriggerRefDto): {
  Icon: LucideIcon
  kind: string
  body: string
} {
  switch (trigger.kind) {
    case 'tool':
      return { Icon: Zap, kind: 'Tool', body: humanizeIdentifier(trigger.name) }
    case 'output_section':
      return { Icon: Tag, kind: 'Section', body: humanizeIdentifier(trigger.id) }
    case 'lifecycle':
      return {
        Icon: CircleDot,
        kind: 'Lifecycle',
        body: lifecycleEventLabel(trigger.event),
      }
    case 'upstream_artifact':
      return { Icon: Workflow, kind: 'Upstream', body: humanizeIdentifier(trigger.id) }
  }
}

function TriggerList({ triggers }: { triggers: readonly AgentTriggerRefDto[] }) {
  return (
    <ul className="space-y-1">
      {triggers.map((trigger, idx) => {
        const info = triggerInfo(trigger)
        const Icon = info.Icon
        return (
          <li
            key={`${trigger.kind}:${idx}`}
            className="flex items-center gap-2 rounded-md border border-border/40 bg-background/40 px-2 py-1.5"
          >
            <Icon className="h-3 w-3 shrink-0 text-muted-foreground/80" aria-hidden="true" />
            <span className="text-[9px] font-semibold uppercase tracking-[0.1em] text-muted-foreground/75">
              {info.kind}
            </span>
            <span className="ml-auto truncate text-[11.5px] text-foreground/90">{info.body}</span>
          </li>
        )
      })}
    </ul>
  )
}

// ──────────────────────────────────────────────────────────────────────────
// Per-node detail views
// ──────────────────────────────────────────────────────────────────────────

function AgentHeaderDetails({ data }: { data: AgentHeaderNodeData }) {
  const { header, summary } = data
  return (
    <>
      {header.description ? (
        <Section label="Description">
          <Prose>{header.description}</Prose>
        </Section>
      ) : null}
      {header.taskPurpose ? (
        <Section label="Task purpose">
          <Prose>{header.taskPurpose}</Prose>
        </Section>
      ) : null}
      <Section label="Runtime">
        <MetaRow
          items={[
            { label: 'Scope', value: getAgentDefinitionScopeLabel(header.scope) },
            {
              label: 'Profile',
              value: getAgentDefinitionBaseCapabilityLabel(header.baseCapabilityProfile),
            },
            { label: 'Lifecycle', value: header.lifecycleState },
            {
              label: 'Approval',
              value: getRuntimeRunApprovalModeLabel(header.defaultApprovalMode),
            },
          ]}
        />
      </Section>
      <Section label="Gates">
        <ChipRow>
          <GatePill on={header.allowPlanGate} label="Plan" />
          <GatePill on={header.allowVerificationGate} label="Verify" />
          {header.allowAutoCompact ? <GatePill on label="Auto-compact" /> : null}
        </ChipRow>
      </Section>
      <Section label="Composition">
        <StatStrip
          items={[
            { label: 'Prompts', value: summary.prompts },
            { label: 'Tools', value: summary.tools },
            { label: 'Touchpoints', value: summary.dbTables },
            { label: 'Sections', value: summary.outputSections },
            ...(summary.consumes > 0
              ? [{ label: 'Consumes', value: summary.consumes }]
              : []),
          ]}
        />
      </Section>
    </>
  )
}

function PromptDetails({ data }: { data: PromptNodeData }) {
  const { prompt } = data
  const tokenEstimate = Math.ceil((prompt.body ?? '').length / 4)
  return (
    <>
      <Section label="Metadata">
        <MetaRow
          items={[
            { label: 'Role', value: prompt.role },
            { label: 'Source', value: humanizeIdentifier(prompt.source) },
          ]}
        />
      </Section>
      {prompt.policy ? (
        <Section label="Policy">
          <Chip>{humanizeIdentifier(prompt.policy)}</Chip>
        </Section>
      ) : null}
      <Section label={`Body · ~${tokenEstimate} tokens`}>
        <Body>{prompt.body}</Body>
      </Section>
    </>
  )
}

function ToolDetails({ data }: { data: ToolNodeData }) {
  const { tool } = data
  return (
    <>
      {tool.description ? (
        <Section label="Description">
          <Prose>{tool.description}</Prose>
        </Section>
      ) : null}
      <Section label="Classification">
        <MetaRow
          items={[
            { label: 'Group', value: humanizeIdentifier(tool.group) },
            { label: 'Effect', value: tool.effectClass },
            ...(tool.riskClass ? [{ label: 'Risk', value: tool.riskClass }] : []),
          ]}
        />
      </Section>
      {tool.tags.length > 0 ? (
        <Section label="Tags" count={tool.tags.length}>
          <ChipRow>
            {tool.tags.map((tag) => (
              <Chip key={tag} mono>
                {tag}
              </Chip>
            ))}
          </ChipRow>
        </Section>
      ) : null}
    </>
  )
}

function DbTableDetails({ data }: { data: DbTableNodeData }) {
  const { table, touchpoint, purpose, columns, triggers } = data
  return (
    <>
      <div className="flex items-center justify-between gap-3 rounded-md border border-border/50 bg-muted/40 px-3 py-2">
        <div className="min-w-0">
          <p className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
            Table
          </p>
          <p className="truncate font-mono text-[12px] font-medium text-foreground/95">
            {table ? humanizeIdentifier(table) : '—'}
          </p>
        </div>
        <TouchpointPill kind={touchpoint} />
      </div>
      {purpose ? (
        <Section label="Purpose">
          <Prose>{purpose}</Prose>
        </Section>
      ) : null}
      {triggers.length > 0 ? (
        <Section label="Triggers" count={triggers.length}>
          <TriggerList triggers={triggers} />
        </Section>
      ) : null}
      {columns.length > 0 ? (
        <Section label="Columns" count={columns.length}>
          <ChipRow>
            {columns.map((column) => (
              <Chip key={column} mono>
                {column}
              </Chip>
            ))}
          </ChipRow>
        </Section>
      ) : null}
    </>
  )
}

function OutputDetails({ data }: { data: OutputNodeData }) {
  const { output } = data
  return (
    <>
      <Section label="Metadata">
        <MetaRow
          items={[
            { label: 'Label', value: output.label },
            { label: 'Contract', value: humanizeIdentifier(output.contract) },
          ]}
        />
      </Section>
      {output.description ? (
        <Section label="Description">
          <Prose>{output.description}</Prose>
        </Section>
      ) : null}
    </>
  )
}

function OutputSectionDetails({ data }: { data: OutputSectionNodeData }) {
  const { section } = data
  return (
    <>
      <Section label="Metadata">
        <MetaRow
          items={[
            { label: 'Section ID', value: section.id },
            { label: 'Emphasis', value: section.emphasis },
          ]}
        />
      </Section>
      {section.description ? (
        <Section label="Description">
          <Prose>{section.description}</Prose>
        </Section>
      ) : null}
      {section.producedByTools.length > 0 ? (
        <Section label="Produced by" count={section.producedByTools.length}>
          <ChipRow>
            {section.producedByTools.map((tool) => (
              <Chip key={tool} mono>
                {humanizeIdentifier(tool)}
              </Chip>
            ))}
          </ChipRow>
        </Section>
      ) : null}
    </>
  )
}

function ConsumedArtifactDetails({ data }: { data: ConsumedArtifactNodeData }) {
  const { artifact } = data
  return (
    <>
      <div className="flex items-center justify-between gap-3 rounded-md border border-border/50 bg-muted/40 px-3 py-2">
        <div className="min-w-0">
          <p className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
            From
          </p>
          <p className="truncate text-[12px] font-medium text-foreground/95">
            {humanizeIdentifier(artifact.sourceAgent)}
          </p>
        </div>
        <span
          className={cn(
            'inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium leading-none',
            artifact.required
              ? 'border-primary/30 bg-primary/10 text-primary'
              : 'border-border/45 bg-muted/30 text-muted-foreground/80',
          )}
        >
          <ArrowDownLeft className="h-3 w-3" />
          {artifact.required ? 'Required' : 'Optional'}
        </span>
      </div>
      <Section label="Metadata">
        <MetaRow
          items={[
            { label: 'Contract', value: humanizeIdentifier(artifact.contract) },
            { label: 'Source agent', value: humanizeIdentifier(artifact.sourceAgent) },
          ]}
        />
      </Section>
      {artifact.description ? (
        <Section label="Description">
          <Prose>{artifact.description}</Prose>
        </Section>
      ) : null}
      {artifact.sections.length > 0 ? (
        <Section label="Sections used" count={artifact.sections.length}>
          <ChipRow>
            {artifact.sections.map((section) => (
              <Chip key={section} mono>
                {humanizeIdentifier(section)}
              </Chip>
            ))}
          </ChipRow>
        </Section>
      ) : null}
    </>
  )
}

