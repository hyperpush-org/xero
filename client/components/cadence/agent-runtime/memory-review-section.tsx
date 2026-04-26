"use client"

import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Brain,
  CheckCircle2,
  Eye,
  EyeOff,
  Filter,
  Loader2,
  RefreshCw,
  Trash2,
  WandSparkles,
  XCircle,
} from 'lucide-react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { AgentSessionView } from '@/src/lib/cadence-model'
import type {
  DeleteSessionMemoryRequestDto,
  ExtractSessionMemoryCandidatesRequestDto,
  ExtractSessionMemoryCandidatesResponseDto,
  ListSessionMemoriesRequestDto,
  ListSessionMemoriesResponseDto,
  SessionMemoryKindDto,
  SessionMemoryRecordDto,
  SessionMemoryReviewStateDto,
  SessionMemoryScopeDto,
  UpdateSessionMemoryRequestDto,
} from '@/src/lib/cadence-model/session-context'

interface MemoryReviewSectionProps {
  projectId: string
  selectedSession: AgentSessionView | null
  runId?: string | null
  onListSessionMemories?: (
    request: ListSessionMemoriesRequestDto,
  ) => Promise<ListSessionMemoriesResponseDto>
  onExtractSessionMemoryCandidates?: (
    request: ExtractSessionMemoryCandidatesRequestDto,
  ) => Promise<ExtractSessionMemoryCandidatesResponseDto>
  onUpdateSessionMemory?: (request: UpdateSessionMemoryRequestDto) => Promise<SessionMemoryRecordDto>
  onDeleteSessionMemory?: (request: DeleteSessionMemoryRequestDto) => Promise<void>
  onContextRefresh?: () => Promise<void>
}

type LoadStatus = 'idle' | 'loading' | 'ready' | 'error'
type MutationStatus = 'idle' | 'running'
type ScopeFilter = 'all' | SessionMemoryScopeDto
type KindFilter = 'all' | SessionMemoryKindDto

const KIND_OPTIONS: Array<{ value: KindFilter; label: string }> = [
  { value: 'all', label: 'All kinds' },
  { value: 'project_fact', label: 'Project facts' },
  { value: 'decision', label: 'Decisions' },
  { value: 'user_preference', label: 'Preferences' },
  { value: 'troubleshooting', label: 'Troubleshooting' },
  { value: 'session_summary', label: 'Summaries' },
]

const SCOPE_OPTIONS: Array<{ value: ScopeFilter; label: string }> = [
  { value: 'all', label: 'All scopes' },
  { value: 'project', label: 'Project' },
  { value: 'session', label: 'Session' },
]

export function MemoryReviewSection({
  projectId,
  selectedSession,
  runId,
  onListSessionMemories,
  onExtractSessionMemoryCandidates,
  onUpdateSessionMemory,
  onDeleteSessionMemory,
  onContextRefresh,
}: MemoryReviewSectionProps) {
  const agentSessionId = selectedSession?.agentSessionId ?? null
  const [memories, setMemories] = useState<SessionMemoryRecordDto[]>([])
  const [status, setStatus] = useState<LoadStatus>('idle')
  const [mutationStatus, setMutationStatus] = useState<MutationStatus>('idle')
  const [message, setMessage] = useState<{ kind: 'success' | 'error'; text: string } | null>(null)
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>('all')
  const [kindFilter, setKindFilter] = useState<KindFilter>('all')

  const canLoad = Boolean(projectId && agentSessionId && onListSessionMemories)
  const loadMemories = useCallback(async () => {
    if (!projectId || !agentSessionId || !onListSessionMemories) return
    setStatus('loading')
    setMessage(null)
    try {
      const response = await onListSessionMemories({
        projectId,
        agentSessionId,
        includeDisabled: true,
        includeRejected: false,
      })
      setMemories(response.memories)
      setStatus('ready')
    } catch (error) {
      setStatus('error')
      setMessage({
        kind: 'error',
        text: getErrorMessage(error, 'Cadence could not load reviewed memory.'),
      })
    }
  }, [agentSessionId, onListSessionMemories, projectId])

  useEffect(() => {
    if (!canLoad) {
      setMemories([])
      setStatus('idle')
      return
    }
    void loadMemories()
  }, [canLoad, loadMemories])

  const filteredMemories = useMemo(
    () =>
      memories.filter((memory) => {
        const scopeMatches = scopeFilter === 'all' || memory.scope === scopeFilter
        const kindMatches = kindFilter === 'all' || memory.kind === kindFilter
        return scopeMatches && kindMatches
      }),
    [kindFilter, memories, scopeFilter],
  )
  const approvedCount = memories.filter((memory) => memory.reviewState === 'approved').length
  const candidateCount = memories.filter((memory) => memory.reviewState === 'candidate').length
  const disabledCount = memories.filter((memory) => memory.reviewState === 'approved' && !memory.enabled).length

  const handleExtract = useCallback(async () => {
    if (!projectId || !agentSessionId || !onExtractSessionMemoryCandidates) return
    setMutationStatus('running')
    setMessage(null)
    try {
      const response = await onExtractSessionMemoryCandidates({
        projectId,
        agentSessionId,
        runId: runId ?? null,
      })
      setMemories(response.memories)
      const skipped = response.skippedDuplicateCount > 0 ? ` ${response.skippedDuplicateCount} duplicate skipped.` : ''
      const rejected = response.rejectedCount > 0 ? ` ${response.rejectedCount} weak or unsafe rejected.` : ''
      setMessage({
        kind: 'success',
        text: `Cadence proposed ${response.createdCount.toLocaleString()} memory candidate${response.createdCount === 1 ? '' : 's'}.${skipped}${rejected}`.trim(),
      })
    } catch (error) {
      setMessage({
        kind: 'error',
        text: getErrorMessage(error, 'Cadence could not extract memory candidates.'),
      })
    } finally {
      setMutationStatus('idle')
    }
  }, [agentSessionId, onExtractSessionMemoryCandidates, projectId, runId])

  const handleUpdate = useCallback(
    async (
      memory: SessionMemoryRecordDto,
      request: Pick<UpdateSessionMemoryRequestDto, 'reviewState' | 'enabled'>,
    ) => {
      if (!onUpdateSessionMemory) return
      setMutationStatus('running')
      setMessage(null)
      try {
        const updated = await onUpdateSessionMemory({
          projectId: memory.projectId,
          memoryId: memory.memoryId,
          ...request,
        })
        setMemories((current) =>
          current.map((item) => (item.memoryId === updated.memoryId ? updated : item)),
        )
        setMessage({ kind: 'success', text: memoryActionMessage(updated) })
        await onContextRefresh?.()
      } catch (error) {
        setMessage({
          kind: 'error',
          text: getErrorMessage(error, 'Cadence could not update memory.'),
        })
      } finally {
        setMutationStatus('idle')
      }
    },
    [onContextRefresh, onUpdateSessionMemory],
  )

  const handleDelete = useCallback(
    async (memory: SessionMemoryRecordDto) => {
      if (!onDeleteSessionMemory) return
      setMutationStatus('running')
      setMessage(null)
      try {
        await onDeleteSessionMemory({
          projectId: memory.projectId,
          memoryId: memory.memoryId,
        })
        setMemories((current) => current.filter((item) => item.memoryId !== memory.memoryId))
        setMessage({ kind: 'success', text: 'Memory deleted.' })
        await onContextRefresh?.()
      } catch (error) {
        setMessage({
          kind: 'error',
          text: getErrorMessage(error, 'Cadence could not delete memory.'),
        })
      } finally {
        setMutationStatus('idle')
      }
    },
    [onContextRefresh, onDeleteSessionMemory],
  )

  if (!onListSessionMemories || !agentSessionId) {
    return null
  }

  return (
    <div className="rounded-lg border border-border/60 bg-muted/10 p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Brain className="h-4 w-4 text-muted-foreground" />
            <p className="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              Memory
            </p>
          </div>
          <p className="mt-1 text-sm font-semibold text-foreground">
            {approvedCount.toLocaleString()} approved · {candidateCount.toLocaleString()} candidate{candidateCount === 1 ? '' : 's'}
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {disabledCount.toLocaleString()} disabled approved item{disabledCount === 1 ? '' : 's'}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={status === 'loading' || mutationStatus === 'running'}
            onClick={() => void loadMemories()}
          >
            {status === 'loading' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
            Refresh
          </Button>
          {onExtractSessionMemoryCandidates ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!runId || mutationStatus === 'running'}
              onClick={() => void handleExtract()}
            >
              {mutationStatus === 'running' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <WandSparkles className="h-3.5 w-3.5" />}
              Extract
            </Button>
          ) : null}
        </div>
      </div>

      {message ? (
        <Alert variant={message.kind === 'error' ? 'destructive' : 'default'} className="mt-3">
          {message.kind === 'error' ? <XCircle className="h-4 w-4" /> : <CheckCircle2 className="h-4 w-4" />}
          <AlertTitle>{message.kind === 'error' ? 'Memory action failed' : 'Memory updated'}</AlertTitle>
          <AlertDescription>{message.text}</AlertDescription>
        </Alert>
      ) : null}

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Filter className="h-3.5 w-3.5 text-muted-foreground" />
        <Select value={scopeFilter} onValueChange={(value) => setScopeFilter(value as ScopeFilter)}>
          <SelectTrigger size="sm" className="w-[138px]" aria-label="Memory scope filter">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SCOPE_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={kindFilter} onValueChange={(value) => setKindFilter(value as KindFilter)}>
          <SelectTrigger size="sm" className="w-[168px]" aria-label="Memory kind filter">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {KIND_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="mt-3 divide-y divide-border/60 overflow-hidden rounded-lg border border-border/60 bg-background/40">
        {status === 'loading' && memories.length === 0 ? (
          <p className="px-3 py-3 text-sm text-muted-foreground">Loading reviewed memory...</p>
        ) : filteredMemories.length > 0 ? (
          filteredMemories.map((memory) => (
            <MemoryRow
              key={memory.memoryId}
              memory={memory}
              mutationRunning={mutationStatus === 'running'}
              onApprove={() => void handleUpdate(memory, { reviewState: 'approved', enabled: true })}
              onReject={() => void handleUpdate(memory, { reviewState: 'rejected', enabled: false })}
              onDisable={() => void handleUpdate(memory, { enabled: false })}
              onEnable={() => void handleUpdate(memory, { enabled: true })}
              onDelete={() => void handleDelete(memory)}
              canUpdate={Boolean(onUpdateSessionMemory)}
              canDelete={Boolean(onDeleteSessionMemory)}
            />
          ))
        ) : (
          <p className="px-3 py-3 text-sm text-muted-foreground">
            No memory matches the current filters.
          </p>
        )}
      </div>
    </div>
  )
}

function MemoryRow({
  memory,
  mutationRunning,
  onApprove,
  onReject,
  onDisable,
  onEnable,
  onDelete,
  canUpdate,
  canDelete,
}: {
  memory: SessionMemoryRecordDto
  mutationRunning: boolean
  onApprove: () => void
  onReject: () => void
  onDisable: () => void
  onEnable: () => void
  onDelete: () => void
  canUpdate: boolean
  canDelete: boolean
}) {
  return (
    <div className="grid gap-3 px-3 py-3 lg:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Badge variant={memory.reviewState === 'candidate' ? 'outline' : 'secondary'}>
            {memory.reviewState}
          </Badge>
          <Badge variant="outline">{memory.scope}</Badge>
          <Badge variant="outline">{memory.kind.replace(/_/g, ' ')}</Badge>
          {memory.enabled ? (
            <Badge variant="secondary" className="gap-1">
              <Eye className="h-3 w-3" />
              model-visible
            </Badge>
          ) : (
            <Badge variant="outline" className="gap-1">
              <EyeOff className="h-3 w-3" />
              disabled
            </Badge>
          )}
          {typeof memory.confidence === 'number' ? (
            <span className="text-xs text-muted-foreground">{memory.confidence}%</span>
          ) : null}
        </div>
        <p className="mt-2 text-sm leading-relaxed text-foreground">{memory.text}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          {memory.sourceRunId ? `Source ${memory.sourceRunId}` : 'No source run'} · {memory.sourceItemIds.length.toLocaleString()} item{memory.sourceItemIds.length === 1 ? '' : 's'}
        </p>
        {memory.diagnostic ? (
          <p className="mt-1 text-xs text-muted-foreground">{memory.diagnostic.message}</p>
        ) : null}
      </div>
      <div className="flex flex-wrap items-start gap-1.5 lg:justify-end">
        {canUpdate && memory.reviewState !== 'approved' ? (
          <Button type="button" size="sm" variant="outline" disabled={mutationRunning} onClick={onApprove}>
            <CheckCircle2 className="h-3.5 w-3.5" />
            Approve
          </Button>
        ) : null}
        {canUpdate && memory.reviewState === 'candidate' ? (
          <Button type="button" size="sm" variant="outline" disabled={mutationRunning} onClick={onReject}>
            <XCircle className="h-3.5 w-3.5" />
            Reject
          </Button>
        ) : null}
        {canUpdate && memory.reviewState === 'approved' && memory.enabled ? (
          <Button type="button" size="sm" variant="outline" disabled={mutationRunning} onClick={onDisable}>
            <EyeOff className="h-3.5 w-3.5" />
            Disable
          </Button>
        ) : null}
        {canUpdate && memory.reviewState === 'approved' && !memory.enabled ? (
          <Button type="button" size="sm" variant="outline" disabled={mutationRunning} onClick={onEnable}>
            <Eye className="h-3.5 w-3.5" />
            Enable
          </Button>
        ) : null}
        {canDelete ? (
          <Button
            type="button"
            size="icon"
            variant="ghost"
            disabled={mutationRunning}
            aria-label={`Delete memory ${memory.memoryId}`}
            onClick={onDelete}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        ) : null}
      </div>
    </div>
  )
}

function memoryActionMessage(memory: SessionMemoryRecordDto): string {
  if (memory.reviewState === 'approved' && memory.enabled) return 'Memory approved and enabled.'
  if (memory.reviewState === 'approved') return 'Memory disabled.'
  if (memory.reviewState === 'rejected') return 'Memory rejected.'
  return 'Memory updated.'
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message
  if (typeof error === 'string' && error.trim()) return error
  return fallback
}
