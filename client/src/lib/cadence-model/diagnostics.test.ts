import { describe, expect, it } from 'vitest'
import {
  cadenceDiagnosticCheckSchema,
  cadenceDoctorReportSchema,
  checkProviderProfileRequestSchema,
  createCadenceDiagnosticCheck,
  createCadenceDoctorReport,
  providerProfileDiagnosticsSchema,
  renderCadenceDoctorReport,
  runDoctorReportRequestSchema,
  sanitizeDiagnosticText,
  summarizeDiagnosticChecks,
} from './diagnostics'

describe('diagnostics contract', () => {
  it('accepts strict diagnostic checks and rejects invalid severity/state combinations', () => {
    const passed = cadenceDiagnosticCheckSchema.parse({
      contractVersion: 1,
      checkId: 'diagnostic:v1:runtime_supervisor:global:global:runtime_ready',
      subject: 'runtime_supervisor',
      status: 'passed',
      severity: 'info',
      retryable: false,
      code: 'runtime_ready',
      message: 'Runtime supervisor binary is available.',
      affectedProfileId: null,
      affectedProviderId: null,
      endpoint: null,
      remediation: null,
      redactionClass: 'public',
      redacted: false,
    })

    expect(passed.status).toBe('passed')

    expect(() =>
      cadenceDiagnosticCheckSchema.parse({
        ...passed,
        status: 'passed',
        severity: 'error',
        retryable: true,
      }),
    ).toThrow(/severity `info`/)

    expect(() =>
      cadenceDiagnosticCheckSchema.parse({
        ...passed,
        status: 'failed',
        severity: 'error',
        remediation: null,
      }),
    ).toThrow(/remediation/)

    expect(() =>
      cadenceDiagnosticCheckSchema.parse({
        ...passed,
        redactionClass: 'secret',
        redacted: false,
      }),
    ).toThrow(/redacted=true/)
  })

  it('creates redacted diagnostics for provider repair details without leaking secrets', () => {
    const sanitized = sanitizeDiagnosticText(
      'Read failed at /Users/sn0w/.config/cadence/provider.json with Authorization: Bearer sk-live-secret',
    )
    expect(sanitized.redacted).toBe(true)
    expect(sanitized.value).not.toContain('/Users/sn0w')
    expect(sanitized.value).not.toContain('sk-live-secret')

    const diagnostic = createCadenceDiagnosticCheck({
      subject: 'provider_credential',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'provider_base_url_invalid',
      message: 'Provider has an invalid endpoint.',
      affectedProfileId: 'openai-compatible-work',
      affectedProviderId: 'openai_api',
      endpoint: {
        baseUrl: 'https://token:sk-live-secret@example.invalid/v1?api_key=sk-other-secret',
        host: null,
        apiVersion: null,
        region: null,
        projectId: null,
        modelListStrategy: null,
        redacted: false,
      },
      remediation: 'Enter a valid /v1 endpoint and resave the provider.',
    })

    const json = JSON.stringify(diagnostic)
    expect(diagnostic.redacted).toBe(true)
    expect(diagnostic.endpoint?.host).toBe('example.invalid')
    expect(json).not.toContain('sk-live-secret')
    expect(json).not.toContain('sk-other-secret')
    expect(json).toContain('redacted')
  })

  it('redacts opaque auth headers, cloud credential paths, and nested copied doctor payloads', () => {
    const authHeader = sanitizeDiagnosticText(
      'Provider returned Authorization: Bearer opaque-oauth-token-123 during refresh.',
    )
    expect(authHeader.redacted).toBe(true)
    expect(authHeader.redactionClass).toBe('secret')
    expect(authHeader.value).toContain('Authorization: Bearer [redacted]')
    expect(authHeader.value).not.toContain('opaque-oauth-token-123')

    const compactAuthHeader = sanitizeDiagnosticText(
      'Provider returned Authorization:Bearer compact-oauth-token-456 during refresh.',
    )
    expect(compactAuthHeader.redacted).toBe(true)
    expect(compactAuthHeader.value).not.toContain('compact-oauth-token-456')

    const cloudPaths = sanitizeDiagnosticText(
      'ADC failed at GOOGLE_APPLICATION_CREDENTIALS=/Users/sn0w/.config/gcloud/application_default_credentials.json and AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY.',
    )
    expect(cloudPaths.redacted).toBe(true)
    expect(cloudPaths.redactionClass).toBe('secret')
    expect(cloudPaths.value).not.toContain('/Users/sn0w')
    expect(cloudPaths.value).not.toContain('wJalrXUtnFEMI')
    expect(cloudPaths.value).toContain('[redacted-path]')

    const rawNested = {
      contractVersion: 1,
      checkId: 'diagnostic:v1:settings_dependency:global:global:nested_secret_payload',
      subject: 'settings_dependency',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'nested_secret_payload',
      message: 'Nested doctor payload included Authorization: Bearer opaque-nested-token-456 from /Users/sn0w/.aws/credentials.',
      affectedProfileId: null,
      affectedProviderId: null,
      endpoint: {
        baseUrl: 'http://local-user:local-pass@127.0.0.1:4000/v1?api_key=opaque-local-key',
        host: null,
        apiVersion: null,
        region: null,
        projectId: null,
        modelListStrategy: 'refresh_token=rt_opaque_nested_secret',
        redacted: false,
      },
      remediation: 'Remove session_id=sess_nested_secret before copying diagnostics.',
      redactionClass: 'public',
      redacted: false,
    } as const

    const copiedJson = renderCadenceDoctorReport({
      contractVersion: 1,
      reportId: 'doctor-20260426-privacy',
      generatedAt: '2026-04-26T12:00:00Z',
      mode: 'quick_local',
      versions: {
        appVersion: '0.1.0',
        runtimeSupervisorVersion: '0.1.0',
        runtimeProtocolVersion: 'diagnostics-v1',
      },
      summary: {
        passed: 0,
        warnings: 0,
        failed: 1,
        skipped: 0,
        total: 1,
        highestSeverity: 'error',
      },
      dictationChecks: [],
      profileChecks: [],
      modelCatalogChecks: [],
      runtimeSupervisorChecks: [],
      mcpDependencyChecks: [],
      settingsDependencyChecks: [rawNested],
    }, 'json')

    for (const leaked of [
      'opaque-nested-token-456',
      'local-pass',
      'opaque-local-key',
      'rt_opaque_nested_secret',
      'sess_nested_secret',
      '/Users/sn0w',
    ]) {
      expect(copiedJson).not.toContain(leaked)
    }
    expect(copiedJson).toContain('"redactionClass": "secret"')
    expect(copiedJson).toContain('"redacted": true')
  })

  it('accepts strict provider diagnostics and rejects cross-profile catalog payloads', () => {
    expect(runDoctorReportRequestSchema.parse({})).toEqual({ mode: 'quick_local' })
    expect(runDoctorReportRequestSchema.parse({ mode: 'extended_network' })).toEqual({
      mode: 'extended_network',
    })

    expect(checkProviderProfileRequestSchema.parse({
      profileId: ' openrouter-default ',
      includeNetwork: true,
    })).toEqual({
      profileId: 'openrouter-default',
      includeNetwork: true,
    })

    const validationCheck = createCadenceDiagnosticCheck({
      subject: 'provider_credential',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'provider_credentials_missing',
      message: 'OpenRouter is missing app-local credentials.',
      affectedProfileId: 'openrouter-default',
      affectedProviderId: 'openrouter',
      remediation: 'Add credentials for OpenRouter in Providers settings.',
    })
    const reachabilityCheck = createCadenceDiagnosticCheck({
      subject: 'model_catalog',
      status: 'warning',
      severity: 'warning',
      retryable: true,
      code: 'openrouter_rate_limited',
      message: 'OpenRouter rate limited model discovery.',
      affectedProfileId: 'openrouter-default',
      affectedProviderId: 'openrouter',
      remediation: 'Cadence is keeping the last successful model catalog visible.',
    })

    const diagnostics = providerProfileDiagnosticsSchema.parse({
      checkedAt: '2026-04-26T12:00:00Z',
      profileId: 'openrouter-default',
      providerId: 'openrouter',
      validationChecks: [validationCheck],
      reachabilityChecks: [reachabilityCheck],
      modelCatalog: {
        profileId: 'openrouter-default',
        providerId: 'openrouter',
        configuredModelId: 'openai/gpt-4.1-mini',
        source: 'cache',
        fetchedAt: '2026-04-26T12:00:00Z',
        lastSuccessAt: '2026-04-26T12:00:00Z',
        lastRefreshError: {
          code: 'openrouter_rate_limited',
          message: 'OpenRouter rate limited model discovery.',
          retryable: true,
        },
        models: [
          {
            modelId: 'openai/gpt-4.1-mini',
            displayName: 'OpenAI GPT-4.1 Mini',
            thinking: {
              supported: true,
              effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
              defaultEffort: 'medium',
            },
          },
        ],
      },
    })

    expect(diagnostics.validationChecks[0].code).toBe('provider_credentials_missing')
    expect(diagnostics.reachabilityChecks[0].code).toBe('openrouter_rate_limited')

    expect(() =>
      providerProfileDiagnosticsSchema.parse({
        ...diagnostics,
        modelCatalog: {
          ...diagnostics.modelCatalog,
          profileId: 'another-profile',
        },
      }),
    ).toThrow(/must belong/)
  })

  it('builds doctor reports with stable summaries, skipped checks, and copy-safe output modes', () => {
    const failed = createCadenceDiagnosticCheck({
      subject: 'settings_dependency',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'settings_secret_path_rejected',
      message:
        'Settings read failed at /Users/sn0w/Library/Application Support/dev.sn0w.cadence/secrets.json with token=sk-live-secret',
      remediation: 'Move the secret out of copied diagnostics and resave settings.',
    })
    const skipped = createCadenceDiagnosticCheck({
      subject: 'mcp_registry',
      status: 'skipped',
      severity: 'info',
      retryable: false,
      code: 'mcp_registry_not_configured',
      message: 'No MCP servers are configured.',
      remediation: 'Add an MCP server before running extended dependency checks.',
    })
    const passed = createCadenceDiagnosticCheck({
      subject: 'runtime_supervisor',
      status: 'passed',
      severity: 'info',
      retryable: false,
      code: 'runtime_supervisor_ready',
      message: 'Runtime supervisor binary is available.',
    })

    const report = createCadenceDoctorReport({
      reportId: 'doctor-20260426-120000',
      generatedAt: '2026-04-26T12:00:00Z',
      mode: 'quick_local',
      versions: {
        appVersion: '0.1.0',
        runtimeSupervisorVersion: '0.1.0',
        runtimeProtocolVersion: 'diagnostics-v1',
      },
      settingsDependencyChecks: [failed],
      mcpDependencyChecks: [skipped],
      runtimeSupervisorChecks: [passed],
    })

    expect(report.summary).toEqual({
      passed: 1,
      warnings: 0,
      failed: 1,
      skipped: 1,
      total: 3,
      highestSeverity: 'error',
    })
    expect(summarizeDiagnosticChecks([
      ...report.runtimeSupervisorChecks,
      ...report.mcpDependencyChecks,
      ...report.settingsDependencyChecks,
    ])).toEqual(report.summary)

    const json = renderCadenceDoctorReport(report, 'json')
    expect(json).toContain('"reportId": "doctor-20260426-120000"')
    expect(json).not.toContain('/Users/sn0w')
    expect(json).not.toContain('sk-live-secret')

    const human = renderCadenceDoctorReport(report, 'compact_human')
    expect(human).toContain('Summary: 1 passed, 0 warning(s), 1 failed, 1 skipped')
    expect(human).toContain('Runtime supervisor:')
    expect(human).not.toContain('sk-live-secret')

    expect(() =>
      cadenceDoctorReportSchema.parse({
        ...report,
        summary: {
          ...report.summary,
          total: 99,
        },
      }),
    ).toThrow(/summary counts/)
  })
})
