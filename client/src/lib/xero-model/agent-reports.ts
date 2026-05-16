import { z } from 'zod'
import { isoTimestampSchema } from '@xero/ui/model/shared'

const nonnegativeIntSchema = z.number().int().nonnegative()
const nullableTextSchema = z.string().trim().min(1).nullable()
const optionalTextSchema = z.string().trim().min(1).nullable().optional()
const jsonObjectSchema = z.record(z.string(), z.unknown())
const handoffIdValueSchema = z.union([
  z.string().trim().min(1),
  z.number().int().nonnegative(),
])
const addDuplicateStringIssues = (
  ctx: z.RefinementCtx,
  path: (string | number)[],
  values: string[],
  message: string,
) => {
  const seen = new Set<string>()
  values.forEach((value, index) => {
    if (seen.has(value)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: [...path, index],
        message,
      })
    }
    seen.add(value)
  })
}

export const getAgentRunStartExplanationRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
  })
  .strict()

export const getAgentKnowledgeInspectionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: optionalTextSchema,
    runId: optionalTextSchema,
    limit: z.number().int().positive().max(50).nullable().optional(),
  })
  .strict()

export const getAgentHandoffContextSummaryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    handoffId: z.string().trim().min(1).nullable().optional(),
    targetRunId: z.string().trim().min(1).nullable().optional(),
    sourceRunId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (!request.handoffId && !request.targetRunId && !request.sourceRunId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['handoffId'],
        message:
          'Provide handoffId, targetRunId, or sourceRunId to look up a handoff context summary.',
      })
    }
  })

export const getAgentSupportDiagnosticsBundleRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: optionalTextSchema,
  })
  .strict()

export const capabilityPermissionSubjectKindSchema = z.enum([
  'custom_agent',
  'tool_pack',
  'external_integration',
  'browser_control',
  'destructive_write',
  'skill_runtime_tool',
  'attached_skill_context',
])

export const getCapabilityPermissionExplanationRequestSchema = z
  .object({
    subjectKind: capabilityPermissionSubjectKindSchema,
    subjectId: z.string().trim().min(1),
  })
  .strict()

export const getAgentDatabaseTouchpointExplanationRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
  })
  .strict()

export const capabilityPermissionToolPackReviewRequirementSchema = z
  .object({
    requirementId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    required: z.boolean(),
  })
  .strict()

export const capabilityPermissionToolPackSchema = z
  .object({
    packId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    policyProfile: z.string().trim().min(1),
    tools: z.array(z.string().trim().min(1)),
    capabilities: z.array(z.string().trim().min(1)),
    allowedEffectClasses: z.array(z.string().trim().min(1)),
    deniedEffectClasses: z.array(z.string().trim().min(1)),
    reviewRequirements: z.array(capabilityPermissionToolPackReviewRequirementSchema),
    approvalBoundaries: z.array(z.string().trim().min(1)),
  })
  .strict()
  .superRefine((toolPack, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['tools'],
      toolPack.tools,
      'Tool-pack permission tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['capabilities'],
      toolPack.capabilities,
      'Tool-pack permission capabilities must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedEffectClasses'],
      toolPack.allowedEffectClasses,
      'Tool-pack permission allowed effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedEffectClasses'],
      toolPack.deniedEffectClasses,
      'Tool-pack permission denied effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['reviewRequirements'],
      toolPack.reviewRequirements.map((requirement) => requirement.requirementId),
      'Tool-pack permission review requirement ids must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['approvalBoundaries'],
      toolPack.approvalBoundaries,
      'Tool-pack permission approval boundaries must be unique.',
    )
    const allowedEffectClasses = new Set(toolPack.allowedEffectClasses)
    toolPack.deniedEffectClasses.forEach((effectClass, index) => {
      if (allowedEffectClasses.has(effectClass)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['deniedEffectClasses', index],
          message: 'Tool-pack permission effect classes cannot be both allowed and denied.',
        })
      }
    })
  })

export const capabilityPermissionExplanationSchema = z
  .object({
    schema: z.literal('xero.capability_permission_explanation.v1'),
    subjectKind: capabilityPermissionSubjectKindSchema,
    subjectId: z.string().trim().min(1),
    summary: z.string().trim().min(1),
    dataAccess: z.string().trim().min(1),
    networkAccess: z.string().trim().min(1),
    fileMutation: z.string().trim().min(1),
    confirmationRequired: z.boolean(),
    riskClass: z.string().trim().min(1),
    toolPack: capabilityPermissionToolPackSchema.optional(),
  })
  .strict()
  .superRefine((explanation, ctx) => {
    if (explanation.subjectKind === 'tool_pack') {
      if (!explanation.toolPack) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['toolPack'],
          message: 'Tool-pack permission explanations must include tool-pack metadata.',
        })
        return
      }
      if (explanation.toolPack.packId !== explanation.subjectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['toolPack', 'packId'],
          message: 'Tool-pack permission metadata packId must match subjectId.',
        })
      }
      return
    }
    if (explanation.toolPack) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['toolPack'],
        message: 'Only tool-pack permission explanations may include tool-pack metadata.',
      })
    }
  })

export const agentDatabaseTouchpointEntrySchema = z
  .object({
    table: z.string().trim().min(1),
    kind: z.enum(['read', 'write', 'encouraged']),
    purpose: z.string().trim().min(1),
    columns: z.array(z.string().trim().min(1)),
    triggerCount: z.number().int().nonnegative(),
  })
  .strict()

export const agentDatabaseTouchpointExplanationSchema = z
  .object({
    schema: z.literal('xero.agent_database_touchpoint_explanation.v1'),
    projectId: z.string().trim().min(1),
    definition: z
      .object({
        definitionId: z.string().trim().min(1),
        version: z.number().int().positive(),
      })
      .strict(),
    summary: z
      .object({
        readCount: z.number().int().nonnegative(),
        writeCount: z.number().int().nonnegative(),
        encouragedCount: z.number().int().nonnegative(),
        hasWrites: z.boolean(),
      })
      .strict(),
    touchpoints: z
      .object({
        reads: z.array(agentDatabaseTouchpointEntrySchema),
        writes: z.array(agentDatabaseTouchpointEntrySchema),
        encouraged: z.array(agentDatabaseTouchpointEntrySchema),
      })
      .strict(),
    explanation: z
      .object({
        readBehavior: z.string().trim().min(1),
        writeBehavior: z.string().trim().min(1),
        encouragedBehavior: z.string().trim().min(1),
        auditVisibility: z.string().trim().min(1),
        userConfirmation: z.string().trim().min(1),
      })
      .strict(),
    source: z
      .object({
        kind: z.literal('agent_definition_snapshot'),
        path: z.literal('dbTouchpoints'),
      })
      .strict(),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((explanation, ctx) => {
    if (explanation.summary.readCount !== explanation.touchpoints.reads.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary', 'readCount'],
        message: 'Database touchpoint readCount must match read touchpoints.',
      })
    }
    if (explanation.summary.writeCount !== explanation.touchpoints.writes.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary', 'writeCount'],
        message: 'Database touchpoint writeCount must match write touchpoints.',
      })
    }
    if (
      explanation.summary.encouragedCount !== explanation.touchpoints.encouraged.length
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary', 'encouragedCount'],
        message: 'Database touchpoint encouragedCount must match encouraged touchpoints.',
      })
    }
    if (explanation.summary.hasWrites !== explanation.touchpoints.writes.length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['summary', 'hasWrites'],
        message: 'Database touchpoint hasWrites must reflect write touchpoints.',
      })
    }
  })

export const agentRunStartSourceSchema = z
  .object({
    kind: z.literal('runtime_audit_export'),
    agentSessionId: z.string().trim().min(1),
    contextManifestIds: z.array(z.string().trim().min(1)),
  })
  .strict()

export const agentRunStartDefinitionSchema = z
  .object({
    runtimeAgentId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
  })
  .strict()

export const agentRunStartModelSchema = z
  .object({
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
  })
  .strict()

export const agentRunStartApprovalSchema = z
  .object({
    defaultMode: nullableTextSchema,
    allowedModes: z.array(z.string().trim().min(1)),
    source: z.literal('agent_definition_snapshot'),
  })
  .strict()
  .superRefine((approval, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['allowedModes'],
      approval.allowedModes,
      'Run-start approval allowed modes must be unique.',
    )
    if (approval.defaultMode !== null && !approval.allowedModes.includes(approval.defaultMode)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['defaultMode'],
        message: 'Run-start approval default mode must be allowed.',
      })
    }
  })

export const agentRunStartExplanationSchema = z
  .object({
    schema: z.literal('xero.agent_run_start_explanation.v1'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    definition: agentRunStartDefinitionSchema,
    model: agentRunStartModelSchema,
    approval: agentRunStartApprovalSchema,
    contextPolicy: jsonObjectSchema,
    toolPolicy: jsonObjectSchema,
    memoryPolicy: jsonObjectSchema,
    retrievalPolicy: jsonObjectSchema,
    outputContract: z.union([jsonObjectSchema, z.string().trim().min(1)]),
    handoffPolicy: jsonObjectSchema,
    databaseTouchpointExplanation: agentDatabaseTouchpointExplanationSchema.optional(),
    capabilityPermissionExplanations: z.array(capabilityPermissionExplanationSchema),
    riskyCapabilityApprovalCount: z.number().int().nonnegative(),
    source: agentRunStartSourceSchema,
  })
  .strict()
  .superRefine((explanation, ctx) => {
    const touchpoints = explanation.databaseTouchpointExplanation
    if (!touchpoints) {
      return
    }
    if (touchpoints.projectId !== explanation.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['databaseTouchpointExplanation', 'projectId'],
        message: 'Run-start database touchpoints must use the same project as the run.',
      })
    }
    if (touchpoints.definition.definitionId !== explanation.definition.definitionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['databaseTouchpointExplanation', 'definition', 'definitionId'],
        message: 'Run-start database touchpoints must use the pinned definition id.',
      })
    }
    if (touchpoints.definition.version !== explanation.definition.version) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['databaseTouchpointExplanation', 'definition', 'version'],
        message: 'Run-start database touchpoints must use the pinned definition version.',
      })
    }
  })

export const agentKnowledgeProjectRecordSchema = z
  .object({
    recordId: z.string().trim().min(1),
    recordKind: z.string().trim().min(1),
    title: z.string().trim().min(1),
    summary: nullableTextSchema,
    textPreview: nullableTextSchema,
    schemaName: nullableTextSchema,
    importance: z.enum(['low', 'normal', 'high', 'critical']),
    confidence: z.number().nullable(),
    tags: z.array(z.string().trim().min(1)),
    relatedPaths: z.array(z.string().trim().min(1)),
    freshnessState: z.string().trim().min(1),
    redactionState: z.enum(['clean', 'redacted', 'blocked']),
    sourceItemIds: z.array(z.string().trim().min(1)),
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((record, ctx) => {
    if (record.redactionState === 'blocked') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['redactionState'],
        message: 'Knowledge inspection must exclude blocked project records.',
      })
    }
    if (record.redactionState === 'redacted' && record.textPreview !== null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['textPreview'],
        message: 'Knowledge inspection must hide redacted project-record text.',
      })
    }
  })

export const agentKnowledgeApprovedMemorySchema = z
  .object({
    memoryId: z.string().trim().min(1),
    scope: z.enum(['project', 'session']),
    kind: z.enum([
      'project_fact',
      'user_preference',
      'decision',
      'session_summary',
      'troubleshooting',
    ]),
    textPreview: z.string().trim().min(1),
    confidence: nonnegativeIntSchema.nullable(),
    sourceRunId: nullableTextSchema,
    sourceItemIds: z.array(z.string().trim().min(1)),
    freshnessState: z.string().trim().min(1),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const agentKnowledgeHandoffRecordSchema = z
  .object({
    handoffId: z.string().trim().min(1),
    status: z.string().trim().min(1),
    sourceRunId: z.string().trim().min(1),
    targetRunId: nullableTextSchema,
    runtimeAgentId: z.string().trim().min(1),
    agentDefinitionId: z.string().trim().min(1),
    agentDefinitionVersion: z.number().int().positive(),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    handoffRecordId: nullableTextSchema,
    bundleKeys: z.array(z.string().trim().min(1)),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const agentKnowledgeInspectionSchema = z
  .object({
    schema: z.literal('xero.agent_knowledge_inspection.v1'),
    projectId: z.string().trim().min(1),
    agentSessionId: optionalTextSchema,
    runId: optionalTextSchema,
    limit: z.number().int().positive().max(50),
    retrievalPolicy: z
      .object({
        source: z.enum(['runtime_audit_export', 'not_requested']),
        policy: jsonObjectSchema,
        recordKindFilter: z.array(z.string().trim().min(1)),
        memoryKindFilter: z.array(z.string().trim().min(1)),
        filtersApplied: z.boolean(),
      })
      .strict(),
    projectRecords: z.array(agentKnowledgeProjectRecordSchema),
    continuityRecords: z.array(agentKnowledgeProjectRecordSchema),
    approvedMemory: z.array(agentKnowledgeApprovedMemorySchema),
    handoffRecords: z.array(agentKnowledgeHandoffRecordSchema),
    redaction: z
      .object({
        rawBlockedRecordsExcluded: z.boolean(),
        redactedProjectRecordTextHidden: z.boolean(),
        handoffBundleRawPayloadHidden: z.boolean(),
      })
      .strict(),
  })
  .strict()
  .superRefine((inspection, ctx) => {
    if (inspection.retrievalPolicy.source === 'runtime_audit_export') {
      if (!inspection.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['runId'],
          message: 'Run-scoped knowledge inspection must include runId.',
        })
      }
      if (!inspection.agentSessionId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['agentSessionId'],
          message: 'Run-scoped knowledge inspection must include agentSessionId.',
        })
      }
    }
    if (inspection.retrievalPolicy.source === 'not_requested' && inspection.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['retrievalPolicy', 'source'],
        message: 'Knowledge inspection without runtime audit must not include runId.',
      })
    }
    const boundedSections = [
      ['projectRecords', inspection.projectRecords],
      ['continuityRecords', inspection.continuityRecords],
      ['approvedMemory', inspection.approvedMemory],
      ['handoffRecords', inspection.handoffRecords],
    ] as const
    boundedSections.forEach(([section, items]) => {
      if (items.length > inspection.limit) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: [section],
          message: 'Knowledge inspection sections must not exceed the requested limit.',
        })
      }
    })
    addDuplicateStringIssues(
      ctx,
      ['projectRecords'],
      inspection.projectRecords.map((record) => record.recordId),
      'Knowledge inspection project records must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['continuityRecords'],
      inspection.continuityRecords.map((record) => record.recordId),
      'Knowledge inspection continuity records must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['approvedMemory'],
      inspection.approvedMemory.map((memory) => memory.memoryId),
      'Knowledge inspection approved memory entries must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['handoffRecords'],
      inspection.handoffRecords.map((handoff) => handoff.handoffId),
      'Knowledge inspection handoff records must be unique.',
    )

    if (inspection.retrievalPolicy.filtersApplied) {
      const recordKindFilter = new Set(inspection.retrievalPolicy.recordKindFilter)
      if (recordKindFilter.size > 0) {
        const filteredRecordSections = [
          ['projectRecords', inspection.projectRecords],
          ['continuityRecords', inspection.continuityRecords],
        ] as const
        filteredRecordSections.forEach(([section, records]) => {
          records.forEach((record, index) => {
            if (!recordKindFilter.has(record.recordKind)) {
              ctx.addIssue({
                code: z.ZodIssueCode.custom,
                path: [section, index, 'recordKind'],
                message: 'Knowledge inspection project records must match retrieval policy filters.',
              })
            }
          })
        })
      }

      const memoryKindFilter = new Set(inspection.retrievalPolicy.memoryKindFilter)
      if (memoryKindFilter.size > 0) {
        inspection.approvedMemory.forEach((memory, index) => {
          if (!memoryKindFilter.has(memory.kind)) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: ['approvedMemory', index, 'kind'],
              message: 'Knowledge inspection approved memory must match retrieval policy filters.',
            })
          }
        })
      }
    }
  })

export const agentHandoffSourceSchema = z
  .object({
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeAgentId: z.string().trim().min(1),
    agentDefinitionId: z.string().trim().min(1),
    agentDefinitionVersion: z.number().int().positive(),
    contextHash: z.string().trim().min(1),
  })
  .strict()

export const agentHandoffTargetSchema = z
  .object({
    agentSessionId: nullableTextSchema,
    runId: nullableTextSchema,
    runtimeAgentId: z.string().trim().min(1),
    agentDefinitionId: z.string().trim().min(1),
    agentDefinitionVersion: z.number().int().positive(),
  })
  .strict()

export const agentHandoffProviderSchema = z
  .object({
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
  })
  .strict()

export const agentHandoffCompletedWorkSchema = z
  .object({
    messageId: handoffIdValueSchema.nullable(),
    createdAt: isoTimestampSchema.nullable(),
    summary: nullableTextSchema,
  })
  .strict()

export const agentHandoffPendingWorkSchema = z
  .object({
    kind: z.string().trim().min(1),
    text: nullableTextSchema,
  })
  .strict()

export const agentHandoffTodoItemSchema = z
  .object({
    id: handoffIdValueSchema.nullable(),
    status: nullableTextSchema,
    text: nullableTextSchema,
  })
  .strict()

export const agentHandoffEventSummarySchema = z
  .object({
    kind: z.string().trim().min(1),
    eventId: handoffIdValueSchema,
    eventKind: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
    summary: z.string().trim().min(1),
  })
  .strict()

export const agentHandoffDurableContextSchema = z
  .object({
    deliveryModel: z.literal('tool_mediated'),
    toolName: z.string().trim().min(1),
    rawContextInjected: z.literal(false),
    sourceContextHash: z.string().trim().min(1),
    instruction: z.string().trim().min(1),
  })
  .strict()

export const agentHandoffWorkingSetSummarySchema = z
  .object({
    schema: z.literal('xero.agent_handoff.working_set.v1'),
    sourceRunId: z.string().trim().min(1),
    sourceContextHash: z.string().trim().min(1),
    activeTodoCount: nonnegativeIntSchema,
    recentFileChangeCount: nonnegativeIntSchema,
    latestChangedPaths: z.array(z.string().trim().min(1)),
    assistantMessageIds: z.array(handoffIdValueSchema),
  })
  .strict()

export const agentHandoffContinuityRecordSchema = z
  .object({
    sourceKind: z.enum(['agent_message', 'agent_file_change']),
    sourceId: handoffIdValueSchema.nullable(),
    createdAt: isoTimestampSchema.nullable(),
    summary: z.union([nullableTextSchema, jsonObjectSchema]),
  })
  .strict()

export const agentHandoffRecentFileChangeSchema = z
  .object({
    path: z.string().trim().min(1),
    operation: z.string().trim().min(1),
    oldHash: nullableTextSchema,
    newHash: nullableTextSchema,
    createdAt: isoTimestampSchema,
  })
  .strict()

export const agentHandoffToolEvidenceSchema = z
  .object({
    toolCallId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    state: z.string().trim().min(1),
    inputPreview: nullableTextSchema,
    error: z
      .object({
        code: z.string().trim().min(1),
        message: nullableTextSchema,
      })
      .strict()
      .nullable(),
  })
  .strict()

export const agentHandoffVerificationStatusSchema = z
  .object({
    status: z.enum(['recorded', 'not_recorded']),
    evidence: z.array(agentHandoffEventSummarySchema),
  })
  .strict()

export const agentHandoffCarriedContextSchema = z
  .object({
    userGoal: nullableTextSchema,
    currentTask: nullableTextSchema,
    currentStatus: nullableTextSchema,
    completedWork: z.array(agentHandoffCompletedWorkSchema),
    pendingWork: z.array(agentHandoffPendingWorkSchema),
    activeTodoItems: z.array(agentHandoffTodoItemSchema),
    importantDecisions: z.array(agentHandoffEventSummarySchema),
    constraints: z.array(z.string().trim().min(1)),
    durableContext: agentHandoffDurableContextSchema,
    workingSetSummary: agentHandoffWorkingSetSummarySchema,
    sourceCitedContinuityRecords: z.array(agentHandoffContinuityRecordSchema),
    recentFileChanges: z.array(agentHandoffRecentFileChangeSchema),
    toolAndCommandEvidence: z.array(agentHandoffToolEvidenceSchema),
    verificationStatus: agentHandoffVerificationStatusSchema,
    knownRisks: z.array(agentHandoffEventSummarySchema),
    openQuestions: z.array(agentHandoffEventSummarySchema),
    approvedMemories: z.array(jsonObjectSchema),
    relevantProjectRecords: z.array(jsonObjectSchema),
    agentSpecific: jsonObjectSchema,
  })
  .strict()
  .superRefine((context, ctx) => {
    if (context.workingSetSummary.activeTodoCount !== context.activeTodoItems.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['workingSetSummary', 'activeTodoCount'],
        message: 'Handoff working-set active todo count must match carried todos.',
      })
    }
    if (context.workingSetSummary.recentFileChangeCount !== context.recentFileChanges.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['workingSetSummary', 'recentFileChangeCount'],
        message: 'Handoff working-set recent file-change count must match carried file changes.',
      })
    }
    if (context.workingSetSummary.sourceContextHash !== context.durableContext.sourceContextHash) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['workingSetSummary', 'sourceContextHash'],
        message: 'Handoff working set must cite the same source context hash as durable context.',
      })
    }
  })

export const agentHandoffOmittedContextSchema = z
  .object({
    kind: z.string().trim().min(1),
    status: z.string().trim().min(1),
    reason: z.string().trim().min(1),
    referenceCount: nonnegativeIntSchema.optional(),
  })
  .strict()

export const agentHandoffSafetyRationaleSchema = z
  .object({
    sameRuntimeAgent: z.boolean(),
    sameDefinitionVersion: z.boolean(),
    sourceContextHashPresent: z.boolean(),
    targetRunCreated: z.boolean(),
    handoffRecordPersisted: z.boolean(),
    reasons: z.array(z.string().trim().min(1)),
  })
  .strict()

export const agentHandoffContextSummarySchema = z
  .object({
    schema: z.literal('xero.agent_handoff_context_summary.v1'),
    projectId: z.string().trim().min(1),
    handoffId: z.string().trim().min(1),
    status: z.string().trim().min(1),
    source: agentHandoffSourceSchema,
    target: agentHandoffTargetSchema,
    provider: agentHandoffProviderSchema,
    carriedContext: agentHandoffCarriedContextSchema,
    omittedContext: z.array(agentHandoffOmittedContextSchema),
    redaction: z
      .object({
        state: z.string().trim().min(1),
        bundleRedactionCount: nonnegativeIntSchema.nullable(),
        summaryRedactionApplied: z.boolean(),
        rawPayloadHidden: z.literal(true),
      })
      .strict(),
    safetyRationale: agentHandoffSafetyRationaleSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    completedAt: optionalTextSchema,
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((summary, ctx) => {
    const sameRuntimeAgent = summary.source.runtimeAgentId === summary.target.runtimeAgentId
    if (summary.safetyRationale.sameRuntimeAgent !== sameRuntimeAgent) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['safetyRationale', 'sameRuntimeAgent'],
        message: 'Handoff safety rationale must match source and target runtime agents.',
      })
    }

    const sameDefinitionVersion =
      summary.source.agentDefinitionId === summary.target.agentDefinitionId &&
      summary.source.agentDefinitionVersion === summary.target.agentDefinitionVersion
    if (summary.safetyRationale.sameDefinitionVersion !== sameDefinitionVersion) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['safetyRationale', 'sameDefinitionVersion'],
        message: 'Handoff safety rationale must match source and target definition versions.',
      })
    }

    if (summary.safetyRationale.sourceContextHashPresent !== true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['safetyRationale', 'sourceContextHashPresent'],
        message: 'Handoff safety rationale must reflect the required source context hash.',
      })
    }
    if (summary.safetyRationale.targetRunCreated !== (summary.target.runId !== null)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['safetyRationale', 'targetRunCreated'],
        message: 'Handoff safety rationale must match target run creation state.',
      })
    }
    if (summary.carriedContext.workingSetSummary.sourceRunId !== summary.source.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['carriedContext', 'workingSetSummary', 'sourceRunId'],
        message: 'Handoff working set must cite the source run.',
      })
    }
    if (summary.carriedContext.workingSetSummary.sourceContextHash !== summary.source.contextHash) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['carriedContext', 'workingSetSummary', 'sourceContextHash'],
        message: 'Handoff working set must cite the source context hash.',
      })
    }
  })

export const agentSupportRuntimeAuditEventSchema = z
  .object({
    auditId: z.string().trim().min(1),
    actionKind: z.string().trim().min(1),
    subjectKind: z.string().trim().min(1),
    subjectId: z.string().trim().min(1),
    riskClass: z.string().trim().min(1).nullable(),
    approvalActionId: z.string().trim().min(1).nullable(),
    createdAt: isoTimestampSchema,
    payload: jsonObjectSchema,
  })
  .strict()

export const agentSupportRuntimeAuditSchema = z.discriminatedUnion('status', [
  z
    .object({
      status: z.literal('available'),
      runId: z.string().trim().min(1),
      agentSessionId: z.string().trim().min(1),
      runtimeAgentId: z.string().trim().min(1),
      providerId: z.string().trim().min(1),
      modelId: z.string().trim().min(1),
      agentDefinitionId: z.string().trim().min(1),
      agentDefinitionVersion: z.number().int().positive(),
      contextManifestIds: z.array(z.string().trim().min(1)),
      toolPolicy: jsonObjectSchema,
      memoryPolicy: jsonObjectSchema,
      retrievalPolicy: jsonObjectSchema,
      outputContract: z.union([jsonObjectSchema, z.string().trim().min(1)]),
      handoffPolicy: jsonObjectSchema,
      capabilityPermissionExplanations: z.array(capabilityPermissionExplanationSchema),
      riskyCapabilityApprovalCount: z.number().int().nonnegative(),
      auditEvents: z.array(agentSupportRuntimeAuditEventSchema),
    })
    .strict(),
  z
    .object({
      status: z.literal('unavailable'),
      runId: z.string().trim().min(1),
      code: z.string().trim().min(1),
      message: z.string().trim().min(1),
    })
    .strict(),
  z
    .object({
      status: z.literal('not_requested'),
      runId: z.null(),
    })
    .strict(),
])

export const agentSupportDeferredSurfaceSchema = z
  .object({
    surface: z.string().trim().min(1),
    slices: z.array(z.string().trim().min(1)),
    status: z.string().trim().min(1),
    backendEvidence: z.array(z.string().trim().min(1)),
  })
  .strict()

export const agentSupportStorageDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    severity: z.string().trim().min(1),
  })
  .strict()

export const agentSupportFreshnessCountsSchema = z
  .object({
    inspectedRowCount: nonnegativeIntSchema,
    currentRowCount: nonnegativeIntSchema,
    sourceUnknownRowCount: nonnegativeIntSchema,
    staleRowCount: nonnegativeIntSchema,
    sourceMissingRowCount: nonnegativeIntSchema,
    supersededRowCount: nonnegativeIntSchema,
    blockedRowCount: nonnegativeIntSchema,
    retrievalDegradedRowCount: nonnegativeIntSchema,
  })
  .strict()

export const agentSupportLanceHealthSchema = z
  .object({
    tableName: z.string().trim().min(1),
    status: z.string().trim().min(1),
    schemaCurrent: z.boolean(),
    version: nonnegativeIntSchema,
    rowCount: nonnegativeIntSchema,
    totalBytes: nonnegativeIntSchema,
    indexCount: nonnegativeIntSchema,
    fragmentCount: nonnegativeIntSchema,
    smallFragmentCount: nonnegativeIntSchema,
    statsLatencyMs: nonnegativeIntSchema,
    maintenanceRecommended: z.boolean(),
    quarantineTableCount: nonnegativeIntSchema,
    diagnosticMarkerCount: nonnegativeIntSchema,
    freshness: agentSupportFreshnessCountsSchema,
  })
  .strict()

export const agentSupportStorageSchema = z
  .object({
    projectId: z.string().trim().min(1),
    migrationVersion: nonnegativeIntSchema,
    stateFileBytes: nonnegativeIntSchema,
    appDataBytes: nonnegativeIntSchema,
    projectRecordHealthStatus: z.string().trim().min(1),
    agentMemoryHealthStatus: z.string().trim().min(1),
    retrievalHealthStatus: z.string().trim().min(1),
    pendingOutboxCount: nonnegativeIntSchema,
    failedReconciliationCount: nonnegativeIntSchema,
    lastSuccessfulMaintenanceAt: nullableTextSchema,
    lanceHealth: z
      .object({
        projectRecords: agentSupportLanceHealthSchema,
        agentMemories: agentSupportLanceHealthSchema,
      })
      .strict(),
    diagnostics: z.array(agentSupportStorageDiagnosticSchema),
  })
  .strict()

export const agentSupportPerformanceBudgetEntrySchema = z
  .object({
    operation: z.string().trim().min(1),
    budgetMs: nonnegativeIntSchema,
    measurementSource: z.string().trim().min(1),
    enforcement: z.string().trim().min(1),
    status: z.string().trim().min(1),
  })
  .strict()

export const agentSupportPerformanceBudgetsSchema = z
  .object({
    projectId: z.string().trim().min(1),
    budgets: z.array(agentSupportPerformanceBudgetEntrySchema),
    diagnostics: z.array(agentSupportStorageDiagnosticSchema),
  })
  .strict()

export const agentSupportFailureAreasSchema = z
  .object({
    visualBuilder: z
      .object({
        status: z.string().trim().min(1),
        signals: z.array(z.string().trim().min(1)),
      })
      .strict(),
    runtimePolicy: z
      .object({
        status: z.string().trim().min(1),
        runtimeAuditStatus: z.string().trim().min(1),
        activeRevocationCount: nonnegativeIntSchema,
        signals: z.array(z.string().trim().min(1)),
      })
      .strict(),
    storage: z
      .object({
        status: z.string().trim().min(1),
        diagnosticCount: nonnegativeIntSchema,
        pendingOutboxCount: nonnegativeIntSchema,
        failedReconciliationCount: nonnegativeIntSchema,
        startupBudgetStatus: nullableTextSchema,
        projectOpenBudgetStatus: nullableTextSchema,
      })
      .strict(),
    retrieval: z
      .object({
        status: z.string().trim().min(1),
        projectRecordHealthStatus: z.string().trim().min(1),
        agentMemoryHealthStatus: z.string().trim().min(1),
        retrievalBudgetStatus: nullableTextSchema,
      })
      .strict(),
    memory: z
      .object({
        status: z.string().trim().min(1),
        memoryReviewBudgetStatus: nullableTextSchema,
        freshness: agentSupportFreshnessCountsSchema,
      })
      .strict(),
    handoff: z
      .object({
        status: z.string().trim().min(1),
        runtimeAuditStatus: z.string().trim().min(1),
        handoffBudgetStatus: nullableTextSchema,
        signals: z.array(z.string().trim().min(1)),
      })
      .strict(),
  })
  .strict()

export const agentSupportCapabilityRevocationSchema = z
  .object({
    revocationId: z.string().trim().min(1),
    subjectKind: z.string().trim().min(1),
    subjectId: z.string().trim().min(1),
    status: z.string().trim().min(1),
    reason: z.string().trim().min(1),
    createdBy: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
    scope: jsonObjectSchema,
  })
  .strict()

export const agentSupportCapabilityRevocationsSchema = z
  .object({
    activeCount: nonnegativeIntSchema,
    active: z.array(agentSupportCapabilityRevocationSchema),
  })
  .strict()
  .superRefine((revocations, ctx) => {
    if (revocations.activeCount !== revocations.active.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['activeCount'],
        message: 'Support diagnostics active revocation count must match active entries.',
      })
    }
  })

export const agentSupportDiagnosticsBundleSchema = z
  .object({
    schema: z.literal('xero.agent_support_diagnostics_bundle.v1'),
    projectId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    redactionState: z.enum(['clean', 'redacted']),
    ui: z
      .object({
        newUiImplemented: z.literal(false),
        reason: z.string().trim().min(1),
        deferredSurfaces: z.array(agentSupportDeferredSurfaceSchema),
      })
      .strict(),
    storage: agentSupportStorageSchema,
    performanceBudgets: agentSupportPerformanceBudgetsSchema,
    failureAreas: agentSupportFailureAreasSchema,
    capabilityRevocations: agentSupportCapabilityRevocationsSchema,
    runtimeAudit: agentSupportRuntimeAuditSchema,
  })
  .strict()
  .superRefine((bundle, ctx) => {
    if (bundle.storage.projectId !== bundle.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['storage', 'projectId'],
        message: 'Support diagnostics storage project must match the bundle project.',
      })
    }
    if (bundle.performanceBudgets.projectId !== bundle.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['performanceBudgets', 'projectId'],
        message: 'Support diagnostics performance-budget project must match the bundle project.',
      })
    }
    if (
      bundle.failureAreas.runtimePolicy.activeRevocationCount !==
      bundle.capabilityRevocations.active.length
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'runtimePolicy', 'activeRevocationCount'],
        message: 'Support diagnostics runtime-policy active revocation count must match active entries.',
      })
    }
    if (bundle.failureAreas.runtimePolicy.runtimeAuditStatus !== bundle.runtimeAudit.status) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'runtimePolicy', 'runtimeAuditStatus'],
        message: 'Support diagnostics runtime-policy audit status must match runtime audit status.',
      })
    }
    if (bundle.failureAreas.handoff.runtimeAuditStatus !== bundle.runtimeAudit.status) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'handoff', 'runtimeAuditStatus'],
        message: 'Support diagnostics handoff audit status must match runtime audit status.',
      })
    }
    if (bundle.failureAreas.storage.diagnosticCount !== bundle.storage.diagnostics.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'storage', 'diagnosticCount'],
        message: 'Support diagnostics storage diagnostic count must match storage diagnostics.',
      })
    }
    if (bundle.failureAreas.storage.pendingOutboxCount !== bundle.storage.pendingOutboxCount) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'storage', 'pendingOutboxCount'],
        message: 'Support diagnostics storage pending outbox count must match storage health.',
      })
    }
    if (
      bundle.failureAreas.storage.failedReconciliationCount !==
      bundle.storage.failedReconciliationCount
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'storage', 'failedReconciliationCount'],
        message: 'Support diagnostics storage failed reconciliation count must match storage health.',
      })
    }
    if (
      bundle.failureAreas.retrieval.projectRecordHealthStatus !==
      bundle.storage.projectRecordHealthStatus
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'retrieval', 'projectRecordHealthStatus'],
        message: 'Support diagnostics retrieval project-record health must match storage health.',
      })
    }
    if (
      bundle.failureAreas.retrieval.agentMemoryHealthStatus !==
      bundle.storage.agentMemoryHealthStatus
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'retrieval', 'agentMemoryHealthStatus'],
        message: 'Support diagnostics retrieval memory health must match storage health.',
      })
    }
    if (
      JSON.stringify(bundle.failureAreas.memory.freshness) !==
      JSON.stringify(bundle.storage.lanceHealth.agentMemories.freshness)
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['failureAreas', 'memory', 'freshness'],
        message: 'Support diagnostics memory freshness must match Lance memory health.',
      })
    }
  })

export type GetAgentRunStartExplanationRequestDto = z.infer<typeof getAgentRunStartExplanationRequestSchema>
export type GetAgentKnowledgeInspectionRequestDto = z.infer<typeof getAgentKnowledgeInspectionRequestSchema>
export type GetAgentHandoffContextSummaryRequestDto = z.infer<typeof getAgentHandoffContextSummaryRequestSchema>
export type GetAgentSupportDiagnosticsBundleRequestDto = z.infer<typeof getAgentSupportDiagnosticsBundleRequestSchema>
export type CapabilityPermissionSubjectKindDto = z.infer<typeof capabilityPermissionSubjectKindSchema>
export type GetCapabilityPermissionExplanationRequestDto = z.infer<
  typeof getCapabilityPermissionExplanationRequestSchema
>
export type GetAgentDatabaseTouchpointExplanationRequestDto = z.infer<
  typeof getAgentDatabaseTouchpointExplanationRequestSchema
>
export type CapabilityPermissionExplanationDto = z.infer<typeof capabilityPermissionExplanationSchema>
export type CapabilityPermissionToolPackReviewRequirementDto = z.infer<
  typeof capabilityPermissionToolPackReviewRequirementSchema
>
export type CapabilityPermissionToolPackDto = z.infer<typeof capabilityPermissionToolPackSchema>
export type AgentDatabaseTouchpointEntryDto = z.infer<typeof agentDatabaseTouchpointEntrySchema>
export type AgentDatabaseTouchpointExplanationDto = z.infer<
  typeof agentDatabaseTouchpointExplanationSchema
>
export type AgentRunStartSourceDto = z.infer<typeof agentRunStartSourceSchema>
export type AgentRunStartDefinitionDto = z.infer<typeof agentRunStartDefinitionSchema>
export type AgentRunStartModelDto = z.infer<typeof agentRunStartModelSchema>
export type AgentRunStartApprovalDto = z.infer<typeof agentRunStartApprovalSchema>
export type AgentRunStartExplanationDto = z.infer<typeof agentRunStartExplanationSchema>
export type AgentKnowledgeProjectRecordDto = z.infer<typeof agentKnowledgeProjectRecordSchema>
export type AgentKnowledgeApprovedMemoryDto = z.infer<typeof agentKnowledgeApprovedMemorySchema>
export type AgentKnowledgeHandoffRecordDto = z.infer<typeof agentKnowledgeHandoffRecordSchema>
export type AgentKnowledgeInspectionDto = z.infer<typeof agentKnowledgeInspectionSchema>
export type AgentHandoffSourceDto = z.infer<typeof agentHandoffSourceSchema>
export type AgentHandoffTargetDto = z.infer<typeof agentHandoffTargetSchema>
export type AgentHandoffProviderDto = z.infer<typeof agentHandoffProviderSchema>
export type AgentHandoffCompletedWorkDto = z.infer<typeof agentHandoffCompletedWorkSchema>
export type AgentHandoffPendingWorkDto = z.infer<typeof agentHandoffPendingWorkSchema>
export type AgentHandoffTodoItemDto = z.infer<typeof agentHandoffTodoItemSchema>
export type AgentHandoffEventSummaryDto = z.infer<typeof agentHandoffEventSummarySchema>
export type AgentHandoffDurableContextDto = z.infer<typeof agentHandoffDurableContextSchema>
export type AgentHandoffWorkingSetSummaryDto = z.infer<
  typeof agentHandoffWorkingSetSummarySchema
>
export type AgentHandoffContinuityRecordDto = z.infer<
  typeof agentHandoffContinuityRecordSchema
>
export type AgentHandoffRecentFileChangeDto = z.infer<
  typeof agentHandoffRecentFileChangeSchema
>
export type AgentHandoffToolEvidenceDto = z.infer<typeof agentHandoffToolEvidenceSchema>
export type AgentHandoffVerificationStatusDto = z.infer<
  typeof agentHandoffVerificationStatusSchema
>
export type AgentHandoffCarriedContextDto = z.infer<typeof agentHandoffCarriedContextSchema>
export type AgentHandoffOmittedContextDto = z.infer<typeof agentHandoffOmittedContextSchema>
export type AgentHandoffSafetyRationaleDto = z.infer<typeof agentHandoffSafetyRationaleSchema>
export type AgentHandoffContextSummaryDto = z.infer<typeof agentHandoffContextSummarySchema>
export type AgentSupportRuntimeAuditEventDto = z.infer<typeof agentSupportRuntimeAuditEventSchema>
export type AgentSupportRuntimeAuditDto = z.infer<typeof agentSupportRuntimeAuditSchema>
export type AgentSupportDeferredSurfaceDto = z.infer<typeof agentSupportDeferredSurfaceSchema>
export type AgentSupportStorageDiagnosticDto = z.infer<typeof agentSupportStorageDiagnosticSchema>
export type AgentSupportFreshnessCountsDto = z.infer<typeof agentSupportFreshnessCountsSchema>
export type AgentSupportLanceHealthDto = z.infer<typeof agentSupportLanceHealthSchema>
export type AgentSupportStorageDto = z.infer<typeof agentSupportStorageSchema>
export type AgentSupportPerformanceBudgetEntryDto = z.infer<
  typeof agentSupportPerformanceBudgetEntrySchema
>
export type AgentSupportPerformanceBudgetsDto = z.infer<typeof agentSupportPerformanceBudgetsSchema>
export type AgentSupportFailureAreasDto = z.infer<typeof agentSupportFailureAreasSchema>
export type AgentSupportCapabilityRevocationDto = z.infer<
  typeof agentSupportCapabilityRevocationSchema
>
export type AgentSupportCapabilityRevocationsDto = z.infer<
  typeof agentSupportCapabilityRevocationsSchema
>
export type AgentSupportDiagnosticsBundleDto = z.infer<typeof agentSupportDiagnosticsBundleSchema>
