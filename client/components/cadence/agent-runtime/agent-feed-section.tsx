import { AlertCircle, LoaderCircle } from 'lucide-react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RuntimeStreamIssueView,
  RuntimeStreamStatus,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'

import {
  displayValue,
  formatSequence,
  formatSkillSource,
  formatSkillTreeHash,
  getSkillCacheLabel,
  getSkillResultBadgeVariant,
  getSkillResultLabel,
  getSkillStageLabel,
  getStreamBadgeVariant,
  getStreamStatusMeta,
  getToolStateBadgeVariant,
  getToolStateLabel,
} from './helpers'

interface AgentFeedSectionProps {
  streamStatusMeta: ReturnType<typeof getStreamStatusMeta>
  streamStatusLabel: string
  streamStatus: RuntimeStreamStatus
  recentRunReplacement: {
    previousRunId: string
    nextRunId: string
  } | null
  showNoRunStreamBanner: boolean
  streamIssue: RuntimeStreamIssueView | null
  transcriptItems: RuntimeStreamView['transcriptItems']
  activityItems: AgentPaneView['activityItems']
  toolCalls: RuntimeStreamView['toolCalls']
  skillItems: AgentPaneView['skillItems']
  streamRunId: string
  streamSequenceLabel: string
  streamSessionLabel: string
  messagesUnavailableReason: string
}

export function AgentFeedSection({
  streamStatusMeta,
  streamStatusLabel,
  streamStatus,
  recentRunReplacement,
  showNoRunStreamBanner,
  streamIssue,
  transcriptItems,
  activityItems,
  toolCalls,
  skillItems,
  streamRunId,
  streamSequenceLabel,
  streamSessionLabel,
  messagesUnavailableReason,
}: AgentFeedSectionProps) {
  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Agent feed</p>
            <h2 className="mt-2 text-lg font-semibold text-foreground">{streamStatusMeta.title}</h2>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={streamStatusMeta.badgeVariant}>{streamStatusLabel}</Badge>
            <Badge variant={getStreamBadgeVariant(streamStatus)}>
              {streamStatus === 'replaying' ? 'Replaying recent activity' : streamStatusLabel}
            </Badge>
          </div>
        </div>

        <p className="text-sm leading-6 text-muted-foreground">{streamStatusMeta.body}</p>

        {recentRunReplacement ? (
          <Alert>
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Switched to a new supervised run</AlertTitle>
            <AlertDescription>
              <p>
                {recentRunReplacement.previousRunId} → {recentRunReplacement.nextRunId}
              </p>
            </AlertDescription>
          </Alert>
        ) : null}

        {streamStatus === 'subscribing' ? (
          <Alert>
            <LoaderCircle className="h-4 w-4 animate-spin" />
            <AlertTitle>Connecting to the live transcript</AlertTitle>
            <AlertDescription>{messagesUnavailableReason}</AlertDescription>
          </Alert>
        ) : null}

        {streamStatus === 'replaying' ? (
          <Alert>
            <LoaderCircle className="h-4 w-4 animate-spin" />
            <AlertTitle>Replaying recent run-scoped backlog</AlertTitle>
            <AlertDescription>{messagesUnavailableReason}</AlertDescription>
          </Alert>
        ) : null}

        {showNoRunStreamBanner ? (
          <FeedEmptyState
            body="Start or reconnect a supervised run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project."
            title="No supervised run is attached"
          />
        ) : null}

        {streamIssue ? (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Live feed issue</AlertTitle>
            <AlertDescription>
              <p>{streamIssue.message}</p>
              <p className="font-mono text-[11px] text-destructive/80">code: {streamIssue.code}</p>
            </AlertDescription>
          </Alert>
        ) : null}

        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          <div className="rounded-xl border border-border/70 bg-card/70 p-4">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-base font-semibold text-foreground">Transcript</h3>
              <Badge variant="outline">{streamRunId}</Badge>
            </div>
            <div className="mt-4 space-y-3">
              {transcriptItems.length > 0 ? (
                transcriptItems.map((item) => (
                  <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{formatSequence(item.sequence)}</span>
                      <span>{item.runId}</span>
                    </div>
                    <p className="mt-2 text-sm leading-6 text-foreground/90">{item.text}</p>
                  </div>
                ))
              ) : (
                <FeedEmptyState body="Cadence has not received transcript lines for this run yet." title="No transcript yet" />
              )}
            </div>
          </div>

          <div className="rounded-xl border border-border/70 bg-card/70 p-4">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-base font-semibold text-foreground">Runtime activity</h3>
              <Badge variant="outline">{streamSequenceLabel}</Badge>
            </div>
            <div className="mt-4 space-y-3">
              {activityItems.length > 0 ? (
                activityItems.map((item) => (
                  <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{formatSequence(item.sequence)}</span>
                      <span>{item.runId}</span>
                    </div>
                    <p className="mt-2 text-sm font-medium text-foreground">{item.title}</p>
                    <p className="mt-1 text-sm leading-6 text-muted-foreground">
                      {displayValue(item.detail, 'Cadence recorded this activity without additional detail.')}
                    </p>
                  </div>
                ))
              ) : (
                <FeedEmptyState body="Cadence has not recorded any runtime activity rows for this run yet." title="No runtime activity yet" />
              )}
            </div>
          </div>

          <div className="rounded-xl border border-border/70 bg-card/70 p-4">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-base font-semibold text-foreground">Tool lane</h3>
              <Badge variant="outline">{streamSessionLabel}</Badge>
            </div>
            <div className="mt-4 space-y-3">
              {toolCalls.length > 0 ? (
                toolCalls.map((item) => (
                  <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{formatSequence(item.sequence)}</span>
                      <span>{item.runId}</span>
                      <Badge variant={getToolStateBadgeVariant(item.toolState)}>{getToolStateLabel(item.toolState)}</Badge>
                    </div>
                    <p className="mt-2 text-sm font-medium text-foreground">{item.toolName}</p>
                    <p className="mt-1 text-sm leading-6 text-muted-foreground">
                      {displayValue(item.detail, 'Cadence has not recorded tool detail for this call yet.')}
                    </p>
                  </div>
                ))
              ) : (
                <FeedEmptyState body="Cadence has not observed any tool calls for this run yet." title="No tool calls yet" />
              )}
            </div>
          </div>

          <div className="rounded-xl border border-border/70 bg-card/70 p-4">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-base font-semibold text-foreground">Skill lane</h3>
              <Badge variant="outline">
                {skillItems.length} item{skillItems.length === 1 ? '' : 's'}
              </Badge>
            </div>
            <div className="mt-4 space-y-3">
              {skillItems.length > 0 ? (
                skillItems.map((item) => (
                  <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                      <span>{formatSequence(item.sequence)}</span>
                      <span>{item.runId}</span>
                      <Badge variant="outline">{getSkillStageLabel(item.stage)}</Badge>
                      <Badge variant={getSkillResultBadgeVariant(item.result)}>{getSkillResultLabel(item.result)}</Badge>
                      {item.cacheStatus ? <Badge variant="secondary">{getSkillCacheLabel(item.cacheStatus)}</Badge> : null}
                    </div>
                    <p className="mt-2 text-sm font-medium text-foreground">{item.skillId}</p>
                    <p className="mt-1 text-sm leading-6 text-muted-foreground">{item.detail}</p>
                    <div className="mt-3 space-y-1 text-[11px] text-muted-foreground">
                      <p>{formatSkillSource(item)}</p>
                      <p className="font-mono text-[10px]">tree {formatSkillTreeHash(item)}</p>
                    </div>
                    {item.diagnostic ? (
                      <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/5 px-2 py-2 text-[11px] text-destructive/90">
                        <p className="font-medium">{item.diagnostic.message}</p>
                        <p className="mt-1 font-mono">
                          code: {item.diagnostic.code}
                          {item.diagnostic.retryable ? ' · retryable' : ' · terminal'}
                        </p>
                      </div>
                    ) : null}
                  </div>
                ))
              ) : (
                <FeedEmptyState
                  body="Cadence has not observed any skill discovery, install, or invoke lifecycle rows for this run yet."
                  title="No skill activity yet"
                />
              )}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}

function FeedEmptyState({
  title,
  body,
}: {
  title: string
  body: string
}) {
  return (
    <div className="rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
      <p className="font-medium text-foreground/85">{title}</p>
      <p className="mt-1 leading-6">{body}</p>
    </div>
  )
}
