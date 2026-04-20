import { z } from 'zod'
import type { Phase, PhaseStatus, PhaseStep, Project } from '@/components/cadence/data'

export type { Phase, PhaseStatus, PhaseStep, Project } from '@/components/cadence/data'

const PHASE_STEPS = ['discuss', 'plan', 'execute', 'verify', 'ship'] as const satisfies readonly PhaseStep[]
const PLANNING_LIFECYCLE_STAGES = ['discussion', 'research', 'requirements', 'roadmap'] as const
const STEP_INDEX = new Map(PHASE_STEPS.map((step, index) => [step, index]))

export const MAX_RUNTIME_STREAM_ITEMS = 40
export const MAX_RUNTIME_STREAM_TRANSCRIPTS = 20
export const MAX_RUNTIME_STREAM_TOOL_CALLS = 20
export const MAX_RUNTIME_STREAM_ACTIVITY = 20
export const MAX_RUNTIME_STREAM_ACTION_REQUIRED = 10

const changeKindSchema = z.enum([
  'added',
  'modified',
  'deleted',
  'renamed',
  'copied',
  'type_change',
  'conflicted',
])

const phaseStatusSchema = z.enum(['complete', 'active', 'pending', 'blocked'])
const phaseStepSchema = z.enum(PHASE_STEPS)
const nullableTextSchema = z.string().nullable().optional()
const nonEmptyOptionalTextSchema = z.string().trim().min(1).nullable().optional()
const isoTimestampSchema = z.string().datetime({ offset: true })

function sortByNewest<T>(
  items: readonly T[],
  getTimestamp: (item: T) => string | null | undefined,
): T[] {
  return [...items]
    .map((item, index) => ({ item, index }))
    .sort((left, right) => {
      const leftTime = Date.parse(getTimestamp(left.item) ?? '')
      const rightTime = Date.parse(getTimestamp(right.item) ?? '')
      const normalizedLeftTime = Number.isFinite(leftTime) ? leftTime : 0
      const normalizedRightTime = Number.isFinite(rightTime) ? rightTime : 0

      if (normalizedLeftTime === normalizedRightTime) {
        return left.index - right.index
      }

      return normalizedRightTime - normalizedLeftTime
    })
    .map(({ item }) => item)
}

export const projectSummarySchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string(),
  milestone: z.string(),
  totalPhases: z.number().int().nonnegative(),
  completedPhases: z.number().int().nonnegative(),
  activePhase: z.number().int().nonnegative(),
  branch: nullableTextSchema,
  runtime: nullableTextSchema,
})

export const phaseSummarySchema = z.object({
  id: z.number().int().nonnegative(),
  name: z.string().min(1),
  description: z.string(),
  status: phaseStatusSchema,
  currentStep: phaseStepSchema.nullable().optional(),
  taskCount: z.number().int().nonnegative(),
  completedTasks: z.number().int().nonnegative(),
  summary: nullableTextSchema,
})

export const planningLifecycleStageKindSchema = z.enum(PLANNING_LIFECYCLE_STAGES)

export const planningLifecycleStageSchema = z
  .object({
    stage: planningLifecycleStageKindSchema,
    nodeId: z.string().trim().min(1),
    status: phaseStatusSchema,
    actionRequired: z.boolean(),
    lastTransitionAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()

export const planningLifecycleProjectionSchema = z
  .object({
    stages: z.array(planningLifecycleStageSchema),
  })
  .strict()
  .superRefine((projection, ctx) => {
    const seenStages = new Set<(typeof PLANNING_LIFECYCLE_STAGES)[number]>()

    projection.stages.forEach((stage, index) => {
      if (seenStages.has(stage.stage)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['stages', index, 'stage'],
          message: `Duplicate lifecycle stage \`${stage.stage}\` is not allowed.`,
        })
        return
      }

      seenStages.add(stage.stage)
    })
  })

export const repositorySummarySchema = z.object({
  id: z.string().min(1),
  projectId: z.string().min(1),
  rootPath: z.string().min(1),
  displayName: z.string().min(1),
  branch: nullableTextSchema,
  headSha: nullableTextSchema,
  isGitRepo: z.boolean(),
})

export const repositoryDiffScopeSchema = z.enum(['staged', 'unstaged', 'worktree'])

export const importRepositoryResponseSchema = z.object({
  project: projectSummarySchema,
  repository: repositorySummarySchema,
})

export const listProjectsResponseSchema = z.object({
  projects: z.array(projectSummarySchema),
})

const notificationCorrelationKeyPattern = /^nfy:[a-f0-9]{32}$/

export const notificationRouteKindSchema = z.enum(['telegram', 'discord'])
export const notificationDispatchStatusSchema = z.enum(['pending', 'sent', 'failed', 'claimed'])
export const notificationReplyClaimStatusSchema = z.enum(['accepted', 'rejected'])
export const notificationDispatchOutcomeStatusSchema = z.enum(['sent', 'failed'])

function splitNotificationRouteTarget(rawTarget: string): { prefix: string; channelTarget: string } | null {
  const separatorIndex = rawTarget.indexOf(':')
  if (separatorIndex < 0) {
    return null
  }

  return {
    prefix: rawTarget.slice(0, separatorIndex).trim(),
    channelTarget: rawTarget.slice(separatorIndex + 1).trim(),
  }
}

export function composeNotificationRouteTarget(
  routeKind: z.infer<typeof notificationRouteKindSchema>,
  rawTarget: string,
): string {
  const trimmed = rawTarget.trim()
  if (!trimmed) {
    throw new Error('Route target is required.')
  }

  const splitTarget = splitNotificationRouteTarget(trimmed)
  if (!splitTarget) {
    return `${routeKind}:${trimmed}`
  }

  if (!splitTarget.prefix) {
    throw new Error('Route target prefix is required when using `<kind>:<channel-target>` format.')
  }

  if (splitTarget.prefix === routeKind) {
    if (!splitTarget.channelTarget) {
      throw new Error('Route target channel segment is required after the `<kind>:` prefix.')
    }

    return `${routeKind}:${splitTarget.channelTarget}`
  }

  if (splitTarget.prefix === 'telegram' || splitTarget.prefix === 'discord') {
    throw new Error(
      `Route target prefix \`${splitTarget.prefix}\` does not match the selected route kind \`${routeKind}\`.`,
    )
  }

  return `${routeKind}:${trimmed}`
}

export function decomposeNotificationRouteTarget(
  routeKind: z.infer<typeof notificationRouteKindSchema>,
  routeTarget: string,
): { channelTarget: string; canonicalTarget: string } {
  const trimmed = routeTarget.trim()
  if (!trimmed) {
    throw new Error('Route target is required.')
  }

  const splitTarget = splitNotificationRouteTarget(trimmed)
  if (!splitTarget || !splitTarget.prefix) {
    throw new Error(`Saved route target must use \`<kind>:<channel-target>\` format for \`${routeKind}\` routes.`)
  }

  if (splitTarget.prefix !== routeKind) {
    throw new Error(
      `Saved route target prefix \`${splitTarget.prefix}\` does not match route kind \`${routeKind}\`.`,
    )
  }

  if (!splitTarget.channelTarget) {
    throw new Error('Saved route target channel segment is empty.')
  }

  return {
    channelTarget: splitTarget.channelTarget,
    canonicalTarget: `${routeKind}:${splitTarget.channelTarget}`,
  }
}

const notificationRouteMetadataSchema = z
  .string()
  .trim()
  .min(1)
  .refine((value) => {
    try {
      const parsed = JSON.parse(value)
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    } catch {
      return false
    }
  }, 'Notification route metadata must be a JSON object string.')

export const notificationRouteCredentialReadinessStatusSchema = z.enum([
  'ready',
  'missing',
  'malformed',
  'unavailable',
])

export const notificationRouteCredentialReadinessDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()

export const notificationRouteCredentialReadinessSchema = z
  .object({
    hasBotToken: z.boolean(),
    hasChatId: z.boolean(),
    hasWebhookUrl: z.boolean(),
    ready: z.boolean(),
    status: notificationRouteCredentialReadinessStatusSchema,
    diagnostic: notificationRouteCredentialReadinessDiagnosticSchema.nullable().optional(),
  })
  .strict()
  .superRefine((readiness, ctx) => {
    if (readiness.status === 'ready') {
      if (!readiness.ready) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['ready'],
          message: 'Credential-readiness rows with `status=ready` must set `ready=true`.',
        })
      }

      if (readiness.diagnostic) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['diagnostic'],
          message: 'Credential-readiness rows with `status=ready` must not include diagnostics.',
        })
      }

      return
    }

    if (readiness.ready) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['ready'],
        message: 'Credential-readiness rows with non-ready status must set `ready=false`.',
      })
    }

    if (!readiness.diagnostic) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostic'],
        message:
          'Credential-readiness rows with non-ready status must include typed diagnostics.',
      })
    }
  })

export const notificationRouteSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1),
    enabled: z.boolean(),
    metadataJson: notificationRouteMetadataSchema.nullable().optional(),
    credentialReadiness: notificationRouteCredentialReadinessSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
const notificationCorrelationKeySchema = z
  .string()
  .trim()
  .regex(
    notificationCorrelationKeyPattern,
    'Correlation keys must match `nfy:<32 lowercase hex>` for deterministic reply correlation.',
  )
const optionalIsoTimestampSchema = isoTimestampSchema.nullable().optional()

export const notificationDispatchSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    status: notificationDispatchStatusSchema,
    attemptCount: z.number().int().nonnegative(),
    lastAttemptAt: optionalIsoTimestampSchema,
    deliveredAt: optionalIsoTimestampSchema,
    claimedAt: optionalIsoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastErrorMessage: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((dispatch, ctx) => {
    const hasAnyFailureDiagnostic = Boolean(dispatch.lastErrorCode || dispatch.lastErrorMessage)
    const hasFailureDiagnosticPair = Boolean(dispatch.lastErrorCode && dispatch.lastErrorMessage)

    if (hasAnyFailureDiagnostic && !hasFailureDiagnosticPair) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastErrorCode'],
        message: 'Notification dispatch rows must include both `lastErrorCode` and `lastErrorMessage` when diagnostics are present.',
      })
    }

    if ((dispatch.status === 'sent' || dispatch.status === 'failed') && dispatch.attemptCount < 1) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptCount'],
        message: 'Sent/failed notification dispatch rows must include a positive `attemptCount`.',
      })
    }

    if ((dispatch.status === 'sent' || dispatch.status === 'failed') && !dispatch.lastAttemptAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastAttemptAt'],
        message: 'Sent/failed notification dispatch rows must include `lastAttemptAt`.',
      })
    }

    switch (dispatch.status) {
      case 'pending':
        if (dispatch.deliveredAt || dispatch.claimedAt || hasAnyFailureDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Pending notification dispatch rows must not include delivery, claim, or failure diagnostics.',
          })
        }
        break
      case 'sent':
        if (!dispatch.deliveredAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['deliveredAt'],
            message: 'Sent notification dispatch rows must include `deliveredAt`.',
          })
        }
        if (dispatch.claimedAt || hasAnyFailureDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Sent notification dispatch rows must not include claim or failure diagnostics.',
          })
        }
        break
      case 'failed':
        if (!hasFailureDiagnosticPair) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['lastErrorCode'],
            message: 'Failed notification dispatch rows must include non-empty `lastErrorCode` and `lastErrorMessage`.',
          })
        }
        if (dispatch.deliveredAt || dispatch.claimedAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Failed notification dispatch rows must not include delivery or claim timestamps.',
          })
        }
        break
      case 'claimed':
        if (!dispatch.claimedAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['claimedAt'],
            message: 'Claimed notification dispatch rows must include `claimedAt`.',
          })
        }
        break
    }
  })

export const notificationReplyClaimSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    responderId: nonEmptyOptionalTextSchema,
    status: notificationReplyClaimStatusSchema,
    rejectionCode: nonEmptyOptionalTextSchema,
    rejectionMessage: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((claim, ctx) => {
    if (claim.status === 'accepted') {
      if (claim.rejectionCode || claim.rejectionMessage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'Accepted notification reply claims must not include rejection diagnostics.',
        })
      }
      return
    }

    if (!claim.rejectionCode || !claim.rejectionMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['rejectionCode'],
        message: 'Rejected notification reply claims must include non-empty `rejectionCode` and `rejectionMessage`.',
      })
    }
  })

export const operatorApprovalStatusSchema = z.enum(['pending', 'approved', 'rejected'])
export const verificationRecordStatusSchema = z.enum(['pending', 'passed', 'failed'])
export const resumeHistoryStatusSchema = z.enum(['started', 'failed'])

export type OperatorApprovalAnswerRequirementReason = 'optional' | 'gate_linked' | 'runtime_resumable'
export type OperatorApprovalAnswerShapeKind = 'plain_text' | 'terminal_input'
type OperatorRuntimeResumableClassification = 'not_runtime_scoped' | 'runtime_resumable' | 'runtime_malformed'

export interface OperatorApprovalAnswerShapeMeta {
  kind: OperatorApprovalAnswerShapeKind
  label: string
  guidance: string
  placeholder: string
}

interface OperatorApprovalAnswerPolicyInput {
  actionId: string
  sessionId?: string | null
  flowId?: string | null
  actionType: string
  gateNodeId?: string | null
  gateKey?: string | null
}

interface OperatorApprovalAnswerPolicy {
  isGateLinked: boolean
  isRuntimeResumable: boolean
  requiresAnswer: boolean
  requirementReason: OperatorApprovalAnswerRequirementReason
  requirementLabel: string
  runtimeScopeClassification: OperatorRuntimeResumableClassification
  answerShape: OperatorApprovalAnswerShapeMeta
}

const DEFAULT_OPERATOR_ANSWER_SHAPE: OperatorApprovalAnswerShapeMeta = {
  kind: 'plain_text',
  label: 'Plain-text response',
  guidance:
    'Provide concise plain-text decision context without secrets. Cadence rejects secret-bearing payloads.',
  placeholder: 'Provide plain-text operator context for this decision.',
}

const OPERATOR_ACTION_ANSWER_SHAPES: Record<string, OperatorApprovalAnswerShapeMeta> = {
  terminal_input_required: {
    kind: 'terminal_input',
    label: 'Terminal input text',
    guidance:
      'Provide the exact non-empty terminal input text that should be submitted when resuming this runtime action.',
    placeholder: 'Type the exact terminal input response to submit on resume.',
  },
  review_worktree: {
    kind: 'plain_text',
    label: 'Worktree review rationale',
    guidance:
      'Summarize why the repository diff is safe to proceed. Keep the rationale plain text and non-secret.',
    placeholder: 'Summarize the worktree review rationale that justifies approval.',
  },
  review_plan: {
    kind: 'plain_text',
    label: 'Plan review rationale',
    guidance:
      'Describe why the proposed plan should proceed. Keep the rationale plain text and non-secret.',
    placeholder: 'Summarize the plan review rationale that justifies approval.',
  },
  confirm_resume: {
    kind: 'plain_text',
    label: 'Resume confirmation note',
    guidance:
      'Provide optional plain-text confirmation context for this resume-related operator decision.',
    placeholder: 'Optional plain-text context for this resume confirmation.',
  },
}

function normalizeApprovalPolicyText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function deriveOperatorScopePrefixForApproval(
  sessionId: string | null | undefined,
  flowId: string | null | undefined,
): string | null {
  const normalizedFlowId = normalizeApprovalPolicyText(flowId)
  if (normalizedFlowId) {
    return `flow:${normalizedFlowId}`
  }

  const normalizedSessionId = normalizeApprovalPolicyText(sessionId)
  if (normalizedSessionId) {
    return `session:${normalizedSessionId}`
  }

  return null
}

function humanizeOperatorActionType(actionType: string): string {
  return actionType
    .split(/[_\-:]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function classifyRuntimeResumableOperatorAction(
  input: OperatorApprovalAnswerPolicyInput,
): OperatorRuntimeResumableClassification {
  const actionId = normalizeApprovalPolicyText(input.actionId)
  const actionType = normalizeApprovalPolicyText(input.actionType)

  if (!actionId || !actionType || !actionId.includes(':run:') || !actionId.includes(':boundary:')) {
    return 'not_runtime_scoped'
  }

  const scopePrefix = deriveOperatorScopePrefixForApproval(input.sessionId, input.flowId)
  if (!scopePrefix) {
    return 'runtime_malformed'
  }

  const runtimePrefix = `${scopePrefix}:run:`
  if (!actionId.startsWith(runtimePrefix)) {
    return 'runtime_malformed'
  }

  const runtimeSuffix = actionId.slice(runtimePrefix.length)
  const boundaryMarker = ':boundary:'
  const boundaryMarkerIndex = runtimeSuffix.indexOf(boundaryMarker)
  if (boundaryMarkerIndex <= 0) {
    return 'runtime_malformed'
  }

  const runId = runtimeSuffix.slice(0, boundaryMarkerIndex).trim()
  const boundaryAndAction = runtimeSuffix.slice(boundaryMarkerIndex + boundaryMarker.length)
  if (!runId || boundaryAndAction.length === 0) {
    return 'runtime_malformed'
  }

  const actionSuffix = `:${actionType}`
  if (!boundaryAndAction.endsWith(actionSuffix)) {
    return 'runtime_malformed'
  }

  const boundaryId = boundaryAndAction.slice(0, -actionSuffix.length).trim()
  if (!boundaryId) {
    return 'runtime_malformed'
  }

  return 'runtime_resumable'
}

export function deriveOperatorAnswerShapeMetadata(
  actionType: string | null | undefined,
): OperatorApprovalAnswerShapeMeta {
  const normalizedActionType = normalizeApprovalPolicyText(actionType)
  if (!normalizedActionType) {
    return DEFAULT_OPERATOR_ANSWER_SHAPE
  }

  const knownShape = OPERATOR_ACTION_ANSWER_SHAPES[normalizedActionType]
  if (knownShape) {
    return knownShape
  }

  return {
    ...DEFAULT_OPERATOR_ANSWER_SHAPE,
    label: `Plain-text response (${humanizeOperatorActionType(normalizedActionType)})`,
  }
}

function getOperatorAnswerRequirementLabel(reason: OperatorApprovalAnswerRequirementReason): string {
  switch (reason) {
    case 'gate_linked':
      return 'Required — gate-linked approvals need a non-empty user answer before approval.'
    case 'runtime_resumable':
      return 'Required — runtime-resumable approvals need a non-empty user answer before approval.'
    case 'optional':
      return 'Optional — this action can be approved or rejected without a user answer.'
  }
}

export function deriveOperatorApprovalAnswerPolicy(
  input: OperatorApprovalAnswerPolicyInput,
): OperatorApprovalAnswerPolicy {
  const gateNodeId = normalizeApprovalPolicyText(input.gateNodeId)
  const gateKey = normalizeApprovalPolicyText(input.gateKey)
  const isGateLinked = Boolean(gateNodeId && gateKey)

  const runtimeScopeClassification = isGateLinked
    ? 'not_runtime_scoped'
    : classifyRuntimeResumableOperatorAction(input)
  const isRuntimeResumable = runtimeScopeClassification === 'runtime_resumable'

  const requirementReason: OperatorApprovalAnswerRequirementReason = isGateLinked
    ? 'gate_linked'
    : isRuntimeResumable
      ? 'runtime_resumable'
      : 'optional'

  return {
    isGateLinked,
    isRuntimeResumable,
    requiresAnswer: requirementReason !== 'optional',
    requirementReason,
    requirementLabel: getOperatorAnswerRequirementLabel(requirementReason),
    runtimeScopeClassification,
    answerShape: deriveOperatorAnswerShapeMetadata(input.actionType),
  }
}

export const operatorApprovalSchema = z
  .object({
    actionId: z.string().trim().min(1),
    sessionId: nonEmptyOptionalTextSchema,
    flowId: nonEmptyOptionalTextSchema,
    actionType: z.string().trim().min(1),
    title: z.string().trim().min(1),
    detail: z.string().trim().min(1),
    gateNodeId: nonEmptyOptionalTextSchema,
    gateKey: nonEmptyOptionalTextSchema,
    transitionFromNodeId: nonEmptyOptionalTextSchema,
    transitionToNodeId: nonEmptyOptionalTextSchema,
    transitionKind: nonEmptyOptionalTextSchema,
    userAnswer: nonEmptyOptionalTextSchema,
    status: operatorApprovalStatusSchema,
    decisionNote: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    resolvedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
  .superRefine((approval, ctx) => {
    const gateNodeId = approval.gateNodeId ?? null
    const gateKey = approval.gateKey ?? null
    const transitionFromNodeId = approval.transitionFromNodeId ?? null
    const transitionToNodeId = approval.transitionToNodeId ?? null
    const transitionKind = approval.transitionKind ?? null
    const userAnswer = approval.userAnswer ?? null
    const decisionNote = approval.decisionNote ?? null

    const gateFieldsPopulated = gateNodeId !== null || gateKey !== null
    if (gateFieldsPopulated && (!gateNodeId || !gateKey)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['gateNodeId'],
        message: 'Gate-linked approvals must include both `gateNodeId` and `gateKey`.',
      })
    }

    const continuationFieldsPopulated =
      transitionFromNodeId !== null || transitionToNodeId !== null || transitionKind !== null
    if (continuationFieldsPopulated && (!transitionFromNodeId || !transitionToNodeId || !transitionKind)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['transitionFromNodeId'],
        message:
          'Gate-linked approvals must include full transition continuation metadata (`transitionFromNodeId`, `transitionToNodeId`, `transitionKind`).',
      })
    }

    const answerPolicy = deriveOperatorApprovalAnswerPolicy(approval)

    if (answerPolicy.runtimeScopeClassification === 'runtime_malformed') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['actionId'],
        message:
          'Runtime-scoped approvals must include consistent scope/run/boundary/action metadata before Cadence can evaluate answer requirements.',
      })
    }

    if (approval.status === 'pending') {
      if (userAnswer || decisionNote || approval.resolvedAt) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'Pending approvals must not include `userAnswer`, `decisionNote`, or `resolvedAt`.',
        })
      }
      return
    }

    if (approval.status === 'approved' && answerPolicy.requiresAnswer && !userAnswer) {
      const missingAnswerMessage =
        answerPolicy.requirementReason === 'gate_linked'
          ? 'Approved gate-linked approvals must include a non-empty `userAnswer`.'
          : answerPolicy.requirementReason === 'runtime_resumable'
            ? 'Approved runtime-resumable approvals must include a non-empty `userAnswer`.'
            : 'Approved required-input approvals must include a non-empty `userAnswer`.'

      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['userAnswer'],
        message: missingAnswerMessage,
      })
    }
  })

export const verificationRecordSchema = z.object({
  id: z.number().int().nonnegative(),
  sourceActionId: nonEmptyOptionalTextSchema,
  status: verificationRecordStatusSchema,
  summary: z.string().trim().min(1),
  detail: nonEmptyOptionalTextSchema,
  recordedAt: isoTimestampSchema,
})

export const resumeHistoryEntrySchema = z.object({
  id: z.number().int().nonnegative(),
  sourceActionId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  status: resumeHistoryStatusSchema,
  summary: z.string().trim().min(1),
  createdAt: isoTimestampSchema,
})

export const resolveOperatorActionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    decision: z.enum(['approve', 'reject']),
    userAnswer: nonEmptyOptionalTextSchema,
  })
  .strict()

export const resolveOperatorActionResponseSchema = z.object({
  approvalRequest: operatorApprovalSchema,
  verificationRecord: verificationRecordSchema,
})

export const workflowHandoffPackageSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    handoffTransitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    packagePayload: z.string().trim().min(1),
    packageHash: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const resumeOperatorRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    userAnswer: nonEmptyOptionalTextSchema,
  })
  .strict()

export const listNotificationRoutesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const listNotificationRoutesResponseSchema = z
  .object({
    routes: z.array(notificationRouteSchema),
  })
  .strict()
  .superRefine((response, ctx) => {
    response.routes.forEach((route, index) => {
      const readiness = route.credentialReadiness
      if (!readiness) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness'],
          message:
            'List-notification-routes responses must include redacted `credentialReadiness` metadata for every route.',
        })
        return
      }

      if (route.routeKind === 'telegram') {
        if (readiness.hasWebhookUrl) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['routes', index, 'credentialReadiness', 'hasWebhookUrl'],
            message: 'Telegram readiness rows must not report `hasWebhookUrl=true`.',
          })
        }

        if (readiness.ready && (!readiness.hasBotToken || !readiness.hasChatId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['routes', index, 'credentialReadiness', 'ready'],
            message:
              'Telegram readiness rows can set `ready=true` only when both `hasBotToken` and `hasChatId` are true.',
          })
        }

        return
      }

      if (readiness.hasChatId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness', 'hasChatId'],
          message: 'Discord readiness rows must not report `hasChatId=true`.',
        })
      }

      if (readiness.ready && (!readiness.hasWebhookUrl || !readiness.hasBotToken)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness', 'ready'],
          message:
            'Discord readiness rows can set `ready=true` only when both `hasWebhookUrl` and `hasBotToken` are true.',
        })
      }
    })
  })

export const upsertNotificationRouteRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1),
    enabled: z.boolean(),
    metadataJson: notificationRouteMetadataSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const upsertNotificationRouteResponseSchema = z
  .object({
    route: notificationRouteSchema,
  })
  .strict()

export const notificationRouteCredentialPayloadSchema = z
  .object({
    botToken: nonEmptyOptionalTextSchema,
    chatId: nonEmptyOptionalTextSchema,
    webhookUrl: nonEmptyOptionalTextSchema,
  })
  .strict()

export const upsertNotificationRouteCredentialsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    credentials: notificationRouteCredentialPayloadSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.routeKind === 'telegram') {
      if (!request.credentials.botToken) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'botToken'],
          message: 'Telegram credentials require non-empty `botToken`.',
        })
      }

      if (!request.credentials.chatId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'chatId'],
          message: 'Telegram credentials require non-empty `chatId`.',
        })
      }

      if (request.credentials.webhookUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'webhookUrl'],
          message: 'Telegram credentials must not include `webhookUrl`.',
        })
      }

      return
    }

    if (!request.credentials.webhookUrl) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentials', 'webhookUrl'],
        message: 'Discord credentials require non-empty `webhookUrl`.',
      })
    } else {
      try {
        const parsed = new URL(request.credentials.webhookUrl)
        if (parsed.protocol !== 'https:') {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['credentials', 'webhookUrl'],
            message: 'Discord credentials require an HTTPS `webhookUrl`.',
          })
        }
      } catch {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'webhookUrl'],
          message: 'Discord credentials require a valid URL `webhookUrl`.',
        })
      }
    }

    if (request.credentials.chatId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentials', 'chatId'],
        message: 'Discord credentials must not include `chatId`.',
      })
    }
  })

export const upsertNotificationRouteCredentialsResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    credentialScope: z.literal('app_local'),
    hasBotToken: z.boolean(),
    hasChatId: z.boolean(),
    hasWebhookUrl: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.routeKind === 'telegram') {
      if (!response.hasBotToken || !response.hasChatId || response.hasWebhookUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routeKind'],
          message:
            'Telegram credential acknowledgements must indicate bot token + chat id and no webhook URL.',
        })
      }
      return
    }

    if (!response.hasWebhookUrl || response.hasChatId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['routeKind'],
        message: 'Discord credential acknowledgements must indicate webhook URL and no chat id.',
      })
    }
  })

export const listNotificationDispatchesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const listNotificationDispatchesResponseSchema = z
  .object({
    dispatches: z.array(notificationDispatchSchema),
  })
  .strict()

export const recordNotificationDispatchOutcomeRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    status: notificationDispatchOutcomeStatusSchema,
    attemptedAt: isoTimestampSchema,
    errorCode: nonEmptyOptionalTextSchema,
    errorMessage: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.status === 'sent') {
      if (request.errorCode || request.errorMessage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['errorCode'],
          message: 'Sent notification dispatch outcomes must not include error diagnostics.',
        })
      }
      return
    }

    if (!request.errorCode || !request.errorMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['errorCode'],
        message: 'Failed notification dispatch outcomes must include non-empty `errorCode` and `errorMessage`.',
      })
    }
  })

export const recordNotificationDispatchOutcomeResponseSchema = z
  .object({
    dispatch: notificationDispatchSchema,
  })
  .strict()

export const submitNotificationReplyRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    responderId: nonEmptyOptionalTextSchema,
    replyText: z.string().trim().min(1),
    decision: z.enum(['approve', 'reject']),
    receivedAt: isoTimestampSchema,
  })
  .strict()

export const submitNotificationReplyResponseSchema = z
  .object({
    claim: notificationReplyClaimSchema,
    dispatch: notificationDispatchSchema,
    resolveResult: resolveOperatorActionResponseSchema,
    resumeResult: z.lazy(() => resumeOperatorRunResponseSchema).nullable().optional(),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.claim.projectId !== response.dispatch.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'projectId'],
        message: 'Notification reply response claim and dispatch must reference the same project.',
      })
    }

    if (response.claim.actionId !== response.dispatch.actionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'actionId'],
        message: 'Notification reply response claim and dispatch must reference the same action.',
      })
    }

    if (response.claim.routeId !== response.dispatch.routeId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'routeId'],
        message: 'Notification reply response claim and dispatch must reference the same route.',
      })
    }

    if (response.claim.correlationKey !== response.dispatch.correlationKey) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'correlationKey'],
        message: 'Notification reply response claim and dispatch must reference the same correlation key.',
      })
    }

    if (response.claim.status === 'accepted' && response.dispatch.status !== 'claimed') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['dispatch', 'status'],
        message: 'Accepted notification reply responses must return a claimed notification dispatch row.',
      })
    }

    if (response.resolveResult.approvalRequest.actionId !== response.claim.actionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resolveResult', 'approvalRequest', 'actionId'],
        message: 'Notification reply resolve results must reference the same action id as the claim.',
      })
    }

    const resolvedStatus = response.resolveResult.approvalRequest.status
    if (resolvedStatus === 'approved' && !response.resumeResult) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult'],
        message: 'Approved notification reply decisions must include a resume result payload.',
      })
    }

    if (resolvedStatus === 'rejected' && response.resumeResult) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult'],
        message: 'Rejected notification reply decisions must not include a resume result payload.',
      })
    }

    if (
      response.resumeResult &&
      response.resumeResult.approvalRequest.actionId !== response.claim.actionId
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult', 'approvalRequest', 'actionId'],
        message: 'Notification reply resume results must reference the same action id as the claim.',
      })
    }
  })

const notificationAdapterErrorCountSchema = z
  .object({
    code: z.string().trim().min(1),
    count: z.number().int().nonnegative(),
  })
  .strict()

export const notificationAdapterDispatchAttemptSchema = z
  .object({
    dispatchId: z.number().int().nonnegative(),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: z.string().trim().min(1),
    outcomeStatus: notificationDispatchStatusSchema,
    diagnosticCode: z.string().trim().min(1),
    diagnosticMessage: z.string().trim().min(1),
    durableErrorCode: nonEmptyOptionalTextSchema,
    durableErrorMessage: nonEmptyOptionalTextSchema,
  })
  .strict()

export const notificationDispatchCycleSummarySchema = z
  .object({
    projectId: z.string().trim().min(1),
    pendingCount: z.number().int().nonnegative(),
    attemptedCount: z.number().int().nonnegative(),
    sentCount: z.number().int().nonnegative(),
    failedCount: z.number().int().nonnegative(),
    attemptLimit: z.number().int().positive(),
    attemptsTruncated: z.boolean(),
    attempts: z.array(notificationAdapterDispatchAttemptSchema),
    errorCodeCounts: z.array(notificationAdapterErrorCountSchema),
  })
  .strict()
  .superRefine((summary, ctx) => {
    if (summary.attemptedCount < summary.attempts.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptedCount'],
        message: 'Notification dispatch cycle summaries cannot report fewer attempts than included attempt rows.',
      })
    }

    if (summary.attempts.length > summary.attemptLimit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification dispatch cycle summaries cannot include attempt rows beyond `attemptLimit`.',
      })
    }

    if (summary.sentCount + summary.failedCount > summary.attemptedCount) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptedCount'],
        message: 'Notification dispatch cycle sent/failed counts cannot exceed attempted count.',
      })
    }
  })

export const notificationAdapterReplyAttemptSchema = z
  .object({
    routeId: z.string().trim().min(1),
    routeKind: z.string().trim().min(1),
    actionId: nonEmptyOptionalTextSchema,
    messageId: nonEmptyOptionalTextSchema,
    accepted: z.boolean(),
    diagnosticCode: z.string().trim().min(1),
    diagnosticMessage: z.string().trim().min(1),
    replyCode: nonEmptyOptionalTextSchema,
    replyMessage: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((attempt, ctx) => {
    if (attempt.accepted && (attempt.replyCode || attempt.replyMessage)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['accepted'],
        message: 'Accepted reply attempts must not include reply rejection diagnostics.',
      })
    }

    const hasReplyCode = Boolean(attempt.replyCode)
    const hasReplyMessage = Boolean(attempt.replyMessage)
    if (hasReplyCode !== hasReplyMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['replyCode'],
        message: 'Reply diagnostics must include both `replyCode` and `replyMessage` when present.',
      })
    }
  })

export const notificationReplyCycleSummarySchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeCount: z.number().int().nonnegative(),
    polledRouteCount: z.number().int().nonnegative(),
    messageCount: z.number().int().nonnegative(),
    acceptedCount: z.number().int().nonnegative(),
    rejectedCount: z.number().int().nonnegative(),
    attemptLimit: z.number().int().positive(),
    attemptsTruncated: z.boolean(),
    attempts: z.array(notificationAdapterReplyAttemptSchema),
    errorCodeCounts: z.array(notificationAdapterErrorCountSchema),
  })
  .strict()
  .superRefine((summary, ctx) => {
    if (summary.polledRouteCount > summary.routeCount) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['polledRouteCount'],
        message: 'Polled notification route count cannot exceed the total route count.',
      })
    }

    if (summary.attempts.length > summary.attemptLimit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification reply cycle summaries cannot include attempt rows beyond `attemptLimit`.',
      })
    }

    if (summary.acceptedCount + summary.rejectedCount < summary.attempts.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification reply cycle attempt rows cannot exceed accepted + rejected totals.',
      })
    }
  })

export const syncNotificationAdaptersRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const syncNotificationAdaptersResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    dispatch: notificationDispatchCycleSummarySchema,
    replies: notificationReplyCycleSummarySchema,
    syncedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.dispatch.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['dispatch', 'projectId'],
        message: 'Notification dispatch cycle summaries must match the sync response project id.',
      })
    }

    if (response.replies.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['replies', 'projectId'],
        message: 'Notification reply cycle summaries must match the sync response project id.',
      })
    }
  })

export const projectSnapshotResponseSchema = z
  .object({
    project: projectSummarySchema,
    repository: repositorySummarySchema.nullable(),
    phases: z.array(phaseSummarySchema),
    lifecycle: planningLifecycleProjectionSchema,
    approvalRequests: z.array(operatorApprovalSchema),
    verificationRecords: z.array(verificationRecordSchema),
    resumeHistory: z.array(resumeHistoryEntrySchema),
    handoffPackages: z.array(workflowHandoffPackageSchema).optional(),
    autonomousRun: z.lazy(() => autonomousRunSchema).nullable().optional(),
    autonomousUnit: z.lazy(() => autonomousUnitSchema).nullable().optional(),
    notificationDispatches: z.array(notificationDispatchSchema).optional(),
    notificationReplyClaims: z.array(notificationReplyClaimSchema).optional(),
  })
  .superRefine((snapshot, ctx) => {
    if (snapshot.autonomousRun && snapshot.autonomousRun.projectId !== snapshot.project.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['autonomousRun', 'projectId'],
        message: 'Autonomous run project id must match the selected project snapshot id.',
      })
    }

    if (snapshot.autonomousUnit && snapshot.autonomousUnit.projectId !== snapshot.project.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['autonomousUnit', 'projectId'],
        message: 'Autonomous unit project id must match the selected project snapshot id.',
      })
    }

    if (snapshot.autonomousRun && snapshot.autonomousUnit) {
      if (snapshot.autonomousUnit.runId !== snapshot.autonomousRun.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['autonomousUnit', 'runId'],
          message: 'Autonomous unit run id must match the active autonomous run id.',
        })
      }
    }
  })

export const branchSummarySchema = z.object({
  name: z.string().min(1),
  headSha: nullableTextSchema,
  detached: z.boolean(),
})

export const repositoryStatusEntrySchema = z.object({
  path: z.string().min(1),
  staged: changeKindSchema.nullable().optional(),
  unstaged: changeKindSchema.nullable().optional(),
  untracked: z.boolean(),
})

export const repositoryStatusResponseSchema = z.object({
  repository: repositorySummarySchema,
  branch: branchSummarySchema.nullable().optional(),
  entries: z.array(repositoryStatusEntrySchema),
  hasStagedChanges: z.boolean(),
  hasUnstagedChanges: z.boolean(),
  hasUntrackedChanges: z.boolean(),
})

export const repositoryDiffResponseSchema = z.object({
  repository: repositorySummarySchema,
  scope: repositoryDiffScopeSchema,
  patch: z.string(),
  truncated: z.boolean(),
  baseRevision: nullableTextSchema,
})

export const workflowGateStateSchema = z.enum(['pending', 'satisfied', 'blocked', 'skipped'])
export const workflowTransitionGateDecisionSchema = z.enum(['approved', 'rejected', 'blocked', 'not_applicable'])

export const workflowGraphNodeSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    phaseId: z.number().int().nonnegative(),
    sortOrder: z.number().int().nonnegative(),
    name: z.string().trim().min(1),
    description: z.string(),
    status: phaseStatusSchema,
    currentStep: phaseStepSchema.nullable().optional(),
    taskCount: z.number().int().nonnegative(),
    completedTasks: z.number().int().nonnegative(),
    summary: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphEdgeSchema = z
  .object({
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateRequirement: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphGateRequestSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    actionType: nonEmptyOptionalTextSchema,
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphGateMetadataSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    actionType: nonEmptyOptionalTextSchema,
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const upsertWorkflowGraphRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    nodes: z.array(workflowGraphNodeSchema),
    edges: z.array(workflowGraphEdgeSchema),
    gates: z.array(workflowGraphGateRequestSchema),
  })
  .strict()

export const upsertWorkflowGraphResponseSchema = z
  .object({
    nodes: z.array(workflowGraphNodeSchema),
    edges: z.array(workflowGraphEdgeSchema),
    gates: z.array(workflowGraphGateMetadataSchema),
    phases: z.array(phaseSummarySchema),
  })
  .strict()

export const workflowTransitionGateUpdateRequestSchema = z
  .object({
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const applyWorkflowTransitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateDecision: workflowTransitionGateDecisionSchema,
    gateDecisionContext: nonEmptyOptionalTextSchema,
    gateUpdates: z.array(workflowTransitionGateUpdateRequestSchema),
    occurredAt: isoTimestampSchema,
  })
  .strict()

export const workflowTransitionEventSchema = z
  .object({
    id: z.number().int().nonnegative(),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateDecision: workflowTransitionGateDecisionSchema,
    gateDecisionContext: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
  })
  .strict()

export const workflowAutomaticDispatchStatusSchema = z.enum(['no_continuation', 'applied', 'replayed', 'skipped'])
export const workflowAutomaticDispatchPackageStatusSchema = z.enum(['persisted', 'replayed', 'skipped'])

export const workflowAutomaticDispatchPackageOutcomeSchema = z
  .object({
    status: workflowAutomaticDispatchPackageStatusSchema,
    package: workflowHandoffPackageSchema.nullable().optional(),
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((outcome, ctx) => {
    if (outcome.status === 'skipped') {
      if (!outcome.code || !outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['code'],
          message: 'Skipped handoff package outcomes must include non-empty `code` and `message` diagnostics.',
        })
      }
      if (outcome.package) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['package'],
          message: 'Skipped handoff package outcomes must not include a persisted package payload.',
        })
      }
      return
    }

    if (!outcome.package) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['package'],
        message: 'Persisted/replayed handoff package outcomes must include a package payload.',
      })
    }

    if (outcome.code || outcome.message) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['code'],
        message: 'Persisted/replayed handoff package outcomes must not include skip diagnostics.',
      })
    }
  })

export const workflowAutomaticDispatchOutcomeSchema = z
  .object({
    status: workflowAutomaticDispatchStatusSchema,
    transitionEvent: workflowTransitionEventSchema.nullable().optional(),
    handoffPackage: workflowAutomaticDispatchPackageOutcomeSchema.nullable().optional(),
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((outcome, ctx) => {
    if (outcome.status === 'no_continuation') {
      if (outcome.transitionEvent || outcome.handoffPackage || outcome.code || outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'No-continuation automatic dispatch outcomes cannot include transition, package, or diagnostic payloads.',
        })
      }
      return
    }

    if (outcome.status === 'skipped') {
      if (!outcome.code || !outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['code'],
          message: 'Skipped automatic dispatch outcomes must include non-empty `code` and `message` diagnostics.',
        })
      }

      if (outcome.transitionEvent || outcome.handoffPackage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['transitionEvent'],
          message: 'Skipped automatic dispatch outcomes must not include transition or handoff payloads.',
        })
      }
      return
    }

    if (!outcome.transitionEvent || !outcome.handoffPackage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['transitionEvent'],
        message: 'Applied/replayed automatic dispatch outcomes must include transition and handoff payloads.',
      })
    }

    if (outcome.code || outcome.message) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['code'],
        message: 'Applied/replayed automatic dispatch outcomes must not include skip diagnostics.',
      })
    }
  })

export const resumeOperatorRunResponseSchema = z.object({
  approvalRequest: operatorApprovalSchema,
  resumeEntry: resumeHistoryEntrySchema,
  automaticDispatch: workflowAutomaticDispatchOutcomeSchema.optional(),
})

export const applyWorkflowTransitionResponseSchema = z
  .object({
    transitionEvent: workflowTransitionEventSchema,
    automaticDispatch: workflowAutomaticDispatchOutcomeSchema.optional(),
    phases: z.array(phaseSummarySchema),
  })
  .strict()

export const projectUpdatedPayloadSchema = z.object({
  project: projectSummarySchema,
  reason: z.enum(['imported', 'refreshed', 'metadata_changed']),
})

export const repositoryStatusChangedPayloadSchema = z.object({
  projectId: z.string().min(1),
  repositoryId: z.string().min(1),
  status: repositoryStatusResponseSchema,
})

export const runtimeAuthPhaseSchema = z.enum([
  'idle',
  'starting',
  'awaiting_browser_callback',
  'awaiting_manual_input',
  'exchanging_code',
  'authenticated',
  'refreshing',
  'cancelled',
  'failed',
])

export const runtimeDiagnosticSchema = z.object({
  code: z.string().trim().min(1),
  message: z.string().trim().min(1),
  retryable: z.boolean(),
})

export const runtimeProviderIdSchema = z.enum(['openrouter', 'openai_codex'])

function validateRuntimeSettingsProviderModel(
  payload: { providerId: z.infer<typeof runtimeProviderIdSchema>; modelId: string },
  ctx: z.RefinementCtx,
): void {
  if (payload.providerId === 'openai_codex' && payload.modelId !== 'openai_codex') {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['modelId'],
      message: 'Cadence only supports modelId `openai_codex` for provider `openai_codex`.',
    })
  }
}

export const runtimeSettingsSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    modelId: z.string().trim().min(1),
    openrouterApiKeyConfigured: z.boolean(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    validateRuntimeSettingsProviderModel(payload, ctx)
  })

export const upsertRuntimeSettingsRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    modelId: z.string().trim().min(1),
    openrouterApiKey: z.string().nullable().optional(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    validateRuntimeSettingsProviderModel(payload, ctx)
  })

export const runtimeSessionSchema = z.object({
  projectId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  providerId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  accountId: nonEmptyOptionalTextSchema,
  phase: runtimeAuthPhaseSchema,
  callbackBound: z.boolean().nullable().optional(),
  authorizationUrl: z.string().url().nullable().optional(),
  redirectUri: z.string().url().nullable().optional(),
  lastErrorCode: nonEmptyOptionalTextSchema,
  lastError: runtimeDiagnosticSchema.nullable().optional(),
  updatedAt: isoTimestampSchema,
})

export const runtimeUpdatedPayloadSchema = z.object({
  projectId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  providerId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  accountId: nonEmptyOptionalTextSchema,
  authPhase: runtimeAuthPhaseSchema,
  lastErrorCode: nonEmptyOptionalTextSchema,
  lastError: runtimeDiagnosticSchema.nullable().optional(),
  updatedAt: isoTimestampSchema,
})

export const runtimeRunStatusSchema = z.enum(['starting', 'running', 'stale', 'stopped', 'failed'])
export const runtimeRunTransportLivenessSchema = z.enum(['unknown', 'reachable', 'unreachable'])
export const runtimeRunCheckpointKindSchema = z.enum(['bootstrap', 'state', 'tool', 'action_required', 'diagnostic'])

export const runtimeRunDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const runtimeRunTransportSchema = z
  .object({
    kind: z.string().trim().min(1),
    endpoint: z.string().trim().min(1),
    liveness: runtimeRunTransportLivenessSchema,
  })
  .strict()

export const runtimeRunCheckpointSchema = z
  .object({
    sequence: z.number().int().nonnegative(),
    kind: runtimeRunCheckpointKindSchema,
    summary: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const runtimeRunSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    supervisorKind: z.string().trim().min(1),
    status: runtimeRunStatusSchema,
    transport: runtimeRunTransportSchema,
    startedAt: isoTimestampSchema,
    lastHeartbeatAt: nonEmptyOptionalTextSchema,
    lastCheckpointSequence: z.number().int().nonnegative(),
    lastCheckpointAt: nonEmptyOptionalTextSchema,
    stoppedAt: nonEmptyOptionalTextSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
    checkpoints: z.array(runtimeRunCheckpointSchema),
  })
  .strict()

export const runtimeRunUpdatedPayloadSchema = z
  .object({
    projectId: z.string().trim().min(1),
    run: runtimeRunSchema.nullable(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    if (payload.run && payload.run.projectId !== payload.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['run', 'projectId'],
        message: 'Cadence received a runtime-run update for a different project than the event envelope.',
      })
    }
  })

export const autonomousRunStatusSchema = z.enum([
  'starting',
  'running',
  'paused',
  'cancelling',
  'cancelled',
  'stale',
  'failed',
  'stopped',
  'crashed',
  'completed',
])
export const autonomousRunRecoveryStateSchema = z.enum(['healthy', 'recovery_required', 'terminal', 'failed'])
export const autonomousUnitKindSchema = z.enum(['bootstrap', 'state', 'tool', 'action_required', 'diagnostic'])
export const autonomousUnitStatusSchema = z.enum([
  'pending',
  'active',
  'blocked',
  'paused',
  'completed',
  'cancelled',
  'failed',
])
export const autonomousUnitArtifactStatusSchema = z.enum(['pending', 'recorded', 'rejected', 'redacted'])
export const autonomousToolCallStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const autonomousVerificationOutcomeSchema = z.enum(['passed', 'failed', 'blocked'])

export const autonomousLifecycleReasonSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const autonomousCommandResultSchema = z
  .object({
    exitCode: z.number().int().nullable().optional(),
    timedOut: z.boolean(),
    summary: z.string().trim().min(1),
  })
  .strict()

export const gitToolResultScopeSchema = z.enum(['staged', 'unstaged', 'worktree'])
export const webToolResultContentKindSchema = z.enum(['html', 'plain_text'])

export const toolResultSummarySchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('command'),
      exitCode: z.number().int().nullable().optional(),
      timedOut: z.boolean(),
      stdoutTruncated: z.boolean(),
      stderrTruncated: z.boolean(),
      stdoutRedacted: z.boolean(),
      stderrRedacted: z.boolean(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('file'),
      path: nonEmptyOptionalTextSchema,
      scope: nonEmptyOptionalTextSchema,
      lineCount: z.number().int().nonnegative().nullable().optional(),
      matchCount: z.number().int().nonnegative().nullable().optional(),
      truncated: z.boolean(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('git'),
      scope: gitToolResultScopeSchema.nullable().optional(),
      changedFiles: z.number().int().nonnegative(),
      truncated: z.boolean(),
      baseRevision: nonEmptyOptionalTextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('web'),
      target: z.string().trim().min(1),
      resultCount: z.number().int().nonnegative().nullable().optional(),
      finalUrl: nonEmptyOptionalTextSchema,
      contentKind: webToolResultContentKindSchema.nullable().optional(),
      contentType: nonEmptyOptionalTextSchema,
      truncated: z.boolean(),
    })
    .strict(),
])

export const autonomousToolResultPayloadSchema = z
  .object({
    kind: z.literal('tool_result'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    toolCallId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    toolState: autonomousToolCallStateSchema,
    commandResult: autonomousCommandResultSchema.nullable().optional(),
    toolSummary: toolResultSummarySchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousVerificationEvidencePayloadSchema = z
  .object({
    kind: z.literal('verification_evidence'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    evidenceKind: z.string().trim().min(1),
    label: z.string().trim().min(1),
    outcome: autonomousVerificationOutcomeSchema,
    commandResult: autonomousCommandResultSchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousPolicyDeniedPayloadSchema = z
  .object({
    kind: z.literal('policy_denied'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    diagnosticCode: z.string().trim().min(1),
    message: z.string().trim().min(1),
    toolName: nonEmptyOptionalTextSchema,
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousArtifactPayloadSchema = z.discriminatedUnion('kind', [
  autonomousToolResultPayloadSchema,
  autonomousVerificationEvidencePayloadSchema,
  autonomousPolicyDeniedPayloadSchema,
])

export const autonomousRunSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    supervisorKind: z.string().trim().min(1),
    status: autonomousRunStatusSchema,
    recoveryState: autonomousRunRecoveryStateSchema,
    activeUnitId: nonEmptyOptionalTextSchema,
    activeAttemptId: nonEmptyOptionalTextSchema,
    duplicateStartDetected: z.boolean(),
    duplicateStartRunId: nonEmptyOptionalTextSchema,
    duplicateStartReason: nonEmptyOptionalTextSchema,
    startedAt: isoTimestampSchema,
    lastHeartbeatAt: nonEmptyOptionalTextSchema,
    lastCheckpointAt: nonEmptyOptionalTextSchema,
    pausedAt: nonEmptyOptionalTextSchema,
    cancelledAt: nonEmptyOptionalTextSchema,
    completedAt: nonEmptyOptionalTextSchema,
    crashedAt: nonEmptyOptionalTextSchema,
    stoppedAt: nonEmptyOptionalTextSchema,
    pauseReason: autonomousLifecycleReasonSchema.nullable().optional(),
    cancelReason: autonomousLifecycleReasonSchema.nullable().optional(),
    crashReason: autonomousLifecycleReasonSchema.nullable().optional(),
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const autonomousWorkflowLinkageSchema = z
  .object({
    workflowNodeId: z.string().trim().min(1),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    handoffTransitionId: z.string().trim().min(1),
    handoffPackageHash: z
      .string()
      .regex(/^[0-9a-f]{64}$/, 'Autonomous workflow linkage handoff package hashes must be lowercase 64-character hex digests.'),
  })
  .strict()

export const autonomousUnitSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    sequence: z.number().int().nonnegative(),
    kind: autonomousUnitKindSchema,
    status: autonomousUnitStatusSchema,
    summary: z.string().trim().min(1),
    boundaryId: nonEmptyOptionalTextSchema,
    workflowLinkage: autonomousWorkflowLinkageSchema.nullable().optional(),
    startedAt: isoTimestampSchema,
    finishedAt: nonEmptyOptionalTextSchema,
    updatedAt: isoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
  })
  .strict()

export const autonomousUnitAttemptSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    attemptNumber: z.number().int().nonnegative(),
    childSessionId: z.string().trim().min(1),
    status: autonomousUnitStatusSchema,
    boundaryId: nonEmptyOptionalTextSchema,
    workflowLinkage: autonomousWorkflowLinkageSchema.nullable().optional(),
    startedAt: isoTimestampSchema,
    finishedAt: nonEmptyOptionalTextSchema,
    updatedAt: isoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
  })
  .strict()

export const autonomousUnitArtifactSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    artifactKind: z.string().trim().min(1),
    status: autonomousUnitArtifactStatusSchema,
    summary: z.string().trim().min(1),
    contentHash: nonEmptyOptionalTextSchema,
    payload: autonomousArtifactPayloadSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((artifact, ctx) => {
    const payload = artifact.payload
    if (!payload) {
      return
    }

    if (payload.projectId !== artifact.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'projectId'],
        message: 'Autonomous artifact payload project id must match the enclosing artifact project id.',
      })
    }

    if (payload.runId !== artifact.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'runId'],
        message: 'Autonomous artifact payload run id must match the enclosing artifact run id.',
      })
    }

    if (payload.unitId !== artifact.unitId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'unitId'],
        message: 'Autonomous artifact payload unit id must match the enclosing artifact unit id.',
      })
    }

    if (payload.attemptId !== artifact.attemptId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'attemptId'],
        message: 'Autonomous artifact payload attempt id must match the enclosing artifact attempt id.',
      })
    }

    if (payload.artifactId !== artifact.artifactId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'artifactId'],
        message: 'Autonomous artifact payload artifact id must match the enclosing artifact id.',
      })
    }
  })

export const autonomousUnitHistoryEntrySchema = z
  .object({
    unit: autonomousUnitSchema,
    latestAttempt: autonomousUnitAttemptSchema.nullable().optional(),
    artifacts: z.array(autonomousUnitArtifactSchema).optional(),
  })
  .strict()
  .superRefine((entry, ctx) => {
    const latestAttempt = entry.latestAttempt ?? null

    if (latestAttempt) {
      if (latestAttempt.projectId !== entry.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'projectId'],
          message: 'Autonomous history attempt project id must match the enclosing unit project id.',
        })
      }

      if (latestAttempt.runId !== entry.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'runId'],
          message: 'Autonomous history attempt run id must match the enclosing unit run id.',
        })
      }

      if (latestAttempt.unitId !== entry.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'unitId'],
          message: 'Autonomous history attempt unit id must match the enclosing unit id.',
        })
      }
    }

    entry.artifacts?.forEach((artifact, index) => {
      if (artifact.projectId !== entry.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'projectId'],
          message: 'Autonomous history artifacts must reference the same project as the enclosing unit.',
        })
      }

      if (artifact.runId !== entry.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'runId'],
          message: 'Autonomous history artifacts must reference the same run as the enclosing unit.',
        })
      }

      if (artifact.unitId !== entry.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'unitId'],
          message: 'Autonomous history artifacts must reference the same unit as the enclosing history entry.',
        })
      }

      if (latestAttempt && artifact.attemptId !== latestAttempt.attemptId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'attemptId'],
          message: 'Autonomous history artifacts must reference the latest attempt id for the enclosing history entry.',
        })
      }
    })
  })

export const autonomousRunStateSchema = z
  .object({
    run: autonomousRunSchema.nullable(),
    unit: autonomousUnitSchema.nullable(),
    attempt: autonomousUnitAttemptSchema.nullable().optional(),
    history: z.array(autonomousUnitHistoryEntrySchema).optional(),
  })
  .strict()
  .superRefine((state, ctx) => {
    const attempt = state.attempt ?? null
    const history = state.history ?? []

    if (state.run && state.unit) {
      if (state.unit.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['unit', 'projectId'],
          message: 'Autonomous unit project id must match the autonomous run project id.',
        })
      }

      if (state.unit.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['unit', 'runId'],
          message: 'Autonomous unit run id must match the autonomous run run id.',
        })
      }
    }

    if (state.run && attempt) {
      if (attempt.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'projectId'],
          message: 'Autonomous attempt project id must match the autonomous run project id.',
        })
      }

      if (attempt.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'runId'],
          message: 'Autonomous attempt run id must match the autonomous run run id.',
        })
      }

      if (state.run.activeAttemptId && attempt.attemptId !== state.run.activeAttemptId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'attemptId'],
          message: 'Autonomous attempt id must match the active attempt id reported on the run.',
        })
      }
    }

    if (state.unit && attempt) {
      if (attempt.projectId !== state.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'projectId'],
          message: 'Autonomous attempt project id must match the autonomous unit project id.',
        })
      }

      if (attempt.runId !== state.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'runId'],
          message: 'Autonomous attempt run id must match the autonomous unit run id.',
        })
      }

      if (attempt.unitId !== state.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'unitId'],
          message: 'Autonomous attempt unit id must match the autonomous unit id.',
        })
      }
    }

    history.forEach((entry, index) => {
      if (state.run && entry.unit.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['history', index, 'unit', 'projectId'],
          message: 'Autonomous history unit project id must match the autonomous run project id.',
        })
      }

      if (state.run && entry.unit.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['history', index, 'unit', 'runId'],
          message: 'Autonomous history unit run id must match the autonomous run run id.',
        })
      }
    })
  })

export const runtimeToolCallStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const runtimeStreamItemKindSchema = z.enum([
  'transcript',
  'tool',
  'activity',
  'action_required',
  'complete',
  'failure',
])

export const runtimeStreamItemSchema = z.object({
  kind: runtimeStreamItemKindSchema,
  runId: z.string().trim().min(1),
  sequence: z.number().int().positive(),
  sessionId: nonEmptyOptionalTextSchema,
  flowId: nonEmptyOptionalTextSchema,
  text: nonEmptyOptionalTextSchema,
  toolCallId: nonEmptyOptionalTextSchema,
  toolName: nonEmptyOptionalTextSchema,
  toolState: runtimeToolCallStateSchema.nullable().optional(),
  toolSummary: toolResultSummarySchema.nullable().optional(),
  actionId: nonEmptyOptionalTextSchema,
  boundaryId: nonEmptyOptionalTextSchema,
  actionType: nonEmptyOptionalTextSchema,
  title: nonEmptyOptionalTextSchema,
  detail: nonEmptyOptionalTextSchema,
  code: nonEmptyOptionalTextSchema,
  message: nonEmptyOptionalTextSchema,
  retryable: z.boolean().nullable().optional(),
  createdAt: isoTimestampSchema,
})

export const subscribeRuntimeStreamRequestSchema = z.object({
  projectId: z.string().trim().min(1),
  itemKinds: z.array(runtimeStreamItemKindSchema).min(1),
})

export const subscribeRuntimeStreamResponseSchema = z.object({
  projectId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  runId: z.string().trim().min(1),
  sessionId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  subscribedItemKinds: z.array(runtimeStreamItemKindSchema).min(1),
})

export type ProjectSummaryDto = z.infer<typeof projectSummarySchema>
export type PhaseSummaryDto = z.infer<typeof phaseSummarySchema>
export type PlanningLifecycleStageKindDto = z.infer<typeof planningLifecycleStageKindSchema>
export type PlanningLifecycleStageDto = z.infer<typeof planningLifecycleStageSchema>
export type PlanningLifecycleProjectionDto = z.infer<typeof planningLifecycleProjectionSchema>
export type RepositorySummaryDto = z.infer<typeof repositorySummarySchema>
export type RepositoryDiffScope = z.infer<typeof repositoryDiffScopeSchema>
export type ImportRepositoryResponseDto = z.infer<typeof importRepositoryResponseSchema>
export type ListProjectsResponseDto = z.infer<typeof listProjectsResponseSchema>
export type NotificationRouteKindDto = z.infer<typeof notificationRouteKindSchema>
export type NotificationDispatchStatusDto = z.infer<typeof notificationDispatchStatusSchema>
export type NotificationReplyClaimStatusDto = z.infer<typeof notificationReplyClaimStatusSchema>
export type NotificationDispatchOutcomeStatusDto = z.infer<typeof notificationDispatchOutcomeStatusSchema>
export type NotificationRouteCredentialReadinessStatusDto = z.infer<
  typeof notificationRouteCredentialReadinessStatusSchema
>
export type NotificationRouteCredentialReadinessDiagnosticDto = z.infer<
  typeof notificationRouteCredentialReadinessDiagnosticSchema
>
export type NotificationRouteCredentialReadinessDto = z.infer<
  typeof notificationRouteCredentialReadinessSchema
>
export type NotificationRouteDto = z.infer<typeof notificationRouteSchema>
export type NotificationDispatchDto = z.infer<typeof notificationDispatchSchema>
export type NotificationReplyClaimDto = z.infer<typeof notificationReplyClaimSchema>
export type NotificationAdapterErrorCountDto = z.infer<typeof notificationAdapterErrorCountSchema>
export type NotificationAdapterDispatchAttemptDto = z.infer<typeof notificationAdapterDispatchAttemptSchema>
export type NotificationDispatchCycleSummaryDto = z.infer<typeof notificationDispatchCycleSummarySchema>
export type NotificationAdapterReplyAttemptDto = z.infer<typeof notificationAdapterReplyAttemptSchema>
export type NotificationReplyCycleSummaryDto = z.infer<typeof notificationReplyCycleSummarySchema>
export type ListNotificationRoutesRequestDto = z.infer<typeof listNotificationRoutesRequestSchema>
export type ListNotificationRoutesResponseDto = z.infer<typeof listNotificationRoutesResponseSchema>
export type UpsertNotificationRouteRequestDto = z.infer<typeof upsertNotificationRouteRequestSchema>
export type UpsertNotificationRouteResponseDto = z.infer<typeof upsertNotificationRouteResponseSchema>
export type NotificationRouteCredentialPayloadDto = z.infer<typeof notificationRouteCredentialPayloadSchema>
export type UpsertNotificationRouteCredentialsRequestDto = z.infer<
  typeof upsertNotificationRouteCredentialsRequestSchema
>
export type UpsertNotificationRouteCredentialsResponseDto = z.infer<
  typeof upsertNotificationRouteCredentialsResponseSchema
>
export type ListNotificationDispatchesRequestDto = z.infer<typeof listNotificationDispatchesRequestSchema>
export type ListNotificationDispatchesResponseDto = z.infer<typeof listNotificationDispatchesResponseSchema>
export type RecordNotificationDispatchOutcomeRequestDto = z.infer<typeof recordNotificationDispatchOutcomeRequestSchema>
export type RecordNotificationDispatchOutcomeResponseDto = z.infer<typeof recordNotificationDispatchOutcomeResponseSchema>
export type SubmitNotificationReplyRequestDto = z.infer<typeof submitNotificationReplyRequestSchema>
export type SubmitNotificationReplyResponseDto = z.infer<typeof submitNotificationReplyResponseSchema>
export type SyncNotificationAdaptersRequestDto = z.infer<typeof syncNotificationAdaptersRequestSchema>
export type SyncNotificationAdaptersResponseDto = z.infer<typeof syncNotificationAdaptersResponseSchema>
export type OperatorApprovalStatusDto = z.infer<typeof operatorApprovalStatusSchema>
export type VerificationRecordStatusDto = z.infer<typeof verificationRecordStatusSchema>
export type ResumeHistoryStatusDto = z.infer<typeof resumeHistoryStatusSchema>
export type OperatorApprovalDto = z.infer<typeof operatorApprovalSchema>
export type VerificationRecordDto = z.infer<typeof verificationRecordSchema>
export type ResumeHistoryEntryDto = z.infer<typeof resumeHistoryEntrySchema>
export type ResolveOperatorActionRequestDto = z.infer<typeof resolveOperatorActionRequestSchema>
export type ResolveOperatorActionResponseDto = z.infer<typeof resolveOperatorActionResponseSchema>
export type WorkflowHandoffPackageDto = z.infer<typeof workflowHandoffPackageSchema>
export type ResumeOperatorRunRequestDto = z.infer<typeof resumeOperatorRunRequestSchema>
export type ResumeOperatorRunResponseDto = z.infer<typeof resumeOperatorRunResponseSchema>
export type ProjectSnapshotResponseDto = z.infer<typeof projectSnapshotResponseSchema>
export type RepositoryStatusResponseDto = z.infer<typeof repositoryStatusResponseSchema>
export type RepositoryDiffResponseDto = z.infer<typeof repositoryDiffResponseSchema>
export type WorkflowGateStateDto = z.infer<typeof workflowGateStateSchema>
export type WorkflowTransitionGateDecisionDto = z.infer<typeof workflowTransitionGateDecisionSchema>
export type WorkflowAutomaticDispatchStatusDto = z.infer<typeof workflowAutomaticDispatchStatusSchema>
export type WorkflowAutomaticDispatchPackageStatusDto = z.infer<typeof workflowAutomaticDispatchPackageStatusSchema>
export type WorkflowAutomaticDispatchPackageOutcomeDto = z.infer<typeof workflowAutomaticDispatchPackageOutcomeSchema>
export type WorkflowAutomaticDispatchOutcomeDto = z.infer<typeof workflowAutomaticDispatchOutcomeSchema>
export type WorkflowGraphNodeDto = z.infer<typeof workflowGraphNodeSchema>
export type WorkflowGraphEdgeDto = z.infer<typeof workflowGraphEdgeSchema>
export type WorkflowGraphGateRequestDto = z.infer<typeof workflowGraphGateRequestSchema>
export type WorkflowGraphGateMetadataDto = z.infer<typeof workflowGraphGateMetadataSchema>
export type UpsertWorkflowGraphRequestDto = z.infer<typeof upsertWorkflowGraphRequestSchema>
export type UpsertWorkflowGraphResponseDto = z.infer<typeof upsertWorkflowGraphResponseSchema>
export type WorkflowTransitionGateUpdateRequestDto = z.infer<typeof workflowTransitionGateUpdateRequestSchema>
export type ApplyWorkflowTransitionRequestDto = z.infer<typeof applyWorkflowTransitionRequestSchema>
export type WorkflowTransitionEventDto = z.infer<typeof workflowTransitionEventSchema>
export type ApplyWorkflowTransitionResponseDto = z.infer<typeof applyWorkflowTransitionResponseSchema>
export type ProjectUpdatedPayloadDto = z.infer<typeof projectUpdatedPayloadSchema>
export type RepositoryStatusChangedPayloadDto = z.infer<typeof repositoryStatusChangedPayloadSchema>
export type RuntimeAuthPhaseDto = z.infer<typeof runtimeAuthPhaseSchema>
export type RuntimeDiagnosticDto = z.infer<typeof runtimeDiagnosticSchema>
export type RuntimeProviderIdDto = z.infer<typeof runtimeProviderIdSchema>
export type RuntimeSettingsDto = z.infer<typeof runtimeSettingsSchema>
export type UpsertRuntimeSettingsRequestDto = z.infer<typeof upsertRuntimeSettingsRequestSchema>
export type RuntimeSessionDto = z.infer<typeof runtimeSessionSchema>
export type RuntimeUpdatedPayloadDto = z.infer<typeof runtimeUpdatedPayloadSchema>
export type RuntimeRunStatusDto = z.infer<typeof runtimeRunStatusSchema>
export type RuntimeRunTransportLivenessDto = z.infer<typeof runtimeRunTransportLivenessSchema>
export type RuntimeRunCheckpointKindDto = z.infer<typeof runtimeRunCheckpointKindSchema>
export type RuntimeRunDiagnosticDto = z.infer<typeof runtimeRunDiagnosticSchema>
export type RuntimeRunTransportDto = z.infer<typeof runtimeRunTransportSchema>
export type RuntimeRunCheckpointDto = z.infer<typeof runtimeRunCheckpointSchema>
export type RuntimeRunDto = z.infer<typeof runtimeRunSchema>
export type RuntimeRunUpdatedPayloadDto = z.infer<typeof runtimeRunUpdatedPayloadSchema>
export type AutonomousRunStatusDto = z.infer<typeof autonomousRunStatusSchema>
export type AutonomousRunRecoveryStateDto = z.infer<typeof autonomousRunRecoveryStateSchema>
export type AutonomousUnitKindDto = z.infer<typeof autonomousUnitKindSchema>
export type AutonomousUnitStatusDto = z.infer<typeof autonomousUnitStatusSchema>
export type AutonomousUnitArtifactStatusDto = z.infer<typeof autonomousUnitArtifactStatusSchema>
export type AutonomousWorkflowLinkageDto = z.infer<typeof autonomousWorkflowLinkageSchema>
export type AutonomousToolCallStateDto = z.infer<typeof autonomousToolCallStateSchema>
export type AutonomousVerificationOutcomeDto = z.infer<typeof autonomousVerificationOutcomeSchema>
export type AutonomousLifecycleReasonDto = z.infer<typeof autonomousLifecycleReasonSchema>
export type AutonomousCommandResultDto = z.infer<typeof autonomousCommandResultSchema>
export type GitToolResultScopeDto = z.infer<typeof gitToolResultScopeSchema>
export type WebToolResultContentKindDto = z.infer<typeof webToolResultContentKindSchema>
export type ToolResultSummaryDto = z.infer<typeof toolResultSummarySchema>
export type AutonomousToolResultPayloadDto = z.infer<typeof autonomousToolResultPayloadSchema>
export type AutonomousVerificationEvidencePayloadDto = z.infer<typeof autonomousVerificationEvidencePayloadSchema>
export type AutonomousPolicyDeniedPayloadDto = z.infer<typeof autonomousPolicyDeniedPayloadSchema>
export type AutonomousArtifactPayloadDto = z.infer<typeof autonomousArtifactPayloadSchema>
export type AutonomousRunDto = z.infer<typeof autonomousRunSchema>
export type AutonomousUnitDto = z.infer<typeof autonomousUnitSchema>
export type AutonomousUnitAttemptDto = z.infer<typeof autonomousUnitAttemptSchema>
export type AutonomousUnitArtifactDto = z.infer<typeof autonomousUnitArtifactSchema>
export type AutonomousUnitHistoryEntryDto = z.infer<typeof autonomousUnitHistoryEntrySchema>
export type AutonomousRunStateDto = z.infer<typeof autonomousRunStateSchema>
export type RuntimeToolCallStateDto = z.infer<typeof runtimeToolCallStateSchema>
export type RuntimeStreamItemKindDto = z.infer<typeof runtimeStreamItemKindSchema>
export type RuntimeStreamItemDto = z.infer<typeof runtimeStreamItemSchema>
export type SubscribeRuntimeStreamRequestDto = z.infer<typeof subscribeRuntimeStreamRequestSchema>
export type SubscribeRuntimeStreamResponseDto = z.infer<typeof subscribeRuntimeStreamResponseSchema>

export interface ProjectListItem {
  id: string
  name: string
  description: string
  milestone: string
  totalPhases: number
  completedPhases: number
  activePhase: number
  branch: string
  runtime: string
  branchLabel: string
  runtimeLabel: string
  phaseProgressPercent: number
}

export interface RepositoryView {
  id: string
  projectId: string
  rootPath: string
  displayName: string
  branch: string | null
  branchLabel: string
  headSha: string | null
  headShaLabel: string
  isGitRepo: boolean
}

export interface RepositoryStatusEntryView {
  path: string
  staged: z.infer<typeof changeKindSchema> | null
  unstaged: z.infer<typeof changeKindSchema> | null
  untracked: boolean
}

export interface RepositoryStatusView {
  projectId: string
  repositoryId: string
  branchLabel: string
  headShaLabel: string
  stagedCount: number
  unstagedCount: number
  untrackedCount: number
  statusCount: number
  hasChanges: boolean
  entries: RepositoryStatusEntryView[]
}

export interface RepositoryDiffView {
  projectId: string
  repositoryId: string
  scope: RepositoryDiffScope
  patch: string
  isEmpty: boolean
  truncated: boolean
  baseRevisionLabel: string
}

export interface OperatorApprovalView {
  actionId: string
  sessionId: string | null
  flowId: string | null
  actionType: string
  title: string
  detail: string
  gateNodeId: string | null
  gateKey: string | null
  transitionFromNodeId: string | null
  transitionToNodeId: string | null
  transitionKind: string | null
  userAnswer: string | null
  status: OperatorApprovalStatusDto
  statusLabel: string
  decisionNote: string | null
  createdAt: string
  updatedAt: string
  resolvedAt: string | null
  isPending: boolean
  isResolved: boolean
  canResume: boolean
  isGateLinked: boolean
  isRuntimeResumable: boolean
  requiresUserAnswer: boolean
  answerRequirementReason: OperatorApprovalAnswerRequirementReason
  answerRequirementLabel: string
  answerShapeKind: OperatorApprovalAnswerShapeKind
  answerShapeLabel: string
  answerShapeHint: string
  answerPlaceholder: string
}

export interface VerificationRecordView {
  id: number
  sourceActionId: string | null
  status: VerificationRecordStatusDto
  statusLabel: string
  summary: string
  detail: string | null
  recordedAt: string
}

export interface ResumeHistoryEntryView {
  id: number
  sourceActionId: string | null
  sessionId: string | null
  status: ResumeHistoryStatusDto
  statusLabel: string
  summary: string
  createdAt: string
}

export interface WorkflowHandoffPackageView {
  id: number
  projectId: string
  handoffTransitionId: string
  causalTransitionId: string | null
  fromNodeId: string
  toNodeId: string
  transitionKind: string
  packagePayload: string
  packageHash: string
  createdAt: string
}

export interface NotificationDispatchView {
  id: number
  projectId: string
  actionId: string
  routeId: string
  correlationKey: string
  status: NotificationDispatchStatusDto
  statusLabel: string
  attemptCount: number
  lastAttemptAt: string | null
  deliveredAt: string | null
  claimedAt: string | null
  lastErrorCode: string | null
  lastErrorMessage: string | null
  createdAt: string
  updatedAt: string
  isPending: boolean
  isSent: boolean
  isFailed: boolean
  isClaimed: boolean
  hasFailureDiagnostics: boolean
}

export interface NotificationBrokerActionView {
  actionId: string
  dispatches: NotificationDispatchView[]
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  hasFailures: boolean
  hasPending: boolean
  hasClaimed: boolean
}

export interface NotificationBrokerRouteView {
  routeId: string
  dispatches: NotificationDispatchView[]
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  latestFailureAt: string | null
  latestFailureCode: string | null
  latestFailureMessage: string | null
  hasFailures: boolean
  hasPending: boolean
}

export interface NotificationBrokerView {
  dispatches: NotificationDispatchView[]
  actions: NotificationBrokerActionView[]
  routes: NotificationBrokerRouteView[]
  byActionId: Record<string, NotificationBrokerActionView>
  byRouteId: Record<string, NotificationBrokerRouteView>
  dispatchCount: number
  routeCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  isTruncated: boolean
  totalBeforeTruncation: number
}

export interface OperatorDecisionOutcomeView {
  actionId: string
  title: string
  status: Extract<OperatorApprovalStatusDto, 'approved' | 'rejected'>
  statusLabel: string
  gateNodeId: string | null
  gateKey: string | null
  userAnswer: string | null
  decisionNote: string | null
  resolvedAt: string
}

export interface PlanningLifecycleStageView {
  stage: PlanningLifecycleStageKindDto
  stageLabel: string
  nodeId: string
  nodeLabel: string
  status: PhaseStatus
  statusLabel: string
  actionRequired: boolean
  lastTransitionAt: string | null
}

export interface PlanningLifecycleView {
  stages: PlanningLifecycleStageView[]
  byStage: Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView | null>
  hasStages: boolean
  activeStage: PlanningLifecycleStageView | null
  actionRequiredCount: number
  blockedCount: number
  completedCount: number
  percentComplete: number
}

export interface RuntimeSessionView {
  projectId: string
  runtimeKind: string
  providerId: string
  flowId: string | null
  sessionId: string | null
  accountId: string | null
  phase: RuntimeAuthPhaseDto
  phaseLabel: string
  runtimeLabel: string
  accountLabel: string
  sessionLabel: string
  callbackBound: boolean | null
  authorizationUrl: string | null
  redirectUri: string | null
  lastErrorCode: string | null
  lastError: RuntimeDiagnosticDto | null
  updatedAt: string
  isAuthenticated: boolean
  isLoginInProgress: boolean
  needsManualInput: boolean
  isSignedOut: boolean
  isFailed: boolean
}

export interface RuntimeRunTransportView {
  kind: string
  endpoint: string
  liveness: RuntimeRunTransportLivenessDto
  livenessLabel: string
}

export interface RuntimeRunCheckpointView {
  sequence: number
  kind: RuntimeRunCheckpointKindDto
  kindLabel: string
  summary: string
  createdAt: string
}

export interface RuntimeRunView {
  projectId: string
  runId: string
  runtimeKind: string
  providerId: string
  runtimeLabel: string
  supervisorKind: string
  supervisorLabel: string
  status: RuntimeRunStatusDto
  statusLabel: string
  transport: RuntimeRunTransportView
  startedAt: string
  lastHeartbeatAt: string | null
  lastCheckpointSequence: number
  lastCheckpointAt: string | null
  stoppedAt: string | null
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  updatedAt: string
  checkpoints: RuntimeRunCheckpointView[]
  latestCheckpoint: RuntimeRunCheckpointView | null
  checkpointCount: number
  hasCheckpoints: boolean
  isActive: boolean
  isTerminal: boolean
  isStale: boolean
  isFailed: boolean
}

export interface AutonomousLifecycleReasonView {
  code: string
  message: string
}

export interface AutonomousRunView {
  projectId: string
  runId: string
  runtimeKind: string
  providerId: string
  runtimeLabel: string
  supervisorKind: string
  supervisorLabel: string
  status: AutonomousRunStatusDto
  statusLabel: string
  recoveryState: AutonomousRunRecoveryStateDto
  recoveryLabel: string
  activeUnitId: string | null
  activeAttemptId: string | null
  duplicateStartDetected: boolean
  duplicateStartRunId: string | null
  duplicateStartReason: string | null
  startedAt: string
  lastHeartbeatAt: string | null
  lastCheckpointAt: string | null
  pausedAt: string | null
  cancelledAt: string | null
  completedAt: string | null
  crashedAt: string | null
  stoppedAt: string | null
  pauseReason: AutonomousLifecycleReasonView | null
  cancelReason: AutonomousLifecycleReasonView | null
  crashReason: AutonomousLifecycleReasonView | null
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  updatedAt: string
  isActive: boolean
  needsRecovery: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousWorkflowLinkageView {
  workflowNodeId: string
  transitionId: string
  causalTransitionId: string | null
  handoffTransitionId: string
  handoffPackageHash: string
}

export interface AutonomousWorkflowHandoffView {
  handoffTransitionId: string
  causalTransitionId: string | null
  fromNodeId: string
  toNodeId: string
  transitionKind: string
  transitionKindLabel: string
  packageHash: string
  createdAt: string
}

export type AutonomousWorkflowLinkageSource = 'unit' | 'attempt'
export type AutonomousWorkflowContextState = 'ready' | 'awaiting_snapshot' | 'awaiting_handoff'

export interface AutonomousWorkflowContextView {
  linkage: AutonomousWorkflowLinkageView
  linkageSource: AutonomousWorkflowLinkageSource
  linkedNodeLabel: string
  linkedStage: PlanningLifecycleStageView | null
  activeLifecycleStage: PlanningLifecycleStageView | null
  handoff: AutonomousWorkflowHandoffView | null
  pendingApproval: OperatorApprovalView | null
  state: AutonomousWorkflowContextState
  stateLabel: string
  detail: string
}

export interface AutonomousUnitView {
  projectId: string
  runId: string
  unitId: string
  sequence: number
  kind: AutonomousUnitKindDto
  kindLabel: string
  status: AutonomousUnitStatusDto
  statusLabel: string
  summary: string
  boundaryId: string | null
  workflowLinkage: AutonomousWorkflowLinkageView | null
  startedAt: string
  finishedAt: string | null
  updatedAt: string
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  isActive: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousUnitAttemptView {
  projectId: string
  runId: string
  unitId: string
  attemptId: string
  attemptNumber: number
  childSessionId: string
  status: AutonomousUnitStatusDto
  statusLabel: string
  boundaryId: string | null
  workflowLinkage: AutonomousWorkflowLinkageView | null
  startedAt: string
  finishedAt: string | null
  updatedAt: string
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  isActive: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousCommandResultView {
  exitCode: number | null
  timedOut: boolean
  summary: string
}

export interface AutonomousUnitArtifactView {
  projectId: string
  runId: string
  unitId: string
  attemptId: string
  artifactId: string
  artifactKind: string
  artifactKindLabel: string
  status: AutonomousUnitArtifactStatusDto
  statusLabel: string
  summary: string
  contentHash: string | null
  payload: AutonomousArtifactPayloadDto | null
  createdAt: string
  updatedAt: string
  detail: string | null
  commandResult: AutonomousCommandResultView | null
  toolName: string | null
  toolState: AutonomousToolCallStateDto | null
  toolStateLabel: string | null
  evidenceKind: string | null
  verificationOutcome: AutonomousVerificationOutcomeDto | null
  verificationOutcomeLabel: string | null
  diagnosticCode: string | null
  actionId: string | null
  boundaryId: string | null
  isToolResult: boolean
  isVerificationEvidence: boolean
  isPolicyDenied: boolean
}

export interface AutonomousUnitHistoryEntryView {
  unit: AutonomousUnitView
  latestAttempt: AutonomousUnitAttemptView | null
  artifacts: AutonomousUnitArtifactView[]
}

export interface AutonomousRunInspectionView {
  autonomousRun: AutonomousRunView | null
  autonomousUnit: AutonomousUnitView | null
  autonomousAttempt: AutonomousUnitAttemptView | null
  autonomousHistory: AutonomousUnitHistoryEntryView[]
  autonomousRecentArtifacts: AutonomousUnitArtifactView[]
}

export type RuntimeStreamStatus = 'idle' | 'subscribing' | 'replaying' | 'live' | 'complete' | 'stale' | 'error'

export interface RuntimeStreamIssueView {
  code: string
  message: string
  retryable: boolean
  observedAt: string
}

interface RuntimeStreamBaseItemView {
  id: string
  runId: string
  sequence: number
  createdAt: string
}

export interface RuntimeStreamTranscriptItemView extends RuntimeStreamBaseItemView {
  kind: 'transcript'
  text: string
}

export interface RuntimeStreamToolItemView extends RuntimeStreamBaseItemView {
  kind: 'tool'
  toolCallId: string
  toolName: string
  toolState: RuntimeToolCallStateDto
  detail: string | null
  toolSummary?: ToolResultSummaryDto | null
}

export interface RuntimeStreamActivityItemView extends RuntimeStreamBaseItemView {
  kind: 'activity'
  code: string
  title: string
  detail: string | null
}

export interface RuntimeStreamActionRequiredItemView extends RuntimeStreamBaseItemView {
  kind: 'action_required'
  actionId: string
  boundaryId: string | null
  actionType: string
  title: string
  detail: string
}

export interface RuntimeStreamCompleteItemView extends RuntimeStreamBaseItemView {
  kind: 'complete'
  detail: string
}

export interface RuntimeStreamFailureItemView extends RuntimeStreamBaseItemView {
  kind: 'failure'
  code: string
  message: string
  retryable: boolean
}

export type RuntimeStreamViewItem =
  | RuntimeStreamTranscriptItemView
  | RuntimeStreamToolItemView
  | RuntimeStreamActivityItemView
  | RuntimeStreamActionRequiredItemView
  | RuntimeStreamCompleteItemView
  | RuntimeStreamFailureItemView

export interface RuntimeStreamEventDto {
  projectId: string
  runtimeKind: string
  runId: string
  sessionId: string
  flowId: string | null
  subscribedItemKinds: RuntimeStreamItemKindDto[]
  item: RuntimeStreamItemDto
}

export interface RuntimeStreamView {
  projectId: string
  runtimeKind: string
  runId: string | null
  sessionId: string | null
  flowId: string | null
  subscribedItemKinds: RuntimeStreamItemKindDto[]
  status: RuntimeStreamStatus
  items: RuntimeStreamViewItem[]
  transcriptItems: RuntimeStreamTranscriptItemView[]
  toolCalls: RuntimeStreamToolItemView[]
  activityItems: RuntimeStreamActivityItemView[]
  actionRequired: RuntimeStreamActionRequiredItemView[]
  completion: RuntimeStreamCompleteItemView | null
  failure: RuntimeStreamFailureItemView | null
  lastIssue: RuntimeStreamIssueView | null
  lastItemAt: string | null
  lastSequence: number | null
}

export interface ProjectDetailView extends Project {
  branchLabel: string
  runtimeLabel: string
  phaseProgressPercent: number
  lifecycle: PlanningLifecycleView
  repository: RepositoryView | null
  repositoryStatus: RepositoryStatusView | null
  approvalRequests: OperatorApprovalView[]
  pendingApprovalCount: number
  latestDecisionOutcome: OperatorDecisionOutcomeView | null
  verificationRecords: VerificationRecordView[]
  resumeHistory: ResumeHistoryEntryView[]
  handoffPackages: WorkflowHandoffPackageView[]
  notificationBroker: NotificationBrokerView
  runtimeSession?: RuntimeSessionView | null
  runtimeRun?: RuntimeRunView | null
  autonomousRun?: AutonomousRunView | null
  autonomousUnit?: AutonomousUnitView | null
  autonomousAttempt?: AutonomousUnitAttemptView | null
  autonomousHistory: AutonomousUnitHistoryEntryView[]
  autonomousRecentArtifacts: AutonomousUnitArtifactView[]
}

export function safePercent(completed: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) {
    return 0
  }

  const ratio = completed / total
  if (!Number.isFinite(ratio) || ratio <= 0) {
    return 0
  }

  return Math.max(0, Math.min(100, Math.round(ratio * 100)))
}

function normalizeText(value: string | null | undefined, fallback: string): string {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : fallback
}

function normalizeOptionalText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

const PLANNING_LIFECYCLE_STAGE_LABELS: Record<PlanningLifecycleStageKindDto, string> = {
  discussion: 'Discussion',
  research: 'Research',
  requirements: 'Requirements',
  roadmap: 'Roadmap',
}

function getPhaseStatusLabel(status: PhaseStatus): string {
  switch (status) {
    case 'complete':
      return 'Complete'
    case 'active':
      return 'Active'
    case 'pending':
      return 'Pending'
    case 'blocked':
      return 'Blocked'
  }
}

function humanizeNodeId(nodeId: string): string {
  return nodeId
    .split(/[_\-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function createEmptyPlanningLifecycleByStage(): Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView | null> {
  return {
    discussion: null,
    research: null,
    requirements: null,
    roadmap: null,
  }
}

export function createEmptyPlanningLifecycle(): PlanningLifecycleView {
  return {
    stages: [],
    byStage: createEmptyPlanningLifecycleByStage(),
    hasStages: false,
    activeStage: null,
    actionRequiredCount: 0,
    blockedCount: 0,
    completedCount: 0,
    percentComplete: 0,
  }
}

export function mapPlanningLifecycle(projection: PlanningLifecycleProjectionDto): PlanningLifecycleView {
  const byStage = createEmptyPlanningLifecycleByStage()
  const stageByKind = new Map(projection.stages.map((stage) => [stage.stage, stage]))
  const stages: PlanningLifecycleStageView[] = []

  PLANNING_LIFECYCLE_STAGES.forEach((stageKind) => {
    const stage = stageByKind.get(stageKind)
    if (!stage) {
      return
    }

    const mappedStage: PlanningLifecycleStageView = {
      stage: stage.stage,
      stageLabel: PLANNING_LIFECYCLE_STAGE_LABELS[stage.stage],
      nodeId: stage.nodeId,
      nodeLabel: humanizeNodeId(stage.nodeId),
      status: stage.status,
      statusLabel: getPhaseStatusLabel(stage.status),
      actionRequired: stage.actionRequired,
      lastTransitionAt: normalizeOptionalText(stage.lastTransitionAt),
    }

    byStage[stage.stage] = mappedStage
    stages.push(mappedStage)
  })

  const completedCount = stages.filter((stage) => stage.status === 'complete').length
  const blockedCount = stages.filter((stage) => stage.status === 'blocked').length
  const actionRequiredCount = stages.filter((stage) => stage.actionRequired).length

  return {
    stages,
    byStage,
    hasStages: stages.length > 0,
    activeStage: stages.find((stage) => stage.status === 'active') ?? null,
    actionRequiredCount,
    blockedCount,
    completedCount,
    percentComplete: safePercent(completedCount, stages.length),
  }
}

function getAutonomousWorkflowContextStateLabel(state: AutonomousWorkflowContextState): string {
  switch (state) {
    case 'ready':
      return 'In sync'
    case 'awaiting_snapshot':
      return 'Snapshot lag'
    case 'awaiting_handoff':
      return 'Handoff pending'
  }
}

function mapAutonomousWorkflowHandoff(pkg: WorkflowHandoffPackageView): AutonomousWorkflowHandoffView {
  return {
    handoffTransitionId: pkg.handoffTransitionId,
    causalTransitionId: pkg.causalTransitionId,
    fromNodeId: pkg.fromNodeId,
    toNodeId: pkg.toNodeId,
    transitionKind: pkg.transitionKind,
    transitionKindLabel: humanizeRuntimeKind(pkg.transitionKind),
    packageHash: pkg.packageHash,
    createdAt: pkg.createdAt,
  }
}

export function deriveAutonomousWorkflowContext(options: {
  lifecycle: PlanningLifecycleView
  handoffPackages: WorkflowHandoffPackageView[]
  approvalRequests: OperatorApprovalView[]
  autonomousUnit: AutonomousUnitView | null
  autonomousAttempt?: AutonomousUnitAttemptView | null
}): AutonomousWorkflowContextView | null {
  const attemptLinkage = options.autonomousAttempt?.workflowLinkage ?? null
  const unitLinkage = options.autonomousUnit?.workflowLinkage ?? null
  const linkage = attemptLinkage ?? unitLinkage
  if (!linkage) {
    return null
  }

  const linkageSource: AutonomousWorkflowLinkageSource = attemptLinkage ? 'attempt' : 'unit'
  const linkedStage = options.lifecycle.stages.find((stage) => stage.nodeId === linkage.workflowNodeId) ?? null
  const activeLifecycleStage = options.lifecycle.activeStage
  const linkedNodeLabel = linkedStage?.nodeLabel ?? humanizeNodeId(linkage.workflowNodeId)
  const matchingHandoffPackage = sortByNewest(
    options.handoffPackages.filter((pkg) => pkg.handoffTransitionId === linkage.handoffTransitionId),
    (pkg) => pkg.createdAt,
  )[0] ?? null
  const handoff = matchingHandoffPackage ? mapAutonomousWorkflowHandoff(matchingHandoffPackage) : null
  const pendingApproval =
    options.approvalRequests.find(
      (approval) => approval.isPending && approval.gateNodeId === linkage.workflowNodeId,
    ) ?? null

  const activeStageMismatch = Boolean(activeLifecycleStage && activeLifecycleStage.nodeId !== linkage.workflowNodeId)
  const handoffHashMismatch = Boolean(handoff && handoff.packageHash !== linkage.handoffPackageHash)

  let state: AutonomousWorkflowContextState
  let detail: string

  if (!linkedStage) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence has persisted autonomous workflow linkage for this boundary, but the selected project snapshot has not exposed the linked lifecycle node yet.'
  } else if (activeStageMismatch) {
    state = 'awaiting_snapshot'
    detail = `Cadence is keeping lifecycle progression anchored to snapshot truth while the linked node \`${linkedStage.stageLabel}\` waits for the active lifecycle stage to catch up.`
  } else if (handoffHashMismatch) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence found the linked handoff transition in the selected project snapshot, but the persisted handoff hash has not caught up to the autonomous linkage yet.'
  } else if (!handoff) {
    state = 'awaiting_handoff'
    detail =
      'Cadence has persisted autonomous workflow linkage for this boundary, but the linked handoff package is not visible in the selected project snapshot yet.'
  } else {
    state = 'ready'
    detail =
      'Lifecycle stage, autonomous linkage, and handoff package all agree on backend truth for this boundary.'
  }

  if (pendingApproval) {
    detail = `${detail} Pending approval \`${pendingApproval.title}\` is still blocking continuation at this linked node.`
  }

  return {
    linkage,
    linkageSource,
    linkedNodeLabel,
    linkedStage,
    activeLifecycleStage,
    handoff,
    pendingApproval,
    state,
    stateLabel: getAutonomousWorkflowContextStateLabel(state),
    detail,
  }
}

function createStepStatuses(
  status: PhaseStatus,
  currentStep: PhaseStep | null,
): Record<PhaseStep, 'complete' | 'active' | 'pending' | 'skipped'> {
  if (status === 'complete') {
    return {
      discuss: 'complete',
      plan: 'complete',
      execute: 'complete',
      verify: 'complete',
      ship: 'complete',
    }
  }

  if (!currentStep) {
    return {
      discuss: 'pending',
      plan: 'pending',
      execute: 'pending',
      verify: 'pending',
      ship: 'pending',
    }
  }

  const activeIndex = STEP_INDEX.get(currentStep) ?? 0

  return PHASE_STEPS.reduce<Record<PhaseStep, 'complete' | 'active' | 'pending' | 'skipped'>>(
    (acc, step, index) => {
      if (index < activeIndex) {
        acc[step] = 'complete'
      } else if (index === activeIndex) {
        acc[step] = 'active'
      } else {
        acc[step] = 'pending'
      }

      return acc
    },
    {
      discuss: 'pending',
      plan: 'pending',
      execute: 'pending',
      verify: 'pending',
      ship: 'pending',
    },
  )
}

function humanizeRuntimeKind(runtimeKind: string): string {
  return runtimeKind
    .split(/[_-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function getRuntimePhaseLabel(phase: RuntimeAuthPhaseDto): string {
  switch (phase) {
    case 'idle':
      return 'Signed out'
    case 'starting':
      return 'Starting login'
    case 'awaiting_browser_callback':
      return 'Awaiting browser'
    case 'awaiting_manual_input':
      return 'Awaiting manual input'
    case 'exchanging_code':
      return 'Signing in'
    case 'authenticated':
      return 'Authenticated'
    case 'refreshing':
      return 'Refreshing session'
    case 'cancelled':
      return 'Login cancelled'
    case 'failed':
      return 'Login failed'
  }
}

function getRuntimeLabel(runtimeKind: string, phase: RuntimeAuthPhaseDto): string {
  if (phase === 'idle' || phase === 'failed' || phase === 'cancelled') {
    return 'Runtime unavailable'
  }

  return `${humanizeRuntimeKind(runtimeKind)} · ${getRuntimePhaseLabel(phase)}`
}

export function getRuntimeRunStatusLabel(status: RuntimeRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Supervisor starting'
    case 'running':
      return 'Supervisor running'
    case 'stale':
      return 'Supervisor stale'
    case 'stopped':
      return 'Run stopped'
    case 'failed':
      return 'Run failed'
  }
}

export function getRuntimeRunTransportLivenessLabel(liveness: RuntimeRunTransportLivenessDto): string {
  switch (liveness) {
    case 'unknown':
      return 'Probe unknown'
    case 'reachable':
      return 'Control reachable'
    case 'unreachable':
      return 'Control unreachable'
  }
}

export function getRuntimeRunCheckpointKindLabel(kind: RuntimeRunCheckpointKindDto): string {
  switch (kind) {
    case 'bootstrap':
      return 'Bootstrap'
    case 'state':
      return 'State'
    case 'tool':
      return 'Tool'
    case 'action_required':
      return 'Action required'
    case 'diagnostic':
      return 'Diagnostic'
  }
}

function getRuntimeRunLabel(runtimeKind: string, status: RuntimeRunStatusDto): string {
  return `${humanizeRuntimeKind(runtimeKind)} · ${getRuntimeRunStatusLabel(status)}`
}

export function getAutonomousRunStatusLabel(status: AutonomousRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Autonomous run starting'
    case 'running':
      return 'Autonomous run active'
    case 'paused':
      return 'Autonomous run paused'
    case 'cancelling':
      return 'Autonomous run cancelling'
    case 'cancelled':
      return 'Autonomous run cancelled'
    case 'stale':
      return 'Autonomous run stale'
    case 'failed':
      return 'Autonomous run failed'
    case 'stopped':
      return 'Autonomous run stopped'
    case 'crashed':
      return 'Autonomous run crashed'
    case 'completed':
      return 'Autonomous run completed'
  }
}

export function getAutonomousRunRecoveryLabel(recoveryState: AutonomousRunRecoveryStateDto): string {
  switch (recoveryState) {
    case 'healthy':
      return 'Recovery healthy'
    case 'recovery_required':
      return 'Recovery required'
    case 'terminal':
      return 'Terminal state'
    case 'failed':
      return 'Recovery failed'
  }
}

export function getAutonomousUnitKindLabel(kind: AutonomousUnitKindDto): string {
  switch (kind) {
    case 'bootstrap':
      return 'Bootstrap'
    case 'state':
      return 'State'
    case 'tool':
      return 'Tool'
    case 'action_required':
      return 'Action required'
    case 'diagnostic':
      return 'Diagnostic'
  }
}

export function getAutonomousUnitStatusLabel(status: AutonomousUnitStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'active':
      return 'Active'
    case 'paused':
      return 'Paused'
    case 'completed':
      return 'Completed'
    case 'cancelled':
      return 'Cancelled'
    case 'failed':
      return 'Failed'
  }
}

function getAutonomousRunLabel(runtimeKind: string, status: AutonomousRunStatusDto): string {
  return `${humanizeRuntimeKind(runtimeKind)} · ${getAutonomousRunStatusLabel(status)}`
}

function capRecent<T>(values: T[], limit: number): T[] {
  return values.length <= limit ? values : values.slice(values.length - limit)
}

function uniqueRuntimeStreamKinds(kinds: RuntimeStreamItemKindDto[]): RuntimeStreamItemKindDto[] {
  return Array.from(new Set(kinds))
}

function ensureRuntimeStreamText(value: string | null | undefined, field: string, kind: string): string {
  const normalized = normalizeOptionalText(value)
  if (!normalized) {
    throw new Error(`Cadence received a ${kind} item without a non-empty ${field}.`)
  }

  return normalized
}

function runtimeStreamItemId(kind: RuntimeStreamItemKindDto, runId: string, sequence: number): string {
  return `${kind}:${runId}:${sequence}`
}

function runtimeStreamActionRequiredItemId(runId: string, actionId: string): string {
  return `action_required:${runId}:${actionId}`
}

function normalizeRuntimeStreamItem(event: RuntimeStreamEventDto): RuntimeStreamViewItem {
  const projectId = normalizeOptionalText(event.projectId)
  if (!projectId) {
    throw new Error('Cadence received a runtime stream item without a selected project id.')
  }

  const expectedRunId = normalizeOptionalText(event.runId)
  const expectedSessionId = normalizeOptionalText(event.sessionId)
  const eventFlowId = normalizeOptionalText(event.flowId)
  const itemRunId = normalizeOptionalText(event.item.runId)
  const itemSessionId = normalizeOptionalText(event.item.sessionId)
  const itemFlowId = normalizeOptionalText(event.item.flowId)

  if (!expectedRunId || !itemRunId || itemRunId !== expectedRunId) {
    throw new Error('Cadence received a runtime stream item for an unexpected run id at the desktop adapter boundary.')
  }

  if (expectedSessionId && itemSessionId && itemSessionId !== expectedSessionId) {
    throw new Error(
      `Cadence received a runtime stream item for an unexpected session (${itemSessionId}) while ${expectedSessionId} is active.`,
    )
  }

  if (eventFlowId && itemFlowId && itemFlowId !== eventFlowId) {
    throw new Error(`Cadence received a runtime stream item for an unexpected auth flow (${itemFlowId}).`)
  }

  switch (event.item.kind) {
    case 'transcript': {
      const text = ensureRuntimeStreamText(event.item.text, 'text', 'transcript')
      return {
        id: runtimeStreamItemId('transcript', itemRunId, event.item.sequence),
        kind: 'transcript',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        text,
      }
    }
    case 'tool': {
      const toolCallId = ensureRuntimeStreamText(event.item.toolCallId, 'toolCallId', 'tool')
      const toolName = ensureRuntimeStreamText(event.item.toolName, 'toolName', 'tool')
      const toolState = event.item.toolState
      if (!toolState) {
        throw new Error('Cadence received a runtime tool item without a toolState value.')
      }

      return {
        id: runtimeStreamItemId('tool', itemRunId, event.item.sequence),
        kind: 'tool',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        toolCallId,
        toolName,
        toolState,
        detail: normalizeOptionalText(event.item.detail),
        ...(event.item.toolSummary ? { toolSummary: event.item.toolSummary } : {}),
      }
    }
    case 'activity': {
      const code = ensureRuntimeStreamText(event.item.code, 'code', 'activity')
      const title = ensureRuntimeStreamText(event.item.title, 'title', 'activity')
      return {
        id: runtimeStreamItemId('activity', itemRunId, event.item.sequence),
        kind: 'activity',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        code,
        title,
        detail: normalizeOptionalText(event.item.detail),
      }
    }
    case 'action_required': {
      const actionId = ensureRuntimeStreamText(event.item.actionId, 'actionId', 'action-required')
      const actionType = ensureRuntimeStreamText(event.item.actionType, 'actionType', 'action-required')
      const title = ensureRuntimeStreamText(event.item.title, 'title', 'action-required')
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'action-required')
      return {
        id: runtimeStreamActionRequiredItemId(itemRunId, actionId),
        kind: 'action_required',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        actionId,
        boundaryId: normalizeOptionalText(event.item.boundaryId),
        actionType,
        title,
        detail,
      }
    }
    case 'complete': {
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'complete')
      return {
        id: runtimeStreamItemId('complete', itemRunId, event.item.sequence),
        kind: 'complete',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        detail,
      }
    }
    case 'failure': {
      const code = ensureRuntimeStreamText(event.item.code, 'code', 'failure')
      const message = ensureRuntimeStreamText(event.item.message, 'message', 'failure')
      if (typeof event.item.retryable !== 'boolean') {
        throw new Error('Cadence received a runtime failure item without a retryable flag.')
      }

      return {
        id: runtimeStreamItemId('failure', itemRunId, event.item.sequence),
        kind: 'failure',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        code,
        message,
        retryable: event.item.retryable,
      }
    }
  }
}

export function createRuntimeStreamView(options: {
  projectId: string
  runtimeKind: string
  runId?: string | null
  sessionId?: string | null
  flowId?: string | null
  subscribedItemKinds?: RuntimeStreamItemKindDto[]
  status?: RuntimeStreamStatus
}): RuntimeStreamView {
  return {
    projectId: options.projectId,
    runtimeKind: normalizeText(options.runtimeKind, 'openai_codex'),
    runId: normalizeOptionalText(options.runId),
    sessionId: normalizeOptionalText(options.sessionId),
    flowId: normalizeOptionalText(options.flowId),
    subscribedItemKinds: uniqueRuntimeStreamKinds(options.subscribedItemKinds ?? []),
    status: options.status ?? 'idle',
    items: [],
    transcriptItems: [],
    toolCalls: [],
    activityItems: [],
    actionRequired: [],
    completion: null,
    failure: null,
    lastIssue: null,
    lastItemAt: null,
    lastSequence: null,
  }
}

export function createRuntimeStreamFromSubscription(
  response: SubscribeRuntimeStreamResponseDto,
  status: RuntimeStreamStatus = 'subscribing',
): RuntimeStreamView {
  return createRuntimeStreamView({
    projectId: response.projectId,
    runtimeKind: response.runtimeKind,
    runId: response.runId,
    sessionId: response.sessionId,
    flowId: response.flowId ?? null,
    subscribedItemKinds: response.subscribedItemKinds,
    status,
  })
}

export function mergeRuntimeStreamEvent(
  current: RuntimeStreamView | null,
  event: RuntimeStreamEventDto,
): RuntimeStreamView {
  if (current && current.projectId !== event.projectId) {
    throw new Error(
      `Cadence received a runtime stream item for ${event.projectId} while ${current.projectId} is the selected project.`,
    )
  }

  if (current?.runId && current.runId !== event.runId) {
    return current
  }

  const base =
    current ??
    createRuntimeStreamView({
      projectId: event.projectId,
      runtimeKind: event.runtimeKind,
      runId: event.runId,
      sessionId: event.sessionId,
      flowId: event.flowId,
      subscribedItemKinds: event.subscribedItemKinds,
      status: 'subscribing',
    })

  if (base.lastSequence !== null) {
    if (event.item.sequence < base.lastSequence) {
      throw new Error(
        `Cadence rejected non-monotonic runtime stream sequence ${event.item.sequence} for run ${event.runId}; last sequence was ${base.lastSequence}.`,
      )
    }

    if (event.item.sequence === base.lastSequence) {
      return base
    }
  }

  const nextItem = normalizeRuntimeStreamItem(event)
  const nextItems =
    nextItem.kind === 'action_required'
      ? capRecent(
          [
            ...base.items.filter(
              (item) => !(item.kind === 'action_required' && item.runId === nextItem.runId && item.actionId === nextItem.actionId),
            ),
            nextItem,
          ],
          MAX_RUNTIME_STREAM_ITEMS,
        )
      : capRecent([...base.items, nextItem], MAX_RUNTIME_STREAM_ITEMS)
  const nextToolCalls =
    nextItem.kind === 'tool'
      ? capRecent(
          [
            ...base.toolCalls.filter((toolCall) => toolCall.toolCallId !== nextItem.toolCallId),
            nextItem,
          ],
          MAX_RUNTIME_STREAM_TOOL_CALLS,
        )
      : base.toolCalls
  const nextTranscriptItems =
    nextItem.kind === 'transcript'
      ? capRecent([...base.transcriptItems, nextItem], MAX_RUNTIME_STREAM_TRANSCRIPTS)
      : base.transcriptItems
  const nextActivityItems =
    nextItem.kind === 'activity'
      ? capRecent([...base.activityItems, nextItem], MAX_RUNTIME_STREAM_ACTIVITY)
      : base.activityItems
  const nextActionRequired =
    nextItem.kind === 'action_required'
      ? capRecent(
          [
            ...base.actionRequired.filter((actionRequiredItem) => actionRequiredItem.actionId !== nextItem.actionId),
            nextItem,
          ],
          MAX_RUNTIME_STREAM_ACTION_REQUIRED,
        )
      : base.actionRequired

  return {
    ...base,
    runtimeKind: normalizeText(event.runtimeKind, base.runtimeKind),
    runId: normalizeOptionalText(event.runId) ?? base.runId,
    sessionId: normalizeOptionalText(event.sessionId) ?? base.sessionId,
    flowId: normalizeOptionalText(event.flowId) ?? base.flowId,
    subscribedItemKinds: uniqueRuntimeStreamKinds(event.subscribedItemKinds),
    status:
      nextItem.kind === 'complete'
        ? 'complete'
        : nextItem.kind === 'failure'
          ? nextItem.retryable
            ? 'stale'
            : 'error'
          : 'live',
    items: nextItems,
    transcriptItems: nextTranscriptItems,
    toolCalls: nextToolCalls,
    activityItems: nextActivityItems,
    actionRequired: nextActionRequired,
    completion: nextItem.kind === 'complete' ? nextItem : base.completion,
    failure: nextItem.kind === 'failure' ? nextItem : null,
    lastIssue:
      nextItem.kind === 'failure'
        ? {
            code: nextItem.code,
            message: nextItem.message,
            retryable: nextItem.retryable,
            observedAt: nextItem.createdAt,
          }
        : null,
    lastItemAt: nextItem.createdAt,
    lastSequence: nextItem.sequence,
  }
}

export function applyRuntimeStreamIssue(
  current: RuntimeStreamView | null,
  options: {
    projectId: string
    runtimeKind: string
    runId?: string | null
    sessionId?: string | null
    flowId?: string | null
    subscribedItemKinds?: RuntimeStreamItemKindDto[]
    code: string
    message: string
    retryable: boolean
    observedAt?: string
  },
): RuntimeStreamView {
  const observedAt = options.observedAt ?? new Date().toISOString()
  const base =
    current ??
    createRuntimeStreamView({
      projectId: options.projectId,
      runtimeKind: options.runtimeKind,
      runId: options.runId,
      sessionId: options.sessionId,
      flowId: options.flowId,
      subscribedItemKinds: options.subscribedItemKinds,
      status: options.retryable ? 'stale' : 'error',
    })

  return {
    ...base,
    runtimeKind: normalizeText(options.runtimeKind, base.runtimeKind),
    runId: normalizeOptionalText(options.runId) ?? base.runId,
    sessionId: normalizeOptionalText(options.sessionId) ?? base.sessionId,
    flowId: normalizeOptionalText(options.flowId) ?? base.flowId,
    subscribedItemKinds: uniqueRuntimeStreamKinds(options.subscribedItemKinds ?? base.subscribedItemKinds),
    status: options.retryable ? 'stale' : 'error',
    lastIssue: {
      code: normalizeText(options.code, 'runtime_stream_issue'),
      message: normalizeText(options.message, 'Cadence could not project runtime activity for this project.'),
      retryable: options.retryable,
      observedAt,
    },
    lastItemAt: base.lastItemAt ?? observedAt,
    lastSequence: base.lastSequence,
  }
}

export function getRuntimeStreamStatusLabel(status: RuntimeStreamStatus): string {
  switch (status) {
    case 'idle':
      return 'No live stream'
    case 'subscribing':
      return 'Connecting stream'
    case 'replaying':
      return 'Replaying recent activity'
    case 'live':
      return 'Streaming live activity'
    case 'complete':
      return 'Stream complete'
    case 'stale':
      return 'Stream stale'
    case 'error':
      return 'Stream failed'
  }
}

function getOperatorApprovalStatusLabel(status: OperatorApprovalStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending approval'
    case 'approved':
      return 'Approved'
    case 'rejected':
      return 'Rejected'
  }
}

function getVerificationRecordStatusLabel(status: VerificationRecordStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending verification'
    case 'passed':
      return 'Passed'
    case 'failed':
      return 'Failed'
  }
}

function getResumeHistoryStatusLabel(status: ResumeHistoryStatusDto): string {
  switch (status) {
    case 'started':
      return 'Resume started'
    case 'failed':
      return 'Resume failed'
  }
}

export function mapOperatorApproval(approval: OperatorApprovalDto): OperatorApprovalView {
  const decisionNote = normalizeOptionalText(approval.decisionNote)
  const resolvedAt = normalizeOptionalText(approval.resolvedAt)
  const sessionId = normalizeOptionalText(approval.sessionId)
  const flowId = normalizeOptionalText(approval.flowId)
  const actionType = normalizeText(approval.actionType, 'manual_review')
  const gateNodeId = normalizeOptionalText(approval.gateNodeId)
  const gateKey = normalizeOptionalText(approval.gateKey)
  const transitionFromNodeId = normalizeOptionalText(approval.transitionFromNodeId)
  const transitionToNodeId = normalizeOptionalText(approval.transitionToNodeId)
  const transitionKind = normalizeOptionalText(approval.transitionKind)
  const userAnswer = normalizeOptionalText(approval.userAnswer)
  const answerPolicy = deriveOperatorApprovalAnswerPolicy({
    actionId: approval.actionId,
    sessionId,
    flowId,
    actionType,
    gateNodeId,
    gateKey,
  })

  return {
    actionId: approval.actionId,
    sessionId,
    flowId,
    actionType,
    title: normalizeText(approval.title, 'Action required'),
    detail: normalizeText(approval.detail, 'Review the operator action before continuing.'),
    gateNodeId,
    gateKey,
    transitionFromNodeId,
    transitionToNodeId,
    transitionKind,
    userAnswer,
    status: approval.status,
    statusLabel: getOperatorApprovalStatusLabel(approval.status),
    decisionNote,
    createdAt: approval.createdAt,
    updatedAt: approval.updatedAt,
    resolvedAt,
    isPending: approval.status === 'pending',
    isResolved: approval.status !== 'pending',
    canResume: approval.status === 'approved',
    isGateLinked: answerPolicy.isGateLinked,
    isRuntimeResumable: answerPolicy.isRuntimeResumable,
    requiresUserAnswer: answerPolicy.requiresAnswer,
    answerRequirementReason: answerPolicy.requirementReason,
    answerRequirementLabel: answerPolicy.requirementLabel,
    answerShapeKind: answerPolicy.answerShape.kind,
    answerShapeLabel: answerPolicy.answerShape.label,
    answerShapeHint: answerPolicy.answerShape.guidance,
    answerPlaceholder: answerPolicy.answerShape.placeholder,
  }
}

export function mapVerificationRecord(record: VerificationRecordDto): VerificationRecordView {
  return {
    id: record.id,
    sourceActionId: normalizeOptionalText(record.sourceActionId),
    status: record.status,
    statusLabel: getVerificationRecordStatusLabel(record.status),
    summary: normalizeText(record.summary, 'Verification record available.'),
    detail: normalizeOptionalText(record.detail),
    recordedAt: record.recordedAt,
  }
}

export function mapResumeHistoryEntry(entry: ResumeHistoryEntryDto): ResumeHistoryEntryView {
  return {
    id: entry.id,
    sourceActionId: normalizeOptionalText(entry.sourceActionId),
    sessionId: normalizeOptionalText(entry.sessionId),
    status: entry.status,
    statusLabel: getResumeHistoryStatusLabel(entry.status),
    summary: normalizeText(entry.summary, 'Resume history recorded.'),
    createdAt: entry.createdAt,
  }
}

export function mapWorkflowHandoffPackage(pkg: WorkflowHandoffPackageDto): WorkflowHandoffPackageView {
  return {
    id: pkg.id,
    projectId: pkg.projectId,
    handoffTransitionId: pkg.handoffTransitionId,
    causalTransitionId: normalizeOptionalText(pkg.causalTransitionId),
    fromNodeId: pkg.fromNodeId,
    toNodeId: pkg.toNodeId,
    transitionKind: pkg.transitionKind,
    packagePayload: pkg.packagePayload,
    packageHash: pkg.packageHash,
    createdAt: pkg.createdAt,
  }
}

export function getLatestDecisionOutcome(
  approvals: OperatorApprovalView[],
): OperatorDecisionOutcomeView | null {
  const latestResolvedApproval = approvals.find((approval) => approval.status !== 'pending' && approval.resolvedAt)
  if (!latestResolvedApproval || !latestResolvedApproval.resolvedAt || latestResolvedApproval.status === 'pending') {
    return null
  }

  return {
    actionId: latestResolvedApproval.actionId,
    title: latestResolvedApproval.title,
    status: latestResolvedApproval.status,
    statusLabel: latestResolvedApproval.statusLabel,
    gateNodeId: latestResolvedApproval.gateNodeId,
    gateKey: latestResolvedApproval.gateKey,
    userAnswer: latestResolvedApproval.userAnswer,
    decisionNote: latestResolvedApproval.decisionNote,
    resolvedAt: latestResolvedApproval.resolvedAt,
  }
}

const MAX_NOTIFICATION_BROKER_DISPATCHES = 250

function getNotificationDispatchStatusLabel(status: NotificationDispatchStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending dispatch'
    case 'sent':
      return 'Delivered'
    case 'failed':
      return 'Delivery failed'
    case 'claimed':
      return 'Reply claimed'
  }
}

function timestampToSortValue(value: string | null): number {
  if (!value) {
    return Number.NEGATIVE_INFINITY
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : Number.NEGATIVE_INFINITY
}

function sortDispatchesByRecency(
  left: { updatedAt: string; createdAt: string },
  right: { updatedAt: string; createdAt: string },
): number {
  const rightUpdatedAt = timestampToSortValue(right.updatedAt)
  const leftUpdatedAt = timestampToSortValue(left.updatedAt)
  if (rightUpdatedAt !== leftUpdatedAt) {
    return rightUpdatedAt - leftUpdatedAt
  }

  return timestampToSortValue(right.createdAt) - timestampToSortValue(left.createdAt)
}

export function mapNotificationDispatch(dispatch: NotificationDispatchDto): NotificationDispatchView {
  const lastErrorCode = normalizeOptionalText(dispatch.lastErrorCode)
  const lastErrorMessage = normalizeOptionalText(dispatch.lastErrorMessage)

  return {
    id: dispatch.id,
    projectId: dispatch.projectId,
    actionId: dispatch.actionId,
    routeId: dispatch.routeId,
    correlationKey: dispatch.correlationKey,
    status: dispatch.status,
    statusLabel: getNotificationDispatchStatusLabel(dispatch.status),
    attemptCount: dispatch.attemptCount,
    lastAttemptAt: normalizeOptionalText(dispatch.lastAttemptAt),
    deliveredAt: normalizeOptionalText(dispatch.deliveredAt),
    claimedAt: normalizeOptionalText(dispatch.claimedAt),
    lastErrorCode,
    lastErrorMessage,
    createdAt: dispatch.createdAt,
    updatedAt: dispatch.updatedAt,
    isPending: dispatch.status === 'pending',
    isSent: dispatch.status === 'sent',
    isFailed: dispatch.status === 'failed',
    isClaimed: dispatch.status === 'claimed',
    hasFailureDiagnostics: Boolean(lastErrorCode && lastErrorMessage),
  }
}

function createEmptyNotificationBrokerView(): NotificationBrokerView {
  return {
    dispatches: [],
    actions: [],
    routes: [],
    byActionId: {},
    byRouteId: {},
    dispatchCount: 0,
    routeCount: 0,
    pendingCount: 0,
    sentCount: 0,
    failedCount: 0,
    claimedCount: 0,
    latestUpdatedAt: null,
    isTruncated: false,
    totalBeforeTruncation: 0,
  }
}

function countNotificationDispatchStatuses(dispatches: NotificationDispatchView[]) {
  return dispatches.reduce(
    (acc, dispatch) => {
      if (dispatch.isPending) {
        acc.pendingCount += 1
      } else if (dispatch.isSent) {
        acc.sentCount += 1
      } else if (dispatch.isFailed) {
        acc.failedCount += 1
      } else if (dispatch.isClaimed) {
        acc.claimedCount += 1
      }

      return acc
    },
    {
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
    },
  )
}

export function mapNotificationBrokerRouteDiagnostics(dispatches: NotificationDispatchView[]): {
  routes: NotificationBrokerRouteView[]
  byRouteId: Record<string, NotificationBrokerRouteView>
} {
  const groupedByRoute = new Map<string, NotificationDispatchView[]>()

  dispatches.forEach((dispatch) => {
    const routeId = dispatch.routeId.trim()
    const existing = groupedByRoute.get(routeId)
    if (existing) {
      existing.push(dispatch)
      return
    }

    groupedByRoute.set(routeId, [dispatch])
  })

  const routes = Array.from(groupedByRoute.entries())
    .map(([routeId, routeDispatches]): NotificationBrokerRouteView => {
      const sortedRouteDispatches = [...routeDispatches].sort(sortDispatchesByRecency)
      const statusCounts = countNotificationDispatchStatuses(sortedRouteDispatches)
      const latestFailedDispatch =
        sortedRouteDispatches.find((dispatch) => dispatch.isFailed && dispatch.hasFailureDiagnostics) ?? null

      return {
        routeId,
        dispatches: sortedRouteDispatches,
        dispatchCount: sortedRouteDispatches.length,
        pendingCount: statusCounts.pendingCount,
        sentCount: statusCounts.sentCount,
        failedCount: statusCounts.failedCount,
        claimedCount: statusCounts.claimedCount,
        latestUpdatedAt: sortedRouteDispatches[0]?.updatedAt ?? null,
        latestFailureAt:
          latestFailedDispatch?.updatedAt ?? latestFailedDispatch?.lastAttemptAt ?? latestFailedDispatch?.createdAt ?? null,
        latestFailureCode: latestFailedDispatch?.lastErrorCode ?? null,
        latestFailureMessage: latestFailedDispatch?.lastErrorMessage ?? null,
        hasFailures: statusCounts.failedCount > 0,
        hasPending: statusCounts.pendingCount > 0,
      }
    })
    .sort((left, right) => {
      const byRecency = sortDispatchesByRecency(
        {
          updatedAt: left.latestUpdatedAt ?? '',
          createdAt: left.dispatches[0]?.createdAt ?? '',
        },
        {
          updatedAt: right.latestUpdatedAt ?? '',
          createdAt: right.dispatches[0]?.createdAt ?? '',
        },
      )

      if (byRecency !== 0) {
        return byRecency
      }

      return left.routeId.localeCompare(right.routeId)
    })

  const byRouteId = routes.reduce<Record<string, NotificationBrokerRouteView>>((acc, route) => {
    acc[route.routeId] = route
    return acc
  }, {})

  return {
    routes,
    byRouteId,
  }
}

export function mapNotificationBroker(
  projectId: string,
  dispatches: NotificationDispatchDto[],
): NotificationBrokerView {
  const inScopeDispatches = dispatches
    .filter((dispatch) => dispatch.projectId === projectId)
    .sort(sortDispatchesByRecency)
  if (inScopeDispatches.length === 0) {
    return createEmptyNotificationBrokerView()
  }

  const totalBeforeTruncation = inScopeDispatches.length
  const boundedDispatches = inScopeDispatches.slice(0, MAX_NOTIFICATION_BROKER_DISPATCHES)
  const dispatchViews = boundedDispatches.map(mapNotificationDispatch)

  const statusCounts = countNotificationDispatchStatuses(dispatchViews)

  const groupedByAction = new Map<string, NotificationDispatchView[]>()
  dispatchViews.forEach((dispatch) => {
    const existing = groupedByAction.get(dispatch.actionId)
    if (existing) {
      existing.push(dispatch)
      return
    }

    groupedByAction.set(dispatch.actionId, [dispatch])
  })

  const actions = Array.from(groupedByAction.entries())
    .map(([actionId, actionDispatches]): NotificationBrokerActionView => {
      const sortedActionDispatches = [...actionDispatches].sort(sortDispatchesByRecency)
      const actionStatusCounts = countNotificationDispatchStatuses(sortedActionDispatches)

      return {
        actionId,
        dispatches: sortedActionDispatches,
        dispatchCount: sortedActionDispatches.length,
        pendingCount: actionStatusCounts.pendingCount,
        sentCount: actionStatusCounts.sentCount,
        failedCount: actionStatusCounts.failedCount,
        claimedCount: actionStatusCounts.claimedCount,
        latestUpdatedAt: sortedActionDispatches[0]?.updatedAt ?? null,
        hasFailures: actionStatusCounts.failedCount > 0,
        hasPending: actionStatusCounts.pendingCount > 0,
        hasClaimed: actionStatusCounts.claimedCount > 0,
      }
    })
    .sort((left, right) =>
      sortDispatchesByRecency(
        {
          updatedAt: left.latestUpdatedAt ?? '',
          createdAt: left.dispatches[0]?.createdAt ?? '',
        },
        {
          updatedAt: right.latestUpdatedAt ?? '',
          createdAt: right.dispatches[0]?.createdAt ?? '',
        },
      ),
    )

  const byActionId = actions.reduce<Record<string, NotificationBrokerActionView>>((acc, action) => {
    acc[action.actionId] = action
    return acc
  }, {})

  const routeDiagnostics = mapNotificationBrokerRouteDiagnostics(dispatchViews)

  return {
    dispatches: dispatchViews,
    actions,
    routes: routeDiagnostics.routes,
    byActionId,
    byRouteId: routeDiagnostics.byRouteId,
    dispatchCount: dispatchViews.length,
    routeCount: routeDiagnostics.routes.length,
    pendingCount: statusCounts.pendingCount,
    sentCount: statusCounts.sentCount,
    failedCount: statusCounts.failedCount,
    claimedCount: statusCounts.claimedCount,
    latestUpdatedAt: dispatchViews[0]?.updatedAt ?? null,
    isTruncated: totalBeforeTruncation > dispatchViews.length,
    totalBeforeTruncation,
  }
}

export function mapProjectSummary(dto: ProjectSummaryDto): ProjectListItem {
  const branch = normalizeOptionalText(dto.branch)
  const runtime = normalizeOptionalText(dto.runtime)

  return {
    id: dto.id,
    name: normalizeText(dto.name, 'Untitled project'),
    description: normalizeText(dto.description, 'No description provided.'),
    milestone: normalizeText(dto.milestone, 'No milestone assigned'),
    totalPhases: dto.totalPhases,
    completedPhases: Math.min(dto.completedPhases, dto.totalPhases),
    activePhase: dto.activePhase,
    branch: branch ?? 'No branch',
    runtime: runtime ?? 'Runtime unavailable',
    runtimeLabel: runtime ?? 'Runtime unavailable',
    branchLabel: branch ?? 'No branch',
    phaseProgressPercent: safePercent(dto.completedPhases, dto.totalPhases),
  }
}

export function mapRepository(repository: RepositorySummaryDto): RepositoryView {
  const branch = normalizeOptionalText(repository.branch)
  const headSha = normalizeOptionalText(repository.headSha)

  return {
    id: repository.id,
    projectId: repository.projectId,
    rootPath: repository.rootPath,
    displayName: repository.displayName,
    branch,
    branchLabel: branch ?? 'No branch',
    headSha,
    headShaLabel: headSha ?? 'No HEAD',
    isGitRepo: repository.isGitRepo,
  }
}

export function mapPhase(phase: PhaseSummaryDto): Phase {
  const taskCount = phase.taskCount
  const completedTasks = Math.min(phase.completedTasks, taskCount)

  return {
    id: phase.id,
    name: normalizeText(phase.name, `Phase ${phase.id}`),
    description: normalizeText(phase.description, 'No phase description provided.'),
    status: phase.status,
    currentStep: phase.currentStep ?? null,
    stepStatuses: createStepStatuses(phase.status, phase.currentStep ?? null),
    taskCount,
    completedTasks,
    summary: normalizeOptionalText(phase.summary) ?? undefined,
  }
}

export function mapProjectSnapshot(
  snapshot: ProjectSnapshotResponseDto,
  options: { notificationDispatches?: NotificationDispatchDto[] } = {},
): ProjectDetailView {
  const summary = mapProjectSummary(snapshot.project)
  const approvalRequests = snapshot.approvalRequests.map(mapOperatorApproval)
  const verificationRecords = snapshot.verificationRecords.map(mapVerificationRecord)
  const resumeHistory = snapshot.resumeHistory.map(mapResumeHistoryEntry)
  const handoffPackages = (snapshot.handoffPackages ?? [])
    .filter((pkg) => pkg.projectId === snapshot.project.id)
    .map(mapWorkflowHandoffPackage)
  const notificationDispatches = options.notificationDispatches ?? snapshot.notificationDispatches ?? []
  const notificationBroker = mapNotificationBroker(snapshot.project.id, notificationDispatches)

  if (!snapshot.lifecycle) {
    throw new Error('Cadence received a project snapshot without the required lifecycle projection.')
  }

  const autonomousRun = snapshot.autonomousRun ? mapAutonomousRun(snapshot.autonomousRun) : null
  const autonomousUnit = snapshot.autonomousUnit ? mapAutonomousUnit(snapshot.autonomousUnit) : null

  return {
    ...summary,
    phases: snapshot.phases.map(mapPhase),
    lifecycle: mapPlanningLifecycle(snapshot.lifecycle),
    repository: snapshot.repository ? mapRepository(snapshot.repository) : null,
    repositoryStatus: null,
    approvalRequests,
    pendingApprovalCount: approvalRequests.filter((approval) => approval.isPending).length,
    latestDecisionOutcome: getLatestDecisionOutcome(approvalRequests),
    verificationRecords,
    resumeHistory,
    handoffPackages,
    notificationBroker,
    runtimeSession: null,
    runtimeRun: null,
    autonomousRun,
    autonomousUnit,
    autonomousAttempt: null,
    autonomousHistory: [],
    autonomousRecentArtifacts: [],
  }
}

export function mapRepositoryStatus(status: RepositoryStatusResponseDto): RepositoryStatusView {
  const branchName = normalizeOptionalText(status.branch?.name) ?? normalizeOptionalText(status.repository.branch)
  const headSha = normalizeOptionalText(status.branch?.headSha) ?? normalizeOptionalText(status.repository.headSha)
  const entries = status.entries.map((entry) => ({
    path: entry.path,
    staged: entry.staged ?? null,
    unstaged: entry.unstaged ?? null,
    untracked: entry.untracked,
  }))

  const stagedCount = entries.filter((entry) => entry.staged !== null).length
  const unstagedCount = entries.filter((entry) => entry.unstaged !== null).length
  const untrackedCount = entries.filter((entry) => entry.untracked).length
  const uniquePaths = new Set(entries.map((entry) => entry.path))

  return {
    projectId: status.repository.projectId,
    repositoryId: status.repository.id,
    branchLabel: branchName ?? 'No branch',
    headShaLabel: headSha ?? 'No HEAD',
    stagedCount,
    unstagedCount,
    untrackedCount,
    statusCount: uniquePaths.size,
    hasChanges:
      status.hasStagedChanges || status.hasUnstagedChanges || status.hasUntrackedChanges || uniquePaths.size > 0,
    entries,
  }
}

export function mapRepositoryDiff(diff: RepositoryDiffResponseDto): RepositoryDiffView {
  const patch = diff.patch.trim().length > 0 ? diff.patch : ''
  const normalizedBaseRevision = normalizeOptionalText(diff.baseRevision)
  const baseRevisionLabel = normalizedBaseRevision ?? (diff.scope === 'unstaged' ? 'Working tree' : 'No HEAD')

  return {
    projectId: diff.repository.projectId,
    repositoryId: diff.repository.id,
    scope: diff.scope,
    patch,
    isEmpty: patch.length === 0,
    truncated: diff.truncated,
    baseRevisionLabel,
  }
}

export function mapRuntimeSession(runtime: RuntimeSessionDto): RuntimeSessionView {
  const runtimeKind = normalizeText(runtime.runtimeKind, 'openai_codex')
  const providerId = normalizeText(runtime.providerId, 'provider-unavailable')
  const accountId = normalizeOptionalText(runtime.accountId)
  const sessionId = normalizeOptionalText(runtime.sessionId)

  return {
    projectId: runtime.projectId,
    runtimeKind,
    providerId,
    flowId: normalizeOptionalText(runtime.flowId),
    sessionId,
    accountId,
    phase: runtime.phase,
    phaseLabel: getRuntimePhaseLabel(runtime.phase),
    runtimeLabel: getRuntimeLabel(runtimeKind, runtime.phase),
    accountLabel: accountId ?? 'Not signed in',
    sessionLabel: sessionId ?? 'No session',
    callbackBound: runtime.callbackBound ?? null,
    authorizationUrl: normalizeOptionalText(runtime.authorizationUrl),
    redirectUri: normalizeOptionalText(runtime.redirectUri),
    lastErrorCode: normalizeOptionalText(runtime.lastErrorCode),
    lastError: runtime.lastError ?? null,
    updatedAt: runtime.updatedAt,
    isAuthenticated: runtime.phase === 'authenticated',
    isLoginInProgress: [
      'starting',
      'awaiting_browser_callback',
      'awaiting_manual_input',
      'exchanging_code',
      'refreshing',
    ].includes(runtime.phase),
    needsManualInput: runtime.phase === 'awaiting_manual_input',
    isSignedOut: runtime.phase === 'idle',
    isFailed: runtime.phase === 'failed' || runtime.phase === 'cancelled',
  }
}

export function mapRuntimeRunCheckpoint(checkpoint: RuntimeRunCheckpointDto): RuntimeRunCheckpointView {
  return {
    sequence: checkpoint.sequence,
    kind: checkpoint.kind,
    kindLabel: getRuntimeRunCheckpointKindLabel(checkpoint.kind),
    summary: normalizeText(checkpoint.summary, 'Durable checkpoint recorded.'),
    createdAt: checkpoint.createdAt,
  }
}

export function mapRuntimeRun(runtimeRun: RuntimeRunDto): RuntimeRunView {
  const runtimeKind = normalizeText(runtimeRun.runtimeKind, 'openai_codex')
  const providerId = normalizeText(runtimeRun.providerId, 'provider-unavailable')
  const supervisorKind = normalizeText(runtimeRun.supervisorKind, 'detached_pty')
  const checkpoints = runtimeRun.checkpoints
    .map(mapRuntimeRunCheckpoint)
    .sort((left, right) => left.sequence - right.sequence)
  const latestCheckpoint = checkpoints[checkpoints.length - 1] ?? null

  return {
    projectId: runtimeRun.projectId,
    runId: normalizeText(runtimeRun.runId, 'run-unavailable'),
    runtimeKind,
    providerId,
    runtimeLabel: getRuntimeRunLabel(runtimeKind, runtimeRun.status),
    supervisorKind,
    supervisorLabel: humanizeRuntimeKind(supervisorKind),
    status: runtimeRun.status,
    statusLabel: getRuntimeRunStatusLabel(runtimeRun.status),
    transport: {
      kind: normalizeText(runtimeRun.transport.kind, 'tcp'),
      endpoint: normalizeText(runtimeRun.transport.endpoint, 'Unavailable'),
      liveness: runtimeRun.transport.liveness,
      livenessLabel: getRuntimeRunTransportLivenessLabel(runtimeRun.transport.liveness),
    },
    startedAt: runtimeRun.startedAt,
    lastHeartbeatAt: normalizeOptionalText(runtimeRun.lastHeartbeatAt),
    lastCheckpointSequence: runtimeRun.lastCheckpointSequence,
    lastCheckpointAt: normalizeOptionalText(runtimeRun.lastCheckpointAt),
    stoppedAt: normalizeOptionalText(runtimeRun.stoppedAt),
    lastErrorCode: normalizeOptionalText(runtimeRun.lastErrorCode),
    lastError: runtimeRun.lastError ?? null,
    updatedAt: runtimeRun.updatedAt,
    checkpoints,
    latestCheckpoint,
    checkpointCount: checkpoints.length,
    hasCheckpoints: checkpoints.length > 0,
    isActive: runtimeRun.status === 'starting' || runtimeRun.status === 'running',
    isTerminal: runtimeRun.status === 'stopped' || runtimeRun.status === 'failed',
    isStale: runtimeRun.status === 'stale',
    isFailed: runtimeRun.status === 'failed',
  }
}

export function mapAutonomousRun(autonomousRun: AutonomousRunDto): AutonomousRunView {
  const runtimeKind = normalizeText(autonomousRun.runtimeKind, 'openai_codex')
  const providerId = normalizeText(autonomousRun.providerId, 'provider-unavailable')
  const supervisorKind = normalizeText(autonomousRun.supervisorKind, 'detached_pty')

  return {
    projectId: autonomousRun.projectId,
    runId: normalizeText(autonomousRun.runId, 'autonomous-run-unavailable'),
    runtimeKind,
    providerId,
    runtimeLabel: getAutonomousRunLabel(runtimeKind, autonomousRun.status),
    supervisorKind,
    supervisorLabel: humanizeRuntimeKind(supervisorKind),
    status: autonomousRun.status,
    statusLabel: getAutonomousRunStatusLabel(autonomousRun.status),
    recoveryState: autonomousRun.recoveryState,
    recoveryLabel: getAutonomousRunRecoveryLabel(autonomousRun.recoveryState),
    activeUnitId: normalizeOptionalText(autonomousRun.activeUnitId),
    activeAttemptId: normalizeOptionalText(autonomousRun.activeAttemptId),
    duplicateStartDetected: autonomousRun.duplicateStartDetected,
    duplicateStartRunId: normalizeOptionalText(autonomousRun.duplicateStartRunId),
    duplicateStartReason: normalizeOptionalText(autonomousRun.duplicateStartReason),
    startedAt: autonomousRun.startedAt,
    lastHeartbeatAt: normalizeOptionalText(autonomousRun.lastHeartbeatAt),
    lastCheckpointAt: normalizeOptionalText(autonomousRun.lastCheckpointAt),
    pausedAt: normalizeOptionalText(autonomousRun.pausedAt),
    cancelledAt: normalizeOptionalText(autonomousRun.cancelledAt),
    completedAt: normalizeOptionalText(autonomousRun.completedAt),
    crashedAt: normalizeOptionalText(autonomousRun.crashedAt),
    stoppedAt: normalizeOptionalText(autonomousRun.stoppedAt),
    pauseReason: autonomousRun.pauseReason ?? null,
    cancelReason: autonomousRun.cancelReason ?? null,
    crashReason: autonomousRun.crashReason ?? null,
    lastErrorCode: normalizeOptionalText(autonomousRun.lastErrorCode),
    lastError: autonomousRun.lastError ?? null,
    updatedAt: autonomousRun.updatedAt,
    isActive: autonomousRun.status === 'starting' || autonomousRun.status === 'running',
    needsRecovery: autonomousRun.recoveryState === 'recovery_required',
    isTerminal: ['cancelled', 'stopped', 'completed'].includes(autonomousRun.status),
    isFailed: ['failed', 'crashed'].includes(autonomousRun.status),
  }
}

function mapAutonomousWorkflowLinkage(
  workflowLinkage: AutonomousWorkflowLinkageDto,
): AutonomousWorkflowLinkageView {
  return {
    workflowNodeId: normalizeText(workflowLinkage.workflowNodeId, 'workflow-node-unavailable'),
    transitionId: normalizeText(workflowLinkage.transitionId, 'workflow-transition-unavailable'),
    causalTransitionId: normalizeOptionalText(workflowLinkage.causalTransitionId),
    handoffTransitionId: normalizeText(
      workflowLinkage.handoffTransitionId,
      'workflow-handoff-transition-unavailable',
    ),
    handoffPackageHash: normalizeText(
      workflowLinkage.handoffPackageHash,
      'workflow-handoff-package-hash-unavailable',
    ),
  }
}

export function mapAutonomousUnit(autonomousUnit: AutonomousUnitDto): AutonomousUnitView {
  return {
    projectId: autonomousUnit.projectId,
    runId: autonomousUnit.runId,
    unitId: normalizeText(autonomousUnit.unitId, 'autonomous-unit-unavailable'),
    sequence: autonomousUnit.sequence,
    kind: autonomousUnit.kind,
    kindLabel: getAutonomousUnitKindLabel(autonomousUnit.kind),
    status: autonomousUnit.status,
    statusLabel: getAutonomousUnitStatusLabel(autonomousUnit.status),
    summary: normalizeText(autonomousUnit.summary, 'Autonomous unit boundary recorded.'),
    boundaryId: normalizeOptionalText(autonomousUnit.boundaryId),
    workflowLinkage: autonomousUnit.workflowLinkage
      ? mapAutonomousWorkflowLinkage(autonomousUnit.workflowLinkage)
      : null,
    startedAt: autonomousUnit.startedAt,
    finishedAt: normalizeOptionalText(autonomousUnit.finishedAt),
    updatedAt: autonomousUnit.updatedAt,
    lastErrorCode: normalizeOptionalText(autonomousUnit.lastErrorCode),
    lastError: autonomousUnit.lastError ?? null,
    isActive: autonomousUnit.status === 'active',
    isTerminal: ['completed', 'cancelled', 'failed'].includes(autonomousUnit.status),
    isFailed: autonomousUnit.status === 'failed',
  }
}

function getAutonomousArtifactKindLabel(artifactKind: string): string {
  switch (artifactKind) {
    case 'tool_result':
      return 'Tool result'
    case 'verification_evidence':
      return 'Verification evidence'
    case 'policy_denied':
      return 'Policy denied'
    default:
      return humanizeRuntimeKind(artifactKind)
  }
}

function getAutonomousArtifactStatusLabel(status: AutonomousUnitArtifactStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'recorded':
      return 'Recorded'
    case 'rejected':
      return 'Rejected'
  }
}

function getAutonomousToolCallStateLabel(state: AutonomousToolCallStateDto): string {
  switch (state) {
    case 'pending':
      return 'Pending'
    case 'running':
      return 'Running'
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

function getAutonomousVerificationOutcomeLabel(outcome: AutonomousVerificationOutcomeDto): string {
  switch (outcome) {
    case 'passed':
      return 'Passed'
    case 'failed':
      return 'Failed'
    case 'blocked':
      return 'Blocked'
  }
}

export function mapAutonomousAttempt(autonomousAttempt: AutonomousUnitAttemptDto): AutonomousUnitAttemptView {
  return {
    projectId: autonomousAttempt.projectId,
    runId: autonomousAttempt.runId,
    unitId: autonomousAttempt.unitId,
    attemptId: normalizeText(autonomousAttempt.attemptId, 'autonomous-attempt-unavailable'),
    attemptNumber: autonomousAttempt.attemptNumber,
    childSessionId: normalizeText(autonomousAttempt.childSessionId, 'child-session-unavailable'),
    status: autonomousAttempt.status,
    statusLabel: getAutonomousUnitStatusLabel(autonomousAttempt.status),
    boundaryId: normalizeOptionalText(autonomousAttempt.boundaryId),
    workflowLinkage: autonomousAttempt.workflowLinkage
      ? mapAutonomousWorkflowLinkage(autonomousAttempt.workflowLinkage)
      : null,
    startedAt: autonomousAttempt.startedAt,
    finishedAt: normalizeOptionalText(autonomousAttempt.finishedAt),
    updatedAt: autonomousAttempt.updatedAt,
    lastErrorCode: normalizeOptionalText(autonomousAttempt.lastErrorCode),
    lastError: autonomousAttempt.lastError ?? null,
    isActive: autonomousAttempt.status === 'active',
    isTerminal: ['completed', 'cancelled', 'failed'].includes(autonomousAttempt.status),
    isFailed: autonomousAttempt.status === 'failed',
  }
}

function mapAutonomousCommandResult(commandResult: AutonomousCommandResultDto): AutonomousCommandResultView {
  return {
    exitCode: commandResult.exitCode ?? null,
    timedOut: commandResult.timedOut,
    summary: normalizeText(commandResult.summary, 'Autonomous command result recorded.'),
  }
}

function getAutonomousArtifactDetail(
  artifact: AutonomousUnitArtifactDto,
  commandResult: AutonomousCommandResultView | null,
): string | null {
  const payload = artifact.payload ?? null
  if (!payload) {
    return normalizeOptionalText(artifact.summary)
  }

  switch (payload.kind) {
    case 'tool_result':
      return commandResult?.summary ?? normalizeOptionalText(artifact.summary)
    case 'verification_evidence':
      return commandResult?.summary ?? normalizeOptionalText(payload.label) ?? normalizeOptionalText(artifact.summary)
    case 'policy_denied':
      return normalizeOptionalText(payload.message) ?? normalizeOptionalText(artifact.summary)
  }
}

export function mapAutonomousArtifact(artifact: AutonomousUnitArtifactDto): AutonomousUnitArtifactView {
  const payload = artifact.payload ?? null
  const commandResult = payload?.commandResult ? mapAutonomousCommandResult(payload.commandResult) : null

  let toolName: string | null = null
  let toolState: AutonomousToolCallStateDto | null = null
  let toolStateLabel: string | null = null
  let evidenceKind: string | null = null
  let verificationOutcome: AutonomousVerificationOutcomeDto | null = null
  let verificationOutcomeLabel: string | null = null
  let diagnosticCode: string | null = null
  let actionId: string | null = null
  let boundaryId: string | null = null

  switch (payload?.kind) {
    case 'tool_result':
      toolName = normalizeOptionalText(payload.toolName)
      toolState = payload.toolState
      toolStateLabel = getAutonomousToolCallStateLabel(payload.toolState)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
    case 'verification_evidence':
      evidenceKind = normalizeOptionalText(payload.evidenceKind)
      verificationOutcome = payload.outcome
      verificationOutcomeLabel = getAutonomousVerificationOutcomeLabel(payload.outcome)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
    case 'policy_denied':
      toolName = normalizeOptionalText(payload.toolName)
      diagnosticCode = normalizeOptionalText(payload.diagnosticCode)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
  }

  return {
    projectId: artifact.projectId,
    runId: artifact.runId,
    unitId: artifact.unitId,
    attemptId: artifact.attemptId,
    artifactId: normalizeText(artifact.artifactId, 'autonomous-artifact-unavailable'),
    artifactKind: artifact.artifactKind,
    artifactKindLabel: getAutonomousArtifactKindLabel(artifact.artifactKind),
    status: artifact.status,
    statusLabel: getAutonomousArtifactStatusLabel(artifact.status),
    summary: normalizeText(artifact.summary, 'Autonomous artifact recorded.'),
    contentHash: normalizeOptionalText(artifact.contentHash),
    payload,
    createdAt: artifact.createdAt,
    updatedAt: artifact.updatedAt,
    detail: getAutonomousArtifactDetail(artifact, commandResult),
    commandResult,
    toolName,
    toolState,
    toolStateLabel,
    evidenceKind,
    verificationOutcome,
    verificationOutcomeLabel,
    diagnosticCode,
    actionId,
    boundaryId,
    isToolResult: artifact.artifactKind === 'tool_result',
    isVerificationEvidence: artifact.artifactKind === 'verification_evidence',
    isPolicyDenied: artifact.artifactKind === 'policy_denied',
  }
}

export function mapAutonomousHistoryEntry(entry: AutonomousUnitHistoryEntryDto): AutonomousUnitHistoryEntryView {
  return {
    unit: mapAutonomousUnit(entry.unit),
    latestAttempt: entry.latestAttempt ? mapAutonomousAttempt(entry.latestAttempt) : null,
    artifacts: sortByNewest((entry.artifacts ?? []).map(mapAutonomousArtifact), (artifact) => artifact.updatedAt || artifact.createdAt),
  }
}

export function mapAutonomousRunInspection(autonomousState: AutonomousRunStateDto): AutonomousRunInspectionView {
  const autonomousHistory = (autonomousState.history ?? []).map(mapAutonomousHistoryEntry)
  const autonomousRecentArtifacts = sortByNewest(
    autonomousHistory.flatMap((entry) => entry.artifacts),
    (artifact) => artifact.updatedAt || artifact.createdAt,
  ).slice(0, 5)

  return {
    autonomousRun: autonomousState.run ? mapAutonomousRun(autonomousState.run) : null,
    autonomousUnit: autonomousState.unit ? mapAutonomousUnit(autonomousState.unit) : null,
    autonomousAttempt: autonomousState.attempt ? mapAutonomousAttempt(autonomousState.attempt) : null,
    autonomousHistory,
    autonomousRecentArtifacts,
  }
}

export function mergeRuntimeUpdated(
  currentRuntime: RuntimeSessionView | null,
  payload: RuntimeUpdatedPayloadDto,
): RuntimeSessionView {
  if (currentRuntime && timestampToSortValue(payload.updatedAt) < timestampToSortValue(currentRuntime.updatedAt)) {
    return currentRuntime
  }

  const nextFlowId = normalizeOptionalText(payload.flowId)
  const currentFlowId = currentRuntime?.flowId ?? null

  return mapRuntimeSession({
    projectId: payload.projectId,
    runtimeKind: payload.runtimeKind,
    providerId: payload.providerId,
    flowId: nextFlowId,
    sessionId: normalizeOptionalText(payload.sessionId),
    accountId: normalizeOptionalText(payload.accountId),
    phase: payload.authPhase,
    callbackBound: currentFlowId === nextFlowId ? currentRuntime?.callbackBound ?? null : null,
    authorizationUrl: currentFlowId === nextFlowId ? currentRuntime?.authorizationUrl ?? null : null,
    redirectUri: currentFlowId === nextFlowId ? currentRuntime?.redirectUri ?? null : null,
    lastErrorCode: normalizeOptionalText(payload.lastErrorCode),
    lastError: payload.lastError ?? null,
    updatedAt: payload.updatedAt,
  })
}

export function applyProjectSummary(
  project: ProjectDetailView,
  summary: ProjectListItem,
): ProjectDetailView {
  return {
    ...project,
    ...summary,
    phases: project.phases,
    repository: project.repository,
    repositoryStatus: project.repositoryStatus,
    runtimeSession: project.runtimeSession ?? null,
    runtimeRun: project.runtimeRun ?? null,
    autonomousRun: project.autonomousRun ?? null,
    autonomousUnit: project.autonomousUnit ?? null,
  }
}

export function applyRepositoryStatus(
  project: ProjectDetailView,
  status: RepositoryStatusView,
): ProjectDetailView {
  const repository = project.repository
    ? {
        ...project.repository,
        branch: status.branchLabel === 'No branch' ? null : status.branchLabel,
        branchLabel: status.branchLabel,
        headSha: status.headShaLabel === 'No HEAD' ? null : status.headShaLabel,
        headShaLabel: status.headShaLabel,
      }
    : project.repository

  return {
    ...project,
    branch: status.branchLabel,
    branchLabel: status.branchLabel,
    repository,
    repositoryStatus: status,
    runtimeSession: project.runtimeSession ?? null,
    runtimeRun: project.runtimeRun ?? null,
  }
}

export function applyRuntimeSession(
  project: ProjectDetailView,
  runtimeSession: RuntimeSessionView | null,
): ProjectDetailView {
  if (!runtimeSession) {
    return {
      ...project,
      runtimeSession: null,
    }
  }

  return {
    ...project,
    runtime: runtimeSession.runtimeLabel,
    runtimeLabel: runtimeSession.runtimeLabel,
    runtimeSession,
  }
}

export function applyRuntimeRun(
  project: ProjectDetailView,
  runtimeRun: RuntimeRunView | null,
): ProjectDetailView {
  return {
    ...project,
    runtimeRun: runtimeRun ?? null,
  }
}

export function upsertProjectListItem(projects: ProjectListItem[], nextProject: ProjectListItem): ProjectListItem[] {
  const existingIndex = projects.findIndex((project) => project.id === nextProject.id)
  if (existingIndex === -1) {
    return [...projects, nextProject]
  }

  return projects.map((project) => (project.id === nextProject.id ? nextProject : project))
}
