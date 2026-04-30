import { z } from 'zod'
import {
  runtimeRunControlInputSchema,
  runtimeRunDiagnosticSchema,
  type RuntimeRunDiagnosticDto,
} from './runtime'
import { isoTimestampSchema, nonEmptyOptionalTextSchema, normalizeOptionalText, normalizeText } from './shared'

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

export const autonomousLifecycleReasonSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const autonomousRunSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    supervisorKind: z.string().trim().min(1),
    status: autonomousRunStatusSchema,
    recoveryState: autonomousRunRecoveryStateSchema,
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

export const getAutonomousRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
  })
  .strict()

export const startAutonomousRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    initialControls: runtimeRunControlInputSchema.nullable().optional(),
    initialPrompt: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const cancelAutonomousRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
  })
  .strict()

export const autonomousRunStateSchema = z
  .object({
    run: autonomousRunSchema.nullable(),
  })
  .strict()

export type AutonomousRunStatusDto = z.infer<typeof autonomousRunStatusSchema>
export type AutonomousRunRecoveryStateDto = z.infer<typeof autonomousRunRecoveryStateSchema>
export type AutonomousLifecycleReasonDto = z.infer<typeof autonomousLifecycleReasonSchema>
export type AutonomousRunDto = z.infer<typeof autonomousRunSchema>
export type GetAutonomousRunRequestDto = z.infer<typeof getAutonomousRunRequestSchema>
export type StartAutonomousRunRequestDto = z.infer<typeof startAutonomousRunRequestSchema>
export type CancelAutonomousRunRequestDto = z.infer<typeof cancelAutonomousRunRequestSchema>
export type AutonomousRunStateDto = z.infer<typeof autonomousRunStateSchema>

export interface AutonomousLifecycleReasonView {
  code: string
  message: string
}

export interface AutonomousRunView {
  projectId: string
  agentSessionId: string
  runId: string
  runtimeKind: string
  runtimeLabel: string
  providerId: string
  supervisorKind: string
  status: AutonomousRunStatusDto
  statusLabel: string
  recoveryState: AutonomousRunRecoveryStateDto
  recoveryLabel: string
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
  isTerminal: boolean
  isStale: boolean
}

export interface AutonomousRunInspectionView {
  autonomousRun: AutonomousRunView | null
}

export function getAutonomousRunStatusLabel(status: AutonomousRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Starting'
    case 'running':
      return 'Running'
    case 'paused':
      return 'Paused'
    case 'cancelling':
      return 'Cancelling'
    case 'cancelled':
      return 'Cancelled'
    case 'stale':
      return 'Stale'
    case 'failed':
      return 'Failed'
    case 'stopped':
      return 'Stopped'
    case 'crashed':
      return 'Crashed'
    case 'completed':
      return 'Completed'
  }
}

export function getAutonomousRunRecoveryLabel(recoveryState: AutonomousRunRecoveryStateDto): string {
  switch (recoveryState) {
    case 'healthy':
      return 'Healthy'
    case 'recovery_required':
      return 'Recovery required'
    case 'terminal':
      return 'Terminal'
    case 'failed':
      return 'Failed'
  }
}

function getAutonomousRunLabel(runtimeKind: string, status: AutonomousRunStatusDto): string {
  return `${normalizeText(runtimeKind, 'runtime')} · ${getAutonomousRunStatusLabel(status)}`
}

function mapLifecycleReason(reason: AutonomousLifecycleReasonDto | null | undefined): AutonomousLifecycleReasonView | null {
  if (!reason) return null
  return {
    code: reason.code,
    message: reason.message,
  }
}

export function mapAutonomousRun(autonomousRun: AutonomousRunDto): AutonomousRunView {
  const runtimeKind = normalizeText(autonomousRun.runtimeKind, 'runtime')
  const isTerminal = ['cancelled', 'failed', 'stopped', 'crashed', 'completed'].includes(autonomousRun.status)

  return {
    projectId: autonomousRun.projectId,
    agentSessionId: autonomousRun.agentSessionId,
    runId: normalizeText(autonomousRun.runId, 'autonomous-run-unavailable'),
    runtimeKind,
    runtimeLabel: getAutonomousRunLabel(runtimeKind, autonomousRun.status),
    providerId: normalizeText(autonomousRun.providerId, 'provider-unavailable'),
    supervisorKind: normalizeText(autonomousRun.supervisorKind, 'supervisor-unavailable'),
    status: autonomousRun.status,
    statusLabel: getAutonomousRunStatusLabel(autonomousRun.status),
    recoveryState: autonomousRun.recoveryState,
    recoveryLabel: getAutonomousRunRecoveryLabel(autonomousRun.recoveryState),
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
    pauseReason: mapLifecycleReason(autonomousRun.pauseReason),
    cancelReason: mapLifecycleReason(autonomousRun.cancelReason),
    crashReason: mapLifecycleReason(autonomousRun.crashReason),
    lastErrorCode: normalizeOptionalText(autonomousRun.lastErrorCode),
    lastError: autonomousRun.lastError ?? null,
    updatedAt: autonomousRun.updatedAt,
    isActive: ['starting', 'running', 'paused', 'cancelling', 'stale'].includes(autonomousRun.status),
    isTerminal,
    isStale: autonomousRun.status === 'stale',
  }
}

export function mapAutonomousRunInspection(autonomousState: AutonomousRunStateDto): AutonomousRunInspectionView {
  return {
    autonomousRun: autonomousState.run ? mapAutonomousRun(autonomousState.run) : null,
  }
}
