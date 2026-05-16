import { z } from 'zod'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
} from '@xero/ui/model/shared'

export const operatorApprovalStatusSchema = z.enum(['pending', 'approved', 'rejected'])
export const verificationRecordStatusSchema = z.enum(['pending', 'passed', 'failed'])
export const resumeHistoryStatusSchema = z.enum(['started', 'failed'])

export type OperatorApprovalAnswerRequirementReason = 'optional' | 'runtime_resumable'
export type OperatorApprovalAnswerShapeKind =
  | 'plain_text'
  | 'terminal_input'
  | 'single_choice'
  | 'multi_choice'
  | 'short_text'
  | 'long_text'
  | 'number'
  | 'date'
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
}

interface OperatorApprovalAnswerPolicy {
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
    'Provide concise plain-text decision context without secrets. Xero rejects secret-bearing payloads.',
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
  single_choice_required: {
    kind: 'single_choice',
    label: 'Single-choice selection',
    guidance:
      'Pick exactly one option from the list provided by the agent. Xero submits the chosen option id as the user answer.',
    placeholder: 'Choose one option to resume the agent run.',
  },
  multi_choice_required: {
    kind: 'multi_choice',
    label: 'Multi-choice selection',
    guidance:
      'Pick one or more options from the list provided by the agent. Xero submits the chosen option ids as a JSON array.',
    placeholder: 'Choose one or more options to resume the agent run.',
  },
  short_text_required: {
    kind: 'short_text',
    label: 'Short-text response',
    guidance:
      'Provide a concise answer for the planning prompt without secrets.',
    placeholder: 'Enter a short answer.',
  },
  long_text_required: {
    kind: 'long_text',
    label: 'Detailed text response',
    guidance:
      'Provide the requested planning detail in plain text without secrets.',
    placeholder: 'Enter the requested details.',
  },
  number_required: {
    kind: 'number',
    label: 'Number response',
    guidance:
      'Provide the requested numeric value for the planning prompt.',
    placeholder: 'Enter a number.',
  },
  date_required: {
    kind: 'date',
    label: 'Date response',
    guidance:
      'Provide the requested date for the planning prompt.',
    placeholder: 'Choose a date.',
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
    case 'runtime_resumable':
      return 'Required — runtime-resumable approvals need a non-empty user answer before approval.'
    case 'optional':
      return 'Optional — this action can be approved or rejected without a user answer.'
  }
}

export function deriveOperatorApprovalAnswerPolicy(
  input: OperatorApprovalAnswerPolicyInput,
): OperatorApprovalAnswerPolicy {
  const runtimeScopeClassification = classifyRuntimeResumableOperatorAction(input)
  const isRuntimeResumable = runtimeScopeClassification === 'runtime_resumable'

  const requirementReason: OperatorApprovalAnswerRequirementReason = isRuntimeResumable
    ? 'runtime_resumable'
    : 'optional'

  return {
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
    userAnswer: nonEmptyOptionalTextSchema,
    status: operatorApprovalStatusSchema,
    decisionNote: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    resolvedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
  .superRefine((approval, ctx) => {
    const userAnswer = approval.userAnswer ?? null
    const decisionNote = approval.decisionNote ?? null

    const answerPolicy = deriveOperatorApprovalAnswerPolicy(approval)

    if (answerPolicy.runtimeScopeClassification === 'runtime_malformed') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['actionId'],
        message:
          'Runtime-scoped approvals must include consistent scope/run/boundary/action metadata before Xero can evaluate answer requirements.',
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
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['userAnswer'],
        message: 'Approved runtime-resumable approvals must include a non-empty `userAnswer`.',
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

export const resumeOperatorRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    userAnswer: nonEmptyOptionalTextSchema,
  })
  .strict()

export const resumeOperatorRunResponseSchema = z.object({
  approvalRequest: operatorApprovalSchema,
  resumeEntry: resumeHistoryEntrySchema,
})

export type OperatorApprovalStatusDto = z.infer<typeof operatorApprovalStatusSchema>
export type VerificationRecordStatusDto = z.infer<typeof verificationRecordStatusSchema>
export type ResumeHistoryStatusDto = z.infer<typeof resumeHistoryStatusSchema>
export type OperatorApprovalDto = z.infer<typeof operatorApprovalSchema>
export type VerificationRecordDto = z.infer<typeof verificationRecordSchema>
export type ResumeHistoryEntryDto = z.infer<typeof resumeHistoryEntrySchema>
export type ResolveOperatorActionRequestDto = z.infer<typeof resolveOperatorActionRequestSchema>
export type ResolveOperatorActionResponseDto = z.infer<typeof resolveOperatorActionResponseSchema>
export type ResumeOperatorRunRequestDto = z.infer<typeof resumeOperatorRunRequestSchema>
export type ResumeOperatorRunResponseDto = z.infer<typeof resumeOperatorRunResponseSchema>

export interface OperatorApprovalView {
  actionId: string
  sessionId: string | null
  flowId: string | null
  actionType: string
  title: string
  detail: string
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

export interface OperatorDecisionOutcomeView {
  actionId: string
  title: string
  status: Extract<OperatorApprovalStatusDto, 'approved' | 'rejected'>
  statusLabel: string
  userAnswer: string | null
  decisionNote: string | null
  resolvedAt: string
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
  const userAnswer = normalizeOptionalText(approval.userAnswer)
  const answerPolicy = deriveOperatorApprovalAnswerPolicy({
    actionId: approval.actionId,
    sessionId,
    flowId,
    actionType,
  })

  return {
    actionId: approval.actionId,
    sessionId,
    flowId,
    actionType,
    title: normalizeText(approval.title, 'Action required'),
    detail: normalizeText(approval.detail, 'Review the operator action before continuing.'),
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
    userAnswer: latestResolvedApproval.userAnswer,
    decisionNote: latestResolvedApproval.decisionNote,
    resolvedAt: latestResolvedApproval.resolvedAt,
  }
}
