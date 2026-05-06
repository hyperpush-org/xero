import { z } from 'zod'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
} from './shared'

const nonEmptyTextSchema = z.string().trim().min(1)
const historyPathSchema = z.string().trim().min(1)

export const codeHistoryOperationModeSchema = z.enum(['selective_undo', 'session_rollback'])
export const codeHistoryOperationStatusSchema = z.enum([
  'pending',
  'planning',
  'conflicted',
  'applying',
  'completed',
  'failed',
])
export const codeHistoryTargetKindSchema = z.enum([
  'change_group',
  'file_change',
  'hunks',
  'session_boundary',
  'run_boundary',
])
export const codeHistoryConflictKindSchema = z.enum([
  'text_overlap',
  'file_missing',
  'file_exists',
  'content_mismatch',
  'metadata_mismatch',
  'unsupported_operation',
  'stale_workspace',
  'storage_error',
])

export const codeWorkspaceHeadSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    headId: nonEmptyOptionalTextSchema,
    treeId: nonEmptyOptionalTextSchema,
    workspaceEpoch: z.number().int().nonnegative(),
    latestHistoryOperationId: nonEmptyOptionalTextSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const codePatchAvailabilitySchema = z
  .object({
    projectId: nonEmptyTextSchema,
    targetChangeGroupId: nonEmptyTextSchema,
    available: z.boolean(),
    affectedPaths: z.array(historyPathSchema),
    fileChangeCount: z.number().int().nonnegative(),
    textHunkCount: z.number().int().nonnegative(),
    unavailableReason: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((availability, ctx) => {
    if (availability.available && availability.affectedPaths.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['affectedPaths'],
        message: 'Patch availability must name affected paths when a target is undoable.',
      })
    }

    if (!availability.available && !availability.unavailableReason) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['unavailableReason'],
        message: 'Patch availability must explain why a target is not undoable.',
      })
    }
  })

export const selectiveUndoTargetSchema = z
  .object({
    targetKind: codeHistoryTargetKindSchema,
    targetId: nonEmptyTextSchema,
    changeGroupId: nonEmptyOptionalTextSchema,
    filePath: nonEmptyOptionalTextSchema,
    hunkIds: z.array(nonEmptyTextSchema).default([]),
  })
  .strict()
  .superRefine((target, ctx) => {
    if (
      target.targetKind === 'session_boundary'
      || target.targetKind === 'run_boundary'
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['targetKind'],
        message: 'Selective undo targets must select a change group, file change, or hunk set.',
      })
    }

    if (!target.changeGroupId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['changeGroupId'],
        message: 'Selective undo targets must include a changeGroupId.',
      })
    }

    if ((target.targetKind === 'file_change' || target.targetKind === 'hunks') && !target.filePath) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['filePath'],
        message: 'File and hunk undo targets must include a filePath.',
      })
    }

    if (target.targetKind === 'hunks' && target.hunkIds.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['hunkIds'],
        message: 'Hunk undo targets must include at least one hunk id.',
      })
    }
  })

export const sessionRollbackTargetSchema = z
  .object({
    targetKind: codeHistoryTargetKindSchema,
    targetId: nonEmptyTextSchema,
    agentSessionId: nonEmptyTextSchema,
    boundaryId: nonEmptyTextSchema,
    runId: nonEmptyOptionalTextSchema,
    changeGroupId: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((target, ctx) => {
    if (
      target.targetKind !== 'session_boundary'
      && target.targetKind !== 'run_boundary'
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['targetKind'],
        message: 'Session rollback targets must select a session or run boundary.',
      })
    }

    if (target.targetKind === 'run_boundary' && !target.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['runId'],
        message: 'Run-boundary rollback targets must include a runId.',
      })
    }
  })

export const codeHistoryOperationTargetSchema = z
  .object({
    targetKind: codeHistoryTargetKindSchema,
    targetId: nonEmptyTextSchema,
  })
  .strict()

export const codeHistoryConflictSchema = z
  .object({
    operationId: nonEmptyTextSchema,
    targetId: nonEmptyTextSchema,
    path: historyPathSchema,
    kind: codeHistoryConflictKindSchema,
    message: nonEmptyTextSchema,
    baseHash: nonEmptyOptionalTextSchema,
    selectedHash: nonEmptyOptionalTextSchema,
    currentHash: nonEmptyOptionalTextSchema,
    hunkIds: z.array(nonEmptyTextSchema).default([]),
  })
  .strict()

export const codeHistoryOperationSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    operationId: nonEmptyTextSchema,
    mode: codeHistoryOperationModeSchema,
    status: codeHistoryOperationStatusSchema,
    target: codeHistoryOperationTargetSchema,
    affectedPaths: z.array(historyPathSchema).min(1),
    conflicts: z.array(codeHistoryConflictSchema),
    workspaceHead: codeWorkspaceHeadSchema.nullable().optional(),
    patchAvailability: codePatchAvailabilitySchema.nullable().optional(),
    resultCommitId: nonEmptyOptionalTextSchema,
    resultChangeGroupId: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((operation, ctx) => {
    if (operation.status === 'conflicted' && operation.conflicts.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['conflicts'],
        message: 'Conflicted code history operations must include conflict records.',
      })
    }

    for (const [index, conflict] of operation.conflicts.entries()) {
      if (conflict.operationId !== operation.operationId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['conflicts', index, 'operationId'],
          message: 'Conflict operation ids must match the enclosing operation.',
        })
      }

      if (conflict.targetId !== operation.target.targetId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['conflicts', index, 'targetId'],
          message: 'Conflict target ids must match the enclosing operation target.',
        })
      }

      if (!operation.affectedPaths.includes(conflict.path)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['conflicts', index, 'path'],
          message: 'Conflict paths must be listed in affectedPaths.',
        })
      }
    }
  })

export const selectiveUndoRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    operationId: nonEmptyTextSchema,
    target: selectiveUndoTargetSchema,
    expectedWorkspaceEpoch: z.number().int().nonnegative().nullable().optional(),
  })
  .strict()

export const selectiveUndoResponseSchema = z
  .object({
    operation: codeHistoryOperationSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.operation.mode !== 'selective_undo') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['operation', 'mode'],
        message: 'Selective undo responses must carry a selective_undo operation.',
      })
    }
  })

export const sessionRollbackRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    operationId: nonEmptyTextSchema,
    target: sessionRollbackTargetSchema,
    expectedWorkspaceEpoch: z.number().int().nonnegative().nullable().optional(),
  })
  .strict()

export const sessionRollbackResponseSchema = z
  .object({
    operation: codeHistoryOperationSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.operation.mode !== 'session_rollback') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['operation', 'mode'],
        message: 'Session rollback responses must carry a session_rollback operation.',
      })
    }
  })

export const codeHistoryOperationStatusRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    operationId: nonEmptyTextSchema,
  })
  .strict()

export const codeHistoryOperationStatusResponseSchema = z
  .object({
    operation: codeHistoryOperationSchema,
  })
  .strict()

export type CodeHistoryOperationModeDto = z.infer<typeof codeHistoryOperationModeSchema>
export type CodeHistoryOperationStatusDto = z.infer<typeof codeHistoryOperationStatusSchema>
export type CodeHistoryTargetKindDto = z.infer<typeof codeHistoryTargetKindSchema>
export type CodeHistoryConflictKindDto = z.infer<typeof codeHistoryConflictKindSchema>
export type CodeWorkspaceHeadDto = z.infer<typeof codeWorkspaceHeadSchema>
export type CodePatchAvailabilityDto = z.infer<typeof codePatchAvailabilitySchema>
export type SelectiveUndoTargetDto = z.infer<typeof selectiveUndoTargetSchema>
export type SessionRollbackTargetDto = z.infer<typeof sessionRollbackTargetSchema>
export type CodeHistoryOperationTargetDto = z.infer<typeof codeHistoryOperationTargetSchema>
export type CodeHistoryConflictDto = z.infer<typeof codeHistoryConflictSchema>
export type CodeHistoryOperationDto = z.infer<typeof codeHistoryOperationSchema>
export type SelectiveUndoRequestDto = z.infer<typeof selectiveUndoRequestSchema>
export type SelectiveUndoResponseDto = z.infer<typeof selectiveUndoResponseSchema>
export type SessionRollbackRequestDto = z.infer<typeof sessionRollbackRequestSchema>
export type SessionRollbackResponseDto = z.infer<typeof sessionRollbackResponseSchema>
export type CodeHistoryOperationStatusRequestDto = z.infer<typeof codeHistoryOperationStatusRequestSchema>
export type CodeHistoryOperationStatusResponseDto = z.infer<typeof codeHistoryOperationStatusResponseSchema>
