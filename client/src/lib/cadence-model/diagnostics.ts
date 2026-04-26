import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema } from './shared'

export const CADENCE_DIAGNOSTIC_CONTRACT_VERSION = 1
export const CADENCE_DOCTOR_REPORT_CONTRACT_VERSION = 1

export const cadenceDiagnosticSubjectSchema = z.enum([
  'provider_profile',
  'model_catalog',
  'runtime_binding',
  'runtime_supervisor',
  'mcp_registry',
  'settings_dependency',
])
export const cadenceDiagnosticStatusSchema = z.enum(['passed', 'warning', 'failed', 'skipped'])
export const cadenceDiagnosticSeveritySchema = z.enum(['info', 'warning', 'error'])
export const cadenceDiagnosticRedactionClassSchema = z.enum([
  'public',
  'endpoint_credential',
  'local_path',
  'secret',
])

export const cadenceDiagnosticEndpointMetadataSchema = z
  .object({
    baseUrl: nonEmptyOptionalTextSchema,
    host: nonEmptyOptionalTextSchema,
    apiVersion: nonEmptyOptionalTextSchema,
    region: nonEmptyOptionalTextSchema,
    projectId: nonEmptyOptionalTextSchema,
    modelListStrategy: nonEmptyOptionalTextSchema,
    redacted: z.boolean().default(false),
  })
  .strict()

export const cadenceDiagnosticCheckSchema = z
  .object({
    contractVersion: z.literal(CADENCE_DIAGNOSTIC_CONTRACT_VERSION),
    checkId: z.string().trim().min(1),
    subject: cadenceDiagnosticSubjectSchema,
    status: cadenceDiagnosticStatusSchema,
    severity: cadenceDiagnosticSeveritySchema,
    retryable: z.boolean(),
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    affectedProfileId: nonEmptyOptionalTextSchema,
    affectedProviderId: nonEmptyOptionalTextSchema,
    endpoint: cadenceDiagnosticEndpointMetadataSchema.nullable().optional(),
    remediation: nonEmptyOptionalTextSchema,
    redactionClass: cadenceDiagnosticRedactionClassSchema,
    redacted: z.boolean().default(false),
  })
  .strict()
  .superRefine((check, ctx) => {
    if (check.status === 'passed') {
      if (check.severity !== 'info' || check.retryable) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'Passed diagnostic checks must use severity `info` and retryable=false.',
        })
      }
      if (check.remediation) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['remediation'],
          message: 'Passed diagnostic checks must not include remediation text.',
        })
      }
    }

    if (check.status === 'skipped' && (check.severity !== 'info' || check.retryable)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['status'],
        message: 'Skipped diagnostic checks must use severity `info` and retryable=false.',
      })
    }

    if (check.status === 'warning') {
      if (check.severity !== 'warning') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['severity'],
          message: 'Warning diagnostic checks must use severity `warning`.',
        })
      }
      if (!check.remediation) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['remediation'],
          message: 'Warning diagnostic checks must include remediation text.',
        })
      }
    }

    if (check.status === 'failed') {
      if (check.severity !== 'error') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['severity'],
          message: 'Failed diagnostic checks must use severity `error`.',
        })
      }
      if (!check.remediation) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['remediation'],
          message: 'Failed diagnostic checks must include remediation text.',
        })
      }
    }

    if (check.redactionClass === 'public' && check.redacted) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['redacted'],
        message: 'Public diagnostic checks must not be marked redacted.',
      })
    }
    if (check.redactionClass !== 'public' && !check.redacted) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['redacted'],
        message: 'Non-public diagnostic redaction classes must set redacted=true.',
      })
    }
  })

export const cadenceDoctorReportModeSchema = z.enum(['quick_local', 'extended_network'])
export const cadenceDoctorReportOutputModeSchema = z.enum(['compact_human', 'json'])

export const cadenceDoctorVersionInfoSchema = z
  .object({
    appVersion: z.string().trim().min(1),
    runtimeSupervisorVersion: nonEmptyOptionalTextSchema,
    runtimeProtocolVersion: nonEmptyOptionalTextSchema,
  })
  .strict()

export const cadenceDoctorReportSummarySchema = z
  .object({
    passed: z.number().int().nonnegative(),
    warnings: z.number().int().nonnegative(),
    failed: z.number().int().nonnegative(),
    skipped: z.number().int().nonnegative(),
    total: z.number().int().nonnegative(),
    highestSeverity: cadenceDiagnosticSeveritySchema,
  })
  .strict()

export const cadenceDoctorReportSchema = z
  .object({
    contractVersion: z.literal(CADENCE_DOCTOR_REPORT_CONTRACT_VERSION),
    reportId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    mode: cadenceDoctorReportModeSchema,
    versions: cadenceDoctorVersionInfoSchema,
    summary: cadenceDoctorReportSummarySchema,
    profileChecks: z.array(cadenceDiagnosticCheckSchema).default([]),
    modelCatalogChecks: z.array(cadenceDiagnosticCheckSchema).default([]),
    runtimeSupervisorChecks: z.array(cadenceDiagnosticCheckSchema).default([]),
    mcpDependencyChecks: z.array(cadenceDiagnosticCheckSchema).default([]),
    settingsDependencyChecks: z.array(cadenceDiagnosticCheckSchema).default([]),
  })
  .strict()
  .superRefine((report, ctx) => {
    const expected = summarizeDiagnosticChecks(collectDoctorChecks(report))
    if (JSON.stringify(report.summary) !== JSON.stringify(expected)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary'],
        message: 'Cadence doctor report summary counts must match the included checks.',
      })
    }
  })

export type CadenceDiagnosticSubjectDto = z.infer<typeof cadenceDiagnosticSubjectSchema>
export type CadenceDiagnosticStatusDto = z.infer<typeof cadenceDiagnosticStatusSchema>
export type CadenceDiagnosticSeverityDto = z.infer<typeof cadenceDiagnosticSeveritySchema>
export type CadenceDiagnosticRedactionClassDto = z.infer<typeof cadenceDiagnosticRedactionClassSchema>
export type CadenceDiagnosticEndpointMetadataDto = z.infer<typeof cadenceDiagnosticEndpointMetadataSchema>
export type CadenceDiagnosticCheckDto = z.infer<typeof cadenceDiagnosticCheckSchema>
export type CadenceDoctorReportModeDto = z.infer<typeof cadenceDoctorReportModeSchema>
export type CadenceDoctorReportOutputModeDto = z.infer<typeof cadenceDoctorReportOutputModeSchema>
export type CadenceDoctorVersionInfoDto = z.infer<typeof cadenceDoctorVersionInfoSchema>
export type CadenceDoctorReportSummaryDto = z.infer<typeof cadenceDoctorReportSummarySchema>
export type CadenceDoctorReportDto = z.infer<typeof cadenceDoctorReportSchema>

export interface CadenceDiagnosticCheckInput {
  subject: CadenceDiagnosticSubjectDto
  status: CadenceDiagnosticStatusDto
  severity: CadenceDiagnosticSeverityDto
  retryable: boolean
  code: string
  message: string
  affectedProfileId?: string | null
  affectedProviderId?: string | null
  endpoint?: CadenceDiagnosticEndpointMetadataDto | null
  remediation?: string | null
}

export interface CadenceDoctorReportInput {
  reportId: string
  generatedAt: string
  mode: CadenceDoctorReportModeDto
  versions: CadenceDoctorVersionInfoDto
  profileChecks?: CadenceDiagnosticCheckDto[]
  modelCatalogChecks?: CadenceDiagnosticCheckDto[]
  runtimeSupervisorChecks?: CadenceDiagnosticCheckDto[]
  mcpDependencyChecks?: CadenceDiagnosticCheckDto[]
  settingsDependencyChecks?: CadenceDiagnosticCheckDto[]
}

export function createCadenceDiagnosticCheck(input: CadenceDiagnosticCheckInput): CadenceDiagnosticCheckDto {
  const message = sanitizeDiagnosticText(input.message)
  const remediation = input.remediation ? sanitizeDiagnosticText(input.remediation) : null
  const endpoint = sanitizeEndpointMetadata(input.endpoint ?? null)
  const redactionClass = strongestRedactionClass(
    strongestRedactionClass(message.redactionClass, remediation?.redactionClass ?? 'public'),
    endpoint?.redactionClass ?? 'public',
  )
  const redacted = message.redacted || Boolean(remediation?.redacted) || Boolean(endpoint?.redacted)

  return cadenceDiagnosticCheckSchema.parse({
    contractVersion: CADENCE_DIAGNOSTIC_CONTRACT_VERSION,
    checkId: diagnosticCheckId(input.subject, input.affectedProviderId, input.affectedProfileId, input.code),
    subject: input.subject,
    status: input.status,
    severity: input.severity,
    retryable: input.retryable,
    code: input.code.trim(),
    message: message.value,
    affectedProfileId: normalizeOptionalText(input.affectedProfileId),
    affectedProviderId: normalizeOptionalText(input.affectedProviderId),
    endpoint: endpoint?.value ?? null,
    remediation: remediation?.value ?? null,
    redactionClass,
    redacted,
  })
}

export function createCadenceDoctorReport(input: CadenceDoctorReportInput): CadenceDoctorReportDto {
  const report = {
    contractVersion: CADENCE_DOCTOR_REPORT_CONTRACT_VERSION,
    reportId: input.reportId.trim(),
    generatedAt: input.generatedAt.trim(),
    mode: input.mode,
    versions: {
      appVersion: sanitizeDiagnosticText(input.versions.appVersion).value,
      runtimeSupervisorVersion: input.versions.runtimeSupervisorVersion
        ? sanitizeDiagnosticText(input.versions.runtimeSupervisorVersion).value
        : null,
      runtimeProtocolVersion: input.versions.runtimeProtocolVersion
        ? sanitizeDiagnosticText(input.versions.runtimeProtocolVersion).value
        : null,
    },
    summary: {
      passed: 0,
      warnings: 0,
      failed: 0,
      skipped: 0,
      total: 0,
      highestSeverity: 'info' as const,
    },
    profileChecks: sortDiagnosticChecks(input.profileChecks ?? []),
    modelCatalogChecks: sortDiagnosticChecks(input.modelCatalogChecks ?? []),
    runtimeSupervisorChecks: sortDiagnosticChecks(input.runtimeSupervisorChecks ?? []),
    mcpDependencyChecks: sortDiagnosticChecks(input.mcpDependencyChecks ?? []),
    settingsDependencyChecks: sortDiagnosticChecks(input.settingsDependencyChecks ?? []),
  }
  report.summary = summarizeDiagnosticChecks(collectDoctorChecks(report))
  return cadenceDoctorReportSchema.parse(report)
}

export function renderCadenceDoctorReport(
  report: CadenceDoctorReportDto,
  mode: CadenceDoctorReportOutputModeDto,
): string {
  const parsed = cadenceDoctorReportSchema.parse(report)
  if (mode === 'json') {
    return JSON.stringify(parsed, null, 2)
  }

  const lines = [
    `Cadence doctor report ${parsed.reportId}`,
    `Generated: ${parsed.generatedAt}`,
    `Mode: ${parsed.mode}`,
    `Summary: ${parsed.summary.passed} passed, ${parsed.summary.warnings} warning(s), ${parsed.summary.failed} failed, ${parsed.summary.skipped} skipped`,
  ]
  pushHumanGroup(lines, 'Provider profiles', parsed.profileChecks)
  pushHumanGroup(lines, 'Model catalogs', parsed.modelCatalogChecks)
  pushHumanGroup(lines, 'Runtime supervisor', parsed.runtimeSupervisorChecks)
  pushHumanGroup(lines, 'MCP dependencies', parsed.mcpDependencyChecks)
  pushHumanGroup(lines, 'Settings dependencies', parsed.settingsDependencyChecks)
  return lines.join('\n')
}

export function summarizeDiagnosticChecks(checks: readonly CadenceDiagnosticCheckDto[]): CadenceDoctorReportSummaryDto {
  const summary: CadenceDoctorReportSummaryDto = {
    passed: 0,
    warnings: 0,
    failed: 0,
    skipped: 0,
    total: checks.length,
    highestSeverity: 'info',
  }
  for (const check of checks) {
    switch (check.status) {
      case 'passed':
        summary.passed += 1
        break
      case 'warning':
        summary.warnings += 1
        break
      case 'failed':
        summary.failed += 1
        break
      case 'skipped':
        summary.skipped += 1
        break
    }
    summary.highestSeverity = highestSeverity(summary.highestSeverity, check.severity)
  }
  return summary
}

export function sanitizeDiagnosticText(value: string): {
  value: string
  redacted: boolean
  redactionClass: CadenceDiagnosticRedactionClassDto
} {
  let redacted = false
  let redactionClass: CadenceDiagnosticRedactionClassDto = 'public'
  let redactNext = false
  const words = value.split(/\s+/).filter(Boolean).map((word) => {
    const bare = trimWordPunctuation(word)
    const lower = bare.toLowerCase()
    if (redactNext) {
      redactNext = false
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, 'secret')
      return word.replace(bare, '[redacted]')
    }

    if (lower === 'bearer' || lower === 'authorization:' || lower === 'authorization') {
      redactNext = true
      return word
    }

    const assignment = redactSensitiveAssignment(bare)
    if (assignment) {
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, 'secret')
      return word.replace(bare, assignment)
    }

    if (looksLikeSecretToken(bare)) {
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, 'secret')
      return '[redacted]'
    }

    if (looksLikeRawLocalPath(bare)) {
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, 'local_path')
      return word.replace(bare, '[redacted-path]')
    }

    return word
  })

  return { value: words.join(' '), redacted, redactionClass }
}

function sanitizeEndpointMetadata(
  endpoint: CadenceDiagnosticEndpointMetadataDto | null,
): { value: CadenceDiagnosticEndpointMetadataDto; redacted: boolean; redactionClass: CadenceDiagnosticRedactionClassDto } | null {
  if (!endpoint) {
    return null
  }

  const baseUrl = endpoint.baseUrl ? sanitizeEndpointUrl(endpoint.baseUrl) : null
  const apiVersion = endpoint.apiVersion ? sanitizeDiagnosticText(endpoint.apiVersion) : null
  const region = endpoint.region ? sanitizeDiagnosticText(endpoint.region) : null
  const projectId = endpoint.projectId ? sanitizeDiagnosticText(endpoint.projectId) : null
  const modelListStrategy = endpoint.modelListStrategy ? sanitizeDiagnosticText(endpoint.modelListStrategy) : null
  const redactionClass = [baseUrl?.redactionClass, apiVersion?.redactionClass, region?.redactionClass, projectId?.redactionClass, modelListStrategy?.redactionClass]
    .filter(Boolean)
    .reduce<CadenceDiagnosticRedactionClassDto>((current, next) => strongestRedactionClass(current, next as CadenceDiagnosticRedactionClassDto), 'public')
  const redacted = Boolean(endpoint.redacted || baseUrl?.redacted || apiVersion?.redacted || region?.redacted || projectId?.redacted || modelListStrategy?.redacted)

  return {
    value: cadenceDiagnosticEndpointMetadataSchema.parse({
      baseUrl: baseUrl?.value ?? null,
      host: baseUrl?.host ?? normalizeOptionalText(endpoint.host),
      apiVersion: apiVersion?.value ?? null,
      region: region?.value ?? null,
      projectId: projectId?.value ?? null,
      modelListStrategy: modelListStrategy?.value ?? null,
      redacted,
    }),
    redacted,
    redactionClass,
  }
}

function sanitizeEndpointUrl(value: string): {
  value: string
  host: string | null
  redacted: boolean
  redactionClass: CadenceDiagnosticRedactionClassDto
} {
  try {
    const url = new URL(value.trim())
    let redacted = false
    if (url.username) {
      url.username = 'redacted'
      redacted = true
    }
    if (url.password) {
      url.password = ''
      redacted = true
    }
    if (url.pathname.split('/').some(looksLikeSecretPathSegment)) {
      url.pathname = '/[redacted-path]'
      redacted = true
    }
    for (const key of [...url.searchParams.keys()]) {
      const existing = url.searchParams.get(key)
      if (existing && isSensitiveName(key)) {
        url.searchParams.set(key, '[redacted]')
        redacted = true
      }
    }
    return {
      value: url.toString(),
      host: url.hostname || null,
      redacted,
      redactionClass: redacted ? 'endpoint_credential' : 'public',
    }
  } catch {
    const sanitized = sanitizeDiagnosticText(value)
    return { value: sanitized.value, host: null, redacted: sanitized.redacted, redactionClass: sanitized.redactionClass }
  }
}

function collectDoctorChecks(report: Pick<CadenceDoctorReportDto, 'profileChecks' | 'modelCatalogChecks' | 'runtimeSupervisorChecks' | 'mcpDependencyChecks' | 'settingsDependencyChecks'>): CadenceDiagnosticCheckDto[] {
  return [
    ...report.profileChecks,
    ...report.modelCatalogChecks,
    ...report.runtimeSupervisorChecks,
    ...report.mcpDependencyChecks,
    ...report.settingsDependencyChecks,
  ]
}

function sortDiagnosticChecks(checks: readonly CadenceDiagnosticCheckDto[]): CadenceDiagnosticCheckDto[] {
  return [...checks].sort((left, right) =>
    [
      left.subject,
      left.affectedProviderId ?? '',
      left.affectedProfileId ?? '',
      left.code,
      left.checkId,
    ].join('\u0000').localeCompare([
      right.subject,
      right.affectedProviderId ?? '',
      right.affectedProfileId ?? '',
      right.code,
      right.checkId,
    ].join('\u0000')),
  )
}

function pushHumanGroup(lines: string[], label: string, checks: readonly CadenceDiagnosticCheckDto[]): void {
  if (checks.length === 0) {
    return
  }
  lines.push(`${label}:`)
  for (const check of checks) {
    const remediation = check.remediation ? ` Remediation: ${check.remediation}` : ''
    lines.push(`- [${check.status}] ${check.code}: ${check.message}${remediation}`)
  }
}

function diagnosticCheckId(
  subject: CadenceDiagnosticSubjectDto,
  providerId: string | null | undefined,
  profileId: string | null | undefined,
  code: string,
): string {
  return `diagnostic:v${CADENCE_DIAGNOSTIC_CONTRACT_VERSION}:${subject}:${providerId?.trim() || 'global'}:${profileId?.trim() || 'global'}:${code.trim()}`.toLowerCase()
}

function normalizeOptionalText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function highestSeverity(
  left: CadenceDiagnosticSeverityDto,
  right: CadenceDiagnosticSeverityDto,
): CadenceDiagnosticSeverityDto {
  const rank: Record<CadenceDiagnosticSeverityDto, number> = { info: 0, warning: 1, error: 2 }
  return rank[right] > rank[left] ? right : left
}

function strongestRedactionClass(
  left: CadenceDiagnosticRedactionClassDto,
  right: CadenceDiagnosticRedactionClassDto,
): CadenceDiagnosticRedactionClassDto {
  const rank: Record<CadenceDiagnosticRedactionClassDto, number> = {
    public: 0,
    endpoint_credential: 1,
    local_path: 2,
    secret: 3,
  }
  return rank[right] > rank[left] ? right : left
}

function trimWordPunctuation(value: string): string {
  return value.replace(/^[,;:.()[\]"']+|[,;:.()[\]"']+$/g, '')
}

function redactSensitiveAssignment(value: string): string | null {
  for (const separator of ['=', ':']) {
    const index = value.indexOf(separator)
    if (index > 0) {
      const key = value.slice(0, index)
      const secret = value.slice(index + 1)
      if (secret.trim().length > 0 && isSensitiveName(key)) {
        return `${key}${separator}[redacted]`
      }
    }
  }
  return null
}

function isSensitiveName(value: string): boolean {
  const normalized = value.trim().replace(/^-+/, '').toLowerCase().replace(/-/g, '_')
  return [
    'access_token',
    'api_key',
    'apikey',
    'authorization',
    'auth_token',
    'bearer',
    'client_secret',
    'password',
    'private_key',
    'refresh_token',
    'secret',
    'session_id',
    'session_token',
    'token',
    'x_api_key',
  ].includes(normalized)
}

function looksLikeSecretToken(value: string): boolean {
  const normalized = value.toLowerCase()
  return (
    normalized.includes('sk-') ||
    normalized.includes('github_pat_') ||
    normalized.includes('ghp_') ||
    normalized.includes('gho_') ||
    normalized.includes('ghu_') ||
    normalized.includes('ghs_') ||
    normalized.includes('glpat-') ||
    normalized.includes('xoxb-') ||
    normalized.includes('xoxp-') ||
    normalized.includes('ya29.') ||
    normalized.includes('-----begin') ||
    normalized.startsWith('akia')
  )
}

function looksLikeRawLocalPath(value: string): boolean {
  return (
    value.startsWith('/Users/') ||
    value.startsWith('/home/') ||
    value.startsWith('/var/folders/') ||
    value.startsWith('/tmp/') ||
    value.startsWith('~/') ||
    value.startsWith('\\Users\\') ||
    value.includes(':\\Users\\')
  )
}

function looksLikeSecretPathSegment(value: string): boolean {
  return looksLikeSecretToken(value) || (value.length >= 32 && /^[A-Za-z0-9]+$/.test(value))
}
