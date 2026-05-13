import { useMemo } from 'react'
import { ArrowRight, Loader2, ShieldCheck, Sparkles } from 'lucide-react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import type { AgentDefinitionPreSaveReviewDto } from '@/src/lib/xero-model/agent-definition'

interface AgentApprovalReviewDialogProps {
  open: boolean
  review: AgentDefinitionPreSaveReviewDto | null
  busy: boolean
  errorMessage: string | null
  onApprove: () => void
  onCancel: () => void
}

const SECTION_LABELS: Record<string, string> = {
  identity: 'Identity',
  prompts: 'Prompts',
  attachedSkills: 'Attached skills',
  toolPolicy: 'Tool policy',
  memoryPolicy: 'Memory policy',
  retrievalPolicy: 'Retrieval policy',
  handoffPolicy: 'Handoff policy',
  outputContract: 'Output contract',
  databaseAccess: 'Database touchpoints',
  consumedArtifacts: 'Consumed artifacts',
  workflowStructure: 'Workflow',
  safetyLimits: 'Safety limits',
}

function sectionLabel(section: string): string {
  return SECTION_LABELS[section] ?? section
}

function formatJsonValue(value: unknown): string {
  if (value === null || value === undefined) {
    return '—'
  }
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

export function AgentApprovalReviewDialog({
  open,
  review,
  busy,
  errorMessage,
  onApprove,
  onCancel,
}: AgentApprovalReviewDialogProps) {
  const orderedSections = useMemo(() => {
    if (!review) return []
    const order = Object.keys(SECTION_LABELS)
    return [...review.sections].sort((a, b) => {
      const ai = order.indexOf(a.section)
      const bi = order.indexOf(b.section)
      if (ai === -1 && bi === -1) return a.section.localeCompare(b.section)
      if (ai === -1) return 1
      if (bi === -1) return -1
      return ai - bi
    })
  }, [review])

  const headerIcon = review?.isInitialVersion ? Sparkles : ShieldCheck
  const HeaderIcon = headerIcon
  const title = review?.isInitialVersion
    ? 'Approve new agent definition'
    : 'Approve update to this agent'
  const description = review?.isInitialVersion
    ? 'No prior version exists. Confirm the configuration this new agent will save with.'
    : !review?.changed
      ? 'This update is byte-equivalent to the active version — saving will create a new version with no behavior changes.'
      : 'Review the changes before saving. Saving creates a new immutable version pinned to future runs.'

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onCancel()
      }}
    >
      <DialogContent className="max-w-3xl gap-0 p-0">
        <DialogHeader className="border-b border-border/50 p-4">
          <DialogTitle className="flex items-center gap-2 text-[14px]">
            <HeaderIcon className="h-4 w-4 text-primary" aria-hidden="true" />
            {title}
          </DialogTitle>
          <DialogDescription className="text-[12.5px]">
            {description}
          </DialogDescription>
          {review ? (
            <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11.5px] text-muted-foreground">
              <span className="font-mono">{review.definitionId}</span>
              {review.isInitialVersion ? (
                <span className="inline-flex items-center gap-1">
                  <Sparkles className="h-3 w-3" aria-hidden="true" />v
                  {review.toVersion}
                </span>
              ) : (
                <span className="inline-flex items-center gap-1">
                  v{review.fromVersion ?? '?'}
                  <ArrowRight className="h-3 w-3" aria-hidden="true" />v
                  {review.toVersion}
                </span>
              )}
              <span aria-live="polite">
                {review.changed
                  ? `${review.changedSections.length} section${review.changedSections.length === 1 ? '' : 's'} changed`
                  : 'No changes'}
              </span>
            </div>
          ) : null}
        </DialogHeader>

        <ScrollArea className="max-h-[60vh]">
          <div className="px-4 py-3">
            {!review ? (
              <p className="text-[12.5px] text-muted-foreground">
                Loading approval review…
              </p>
            ) : !review.changed && !review.isInitialVersion ? (
              <ZeroDeltaNotice />
            ) : (
              <ul className="flex flex-col gap-2">
                {orderedSections.map((section) => (
                  <li
                    key={section.section}
                    className={cn(
                      'rounded-md border px-2.5 py-2',
                      section.changed
                        ? 'border-warning/30 bg-warning/[0.05]'
                        : 'border-border/40 bg-secondary/10',
                    )}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <span className="text-[12px] font-medium text-foreground">
                        {sectionLabel(section.section)}
                      </span>
                      <span className="text-[11px] text-muted-foreground">
                        {section.changed
                          ? review.isInitialVersion
                            ? 'added'
                            : 'changed'
                          : 'unchanged'}
                      </span>
                    </div>
                    {section.changed ? (
                      <DiffFieldList
                        fields={section.fields}
                        before={section.before}
                        after={section.after}
                        initialVersion={review.isInitialVersion}
                      />
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </ScrollArea>

        {errorMessage ? (
          <div className="border-t border-destructive/30 bg-destructive/[0.06] px-4 py-2 text-[12px] text-destructive">
            {errorMessage}
          </div>
        ) : null}

        <DialogFooter className="border-t border-border/50 bg-background/40 p-3">
          <Button
            variant="ghost"
            onClick={onCancel}
            disabled={busy}
            className="h-8 text-[12.5px]"
          >
            Cancel
          </Button>
          <Button
            onClick={onApprove}
            disabled={busy || !review}
            className="h-8 gap-1.5 text-[12.5px]"
          >
            {busy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            ) : null}
            {review?.isInitialVersion
              ? 'Approve and save'
              : !review?.changed
                ? 'Approve no-change save'
                : 'Approve and save changes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ZeroDeltaNotice() {
  return (
    <div className="rounded-md border border-border/40 bg-secondary/10 px-3 py-3">
      <p className="text-[12.5px] font-medium text-foreground">
        No changes detected
      </p>
      <p className="mt-1 text-[11.5px] text-muted-foreground">
        Saving this update will create a new version with the same canonical
        snapshot as the active one. The audit trail will record the operator
        approval, but no behavior changes for live runs.
      </p>
    </div>
  )
}

interface DiffFieldListProps {
  fields: string[]
  before: Record<string, unknown>
  after: Record<string, unknown>
  initialVersion: boolean
}

function DiffFieldList({
  fields,
  before,
  after,
  initialVersion,
}: DiffFieldListProps) {
  const changedFields = fields.filter(
    (field) => formatJsonValue(before[field]) !== formatJsonValue(after[field]),
  )
  const visibleFields = changedFields.length > 0 ? changedFields : fields

  return (
    <div className="mt-2 flex flex-col gap-2">
      {visibleFields.map((field) => (
        <div key={field} className="flex flex-col gap-1">
          <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
            {field}
          </span>
          {initialVersion ? (
            <DiffPane tone="after" value={after[field]} />
          ) : (
            <div className="grid gap-1.5 sm:grid-cols-2">
              <DiffPane tone="before" value={before[field]} />
              <DiffPane tone="after" value={after[field]} />
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

interface DiffPaneProps {
  tone: 'before' | 'after'
  value: unknown
}

function DiffPane({ tone, value }: DiffPaneProps) {
  return (
    <div
      className={cn(
        'rounded-sm border px-2 py-1.5',
        tone === 'before'
          ? 'border-destructive/20 bg-destructive/[0.04]'
          : 'border-success/20 bg-success/[0.04]',
      )}
    >
      <span
        className={cn(
          'block text-[10px] font-semibold uppercase tracking-wide',
          tone === 'before' ? 'text-destructive' : 'text-success',
        )}
      >
        {tone === 'before' ? 'Before' : 'After'}
      </span>
      <pre className="mt-0.5 max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-[1.45] text-foreground/80">
        {formatJsonValue(value)}
      </pre>
    </div>
  )
}
