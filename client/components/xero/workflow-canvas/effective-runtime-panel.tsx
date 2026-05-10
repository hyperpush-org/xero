'use client'

import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  X,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { cn } from '@/lib/utils'
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionLifecycleLabel,
  getAgentDefinitionScopeLabel,
  type AgentDefinitionPreviewResponseDto,
  type AgentDefinitionValidationDiagnosticDto,
  type AgentEffectiveRuntimePreviewDto,
  type AgentPreviewAttachedSkillInjectionEntryDto,
  type AgentPreviewGraphDiagnosticDto,
  type AgentPreviewPromptFragmentDto,
  type AgentPreviewToolAccessEntryDto,
  type CustomAgentWorkflowStructureDto,
  type AgentDefinitionBaseCapabilityProfileDto,
  type AgentDefinitionLifecycleStateDto,
  type AgentDefinitionScopeDto,
  customAgentWorkflowStructureSchema,
} from '@/src/lib/xero-model/agent-definition'
import type { CapabilityPermissionExplanationDto } from '@/src/lib/xero-model/agent-reports'

export interface EffectiveRuntimePanelProps {
  open: boolean
  onClose: () => void
  // The label shown next to the source toggle (typically the agent display name).
  agentLabel: string
  // Loading status for the active-version preview ("saved"); the unsaved canvas
  // ("draft") preview is only valid in edit mode.
  loading: boolean
  errorMessage: string | null
  preview: AgentDefinitionPreviewResponseDto | null
  // When the canvas is in edit mode, provide a way to also preview the unsaved
  // snapshot. Omit `draftPreview*` and `onRefreshDraft` to render a single
  // "Active version" view.
  draftAvailable?: boolean
  draftPreview?: AgentDefinitionPreviewResponseDto | null
  draftLoading?: boolean
  draftErrorMessage?: string | null
  onRefreshActive?: () => void
  onRefreshDraft?: () => void
}

type PanelSource = 'saved' | 'draft'

export function EffectiveRuntimePanel({
  open,
  onClose,
  agentLabel,
  loading,
  errorMessage,
  preview,
  draftAvailable = false,
  draftPreview = null,
  draftLoading = false,
  draftErrorMessage = null,
  onRefreshActive,
  onRefreshDraft,
}: EffectiveRuntimePanelProps) {
  const [source, setSource] = useState<PanelSource>(draftAvailable ? 'draft' : 'saved')

  useEffect(() => {
    if (!draftAvailable && source === 'draft') setSource('saved')
  }, [draftAvailable, source])

  const activeLoading = source === 'saved' ? loading : draftLoading
  const activeError = source === 'saved' ? errorMessage : draftErrorMessage
  const activePreview = source === 'saved' ? preview : draftPreview

  const handleRefresh = useCallback(() => {
    if (source === 'saved') onRefreshActive?.()
    else onRefreshDraft?.()
  }, [source, onRefreshActive, onRefreshDraft])

  if (!open) return null

  return (
    <aside
      role="dialog"
      aria-label="Effective runtime preview"
      className="agent-effective-runtime-panel pointer-events-auto absolute right-4 top-14 z-30 flex max-h-[calc(100%-4.5rem)] w-[420px] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[12px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
      onPointerDown={(event) => event.stopPropagation()}
      onWheel={(event) => event.stopPropagation()}
    >
      <header className="flex items-center gap-2 border-b border-border/50 px-3 py-2">
        <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
        <p
          className="min-w-0 flex-1 truncate text-[12px] font-semibold leading-none text-foreground"
          title={`Effective runtime · ${agentLabel}`}
        >
          Effective runtime
        </p>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          onClick={onClose}
          className="size-5 shrink-0 text-muted-foreground hover:text-foreground"
          aria-label="Close effective runtime panel"
        >
          <X className="h-3 w-3" />
        </Button>
      </header>

      {draftAvailable ? (
        <div className="flex items-center gap-1.5 border-b border-border/40 px-3 py-1.5">
          <SourceToggle
            label="Saved version"
            active={source === 'saved'}
            onClick={() => setSource('saved')}
          />
          <SourceToggle
            label="Unsaved canvas"
            active={source === 'draft'}
            onClick={() => setSource('draft')}
          />
          <div className="ml-auto">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-6 px-2 text-[11px]"
              onClick={handleRefresh}
              disabled={activeLoading}
            >
              {activeLoading ? (
                <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
              ) : (
                'Refresh'
              )}
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-1.5 border-b border-border/40 px-3 py-1.5">
          <Badge variant="secondary" className="h-5 px-1.5 text-[10px] font-medium">
            Saved version
          </Badge>
          <span className="truncate text-[11px] text-muted-foreground">{agentLabel}</span>
          <div className="ml-auto">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-6 px-2 text-[11px]"
              onClick={handleRefresh}
              disabled={activeLoading || !onRefreshActive}
            >
              {activeLoading ? (
                <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
              ) : (
                'Refresh'
              )}
            </Button>
          </div>
        </div>
      )}

      <ScrollArea className="min-h-0 flex-1">
        <div className="px-3 py-3">
          <PanelBody
            loading={activeLoading}
            errorMessage={activeError}
            preview={activePreview}
          />
        </div>
      </ScrollArea>
    </aside>
  )
}

function SourceToggle({
  label,
  active,
  onClick,
}: {
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        'rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors',
        active
          ? 'border-primary/40 bg-primary/10 text-foreground'
          : 'border-transparent text-muted-foreground hover:bg-muted/40',
      )}
    >
      {label}
    </button>
  )
}

function PanelBody({
  loading,
  errorMessage,
  preview,
}: {
  loading: boolean
  errorMessage: string | null
  preview: AgentDefinitionPreviewResponseDto | null
}) {
  if (loading && !preview) {
    return (
      <div className="flex items-center gap-2 py-6 text-muted-foreground">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        <span className="text-[12px]">Compiling effective runtime…</span>
      </div>
    )
  }
  if (errorMessage) {
    return (
      <div className="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-destructive">
        <p className="flex items-center gap-1.5 text-[11.5px] font-semibold">
          <ShieldAlert className="h-3 w-3" aria-hidden="true" />
          Preview failed
        </p>
        <p className="text-[11.5px] leading-relaxed">{errorMessage}</p>
      </div>
    )
  }
  if (!preview) {
    return (
      <p className="py-6 text-center text-[12px] text-muted-foreground">
        No preview available yet.
      </p>
    )
  }
  return <PreviewContent preview={preview} />
}

function PreviewContent({ preview }: { preview: AgentDefinitionPreviewResponseDto }) {
  const runtime = preview.effectiveRuntimePreview
  const validation = preview.validation

  return (
    <Tabs defaultValue="overview" className="gap-3">
      <TabsList className="grid h-8 w-full grid-cols-5 p-0.5">
        <TabsTrigger value="overview" className="h-7 text-[11px]">
          Overview
        </TabsTrigger>
        <TabsTrigger value="prompt" className="h-7 text-[11px]">
          Prompt
        </TabsTrigger>
        <TabsTrigger value="tools" className="h-7 text-[11px]">
          Tools
        </TabsTrigger>
        <TabsTrigger value="workflow" className="h-7 text-[11px]">
          Workflow
        </TabsTrigger>
        <TabsTrigger value="diagnostics" className="h-7 text-[11px]">
          Diagnostics
        </TabsTrigger>
      </TabsList>

      <TabsContent value="overview" className="space-y-3">
        <OverviewSection runtime={runtime} preview={preview} />
        <CapabilityExplanationsSection
          explanations={runtime.capabilityPermissionExplanations}
        />
        <AttachedSkillsSection injection={runtime.attachedSkillInjection} />
      </TabsContent>

      <TabsContent value="prompt" className="space-y-3">
        <PromptSection prompt={runtime.prompt} />
      </TabsContent>

      <TabsContent value="tools" className="space-y-3">
        <ToolsSection access={runtime.effectiveToolAccess} />
      </TabsContent>

      <TabsContent value="workflow" className="space-y-3">
        <WorkflowSection workflowStructure={runtime.policies.workflowStructure} />
      </TabsContent>

      <TabsContent value="diagnostics" className="space-y-3">
        <DiagnosticsSection
          validation={validation}
          graphCategories={runtime.graphValidation.categories}
        />
      </TabsContent>
    </Tabs>
  )
}

// ──────────────────────────────────────────────────────────────────────────
// Sections
// ──────────────────────────────────────────────────────────────────────────

function OverviewSection({
  runtime,
  preview,
}: {
  runtime: AgentEffectiveRuntimePreviewDto
  preview: AgentDefinitionPreviewResponseDto
}) {
  const def = runtime.definition
  const lifecycleLabel = getAgentDefinitionLifecycleLabel(
    def.lifecycleState as AgentDefinitionLifecycleStateDto,
  )
  const scopeLabel = preview.definition
    ? getAgentDefinitionScopeLabel(preview.definition.scope as AgentDefinitionScopeDto)
    : '—'
  const profileLabel = getAgentDefinitionBaseCapabilityLabel(
    def.baseCapabilityProfile as AgentDefinitionBaseCapabilityProfileDto,
  )

  const validationStatus = preview.validation.status

  return (
    <Section label="Definition">
      <dl className="grid grid-cols-2 gap-x-3 gap-y-2">
        <Meta label="Display name" value={def.displayName} />
        <Meta label="Version" value={`v${def.version}`} />
        <Meta label="Profile" value={profileLabel} />
        <Meta label="Runtime agent" value={def.runtimeAgentId} />
        <Meta label="Scope" value={scopeLabel} />
        <Meta label="Lifecycle" value={lifecycleLabel} />
      </dl>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <ValidationBadge status={validationStatus} />
        {runtime.attachedSkillInjection.attachmentCount > 0 ? (
          <Badge variant="outline" className="h-5 px-1.5 text-[10px]">
            {runtime.attachedSkillInjection.resolvedCount}/
            {runtime.attachedSkillInjection.attachmentCount} skills resolved
          </Badge>
        ) : null}
        <Badge variant="outline" className="h-5 px-1.5 text-[10px]">
          {runtime.effectiveToolAccess.allowedToolCount} allowed tools
        </Badge>
        {runtime.effectiveToolAccess.deniedCapabilityCount > 0 ? (
          <Badge variant="outline" className="h-5 px-1.5 text-[10px]">
            {runtime.effectiveToolAccess.deniedCapabilityCount} denied
          </Badge>
        ) : null}
      </div>
    </Section>
  )
}

function PromptSection({
  prompt,
}: {
  prompt: AgentEffectiveRuntimePreviewDto['prompt']
}) {
  const utilization = prompt.promptBudgetTokens
    ? Math.min(100, Math.round((prompt.estimatedPromptTokens / prompt.promptBudgetTokens) * 100))
    : 0

  return (
    <>
      <Section label="Compiled prompt">
        <dl className="grid grid-cols-2 gap-x-3 gap-y-2">
          <Meta label="Fragments" value={String(prompt.fragmentCount)} />
          <Meta
            label="Tokens"
            value={`${prompt.estimatedPromptTokens.toLocaleString()} / ${prompt.promptBudgetTokens.toLocaleString()}`}
          />
          <Meta label="Compiler" value={prompt.compiler} />
          <Meta label="Utilization" value={`${utilization}%`} />
        </dl>
        <p className="mt-2 truncate font-mono text-[10px] text-muted-foreground">
          sha256: {prompt.promptSha256}
        </p>
      </Section>

      <Section label="Fragments" count={prompt.fragments.length}>
        <ul className="space-y-1.5">
          {prompt.fragments
            .slice()
            .sort((a, b) => b.priority - a.priority)
            .map((fragment) => (
              <FragmentRow key={fragment.id} fragment={fragment} />
            ))}
        </ul>
      </Section>
    </>
  )
}

function FragmentRow({ fragment }: { fragment: AgentPreviewPromptFragmentDto }) {
  return (
    <li className="rounded-md border border-border/45 bg-background/40 px-2 py-1.5">
      <div className="flex items-center gap-2">
        <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
          {fragment.priority}
        </span>
        <span className="min-w-0 flex-1 truncate text-[11.5px] font-medium text-foreground">
          {fragment.title}
        </span>
        <span className="text-[10px] tabular-nums text-muted-foreground">
          {fragment.tokenEstimate.toLocaleString()}t
        </span>
      </div>
      <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground">
        {fragment.id} · {fragment.budgetPolicy}
      </p>
      <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground/80">
        {fragment.inclusionReason}
      </p>
    </li>
  )
}

function ToolsSection({
  access,
}: {
  access: AgentEffectiveRuntimePreviewDto['effectiveToolAccess']
}) {
  return (
    <>
      <Section label="Allowed" count={access.allowedTools.length}>
        {access.allowedTools.length === 0 ? (
          <Empty label="No tools allowed by the effective policy." />
        ) : (
          <ul className="space-y-1">
            {access.allowedTools.map((entry) => (
              <ToolRow key={entry.toolName} entry={entry} allowed />
            ))}
          </ul>
        )}
      </Section>
      <Section label="Denied" count={access.deniedCapabilities.length}>
        {access.deniedCapabilities.length === 0 ? (
          <Empty label="No tools denied — every host-available tool is reachable." />
        ) : (
          <ul className="space-y-1">
            {access.deniedCapabilities.map((entry) => (
              <ToolRow key={entry.toolName} entry={entry} allowed={false} />
            ))}
          </ul>
        )}
      </Section>
    </>
  )
}

function ToolRow({
  entry,
  allowed,
}: {
  entry: AgentPreviewToolAccessEntryDto
  allowed: boolean
}) {
  return (
    <li
      className={cn(
        'rounded-md border px-2 py-1.5',
        allowed ? 'border-border/45 bg-background/40' : 'border-destructive/30 bg-destructive/5',
      )}
    >
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] font-medium text-foreground">
          {entry.toolName}
        </span>
        <Badge
          variant="outline"
          className={cn(
            'h-4 px-1 text-[9.5px] uppercase tracking-wide',
            allowed ? '' : 'border-destructive/40 text-destructive',
          )}
        >
          {entry.effectClass}
        </Badge>
        <Badge variant="outline" className="h-4 px-1 text-[9.5px]">
          {entry.riskClass}
        </Badge>
      </div>
      <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground">
        {entry.description}
      </p>
      {!allowed && entry.deniedBy.length > 0 ? (
        <p className="mt-0.5 truncate text-[10.5px] text-destructive/80">
          Denied by: {entry.deniedBy.join(', ')}
        </p>
      ) : null}
    </li>
  )
}

function WorkflowSection({ workflowStructure }: { workflowStructure: unknown }) {
  const parsed = useMemo<CustomAgentWorkflowStructureDto | null>(() => {
    if (!workflowStructure || typeof workflowStructure !== 'object') return null
    const result = customAgentWorkflowStructureSchema.safeParse(workflowStructure)
    return result.success ? result.data : null
  }, [workflowStructure])

  if (!parsed) {
    return (
      <Section label="Workflow phases">
        <Empty label="No workflow state machine declared. Tools follow the base capability profile." />
      </Section>
    )
  }

  const startId = parsed.startPhaseId ?? parsed.phases[0]?.id

  return (
    <Section label="Workflow phases" count={parsed.phases.length}>
      <ul className="space-y-1.5">
        {parsed.phases.map((phase) => {
          const isStart = phase.id === startId
          return (
            <li
              key={phase.id}
              className="rounded-md border border-border/45 bg-background/40 px-2 py-1.5"
            >
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-[11.5px] font-medium text-foreground">
                  {phase.title}
                </span>
                {isStart ? (
                  <Badge variant="outline" className="h-4 px-1 text-[9.5px] uppercase">
                    Start
                  </Badge>
                ) : null}
              </div>
              <p className="mt-0.5 truncate font-mono text-[10.5px] text-muted-foreground">
                {phase.id}
              </p>
              {phase.allowedTools && phase.allowedTools.length > 0 ? (
                <p className="mt-0.5 text-[10.5px] text-muted-foreground/90">
                  Tools: {phase.allowedTools.join(', ')}
                </p>
              ) : null}
              {phase.requiredChecks && phase.requiredChecks.length > 0 ? (
                <ul className="mt-0.5 space-y-0.5">
                  {phase.requiredChecks.map((check, index) => (
                    <li key={index} className="text-[10.5px] text-muted-foreground/90">
                      ✓{' '}
                      {check.kind === 'todo_completed'
                        ? `todo:${check.todoId}`
                        : `tool:${check.toolName}${check.minCount ? ` ×${check.minCount}` : ''}`}
                    </li>
                  ))}
                </ul>
              ) : null}
              {phase.branches && phase.branches.length > 0 ? (
                <ul className="mt-0.5 space-y-0.5">
                  {phase.branches.map((branch, index) => (
                    <li key={index} className="text-[10.5px] text-muted-foreground/90">
                      → {branch.targetPhaseId} ({branch.condition.kind})
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          )
        })}
      </ul>
    </Section>
  )
}

function DiagnosticsSection({
  validation,
  graphCategories,
}: {
  validation: { status: string; diagnostics: AgentDefinitionValidationDiagnosticDto[] }
  graphCategories: { category: string; diagnostics: AgentPreviewGraphDiagnosticDto[] }[]
}) {
  const hasDefinition = validation.diagnostics.length > 0
  const flatGraph = graphCategories.flatMap((category) =>
    category.diagnostics.map((diagnostic) => ({ category: category.category, diagnostic })),
  )

  if (!hasDefinition && flatGraph.length === 0) {
    return (
      <div className="flex items-center gap-2 rounded-md border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-emerald-600 dark:text-emerald-400">
        <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
        <span className="text-[11.5px] font-medium">No diagnostics — runtime is consistent.</span>
      </div>
    )
  }

  return (
    <>
      {hasDefinition ? (
        <Section label="Definition" count={validation.diagnostics.length}>
          <ul className="space-y-1">
            {validation.diagnostics.map((diagnostic, index) => (
              <DiagnosticRow
                key={`${diagnostic.code}-${index}`}
                code={diagnostic.code}
                path={diagnostic.path}
                message={diagnostic.message}
                repairHint={diagnostic.repairHint ?? null}
              />
            ))}
          </ul>
        </Section>
      ) : null}
      {flatGraph.length > 0 ? (
        <Section label="Graph" count={flatGraph.length}>
          <ul className="space-y-1">
            {flatGraph.map(({ category, diagnostic }, index) => (
              <DiagnosticRow
                key={`${diagnostic.code}-${index}`}
                code={diagnostic.code}
                path={diagnostic.path}
                message={diagnostic.message}
                repairHint={diagnostic.repairHint}
                category={category}
              />
            ))}
          </ul>
        </Section>
      ) : null}
    </>
  )
}

export function diagnosticToRow(
  diagnostic: AgentDefinitionValidationDiagnosticDto | AgentPreviewGraphDiagnosticDto,
  category?: string,
) {
  return {
    code: diagnostic.code,
    path: diagnostic.path,
    message: diagnostic.message,
    repairHint: diagnostic.repairHint ?? null,
    category: category ?? null,
  }
}

function DiagnosticRow({
  code,
  path,
  message,
  repairHint,
  category,
}: {
  code: string
  path: string
  message: string
  repairHint?: string | null
  category?: string
}) {
  return (
    <li className="rounded-md border border-amber-500/30 bg-amber-500/5 px-2 py-1.5">
      <div className="flex items-center gap-2">
        <AlertTriangle className="h-3 w-3 text-amber-600 dark:text-amber-400" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate font-mono text-[10.5px] font-medium text-foreground">
          {code}
        </span>
        {category ? (
          <Badge variant="outline" className="h-4 px-1 text-[9.5px] uppercase">
            {category}
          </Badge>
        ) : null}
      </div>
      <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground">{path}</p>
      <p className="mt-0.5 text-[11px] text-foreground/85">{message}</p>
      {repairHint ? (
        <p className="mt-0.5 text-[10.5px] text-muted-foreground/90">Repair: {repairHint}</p>
      ) : null}
    </li>
  )
}

function CapabilityExplanationsSection({
  explanations,
}: {
  explanations: CapabilityPermissionExplanationDto[]
}) {
  if (explanations.length === 0) return null
  return (
    <Section label="Capabilities" count={explanations.length}>
      <ul className="space-y-1">
        {explanations.map((explanation) => (
          <li
            key={`${explanation.subjectKind}:${explanation.subjectId}`}
            className="rounded-md border border-border/45 bg-background/40 px-2 py-1.5"
          >
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground">
                {explanation.subjectId}
              </span>
              <Badge variant="outline" className="h-4 px-1 text-[9.5px] uppercase">
                {explanation.riskClass}
              </Badge>
              {explanation.confirmationRequired ? (
                <Badge variant="outline" className="h-4 px-1 text-[9.5px] uppercase">
                  Confirm
                </Badge>
              ) : null}
            </div>
            <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground">
              {explanation.subjectKind}
            </p>
            <p className="mt-0.5 text-[11px] text-foreground/85">{explanation.summary}</p>
          </li>
        ))}
      </ul>
    </Section>
  )
}

function AttachedSkillsSection({
  injection,
}: {
  injection: AgentEffectiveRuntimePreviewDto['attachedSkillInjection']
}) {
  if (injection.attachmentCount === 0) return null
  return (
    <Section label="Attached skills" count={injection.attachmentCount}>
      <ul className="space-y-1">
        {injection.entries.map((entry) => (
          <AttachedSkillRow key={entry.attachmentId} entry={entry} />
        ))}
      </ul>
    </Section>
  )
}

function AttachedSkillRow({
  entry,
}: {
  entry: AgentPreviewAttachedSkillInjectionEntryDto
}) {
  const tone =
    entry.status === 'resolved'
      ? 'border-border/45 bg-background/40'
      : entry.status === 'stale'
      ? 'border-amber-500/30 bg-amber-500/5'
      : 'border-destructive/30 bg-destructive/5'
  return (
    <li className={cn('rounded-md border px-2 py-1.5', tone)}>
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground">
          {entry.name}
        </span>
        <Badge variant="outline" className="h-4 px-1 text-[9.5px] uppercase">
          {entry.status}
        </Badge>
      </div>
      <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground">{entry.skillId}</p>
      <p className="mt-0.5 text-[11px] text-foreground/85">{entry.explanation}</p>
    </li>
  )
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
  children: React.ReactNode
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
      <div>{children}</div>
    </section>
  )
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 space-y-0.5">
      <dt className="text-[9.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground/70">
        {label}
      </dt>
      <dd className="truncate text-[12px] text-foreground/95">{value || '—'}</dd>
    </div>
  )
}

function Empty({ label }: { label: string }) {
  return <p className="text-[11.5px] text-muted-foreground">{label}</p>
}

function ValidationBadge({ status }: { status: string }) {
  if (status === 'valid') {
    return (
      <Badge
        variant="outline"
        className="h-5 border-emerald-500/40 bg-emerald-500/10 px-1.5 text-[10px] text-emerald-700 dark:text-emerald-300"
      >
        <CheckCircle2 className="mr-1 h-3 w-3" aria-hidden="true" />
        Valid
      </Badge>
    )
  }
  return (
    <Badge
      variant="outline"
      className="h-5 border-destructive/40 bg-destructive/10 px-1.5 text-[10px] text-destructive"
    >
      <ShieldAlert className="mr-1 h-3 w-3" aria-hidden="true" />
      Invalid
    </Badge>
  )
}
