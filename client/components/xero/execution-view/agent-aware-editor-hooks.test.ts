import { describe, expect, it } from 'vitest'
import type { RuntimeStreamActivityItemView, RuntimeStreamToolItemView } from '@/src/lib/xero-model'
import {
  buildEditorAgentActivities,
  buildEditorAgentContextRequest,
  normalizeAgentActivityPath,
  parseAgentFileActivityDetail,
  truncatePromptText,
} from './agent-aware-editor-hooks'

function fileChangeItem(overrides: Partial<RuntimeStreamActivityItemView> = {}): RuntimeStreamActivityItemView {
  return {
    id: 'activity:run-1:1',
    kind: 'activity',
    runId: 'run-1',
    sequence: 1,
    createdAt: '2026-05-01T12:00:00Z',
    code: 'owned_agent_file_changed',
    title: 'File changed',
    text: null,
    detail: 'modify: src/app.ts',
    codeChangeGroupId: 'code-change-1',
    codeWorkspaceEpoch: 7,
    codePatchAvailability: null,
    ...overrides,
  }
}

function toolItem(overrides: Partial<RuntimeStreamToolItemView> = {}): RuntimeStreamToolItemView {
  return {
    id: 'tool:run-1:1',
    kind: 'tool',
    runId: 'run-1',
    sequence: 2,
    createdAt: '2026-05-01T12:00:01Z',
    toolCallId: 'tool-call-1',
    toolName: 'edit',
    toolState: 'running',
    detail: 'write: src/app.ts',
    toolSummary: null,
    toolResultPreview: null,
    codeChangeGroupId: null,
    codeWorkspaceEpoch: null,
    codePatchAvailability: null,
    ...overrides,
  }
}

describe('agent-aware editor hooks', () => {
  it('normalizes agent activity paths to editor project paths', () => {
    expect(normalizeAgentActivityPath('src/app.ts')).toBe('/src/app.ts')
    expect(normalizeAgentActivityPath('./src/app.ts')).toBe('/src/app.ts')
    expect(normalizeAgentActivityPath('b/src/app.ts')).toBe('/src/app.ts')
    expect(normalizeAgentActivityPath('../secret')).toBeNull()
  })

  it('parses changed and renamed file activity details', () => {
    expect(parseAgentFileActivityDetail('modify: src/app.ts · 12 lines')).toEqual({
      operation: 'modify',
      paths: ['/src/app.ts'],
    })
    expect(parseAgentFileActivityDetail('rename: src/old.ts -> src/new.ts')).toEqual({
      operation: 'rename',
      paths: ['/src/old.ts', '/src/new.ts'],
    })
  })

  it('builds recent and active agent activities from runtime stream items', () => {
    const activities = buildEditorAgentActivities([
      {
        paneId: 'pane-1',
        sessionTitle: 'Main session',
        runtimeStreamItems: [
          fileChangeItem({
            detail: 'modify: src/app.ts',
            codePatchAvailability: {
              projectId: 'project-1',
              targetChangeGroupId: 'code-change-1',
              available: true,
              affectedPaths: ['src/app.ts'],
              fileChangeCount: 1,
              textHunkCount: 1,
              textHunks: [
                {
                  hunkId: 'hunk-1',
                  patchFileId: 'patch-file-1',
                  filePath: 'src/app.ts',
                  hunkIndex: 0,
                  baseStartLine: 1,
                  baseLineCount: 1,
                  resultStartLine: 1,
                  resultLineCount: 2,
                },
              ],
              unavailableReason: null,
            },
          }),
          toolItem(),
        ],
      },
    ])

    expect(activities.map((activity) => activity.status)).toEqual(['active', 'recent'])
    expect(activities[0]).toMatchObject({
      path: '/src/app.ts',
      sessionTitle: 'Main session',
      status: 'active',
    })
    expect(activities[1]?.patchAvailability?.textHunks[0]?.hunkId).toBe('hunk-1')
  })

  it('builds prompts with selection and dirty draft context', () => {
    const request = buildEditorAgentContextRequest({
      kind: 'ask_selection',
      path: '/src/app.ts',
      content: 'const value = 1\n',
      savedContent: 'const value = 0\n',
      isDirty: true,
      selection: {
        text: 'value',
        fromLine: 1,
        fromColumn: 7,
        toLine: 1,
        toColumn: 12,
      },
    })

    expect(request.prompt).toContain('/src/app.ts')
    expect(request.prompt).toContain('unsaved changes')
    expect(request.prompt).toContain('Selection: lines 1:7 to 1:12')
  })

  it('truncates prompt payloads predictably', () => {
    expect(truncatePromptText('abcdef', 3)).toBe('abc\n\n[... 3 characters omitted ...]')
  })
})
