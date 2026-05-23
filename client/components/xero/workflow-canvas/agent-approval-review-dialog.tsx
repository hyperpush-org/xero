import { useEffect, useMemo, useState } from 'react'
import { ArrowRight, ChevronDown, Loader2, ShieldCheck, Sparkles } from 'lucide-react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
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
  outputContract: 'Response format',
  databaseAccess: 'Database touchpoints',
  consumedArtifacts: 'Consumed artifacts',
  workflowStructure: 'Stages',
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
  const [detailsOpen, setDetailsOpen] = useState(false)
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
    ? 'Confirm this new agent before it becomes available to you.'
    : !review?.changed
      ? 'This save creates a new version without behavior changes.'
      : 'Saving creates a new immutable version for future runs.'
  const changedSectionLabels = review
    ? orderedSections
        .filter((section) => section.changed)
        .map((section) => sectionLabel(section.section))
    : []

  useEffect(() => {
    setDetailsOpen(false)
  }, [open, review?.definitionId, review?.toVersion])

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onCancel()
      }}
    >
      <DialogContent className="max-w-lg gap-0 overflow-hidden p-0">
        <DialogHeader className="border-b border-border/50 p-5 pb-4">
          <DialogTitle className="flex items-center gap-2 text-[15px]">
            <span className="flex size-7 items-center justify-center rounded-md border border-primary/20 bg-primary/10">
              <HeaderIcon className="h-4 w-4 text-primary" aria-hidden="true" />
            </span>
            {title}
          </DialogTitle>
          <DialogDescription className="text-[13px] leading-5">
            {description}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3 px-5 py-4">
          {!review ? (
            <p className="text-[13px] text-muted-foreground">
              Loading approval review…
            </p>
          ) : (
            <>
              <div className="rounded-md border border-border/60 bg-secondary/10 p-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate font-mono text-[13px] text-foreground">
                      {review.definitionId}
                    </p>
                    <p className="mt-1 text-[12px] text-muted-foreground">
                      {review.isInitialVersion ? 'New user agent' : 'Agent update'}
                    </p>
                  </div>
                  <span className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[11.5px] text-muted-foreground">
                    {review.isInitialVersion ? (
                      <>
                        <Sparkles className="h-3 w-3" aria-hidden="true" />v
                        {review.toVersion}
                      </>
                    ) : (
                      <>
                        v{review.fromVersion ?? '?'}
                        <ArrowRight className="h-3 w-3" aria-hidden="true" />v
                        {review.toVersion}
                      </>
                    )}
                  </span>
                </div>
              </div>

              {!review.changed && !review.isInitialVersion ? (
                <ZeroDeltaNotice />
              ) : (
                <div className="rounded-md border border-border/60 bg-background/40 px-3 py-2.5">
                  <p className="text-[12.5px] font-medium text-foreground">
                    {review.isInitialVersion
                      ? 'This will save the current canvas as a new agent.'
                      : 'This will save the current canvas as the next agent version.'}
                  </p>
                  <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
                    {changedSectionLabels.length > 0
                      ? `${changedSectionLabels.length} configuration area${
                          changedSectionLabels.length === 1 ? '' : 's'
                        } will be saved.`
                      : 'No behavior changes were detected.'}
                  </p>
                </div>
              )}

              <Collapsible open={detailsOpen} onOpenChange={setDetailsOpen}>
                <CollapsibleTrigger
                  className={cn(
                    'inline-flex items-center gap-1.5 self-start rounded-md text-[12.5px] font-medium',
                    'text-muted-foreground transition-colors hover:text-foreground',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  )}
                >
                  <ChevronDown
                    className={cn(
                      'h-3.5 w-3.5 transition-transform motion-fast',
                      detailsOpen ? 'rotate-0' : '-rotate-90',
                    )}
                    aria-hidden="true"
                  />
                  {detailsOpen ? 'Hide' : 'Show'} technical details
                </CollapsibleTrigger>
                <CollapsibleContent className="mt-3">
                  <ScrollArea className="h-[min(30vh,18rem)] rounded-md border border-border/60 bg-background/40">
                    <ul className="flex flex-col gap-2 p-3">
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
                  </ScrollArea>
                </CollapsibleContent>
              </Collapsible>
            </>
          )}
        </div>

        {errorMessage ? (
          <div className="border-t border-destructive/30 bg-destructive/[0.06] px-5 py-2 text-[12px] text-destructive">
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
        This will create a new version with the same saved behavior as the
        active one.
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
