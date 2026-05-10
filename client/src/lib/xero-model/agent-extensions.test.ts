import { describe, expect, it } from 'vitest'

import {
  agentToolExtensionManifestValidationSchema,
  toolExtensionManifestSchema,
  validateAgentToolExtensionManifestRequestSchema,
} from './agent-extensions'

describe('agent extension model contracts', () => {
  it('parses extension manifests and backend validation reports without UI', () => {
    const manifest = toolExtensionManifestSchema.parse({
      contractVersion: 1,
      extensionId: 'demo_extension',
      toolName: 'demo_tool',
      label: 'Demo Tool',
      description: 'Runs a deterministic extension fixture.',
      inputSchema: {
        type: 'object',
        properties: {
          query: { type: 'string' },
        },
        required: ['query'],
      },
      permission: {
        permissionId: 'demo_extension_read',
        label: 'Demo extension read',
        effectClass: 'observe',
        riskClass: 'low',
        auditLabel: 'Demo extension read',
      },
      mutability: 'read_only',
      sandboxRequirement: 'read_only',
      approvalRequirement: 'policy',
      capabilityTags: ['demo', 'extension'],
      testFixtures: [
        {
          fixtureId: 'basic_read',
          input: { query: 'hello' },
          expectedSummaryContains: 'hello',
        },
      ],
    })

    const request = validateAgentToolExtensionManifestRequestSchema.parse({
      projectId: 'project-1',
      manifest,
    })
    const report = agentToolExtensionManifestValidationSchema.parse({
      schema: 'xero.agent_tool_extension_manifest_validation.v1',
      projectId: request.projectId,
      valid: true,
      extensionId: 'demo_extension',
      toolName: 'demo_tool',
      descriptor: {
        name: 'demo_tool',
        description: 'Runs a deterministic extension fixture.',
        inputSchema: manifest.inputSchema,
        capabilityTags: ['demo', 'extension'],
        effectClass: 'observe',
        mutability: 'read_only',
        sandboxRequirement: 'read_only',
        approvalRequirement: 'policy',
        telemetryAttributes: {
          'xero.extension.id': 'demo_extension',
          'xero.extension.permission_id': 'demo_extension_read',
        },
        resultTruncation: {
          maxOutputBytes: 65536,
          preserveJsonShape: false,
        },
      },
      permission: {
        permissionId: 'demo_extension_read',
        label: 'Demo extension read',
        effectClass: 'observe',
        riskClass: 'low',
        auditLabel: 'Demo extension read',
        mutability: 'read_only',
        sandboxRequirement: 'read_only',
        approvalRequirement: 'policy',
        capabilityTags: ['demo', 'extension'],
      },
      fixtureCount: 1,
      fixtureIds: ['basic_read'],
      diagnostics: [],
      uiDeferred: true,
    })

    expect(report.valid).toBe(true)
    expect(report.uiDeferred).toBe(true)
    expect(report.fixtureIds).toContain('basic_read')
    expect(report.descriptor?.telemetryAttributes['xero.extension.id']).toBe(
      'demo_extension',
    )
    const descriptor = report.descriptor
    expect(descriptor).not.toBeNull()
    if (!descriptor) {
      throw new Error('Expected validation report to include a descriptor.')
    }

    expect(
      toolExtensionManifestSchema.safeParse({
        ...manifest,
        testFixtures: [],
      }).success,
    ).toBe(false)
    expect(
      toolExtensionManifestSchema.safeParse({
        ...manifest,
        contractVersion: 2,
      }).success,
    ).toBe(false)
    expect(
      toolExtensionManifestSchema.safeParse({
        ...manifest,
        testFixtures: [
          ...manifest.testFixtures,
          {
            ...manifest.testFixtures[0],
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      toolExtensionManifestSchema.safeParse({
        ...manifest,
        testFixtures: [
          {
            ...manifest.testFixtures[0],
            expectedSummaryContains: ' ',
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      agentToolExtensionManifestValidationSchema.safeParse({
        ...report,
        fixtureCount: 2,
      }).success,
    ).toBe(false)
    expect(
      agentToolExtensionManifestValidationSchema.safeParse({
        ...report,
        descriptor: {
          ...descriptor,
          telemetryAttributes: {
            ...descriptor.telemetryAttributes,
            'xero.extension.permission_id': 'other_permission',
          },
        },
      }).success,
    ).toBe(false)
    expect(
      agentToolExtensionManifestValidationSchema.safeParse({
        ...report,
        descriptor: {
          ...descriptor,
          effectClass: 'workspace_mutation',
        },
      }).success,
    ).toBe(false)
    expect(
      agentToolExtensionManifestValidationSchema.safeParse({
        ...report,
        descriptor: {
          ...descriptor,
          capabilityTags: ['demo'],
        },
      }).success,
    ).toBe(false)

    const invalidReport = agentToolExtensionManifestValidationSchema.parse({
      schema: 'xero.agent_tool_extension_manifest_validation.v1',
      projectId: request.projectId,
      valid: false,
      fixtureCount: 0,
      fixtureIds: [],
      diagnostics: [
        {
          code: 'agent_tool_extension_fixture_missing',
          message:
            'Tool extension `demo_extension` must declare at least one executable test fixture.',
        },
      ],
      uiDeferred: true,
    })

    expect(invalidReport.valid).toBe(false)
    expect(invalidReport.diagnostics[0]?.code).toBe(
      'agent_tool_extension_fixture_missing',
    )
  })
})
