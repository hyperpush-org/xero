import { describe, expect, it } from 'vitest'
import {
  workspaceExplainResponseSchema,
  workspaceIndexResponseSchema,
  workspaceQueryResponseSchema,
} from './project'

const readyStatus = {
  projectId: 'project-1',
  state: 'ready',
  indexVersion: 1,
  rootPath: '/tmp/xero',
  storagePath: '/Users/alice/Library/Application Support/dev.sn0w.xero/projects/project-1/state.db',
  totalFiles: 3,
  indexedFiles: 3,
  skippedFiles: 0,
  staleFiles: 0,
  symbolCount: 4,
  indexedBytes: 4096,
  coveragePercent: 100,
  headSha: 'abc123',
  startedAt: '2026-05-04T12:00:00Z',
  completedAt: '2026-05-04T12:00:02Z',
  updatedAt: '2026-05-04T12:00:02Z',
  diagnostics: [
    {
      severity: 'info',
      code: 'workspace_index_ready',
      message: 'Semantic workspace index is ready.',
    },
  ],
}

describe('workspace index schemas', () => {
  it('accepts strict workspace index status, query, and explain contracts', () => {
    const indexed = workspaceIndexResponseSchema.parse({
      status: readyStatus,
      changedFiles: 2,
      unchangedFiles: 1,
      removedFiles: 0,
      durationMs: 512,
    })

    expect(indexed.status.storagePath).toContain('Application Support')
    expect(indexed.status.coveragePercent).toBe(100)

    const query = workspaceQueryResponseSchema.parse({
      projectId: 'project-1',
      query: 'runtime lifecycle',
      mode: 'semantic',
      resultCount: 1,
      stale: false,
      diagnostics: [],
      results: [
        {
          rank: 1,
          path: '/src/runtime/agent_core/environment_lifecycle.rs',
          score: 0.92,
          language: 'rust',
          summary: 'Environment lifecycle startup checks and health state.',
          snippet: 'Checking workspace index readiness.',
          symbols: ['EnvironmentLifecycleService'],
          imports: ['xero_agent_core'],
          tests: ['fake_provider_records_environment_lifecycle_before_provider_turn'],
          diffs: ['recently touched lifecycle checks'],
          failures: [],
          reasons: ['semantic embedding similarity', 'symbol match'],
          contentHash: 'sha256:abc123',
          indexedAt: '2026-05-04T12:00:02Z',
        },
      ],
    })

    expect(query.results[0]?.tests).toContain(
      'fake_provider_records_environment_lifecycle_before_provider_turn',
    )

    const explain = workspaceExplainResponseSchema.parse({
      projectId: 'project-1',
      summary: 'Top result matched semantic, symbol, and related-test signals.',
      status: readyStatus,
      topSignals: ['semantic embedding similarity', 'symbol match'],
      diagnostics: [],
    })

    expect(explain.topSignals).toEqual(['semantic embedding similarity', 'symbol match'])
  })

  it('rejects out-of-bounds workspace ranking and coverage values', () => {
    expect(() =>
      workspaceIndexResponseSchema.parse({
        status: {
          ...readyStatus,
          coveragePercent: 101,
        },
        changedFiles: 0,
        unchangedFiles: 0,
        removedFiles: 0,
        durationMs: 0,
      }),
    ).toThrow()

    expect(() =>
      workspaceQueryResponseSchema.parse({
        projectId: 'project-1',
        query: 'runtime lifecycle',
        mode: 'semantic',
        resultCount: 1,
        stale: false,
        diagnostics: [],
        results: [
          {
            rank: 0,
            path: '/src/runtime/agent_core/environment_lifecycle.rs',
            score: 1.2,
            language: 'rust',
            summary: 'Invalid result.',
            snippet: 'Invalid result.',
            symbols: [],
            imports: [],
            tests: [],
            diffs: [],
            failures: [],
            reasons: [],
            contentHash: 'sha256:abc123',
            indexedAt: '2026-05-04T12:00:02Z',
          },
        ],
      }),
    ).toThrow()
  })
})
