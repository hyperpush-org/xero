import { describe, expect, it } from 'vitest'
import {
  agentHandoffContextSummarySchema,
  agentDatabaseTouchpointExplanationSchema,
  agentKnowledgeInspectionSchema,
  agentRunStartExplanationSchema,
  agentSupportDiagnosticsBundleSchema,
  agentSupportRuntimeAuditSchema,
  capabilityPermissionExplanationSchema,
  getAgentHandoffContextSummaryRequestSchema,
  getAgentDatabaseTouchpointExplanationRequestSchema,
  getAgentKnowledgeInspectionRequestSchema,
  getAgentRunStartExplanationRequestSchema,
  getAgentSupportDiagnosticsBundleRequestSchema,
  getCapabilityPermissionExplanationRequestSchema,
} from './agent-reports'

const projectId = 'project-agent-reports'
const runId = 'run-agent-report'
const handoffId = 'handoff-agent-report'
const createdAt = '2026-05-09T10:15:00Z'
const freshnessCounts = {
  inspectedRowCount: 4,
  currentRowCount: 3,
  sourceUnknownRowCount: 0,
  staleRowCount: 0,
  sourceMissingRowCount: 0,
  supersededRowCount: 1,
  blockedRowCount: 0,
  retrievalDegradedRowCount: 1,
}

const lanceHealth = (tableName: string) => ({
  tableName,
  status: 'healthy',
  schemaCurrent: true,
  version: 1,
  rowCount: 4,
  totalBytes: 4096,
  indexCount: 1,
  fragmentCount: 1,
  smallFragmentCount: 0,
  statsLatencyMs: 12,
  maintenanceRecommended: false,
  quarantineTableCount: 0,
  diagnosticMarkerCount: 0,
  freshness: freshnessCounts,
})

describe('agent report command contracts', () => {
  it('validates deferred report requests and redaction-safe responses', () => {
    expect(getAgentRunStartExplanationRequestSchema.parse({ projectId, runId }).runId).toBe(runId)
    expect(
      getAgentKnowledgeInspectionRequestSchema.parse({
        projectId,
        agentSessionId: 'agent-session-main',
        runId,
        limit: 25,
      }).limit,
    ).toBe(25)
    expect(getAgentHandoffContextSummaryRequestSchema.parse({ projectId, handoffId }).handoffId).toBe(handoffId)
    expect(getAgentSupportDiagnosticsBundleRequestSchema.parse({ projectId, runId }).runId).toBe(runId)
    expect(
      getCapabilityPermissionExplanationRequestSchema.parse({
        subjectKind: 'tool_pack',
        subjectId: 'project_context_tools',
      }).subjectKind,
    ).toBe('tool_pack')
    expect(
      getAgentDatabaseTouchpointExplanationRequestSchema.parse({
        projectId,
        definitionId: 'custom-agent',
        version: 3,
      }).definitionId,
    ).toBe('custom-agent')

    const permission = capabilityPermissionExplanationSchema.parse({
      schema: 'xero.capability_permission_explanation.v1',
      subjectKind: 'tool_pack',
      subjectId: 'project_context_tools',
      summary: 'Tool pack grants expose a group of runtime tools to an agent.',
      dataAccess: 'data reachable by tools inside the pack',
      networkAccess: 'depends_on_tools_in_pack',
      fileMutation: 'depends_on_tools_in_pack',
      confirmationRequired: true,
      riskClass: 'tool_pack_grant',
      toolPack: {
        packId: 'project_context_tools',
        label: 'Project context tools',
        policyProfile: 'project_context',
        tools: ['project_context_get'],
        capabilities: ['project_context_read'],
        allowedEffectClasses: ['observe', 'project_context_read'],
        deniedEffectClasses: ['network'],
        reviewRequirements: [
          {
            requirementId: 'context_review',
            label: 'Review project context',
            description: 'Project context reads should stay source-cited.',
            required: true,
          },
        ],
        approvalBoundaries: ['Cannot mutate project state.'],
      },
    })
    expect(permission.confirmationRequired).toBe(true)
    expect(() =>
      capabilityPermissionExplanationSchema.parse({
        ...permission,
        subjectId: 'other_pack',
      }),
    ).toThrow(/packId/)
    expect(() =>
      capabilityPermissionExplanationSchema.parse({
        ...permission,
        subjectKind: 'custom_agent',
      }),
    ).toThrow(/Only tool-pack/)
    expect(() =>
      capabilityPermissionExplanationSchema.parse({
        ...permission,
        toolPack: undefined,
      }),
    ).toThrow(/tool-pack metadata/)
    expect(() =>
      capabilityPermissionExplanationSchema.parse({
        ...permission,
        toolPack: {
          ...permission.toolPack!,
          tools: ['project_context_get', 'project_context_get'],
        },
      }),
    ).toThrow(/tools/)
    expect(() =>
      capabilityPermissionExplanationSchema.parse({
        ...permission,
        toolPack: {
          ...permission.toolPack!,
          deniedEffectClasses: ['observe'],
        },
      }),
    ).toThrow(/both allowed and denied/)

    const touchpoints = agentDatabaseTouchpointExplanationSchema.parse({
      schema: 'xero.agent_database_touchpoint_explanation.v1',
      projectId,
      definition: {
        definitionId: 'custom-agent',
        version: 3,
      },
      summary: {
        readCount: 1,
        writeCount: 1,
        encouragedCount: 0,
        hasWrites: true,
      },
      touchpoints: {
        reads: [
          {
            table: 'project_records',
            kind: 'read',
            purpose: 'Read reviewed project state.',
            columns: ['text'],
            triggerCount: 0,
          },
        ],
        writes: [
          {
            table: 'agent_runtime_audit_events',
            kind: 'write',
            purpose: 'Record audit metadata.',
            columns: ['payload_json'],
            triggerCount: 1,
          },
        ],
        encouraged: [],
      },
      explanation: {
        readBehavior: 'Read touchpoints guide context selection.',
        writeBehavior: 'Write touchpoints identify project-state tables.',
        encouragedBehavior: 'Encouraged touchpoints are hints.',
        auditVisibility: 'Runtime audit records carry the compact summary.',
        userConfirmation: 'Mutating tools still require effective permissions.',
      },
      source: {
        kind: 'agent_definition_snapshot',
        path: 'dbTouchpoints',
      },
      uiDeferred: true,
    })
    expect(touchpoints.summary.hasWrites).toBe(true)
    expect(() =>
      agentDatabaseTouchpointExplanationSchema.parse({
        ...touchpoints,
        summary: {
          ...touchpoints.summary,
          writeCount: 0,
        },
      }),
    ).toThrow(/writeCount/)
    expect(() =>
      agentDatabaseTouchpointExplanationSchema.parse({
        ...touchpoints,
        summary: {
          ...touchpoints.summary,
          hasWrites: false,
        },
      }),
    ).toThrow(/hasWrites/)

    const runStart = agentRunStartExplanationSchema.parse({
      schema: 'xero.agent_run_start_explanation.v1',
      projectId,
      runId,
      definition: {
        runtimeAgentId: 'engineer',
        definitionId: 'custom-agent',
        version: 3,
      },
      model: {
        providerId: 'openrouter',
        modelId: 'openai/gpt-5.4',
      },
      approval: {
        defaultMode: 'suggest',
        allowedModes: ['suggest'],
        source: 'agent_definition_snapshot',
      },
      contextPolicy: { status: 'recorded' },
      toolPolicy: { allowedTools: ['project_context_get'] },
      memoryPolicy: { reviewRequired: true },
      retrievalPolicy: { searchScope: 'hybrid_context' },
      outputContract: { kind: 'patch' },
      handoffPolicy: { status: 'recorded' },
      databaseTouchpointExplanation: {
        schema: 'xero.agent_database_touchpoint_explanation.v1',
        projectId,
        definition: {
          definitionId: 'custom-agent',
          version: 3,
        },
        summary: {
          readCount: 1,
          writeCount: 0,
          encouragedCount: 0,
          hasWrites: false,
        },
        touchpoints: {
          reads: [
            {
              table: 'project_context_records',
              kind: 'read',
              purpose: 'Read reviewed context.',
              columns: ['record_id'],
              triggerCount: 1,
            },
          ],
          writes: [],
          encouraged: [],
        },
        explanation: {
          readBehavior: 'Read touchpoints guide context selection.',
          writeBehavior: 'This saved agent definition declares no write touchpoints.',
          encouragedBehavior: 'Encouraged touchpoints are hints.',
          auditVisibility: 'Manifests and audit records carry compact touchpoints.',
          userConfirmation: 'Touchpoints do not bypass tool policy.',
        },
        source: {
          kind: 'agent_definition_snapshot',
          path: 'dbTouchpoints',
        },
        uiDeferred: true,
      },
      capabilityPermissionExplanations: [
        {
          schema: 'xero.capability_permission_explanation.v1',
          subjectKind: 'custom_agent',
          subjectId: 'custom-agent',
          summary: 'Custom agent definition can select runtime policy.',
          dataAccess: 'project runtime state',
          networkAccess: 'depends_on_effective_tool_policy',
          fileMutation: 'depends_on_effective_tool_policy',
          confirmationRequired: true,
          riskClass: 'custom_agent_runtime',
        },
      ],
      riskyCapabilityApprovalCount: 0,
      source: {
        kind: 'runtime_audit_export',
        agentSessionId: 'agent-session-main',
        contextManifestIds: ['manifest-1'],
      },
    })
    expect(runStart.capabilityPermissionExplanations).toHaveLength(1)
    expect(runStart.databaseTouchpointExplanation?.summary.readCount).toBe(1)
    expect(() =>
      agentRunStartExplanationSchema.parse({
        ...runStart,
        contextPolicy: 'not a backend policy object',
      }),
    ).toThrow()
    expect(() =>
      agentRunStartExplanationSchema.parse({
        ...runStart,
        approval: {
          ...runStart.approval,
          defaultMode: 'auto',
        },
      }),
    ).toThrow(/default mode/)
    expect(() =>
      agentRunStartExplanationSchema.parse({
        ...runStart,
        approval: {
          ...runStart.approval,
          allowedModes: ['suggest', 'suggest'],
        },
      }),
    ).toThrow(/allowed modes/)
    expect(() =>
      agentRunStartExplanationSchema.parse({
        ...runStart,
        databaseTouchpointExplanation: {
          ...runStart.databaseTouchpointExplanation!,
          projectId: 'different-project',
        },
      }),
    ).toThrow(/same project/)
    expect(() =>
      agentRunStartExplanationSchema.parse({
        ...runStart,
        databaseTouchpointExplanation: {
          ...runStart.databaseTouchpointExplanation!,
          definition: {
            ...runStart.databaseTouchpointExplanation!.definition,
            version: 4,
          },
        },
      }),
    ).toThrow(/pinned definition version/)

    const knowledge = agentKnowledgeInspectionSchema.parse({
      schema: 'xero.agent_knowledge_inspection.v1',
      projectId,
      agentSessionId: 'agent-session-main',
      runId,
      limit: 25,
      retrievalPolicy: {
        source: 'runtime_audit_export',
        policy: {
          deliveryModel: 'tool_mediated',
          recordKinds: ['project_fact', 'context_note'],
          memoryKinds: ['decision'],
        },
        recordKindFilter: ['context_note', 'project_fact'],
        memoryKindFilter: ['decision'],
        filtersApplied: true,
      },
      projectRecords: [
        {
          recordId: 'record-project-fact',
          recordKind: 'project_fact',
          title: 'Project fact',
          summary: 'Uses app-data project state.',
          textPreview: 'Persist new project state under the OS app-data directory.',
          schemaName: null,
          importance: 'high',
          confidence: 0.94,
          tags: ['storage'],
          relatedPaths: ['client/src-tauri/src/db/project_store'],
          freshnessState: 'current',
          redactionState: 'clean',
          sourceItemIds: ['manifest-1'],
          updatedAt: createdAt,
        },
      ],
      continuityRecords: [
        {
          recordId: 'record-continuity',
          recordKind: 'context_note',
          title: 'Current problem continuity',
          summary: null,
          textPreview: null,
          schemaName: 'xero.project_record.current_problem_continuity.v1',
          importance: 'normal',
          confidence: null,
          tags: ['handoff'],
          relatedPaths: [],
          freshnessState: 'current',
          redactionState: 'redacted',
          sourceItemIds: ['continuity-1'],
          updatedAt: createdAt,
        },
      ],
      approvedMemory: [
        {
          memoryId: 'memory-decision',
          scope: 'session',
          kind: 'decision',
          textPreview: 'Keep UI work deferred until the end.',
          confidence: 93,
          sourceRunId: runId,
          sourceItemIds: ['memory-source-1'],
          freshnessState: 'current',
          updatedAt: createdAt,
        },
      ],
      handoffRecords: [
        {
          handoffId,
          status: 'completed',
          sourceRunId: runId,
          targetRunId: 'run-target',
          runtimeAgentId: 'engineer',
          agentDefinitionId: 'custom-agent',
          agentDefinitionVersion: 3,
          providerId: 'openrouter',
          modelId: 'openai/gpt-5.4',
          handoffRecordId: 'record-handoff',
          bundleKeys: ['userGoal', 'pendingWork'],
          createdAt,
          updatedAt: createdAt,
        },
      ],
      redaction: {
        rawBlockedRecordsExcluded: true,
        redactedProjectRecordTextHidden: true,
        handoffBundleRawPayloadHidden: true,
      },
    })
    expect(knowledge.redaction.rawBlockedRecordsExcluded).toBe(true)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        projectRecords: [
          {
            ...knowledge.projectRecords[0],
            redactionState: 'blocked',
          },
        ],
      }),
    ).toThrow(/blocked/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        continuityRecords: [
          {
            ...knowledge.continuityRecords[0],
            textPreview: 'leaked redacted text',
          },
        ],
      }),
    ).toThrow(/redacted/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        limit: 1,
        projectRecords: [
          knowledge.projectRecords[0],
          {
            ...knowledge.projectRecords[0],
            recordId: 'record-project-fact-extra',
          },
        ],
      }),
    ).toThrow(/limit/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        runId: null,
      }),
    ).toThrow(/runId/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        projectRecords: [
          {
            ...knowledge.projectRecords[0],
            recordKind: 'unfiltered_kind',
          },
        ],
      }),
    ).toThrow(/retrieval policy filters/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        approvedMemory: [
          {
            ...knowledge.approvedMemory[0],
            kind: 'project_fact',
          },
        ],
      }),
    ).toThrow(/retrieval policy filters/)
    expect(() =>
      agentKnowledgeInspectionSchema.parse({
        ...knowledge,
        handoffRecords: [knowledge.handoffRecords[0], { ...knowledge.handoffRecords[0] }],
      }),
    ).toThrow(/unique/)

    const handoff = agentHandoffContextSummarySchema.parse({
      schema: 'xero.agent_handoff_context_summary.v1',
      projectId,
      handoffId,
      status: 'completed',
      source: {
        agentSessionId: 'agent-session-main',
        runId,
        runtimeAgentId: 'engineer',
        agentDefinitionId: 'custom-agent',
        agentDefinitionVersion: 3,
        contextHash: 'context-hash-agent-report',
      },
      target: {
        agentSessionId: 'agent-session-target',
        runId: 'run-target',
        runtimeAgentId: 'engineer',
        agentDefinitionId: 'custom-agent',
        agentDefinitionVersion: 3,
      },
      provider: { providerId: 'openrouter', modelId: 'openai/gpt-5.4' },
      carriedContext: {
        userGoal: 'Finish the agent-system plan.',
        currentTask: 'Tighten backend report contracts.',
        currentStatus: 'in_progress',
        completedWork: [
          {
            messageId: 12,
            createdAt,
            summary: 'S61 support diagnostics contract tightened.',
          },
        ],
        pendingWork: [
          {
            kind: 'user_prompt',
            text: 'Visible handoff notice remains deferred.',
          },
        ],
        activeTodoItems: [
          {
            id: 'todo-1',
            status: 'pending',
            text: 'Keep UI work deferred.',
          },
        ],
        importantDecisions: [
          {
            kind: 'decision',
            eventId: 22,
            eventKind: 'PlanUpdated',
            createdAt,
            summary: 'No new UI while the active constraint is in force.',
          },
        ],
        constraints: ['Use app-data storage for new project state.'],
        durableContext: {
          deliveryModel: 'tool_mediated',
          toolName: 'project_context',
          rawContextInjected: false,
          sourceContextHash: 'context-hash-agent-report',
          instruction: 'Use project_context for exact durable context.',
        },
        workingSetSummary: {
          schema: 'xero.agent_handoff.working_set.v1',
          sourceRunId: runId,
          sourceContextHash: 'context-hash-agent-report',
          activeTodoCount: 1,
          recentFileChangeCount: 1,
          latestChangedPaths: ['client/src/lib/xero-model/agent-reports.ts'],
          assistantMessageIds: [12],
        },
        sourceCitedContinuityRecords: [
          {
            sourceKind: 'agent_message',
            sourceId: 12,
            createdAt,
            summary: 'S61 support diagnostics contract tightened.',
          },
        ],
        recentFileChanges: [
          {
            path: 'client/src/lib/xero-model/agent-reports.ts',
            operation: 'modified',
            oldHash: null,
            newHash: 'new-hash',
            createdAt,
          },
        ],
        toolAndCommandEvidence: [
          {
            toolCallId: 'tool-call-1',
            toolName: 'pnpm',
            state: 'Succeeded',
            inputPreview: 'vitest run src/lib/xero-model/agent-reports.test.ts',
            error: null,
          },
        ],
        verificationStatus: {
          status: 'recorded',
          evidence: [
            {
              kind: 'verification',
              eventId: 23,
              eventKind: 'VerificationGate',
              createdAt,
              summary: 'Focused frontend contract tests passed.',
            },
          ],
        },
        knownRisks: [
          {
            kind: 'risk',
            eventId: 24,
            eventKind: 'PlanUpdated',
            createdAt,
            summary: 'Rust verification deferred while disk is critically low.',
          },
        ],
        openQuestions: [],
        approvedMemories: [],
        relevantProjectRecords: [],
        agentSpecific: {},
      },
      omittedContext: [
        {
          kind: 'raw_bundle_payload',
          status: 'hidden',
          reason: 'This summary exposes only whitelisted carried-context fields.',
        },
      ],
      redaction: {
        state: 'clean',
        bundleRedactionCount: 0,
        summaryRedactionApplied: false,
        rawPayloadHidden: true,
      },
      safetyRationale: {
        sameRuntimeAgent: true,
        sameDefinitionVersion: true,
        sourceContextHashPresent: true,
        targetRunCreated: true,
        handoffRecordPersisted: true,
        reasons: ['Target receives current higher-priority policy separately.'],
      },
      createdAt,
      updatedAt: createdAt,
      completedAt: null,
      uiDeferred: true,
    })
    expect(handoff.redaction.rawPayloadHidden).toBe(true)
    expect(() =>
      agentHandoffContextSummarySchema.parse({
        ...handoff,
        carriedContext: {
          ...handoff.carriedContext,
          completedWork: ['not a backend handoff work item'],
        },
      }),
    ).toThrow()
    expect(() =>
      agentHandoffContextSummarySchema.parse({
        ...handoff,
        carriedContext: {
          ...handoff.carriedContext,
          workingSetSummary: {
            ...handoff.carriedContext.workingSetSummary,
            activeTodoCount: 2,
          },
        },
      }),
    ).toThrow(/active todo count/)
    expect(() =>
      agentHandoffContextSummarySchema.parse({
        ...handoff,
        safetyRationale: {
          ...handoff.safetyRationale,
          sameRuntimeAgent: false,
        },
      }),
    ).toThrow(/runtime agents/)

    const support = agentSupportDiagnosticsBundleSchema.parse({
      schema: 'xero.agent_support_diagnostics_bundle.v1',
      projectId,
      generatedAt: createdAt,
      redactionState: 'clean',
      ui: {
        newUiImplemented: false,
        reason: 'Backend diagnostics bundle only.',
        deferredSurfaces: [
          {
            surface: 'user_control',
            slices: ['S28', 'S43', 'S61', 'S65'],
            status: 'deferred_no_new_ui',
            backendEvidence: ['get_agent_support_diagnostics_bundle'],
          },
        ],
      },
      storage: {
        projectId,
        migrationVersion: 42,
        stateFileBytes: 1024,
        appDataBytes: 4096,
        projectRecordHealthStatus: 'healthy',
        agentMemoryHealthStatus: 'healthy',
        retrievalHealthStatus: 'healthy',
        pendingOutboxCount: 0,
        failedReconciliationCount: 0,
        lastSuccessfulMaintenanceAt: null,
        lanceHealth: {
          projectRecords: lanceHealth('project_records'),
          agentMemories: lanceHealth('agent_memories'),
        },
        diagnostics: [],
      },
      performanceBudgets: {
        projectId,
        budgets: [
          {
            operation: 'startup_diagnostics',
            budgetMs: 1000,
            measurementSource: 'SQLite quick_check, migration status, and Lance health',
            enforcement: 'warning',
            status: 'unmeasured',
          },
          {
            operation: 'handoff_preparation',
            budgetMs: 2000,
            measurementSource: 'handoff bundle build, record write, and lineage update',
            enforcement: 'blocker',
            status: 'unmeasured',
          },
        ],
        diagnostics: [],
      },
      failureAreas: {
        visualBuilder: {
          status: 'ui_deferred',
          signals: ['visible_builder_support_report_not_implemented'],
        },
        runtimePolicy: {
          status: 'available',
          runtimeAuditStatus: 'available',
          activeRevocationCount: 1,
          signals: ['tool_policy', 'capability_permission_explanations'],
        },
        storage: {
          status: 'healthy',
          diagnosticCount: 0,
          pendingOutboxCount: 0,
          failedReconciliationCount: 0,
          startupBudgetStatus: 'unmeasured',
          projectOpenBudgetStatus: null,
        },
        retrieval: {
          status: 'healthy',
          projectRecordHealthStatus: 'healthy',
          agentMemoryHealthStatus: 'healthy',
          retrievalBudgetStatus: null,
        },
        memory: {
          status: 'healthy',
          memoryReviewBudgetStatus: null,
          freshness: freshnessCounts,
        },
        handoff: {
          status: 'available',
          runtimeAuditStatus: 'available',
          handoffBudgetStatus: 'unmeasured',
          signals: ['handoff_policy', 'context_manifest_ids'],
        },
      },
      capabilityRevocations: {
        activeCount: 1,
        active: [
          {
            revocationId: 'revocation-1',
            subjectKind: 'tool_pack',
            subjectId: 'project_context_tools',
            status: 'active',
            reason: 'User disabled this pack for the project.',
            createdBy: 'user',
            createdAt,
            scope: { projectId },
          },
        ],
      },
      runtimeAudit: {
        status: 'available',
        runId,
        agentSessionId: 'agent-session-main',
        runtimeAgentId: 'engineer',
        providerId: 'openrouter',
        modelId: 'openai/gpt-5.4',
        agentDefinitionId: 'custom-agent',
        agentDefinitionVersion: 3,
        contextManifestIds: ['manifest-1'],
        toolPolicy: { allowedTools: ['project_context_get'] },
        memoryPolicy: { reviewRequired: true },
        retrievalPolicy: { searchScope: 'hybrid_context' },
        outputContract: { kind: 'patch' },
        handoffPolicy: { status: 'recorded' },
        capabilityPermissionExplanations: [
          {
            schema: 'xero.capability_permission_explanation.v1',
            subjectKind: 'custom_agent',
            subjectId: 'custom-agent',
            summary: 'Custom agent definition can select runtime policy.',
            dataAccess: 'project runtime state',
            networkAccess: 'depends_on_effective_tool_policy',
            fileMutation: 'depends_on_effective_tool_policy',
            confirmationRequired: true,
            riskClass: 'custom_agent_runtime',
          },
        ],
        riskyCapabilityApprovalCount: 0,
        auditEvents: [
          {
            auditId: 'audit-1',
            actionKind: 'capability_approval_granted',
            subjectKind: 'custom_agent',
            subjectId: 'custom-agent',
            riskClass: null,
            approvalActionId: null,
            createdAt,
            payload: { reason: 'Fixture event summary.' },
          },
        ],
      },
    })
    expect(support.ui.newUiImplemented).toBe(false)
    expect(support.ui.deferredSurfaces[0]?.backendEvidence).toContain(
      'get_agent_support_diagnostics_bundle',
    )
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        capabilityRevocations: {
          ...support.capabilityRevocations,
          activeCount: 2,
        },
      }),
    ).toThrow(/revocation count/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        storage: {
          ...support.storage,
          projectId: 'different-project',
        },
      }),
    ).toThrow(/storage project/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        failureAreas: {
          ...support.failureAreas,
          runtimePolicy: {
            ...support.failureAreas.runtimePolicy,
            activeRevocationCount: 2,
          },
        },
      }),
    ).toThrow(/active revocation count/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        failureAreas: {
          ...support.failureAreas,
          runtimePolicy: {
            ...support.failureAreas.runtimePolicy,
            runtimeAuditStatus: 'unavailable',
          },
        },
      }),
    ).toThrow(/audit status/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        failureAreas: {
          ...support.failureAreas,
          storage: {
            ...support.failureAreas.storage,
            diagnosticCount: 1,
          },
        },
      }),
    ).toThrow(/diagnostic count/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        failureAreas: {
          ...support.failureAreas,
          retrieval: {
            ...support.failureAreas.retrieval,
            projectRecordHealthStatus: 'degraded',
          },
        },
      }),
    ).toThrow(/project-record health/)
    expect(() =>
      agentSupportDiagnosticsBundleSchema.parse({
        ...support,
        failureAreas: {
          ...support.failureAreas,
          memory: {
            ...support.failureAreas.memory,
            freshness: {
              ...support.failureAreas.memory.freshness,
              staleRowCount: 1,
            },
          },
        },
      }),
    ).toThrow(/memory freshness/)
    expect(() =>
      agentSupportRuntimeAuditSchema.parse({
        ...support.runtimeAudit,
        toolPolicy: 'not a backend policy object',
      }),
    ).toThrow()
    expect(
      agentSupportRuntimeAuditSchema.parse({
        status: 'unavailable',
        runId,
        code: 'agent_runtime_audit_missing',
        message: 'Runtime audit is not available for this fixture.',
      }).status,
    ).toBe('unavailable')
    expect(agentSupportRuntimeAuditSchema.parse({ status: 'not_requested', runId: null }).runId).toBeNull()
  })
})
