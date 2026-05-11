import { describe, expect, it } from 'vitest'

import {
  AGENT_DEFINITION_SCHEMA,
  AGENT_DEFINITION_SCHEMA_VERSION,
  agentDefinitionPreviewResponseSchema,
  agentDefinitionVersionDiffSchema,
  agentDefinitionSummarySchema,
  canonicalCustomAgentDefinitionSchema,
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionVersionDiffRequestSchema,
  previewAgentDefinitionRequestSchema,
  saveAgentDefinitionRequestSchema,
  updateAgentDefinitionRequestSchema,
} from './agent-definition'

describe('agent definition contracts', () => {
  const canonicalDefinition = {
    schema: AGENT_DEFINITION_SCHEMA,
    schemaVersion: AGENT_DEFINITION_SCHEMA_VERSION,
    id: 'release_notes_helper',
    displayName: 'Release Notes Helper',
    shortLabel: 'Release',
    description: 'Draft release notes from reviewed project context.',
    taskPurpose: 'Retrieve project context and draft source-cited release notes.',
    scope: 'project_custom',
    lifecycleState: 'active',
    baseCapabilityProfile: 'observe_only',
    defaultApprovalMode: 'suggest',
    allowedApprovalModes: ['suggest'],
    toolPolicy: {
      allowedTools: ['read', 'project_context_search'],
      deniedTools: ['write'],
      allowedToolPacks: [],
      deniedToolPacks: [],
      allowedToolGroups: ['core'],
      deniedToolGroups: [],
      allowedEffectClasses: ['observe'],
      externalServiceAllowed: false,
      browserControlAllowed: false,
      skillRuntimeAllowed: false,
      subagentAllowed: false,
      commandAllowed: false,
      destructiveWriteAllowed: false,
    },
    workflowContract: 'Clarify range, retrieve reviewed context, draft notes.',
    workflowStructure: {
      startPhaseId: 'inspect',
      phases: [
        {
          id: 'inspect',
          title: 'Inspect',
          allowedTools: ['read', 'project_context_search', 'todo'],
          requiredChecks: [{ kind: 'todo_completed', todoId: 'inspect_done' }],
          retryLimit: 1,
          branches: [
            {
              targetPhaseId: 'draft',
              condition: { kind: 'todo_completed', todoId: 'inspect_done' },
            },
          ],
        },
        {
          id: 'draft',
          title: 'Draft',
          allowedTools: ['read'],
          requiredChecks: [{ kind: 'tool_succeeded', toolName: 'read', minCount: 1 }],
        },
      ],
    },
    finalResponseContract: 'Return changes, fixes, risks, and unknowns.',
    examplePrompts: ['Draft release notes.', 'Summarize fixes.', 'List risks.'],
    refusalEscalationCases: ['Refuse file edits.', 'Escalate missing context.', 'Refuse secrets.'],
    attachedSkills: [],
    prompts: [
      {
        id: 'system_prompt',
        label: 'System prompt',
        role: 'system',
        source: 'custom',
        body: 'Draft source-cited release notes.',
      },
    ],
    tools: [
      {
        name: 'read',
        group: 'core',
        description: 'Read files.',
        effectClass: 'observe',
        riskClass: 'observe',
        tags: [],
        schemaFields: ['path'],
        examples: [],
      },
    ],
    output: {
      contract: 'answer',
      label: 'Release answer',
      description: 'Release notes with sources.',
      sections: [
        {
          id: 'changes',
          label: 'Changes',
          description: 'User-visible changes.',
          emphasis: 'core',
          producedByTools: ['read'],
        },
      ],
    },
    dbTouchpoints: {
      reads: [
        {
          table: 'project_context_records',
          kind: 'read',
          purpose: 'Read approved facts.',
          triggers: [{ kind: 'tool', name: 'project_context_search' }],
          columns: ['record_id'],
        },
      ],
      writes: [],
      encouraged: [],
    },
    consumes: [
      {
        id: 'plan_pack',
        label: 'Plan Pack',
        description: 'Optional plan context.',
        sourceAgent: 'plan',
        contract: 'plan_pack',
        sections: ['decisions'],
        required: false,
      },
    ],
    projectDataPolicy: {
      recordKinds: ['project_fact'],
      structuredSchemas: ['xero.project_record.v1'],
    },
    memoryCandidatePolicy: {
      memoryKinds: ['project_fact'],
      reviewRequired: true,
    },
    retrievalDefaults: {
      enabled: true,
      recordKinds: ['project_fact'],
      memoryKinds: ['project_fact'],
      limit: 6,
    },
    handoffPolicy: {
      enabled: true,
      preserveDefinitionVersion: true,
    },
  } as const

  it('accepts the canonical custom-agent graph contract', () => {
    const parsed = canonicalCustomAgentDefinitionSchema.parse(canonicalDefinition)

    expect(parsed.schemaVersion).toBe(AGENT_DEFINITION_SCHEMA_VERSION)
    expect(parsed.tools[0]?.name).toBe('read')
    expect(parsed.dbTouchpoints.reads[0]?.table).toBe('project_context_records')
    expect(parsed.consumes[0]?.id).toBe('plan_pack')
    expect(parsed.workflowStructure?.phases[0]?.requiredChecks?.[0]?.kind).toBe(
      'todo_completed',
    )
    expect(
      saveAgentDefinitionRequestSchema.parse({
        projectId: 'project-agent-definition',
        definitionId: canonicalDefinition.id,
        definition: canonicalDefinition,
      }).definitionId,
    ).toBe(canonicalDefinition.id)
    expect(() =>
      saveAgentDefinitionRequestSchema.parse({
        projectId: 'project-agent-definition',
        definitionId: 'different_definition',
        definition: canonicalDefinition,
      }),
    ).toThrow(/canonical definition id/)
    expect(() =>
      updateAgentDefinitionRequestSchema.parse({
        projectId: 'project-agent-definition',
        definitionId: 'different_definition',
        definition: canonicalDefinition,
      }),
    ).toThrow(/canonical definition id/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        allowedApprovalModes: ['suggest', 'suggest'],
      }),
    ).toThrow(/approval modes/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        defaultApprovalMode: 'auto_edit',
      }),
    ).toThrow(/default approval mode/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        prompts: [...canonicalDefinition.prompts, { ...canonicalDefinition.prompts[0]! }],
      }),
    ).toThrow(/prompt ids/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        tools: [...canonicalDefinition.tools, { ...canonicalDefinition.tools[0]! }],
      }),
    ).toThrow(/tool names/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        toolPolicy: {
          ...canonicalDefinition.toolPolicy,
          subagentAllowed: true,
        },
      }),
    ).toThrow(/allowedSubagentRoles/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        dbTouchpoints: {
          ...canonicalDefinition.dbTouchpoints,
          reads: [
            {
              ...canonicalDefinition.dbTouchpoints.reads[0]!,
              kind: 'write',
            },
          ],
        },
      }),
    ).toThrow(/reads entries/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        output: {
          ...canonicalDefinition.output,
          sections: [
            ...canonicalDefinition.output.sections,
            {
              ...canonicalDefinition.output.sections[0]!,
            },
          ],
        },
      }),
    ).toThrow(/section ids/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        workflowStructure: {
          ...canonicalDefinition.workflowStructure,
          startPhaseId: 'missing-phase',
        },
      }),
    ).toThrow(/start phase/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        workflowStructure: {
          ...canonicalDefinition.workflowStructure,
          phases: [
            ...canonicalDefinition.workflowStructure.phases,
            {
              ...canonicalDefinition.workflowStructure.phases[0]!,
            },
          ],
        },
      }),
    ).toThrow(/phase ids/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        workflowStructure: {
          ...canonicalDefinition.workflowStructure,
          phases: [
            {
              ...canonicalDefinition.workflowStructure.phases[0]!,
              branches: [
                {
                  targetPhaseId: 'missing-phase',
                  condition: { kind: 'always' },
                },
              ],
            },
          ],
        },
      }),
    ).toThrow(/branch target/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        workflowStructure: {
          ...canonicalDefinition.workflowStructure,
          phases: [
            {
              ...canonicalDefinition.workflowStructure.phases[0]!,
              allowedTools: ['read', 'read'],
            },
          ],
        },
      }),
    ).toThrow(/allowed tools/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        attachedSkills: [
          {
            id: 'rust-guidance',
            sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices',
            skillId: 'rust-best-practices',
            name: 'Rust Best Practices',
            description: 'Guide for writing idiomatic Rust code.',
            sourceKind: 'bundled',
            scope: 'global',
            versionHash: 'sha256-rust-best-practices',
            includeSupportingAssets: false,
            required: true,
          },
          {
            id: 'rust-guidance',
            sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices-copy',
            skillId: 'rust-best-practices-copy',
            name: 'Rust Best Practices Copy',
            description: 'Guide for writing idiomatic Rust code.',
            sourceKind: 'bundled',
            scope: 'global',
            versionHash: 'sha256-rust-best-practices-copy',
            includeSupportingAssets: false,
            required: true,
          },
        ],
      }),
    ).toThrow(/attached skill ids/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        attachedSkills: [
          {
            id: 'rust-guidance',
            sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices',
            skillId: 'rust-best-practices',
            name: 'Rust Best Practices',
            description: 'Guide for writing idiomatic Rust code.',
            sourceKind: 'bundled',
            scope: 'global',
            versionHash: 'sha256-rust-best-practices',
            includeSupportingAssets: false,
            required: true,
          },
          {
            id: 'rust-guidance-2',
            sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices',
            skillId: 'rust-best-practices',
            name: 'Rust Best Practices',
            description: 'Guide for writing idiomatic Rust code.',
            sourceKind: 'bundled',
            scope: 'global',
            versionHash: 'sha256-rust-best-practices',
            includeSupportingAssets: true,
            required: true,
          },
        ],
      }),
    ).toThrow(/attached skill source ids/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        attachedSkills: undefined,
      }),
    ).toThrow()
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        attachedSkills: [
          {
            id: 'rust-guidance',
            sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices',
            skillId: 'rust-best-practices',
            name: 'Rust Best Practices',
            description: 'Guide for writing idiomatic Rust code.',
            sourceKind: 'bundled',
            scope: 'global',
            versionHash: 'sha256-rust-best-practices',
            includeSupportingAssets: false,
            required: false,
          },
        ],
      }),
    ).toThrow(/true/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        projectDataPolicy: {
          ...canonicalDefinition.projectDataPolicy,
          recordKinds: ['project_fact', 'project_fact'],
        },
      }),
    ).toThrow(/record kinds/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        memoryCandidatePolicy: {
          ...canonicalDefinition.memoryCandidatePolicy,
          memoryKinds: ['project_fact', 'project_fact'],
        },
      }),
    ).toThrow(/memory kinds/)
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        retrievalDefaults: {
          ...canonicalDefinition.retrievalDefaults,
          recordKinds: ['project_fact', 'project_fact'],
        },
      }),
    ).toThrow(/retrieval record kinds/)
  })

  it('accepts v3 built-in overlay references', () => {
    const parsed = canonicalCustomAgentDefinitionSchema.parse({
      ...canonicalDefinition,
      extends: 'engineer@1',
      baseCapabilityProfile: 'engineering',
    })

    expect(parsed.schemaVersion).toBe(AGENT_DEFINITION_SCHEMA_VERSION)
    expect(parsed.extends).toBe('engineer@1')
  })

  it('rejects missing and future custom-agent schema versions', () => {
    const missing = { ...canonicalDefinition }
    delete (missing as { schemaVersion?: number }).schemaVersion

    expect(() => canonicalCustomAgentDefinitionSchema.parse(missing)).toThrow()
    expect(() =>
      canonicalCustomAgentDefinitionSchema.parse({
        ...canonicalDefinition,
        schemaVersion: AGENT_DEFINITION_SCHEMA_VERSION + 1,
      }),
    ).toThrow()
  })

  it('accepts the built-in planning profile in registry summaries', () => {
    const summary = agentDefinitionSummarySchema.parse({
      definitionId: 'plan',
      currentVersion: 1,
      displayName: 'Plan',
      shortLabel: 'Plan',
      description: 'Draft accepted implementation plans without mutating repository files.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'planning',
      createdAt: '2026-05-06T00:00:00Z',
      updatedAt: '2026-05-06T00:00:00Z',
      isBuiltIn: true,
    })

    expect(summary.baseCapabilityProfile).toBe('planning')
    expect(getAgentDefinitionBaseCapabilityLabel('planning')).toBe('Planning')
  })

  it('accepts the built-in repository recon profile in registry summaries', () => {
    const summary = agentDefinitionSummarySchema.parse({
      definitionId: 'crawl',
      currentVersion: 1,
      displayName: 'Crawl',
      shortLabel: 'Crawl',
      description: 'Map an existing repository without editing files.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'repository_recon',
      createdAt: '2026-05-06T00:00:00Z',
      updatedAt: '2026-05-06T00:00:00Z',
      isBuiltIn: true,
    })

    expect(summary.baseCapabilityProfile).toBe('repository_recon')
    expect(getAgentDefinitionBaseCapabilityLabel('repository_recon')).toBe('Repository Recon')
  })

  it('accepts saved definition version diff contracts', () => {
    const request = getAgentDefinitionVersionDiffRequestSchema.parse({
      projectId: 'project-agent-definition',
      definitionId: 'release_notes_helper',
      fromVersion: 1,
      toVersion: 2,
    })
    expect(request.fromVersion).toBe(1)

    const diff = agentDefinitionVersionDiffSchema.parse({
      schema: 'xero.agent_definition_version_diff.v1',
      definitionId: request.definitionId,
      fromVersion: request.fromVersion,
      toVersion: request.toVersion,
      fromCreatedAt: '2026-05-01T12:00:00Z',
      toCreatedAt: '2026-05-01T12:05:00Z',
      changed: true,
      changedSections: ['prompts', 'attachedSkills', 'toolPolicy'],
      sections: [
        {
          section: 'prompts',
          changed: true,
          fields: ['prompts'],
          before: {
            prompts: [{ id: 'old' }],
          },
          after: {
            prompts: [{ id: 'new' }],
          },
        },
        {
          section: 'attachedSkills',
          changed: true,
          fields: ['attachedSkills'],
          before: {
            attachedSkills: [],
          },
          after: {
            attachedSkills: [
              {
                sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices',
              },
            ],
          },
        },
        {
          section: 'toolPolicy',
          changed: true,
          fields: ['toolPolicy', 'tools'],
          before: {
            toolPolicy: { allowedTools: ['read'] },
            tools: ['read'],
          },
          after: {
            toolPolicy: { allowedTools: ['read', 'project_context_search'] },
            tools: ['read', 'project_context_search'],
          },
        },
      ],
    })
    expect(diff.changedSections).toContain('toolPolicy')
    expect(() =>
      agentDefinitionVersionDiffSchema.parse({
        ...diff,
        changedSections: ['prompts', 'attachedSkills'],
      }),
    ).toThrow(/changedSections/)
    expect(() =>
      getAgentDefinitionVersionDiffRequestSchema.parse({
        ...request,
        toVersion: request.fromVersion,
      }),
    ).toThrow(/distinct/)
    expect(() =>
      agentDefinitionVersionDiffSchema.parse({
        ...diff,
        toVersion: diff.fromVersion,
      }),
    ).toThrow(/distinct/)
    expect(() =>
      agentDefinitionVersionDiffSchema.parse({
        ...diff,
        changedSections: ['prompts', 'prompts'],
      }),
    ).toThrow(/unique/)
    expect(() =>
      agentDefinitionVersionDiffSchema.parse({
        ...diff,
        sections: [
          ...diff.sections,
          {
            ...diff.sections[0]!,
          },
        ],
      }),
    ).toThrow(/sections/)
    expect(() =>
      agentDefinitionVersionDiffSchema.parse({
        ...diff,
        sections: [
          {
            ...diff.sections[0]!,
            fields: ['prompts', 'prompts'],
          },
          diff.sections[1]!,
          diff.sections[2]!,
        ],
      }),
    ).toThrow(/fields/)
  })

  it('accepts effective runtime preview command contracts', () => {
    const request = previewAgentDefinitionRequestSchema.parse({
      projectId: 'project-agent-definition',
      definitionId: 'release_notes_helper',
      definition: canonicalDefinition,
    })
    expect(request.definitionId).toBe('release_notes_helper')
    expect(() =>
      previewAgentDefinitionRequestSchema.parse({
        projectId: 'project-agent-definition',
        definitionId: 'different_definition',
        definition: canonicalDefinition,
      }),
    ).toThrow(/canonical definition id/)

    const response = agentDefinitionPreviewResponseSchema.parse({
      schema: 'xero.agent_definition_preview_command.v1',
      projectId: request.projectId,
      applied: false,
      message: 'Previewed effective runtime for agent definition `release_notes_helper` version 1.',
      definition: {
        definitionId: 'release_notes_helper',
        version: 1,
        displayName: 'Release Notes Helper',
        shortLabel: 'Release',
        description: 'Draft release notes from reviewed project context.',
        scope: 'project_custom',
        lifecycleState: 'active',
        baseCapabilityProfile: 'observe_only',
      },
      validation: {
        status: 'valid',
        diagnostics: [],
      },
      effectiveRuntimePreview: {
        schema: 'xero.agent_effective_runtime_preview.v1',
        schemaVersion: 1,
        source: {
          kind: 'normalized_agent_definition_snapshot',
          uiDeferred: true,
          uiDeferralReason:
            'The active implementation constraint forbids adding a new visible effective-runtime preview surface.',
        },
        definition: {
          definitionId: 'release_notes_helper',
          version: 1,
          displayName: 'Release Notes Helper',
          scope: 'project_custom',
          lifecycleState: 'active',
          baseCapabilityProfile: 'observe_only',
          runtimeAgentId: 'ask',
        },
        validation: { status: 'valid' },
        prompt: {
          compiler: 'PromptCompiler',
          selectionMode: 'capability_ceiling_without_task_prompt',
          promptSha256: 'a'.repeat(64),
          promptBudgetTokens: 8000,
          estimatedPromptTokens: 120,
          fragmentCount: 1,
          fragmentIds: ['xero.soul'],
          fragments: [
            {
              id: 'xero.soul',
              priority: 1000,
              title: 'Xero Soul',
              provenance: 'runtime',
              budgetPolicy: 'always',
              inclusionReason: 'base runtime policy',
              content: 'Follow system and developer policy.',
              sha256: 'b'.repeat(64),
              tokenEstimate: 24,
            },
          ],
        },
        graphValidation: {
          schema: 'xero.agent_graph_validation_summary.v1',
          status: 'valid',
          diagnosticCount: 0,
          categories: [
            {
              category: 'unavailable_tools',
              count: 0,
              diagnostics: [],
            },
          ],
        },
        graphRepairHints: {
          schema: 'xero.agent_graph_repair_hints.v1',
          supported: [
            {
              kind: 'tool',
              capabilityId: 'read',
              status: 'supported',
              note: 'Tool `read` is available in the effective runtime graph.',
            },
          ],
          partiallySupported: [],
          unsupported: [],
        },
        attachedSkillInjection: {
          schema: 'xero.agent_attached_skill_injection_preview.v1',
          schemaVersion: 1,
          selectionMode: 'definition_attached_skills_without_skill_tool',
          status: 'resolved',
          skillToolRequired: false,
          attachmentCount: 0,
          resolvedCount: 0,
          staleCount: 0,
          unavailableCount: 0,
          blockedCount: 0,
          entries: [],
        },
        effectiveToolAccess: {
          selectionMode: 'capability_ceiling_without_task_prompt',
          skillToolEnabled: false,
          runtimeAgentId: 'ask',
          requestedTools: ['read'],
          requestedEffectClasses: ['observe'],
          explicitlyDeniedTools: [],
          allowedToolCount: 1,
          deniedCapabilityCount: 0,
          allowedTools: [
            {
              toolName: 'read',
              group: 'filesystem',
              description: 'Read files.',
              riskClass: 'low',
              effectClass: 'observe',
              tags: ['read'],
              schemaFields: ['path'],
              runtimeProfileAllowed: true,
              customPolicyAllowed: true,
              hostAvailable: true,
              effectiveAllowed: true,
              deniedBy: [],
            },
          ],
          deniedCapabilities: [],
        },
        capabilityPermissionExplanations: [
          {
            schema: 'xero.capability_permission_explanation.v1',
            subjectKind: 'custom_agent',
            subjectId: 'release_notes_helper',
            summary:
              'Custom agent definition can select prompts, policies, tool packs, memory, retrieval, and handoff behavior for new runs.',
            dataAccess:
              'project runtime state, selected project context, approved memory, and any tools granted by its effective policy',
            networkAccess: 'depends_on_effective_tool_policy',
            fileMutation: 'depends_on_effective_tool_policy',
            confirmationRequired: true,
            riskClass: 'custom_agent_runtime',
          },
        ],
        policies: {
          toolPolicy: canonicalDefinition.toolPolicy,
          outputContract: canonicalDefinition.output,
          contextPolicy: canonicalDefinition.projectDataPolicy,
          memoryPolicy: canonicalDefinition.memoryCandidatePolicy,
          retrievalPolicy: canonicalDefinition.retrievalDefaults,
          handoffPolicy: canonicalDefinition.handoffPolicy,
          attachedSkills: canonicalDefinition.attachedSkills,
          workflowContract: canonicalDefinition.workflowContract,
          workflowStructure: canonicalDefinition.workflowStructure,
          finalResponseContract: canonicalDefinition.finalResponseContract,
        },
        riskyCapabilityPrompts: [],
        runtimeConsistency: {
          toolPolicySource: 'AutonomousAgentToolPolicy::from_definition_snapshot',
          toolRegistrySource: 'ToolRegistry::builtin_with_options',
          promptCompilerSource: 'PromptCompiler::with_agent_definition_snapshot',
          taskPromptNarrowing: 'not_applied_in_preview',
        },
      },
      uiDeferred: true,
    })
    expect(response.effectiveRuntimePreview.source.uiDeferred).toBe(true)
    expect(response.effectiveRuntimePreview.graphRepairHints).toMatchObject({
      schema: 'xero.agent_graph_repair_hints.v1',
    })
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        effectiveRuntimePreview: {
          ...response.effectiveRuntimePreview,
          effectiveToolAccess: {
            ...response.effectiveRuntimePreview.effectiveToolAccess,
            allowedToolCount: 2,
          },
        },
      }),
    ).toThrow(/allowedToolCount/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        effectiveRuntimePreview: {
          ...response.effectiveRuntimePreview,
          graphRepairHints: {
            ...response.effectiveRuntimePreview.graphRepairHints,
            supported: [
              {
                ...response.effectiveRuntimePreview.graphRepairHints.supported[0]!,
                status: 'unsupported',
              },
            ],
          },
        },
      }),
    ).toThrow(/status/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        effectiveRuntimePreview: {
          ...response.effectiveRuntimePreview,
          effectiveToolAccess: {
            ...response.effectiveRuntimePreview.effectiveToolAccess,
            allowedTools: [
              {
                ...response.effectiveRuntimePreview.effectiveToolAccess.allowedTools[0]!,
                effectiveAllowed: false,
              },
            ],
          },
        },
      }),
    ).toThrow(/effectively allowed/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        definition: {
          ...response.definition!,
          definitionId: 'different_definition',
        },
      }),
    ).toThrow(/summary/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        validation: {
          status: 'invalid',
          diagnostics: [],
        },
      }),
    ).toThrow(/Invalid/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        effectiveRuntimePreview: {
          ...response.effectiveRuntimePreview,
          graphValidation: {
            ...response.effectiveRuntimePreview.graphValidation,
            status: 'invalid',
            diagnosticCount: 0,
          },
        },
      }),
    ).toThrow(/Invalid graph/)
    expect(() =>
      agentDefinitionPreviewResponseSchema.parse({
        ...response,
        effectiveRuntimePreview: {
          ...response.effectiveRuntimePreview,
          policies: {
            ...response.effectiveRuntimePreview.policies,
            contextPolicy: 'not a backend policy object',
          },
        },
      }),
    ).toThrow()
  })
})
