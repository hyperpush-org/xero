import { describe, expect, it } from 'vitest'
import {
  codeHistoryOperationSchema,
  returnSessionToHereRequestSchema,
  returnSessionToHereResponseSchema,
  selectiveUndoRequestSchema,
  selectiveUndoResponseSchema,
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
  it('accepts selective undo and return session to here request/response contracts', () => {
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
    const hunkUndoOperation = {
      ...conflictedOperation,
      target: {
        targetKind: 'hunks',
        targetId: 'code-change-1:src/app.ts:hunk-1',
        hunkIds: ['hunk-1'],
      },
      conflicts: [
        {
          ...conflictedOperation.conflicts[0],
          targetId: 'code-change-1:src/app.ts:hunk-1',
        },
      ],
    }
    expect(
      selectiveUndoResponseSchema.parse({ operation: hunkUndoOperation }).operation.target.hunkIds,
    ).toEqual(['hunk-1'])

    const returnSessionRequest = returnSessionToHereRequestSchema.parse({
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
    expect(returnSessionRequest.target.runId).toBe('run-1')

    const returnSessionOperation = {
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
    expect(returnSessionToHereResponseSchema.parse({ operation: returnSessionOperation }).operation.mode).toBe(
      'session_rollback',
    )
  })

  it('uses return session to here terminology for session-boundary validation copy', () => {
    const wrongBoundary = returnSessionToHereRequestSchema.safeParse({
      projectId: 'project-1',
      operationId: 'history-op-3',
      target: {
        targetKind: 'change_group',
        targetId: 'code-change-1',
        agentSessionId: 'agent-session-1',
        boundaryId: 'boundary-1',
      },
    })

    expect(wrongBoundary.success).toBe(false)
    if (wrongBoundary.success) {
      throw new Error('Expected return session to here validation to fail.')
    }
    expect(wrongBoundary.error.issues[0]?.message).toContain('Return session to here')
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
      target: {
        targetKind: 'hunks',
        targetId: 'code-change-1:src/app.ts:hunk-1',
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
