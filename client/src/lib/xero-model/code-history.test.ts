import { describe, expect, it } from 'vitest'
import {
  codeHistoryOperationSchema,
  selectiveUndoRequestSchema,
  selectiveUndoResponseSchema,
  sessionRollbackRequestSchema,
  sessionRollbackResponseSchema,
} from './code-history'

const conflictedOperation = {
  projectId: 'project-1',
  operationId: 'history-op-1',
  mode: 'selective_undo',
  status: 'conflicted',
  target: {
    targetKind: 'file_change',
    targetId: 'code-change-1:src/app.ts',
  },
  affectedPaths: ['src/app.ts'],
  conflicts: [
    {
      operationId: 'history-op-1',
      targetId: 'code-change-1:src/app.ts',
      path: 'src/app.ts',
      kind: 'text_overlap',
      message: 'Current content changed lines selected for undo.',
      baseHash: 'sha256:base',
      selectedHash: 'sha256:selected',
      currentHash: 'sha256:current',
      hunkIds: ['hunk-1'],
    },
  ],
  workspaceHead: {
    projectId: 'project-1',
    headId: 'code-head-1',
    treeId: 'code-tree-1',
    workspaceEpoch: 12,
    latestHistoryOperationId: 'history-op-1',
    updatedAt: '2026-05-06T12:00:01Z',
  },
  patchAvailability: {
    projectId: 'project-1',
    targetChangeGroupId: 'code-change-1',
    available: true,
    affectedPaths: ['src/app.ts'],
    fileChangeCount: 1,
    textHunkCount: 1,
    unavailableReason: null,
  },
  resultCommitId: null,
  resultChangeGroupId: null,
  createdAt: '2026-05-06T12:00:00Z',
  updatedAt: '2026-05-06T12:00:01Z',
}

describe('code history contracts', () => {
  it('accepts selective undo and session rollback request/response contracts', () => {
    const undoRequest = selectiveUndoRequestSchema.parse({
      projectId: 'project-1',
      operationId: 'history-op-1',
      target: {
        targetKind: 'hunks',
        targetId: 'code-change-1:src/app.ts:hunk-1',
        changeGroupId: 'code-change-1',
        filePath: 'src/app.ts',
        hunkIds: ['hunk-1'],
      },
      expectedWorkspaceEpoch: 11,
    })
    expect(undoRequest.target.hunkIds).toEqual(['hunk-1'])

    expect(selectiveUndoResponseSchema.parse({ operation: conflictedOperation }).operation.mode).toBe(
      'selective_undo',
    )

    const rollbackRequest = sessionRollbackRequestSchema.parse({
      projectId: 'project-1',
      operationId: 'history-op-2',
      target: {
        targetKind: 'run_boundary',
        targetId: 'run-1:boundary-1',
        agentSessionId: 'agent-session-1',
        runId: 'run-1',
        boundaryId: 'boundary-1',
        changeGroupId: 'code-change-1',
      },
      expectedWorkspaceEpoch: 12,
    })
    expect(rollbackRequest.target.runId).toBe('run-1')

    const rollbackOperation = {
      ...conflictedOperation,
      operationId: 'history-op-2',
      mode: 'session_rollback',
      target: {
        targetKind: 'run_boundary',
        targetId: 'run-1:boundary-1',
      },
      conflicts: [
        {
          ...conflictedOperation.conflicts[0],
          operationId: 'history-op-2',
          targetId: 'run-1:boundary-1',
        },
      ],
    }
    expect(sessionRollbackResponseSchema.parse({ operation: rollbackOperation }).operation.mode).toBe(
      'session_rollback',
    )
  })

  it('rejects unknown modes and statuses', () => {
    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      mode: 'snapshot_restore',
    }).success).toBe(false)

    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      status: 'restored',
    }).success).toBe(false)
  })

  it('requires operation ids, target ids, affected paths, and conflict payloads', () => {
    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      operationId: '',
    }).success).toBe(false)

    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      target: {
        targetKind: 'file_change',
      },
    }).success).toBe(false)

    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      affectedPaths: [],
    }).success).toBe(false)

    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      conflicts: [],
    }).success).toBe(false)

    expect(codeHistoryOperationSchema.safeParse({
      ...conflictedOperation,
      conflicts: [
        {
          operationId: 'history-op-1',
          targetId: 'code-change-1:src/app.ts',
          kind: 'text_overlap',
          message: 'Missing path must fail.',
        },
      ],
    }).success).toBe(false)
  })
})
