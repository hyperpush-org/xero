'use client'

import { useMemo } from 'react'
import {
  Bot,
  Database,
  FileText,
  GitMerge,
  Layers,
  Lock,
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
  'harness_test_report',
]
const EMPHASIS_OPTIONS: AgentOutputSectionEmphasisDto[] = ['core', 'standard', 'optional']
const TOUCHPOINT_OPTIONS: AgentDbTouchpointKindDto[] = ['read', 'write', 'encouraged']

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
        className="agent-properties-panel pointer-events-auto absolute bottom-4 left-4 top-14 z-30 flex w-[272px] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[10.5px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
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
        <div className="flex-1 overflow-y-auto px-3 py-3">{renderEditor(selectedNode)}</div>
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
        className="h-8 w-full border-border/60 bg-background/60 text-[10.5px] font-medium text-foreground/90 shadow-none data-[size=sm]:h-8"
      >
        <SelectValue placeholder={placeholder ?? 'Select…'} />
      </SelectTrigger>
      <SelectContent className="text-[10.5px]">
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value} className="text-[10.5px]">
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
  const { updateNodeData, inferredAdvanced } = useCanvasMode()
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
            className="h-8 text-[11.5px] font-semibold"
          />
        </FieldGroup>
        <FieldGroup label="Short label" hint={`${header.shortLabel.length}/24`}>
          <Input
            value={header.shortLabel}
            onChange={(event) => updateHeader({ shortLabel: event.target.value })}
            placeholder="Short label"
            maxLength={24}
            className="h-8 text-[10.5px]"
          />
        </FieldGroup>
        <FieldGroup label="Description">
          <Textarea
            value={header.description}
            onChange={(event) => updateHeader({ description: event.target.value })}
            placeholder="What does this agent do?"
            rows={2}
            className="resize-none text-[10.5px]"
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
            value={
              header.baseCapabilityProfile === 'harness_test'
                ? 'observe_only'
                : header.baseCapabilityProfile
            }
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
            className="resize-none text-[10.5px]"
          />
        </FieldGroup>
        <FieldGroup label="Workflow contract">
          <Textarea
            value={advanced.workflowContract}
            onChange={(event) => updateAdvanced({ workflowContract: event.target.value })}
            placeholder="What end-to-end workflow does this agent run?"
            rows={2}
            className="resize-none text-[10.5px]"
          />
        </FieldGroup>
        <FieldGroup label="Final response contract">
          <Textarea
            value={advanced.finalResponseContract}
            onChange={(event) => updateAdvanced({ finalResponseContract: event.target.value })}
            placeholder="What does a successful final response include?"
            rows={2}
            className="resize-none text-[10.5px]"
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
              className="h-8 text-[10.5px]"
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
              className="h-8 text-[10.5px]"
            />
          ))}
        </div>
      </div>

      <Separator className="opacity-60" />

      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <SectionHeading label="Allowed approval modes" />
        </div>
        <div className="grid grid-cols-3 gap-1.5">
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
        <div className="grid grid-cols-2 gap-1.5">
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
        <div className="grid grid-cols-2 gap-1.5">
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
    </div>
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
          className="h-8 text-[11.5px] font-medium"
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
          className="resize-none font-mono text-[10.5px]"
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
          className="h-8 text-[11.5px] font-medium"
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
          className="resize-none text-[10.5px]"
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
          className="h-8 text-[11.5px] font-medium"
        />
      </FieldGroup>
      <FieldGroup label="Section ID">
        <Input
          value={section.id}
          onChange={(event) => updateSection({ id: event.target.value })}
          placeholder="section_id"
          className="h-8 font-mono text-[10.5px]"
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
          className="resize-none text-[10.5px]"
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
