import { describe, expect, it } from 'vitest'
import {
  cadenceDiagnosticCheckSchema,
  cadenceDoctorReportSchema,
  createCadenceDiagnosticCheck,
  createCadenceDoctorReport,
  renderCadenceDoctorReport,
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
      subject: 'provider_profile',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'provider_profile_base_url_invalid',
      message: 'Provider profile has an invalid endpoint.',
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
      remediation: 'Enter a valid /v1 endpoint and resave the provider profile.',
    })

    const json = JSON.stringify(diagnostic)
    expect(diagnostic.redacted).toBe(true)
    expect(diagnostic.endpoint?.host).toBe('example.invalid')
    expect(json).not.toContain('sk-live-secret')
    expect(json).not.toContain('sk-other-secret')
    expect(json).toContain('redacted')
  })

  it('builds doctor reports with stable summaries, skipped checks, and copy-safe output modes', () => {
    const failed = createCadenceDiagnosticCheck({
      subject: 'settings_dependency',
      status: 'failed',
      severity: 'error',
      retryable: false,
      code: 'settings_secret_path_rejected',
      message: 'Settings read failed at /Users/sn0w/.cadence/secrets.json with token=sk-live-secret',
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
