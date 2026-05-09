import { describe, expect, it } from 'vitest'
import {
  deleteProjectContextRecordRequestSchema,
  deleteProjectContextRecordResponseSchema,
  supersedeProjectContextRecordRequestSchema,
  supersedeProjectContextRecordResponseSchema,
} from './project-records'

describe('project record correction command contracts', () => {
  it('validates redaction-safe delete and supersede command payloads', () => {
    const request = deleteProjectContextRecordRequestSchema.parse({
      projectId: 'project-record-correction',
      recordId: 'record-stale-fact',
    })
    expect(request.recordId).toBe('record-stale-fact')

    const response = deleteProjectContextRecordResponseSchema.parse({
      schema: 'xero.project_context_record_delete_command.v1',
      projectId: request.projectId,
      recordId: request.recordId,
      retrievalRemoved: true,
      uiDeferred: true,
    })
    expect(response.retrievalRemoved).toBe(true)
    expect(JSON.stringify(response)).not.toContain('/Users/')

    const supersedeRequest = supersedeProjectContextRecordRequestSchema.parse({
      projectId: request.projectId,
      supersededRecordId: 'record-stale-fact',
      supersedingRecordId: 'record-corrected-fact',
    })
    const supersedeResponse = supersedeProjectContextRecordResponseSchema.parse({
      schema: 'xero.project_context_record_supersede_command.v1',
      projectId: supersedeRequest.projectId,
      supersededRecordId: supersedeRequest.supersededRecordId,
      supersedingRecordId: supersedeRequest.supersedingRecordId,
      retrievalChanged: true,
      uiDeferred: true,
    })
    expect(supersedeResponse.retrievalChanged).toBe(true)
    expect(JSON.stringify(supersedeResponse)).not.toContain('/Users/')
    expect(() =>
      supersedeProjectContextRecordRequestSchema.parse({
        projectId: request.projectId,
        supersededRecordId: 'record-same',
        supersedingRecordId: 'record-same',
      }),
    ).toThrow(/distinct/)
    expect(() =>
      supersedeProjectContextRecordResponseSchema.parse({
        ...supersedeResponse,
        supersedingRecordId: supersedeResponse.supersededRecordId,
      }),
    ).toThrow(/distinct/)
  })
})
