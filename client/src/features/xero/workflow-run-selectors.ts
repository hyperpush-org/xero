import type {
  WorkflowNodeDto,
  WorkflowNodeRunStatusDto,
  WorkflowRunStatusDto,
} from '@/src/lib/xero-model/workflow-definition'
import type { WorkflowRunDto, WorkflowRunNodeDto } from '@/src/lib/xero-model/workflow-run'

const TERMINAL_RUN_STATUSES: readonly WorkflowRunStatusDto[] = [
  'completed',
  'failed',
  'cancelled',
]

const ACTIVE_NODE_STATUS_RANK: Partial<Record<WorkflowNodeRunStatusDto, number>> = {
  running: 3,
  starting: 2,
  waiting_on_gate: 1,
}

export function isTerminalWorkflowRunStatus(status: WorkflowRunStatusDto): boolean {
  return TERMINAL_RUN_STATUSES.includes(status)
}

/** The run the chat surface should surface: the most recently started run that
 * is still queued/running/paused/cancelling, or null when everything settled. */
export function pickActiveWorkflowRun(
  runs: readonly WorkflowRunDto[],
): WorkflowRunDto | null {
  let latest: WorkflowRunDto | null = null
  for (const run of runs) {
    if (isTerminalWorkflowRunStatus(run.status)) continue
    if (!latest || compareTimestampsDesc(run.startedAt, latest.startedAt) < 0) {
      latest = run
    }
  }
  return latest
}

/** Latest attempt per definition node, keyed by nodeId. */
export function latestWorkflowRunNodesByNodeId(
  run: WorkflowRunDto,
): Map<string, WorkflowRunNodeDto> {
  const byNodeId = new Map<string, WorkflowRunNodeDto>()
  for (const node of run.nodes) {
    const current = byNodeId.get(node.nodeId)
    if (!current || node.attemptNumber >= current.attemptNumber) {
      byNodeId.set(node.nodeId, node)
    }
  }
  return byNodeId
}

/** The agent session the workflow is currently working in: prefers the node
 * run that is actively running, then starting, then waiting on a gate, and
 * falls back to the most recently updated node run that has a session. */
export function activeWorkflowAgentSessionId(run: WorkflowRunDto): string | null {
  let best: WorkflowRunNodeDto | null = null
  let bestRank = -1
  for (const node of run.nodes) {
    if (!node.agentSessionId) continue
    const rank = ACTIVE_NODE_STATUS_RANK[node.status] ?? 0
    if (
      rank > bestRank ||
      (rank === bestRank &&
        (!best || compareTimestampsDesc(node.updatedAt, best.updatedAt) < 0))
    ) {
      best = node
      bestRank = rank
    }
  }
  return best?.agentSessionId ?? null
}

export function workflowRunSessionIds(run: WorkflowRunDto): Set<string> {
  const ids = new Set<string>()
  for (const node of run.nodes) {
    if (node.agentSessionId) ids.add(node.agentSessionId)
  }
  return ids
}

export interface WorkflowRunNodeProgress {
  node: WorkflowNodeDto
  runNode: WorkflowRunNodeDto | null
  status: WorkflowNodeRunStatusDto
}

export interface WorkflowRunProgress {
  entries: WorkflowRunNodeProgress[]
  totalCount: number
  settledCount: number
  /** Definition node currently being worked (running/starting/waiting), if any. */
  activeEntry: WorkflowRunNodeProgress | null
  /** Paused human-checkpoint/gate node run awaiting a decision, if any. */
  waitingEntry: WorkflowRunNodeProgress | null
}

const SETTLED_NODE_STATUSES: readonly WorkflowNodeRunStatusDto[] = [
  'succeeded',
  'failed',
  'skipped',
  'cancelled',
]

/** Joins the definition snapshot with the latest run-node attempts, preserving
 * definition order so a canvas-free surface can mirror the canvas status. */
export function buildWorkflowRunProgress(run: WorkflowRunDto): WorkflowRunProgress {
  const runNodes = latestWorkflowRunNodesByNodeId(run)
  const entries: WorkflowRunNodeProgress[] = run.definitionSnapshot.nodes.map((node) => {
    const runNode = runNodes.get(node.id) ?? null
    return { node, runNode, status: runNode?.status ?? 'pending' }
  })

  let activeEntry: WorkflowRunNodeProgress | null = null
  let activeRank = 0
  let waitingEntry: WorkflowRunNodeProgress | null = null
  let settledCount = 0
  for (const entry of entries) {
    const rank = ACTIVE_NODE_STATUS_RANK[entry.status] ?? 0
    if (rank > activeRank) {
      activeEntry = entry
      activeRank = rank
    }
    if (entry.status === 'waiting_on_gate' && !waitingEntry) {
      waitingEntry = entry
    }
    if (SETTLED_NODE_STATUSES.includes(entry.status)) {
      settledCount += 1
    }
  }

  return {
    entries,
    totalCount: entries.length,
    settledCount,
    activeEntry,
    waitingEntry,
  }
}

function compareTimestampsDesc(left: string, right: string): number {
  const leftMs = Date.parse(left)
  const rightMs = Date.parse(right)
  if (Number.isNaN(leftMs) || Number.isNaN(rightMs)) {
    return right.localeCompare(left)
  }
  return rightMs - leftMs
}
