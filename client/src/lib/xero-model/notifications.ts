import { z } from 'zod'
import {
  resolveOperatorActionResponseSchema,
  resumeOperatorRunResponseSchema,
} from './operator-actions'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  optionalIsoTimestampSchema,
} from './shared'

const notificationCorrelationKeyPattern = /^nfy:[a-f0-9]{32}$/
const MAX_NOTIFICATION_BROKER_DISPATCHES = 250

export const notificationRouteKindSchema = z.enum(['telegram', 'discord'])
export const notificationDispatchStatusSchema = z.enum(['pending', 'sent', 'failed', 'claimed'])
export const notificationReplyClaimStatusSchema = z.enum(['accepted', 'rejected'])
export const notificationDispatchOutcomeStatusSchema = z.enum(['sent', 'failed'])

function splitNotificationRouteTarget(rawTarget: string): { prefix: string; channelTarget: string } | null {
  const separatorIndex = rawTarget.indexOf(':')
  if (separatorIndex < 0) {
    return null
  }

  return {
    prefix: rawTarget.slice(0, separatorIndex).trim(),
    channelTarget: rawTarget.slice(separatorIndex + 1).trim(),
  }
}

export function composeNotificationRouteTarget(
  routeKind: z.infer<typeof notificationRouteKindSchema>,
  rawTarget: string,
): string {
  const trimmed = rawTarget.trim()
  if (!trimmed) {
    throw new Error('Route target is required.')
  }

  const splitTarget = splitNotificationRouteTarget(trimmed)
  if (!splitTarget) {
    return `${routeKind}:${trimmed}`
  }

  if (!splitTarget.prefix) {
    throw new Error('Route target prefix is required when using `<kind>:<channel-target>` format.')
  }

  if (splitTarget.prefix === routeKind) {
    if (!splitTarget.channelTarget) {
      throw new Error('Route target channel segment is required after the `<kind>:` prefix.')
    }

    return `${routeKind}:${splitTarget.channelTarget}`
  }

  if (splitTarget.prefix === 'telegram' || splitTarget.prefix === 'discord') {
    throw new Error(
      `Route target prefix \`${splitTarget.prefix}\` does not match the selected route kind \`${routeKind}\`.`,
    )
  }

  return `${routeKind}:${trimmed}`
}

export function decomposeNotificationRouteTarget(
  routeKind: z.infer<typeof notificationRouteKindSchema>,
  routeTarget: string,
): { channelTarget: string; canonicalTarget: string } {
  const trimmed = routeTarget.trim()
  if (!trimmed) {
    throw new Error('Route target is required.')
  }

  const splitTarget = splitNotificationRouteTarget(trimmed)
  if (!splitTarget || !splitTarget.prefix) {
    throw new Error(`Saved route target must use \`<kind>:<channel-target>\` format for \`${routeKind}\` routes.`)
  }

  if (splitTarget.prefix !== routeKind) {
    throw new Error(
      `Saved route target prefix \`${splitTarget.prefix}\` does not match route kind \`${routeKind}\`.`,
    )
  }

  if (!splitTarget.channelTarget) {
    throw new Error('Saved route target channel segment is empty.')
  }

  return {
    channelTarget: splitTarget.channelTarget,
    canonicalTarget: `${routeKind}:${splitTarget.channelTarget}`,
  }
}

const notificationRouteMetadataSchema = z
  .string()
  .trim()
  .min(1)
  .refine((value) => {
    try {
      const parsed = JSON.parse(value)
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    } catch {
      return false
    }
  }, 'Notification route metadata must be a JSON object string.')

export const notificationRouteCredentialReadinessStatusSchema = z.enum([
  'ready',
  'missing',
  'malformed',
  'unavailable',
])

export const notificationRouteCredentialReadinessDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()

export const notificationRouteCredentialReadinessSchema = z
  .object({
    hasBotToken: z.boolean(),
    hasChatId: z.boolean(),
    hasWebhookUrl: z.boolean(),
    ready: z.boolean(),
    status: notificationRouteCredentialReadinessStatusSchema,
    diagnostic: notificationRouteCredentialReadinessDiagnosticSchema.nullable().optional(),
  })
  .strict()
  .superRefine((readiness, ctx) => {
    if (readiness.status === 'ready') {
      if (!readiness.ready) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['ready'],
          message: 'Credential-readiness rows with `status=ready` must set `ready=true`.',
        })
      }

      if (readiness.diagnostic) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['diagnostic'],
          message: 'Credential-readiness rows with `status=ready` must not include diagnostics.',
        })
      }

      return
    }

    if (readiness.ready) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['ready'],
        message: 'Credential-readiness rows with non-ready status must set `ready=false`.',
      })
    }

    if (!readiness.diagnostic) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostic'],
        message:
          'Credential-readiness rows with non-ready status must include typed diagnostics.',
      })
    }
  })

export const notificationRouteSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1),
    enabled: z.boolean(),
    metadataJson: notificationRouteMetadataSchema.nullable().optional(),
    credentialReadiness: notificationRouteCredentialReadinessSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

const notificationCorrelationKeySchema = z
  .string()
  .trim()
  .regex(
    notificationCorrelationKeyPattern,
    'Correlation keys must match `nfy:<32 lowercase hex>` for deterministic reply correlation.',
  )

export const notificationDispatchSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    status: notificationDispatchStatusSchema,
    attemptCount: z.number().int().nonnegative(),
    lastAttemptAt: optionalIsoTimestampSchema,
    deliveredAt: optionalIsoTimestampSchema,
    claimedAt: optionalIsoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastErrorMessage: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((dispatch, ctx) => {
    const hasAnyFailureDiagnostic = Boolean(dispatch.lastErrorCode || dispatch.lastErrorMessage)
    const hasFailureDiagnosticPair = Boolean(dispatch.lastErrorCode && dispatch.lastErrorMessage)

    if (hasAnyFailureDiagnostic && !hasFailureDiagnosticPair) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastErrorCode'],
        message: 'Notification dispatch rows must include both `lastErrorCode` and `lastErrorMessage` when diagnostics are present.',
      })
    }

    if ((dispatch.status === 'sent' || dispatch.status === 'failed') && dispatch.attemptCount < 1) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptCount'],
        message: 'Sent/failed notification dispatch rows must include a positive `attemptCount`.',
      })
    }

    if ((dispatch.status === 'sent' || dispatch.status === 'failed') && !dispatch.lastAttemptAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastAttemptAt'],
        message: 'Sent/failed notification dispatch rows must include `lastAttemptAt`.',
      })
    }

    switch (dispatch.status) {
      case 'pending':
        if (dispatch.deliveredAt || dispatch.claimedAt || hasAnyFailureDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Pending notification dispatch rows must not include delivery, claim, or failure diagnostics.',
          })
        }
        break
      case 'sent':
        if (!dispatch.deliveredAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['deliveredAt'],
            message: 'Sent notification dispatch rows must include `deliveredAt`.',
          })
        }
        if (dispatch.claimedAt || hasAnyFailureDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Sent notification dispatch rows must not include claim or failure diagnostics.',
          })
        }
        break
      case 'failed':
        if (!hasFailureDiagnosticPair) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['lastErrorCode'],
            message: 'Failed notification dispatch rows must include non-empty `lastErrorCode` and `lastErrorMessage`.',
          })
        }
        if (dispatch.deliveredAt || dispatch.claimedAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['status'],
            message: 'Failed notification dispatch rows must not include delivery or claim timestamps.',
          })
        }
        break
      case 'claimed':
        if (!dispatch.claimedAt) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['claimedAt'],
            message: 'Claimed notification dispatch rows must include `claimedAt`.',
          })
        }
        break
    }
  })

export const notificationReplyClaimSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    responderId: nonEmptyOptionalTextSchema,
    status: notificationReplyClaimStatusSchema,
    rejectionCode: nonEmptyOptionalTextSchema,
    rejectionMessage: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((claim, ctx) => {
    if (claim.status === 'accepted') {
      if (claim.rejectionCode || claim.rejectionMessage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'Accepted notification reply claims must not include rejection diagnostics.',
        })
      }
      return
    }

    if (!claim.rejectionCode || !claim.rejectionMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['rejectionCode'],
        message: 'Rejected notification reply claims must include non-empty `rejectionCode` and `rejectionMessage`.',
      })
    }
  })

export const listNotificationRoutesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const listNotificationRoutesResponseSchema = z
  .object({
    routes: z.array(notificationRouteSchema),
  })
  .strict()
  .superRefine((response, ctx) => {
    response.routes.forEach((route, index) => {
      const readiness = route.credentialReadiness
      if (!readiness) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness'],
          message:
            'List-notification-routes responses must include redacted `credentialReadiness` metadata for every route.',
        })
        return
      }

      if (route.routeKind === 'telegram') {
        if (readiness.hasWebhookUrl) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['routes', index, 'credentialReadiness', 'hasWebhookUrl'],
            message: 'Telegram readiness rows must not report `hasWebhookUrl=true`.',
          })
        }

        if (readiness.ready && (!readiness.hasBotToken || !readiness.hasChatId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['routes', index, 'credentialReadiness', 'ready'],
            message:
              'Telegram readiness rows can set `ready=true` only when both `hasBotToken` and `hasChatId` are true.',
          })
        }

        return
      }

      if (readiness.hasChatId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness', 'hasChatId'],
          message: 'Discord readiness rows must not report `hasChatId=true`.',
        })
      }

      if (readiness.ready && (!readiness.hasWebhookUrl || !readiness.hasBotToken)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routes', index, 'credentialReadiness', 'ready'],
          message:
            'Discord readiness rows can set `ready=true` only when both `hasWebhookUrl` and `hasBotToken` are true.',
        })
      }
    })
  })

export const upsertNotificationRouteRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1),
    enabled: z.boolean(),
    metadataJson: notificationRouteMetadataSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const upsertNotificationRouteResponseSchema = z
  .object({
    route: notificationRouteSchema,
  })
  .strict()

export const notificationRouteCredentialPayloadSchema = z
  .object({
    botToken: nonEmptyOptionalTextSchema,
    chatId: nonEmptyOptionalTextSchema,
    webhookUrl: nonEmptyOptionalTextSchema,
  })
  .strict()

export const upsertNotificationRouteCredentialsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    credentials: notificationRouteCredentialPayloadSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.routeKind === 'telegram') {
      if (!request.credentials.botToken) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'botToken'],
          message: 'Telegram credentials require non-empty `botToken`.',
        })
      }

      if (!request.credentials.chatId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'chatId'],
          message: 'Telegram credentials require non-empty `chatId`.',
        })
      }

      if (request.credentials.webhookUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'webhookUrl'],
          message: 'Telegram credentials must not include `webhookUrl`.',
        })
      }

      return
    }

    if (!request.credentials.webhookUrl) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentials', 'webhookUrl'],
        message: 'Discord credentials require non-empty `webhookUrl`.',
      })
    } else {
      try {
        const parsed = new URL(request.credentials.webhookUrl)
        if (parsed.protocol !== 'https:') {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['credentials', 'webhookUrl'],
            message: 'Discord credentials require an HTTPS `webhookUrl`.',
          })
        }
      } catch {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentials', 'webhookUrl'],
          message: 'Discord credentials require a valid URL `webhookUrl`.',
        })
      }
    }

    if (request.credentials.chatId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentials', 'chatId'],
        message: 'Discord credentials must not include `chatId`.',
      })
    }
  })

export const upsertNotificationRouteCredentialsResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: notificationRouteKindSchema,
    credentialScope: z.literal('app_local'),
    hasBotToken: z.boolean(),
    hasChatId: z.boolean(),
    hasWebhookUrl: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.routeKind === 'telegram') {
      if (!response.hasBotToken || !response.hasChatId || response.hasWebhookUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['routeKind'],
          message:
            'Telegram credential acknowledgements must indicate bot token + chat id and no webhook URL.',
        })
      }
      return
    }

    if (!response.hasWebhookUrl || response.hasChatId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['routeKind'],
        message: 'Discord credential acknowledgements must indicate webhook URL and no chat id.',
      })
    }
  })

export const listNotificationDispatchesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const listNotificationDispatchesResponseSchema = z
  .object({
    dispatches: z.array(notificationDispatchSchema),
  })
  .strict()

export const recordNotificationDispatchOutcomeRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    status: notificationDispatchOutcomeStatusSchema,
    attemptedAt: isoTimestampSchema,
    errorCode: nonEmptyOptionalTextSchema,
    errorMessage: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.status === 'sent') {
      if (request.errorCode || request.errorMessage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['errorCode'],
          message: 'Sent notification dispatch outcomes must not include error diagnostics.',
        })
      }
      return
    }

    if (!request.errorCode || !request.errorMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['errorCode'],
        message: 'Failed notification dispatch outcomes must include non-empty `errorCode` and `errorMessage`.',
      })
    }
  })

export const recordNotificationDispatchOutcomeResponseSchema = z
  .object({
    dispatch: notificationDispatchSchema,
  })
  .strict()

export const submitNotificationReplyRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    correlationKey: notificationCorrelationKeySchema,
    responderId: nonEmptyOptionalTextSchema,
    replyText: z.string().trim().min(1),
    decision: z.enum(['approve', 'reject']),
    receivedAt: isoTimestampSchema,
  })
  .strict()

export const submitNotificationReplyResponseSchema = z
  .object({
    claim: notificationReplyClaimSchema,
    dispatch: notificationDispatchSchema,
    resolveResult: resolveOperatorActionResponseSchema,
    resumeResult: z.lazy(() => resumeOperatorRunResponseSchema).nullable().optional(),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.claim.projectId !== response.dispatch.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'projectId'],
        message: 'Notification reply response claim and dispatch must reference the same project.',
      })
    }

    if (response.claim.actionId !== response.dispatch.actionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'actionId'],
        message: 'Notification reply response claim and dispatch must reference the same action.',
      })
    }

    if (response.claim.routeId !== response.dispatch.routeId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'routeId'],
        message: 'Notification reply response claim and dispatch must reference the same route.',
      })
    }

    if (response.claim.correlationKey !== response.dispatch.correlationKey) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['claim', 'correlationKey'],
        message: 'Notification reply response claim and dispatch must reference the same correlation key.',
      })
    }

    if (response.claim.status === 'accepted' && response.dispatch.status !== 'claimed') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['dispatch', 'status'],
        message: 'Accepted notification reply responses must return a claimed notification dispatch row.',
      })
    }

    if (response.resolveResult.approvalRequest.actionId !== response.claim.actionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resolveResult', 'approvalRequest', 'actionId'],
        message: 'Notification reply resolve results must reference the same action id as the claim.',
      })
    }

    const resolvedStatus = response.resolveResult.approvalRequest.status
    if (resolvedStatus === 'approved' && !response.resumeResult) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult'],
        message: 'Approved notification reply decisions must include a resume result payload.',
      })
    }

    if (resolvedStatus === 'rejected' && response.resumeResult) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult'],
        message: 'Rejected notification reply decisions must not include a resume result payload.',
      })
    }

    if (
      response.resumeResult
      && response.resumeResult.approvalRequest.actionId !== response.claim.actionId
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resumeResult', 'approvalRequest', 'actionId'],
        message: 'Notification reply resume results must reference the same action id as the claim.',
      })
    }
  })

const notificationAdapterErrorCountSchema = z
  .object({
    code: z.string().trim().min(1),
    count: z.number().int().nonnegative(),
  })
  .strict()

export const notificationAdapterDispatchAttemptSchema = z
  .object({
    dispatchId: z.number().int().nonnegative(),
    actionId: z.string().trim().min(1),
    routeId: z.string().trim().min(1),
    routeKind: z.string().trim().min(1),
    outcomeStatus: notificationDispatchStatusSchema,
    diagnosticCode: z.string().trim().min(1),
    diagnosticMessage: z.string().trim().min(1),
    durableErrorCode: nonEmptyOptionalTextSchema,
    durableErrorMessage: nonEmptyOptionalTextSchema,
  })
  .strict()

export const notificationDispatchCycleSummarySchema = z
  .object({
    projectId: z.string().trim().min(1),
    pendingCount: z.number().int().nonnegative(),
    attemptedCount: z.number().int().nonnegative(),
    sentCount: z.number().int().nonnegative(),
    failedCount: z.number().int().nonnegative(),
    attemptLimit: z.number().int().positive(),
    attemptsTruncated: z.boolean(),
    attempts: z.array(notificationAdapterDispatchAttemptSchema),
    errorCodeCounts: z.array(notificationAdapterErrorCountSchema),
  })
  .strict()
  .superRefine((summary, ctx) => {
    if (summary.attemptedCount < summary.attempts.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptedCount'],
        message: 'Notification dispatch cycle summaries cannot report fewer attempts than included attempt rows.',
      })
    }

    if (summary.attempts.length > summary.attemptLimit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification dispatch cycle summaries cannot include attempt rows beyond `attemptLimit`.',
      })
    }

    if (summary.sentCount + summary.failedCount > summary.attemptedCount) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attemptedCount'],
        message: 'Notification dispatch cycle sent/failed counts cannot exceed attempted count.',
      })
    }
  })

export const notificationAdapterReplyAttemptSchema = z
  .object({
    routeId: z.string().trim().min(1),
    routeKind: z.string().trim().min(1),
    actionId: nonEmptyOptionalTextSchema,
    messageId: nonEmptyOptionalTextSchema,
    accepted: z.boolean(),
    diagnosticCode: z.string().trim().min(1),
    diagnosticMessage: z.string().trim().min(1),
    replyCode: nonEmptyOptionalTextSchema,
    replyMessage: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((attempt, ctx) => {
    if (attempt.accepted && (attempt.replyCode || attempt.replyMessage)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['accepted'],
        message: 'Accepted reply attempts must not include reply rejection diagnostics.',
      })
    }

    const hasReplyCode = Boolean(attempt.replyCode)
    const hasReplyMessage = Boolean(attempt.replyMessage)
    if (hasReplyCode !== hasReplyMessage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['replyCode'],
        message: 'Reply diagnostics must include both `replyCode` and `replyMessage` when present.',
      })
    }
  })

export const notificationReplyCycleSummarySchema = z
  .object({
    projectId: z.string().trim().min(1),
    routeCount: z.number().int().nonnegative(),
    polledRouteCount: z.number().int().nonnegative(),
    messageCount: z.number().int().nonnegative(),
    acceptedCount: z.number().int().nonnegative(),
    rejectedCount: z.number().int().nonnegative(),
    attemptLimit: z.number().int().positive(),
    attemptsTruncated: z.boolean(),
    attempts: z.array(notificationAdapterReplyAttemptSchema),
    errorCodeCounts: z.array(notificationAdapterErrorCountSchema),
  })
  .strict()
  .superRefine((summary, ctx) => {
    if (summary.polledRouteCount > summary.routeCount) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['polledRouteCount'],
        message: 'Polled notification route count cannot exceed the total route count.',
      })
    }

    if (summary.attempts.length > summary.attemptLimit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification reply cycle summaries cannot include attempt rows beyond `attemptLimit`.',
      })
    }

    if (summary.acceptedCount + summary.rejectedCount < summary.attempts.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attempts'],
        message: 'Notification reply cycle attempt rows cannot exceed accepted + rejected totals.',
      })
    }
  })

export const syncNotificationAdaptersRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()

export const syncNotificationAdaptersResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    dispatch: notificationDispatchCycleSummarySchema,
    replies: notificationReplyCycleSummarySchema,
    syncedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.dispatch.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['dispatch', 'projectId'],
        message: 'Notification dispatch cycle summaries must match the sync response project id.',
      })
    }

    if (response.replies.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['replies', 'projectId'],
        message: 'Notification reply cycle summaries must match the sync response project id.',
      })
    }
  })

export type NotificationRouteKindDto = z.infer<typeof notificationRouteKindSchema>
export type NotificationDispatchStatusDto = z.infer<typeof notificationDispatchStatusSchema>
export type NotificationReplyClaimStatusDto = z.infer<typeof notificationReplyClaimStatusSchema>
export type NotificationDispatchOutcomeStatusDto = z.infer<typeof notificationDispatchOutcomeStatusSchema>
export type NotificationRouteCredentialReadinessStatusDto = z.infer<
  typeof notificationRouteCredentialReadinessStatusSchema
>
export type NotificationRouteCredentialReadinessDiagnosticDto = z.infer<
  typeof notificationRouteCredentialReadinessDiagnosticSchema
>
export type NotificationRouteCredentialReadinessDto = z.infer<
  typeof notificationRouteCredentialReadinessSchema
>
export type NotificationRouteDto = z.infer<typeof notificationRouteSchema>
export type NotificationDispatchDto = z.infer<typeof notificationDispatchSchema>
export type NotificationReplyClaimDto = z.infer<typeof notificationReplyClaimSchema>
export type NotificationAdapterErrorCountDto = z.infer<typeof notificationAdapterErrorCountSchema>
export type NotificationAdapterDispatchAttemptDto = z.infer<typeof notificationAdapterDispatchAttemptSchema>
export type NotificationDispatchCycleSummaryDto = z.infer<typeof notificationDispatchCycleSummarySchema>
export type NotificationAdapterReplyAttemptDto = z.infer<typeof notificationAdapterReplyAttemptSchema>
export type NotificationReplyCycleSummaryDto = z.infer<typeof notificationReplyCycleSummarySchema>
export type ListNotificationRoutesRequestDto = z.infer<typeof listNotificationRoutesRequestSchema>
export type ListNotificationRoutesResponseDto = z.infer<typeof listNotificationRoutesResponseSchema>
export type UpsertNotificationRouteRequestDto = z.infer<typeof upsertNotificationRouteRequestSchema>
export type UpsertNotificationRouteResponseDto = z.infer<typeof upsertNotificationRouteResponseSchema>
export type NotificationRouteCredentialPayloadDto = z.infer<typeof notificationRouteCredentialPayloadSchema>
export type UpsertNotificationRouteCredentialsRequestDto = z.infer<
  typeof upsertNotificationRouteCredentialsRequestSchema
>
export type UpsertNotificationRouteCredentialsResponseDto = z.infer<
  typeof upsertNotificationRouteCredentialsResponseSchema
>
export type ListNotificationDispatchesRequestDto = z.infer<typeof listNotificationDispatchesRequestSchema>
export type ListNotificationDispatchesResponseDto = z.infer<typeof listNotificationDispatchesResponseSchema>
export type RecordNotificationDispatchOutcomeRequestDto = z.infer<typeof recordNotificationDispatchOutcomeRequestSchema>
export type RecordNotificationDispatchOutcomeResponseDto = z.infer<typeof recordNotificationDispatchOutcomeResponseSchema>
export type SubmitNotificationReplyRequestDto = z.infer<typeof submitNotificationReplyRequestSchema>
export type SubmitNotificationReplyResponseDto = z.infer<typeof submitNotificationReplyResponseSchema>
export type SyncNotificationAdaptersRequestDto = z.infer<typeof syncNotificationAdaptersRequestSchema>
export type SyncNotificationAdaptersResponseDto = z.infer<typeof syncNotificationAdaptersResponseSchema>

export interface NotificationDispatchView {
  id: number
  projectId: string
  actionId: string
  routeId: string
  correlationKey: string
  status: NotificationDispatchStatusDto
  statusLabel: string
  attemptCount: number
  lastAttemptAt: string | null
  deliveredAt: string | null
  claimedAt: string | null
  lastErrorCode: string | null
  lastErrorMessage: string | null
  createdAt: string
  updatedAt: string
  isPending: boolean
  isSent: boolean
  isFailed: boolean
  isClaimed: boolean
  hasFailureDiagnostics: boolean
}

export interface NotificationBrokerActionView {
  actionId: string
  dispatches: NotificationDispatchView[]
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  hasFailures: boolean
  hasPending: boolean
  hasClaimed: boolean
}

export interface NotificationBrokerRouteView {
  routeId: string
  dispatches: NotificationDispatchView[]
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  latestFailureAt: string | null
  latestFailureCode: string | null
  latestFailureMessage: string | null
  hasFailures: boolean
  hasPending: boolean
}

export interface NotificationBrokerView {
  dispatches: NotificationDispatchView[]
  actions: NotificationBrokerActionView[]
  routes: NotificationBrokerRouteView[]
  byActionId: Record<string, NotificationBrokerActionView>
  byRouteId: Record<string, NotificationBrokerRouteView>
  dispatchCount: number
  routeCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestUpdatedAt: string | null
  isTruncated: boolean
  totalBeforeTruncation: number
}

function getNotificationDispatchStatusLabel(status: NotificationDispatchStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending dispatch'
    case 'sent':
      return 'Delivered'
    case 'failed':
      return 'Delivery failed'
    case 'claimed':
      return 'Reply claimed'
  }
}

function timestampToSortValue(value: string | null): number {
  if (!value) {
    return Number.NEGATIVE_INFINITY
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : Number.NEGATIVE_INFINITY
}

function sortDispatchesByRecency(
  left: { updatedAt: string; createdAt: string },
  right: { updatedAt: string; createdAt: string },
): number {
  const rightUpdatedAt = timestampToSortValue(right.updatedAt)
  const leftUpdatedAt = timestampToSortValue(left.updatedAt)
  if (rightUpdatedAt !== leftUpdatedAt) {
    return rightUpdatedAt - leftUpdatedAt
  }

  return timestampToSortValue(right.createdAt) - timestampToSortValue(left.createdAt)
}

export function mapNotificationDispatch(dispatch: NotificationDispatchDto): NotificationDispatchView {
  const lastErrorCode = normalizeOptionalText(dispatch.lastErrorCode)
  const lastErrorMessage = normalizeOptionalText(dispatch.lastErrorMessage)

  return {
    id: dispatch.id,
    projectId: dispatch.projectId,
    actionId: dispatch.actionId,
    routeId: dispatch.routeId,
    correlationKey: dispatch.correlationKey,
    status: dispatch.status,
    statusLabel: getNotificationDispatchStatusLabel(dispatch.status),
    attemptCount: dispatch.attemptCount,
    lastAttemptAt: normalizeOptionalText(dispatch.lastAttemptAt),
    deliveredAt: normalizeOptionalText(dispatch.deliveredAt),
    claimedAt: normalizeOptionalText(dispatch.claimedAt),
    lastErrorCode,
    lastErrorMessage,
    createdAt: dispatch.createdAt,
    updatedAt: dispatch.updatedAt,
    isPending: dispatch.status === 'pending',
    isSent: dispatch.status === 'sent',
    isFailed: dispatch.status === 'failed',
    isClaimed: dispatch.status === 'claimed',
    hasFailureDiagnostics: Boolean(lastErrorCode && lastErrorMessage),
  }
}

function createEmptyNotificationBrokerView(): NotificationBrokerView {
  return {
    dispatches: [],
    actions: [],
    routes: [],
    byActionId: {},
    byRouteId: {},
    dispatchCount: 0,
    routeCount: 0,
    pendingCount: 0,
    sentCount: 0,
    failedCount: 0,
    claimedCount: 0,
    latestUpdatedAt: null,
    isTruncated: false,
    totalBeforeTruncation: 0,
  }
}

function countNotificationDispatchStatuses(dispatches: NotificationDispatchView[]) {
  return dispatches.reduce(
    (acc, dispatch) => {
      if (dispatch.isPending) {
        acc.pendingCount += 1
      } else if (dispatch.isSent) {
        acc.sentCount += 1
      } else if (dispatch.isFailed) {
        acc.failedCount += 1
      } else if (dispatch.isClaimed) {
        acc.claimedCount += 1
      }

      return acc
    },
    {
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
    },
  )
}

export function mapNotificationBrokerRouteDiagnostics(dispatches: NotificationDispatchView[]): {
  routes: NotificationBrokerRouteView[]
  byRouteId: Record<string, NotificationBrokerRouteView>
} {
  const groupedByRoute = new Map<string, NotificationDispatchView[]>()

  dispatches.forEach((dispatch) => {
    const routeId = dispatch.routeId.trim()
    const existing = groupedByRoute.get(routeId)
    if (existing) {
      existing.push(dispatch)
      return
    }

    groupedByRoute.set(routeId, [dispatch])
  })

  const routes = Array.from(groupedByRoute.entries())
    .map(([routeId, routeDispatches]): NotificationBrokerRouteView => {
      const sortedRouteDispatches = [...routeDispatches].sort(sortDispatchesByRecency)
      const statusCounts = countNotificationDispatchStatuses(sortedRouteDispatches)
      const latestFailedDispatch =
        sortedRouteDispatches.find((dispatch) => dispatch.isFailed && dispatch.hasFailureDiagnostics) ?? null

      return {
        routeId,
        dispatches: sortedRouteDispatches,
        dispatchCount: sortedRouteDispatches.length,
        pendingCount: statusCounts.pendingCount,
        sentCount: statusCounts.sentCount,
        failedCount: statusCounts.failedCount,
        claimedCount: statusCounts.claimedCount,
        latestUpdatedAt: sortedRouteDispatches[0]?.updatedAt ?? null,
        latestFailureAt:
          latestFailedDispatch?.updatedAt ?? latestFailedDispatch?.lastAttemptAt ?? latestFailedDispatch?.createdAt ?? null,
        latestFailureCode: latestFailedDispatch?.lastErrorCode ?? null,
        latestFailureMessage: latestFailedDispatch?.lastErrorMessage ?? null,
        hasFailures: statusCounts.failedCount > 0,
        hasPending: statusCounts.pendingCount > 0,
      }
    })
    .sort((left, right) => {
      const byRecency = sortDispatchesByRecency(
        {
          updatedAt: left.latestUpdatedAt ?? '',
          createdAt: left.dispatches[0]?.createdAt ?? '',
        },
        {
          updatedAt: right.latestUpdatedAt ?? '',
          createdAt: right.dispatches[0]?.createdAt ?? '',
        },
      )

      if (byRecency !== 0) {
        return byRecency
      }

      return left.routeId.localeCompare(right.routeId)
    })

  const byRouteId = routes.reduce<Record<string, NotificationBrokerRouteView>>((acc, route) => {
    acc[route.routeId] = route
    return acc
  }, {})

  return {
    routes,
    byRouteId,
  }
}

export function mapNotificationBroker(
  projectId: string,
  dispatches: NotificationDispatchDto[],
): NotificationBrokerView {
  const inScopeDispatches = dispatches
    .filter((dispatch) => dispatch.projectId === projectId)
    .sort(sortDispatchesByRecency)
  if (inScopeDispatches.length === 0) {
    return createEmptyNotificationBrokerView()
  }

  const totalBeforeTruncation = inScopeDispatches.length
  const boundedDispatches = inScopeDispatches.slice(0, MAX_NOTIFICATION_BROKER_DISPATCHES)
  const dispatchViews = boundedDispatches.map(mapNotificationDispatch)

  const statusCounts = countNotificationDispatchStatuses(dispatchViews)

  const groupedByAction = new Map<string, NotificationDispatchView[]>()
  dispatchViews.forEach((dispatch) => {
    const existing = groupedByAction.get(dispatch.actionId)
    if (existing) {
      existing.push(dispatch)
      return
    }

    groupedByAction.set(dispatch.actionId, [dispatch])
  })

  const actions = Array.from(groupedByAction.entries())
    .map(([actionId, actionDispatches]): NotificationBrokerActionView => {
      const sortedActionDispatches = [...actionDispatches].sort(sortDispatchesByRecency)
      const actionStatusCounts = countNotificationDispatchStatuses(sortedActionDispatches)

      return {
        actionId,
        dispatches: sortedActionDispatches,
        dispatchCount: sortedActionDispatches.length,
        pendingCount: actionStatusCounts.pendingCount,
        sentCount: actionStatusCounts.sentCount,
        failedCount: actionStatusCounts.failedCount,
        claimedCount: actionStatusCounts.claimedCount,
        latestUpdatedAt: sortedActionDispatches[0]?.updatedAt ?? null,
        hasFailures: actionStatusCounts.failedCount > 0,
        hasPending: actionStatusCounts.pendingCount > 0,
        hasClaimed: actionStatusCounts.claimedCount > 0,
      }
    })
    .sort((left, right) =>
      sortDispatchesByRecency(
        {
          updatedAt: left.latestUpdatedAt ?? '',
          createdAt: left.dispatches[0]?.createdAt ?? '',
        },
        {
          updatedAt: right.latestUpdatedAt ?? '',
          createdAt: right.dispatches[0]?.createdAt ?? '',
        },
      ),
    )

  const byActionId = actions.reduce<Record<string, NotificationBrokerActionView>>((acc, action) => {
    acc[action.actionId] = action
    return acc
  }, {})

  const routeDiagnostics = mapNotificationBrokerRouteDiagnostics(dispatchViews)

  return {
    dispatches: dispatchViews,
    actions,
    routes: routeDiagnostics.routes,
    byActionId,
    byRouteId: routeDiagnostics.byRouteId,
    dispatchCount: dispatchViews.length,
    routeCount: routeDiagnostics.routes.length,
    pendingCount: statusCounts.pendingCount,
    sentCount: statusCounts.sentCount,
    failedCount: statusCounts.failedCount,
    claimedCount: statusCounts.claimedCount,
    latestUpdatedAt: dispatchViews[0]?.updatedAt ?? null,
    isTruncated: totalBeforeTruncation > dispatchViews.length,
    totalBeforeTruncation,
  }
}
