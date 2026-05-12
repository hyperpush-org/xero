'use client'

import { useMemo, useState } from 'react'
import {
  Bot,
  ChevronDown,
  ChevronRight,
  Database,
  FileText,
  Flag,
  GitBranch,
  GitMerge,
  Info,
  Layers,
  ListChecks,
  Lock,
  Plus,
  RefreshCcw,
  Sparkles,
  Target,
  Trash2,
  Wrench,
  X,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Textarea } from '@/components/ui/textarea'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import {
  agentDefinitionBaseCapabilityProfileSchema,
  agentDefinitionScopeSchema,
  getAgentDefinitionBaseCapabilityLabel,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionValidationDiagnosticDto,
  type CustomAgentWorkflowBranchConditionDto,
  type CustomAgentWorkflowBranchDto,
  type CustomAgentWorkflowGateDto,
  type CustomAgentWorkflowPhaseDto,
} from '@/src/lib/xero-model/agent-definition'
import {
  getRuntimeRunApprovalModeLabel,
  type RuntimeRunApprovalModeDto,
} from '@/src/lib/xero-model/runtime'
import type {
  AgentAttachedSkillDto,
  AgentAuthoringAttachableSkillDto,
  AgentDbTouchpointKindDto,
  AgentOutputSectionEmphasisDto,
  AgentPromptRoleDto,
  AgentToolEffectClassDto,
  AgentToolPackManifestDto,
  RuntimeAgentOutputContractDto,
} from '@/src/lib/xero-model/workflow-agents'

import type {
  AgentGraphNode,
  AgentHeaderAdvancedFields,
  AgentHeaderNodeData,
  AgentInferredAdvanced,
  ConsumedArtifactNodeData,
  DbTableNodeData,
  OutputNodeData,
  OutputSectionNodeData,
  PromptNodeData,
  SkillNodeData,
  StageNodeData,
  ToolNodeData,
} from './build-agent-graph'
import { humanizeIdentifier, humanizeToolGroupKey } from './build-agent-graph'
import { useCanvasMode } from './canvas-mode-context'
import { CatalogPicker, type CatalogPickerOption } from './nodes/catalog-picker'

const EDITABLE_PROFILES: AgentDefinitionBaseCapabilityProfileDto[] = [
  'observe_only',
  'planning',
  'repository_recon',
  'engineering',
  'debugging',
  'agent_builder',
]
const APPROVAL_MODES: RuntimeRunApprovalModeDto[] = ['suggest', 'auto_edit', 'yolo']
const TOOL_GROUP_OPTIONS = [
  'core',
  'harness_runner',
  'engineering',
  'browser_control',
  'external_service',
  'skill_runtime',
] as const
const ROLE_OPTIONS: AgentPromptRoleDto[] = ['system', 'developer', 'task']
const ROLE_LABEL: Record<AgentPromptRoleDto, string> = {
  system: 'System',
  developer: 'Developer',
  task: 'Task',
}
const OUTPUT_CONTRACTS: RuntimeAgentOutputContractDto[] = [
  'answer',
  'plan_pack',
  'crawl_report',
  'engineering_summary',
  'debug_summary',
  'agent_definition_draft',
]
const EMPHASIS_OPTIONS: AgentOutputSectionEmphasisDto[] = ['core', 'standard', 'optional']
const TOUCHPOINT_OPTIONS: AgentDbTouchpointKindDto[] = ['read', 'write', 'encouraged']

// Effect classes the granular policy editor exposes for opt-in. Order
// matches the AgentToolEffectClassDto enum so the column reads stably as
// the user scans down. Each effect class lines up with a validator code
// (`agent_definition_effect_class_*`).
const EFFECT_CLASS_OPTIONS: { value: AgentToolEffectClassDto; label: string }[] = [
  { value: 'observe', label: 'Observe' },
  { value: 'runtime_state', label: 'Runtime state' },
  { value: 'write', label: 'Write' },
  { value: 'destructive_write', label: 'Destructive write' },
  { value: 'command', label: 'Command' },
  { value: 'process_control', label: 'Process control' },
  { value: 'browser_control', label: 'Browser control' },
  { value: 'device_control', label: 'Device control' },
  { value: 'external_service', label: 'External service' },
  { value: 'skill_runtime', label: 'Skill runtime' },
  { value: 'agent_delegation', label: 'Agent delegation' },
]

// Subagent roles a definition may dispatch when `subagentAllowed` is on.
// Sourced from `customAgentSubagentRoleSchema` — kept inline so the
// component can iterate without depending on the schema enum at runtime.
const SUBAGENT_ROLE_OPTIONS = [
  'engineer',
  'debugger',
  'planner',
  'researcher',
  'reviewer',
  'agent_builder',
  'browser',
  'emulator',
  'solana',
  'database',
] as const
type SubagentRoleOption = (typeof SUBAGENT_ROLE_OPTIONS)[number]

type CapabilityFlagKey = keyof AgentInferredAdvanced['flags']

const CAPABILITY_FLAGS: ReadonlyArray<{ key: CapabilityFlagKey; label: string }> = [
  { key: 'externalServiceAllowed', label: 'External service' },
  { key: 'browserControlAllowed', label: 'Browser control' },
  { key: 'skillRuntimeAllowed', label: 'Skill runtime' },
  { key: 'subagentAllowed', label: 'Subagent delegation' },
  { key: 'commandAllowed', label: 'Shell commands' },
  { key: 'destructiveWriteAllowed', label: 'Destructive writes' },
]

interface NodePropertiesPanelProps {
  selectedNode: AgentGraphNode | null
  onClose: () => void
}

export function NodePropertiesPanel({ selectedNode, onClose }: NodePropertiesPanelProps) {
  if (!selectedNode) return null
  if (selectedNode.type === 'lane-label' || selectedNode.type === 'tool-group-frame') return null

  const meta = panelMetaForNode(selectedNode)

  return (
    <TooltipProvider delayDuration={150}>
      <div
        className="agent-properties-panel pointer-events-auto absolute left-2.5 top-14 z-30 flex max-h-[calc(100%-4.5rem)] w-[272px] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[10.5px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
        onPointerDown={(event) => event.stopPropagation()}
        onWheel={(event) => event.stopPropagation()}
      >
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
            className="min-w-0 flex-1 truncate text-[11.5px] font-semibold leading-none text-foreground"
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
            aria-label="Close properties"
          >
            <X className="h-3 w-3" />
          </Button>
        </header>
        <div className="min-h-0 overflow-y-auto px-3 py-3">{renderEditor(selectedNode)}</div>
      </div>
    </TooltipProvider>
  )
}

function renderEditor(node: AgentGraphNode): React.ReactNode {
  switch (node.type) {
    case 'agent-header':
      return <AgentHeaderEditor nodeId={node.id} data={node.data as AgentHeaderNodeData} />
    case 'prompt':
      return <PromptEditor nodeId={node.id} data={node.data as PromptNodeData} />
    case 'skills':
      return <SkillEditor nodeId={node.id} data={node.data as SkillNodeData} />
    case 'tool':
      return <ToolEditor nodeId={node.id} data={node.data as ToolNodeData} />
    case 'db-table':
      return <DbTableEditor nodeId={node.id} data={node.data as DbTableNodeData} />
    case 'agent-output':
      return <OutputEditor nodeId={node.id} data={node.data as OutputNodeData} />
    case 'output-section':
      return <OutputSectionEditor nodeId={node.id} data={node.data as OutputSectionNodeData} />
    case 'consumed-artifact':
      return (
        <ConsumedArtifactEditor nodeId={node.id} data={node.data as ConsumedArtifactNodeData} />
      )
    case 'stage':
      return <StageEditor nodeId={node.id} data={node.data as StageNodeData} />
    default:
      return null
  }
}

export interface PanelMeta {
  title: string
  subtitle: string
  Icon: typeof Bot
  iconWrap: string
}

export function panelMetaForNode(node: AgentGraphNode): PanelMeta {
  switch (node.type) {
    case 'agent-header': {
      const data = node.data as AgentHeaderNodeData
      return {
        title: data.header.displayName || 'Agent',
        subtitle: 'Agent header',
        Icon: Bot,
        iconWrap: 'bg-primary/15 text-primary ring-1 ring-primary/30',
      }
    }
    case 'prompt': {
      const data = node.data as PromptNodeData
      return {
        title: data.prompt.label || 'Prompt',
        subtitle: 'Prompt',
        Icon: FileText,
        iconWrap: 'bg-amber-500/15 text-amber-500 ring-1 ring-amber-500/30',
      }
    }
    case 'skills': {
      const data = node.data as SkillNodeData
      return {
        title: data.skill.name || 'Skill',
        subtitle: 'Attached skill',
        Icon: Sparkles,
        iconWrap: 'bg-violet-500/15 text-violet-500 ring-1 ring-violet-500/30',
      }
    }
    case 'tool': {
      const data = node.data as ToolNodeData
      return {
        title: data.tool.name ? humanizeIdentifier(data.tool.name) : 'Tool',
        subtitle: 'Tool',
        Icon: Wrench,
        iconWrap: 'bg-sky-500/15 text-sky-500 ring-1 ring-sky-500/30',
      }
    }
    case 'db-table': {
      const data = node.data as DbTableNodeData
      return {
        title: data.table ? humanizeIdentifier(data.table) : 'Database table',
        subtitle: 'Database touchpoint',
        Icon: Database,
        iconWrap: 'bg-emerald-500/15 text-emerald-500 ring-1 ring-emerald-500/30',
      }
    }
    case 'agent-output': {
      const data = node.data as OutputNodeData
      return {
        title: data.output.label || 'Final response',
        subtitle: 'Output',
        Icon: Target,
        iconWrap: 'bg-foreground/10 text-foreground/80 ring-1 ring-foreground/30',
      }
    }
    case 'output-section': {
      const data = node.data as OutputSectionNodeData
      return {
        title: data.section.label || 'Section',
        subtitle: 'Output section',
        Icon: Layers,
        iconWrap: 'bg-foreground/10 text-foreground/70 ring-1 ring-foreground/30',
      }
    }
    case 'consumed-artifact': {
      const data = node.data as ConsumedArtifactNodeData
      return {
        title: data.artifact.label || 'Upstream artifact',
        subtitle: 'Consumed artifact',
        Icon: GitMerge,
        iconWrap: 'bg-teal-500/15 text-teal-500 ring-1 ring-teal-500/30',
      }
    }
    case 'stage': {
      const data = node.data as StageNodeData
      return {
        title: data.phase.title || humanizeIdentifier(data.phase.id),
        subtitle: 'Stage',
        Icon: GitBranch,
        iconWrap: 'bg-amber-500/15 text-amber-500 ring-1 ring-amber-500/30',
      }
    }
    default:
      return {
        title: 'Node',
        subtitle: '',
        Icon: Bot,
        iconWrap: 'bg-muted text-muted-foreground',
      }
  }
}

function FieldGroup({
  label,
  hint,
  children,
  className,
}: {
  label?: string
  hint?: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={cn('space-y-1.5', className)}>
      {label ? (
        <div className="flex items-baseline justify-between gap-2">
          <Label className="text-[9.5px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/85">
            {label}
          </Label>
          {hint ? (
            <span className="text-[8.5px] font-normal lowercase tracking-wide text-muted-foreground/60">
              {hint}
            </span>
          ) : null}
        </div>
      ) : null}
      {children}
    </div>
  )
}

function SectionHeading({
  icon: Icon,
  label,
  hint,
}: {
  icon?: React.ElementType
  label: string
  hint?: string
}) {
  return (
    <div className="mb-2 flex items-baseline gap-2">
      {Icon ? <Icon className="h-3 w-3 self-center text-muted-foreground/70" /> : null}
      <span className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
        {label}
      </span>
      {hint ? (
        <span className="text-[8.5px] font-normal normal-case text-muted-foreground/55">
          {hint}
        </span>
      ) : null}
    </div>
  )
}

function PanelSelect<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
  placeholder,
}: {
  value: T
  onChange: (value: T) => void
  options: { value: T; label: string }[]
  ariaLabel?: string
  placeholder?: string
}) {
  return (
    <Select value={value} onValueChange={(next) => onChange(next as T)}>
      <SelectTrigger
        size="sm"
        aria-label={ariaLabel}
        className="h-8 w-full border-border/60 bg-background/60 text-[10px] font-medium text-foreground/90 shadow-none data-[size=sm]:h-8"
      >
        <SelectValue placeholder={placeholder ?? 'Select…'} />
      </SelectTrigger>
      <SelectContent className="text-[10px]">
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value} className="text-[10px]">
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function RemoveRow({ onRemove, label }: { onRemove: () => void; label: string }) {
  return (
    <div className="pt-1">
      <Separator className="mb-2 opacity-60" />
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onClick={onRemove}
        className="h-7 w-full justify-start gap-1.5 px-2 text-[10px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
      >
        <Trash2 className="h-3 w-3" />
        {label}
      </Button>
    </div>
  )
}

function CheckboxToggle({
  checked,
  onChange,
  label,
  disabled,
  locked,
  monoLabel,
  reasonTooltip,
}: {
  checked: boolean
  onChange: (next: boolean) => void
  label: string
  disabled?: boolean
  locked?: boolean
  monoLabel?: boolean
  reasonTooltip?: string
}) {
  const id = useMemo(
    () => `cb-${label.replace(/\W+/g, '-').toLowerCase()}-${Math.random().toString(36).slice(2, 7)}`,
    [label],
  )
  const isLocked = !!locked
  const row = (
    <label
      htmlFor={id}
      className={cn(
        'group flex items-center gap-2 rounded-md border border-border/55 bg-background/40 px-2 py-1.5 text-[10px] transition-colors',
        checked && !isLocked && 'border-primary/40 bg-primary/8',
        isLocked && 'border-primary/45 bg-primary/12 text-foreground',
        !checked && !isLocked && 'hover:border-border hover:bg-background/70',
        disabled && !isLocked && 'cursor-not-allowed opacity-60',
        !disabled && 'cursor-pointer',
      )}
    >
      <Checkbox
        id={id}
        checked={checked}
        disabled={disabled || isLocked}
        onCheckedChange={(value) => onChange(value === true)}
        className="size-3.5"
      />
      <span
        className={cn(
          'min-w-0 flex-1 truncate',
          monoLabel && 'font-mono text-[9.5px] tracking-tight',
          isLocked && 'text-foreground/95',
        )}
      >
        {label}
      </span>
      {isLocked ? (
        <Lock className="h-2.5 w-2.5 shrink-0 text-primary/70" aria-hidden />
      ) : null}
    </label>
  )
  if (!reasonTooltip) return row
  return (
    <Tooltip>
      <TooltipTrigger asChild>{row}</TooltipTrigger>
      <TooltipContent side="top" className="max-w-[220px] text-[9.5px]">
        {reasonTooltip}
      </TooltipContent>
    </Tooltip>
  )
}

function InferenceBadge() {
  return (
    <span className="inline-flex items-center gap-1 rounded-md border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[8px] font-semibold uppercase tracking-wider text-primary">
      <Sparkles className="h-2.5 w-2.5" />
      Auto
    </span>
  )
}

function AgentHeaderEditor({ nodeId, data }: { nodeId: string; data: AgentHeaderNodeData }) {
  const {
    updateNodeData,
    inferredAdvanced,
    authoringCatalog,
    toolPackCatalog,
    policyDiagnostics,
    policyDiagnosticsLoading,
  } = useCanvasMode()
  const { header, advanced } = data

  const updateHeader = (next: Partial<AgentHeaderNodeData['header']>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as AgentHeaderNodeData
      return { ...cur, header: { ...cur.header, ...next } }
    })

  const updateAdvanced = (next: Partial<AgentHeaderAdvancedFields>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as AgentHeaderNodeData
      return { ...cur, advanced: { ...cur.advanced, ...next } }
    })

  const ensureLength = (values: readonly string[], min: number): string[] => {
    const next = [...values]
    while (next.length < min) next.push('')
    return next
  }
  const examplePrompts = ensureLength(advanced.examplePrompts, 3)
  const refusalCases = ensureLength(advanced.refusalEscalationCases, 3)

  const inferredGroupSet = useMemo(
    () => new Set(inferredAdvanced.toolGroups),
    [inferredAdvanced.toolGroups],
  )
  const visibleToolGroups = useMemo(() => {
    const seen = new Set<string>()
    const ordered: string[] = []
    for (const group of TOOL_GROUP_OPTIONS) {
      if (!seen.has(group)) {
        seen.add(group)
        ordered.push(group)
      }
    }
    for (const group of inferredAdvanced.toolGroups) {
      if (!seen.has(group)) {
        seen.add(group)
        ordered.push(group)
      }
    }
    return ordered
  }, [inferredAdvanced.toolGroups])

  const toggleApproval = (mode: RuntimeRunApprovalModeDto, next: boolean) => {
    if (mode === 'suggest') return
    const set = new Set<RuntimeRunApprovalModeDto>(header.allowedApprovalModes)
    if (next) set.add(mode)
    else set.delete(mode)
    set.add('suggest')
    updateHeader({ allowedApprovalModes: Array.from(set) })
  }

  const toggleGroup = (group: string, next: boolean) => {
    if (inferredGroupSet.has(group)) return
    const set = new Set(advanced.allowedToolGroups)
    if (next) set.add(group)
    else set.delete(group)
    updateAdvanced({ allowedToolGroups: Array.from(set) })
  }

  const formatReasons = (sources: readonly string[]): string => {
    const display = sources
      .filter((src) => !src.startsWith('db:'))
      .map((src) => humanizeIdentifier(src))
    const dbSources = sources
      .filter((src) => src.startsWith('db:'))
      .map((src) => humanizeIdentifier(src.slice(3)))
    const parts: string[] = []
    if (display.length > 0) {
      parts.push(`Required by tool${display.length > 1 ? 's' : ''}: ${display.join(', ')}`)
    }
    if (dbSources.length > 0) {
      parts.push(`Required by DB write${dbSources.length > 1 ? 's' : ''}: ${dbSources.join(', ')}`)
    }
    return parts.join(' • ')
  }

  return (
    <div className="space-y-4">
      <div className="space-y-2.5">
        <FieldGroup label="Display name">
          <Input
            value={header.displayName}
            onChange={(event) => updateHeader({ displayName: event.target.value })}
            placeholder="Display name"
            maxLength={80}
            className="h-8 text-[10.75px] font-semibold"
          />
        </FieldGroup>
        <FieldGroup label="Short label" hint={`${header.shortLabel.length}/24`}>
          <Input
            value={header.shortLabel}
            onChange={(event) => updateHeader({ shortLabel: event.target.value })}
            placeholder="Short label"
            maxLength={24}
            className="h-8 text-[10px]"
          />
        </FieldGroup>
        <FieldGroup label="Description">
          <Textarea
            value={header.description}
            onChange={(event) => updateHeader({ description: event.target.value })}
            placeholder="What does this agent do?"
            rows={2}
            className="resize-none text-[10px]"
          />
        </FieldGroup>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading label="Runtime" />
        <div className="grid grid-cols-2 gap-2">
          <FieldGroup label="Visibility">
            <PanelSelect
              value={header.scope === 'built_in' ? 'project_custom' : header.scope}
              onChange={(value) => {
                const parsed = agentDefinitionScopeSchema.safeParse(value)
                if (
                  parsed.success &&
                  (parsed.data === 'project_custom' || parsed.data === 'global_custom')
                ) {
                  updateHeader({ scope: parsed.data })
                }
              }}
              options={[
                { value: 'project_custom', label: 'Project' },
                { value: 'global_custom', label: 'Global' },
              ]}
            />
          </FieldGroup>
          <FieldGroup label="Approval">
            <PanelSelect
              value={header.defaultApprovalMode}
              onChange={(value) =>
                updateHeader({ defaultApprovalMode: value as RuntimeRunApprovalModeDto })
              }
              options={APPROVAL_MODES.map((mode) => ({
                value: mode,
                label: getRuntimeRunApprovalModeLabel(mode),
              }))}
            />
          </FieldGroup>
        </div>
        <FieldGroup label="Capability">
          <PanelSelect
            value={header.baseCapabilityProfile}
            onChange={(value) => {
              const parsed = agentDefinitionBaseCapabilityProfileSchema.safeParse(value)
              if (parsed.success && EDITABLE_PROFILES.includes(parsed.data)) {
                updateHeader({ baseCapabilityProfile: parsed.data })
              }
            }}
            options={EDITABLE_PROFILES.map((profile) => ({
              value: profile,
              label: getAgentDefinitionBaseCapabilityLabel(profile),
            }))}
          />
        </FieldGroup>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading label="Behavior contracts" />
        <FieldGroup label="Task purpose">
          <Textarea
            value={header.taskPurpose}
            onChange={(event) => updateHeader({ taskPurpose: event.target.value })}
            placeholder="Describe the agent's primary task and constraints."
            rows={3}
            className="resize-none text-[10px]"
          />
        </FieldGroup>
        <FieldGroup label="Workflow contract">
          <Textarea
            value={advanced.workflowContract}
            onChange={(event) => updateAdvanced({ workflowContract: event.target.value })}
            placeholder="What end-to-end workflow does this agent run?"
            rows={2}
            className="resize-none text-[10px]"
          />
        </FieldGroup>
        <FieldGroup label="Final response contract">
          <Textarea
            value={advanced.finalResponseContract}
            onChange={(event) => updateAdvanced({ finalResponseContract: event.target.value })}
            placeholder="What does a successful final response include?"
            rows={2}
            className="resize-none text-[10px]"
          />
        </FieldGroup>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading label="Example prompts" hint="≥ 3" />
        <div className="space-y-1.5">
          {examplePrompts.map((value, index) => (
            <Input
              key={index}
              value={value}
              onChange={(event) => {
                const next = [...examplePrompts]
                next[index] = event.target.value
                updateAdvanced({ examplePrompts: next })
              }}
              placeholder={`Example #${index + 1}`}
              className="h-8 text-[10px]"
            />
          ))}
        </div>
      </div>

      <div className="space-y-2.5">
        <SectionHeading label="Refusal escalation cases" hint="≥ 3" />
        <div className="space-y-1.5">
          {refusalCases.map((value, index) => (
            <Input
              key={index}
              value={value}
              onChange={(event) => {
                const next = [...refusalCases]
                next[index] = event.target.value
                updateAdvanced({ refusalEscalationCases: next })
              }}
              placeholder={`Case #${index + 1}`}
              className="h-8 text-[10px]"
            />
          ))}
        </div>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading label="Allowed approval modes" />
        </div>
        <div className="flex flex-col gap-1">
          {APPROVAL_MODES.map((mode) => {
            const checked = header.allowedApprovalModes.includes(mode)
            const lockedOn = mode === 'suggest'
            return (
              <CheckboxToggle
                key={mode}
                label={getRuntimeRunApprovalModeLabel(mode)}
                checked={checked}
                locked={lockedOn}
                reasonTooltip={lockedOn ? 'Suggest mode is always allowed.' : undefined}
                onChange={(next) => toggleApproval(mode, next)}
              />
            )
          })}
        </div>
      </div>

      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading label="Allowed tool groups" />
          {inferredAdvanced.toolGroups.length > 0 ? <InferenceBadge /> : null}
        </div>
        {inferredAdvanced.toolGroups.length > 0 ? (
          <p className="-mt-1 text-[9.5px] leading-snug text-muted-foreground/75">
            Auto-checked groups come from tools you've added to the canvas. Add more tools to grow
            the set.
          </p>
        ) : (
          <p className="-mt-1 text-[9.5px] leading-snug text-muted-foreground/75">
            Drag tools onto the canvas to auto-fill required groups, or pick extras here.
          </p>
        )}
        <div className="flex flex-col gap-1">
          {visibleToolGroups.map((group) => {
            const inferredHere = inferredGroupSet.has(group)
            const userPicked = advanced.allowedToolGroups.includes(group)
            const checked = inferredHere || userPicked
            const reasons = inferredHere
              ? formatReasons(inferredAdvanced.toolGroupReasons[group] ?? [])
              : undefined
            return (
              <CheckboxToggle
                key={group}
                label={humanizeToolGroupKey(group)}
                checked={checked}
                locked={inferredHere}
                monoLabel
                reasonTooltip={reasons}
                onChange={(next) => toggleGroup(group, next)}
              />
            )
          })}
        </div>
      </div>

      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading label="Capability flags" />
          {Object.values(inferredAdvanced.flags).some(Boolean) ? <InferenceBadge /> : null}
        </div>
        <div className="flex flex-col gap-1">
          {CAPABILITY_FLAGS.map(({ key, label }) => {
            const inferredHere = inferredAdvanced.flags[key]
            const userPicked = advanced[key]
            const checked = inferredHere || userPicked
            const reasons = inferredHere
              ? formatReasons(inferredAdvanced.flagReasons[key] ?? [])
              : undefined
            return (
              <CheckboxToggle
                key={key}
                label={label}
                checked={checked}
                locked={inferredHere}
                reasonTooltip={reasons}
                onChange={(next) => {
                  if (inferredHere) return
                  updateAdvanced({ [key]: next } as Partial<AgentHeaderAdvancedFields>)
                }}
              />
            )
          })}
        </div>
      </div>

      <Separator className="opacity-60" />

      <GranularPolicyEditor
        advanced={advanced}
        updateAdvanced={updateAdvanced}
        authoringCatalog={authoringCatalog}
        toolPackCatalog={toolPackCatalog}
        inferredEffectClasses={inferredAdvanced.effectClasses}
        subagentInferred={inferredAdvanced.flags.subagentAllowed}
        diagnostics={policyDiagnostics}
        diagnosticsLoading={policyDiagnosticsLoading}
      />
    </div>
  )
}

interface GranularPolicyEditorProps {
  advanced: AgentHeaderAdvancedFields
  updateAdvanced: (next: Partial<AgentHeaderAdvancedFields>) => void
  authoringCatalog: ReturnType<typeof useCanvasMode>['authoringCatalog']
  toolPackCatalog: ReturnType<typeof useCanvasMode>['toolPackCatalog']
  inferredEffectClasses: readonly string[]
  subagentInferred: boolean
  diagnostics: readonly AgentDefinitionValidationDiagnosticDto[]
  diagnosticsLoading: boolean
}

function GranularPolicyEditor({
  advanced,
  updateAdvanced,
  authoringCatalog,
  toolPackCatalog,
  inferredEffectClasses,
  subagentInferred,
  diagnostics,
  diagnosticsLoading,
}: GranularPolicyEditorProps) {
  const inferredEffectClassSet = useMemo(
    () => new Set(inferredEffectClasses),
    [inferredEffectClasses],
  )

  const toolOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    if (!authoringCatalog) return []
    return authoringCatalog.tools.map((entry) => ({
      value: entry.name,
      label: humanizeIdentifier(entry.name),
      description: entry.description,
      meta: entry.effectClass,
      group: entry.group,
      keywords: [entry.name, entry.group, entry.riskClass, ...entry.tags],
    }))
  }, [authoringCatalog])

  const packManifestsById = useMemo(() => {
    const map = new Map<string, AgentToolPackManifestDto>()
    for (const manifest of toolPackCatalog?.toolPacks ?? []) {
      map.set(manifest.packId, manifest)
    }
    return map
  }, [toolPackCatalog])

  const packOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    if (!toolPackCatalog) return []
    return toolPackCatalog.toolPacks.map((manifest) => ({
      value: manifest.packId,
      label: manifest.label,
      description: manifest.summary,
      meta: `${manifest.tools.length} tool${manifest.tools.length === 1 ? '' : 's'}`,
      group: manifest.policyProfile,
      keywords: [
        manifest.packId,
        manifest.policyProfile,
        ...manifest.capabilities,
        ...manifest.tools,
      ],
    }))
  }, [toolPackCatalog])

  const diagnosticsByPathPrefix = useMemo(() => {
    const grouped: Record<string, AgentDefinitionValidationDiagnosticDto[]> = {
      'toolPolicy.allowedEffectClasses': [],
      'toolPolicy.deniedTools': [],
      'toolPolicy.allowedTools': [],
      'toolPolicy.allowedToolPacks': [],
      'toolPolicy.deniedToolPacks': [],
      'toolPolicy.allowedSubagentRoles': [],
      'toolPolicy.deniedSubagentRoles': [],
    }
    for (const diagnostic of diagnostics) {
      const path = diagnostic.path
      for (const prefix of Object.keys(grouped)) {
        if (path === prefix || path.startsWith(`${prefix}.`)) {
          grouped[prefix].push(diagnostic)
        }
      }
    }
    return grouped
  }, [diagnostics])

  const addEntry = (
    field:
      | 'allowedEffectClasses'
      | 'deniedTools'
      | 'allowedToolPacks'
      | 'deniedToolPacks'
      | 'allowedSubagentRoles'
      | 'deniedSubagentRoles',
    value: string,
  ) => {
    const current = advanced[field]
    if (current.includes(value)) return
    updateAdvanced({ [field]: [...current, value] } as Partial<AgentHeaderAdvancedFields>)
  }

  const removeEntry = (
    field:
      | 'allowedEffectClasses'
      | 'deniedTools'
      | 'allowedToolPacks'
      | 'deniedToolPacks'
      | 'allowedSubagentRoles'
      | 'deniedSubagentRoles',
    value: string,
  ) => {
    updateAdvanced({
      [field]: advanced[field].filter((entry) => entry !== value),
    } as Partial<AgentHeaderAdvancedFields>)
  }

  const allowedEffectClassSelectable = EFFECT_CLASS_OPTIONS.filter(
    (option) =>
      !inferredEffectClassSet.has(option.value) &&
      !advanced.allowedEffectClasses.includes(option.value),
  )

  // Expanded set of tools granted by every currently-allowed tool pack —
  // surfaced as a read-only chip list so the user can see what a pack
  // grants without leaving the panel. This is purely informative; runtime
  // resolution still happens server-side via previewAgentDefinition.
  const packGrantedTools = useMemo(() => {
    const tools = new Set<string>()
    for (const packId of advanced.allowedToolPacks) {
      const manifest = packManifestsById.get(packId)
      if (!manifest) continue
      for (const tool of manifest.tools) tools.add(tool)
    }
    return Array.from(tools).sort((a, b) => a.localeCompare(b))
  }, [advanced.allowedToolPacks, packManifestsById])

  // Tool packs that the user listed in both the allow- and deny-set. This
  // is a structural conflict we can detect entirely on the client; the
  // validator would also flag it, but rendering the warning immediately
  // makes the picker self-explaining.
  const conflictingPackIds = useMemo(
    () => advanced.allowedToolPacks.filter((id) => advanced.deniedToolPacks.includes(id)),
    [advanced.allowedToolPacks, advanced.deniedToolPacks],
  )
  const conflictingRoles = useMemo(
    () => advanced.allowedSubagentRoles.filter((role) => advanced.deniedSubagentRoles.includes(role)),
    [advanced.allowedSubagentRoles, advanced.deniedSubagentRoles],
  )

  const subagentEffective = subagentInferred || advanced.subagentAllowed

  return (
    <div className="space-y-4">
      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading
            label="Tool policy"
            hint={diagnosticsLoading ? 'Previewing…' : undefined}
          />
        </div>
        <p className="-mt-1 text-[9.5px] leading-snug text-muted-foreground/75">
          Narrow the runtime's effective tool access. Canvas tools and inferred
          effect classes always stay allowed; everything below is on top.
        </p>
      </div>

      <PolicyChipList
        label="Effect classes"
        hint="Allow extra"
        entries={advanced.allowedEffectClasses}
        inferred={inferredEffectClasses}
        formatLabel={(value) =>
          EFFECT_CLASS_OPTIONS.find((option) => option.value === value)?.label ??
          humanizeIdentifier(value)
        }
        onRemove={(value) => removeEntry('allowedEffectClasses', value)}
        emptyMessage="No extra effect classes — only those inferred from canvas tools."
        diagnostics={diagnosticsByPathPrefix['toolPolicy.allowedEffectClasses']}
      >
        {allowedEffectClassSelectable.length > 0 ? (
          <CatalogPicker
            value={null}
            placeholder="Add effect class…"
            searchPlaceholder="Search effect classes"
            emptyMessage="All effect classes already covered."
            options={allowedEffectClassSelectable.map((option) => ({
              value: option.value,
              label: option.label,
              keywords: [option.value],
            }))}
            onChange={(value) => addEntry('allowedEffectClasses', value)}
          />
        ) : null}
      </PolicyChipList>

      <PolicyChipList
        label="Tool packs (allow)"
        entries={advanced.allowedToolPacks}
        formatLabel={(value) => packManifestsById.get(value)?.label ?? value}
        onRemove={(value) => removeEntry('allowedToolPacks', value)}
        emptyMessage={
          toolPackCatalog
            ? 'No tool packs requested.'
            : 'Tool-pack catalog unavailable — pack picker hidden.'
        }
        diagnostics={diagnosticsByPathPrefix['toolPolicy.allowedToolPacks']}
      >
        {toolPackCatalog ? (
          <CatalogPicker
            value={null}
            placeholder="Add tool pack…"
            searchPlaceholder="Search tool packs"
            emptyMessage="No tool packs available."
            options={packOptions.filter(
              (option) => !advanced.allowedToolPacks.includes(option.value),
            )}
            onChange={(value) => addEntry('allowedToolPacks', value)}
          />
        ) : null}
        {packGrantedTools.length > 0 ? (
          <div className="rounded-md border border-border/50 bg-background/40 p-2">
            <p className="mb-1 text-[9px] uppercase tracking-wider text-muted-foreground/70">
              Pack grants {packGrantedTools.length} tool
              {packGrantedTools.length === 1 ? '' : 's'}
            </p>
            <div className="flex flex-wrap gap-1">
              {packGrantedTools.map((tool) => (
                <Badge
                  key={tool}
                  variant="outline"
                  className="font-mono text-[9px] font-normal"
                  title={tool}
                >
                  {humanizeIdentifier(tool)}
                </Badge>
              ))}
            </div>
          </div>
        ) : null}
        {conflictingPackIds.length > 0 ? (
          <PolicyDiagnosticRow
            severity="warn"
            message={`Pack${conflictingPackIds.length === 1 ? '' : 's'} listed as both allowed and denied: ${conflictingPackIds
              .map((id) => packManifestsById.get(id)?.label ?? id)
              .join(', ')}.`}
            hint="Remove from one side — runtime treats denial as authoritative."
          />
        ) : null}
      </PolicyChipList>

      <PolicyChipList
        label="Tool packs (deny)"
        entries={advanced.deniedToolPacks}
        formatLabel={(value) => packManifestsById.get(value)?.label ?? value}
        onRemove={(value) => removeEntry('deniedToolPacks', value)}
        emptyMessage="No tool-pack denials."
        diagnostics={diagnosticsByPathPrefix['toolPolicy.deniedToolPacks']}
      >
        {toolPackCatalog ? (
          <CatalogPicker
            value={null}
            placeholder="Deny tool pack…"
            searchPlaceholder="Search tool packs"
            emptyMessage="No tool packs available."
            options={packOptions.filter(
              (option) => !advanced.deniedToolPacks.includes(option.value),
            )}
            onChange={(value) => addEntry('deniedToolPacks', value)}
          />
        ) : null}
      </PolicyChipList>

      <PolicyChipList
        label="Tools (deny)"
        entries={advanced.deniedTools}
        formatLabel={(value) => humanizeIdentifier(value)}
        onRemove={(value) => removeEntry('deniedTools', value)}
        emptyMessage="No per-tool denials."
        diagnostics={[
          ...diagnosticsByPathPrefix['toolPolicy.deniedTools'],
          ...diagnosticsByPathPrefix['toolPolicy.allowedTools'],
        ]}
      >
        {toolOptions.length > 0 ? (
          <CatalogPicker
            value={null}
            placeholder="Deny tool…"
            searchPlaceholder="Search tools"
            emptyMessage="No matching tools."
            options={toolOptions.filter(
              (option) => !advanced.deniedTools.includes(option.value),
            )}
            onChange={(value) => addEntry('deniedTools', value)}
          />
        ) : null}
      </PolicyChipList>

      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading
            label="Subagent roles"
            hint={subagentEffective ? undefined : 'Enable subagent delegation first'}
          />
        </div>
        <p className="-mt-1 text-[9.5px] leading-snug text-muted-foreground/75">
          When subagent delegation is on, the runtime can only spawn the
          listed roles. Denied roles override allowed ones.
        </p>
        <PolicyChipList
          label="Allowed"
          entries={advanced.allowedSubagentRoles}
          formatLabel={(value) => humanizeIdentifier(value)}
          onRemove={(value) => removeEntry('allowedSubagentRoles', value)}
          emptyMessage={
            subagentEffective
              ? 'Add at least one allowed role — required when subagent delegation is on.'
              : 'No subagent roles requested.'
          }
          diagnostics={diagnosticsByPathPrefix['toolPolicy.allowedSubagentRoles']}
        >
          <SubagentRolePicker
            disabledRoles={[
              ...advanced.allowedSubagentRoles,
              ...advanced.deniedSubagentRoles,
            ]}
            placeholder="Allow role…"
            onPick={(role) => addEntry('allowedSubagentRoles', role)}
          />
        </PolicyChipList>
        <PolicyChipList
          label="Denied"
          entries={advanced.deniedSubagentRoles}
          formatLabel={(value) => humanizeIdentifier(value)}
          onRemove={(value) => removeEntry('deniedSubagentRoles', value)}
          emptyMessage="No subagent role denials."
          diagnostics={diagnosticsByPathPrefix['toolPolicy.deniedSubagentRoles']}
        >
          <SubagentRolePicker
            disabledRoles={[
              ...advanced.allowedSubagentRoles,
              ...advanced.deniedSubagentRoles,
            ]}
            placeholder="Deny role…"
            onPick={(role) => addEntry('deniedSubagentRoles', role)}
          />
        </PolicyChipList>
        {conflictingRoles.length > 0 ? (
          <PolicyDiagnosticRow
            severity="warn"
            message={`Role${conflictingRoles.length === 1 ? '' : 's'} listed as both allowed and denied: ${conflictingRoles
              .map((role) => humanizeIdentifier(role))
              .join(', ')}.`}
            hint="Validator will reject save until cleared."
          />
        ) : null}
      </div>
    </div>
  )
}

interface PolicyChipListProps {
  label: string
  hint?: string
  entries: readonly string[]
  inferred?: readonly string[]
  formatLabel: (value: string) => string
  onRemove: (value: string) => void
  emptyMessage: string
  diagnostics: readonly AgentDefinitionValidationDiagnosticDto[]
  children?: React.ReactNode
}

function PolicyChipList({
  label,
  hint,
  entries,
  inferred,
  formatLabel,
  onRemove,
  emptyMessage,
  diagnostics,
  children,
}: PolicyChipListProps) {
  const inferredEntries = inferred ?? []
  const isEmpty = entries.length === 0 && inferredEntries.length === 0
  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between">
        <Label className="text-[9.5px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/85">
          {label}
        </Label>
        {hint ? (
          <span className="text-[8.5px] font-normal lowercase tracking-wide text-muted-foreground/60">
            {hint}
          </span>
        ) : null}
      </div>
      {isEmpty ? (
        <p className="text-[9.5px] leading-snug text-muted-foreground/70">{emptyMessage}</p>
      ) : (
        <div className="flex flex-wrap gap-1">
          {inferredEntries.map((value) => (
            <Badge
              key={`inferred-${value}`}
              variant="secondary"
              className="gap-1 font-mono text-[9.5px] font-normal"
            >
              <Sparkles className="h-2.5 w-2.5" aria-hidden />
              {formatLabel(value)}
            </Badge>
          ))}
          {entries.map((value) => (
            <Badge
              key={value}
              variant="outline"
              className="group gap-1 font-mono text-[9.5px] font-normal"
            >
              {formatLabel(value)}
              <button
                type="button"
                aria-label={`Remove ${formatLabel(value)}`}
                className="rounded-sm text-muted-foreground/70 transition-colors hover:bg-destructive/15 hover:text-destructive"
                onClick={() => onRemove(value)}
              >
                <X className="h-2.5 w-2.5" />
              </button>
            </Badge>
          ))}
        </div>
      )}
      {children}
      {diagnostics.length > 0 ? (
        <div className="flex flex-col gap-1">
          {diagnostics.map((diagnostic, index) => (
            <PolicyDiagnosticRow
              key={`${diagnostic.code}-${index}`}
              severity="error"
              message={diagnostic.message}
              hint={diagnostic.repairHint ?? diagnostic.reason ?? undefined}
              code={diagnostic.code}
            />
          ))}
        </div>
      ) : null}
    </div>
  )
}

function PolicyDiagnosticRow({
  severity,
  message,
  hint,
  code,
}: {
  severity: 'error' | 'warn'
  message: string
  hint?: string
  code?: string
}) {
  return (
    <div
      className={cn(
        'rounded-md border px-2 py-1.5 text-[9.5px] leading-snug',
        severity === 'error'
          ? 'border-destructive/45 bg-destructive/10 text-destructive'
          : 'border-amber-500/45 bg-amber-500/10 text-amber-700 dark:text-amber-300',
      )}
    >
      <p className="font-medium">{message}</p>
      {hint ? (
        <p className="mt-0.5 text-[9px] opacity-80">{hint}</p>
      ) : null}
      {code ? (
        <p className="mt-0.5 font-mono text-[8.5px] opacity-60">{code}</p>
      ) : null}
    </div>
  )
}

function SubagentRolePicker({
  disabledRoles,
  placeholder,
  onPick,
}: {
  disabledRoles: readonly string[]
  placeholder: string
  onPick: (role: SubagentRoleOption) => void
}) {
  const disabledSet = useMemo(() => new Set(disabledRoles), [disabledRoles])
  const options = useMemo<CatalogPickerOption<string>[]>(
    () =>
      SUBAGENT_ROLE_OPTIONS.filter((role) => !disabledSet.has(role)).map((role) => ({
        value: role,
        label: humanizeIdentifier(role),
        keywords: [role],
      })),
    [disabledSet],
  )
  if (options.length === 0) return null
  return (
    <CatalogPicker
      value={null}
      placeholder={placeholder}
      searchPlaceholder="Search roles"
      emptyMessage="No remaining roles."
      options={options}
      onChange={(value) => onPick(value as SubagentRoleOption)}
    />
  )
}

function PromptEditor({ nodeId, data }: { nodeId: string; data: PromptNodeData }) {
  const { updateNodeData, removeNode } = useCanvasMode()
  const { prompt } = data

  const updatePrompt = (next: Partial<PromptNodeData['prompt']>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as PromptNodeData
      return { ...cur, prompt: { ...cur.prompt, ...next } }
    })

  return (
    <div className="space-y-3">
      <FieldGroup label="Label">
        <Input
          value={prompt.label}
          onChange={(event) => updatePrompt({ label: event.target.value })}
          placeholder="Label"
          className="h-8 text-[10.75px] font-medium"
        />
      </FieldGroup>
      <FieldGroup label="Role">
        <PanelSelect
          value={prompt.role}
          onChange={(value) => updatePrompt({ role: value as AgentPromptRoleDto })}
          options={ROLE_OPTIONS.map((role) => ({ value: role, label: ROLE_LABEL[role] }))}
        />
      </FieldGroup>
      <FieldGroup label="Body">
        <Textarea
          value={prompt.body}
          onChange={(event) => updatePrompt({ body: event.target.value })}
          placeholder="Prompt body…"
          rows={9}
          className="resize-none font-mono text-[10px]"
        />
      </FieldGroup>
      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove prompt" />
    </div>
  )
}

function skillFromCatalogEntry(
  entry: AgentAuthoringAttachableSkillDto,
  options: { id?: string; includeSupportingAssets?: boolean } = {},
): AgentAttachedSkillDto {
  return {
    ...entry.attachment,
    id: options.id ?? entry.attachment.id,
    includeSupportingAssets:
      options.includeSupportingAssets ?? entry.attachment.includeSupportingAssets,
    sourceState: entry.sourceState,
    trustState: entry.trustState,
    availabilityStatus: entry.availabilityStatus,
    availabilityReason: 'Skill source is available in the authoring catalog.',
    repairHint: null,
  }
}

function SkillEditor({ nodeId, data }: { nodeId: string; data: SkillNodeData }) {
  const { updateNodeData, removeNode, authoringCatalog } = useCanvasMode()
  const { skill } = data
  const catalogEntry = authoringCatalog?.attachableSkills.find(
    (entry) => entry.sourceId === skill.sourceId,
  )

  const skillOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    const entries = authoringCatalog?.attachableSkills ?? []
    const options = entries.map((entry) => ({
      value: entry.sourceId,
      label: entry.name,
      description: entry.description,
      meta: humanizeIdentifier(entry.sourceKind),
      group: humanizeIdentifier(entry.scope),
      keywords: [entry.skillId, entry.sourceId, entry.sourceKind, entry.scope],
    }))
    if (skill.sourceId && !options.some((option) => option.value === skill.sourceId)) {
      options.push({
        value: skill.sourceId,
        label: skill.name,
        description: skill.description,
        meta: humanizeIdentifier(skill.availabilityStatus),
        group: humanizeIdentifier(skill.scope),
        keywords: [skill.skillId, skill.sourceId, skill.sourceKind, skill.scope],
      })
    }
    return options
  }, [authoringCatalog, skill])

  const updateSkill = (next: Partial<AgentAttachedSkillDto>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as SkillNodeData
      return { ...cur, skill: { ...cur.skill, ...next } }
    })

  const handlePickSkill = (sourceId: string) => {
    const entry = authoringCatalog?.attachableSkills.find(
      (candidate) => candidate.sourceId === sourceId,
    )
    if (!entry) return
    updateNodeData(nodeId, (current) => {
      const cur = current as SkillNodeData
      return {
        ...cur,
        skill: skillFromCatalogEntry(entry, {
          includeSupportingAssets: cur.skill.includeSupportingAssets,
        }),
      }
    })
  }

  const handleRefreshPin = () => {
    if (!catalogEntry) return
    updateNodeData(nodeId, (current) => {
      const cur = current as SkillNodeData
      return {
        ...cur,
        skill: skillFromCatalogEntry(catalogEntry, {
          id: cur.skill.id,
          includeSupportingAssets: cur.skill.includeSupportingAssets,
        }),
      }
    })
  }

  return (
    <div className="space-y-3">
      <FieldGroup label="Skill">
        <CatalogPicker
          options={skillOptions}
          value={skill.sourceId || null}
          onChange={handlePickSkill}
          placeholder="Pick an attachable skill"
          searchPlaceholder="Search skills, sources…"
          emptyMessage="No attachable skills available."
          loading={!authoringCatalog}
          loadingMessage="Loading skill catalog…"
        />
      </FieldGroup>

      <div className="grid grid-cols-2 gap-2">
        <FieldGroup label="Source">
          <Badge
            variant="outline"
            className="w-fit max-w-full font-mono text-[9.5px] font-normal"
          >
            {skill.sourceKind}
          </Badge>
        </FieldGroup>
        <FieldGroup label="State">
          <Badge
            variant={skill.availabilityStatus === 'available' ? 'secondary' : 'outline'}
            className="w-fit max-w-full font-mono text-[9.5px] font-normal"
          >
            {skill.availabilityStatus}
          </Badge>
        </FieldGroup>
      </div>

      {skill.description ? (
        <FieldGroup label="Description">
          <p className="text-[10.5px] leading-relaxed text-muted-foreground">
            {skill.description}
          </p>
        </FieldGroup>
      ) : null}

      <FieldGroup label="Pin">
        <div className="space-y-1.5 rounded-md border border-border/50 bg-background/40 p-2">
          <p className="break-all font-mono text-[9.5px] leading-snug text-foreground/80">
            {skill.versionHash}
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={handleRefreshPin}
            disabled={!catalogEntry}
            className="h-7 w-full justify-start gap-1.5 px-2 text-[10px]"
            aria-label="Refresh skill pin"
          >
            <RefreshCcw className="h-3 w-3" />
            Refresh pin
          </Button>
        </div>
      </FieldGroup>

      <FieldGroup label="Supporting assets">
        <CheckboxToggle
          checked={skill.includeSupportingAssets}
          onChange={(next) => updateSkill({ includeSupportingAssets: next })}
          label="Include supporting assets"
        />
      </FieldGroup>

      <FieldGroup label="Required">
        <CheckboxToggle
          checked={skill.required}
          onChange={() => undefined}
          label="Required every run"
          locked
          reasonTooltip="Required attachments block the run when unavailable."
        />
      </FieldGroup>

      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove skill" />
    </div>
  )
}

function ToolEditor({ nodeId, data }: { nodeId: string; data: ToolNodeData }) {
  const { updateNodeData, removeNode, authoringCatalog } = useCanvasMode()
  const { tool } = data
  const isPlaceholder = !tool.name || /^tool_\d+$/.test(tool.name)

  const toolOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    if (!authoringCatalog) return []
    return authoringCatalog.tools.map((entry) => ({
      value: entry.name,
      label: humanizeIdentifier(entry.name),
      description: entry.description,
      meta: entry.effectClass,
      group: entry.group,
      keywords: [entry.name, entry.group, entry.riskClass, ...entry.tags],
    }))
  }, [authoringCatalog])

  const handlePickTool = (toolName: string) => {
    const entry = authoringCatalog?.tools.find((candidate) => candidate.name === toolName)
    if (!entry) return
    updateNodeData(nodeId, (current) => {
      const cur = current as ToolNodeData
      return {
        ...cur,
        tool: {
          ...cur.tool,
          name: entry.name,
          group: entry.group,
          description: entry.description,
          effectClass: entry.effectClass,
          riskClass: entry.riskClass,
          tags: [...entry.tags],
          schemaFields: [...entry.schemaFields],
          examples: [...entry.examples],
        },
      }
    })
  }

  return (
    <div className="space-y-3">
      <FieldGroup label="Tool">
        <CatalogPicker
          options={toolOptions}
          value={isPlaceholder ? null : tool.name}
          onChange={handlePickTool}
          placeholder="Pick a registered tool"
          searchPlaceholder="Search tools, groups, tags…"
          emptyMessage="No matching tools."
          loading={!authoringCatalog}
          loadingMessage="Loading tool catalog…"
        />
      </FieldGroup>
      {!isPlaceholder ? (
        <>
          <FieldGroup label="Description">
            <p className="text-[10.5px] leading-relaxed text-muted-foreground">{tool.description}</p>
          </FieldGroup>
          <div className="grid grid-cols-2 gap-2">
            <FieldGroup label="Group">
              <Badge
                variant="secondary"
                className="w-fit max-w-full font-mono text-[9.5px] font-normal"
              >
                {tool.group}
              </Badge>
            </FieldGroup>
            <FieldGroup label="Effect">
              <Badge
                variant="outline"
                className="w-fit max-w-full font-mono text-[9.5px] font-normal"
              >
                {tool.effectClass}
              </Badge>
            </FieldGroup>
          </div>
          {tool.riskClass ? (
            <FieldGroup label="Risk">
              <Badge
                variant="outline"
                className="w-fit max-w-full font-mono text-[9.5px] font-normal"
              >
                {tool.riskClass}
              </Badge>
            </FieldGroup>
          ) : null}
        </>
      ) : (
        <p className="text-[10px] leading-snug text-muted-foreground/80">
          Pick a tool from the registry. The runtime can only dispatch tools it knows about.
        </p>
      )}
      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove tool" />
    </div>
  )
}

function DbTableEditor({ nodeId, data }: { nodeId: string; data: DbTableNodeData }) {
  const { updateNodeData, removeNode, authoringCatalog } = useCanvasMode()
  const { table, touchpoint, purpose, columns } = data
  const isPlaceholder = !table || /^table_\d+$/.test(table)

  const tableOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    if (!authoringCatalog) return []
    return authoringCatalog.dbTables.map((entry) => ({
      value: entry.table,
      label: humanizeIdentifier(entry.table),
      description: entry.purpose,
      group: 'tables',
      keywords: [entry.table, ...entry.columns],
    }))
  }, [authoringCatalog])

  const handlePickTable = (selected: string) => {
    const entry = authoringCatalog?.dbTables.find((candidate) => candidate.table === selected)
    if (!entry) return
    updateNodeData(nodeId, (current) => {
      const cur = current as DbTableNodeData
      return {
        ...cur,
        table: entry.table,
        purpose: entry.purpose,
        columns: [...entry.columns],
      }
    })
  }

  return (
    <div className="space-y-3">
      <FieldGroup label="Table">
        <CatalogPicker
          options={tableOptions}
          value={isPlaceholder ? null : table}
          onChange={handlePickTable}
          placeholder="Pick a known table"
          searchPlaceholder="Search tables…"
          emptyMessage="No matching tables."
          loading={!authoringCatalog}
          loadingMessage="Loading table catalog…"
        />
      </FieldGroup>
      <FieldGroup label="Touchpoint">
        <PanelSelect
          value={touchpoint}
          onChange={(value) =>
            updateNodeData(nodeId, (current) => {
              const cur = current as DbTableNodeData
              return { ...cur, touchpoint: value as AgentDbTouchpointKindDto }
            })
          }
          options={TOUCHPOINT_OPTIONS.map((option) => ({
            value: option,
            label: humanizeIdentifier(option),
          }))}
        />
      </FieldGroup>
      {!isPlaceholder ? (
        <>
          {purpose ? (
            <FieldGroup label="Purpose">
              <p className="text-[10.5px] leading-relaxed text-muted-foreground">{purpose}</p>
            </FieldGroup>
          ) : null}
          {columns.length > 0 ? (
            <FieldGroup label="Columns">
              <p className="break-words font-mono text-[10px] leading-snug text-foreground/80">
                {columns.join(', ')}
              </p>
            </FieldGroup>
          ) : null}
        </>
      ) : (
        <p className="text-[10px] leading-snug text-muted-foreground/80">
          Pick a table the runtime knows about. Columns and purpose come from the catalog.
        </p>
      )}
      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove DB table" />
    </div>
  )
}

function OutputEditor({ nodeId, data }: { nodeId: string; data: OutputNodeData }) {
  const { updateNodeData } = useCanvasMode()
  const { output } = data
  const updateOutput = (next: Partial<OutputNodeData['output']>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as OutputNodeData
      return { ...cur, output: { ...cur.output, ...next } }
    })

  return (
    <div className="space-y-3">
      <FieldGroup label="Label">
        <Input
          value={output.label}
          onChange={(event) => updateOutput({ label: event.target.value })}
          placeholder="Final response"
          className="h-8 text-[10.75px] font-medium"
        />
      </FieldGroup>
      <FieldGroup label="Contract">
        <PanelSelect
          value={output.contract}
          onChange={(value) =>
            updateOutput({ contract: value as RuntimeAgentOutputContractDto })
          }
          options={OUTPUT_CONTRACTS.map((contract) => ({
            value: contract,
            label: humanizeIdentifier(contract),
          }))}
        />
      </FieldGroup>
      <FieldGroup label="Description">
        <Textarea
          value={output.description}
          onChange={(event) => updateOutput({ description: event.target.value })}
          placeholder="Describe what a successful response includes."
          rows={3}
          className="resize-none text-[10px]"
        />
      </FieldGroup>
    </div>
  )
}

function OutputSectionEditor({
  nodeId,
  data,
}: {
  nodeId: string
  data: OutputSectionNodeData
}) {
  const { updateNodeData, removeNode } = useCanvasMode()
  const { section } = data
  const updateSection = (next: Partial<OutputSectionNodeData['section']>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as OutputSectionNodeData
      return { ...cur, section: { ...cur.section, ...next } }
    })

  return (
    <div className="space-y-3">
      <FieldGroup label="Label">
        <Input
          value={section.label}
          onChange={(event) => updateSection({ label: event.target.value })}
          placeholder="Section label"
          className="h-8 text-[10.75px] font-medium"
        />
      </FieldGroup>
      <FieldGroup label="Section ID">
        <Input
          value={section.id}
          onChange={(event) => updateSection({ id: event.target.value })}
          placeholder="section_id"
          className="h-8 font-mono text-[10px]"
        />
      </FieldGroup>
      <FieldGroup label="Emphasis">
        <PanelSelect
          value={section.emphasis}
          onChange={(value) =>
            updateSection({ emphasis: value as AgentOutputSectionEmphasisDto })
          }
          options={EMPHASIS_OPTIONS.map((option) => ({
            value: option,
            label: humanizeIdentifier(option),
          }))}
        />
      </FieldGroup>
      <FieldGroup label="Description">
        <Textarea
          value={section.description}
          onChange={(event) => updateSection({ description: event.target.value })}
          placeholder="What goes in this section?"
          rows={3}
          className="resize-none text-[10px]"
        />
      </FieldGroup>
      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove section" />
    </div>
  )
}

function ConsumedArtifactEditor({
  nodeId,
  data,
}: {
  nodeId: string
  data: ConsumedArtifactNodeData
}) {
  const { updateNodeData, removeNode, authoringCatalog } = useCanvasMode()
  const { artifact } = data
  const isPlaceholder = !artifact.id || /^artifact_\d+$/.test(artifact.id)

  const artifactOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    if (!authoringCatalog) return []
    return authoringCatalog.upstreamArtifacts.map((entry) => ({
      value: `${entry.sourceAgent}::${entry.contract}`,
      label: `${entry.sourceAgentLabel} → ${entry.contractLabel}`,
      description: entry.description,
      group: entry.sourceAgentLabel,
      keywords: [entry.sourceAgent, entry.contract, ...entry.sections.map((s) => s.id)],
    }))
  }, [authoringCatalog])

  const handlePickArtifact = (key: string) => {
    const entry = authoringCatalog?.upstreamArtifacts.find(
      (candidate) => `${candidate.sourceAgent}::${candidate.contract}` === key,
    )
    if (!entry) return
    updateNodeData(nodeId, (current) => {
      const cur = current as ConsumedArtifactNodeData
      return {
        ...cur,
        artifact: {
          ...cur.artifact,
          id: `${entry.sourceAgent}::${entry.contract}`,
          label: entry.label,
          description: entry.description,
          sourceAgent: entry.sourceAgent,
          contract: entry.contract,
          sections: entry.sections.map((section) => section.id),
        },
      }
    })
  }

  const updateArtifact = (next: Partial<ConsumedArtifactNodeData['artifact']>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as ConsumedArtifactNodeData
      return { ...cur, artifact: { ...cur.artifact, ...next } }
    })

  return (
    <div className="space-y-3">
      <FieldGroup label="Upstream artifact">
        <CatalogPicker
          options={artifactOptions}
          value={isPlaceholder ? null : `${artifact.sourceAgent}::${artifact.contract}`}
          onChange={handlePickArtifact}
          placeholder="Pick an upstream agent + contract"
          searchPlaceholder="Search agents, contracts, sections…"
          emptyMessage="No upstream artifacts available."
          loading={!authoringCatalog}
          loadingMessage="Loading upstream catalog…"
        />
      </FieldGroup>
      {!isPlaceholder ? (
        <>
          <FieldGroup label="Description">
            <p className="text-[10.5px] leading-relaxed text-muted-foreground">
              {artifact.description}
            </p>
          </FieldGroup>
          {artifact.sections.length > 0 ? (
            <FieldGroup label="Sections used">
              <div className="flex flex-wrap gap-1">
                {artifact.sections.map((section) => (
                  <Badge
                    key={section}
                    variant="outline"
                    className="text-[9px] font-normal"
                  >
                    {humanizeIdentifier(section)}
                  </Badge>
                ))}
              </div>
            </FieldGroup>
          ) : null}
        </>
      ) : (
        <p className="text-[10px] leading-snug text-muted-foreground/80">
          Pick an upstream agent's output. Description and sections come from its contract.
        </p>
      )}
      <FieldGroup label="Required">
        <CheckboxToggle
          checked={artifact.required}
          onChange={(next) => updateArtifact({ required: next })}
          label="Required for run"
        />
      </FieldGroup>
      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove artifact" />
    </div>
  )
}

type StageEditorContext = {
  phase: CustomAgentWorkflowPhaseDto
  isStart: boolean
}

function StageEditor({ nodeId, data }: { nodeId: string; data: StageNodeData }) {
  const {
    updateNodeData,
    removeNode,
    authoringCatalog,
    stageList,
    agentToolNames,
    policyDiagnostics,
  } = useCanvasMode()
  const { phase, isStart } = data
  // Default the explainer open when this is the agent's first stage — the
  // user just reached for an unfamiliar feature and the panel is the first
  // place they'll look for "what does this do." Once they add a second
  // stage they've clearly understood the concept, so the explainer
  // collapses by default for every later stage.
  const [explainerOpen, setExplainerOpen] = useState(() => stageList.length <= 1)

  const updatePhase = (next: Partial<CustomAgentWorkflowPhaseDto>) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as unknown as StageEditorContext
      return {
        ...cur,
        phase: { ...cur.phase, ...next },
      } as unknown as StageNodeData
    })

  const setIsStart = (next: boolean) =>
    updateNodeData(nodeId, (current) => {
      const cur = current as unknown as StageEditorContext
      return { ...cur, isStart: next } as unknown as StageNodeData
    })

  // Allowed-tools picker. Prefer the agent's wired tools — those are what the
  // runtime will actually have available. Fall back to the full authoring
  // catalog when the canvas has no tool nodes yet, so users can still pick.
  const allowedToolOptions = useMemo<CatalogPickerOption<string>[]>(() => {
    const catalogByName = new Map<string, { description?: string; group?: string }>()
    if (authoringCatalog) {
      for (const tool of authoringCatalog.tools) {
        catalogByName.set(tool.name, { description: tool.description, group: tool.group })
      }
    }
    const allowedSet = new Set(phase.allowedTools ?? [])
    const universe = agentToolNames.length > 0
      ? agentToolNames
      : (authoringCatalog?.tools ?? []).map((tool) => tool.name)
    return universe
      .filter((name) => !allowedSet.has(name))
      .map((name) => {
        const meta = catalogByName.get(name)
        return {
          value: name,
          label: humanizeIdentifier(name),
          description: meta?.description,
          group: meta?.group ?? 'tools',
          keywords: [name],
        }
      })
  }, [authoringCatalog, agentToolNames, phase.allowedTools])

  const addAllowedTool = (toolName: string) => {
    const existing = phase.allowedTools ?? []
    if (existing.includes(toolName)) return
    updatePhase({ allowedTools: [...existing, toolName] })
  }
  const removeAllowedTool = (toolName: string) => {
    const next = (phase.allowedTools ?? []).filter((name) => name !== toolName)
    updatePhase({ allowedTools: next.length > 0 ? next : undefined })
  }

  // Required-gate (`requiredChecks`) edits.
  const addGate = (kind: CustomAgentWorkflowGateDto['kind']) => {
    const next: CustomAgentWorkflowGateDto =
      kind === 'todo_completed'
        ? { kind: 'todo_completed', todoId: '' }
        : { kind: 'tool_succeeded', toolName: '' }
    updatePhase({ requiredChecks: [...(phase.requiredChecks ?? []), next] })
  }
  const updateGate = (index: number, patch: Partial<CustomAgentWorkflowGateDto>) => {
    const current = phase.requiredChecks ?? []
    const next = current.map((check, i) => {
      if (i !== index) return check
      // Type-narrow per kind so the partial only applies to compatible fields.
      if (check.kind === 'todo_completed') {
        return { ...check, ...(patch as Partial<typeof check>) }
      }
      return { ...check, ...(patch as Partial<typeof check>) }
    })
    updatePhase({ requiredChecks: next })
  }
  const removeGate = (index: number) => {
    const next = (phase.requiredChecks ?? []).filter((_, i) => i !== index)
    updatePhase({ requiredChecks: next.length > 0 ? next : undefined })
  }

  // Retry-limit. Empty input clears the field so the runtime falls back to
  // the global default; a numeric value is clamped to non-negative integers.
  const handleRetryLimitChange = (raw: string) => {
    const trimmed = raw.trim()
    if (trimmed === '') {
      updatePhase({ retryLimit: undefined })
      return
    }
    const parsed = Number.parseInt(trimmed, 10)
    if (Number.isNaN(parsed) || parsed < 0) return
    updatePhase({ retryLimit: parsed })
  }

  // Exits — branches authored on the source stage. The exit target dropdown
  // lists every OTHER stage so the user can't accidentally wire a self-loop.
  const otherStages = useMemo(
    () => stageList.filter((entry) => entry.id !== phase.id),
    [stageList, phase.id],
  )
  const addExit = (targetPhaseId: string) => {
    const existing = phase.branches ?? []
    if (existing.some((branch) => branch.targetPhaseId === targetPhaseId)) return
    const branch: CustomAgentWorkflowBranchDto = {
      targetPhaseId,
      condition: { kind: 'always' },
    }
    updatePhase({ branches: [...existing, branch] })
  }
  const updateExit = (index: number, patch: Partial<CustomAgentWorkflowBranchDto>) => {
    const next = (phase.branches ?? []).map((branch, i) =>
      i === index ? { ...branch, ...patch } : branch,
    )
    updatePhase({ branches: next })
  }
  const updateExitCondition = (
    index: number,
    condition: CustomAgentWorkflowBranchConditionDto,
  ) => updateExit(index, { condition })
  const removeExit = (index: number) => {
    const next = (phase.branches ?? []).filter((_, i) => i !== index)
    updatePhase({ branches: next.length > 0 ? next : undefined })
  }

  // Inline diagnostics — filter to issues whose path targets this stage's
  // index in workflowStructure.phases. Without an index match (e.g. when the
  // stage isn't yet visible in the snapshot) we fall back to issues that name
  // the phase id explicitly in the message.
  const phaseIndex = stageList.findIndex((entry) => entry.id === phase.id)
  const stageDiagnostics = useMemo(() => {
    return policyDiagnostics.filter((diag) => {
      if (!diag.path) return false
      if (phaseIndex >= 0 && diag.path.startsWith(`workflowStructure.phases[${phaseIndex}]`)) {
        return true
      }
      return (
        diag.path.startsWith('workflowStructure.startPhaseId') &&
        (diag.message.includes(phase.id) || diag.reason?.includes(phase.id))
      )
    })
  }, [policyDiagnostics, phaseIndex, phase.id])

  const allowedToolsHint =
    (phase.allowedTools?.length ?? 0) === 0
      ? 'Empty — every tool the agent has is allowed.'
      : `${phase.allowedTools?.length} tool${phase.allowedTools?.length === 1 ? '' : 's'} allowed.`

  return (
    <div className="space-y-4">
      <button
        type="button"
        onClick={() => setExplainerOpen((open) => !open)}
        className="flex w-full items-start gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/5 px-2 py-1.5 text-left text-[10px] leading-snug text-foreground/85 transition-colors hover:bg-amber-500/10"
      >
        <Info className="mt-0.5 h-3 w-3 shrink-0 text-amber-500" aria-hidden />
        <span className="flex-1">
          <span className="font-semibold">What is a stage?</span>{' '}
          {explainerOpen ? (
            <span className="text-muted-foreground">
              Stages let one agent change its rules part-way through a run. Each stage decides
              which tools the agent can call, and the agent can only leave a stage once your
              gates are satisfied. Use this to force a "research before edit" or
              "plan before execute" flow.
            </span>
          ) : (
            <span className="text-muted-foreground">Click to learn how stages work.</span>
          )}
        </span>
        {explainerOpen ? (
          <ChevronDown className="mt-0.5 h-3 w-3 shrink-0 text-muted-foreground/70" />
        ) : (
          <ChevronRight className="mt-0.5 h-3 w-3 shrink-0 text-muted-foreground/70" />
        )}
      </button>

      <div className="space-y-2.5">
        <SectionHeading label="Identity" />
        <FieldGroup label="Title">
          <Input
            value={phase.title}
            onChange={(event) => updatePhase({ title: event.target.value })}
            placeholder={humanizeIdentifier(phase.id)}
            maxLength={80}
            className="h-8 text-[10.75px] font-semibold"
          />
        </FieldGroup>
        <FieldGroup label="Description">
          <Textarea
            value={phase.description ?? ''}
            onChange={(event) =>
              updatePhase({
                description: event.target.value === '' ? undefined : event.target.value,
              })
            }
            placeholder="What is the agent doing during this stage?"
            rows={2}
            className="resize-none text-[10px]"
          />
        </FieldGroup>
        <FieldGroup label="Stage ID" hint="locked">
          <div className="flex items-center gap-1.5 rounded-md border border-border/55 bg-background/40 px-2 py-1.5 text-[10px]">
            <Lock className="h-2.5 w-2.5 text-muted-foreground/70" aria-hidden />
            <span className="truncate font-mono text-[10px] text-foreground/85">{phase.id}</span>
          </div>
        </FieldGroup>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading icon={Wrench} label="Allowed tools" hint={allowedToolsHint} />
        {(phase.allowedTools?.length ?? 0) > 0 ? (
          <div className="flex flex-wrap gap-1">
            {(phase.allowedTools ?? []).map((toolName) => (
              <Badge
                key={toolName}
                variant="outline"
                className="group gap-1 font-mono text-[9.5px] font-normal"
              >
                {humanizeIdentifier(toolName)}
                <button
                  type="button"
                  aria-label={`Remove ${humanizeIdentifier(toolName)}`}
                  className="rounded-sm text-muted-foreground/70 transition-colors hover:bg-destructive/15 hover:text-destructive"
                  onClick={() => removeAllowedTool(toolName)}
                >
                  <X className="h-2.5 w-2.5" />
                </button>
              </Badge>
            ))}
          </div>
        ) : (
          <p className="text-[9.5px] leading-snug text-muted-foreground/70">
            Add tools to restrict what the agent may call while this stage is active. Leave
            empty to allow every tool the agent has.
          </p>
        )}
        {allowedToolOptions.length > 0 ? (
          <CatalogPicker
            value={null}
            onChange={addAllowedTool}
            options={allowedToolOptions}
            placeholder="Allow another tool…"
            searchPlaceholder="Search tools…"
            emptyMessage="No more tools to allow."
            loading={!authoringCatalog && agentToolNames.length === 0}
            loadingMessage="Loading tool catalog…"
          />
        ) : null}
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading
          icon={ListChecks}
          label="Required gates"
          hint="must pass before exit"
        />
        <p className="text-[9.5px] leading-snug text-muted-foreground/70">
          The agent cannot leave this stage until every gate listed here passes.
        </p>
        {(phase.requiredChecks ?? []).map((check, index) => (
          <div
            key={`${check.kind}:${index}`}
            className="space-y-1.5 rounded-md border border-border/55 bg-background/40 px-2 py-2"
          >
            <div className="flex items-center justify-between gap-2">
              <Badge
                variant="outline"
                className="h-5 px-1.5 text-[9px] font-medium uppercase tracking-wide"
              >
                {check.kind === 'todo_completed' ? 'todo' : 'tool succeeded'}
              </Badge>
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                onClick={() => removeGate(index)}
                className="size-5 text-muted-foreground hover:text-destructive"
                aria-label="Remove gate"
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </div>
            {check.kind === 'todo_completed' ? (
              <Input
                value={check.todoId}
                onChange={(event) => updateGate(index, { todoId: event.target.value })}
                placeholder="todo_id"
                className="h-7 font-mono text-[10px]"
                aria-label="Todo id"
              />
            ) : (
              <div className="grid grid-cols-[1fr_64px] gap-1.5">
                <Input
                  value={check.toolName}
                  onChange={(event) => updateGate(index, { toolName: event.target.value })}
                  placeholder="tool_name"
                  className="h-7 font-mono text-[10px]"
                  aria-label="Tool name"
                />
                <Input
                  value={check.minCount === undefined ? '' : String(check.minCount)}
                  onChange={(event) => {
                    const trimmed = event.target.value.trim()
                    if (trimmed === '') {
                      updateGate(index, { minCount: undefined })
                      return
                    }
                    const parsed = Number.parseInt(trimmed, 10)
                    if (Number.isNaN(parsed) || parsed < 1) return
                    updateGate(index, { minCount: parsed })
                  }}
                  placeholder="× 1"
                  className="h-7 text-center font-mono text-[10px]"
                  aria-label="Minimum count"
                />
              </div>
            )}
          </div>
        ))}
        <div className="flex gap-1">
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => addGate('todo_completed')}
            className="h-7 flex-1 gap-1 text-[10px]"
          >
            <Plus className="h-3 w-3" />
            Todo
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => addGate('tool_succeeded')}
            className="h-7 flex-1 gap-1 text-[10px]"
          >
            <Plus className="h-3 w-3" />
            Tool
          </Button>
        </div>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading icon={RefreshCcw} label="Retry limit" hint="optional" />
        <FieldGroup>
          <Input
            type="number"
            min={0}
            value={phase.retryLimit === undefined ? '' : String(phase.retryLimit)}
            onChange={(event) => handleRetryLimitChange(event.target.value)}
            placeholder="Default"
            className="h-8 text-[10.75px]"
            aria-label="Retry limit"
          />
        </FieldGroup>
        <p className="text-[9.5px] leading-snug text-muted-foreground/70">
          How many tool failures in this stage before the run aborts. Leave empty for the
          runtime default.
        </p>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading icon={GitBranch} label="Exits" hint="branches" />
        <p className="text-[9.5px] leading-snug text-muted-foreground/70">
          When an exit's condition is met the agent moves to that stage on its next tool call.
        </p>
        {(phase.branches ?? []).map((branch, index) => {
          const targetTitle =
            stageList.find((entry) => entry.id === branch.targetPhaseId)?.title ??
            branch.targetPhaseId
          return (
            <div
              key={`${branch.targetPhaseId}:${index}`}
              className="space-y-1.5 rounded-md border border-border/55 bg-background/40 px-2 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-[10px] font-semibold text-foreground/90">
                  → {targetTitle}
                </span>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  onClick={() => removeExit(index)}
                  className="size-5 text-muted-foreground hover:text-destructive"
                  aria-label="Remove exit"
                >
                  <Trash2 className="h-3 w-3" />
                </Button>
              </div>
              <FieldGroup label="When">
                <PanelSelect
                  value={branch.condition.kind}
                  onChange={(kind) => {
                    if (kind === 'always') {
                      updateExitCondition(index, { kind: 'always' })
                    } else if (kind === 'todo_completed') {
                      updateExitCondition(index, { kind: 'todo_completed', todoId: '' })
                    } else if (kind === 'tool_succeeded') {
                      updateExitCondition(index, { kind: 'tool_succeeded', toolName: '' })
                    }
                  }}
                  options={[
                    { value: 'always', label: 'Always' },
                    { value: 'todo_completed', label: 'Todo completed' },
                    { value: 'tool_succeeded', label: 'Tool succeeded' },
                  ]}
                  ariaLabel="Exit condition kind"
                />
              </FieldGroup>
              {branch.condition.kind === 'todo_completed' ? (
                <Input
                  value={branch.condition.todoId}
                  onChange={(event) =>
                    updateExitCondition(index, {
                      kind: 'todo_completed',
                      todoId: event.target.value,
                    })
                  }
                  placeholder="todo_id"
                  className="h-7 font-mono text-[10px]"
                  aria-label="Exit todo id"
                />
              ) : null}
              {branch.condition.kind === 'tool_succeeded' ? (
                <div className="grid grid-cols-[1fr_64px] gap-1.5">
                  <Input
                    value={branch.condition.toolName}
                    onChange={(event) =>
                      updateExitCondition(index, {
                        kind: 'tool_succeeded',
                        toolName: event.target.value,
                        minCount: branch.condition.kind === 'tool_succeeded'
                          ? branch.condition.minCount
                          : undefined,
                      })
                    }
                    placeholder="tool_name"
                    className="h-7 font-mono text-[10px]"
                    aria-label="Exit tool name"
                  />
                  <Input
                    value={
                      branch.condition.kind === 'tool_succeeded' &&
                      branch.condition.minCount !== undefined
                        ? String(branch.condition.minCount)
                        : ''
                    }
                    onChange={(event) => {
                      const trimmed = event.target.value.trim()
                      const condition = branch.condition
                      if (condition.kind !== 'tool_succeeded') return
                      if (trimmed === '') {
                        updateExitCondition(index, {
                          kind: 'tool_succeeded',
                          toolName: condition.toolName,
                        })
                        return
                      }
                      const parsed = Number.parseInt(trimmed, 10)
                      if (Number.isNaN(parsed) || parsed < 1) return
                      updateExitCondition(index, {
                        kind: 'tool_succeeded',
                        toolName: condition.toolName,
                        minCount: parsed,
                      })
                    }}
                    placeholder="× 1"
                    className="h-7 text-center font-mono text-[10px]"
                    aria-label="Exit minimum count"
                  />
                </div>
              ) : null}
            </div>
          )
        })}
        {otherStages.length > 0 ? (
          <CatalogPicker
            value={null}
            onChange={addExit}
            options={otherStages
              .filter(
                (entry) =>
                  !(phase.branches ?? []).some(
                    (branch) => branch.targetPhaseId === entry.id,
                  ),
              )
              .map((entry) => ({
                value: entry.id,
                label: entry.title || humanizeIdentifier(entry.id),
                description: entry.id,
                keywords: [entry.id, entry.title],
              }))}
            placeholder="Add exit to another stage…"
            searchPlaceholder="Search stages…"
            emptyMessage="No more stages to exit to."
          />
        ) : (
          <p className="text-[9.5px] leading-snug text-muted-foreground/70">
            Add more stages on the canvas to create exits.
          </p>
        )}
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <SectionHeading icon={Flag} label="Start stage" />
        <CheckboxToggle
          checked={isStart}
          onChange={(next) => setIsStart(next)}
          label="Mark as start"
        />
        <p className="text-[9.5px] leading-snug text-muted-foreground/70">
          The runtime begins each run in the start stage. Marking another stage as start clears
          this one automatically.
        </p>
      </div>

      {stageDiagnostics.length > 0 ? (
        <>
          <Separator className="opacity-60" />
          <div className="space-y-1.5">
            <SectionHeading label="Validation" hint={`${stageDiagnostics.length}`} />
            <div className="flex flex-col gap-1">
              {stageDiagnostics.map((diagnostic, index) => (
                <PolicyDiagnosticRow
                  key={`${diagnostic.code}-${index}`}
                  severity="error"
                  message={diagnostic.message}
                  hint={diagnostic.repairHint ?? diagnostic.reason ?? undefined}
                  code={diagnostic.code}
                />
              ))}
            </div>
          </div>
        </>
      ) : null}

      <RemoveRow onRemove={() => removeNode(nodeId)} label="Remove stage" />
    </div>
  )
}
