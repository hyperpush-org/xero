import { describe, expect, it } from 'vitest'

import {
  agentAuthoringCatalogSchema,
  agentConsumedArtifactSchema,
  agentDbTouchpointsSchema,
  agentOutputContractSchema,
  agentToolPolicyDetailsSchema,
  agentToolPackCatalogSchema,
  agentToolPackManifestSchema,
  getAgentToolPackCatalogRequestSchema,
  workflowAgentDetailSchema,
} from './workflow-agents'

const templateDefinition = {
  schema: 'xero.agent_definition.v1',
  schemaVersion: 1,
  id: 'engineering_patch_agent',
  displayName: 'Engineering Patch',
  shortLabel: 'Patch',
  description: 'Inspect, edit, verify, and summarize.',
  taskPurpose: 'Implement a focused code change.',
  scope: 'project_custom',
  lifecycleState: 'active',
  baseCapabilityProfile: 'engineering',
  defaultApprovalMode: 'suggest',
  allowedApprovalModes: ['suggest'],
  toolPolicy: {
    allowedTools: ['read'],
    deniedTools: [],
    allowedToolPacks: [],
    deniedToolPacks: [],
    allowedToolGroups: [],
    deniedToolGroups: [],
    allowedEffectClasses: ['observe'],
    externalServiceAllowed: false,
    browserControlAllowed: false,
    skillRuntimeAllowed: false,
    subagentAllowed: false,
    allowedSubagentRoles: [],
    deniedSubagentRoles: [],
    commandAllowed: false,
    destructiveWriteAllowed: false,
  },
  workflowContract: 'Inspect, patch, verify, and summarize the change.',
  finalResponseContract: 'Summarize changed files, verification, and residual risks.',
  examplePrompts: [
    'Fix a failing parser test.',
    'Patch a bug in retrieval.',
    'Refactor a small helper.',
  ],
  refusalEscalationCases: [
    'Refuse to bypass approval policy.',
    'Escalate missing repository context.',
    'Refuse to expose secrets.',
  ],
  prompts: [
    {
      id: 'engineering_patch_prompt',
      label: 'Engineering patch prompt',
      role: 'task',
      source: 'template',
      body: 'Implement focused repository changes and report verification.',
    },
  ],
  tools: [
    {
      name: 'read',
      group: 'core',
      description: 'Read project files.',
      effectClass: 'observe',
      riskClass: 'observe',
      tags: [],
      schemaFields: ['path'],
      examples: [],
    },
  ],
  output: {
    contract: 'engineering_summary',
    label: 'Engineering Summary',
    description: 'Implementation summary.',
    sections: [
      {
        id: 'changes',
        label: 'Changes',
        description: 'Changed files and behavior.',
        emphasis: 'core',
        producedByTools: ['read'],
      },
    ],
  },
  dbTouchpoints: { reads: [], writes: [], encouraged: [] },
  consumes: [],
  projectDataPolicy: { recordKinds: ['project_fact'] },
  memoryCandidatePolicy: { memoryKinds: ['project_fact'], reviewRequired: true },
  retrievalDefaults: {
    enabled: true,
    recordKinds: ['project_fact'],
    memoryKinds: ['project_fact'],
    limit: 6,
  },
  handoffPolicy: { enabled: true, preserveDefinitionVersion: true },
} as const

describe('workflow agent model contracts', () => {
  it('parses profile-aware authoring catalog availability metadata', () => {
    const catalog = agentAuthoringCatalogSchema.parse({
      tools: [
        {
          name: 'project_context_search',
          group: 'project_context',
          description: 'Search project context.',
          effectClass: 'runtime_state',
          riskClass: 'low',
          tags: ['context'],
          schemaFields: ['query'],
          examples: ['Search for recent decisions.'],
        },
      ],
      toolCategories: [
        {
          id: 'project_context',
          label: 'Project Context',
          description: 'Project-context tools.',
          tools: [
            {
              name: 'project_context_search',
              group: 'project_context',
              description: 'Search project context.',
              effectClass: 'runtime_state',
              riskClass: 'low',
              tags: ['context'],
              schemaFields: ['query'],
              examples: ['Search for recent decisions.'],
            },
          ],
        },
      ],
      dbTables: [
        {
          table: 'agent_context_manifests',
          purpose: 'Runtime context manifests.',
          columns: ['manifest_id'],
        },
      ],
      upstreamArtifacts: [
        {
          sourceAgent: 'plan',
          sourceAgentLabel: 'Plan',
          contract: 'plan_pack',
          contractLabel: 'Plan Pack',
          label: 'Plan output',
          description: 'Accepted plan output.',
          sections: [],
        },
      ],
      policyControls: [
        {
          id: 'retrieval.limit',
          kind: 'retrieval',
          label: 'Retrieval Limit',
          description: 'Maximum durable context records considered.',
          snapshotPath: 'retrievalDefaults.limit',
          valueKind: 'positive_integer',
          defaultValue: 6,
          runtimeEffect: 'Bounds first-turn working-set retrieval.',
          reviewRequired: false,
        },
        {
          id: 'memory.reviewRequired',
          kind: 'memory',
          label: 'Memory Review Required',
          description: 'Whether memory candidates need approval.',
          snapshotPath: 'memoryCandidatePolicy.reviewRequired',
          valueKind: 'boolean',
          defaultValue: true,
          runtimeEffect: 'Keeps memory writes in review until explicitly approved.',
          reviewRequired: true,
        },
      ],
      templates: [
        {
          id: 'engineering_patch',
          label: 'Engineering Patch',
          description: 'Inspect, edit, verify, and summarize.',
          taskKind: 'engineering',
          baseCapabilityProfile: 'engineering',
          definition: templateDefinition,
          examples: ['Fix a failing parser test.'],
        },
      ],
      creationFlows: [
        {
          id: 'start_from_engineering_task',
          label: 'Start From Engineering Task',
          description: 'Create a custom implementation agent.',
          entryKind: 'template',
          taskKind: 'engineering',
          templateIds: ['engineering_patch'],
          intentPrompt: 'Describe the implementation task.',
          expectedOutputContract: 'engineering_summary',
          baseCapabilityProfile: 'engineering',
        },
      ],
      profileAvailability: [
        {
          subjectKind: 'tool',
          subjectId: 'project_context_search',
          baseCapabilityProfile: 'observe_only',
          status: 'requires_profile_change',
          reason: 'tool requires the `engineering` base capability profile.',
          requiredProfile: 'engineering',
        },
        {
          subjectKind: 'tool',
          subjectId: 'project_context_search',
          baseCapabilityProfile: 'engineering',
          status: 'available',
          reason: 'tool is available for this base capability profile.',
        },
      ],
      constraintExplanations: [
        {
          id: 'tool:project_context_search:observe_only',
          subjectKind: 'tool',
          subjectId: 'project_context_search',
          baseCapabilityProfile: 'observe_only',
          status: 'requires_profile_change',
          message:
            'Tool `project_context_search` is not available on `observe_only` because that profile cannot safely run the required capability.',
          resolution:
            'Switch the agent base capability profile to `engineering` or remove `project_context_search` before saving.',
          requiredProfile: 'engineering',
          source: 'profileAvailability',
        },
      ],
    })

    expect(catalog.profileAvailability).toHaveLength(2)
    expect(catalog.profileAvailability[0]?.requiredProfile).toBe('engineering')
    expect(catalog.policyControls.map((control) => control.id)).toContain(
      'memory.reviewRequired',
    )
    expect(catalog.policyControls[0]?.snapshotPath).toBe('retrievalDefaults.limit')
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        policyControls: [
          {
            ...catalog.policyControls[0],
            defaultValue: true,
          },
        ],
      }),
    ).toThrow()
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        tools: [...catalog.tools, { ...catalog.tools[0]! }],
      }),
    ).toThrow(/tool names/)
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        toolCategories: [
          {
            ...catalog.toolCategories[0]!,
            tools: [
              {
                ...catalog.toolCategories[0]!.tools[0]!,
                name: 'missing_tool',
              },
            ],
          },
        ],
      }),
    ).toThrow(/category tools/)
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        policyControls: [
          ...catalog.policyControls,
          {
            ...catalog.policyControls[0]!,
            id: 'duplicate-path',
          },
        ],
      }),
    ).toThrow(/snapshot paths/)
    expect(catalog.templates[0]?.baseCapabilityProfile).toBe('engineering')
    expect(catalog.templates[0]?.definition.schema).toBe('xero.agent_definition.v1')
    expect(catalog.templates[0]?.definition.output.contract).toBe('engineering_summary')
    expect(catalog.templates[0]?.examples).toContain('Fix a failing parser test.')
    expect(catalog.creationFlows[0]?.templateIds).toContain('engineering_patch')
    expect(catalog.creationFlows[0]?.entryKind).toBe('template')
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        creationFlows: [
          {
            ...catalog.creationFlows[0],
            templateIds: ['missing_template'],
          },
        ],
      }),
    ).toThrow()
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        creationFlows: [
          {
            ...catalog.creationFlows[0],
            expectedOutputContract: 'debug_summary',
          },
        ],
      }),
    ).toThrow()
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        creationFlows: [
          {
            ...catalog.creationFlows[0],
            taskKind: 'debugging',
          },
        ],
      }),
    ).toThrow(/task kind/)
    expect(catalog.constraintExplanations[0]?.message).toContain('observe_only')
    expect(catalog.constraintExplanations[0]?.resolution).toContain('engineering')
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        profileAvailability: [
          {
            ...catalog.profileAvailability[0],
            requiredProfile: null,
          },
        ],
        constraintExplanations: [],
      }),
    ).toThrow()
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        profileAvailability: [...catalog.profileAvailability, { ...catalog.profileAvailability[0]! }],
        constraintExplanations: [],
      }),
    ).toThrow(/profile availability/)
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        constraintExplanations: [
          {
            ...catalog.constraintExplanations[0],
            subjectId: 'missing_tool',
          },
        ],
      }),
    ).toThrow()
    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        constraintExplanations: [
          ...catalog.constraintExplanations,
          {
            ...catalog.constraintExplanations[0]!,
            id: 'duplicate-constraint-id',
          },
        ],
      }),
    ).toThrow(/unique per subject/)

    expect(() =>
      agentAuthoringCatalogSchema.parse({
        ...catalog,
        templates: [
          {
            ...catalog.templates[0],
            definition: {
              ...templateDefinition,
              schemaVersion: 2,
            },
          },
        ],
      }),
    ).toThrow()
  })

  it('parses editable custom-agent authoring graph envelopes', () => {
    const detail = workflowAgentDetailSchema.parse({
      ref: {
        kind: 'custom',
        definitionId: 'engineering_patch_agent',
        version: 1,
      },
      header: {
        displayName: 'Engineering Patch',
        shortLabel: 'Patch',
        description: 'Inspect, edit, verify, and summarize.',
        taskPurpose: 'Implement a focused code change.',
        scope: 'project_custom',
        lifecycleState: 'active',
        baseCapabilityProfile: 'engineering',
        defaultApprovalMode: 'suggest',
        allowedApprovalModes: ['suggest'],
        allowPlanGate: true,
        allowVerificationGate: true,
        allowAutoCompact: true,
      },
      promptPolicy: 'engineer',
      toolPolicy: 'engineering',
      toolPolicyDetails: {
        allowedTools: ['read'],
        deniedTools: [],
        allowedToolPacks: [],
        deniedToolPacks: [],
        allowedToolGroups: [],
        deniedToolGroups: [],
        allowedEffectClasses: ['observe'],
        externalServiceAllowed: false,
        browserControlAllowed: false,
        skillRuntimeAllowed: false,
        subagentAllowed: false,
        allowedSubagentRoles: [],
        deniedSubagentRoles: [],
        commandAllowed: false,
        destructiveWriteAllowed: false,
      },
      prompts: [],
      tools: [],
      dbTouchpoints: {
        reads: [],
        writes: [],
        encouraged: [],
      },
      output: {
        contract: 'engineering_summary',
        label: 'Engineering Summary',
        description: 'Summarize implementation work.',
        sections: [],
      },
      consumes: [],
      authoringGraph: {
        schema: 'xero.agent_authoring_graph.v1',
        source: {
          kind: 'agent_definition_version',
          definitionId: 'engineering_patch_agent',
          version: 1,
          scope: 'project_custom',
          lifecycleState: 'active',
          baseCapabilityProfile: 'engineering',
          createdAt: '2026-05-09T12:00:00Z',
          generatedBy: 'agent_builder',
          uiDeferred: true,
        },
        editableFields: ['prompts', 'tools', 'toolPolicy'],
        canonicalGraph: {
          ...templateDefinition,
          version: 1,
        },
      },
    })

    expect(detail.authoringGraph?.source.generatedBy).toBe('agent_builder')
    expect(detail.authoringGraph?.canonicalGraph.output.contract).toBe(
      'engineering_summary',
    )
    expect(() =>
      workflowAgentDetailSchema.parse({
        ...detail,
        ref: {
          kind: 'custom',
          definitionId: 'engineering_patch_agent',
          version: 2,
        },
      }),
    ).toThrow(/detail ref/)
    expect(() =>
      workflowAgentDetailSchema.parse({
        ...detail,
        authoringGraph: {
          ...detail.authoringGraph,
          editableFields: ['prompts', 'prompts'],
        },
      }),
    ).toThrow(/editable fields/)
    expect(() =>
      workflowAgentDetailSchema.parse({
        ...detail,
        authoringGraph: {
          ...detail.authoringGraph,
          canonicalGraph: {
            ...detail.authoringGraph?.canonicalGraph,
            id: 'different_agent',
          },
        },
      }),
    ).toThrow(/definitionId/)
    expect(() =>
      workflowAgentDetailSchema.parse({
        ...detail,
        authoringGraph: {
          ...detail.authoringGraph,
          editableFields: ['prompts', 'notEditable'],
        },
      }),
    ).toThrow()
    expect(
      agentToolPolicyDetailsSchema.safeParse({
        ...detail.toolPolicyDetails!,
        allowedTools: ['read', 'read'],
      }).success,
    ).toBe(false)
    expect(
      agentToolPolicyDetailsSchema.safeParse({
        ...detail.toolPolicyDetails!,
        deniedToolPacks: [' '],
      }).success,
    ).toBe(false)
    expect(() =>
      agentDbTouchpointsSchema.parse({
        reads: [
          {
            table: 'agent_runtime_audit_events',
            kind: 'write',
            purpose: 'Read audit metadata.',
            triggers: [],
            columns: ['payload_json'],
          },
        ],
        writes: [],
        encouraged: [],
      }),
    ).toThrow(/reads entries/)
    expect(() =>
      agentDbTouchpointsSchema.parse({
        reads: [
          {
            table: 'agent_context_manifests',
            kind: 'read',
            purpose: 'Read manifest metadata.',
            triggers: [],
            columns: ['manifest_id', 'manifest_id'],
          },
        ],
        writes: [],
        encouraged: [],
      }),
    ).toThrow(/columns/)
    expect(() =>
      agentOutputContractSchema.parse({
        ...detail.output,
        sections: [
          {
            id: 'changes',
            label: 'Changes',
            description: 'Changed files and behavior.',
            emphasis: 'core',
            producedByTools: ['read'],
          },
          {
            id: 'changes',
            label: 'Duplicate',
            description: 'Duplicate section id.',
            emphasis: 'standard',
            producedByTools: ['read'],
          },
        ],
      }),
    ).toThrow(/section ids/)
    expect(() =>
      agentConsumedArtifactSchema.parse({
        id: 'plan-output',
        label: 'Plan Output',
        description: 'Accepted planning output.',
        sourceAgent: 'plan',
        contract: 'plan_pack',
        sections: ['goal', 'goal'],
        required: true,
      }),
    ).toThrow(/section ids/)
  })

  it('parses the backend-only tool-pack catalog contract', () => {
    const request = getAgentToolPackCatalogRequestSchema.parse({
      projectId: 'project-1',
    })
    const catalog = agentToolPackCatalogSchema.parse({
      schema: 'xero.agent_tool_pack_catalog.v1',
      projectId: request.projectId,
      uiDeferred: true,
      availablePackIds: ['project_context'],
      toolPacks: [
        {
          contractVersion: 1,
          packId: 'project_context',
          label: 'Project Context',
          summary: 'Read durable project context and approved memory.',
          policyProfile: 'runtime_state',
          toolGroups: ['project_context'],
          tools: ['project_context_search'],
          capabilities: ['durable_context'],
          allowedEffectClasses: ['runtime_state'],
          deniedEffectClasses: ['write'],
          reviewRequirements: [
            {
              requirementId: 'memory_review',
              label: 'Memory Review',
              description: 'Approved memory only.',
              required: true,
            },
          ],
          prerequisites: [
            {
              prerequisiteId: 'app_data_store',
              label: 'App Data Store',
              kind: 'storage',
              required: true,
              remediation: 'Open an imported project.',
            },
          ],
          healthChecks: [
            {
              checkId: 'app_data_store',
              label: 'App Data Store',
              description: 'Project app-data storage must be available.',
              prerequisiteIds: ['app_data_store'],
            },
          ],
          scenarioChecks: [
            {
              scenarioId: 'search_context',
              label: 'Search Context',
              description: 'Searches durable context without mutation.',
              toolNames: ['project_context_search'],
              mutating: false,
              requiresApproval: false,
            },
          ],
          uiAffordances: [
            {
              surface: 'builder',
              label: 'Project Context',
            },
          ],
          cliCommands: [],
          approvalBoundaries: ['Memory changes require review.'],
        },
      ],
      healthReports: [
        {
          contractVersion: 1,
          packId: 'project_context',
          label: 'Project Context',
          enabledByPolicy: true,
          status: 'passed',
          checkedAt: '2026-05-09T12:00:00Z',
          checks: [
            {
              checkId: 'app_data_store',
              label: 'App Data Store',
              status: 'passed',
              summary: 'Prerequisite is available.',
            },
          ],
          scenarioChecks: [
            {
              scenarioId: 'search_context',
              label: 'Search Context',
              status: 'passed',
              summary: 'Scenario can run.',
              toolNames: ['project_context_search'],
              mutating: false,
              requiresApproval: false,
            },
          ],
          missingPrerequisites: [],
        },
      ],
    })

    expect(catalog.schema).toBe('xero.agent_tool_pack_catalog.v1')
    expect(catalog.uiDeferred).toBe(true)
    expect(catalog.toolPacks[0]?.reviewRequirements[0]?.required).toBe(true)
    expect(catalog.healthReports[0]?.enabledByPolicy).toBe(true)

    expect(
      agentToolPackManifestSchema.safeParse({
        ...catalog.toolPacks[0],
        contractVersion: 2,
      }).success,
    ).toBe(false)
    expect(
      agentToolPackManifestSchema.safeParse({
        ...catalog.toolPacks[0],
        summary: ' ',
      }).success,
    ).toBe(false)
    expect(
      agentToolPackManifestSchema.safeParse({
        ...catalog.toolPacks[0],
        healthChecks: [
          {
            ...catalog.toolPacks[0]!.healthChecks[0]!,
            prerequisiteIds: ['missing-prerequisite'],
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      agentToolPackManifestSchema.safeParse({
        ...catalog.toolPacks[0],
        scenarioChecks: [
          {
            ...catalog.toolPacks[0]!.scenarioChecks[0]!,
            toolNames: ['missing_tool'],
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      agentToolPackCatalogSchema.safeParse({
        ...catalog,
        availablePackIds: ['missing_pack'],
      }).success,
    ).toBe(false)
    expect(
      agentToolPackCatalogSchema.safeParse({
        ...catalog,
        toolPacks: [
          ...catalog.toolPacks,
          {
            ...catalog.toolPacks[0]!,
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      agentToolPackCatalogSchema.safeParse({
        ...catalog,
        healthReports: [
          {
            ...catalog.healthReports[0]!,
            label: 'Different Label',
          },
        ],
      }).success,
    ).toBe(false)
    expect(
      agentToolPackCatalogSchema.safeParse({
        ...catalog,
        healthReports: [
          {
            ...catalog.healthReports[0]!,
            checks: [
              {
                ...catalog.healthReports[0]!.checks[0]!,
                checkId: 'missing_check',
              },
            ],
          },
        ],
      }).success,
    ).toBe(false)
  })
})
