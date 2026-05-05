import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema } from './shared'
import { providerModelCatalogSchema } from './provider-models'
import { runtimeProviderIdSchema } from './runtime'

export const XERO_DIAGNOSTIC_CONTRACT_VERSION = 1
export const XERO_DOCTOR_REPORT_CONTRACT_VERSION = 1

export const xeroDiagnosticSubjectSchema = z.enum([
  'dictation',
  'provider_credential',
  'model_catalog',
  'runtime_binding',
  'runtime_supervisor',
  'mcp_registry',
  'settings_dependency',
])
export const xeroDiagnosticStatusSchema = z.enum(['passed', 'warning', 'failed', 'skipped'])
export const xeroDiagnosticSeveritySchema = z.enum(['info', 'warning', 'error'])
export const xeroDiagnosticRedactionClassSchema = z.enum([
  'public',
  'endpoint_credential',
  'local_path',
  'secret',
])

export const xeroDiagnosticEndpointMetadataSchema = z
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

export const xeroDiagnosticCheckSchema = z
  .object({
    contractVersion: z.literal(XERO_DIAGNOSTIC_CONTRACT_VERSION),
    checkId: z.string().trim().min(1),
    subject: xeroDiagnosticSubjectSchema,
    status: xeroDiagnosticStatusSchema,
    severity: xeroDiagnosticSeveritySchema,
    retryable: z.boolean(),
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    affectedProfileId: nonEmptyOptionalTextSchema,
    affectedProviderId: nonEmptyOptionalTextSchema,
    endpoint: xeroDiagnosticEndpointMetadataSchema.nullable().optional(),
    remediation: nonEmptyOptionalTextSchema,
    redactionClass: xeroDiagnosticRedactionClassSchema,
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

export const xeroDoctorReportModeSchema = z.enum(['quick_local', 'extended_network'])
export const xeroDoctorReportOutputModeSchema = z.enum(['compact_human', 'json'])

export const xeroDoctorVersionInfoSchema = z
  .object({
    appVersion: z.string().trim().min(1),
    runtimeSupervisorVersion: nonEmptyOptionalTextSchema,
    runtimeProtocolVersion: nonEmptyOptionalTextSchema,
  })
  .strict()

export const xeroDoctorReportSummarySchema = z
  .object({
    passed: z.number().int().nonnegative(),
    warnings: z.number().int().nonnegative(),
    failed: z.number().int().nonnegative(),
    skipped: z.number().int().nonnegative(),
    total: z.number().int().nonnegative(),
    highestSeverity: xeroDiagnosticSeveritySchema,
  })
  .strict()

export const xeroDoctorReportSchema = z
  .object({
    contractVersion: z.literal(XERO_DOCTOR_REPORT_CONTRACT_VERSION),
    reportId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    mode: xeroDoctorReportModeSchema,
    versions: xeroDoctorVersionInfoSchema,
    summary: xeroDoctorReportSummarySchema,
    dictationChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    profileChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    modelCatalogChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    runtimeSupervisorChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    mcpDependencyChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    settingsDependencyChecks: z.array(xeroDiagnosticCheckSchema).default([]),
  })
  .strict()
  .superRefine((report, ctx) => {
    const expected = summarizeDiagnosticChecks(collectDoctorChecks(report))
    if (JSON.stringify(report.summary) !== JSON.stringify(expected)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary'],
        message: 'Xero doctor report summary counts must match the included checks.',
      })
    }
  })

export const runDoctorReportRequestSchema = z
  .object({
    mode: xeroDoctorReportModeSchema.default('quick_local'),
  })
  .strict()

export const checkProviderProfileRequestSchema = z
  .object({
    profileId: z.string().trim().min(1),
    includeNetwork: z.boolean().default(false),
    modelId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const providerProfileDiagnosticsSchema = z
  .object({
    checkedAt: isoTimestampSchema,
    profileId: z.string().trim().min(1),
    providerId: runtimeProviderIdSchema,
    validationChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    reachabilityChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    capabilityChecks: z.array(xeroDiagnosticCheckSchema).default([]),
    modelCatalog: providerModelCatalogSchema.nullable().optional(),
  })
  .strict()
  .superRefine((diagnostics, ctx) => {
    for (const [index, check] of diagnostics.validationChecks.entries()) {
      if (check.affectedProfileId && check.affectedProfileId !== diagnostics.profileId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['validationChecks', index, 'affectedProfileId'],
          message: 'Provider diagnostics must not include validation checks for another provider connection.',
        })
      }
    }

    for (const [index, check] of diagnostics.reachabilityChecks.entries()) {
      if (check.affectedProfileId && check.affectedProfileId !== diagnostics.profileId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['reachabilityChecks', index, 'affectedProfileId'],
          message: 'Provider diagnostics must not include reachability checks for another provider connection.',
        })
      }
    }

    for (const [index, check] of diagnostics.capabilityChecks.entries()) {
      if (check.affectedProfileId && check.affectedProfileId !== diagnostics.profileId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['capabilityChecks', index, 'affectedProfileId'],
          message: 'Provider diagnostics must not include capability checks for another provider connection.',
        })
      }
    }

    if (diagnostics.modelCatalog && diagnostics.modelCatalog.profileId !== diagnostics.profileId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['modelCatalog', 'profileId'],
        message: 'Provider diagnostics model catalog must belong to the checked provider connection.',
      })
    }
  })

export type XeroDiagnosticSubjectDto = z.infer<typeof xeroDiagnosticSubjectSchema>
export type XeroDiagnosticStatusDto = z.infer<typeof xeroDiagnosticStatusSchema>
export type XeroDiagnosticSeverityDto = z.infer<typeof xeroDiagnosticSeveritySchema>
export type XeroDiagnosticRedactionClassDto = z.infer<typeof xeroDiagnosticRedactionClassSchema>
export type XeroDiagnosticEndpointMetadataDto = z.infer<typeof xeroDiagnosticEndpointMetadataSchema>
export type XeroDiagnosticCheckDto = z.infer<typeof xeroDiagnosticCheckSchema>
export type XeroDoctorReportModeDto = z.infer<typeof xeroDoctorReportModeSchema>
export type XeroDoctorReportOutputModeDto = z.infer<typeof xeroDoctorReportOutputModeSchema>
export type XeroDoctorVersionInfoDto = z.infer<typeof xeroDoctorVersionInfoSchema>
export type XeroDoctorReportSummaryDto = z.infer<typeof xeroDoctorReportSummarySchema>
export type XeroDoctorReportDto = z.infer<typeof xeroDoctorReportSchema>
export type RunDoctorReportRequestDto = z.infer<typeof runDoctorReportRequestSchema>
export type CheckProviderProfileRequestDto = z.infer<typeof checkProviderProfileRequestSchema>
export type ProviderProfileDiagnosticsDto = z.infer<typeof providerProfileDiagnosticsSchema>

export interface XeroDiagnosticCheckInput {
  subject: XeroDiagnosticSubjectDto
  status: XeroDiagnosticStatusDto
  severity: XeroDiagnosticSeverityDto
  retryable: boolean
  code: string
  message: string
  affectedProfileId?: string | null
  affectedProviderId?: string | null
  endpoint?: XeroDiagnosticEndpointMetadataDto | null
  remediation?: string | null
}

export interface XeroDoctorReportInput {
  reportId: string
  generatedAt: string
  mode: XeroDoctorReportModeDto
  versions: XeroDoctorVersionInfoDto
  profileChecks?: XeroDiagnosticCheckDto[]
  dictationChecks?: XeroDiagnosticCheckDto[]
  modelCatalogChecks?: XeroDiagnosticCheckDto[]
  runtimeSupervisorChecks?: XeroDiagnosticCheckDto[]
  mcpDependencyChecks?: XeroDiagnosticCheckDto[]
  settingsDependencyChecks?: XeroDiagnosticCheckDto[]
}

export function createXeroDiagnosticCheck(input: XeroDiagnosticCheckInput): XeroDiagnosticCheckDto {
  const message = sanitizeDiagnosticText(input.message)
  const remediation = input.remediation ? sanitizeDiagnosticText(input.remediation) : null
  const endpoint = sanitizeEndpointMetadata(input.endpoint ?? null)
  const redactionClass = strongestRedactionClass(
    strongestRedactionClass(message.redactionClass, remediation?.redactionClass ?? 'public'),
    endpoint?.redactionClass ?? 'public',
  )
  const redacted = message.redacted || Boolean(remediation?.redacted) || Boolean(endpoint?.redacted)

  return xeroDiagnosticCheckSchema.parse({
    contractVersion: XERO_DIAGNOSTIC_CONTRACT_VERSION,
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

export function createXeroDoctorReport(input: XeroDoctorReportInput): XeroDoctorReportDto {
  const report: XeroDoctorReportDto = {
    contractVersion: XERO_DOCTOR_REPORT_CONTRACT_VERSION,
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
    dictationChecks: sanitizeAndSortDiagnosticChecks(input.dictationChecks ?? []),
    profileChecks: sanitizeAndSortDiagnosticChecks(input.profileChecks ?? []),
    modelCatalogChecks: sanitizeAndSortDiagnosticChecks(input.modelCatalogChecks ?? []),
    runtimeSupervisorChecks: sanitizeAndSortDiagnosticChecks(input.runtimeSupervisorChecks ?? []),
    mcpDependencyChecks: sanitizeAndSortDiagnosticChecks(input.mcpDependencyChecks ?? []),
    settingsDependencyChecks: sanitizeAndSortDiagnosticChecks(input.settingsDependencyChecks ?? []),
  }
  report.summary = summarizeDiagnosticChecks(collectDoctorChecks(report))
  return xeroDoctorReportSchema.parse(report)
}

export function renderXeroDoctorReport(
  report: XeroDoctorReportDto,
  mode: XeroDoctorReportOutputModeDto,
): string {
  const parsed = xeroDoctorReportSchema.parse(report)
  const sanitized = createXeroDoctorReport(parsed)
  if (mode === 'json') {
    return JSON.stringify(sanitized, null, 2)
  }

  const lines = [
    `Xero doctor report ${sanitized.reportId}`,
    `Generated: ${sanitized.generatedAt}`,
    `Mode: ${sanitized.mode}`,
    `Summary: ${sanitized.summary.passed} passed, ${sanitized.summary.warnings} warning(s), ${sanitized.summary.failed} failed, ${sanitized.summary.skipped} skipped`,
  ]
  pushHumanGroup(lines, 'Providers', sanitized.profileChecks)
  pushHumanGroup(lines, 'Dictation', sanitized.dictationChecks)
  pushHumanGroup(lines, 'Model catalogs', sanitized.modelCatalogChecks)
  pushHumanGroup(lines, 'Agent runtime', sanitized.runtimeSupervisorChecks)
  pushHumanGroup(lines, 'MCP dependencies', sanitized.mcpDependencyChecks)
  pushHumanGroup(lines, 'Settings dependencies', sanitized.settingsDependencyChecks)
  return lines.join('\n')
}

export function summarizeDiagnosticChecks(checks: readonly XeroDiagnosticCheckDto[]): XeroDoctorReportSummaryDto {
  const summary: XeroDoctorReportSummaryDto = {
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
  redactionClass: XeroDiagnosticRedactionClassDto
} {
  let redacted = false
  let redactionClass: XeroDiagnosticRedactionClassDto = 'public'
  let redactNext = false
  const words = value.split(/\s+/).filter(Boolean).map((word) => {
    const bare = trimWordPunctuation(word)
    const lower = bare.toLowerCase()
    if (redactNext) {
      if (isAuthorizationScheme(lower)) {
        return word
      }

      redactNext = false
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, 'secret')
      return word.replace(bare, '[redacted]')
    }

    if (lower === 'authorization' || lower === 'bearer' || isSensitiveValueLabel(lower)) {
      redactNext = true
      return word
    }

    const assignment = redactSensitiveAssignment(bare)
    if (assignment) {
      redacted = true
      redactionClass = strongestRedactionClass(redactionClass, assignment.redactionClass)
      if (assignment.redactNext) {
        redactNext = true
      }
      return word.replace(bare, assignment.value)
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
  endpoint: XeroDiagnosticEndpointMetadataDto | null,
): { value: XeroDiagnosticEndpointMetadataDto; redacted: boolean; redactionClass: XeroDiagnosticRedactionClassDto } | null {
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
    .reduce<XeroDiagnosticRedactionClassDto>((current, next) => strongestRedactionClass(current, next as XeroDiagnosticRedactionClassDto), 'public')
  const redacted = Boolean(endpoint.redacted || baseUrl?.redacted || apiVersion?.redacted || region?.redacted || projectId?.redacted || modelListStrategy?.redacted)

  return {
    value: xeroDiagnosticEndpointMetadataSchema.parse({
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
  redactionClass: XeroDiagnosticRedactionClassDto
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

function collectDoctorChecks(report: Pick<XeroDoctorReportDto, 'dictationChecks' | 'profileChecks' | 'modelCatalogChecks' | 'runtimeSupervisorChecks' | 'mcpDependencyChecks' | 'settingsDependencyChecks'>): XeroDiagnosticCheckDto[] {
  return [
    ...report.dictationChecks,
    ...report.profileChecks,
    ...report.modelCatalogChecks,
    ...report.runtimeSupervisorChecks,
    ...report.mcpDependencyChecks,
    ...report.settingsDependencyChecks,
  ]
}

function sortDiagnosticChecks(checks: readonly XeroDiagnosticCheckDto[]): XeroDiagnosticCheckDto[] {
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

function sanitizeAndSortDiagnosticChecks(checks: readonly XeroDiagnosticCheckDto[]): XeroDiagnosticCheckDto[] {
  return sortDiagnosticChecks(
    checks.map((check) =>
      createXeroDiagnosticCheck({
        subject: check.subject,
        status: check.status,
        severity: check.severity,
        retryable: check.retryable,
        code: check.code,
        message: check.message,
        affectedProfileId: check.affectedProfileId,
        affectedProviderId: check.affectedProviderId,
        endpoint: check.endpoint,
        remediation: check.remediation,
      }),
    ),
  )
}

function pushHumanGroup(lines: string[], label: string, checks: readonly XeroDiagnosticCheckDto[]): void {
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
  subject: XeroDiagnosticSubjectDto,
  providerId: string | null | undefined,
  profileId: string | null | undefined,
  code: string,
): string {
  return `diagnostic:v${XERO_DIAGNOSTIC_CONTRACT_VERSION}:${subject}:${providerId?.trim() || 'global'}:${profileId?.trim() || 'global'}:${code.trim()}`.toLowerCase()
}

function normalizeOptionalText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function highestSeverity(
  left: XeroDiagnosticSeverityDto,
  right: XeroDiagnosticSeverityDto,
): XeroDiagnosticSeverityDto {
  const rank: Record<XeroDiagnosticSeverityDto, number> = { info: 0, warning: 1, error: 2 }
  return rank[right] > rank[left] ? right : left
}

function strongestRedactionClass(
  left: XeroDiagnosticRedactionClassDto,
  right: XeroDiagnosticRedactionClassDto,
): XeroDiagnosticRedactionClassDto {
  const rank: Record<XeroDiagnosticRedactionClassDto, number> = {
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

function redactSensitiveAssignment(value: string): {
  value: string
  redactionClass: XeroDiagnosticRedactionClassDto
  redactNext: boolean
} | null {
  for (const separator of ['=', ':']) {
    const index = value.indexOf(separator)
    if (index > 0) {
      const key = value.slice(0, index)
      const secret = value.slice(index + 1)
      if (secret.trim().length > 0 && isSensitiveName(key)) {
        return {
          value: `${key}${separator}[redacted]`,
          redactionClass: 'secret',
          redactNext: isAuthorizationScheme(trimWordPunctuation(secret).toLowerCase()),
        }
      }

      if (looksLikeRawLocalPath(secret.trim())) {
        return {
          value: `${key}${separator}[redacted-path]`,
          redactionClass: 'local_path',
          redactNext: false,
        }
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
    'anthropic_api_key',
    'authorization',
    'aws_access_key_id',
    'aws_secret_access_key',
    'aws_session_token',
    'auth_token',
    'bearer',
    'client_secret',
    'github_token',
    'google_oauth_access_token',
    'openai_api_key',
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

function isAuthorizationScheme(value: string): boolean {
  return ['bearer', 'basic', 'token'].includes(value)
}

function isSensitiveValueLabel(value: string): boolean {
  return [
    'access_token',
    'api_key',
    'apikey',
    'anthropic_api_key',
    'aws_access_key_id',
    'aws_secret_access_key',
    'aws_session_token',
    'auth_token',
    'client_secret',
    'github_token',
    'google_oauth_access_token',
    'openai_api_key',
    'password',
    'private_key',
    'refresh_token',
    'session_id',
    'session_token',
    'x_api_key',
  ].includes(value)
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
  const windows = value.replace(/\//g, '\\').toLowerCase()
  return (
    value.startsWith('/Users/') ||
    value.startsWith('/home/') ||
    value.startsWith('/var/folders/') ||
    value.startsWith('/tmp/') ||
    value.startsWith('~/') ||
    value.startsWith('\\Users\\') ||
    value.includes(':\\Users\\') ||
    windows.includes(':\\programdata\\') ||
    windows.includes(':\\windows\\temp\\') ||
    windows.startsWith('%appdata%\\') ||
    windows.startsWith('%localappdata%\\')
  )
}

function looksLikeSecretPathSegment(value: string): boolean {
  return looksLikeSecretToken(value) || (value.length >= 32 && /^[A-Za-z0-9]+$/.test(value))
}
