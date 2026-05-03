/** @vitest-environment jsdom */

import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { Profiler, useMemo, useState } from 'react'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterAll, describe, expect, it, vi } from 'vitest'

import { createBrowserResizeScheduler, type ViewportRect } from '@/components/xero/browser-resize-scheduler'
import { createEditorFrameScheduler } from '@/components/xero/code-editor'
import { flattenFileTreeRows } from '@/components/xero/file-tree'
import {
  createSearchIndex,
  filterSearchIndex,
  useDeferredFilterQuery,
} from '@/lib/input-priority'
import {
  DIFF_TOKENIZATION_BATCH_SIZE,
  createDiffPatchCache,
  createDiffTokenizationBatches,
  getDiffParsingStats,
  getDiffPatchCacheStats,
  parseDiffLines,
  parseDiffLinesForPatchKey,
  setCachedDiffPatch,
  VcsSidebar,
  type VcsSidebarProps,
} from '@/components/xero/vcs-sidebar'
import {
  Markdown,
  getMarkdownSegmentStats,
  resetMarkdownSegmentCacheForTests,
} from '@/components/xero/agent-runtime/conversation-markdown'
import {
  getShikiTokenCacheStats,
} from '@/lib/shiki'
import {
  getIpcPayloadBudgetMetrics,
  recordIpcPayloadSample,
  resetIpcPayloadBudgetMetricsForTests,
} from '@/src/lib/ipc-payload-budget'
import { createFrameCoalescer } from '@/lib/frame-governance'
import { calculateVirtualRange, getVirtualIndexes } from '@/lib/virtual-list'
import {
  applyProjectFileListing,
  createEmptyProjectFileTreeStore,
  getProjectFileTreeStoreStats,
  type FileSystemNode,
} from '@/src/lib/file-system-tree'
import {
  createRuntimeStreamEventBuffer,
  mergeRuntimeStreamEvents,
} from '@/src/features/xero/use-xero-desktop-state/runtime-stream'
import {
  createXeroHighChurnStore,
  useSelectorStoreValue,
  type XeroHighChurnStore,
} from '@/src/features/xero/use-xero-desktop-state/high-churn-store'
import {
  createRuntimeStreamView,
  estimateRuntimeStreamViewBytes,
  type RuntimeStreamEventDto,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'
import { estimateProviderModelCatalogBytes, type ProviderModelCatalogDto } from '@/src/lib/xero-model/provider-models'
import {
  createRepositoryStatusDiffRevision,
  type GitCommitResponseDto,
  type GitFetchResponseDto,
  type GitPullResponseDto,
  type GitPushResponseDto,
  type RepositoryDiffResponseDto,
  type RepositoryStatusView,
} from '@/src/lib/xero-model/project'

type ReplayMetricValue = number | string | boolean | null | ReplayMetricValue[] | {
  [key: string]: ReplayMetricValue
}

interface SmokeTaskTiming {
  name: string
  durationMs: number
}

interface SmokeReport {
  generatedAt: string
  replays: Record<string, ReplayMetricValue>
  slowestTasks: SmokeTaskTiming[]
}

const smokeReport: SmokeReport = {
  generatedAt: new Date().toISOString(),
  replays: {},
  slowestTasks: [],
}

async function measureReplay<T>(name: string, fn: () => T | Promise<T>): Promise<T> {
  const startedAt = performance.now()
  try {
    return await fn()
  } finally {
    smokeReport.slowestTasks.push({
      name,
      durationMs: Math.round((performance.now() - startedAt) * 100) / 100,
    })
  }
}

function makeRuntimeStreamEvent(sequence: number): RuntimeStreamEventDto {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind: 'transcript',
      runId: 'run-1',
      sequence,
      sessionId: 'session-1',
      flowId: 'flow-1',
      text: `chunk-${sequence}`,
      transcriptRole: 'assistant',
      toolCallId: null,
      toolName: null,
      toolState: null,
      toolSummary: null,
      skillId: null,
      skillStage: null,
      skillResult: null,
      skillSource: null,
      skillCacheStatus: null,
      skillDiagnostic: null,
      actionId: null,
      boundaryId: null,
      actionType: null,
      title: null,
      detail: null,
      code: null,
      message: null,
      retryable: null,
      createdAt: new Date(Date.UTC(2026, 4, 2, 12, 0, 0, sequence)).toISOString(),
    },
  }
}

function makeRuntimeStream(): RuntimeStreamView {
  return createRuntimeStreamView({
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    status: 'live',
  })
}

function makeRepositoryStatus(overrides: Partial<RepositoryStatusView> = {}): RepositoryStatusView {
  const { diffRevision, ...statusOverrides } = overrides
  const status: Omit<RepositoryStatusView, 'diffRevision'> = {
    projectId: 'project-1',
    repositoryId: 'repo-project-1',
    branchLabel: 'main',
    headShaLabel: 'abc1234',
    upstream: null,
    lastCommit: null,
    stagedCount: 0,
    unstagedCount: 1,
    untrackedCount: 0,
    statusCount: 1,
    additions: 2,
    deletions: 1,
    hasChanges: true,
    entries: [
      {
        path: 'file.txt',
        staged: null,
        unstaged: 'modified',
        untracked: false,
      },
    ],
    ...statusOverrides,
  }

  return {
    ...status,
    diffRevision: diffRevision ?? createRepositoryStatusDiffRevision(status),
  }
}

function installRect(
  node: HTMLElement,
  rect: Pick<DOMRect, 'height' | 'left' | 'top' | 'width'>,
) {
  Object.defineProperty(node, 'getBoundingClientRect', {
    configurable: true,
    value: () =>
      ({
        bottom: rect.top + rect.height,
        height: rect.height,
        left: rect.left,
        right: rect.left + rect.width,
        top: rect.top,
        width: rect.width,
        x: rect.left,
        y: rect.top,
        toJSON: () => ({}),
      }) as DOMRect,
  })
}

function createFrameController() {
  let nextId = 1
  const frames = new Map<number, FrameRequestCallback>()

  return {
    cancelFrame(id: number) {
      frames.delete(id)
    },
    flushFrame() {
      const [id, callback] = frames.entries().next().value ?? []
      if (!id || !callback) {
        throw new Error('No pending frame to flush')
      }
      frames.delete(id)
      callback(0)
    },
    get pendingCount() {
      return frames.size
    },
    requestFrame(callback: FrameRequestCallback) {
      const id = nextId
      nextId += 1
      frames.set(id, callback)
      return id
    },
  }
}

const repository = {
  id: 'repo-project-1',
  projectId: 'project-1',
  rootPath: '/tmp/project-1',
  displayName: 'Project 1',
  branch: 'main',
  headSha: 'abc1234',
  isGitRepo: true,
}

function makeDiff(patch: string): RepositoryDiffResponseDto {
  return {
    repository,
    scope: 'unstaged',
    patch,
    truncated: false,
    baseRevision: null,
  }
}

function makeSingleFilePatch(line: string): string {
  return [
    'diff --git a/file.txt b/file.txt',
    '--- a/file.txt',
    '+++ b/file.txt',
    '@@ -1 +1 @@',
    `+${line}`,
  ].join('\n')
}

function makeLargeFileTree(fileCount: number): FileSystemNode {
  return {
    id: '/',
    name: 'root',
    path: '/',
    type: 'folder',
    children: Array.from({ length: fileCount }, (_, index) => {
      const path = `/src/file-${String(index).padStart(4, '0')}.ts`
      return {
        id: path,
        name: path.split('/').pop() ?? path,
        path,
        type: 'file',
      }
    }),
  }
}

function makeLargeDiffPatch(lineCount: number): string {
  return [
    'diff --git a/file.txt b/file.txt',
    '--- a/file.txt',
    '+++ b/file.txt',
    `@@ -1,${lineCount} +1,${lineCount} @@`,
    ...Array.from({ length: lineCount }, (_, index) => `+line-${String(index).padStart(4, '0')}`),
  ].join('\n')
}

function makeProviderModelCatalog(modelCount: number): ProviderModelCatalogDto {
  return {
    profileId: 'openrouter-default',
    providerId: 'openrouter',
    configuredModelId: 'model-0',
    source: 'live',
    fetchedAt: '2026-05-02T12:00:00Z',
    lastSuccessAt: '2026-05-02T12:00:00Z',
    lastRefreshError: null,
    models: Array.from({ length: modelCount }, (_, index) => ({
      modelId: `model-${index}`,
      displayName: `Model ${index}`,
      thinking: {
        supported: index % 2 === 0,
        effortOptions: index % 2 === 0 ? ['low' as const, 'medium' as const, 'high' as const] : [],
        defaultEffort: index % 2 === 0 ? ('medium' as const) : null,
      },
    })),
  }
}

function resolvedCommit(): Promise<GitCommitResponseDto> {
  return Promise.resolve({
    sha: 'def5678',
    summary: 'Commit summary',
    signature: { name: 'Test User', email: 'test@example.com' },
  })
}

function resolvedFetch(): Promise<GitFetchResponseDto> {
  return Promise.resolve({ remote: 'origin', refspecs: [] })
}

function resolvedPull(): Promise<GitPullResponseDto> {
  return Promise.resolve({
    remote: 'origin',
    branch: 'main',
    updated: false,
    summary: 'Already up to date.',
    newHeadSha: null,
  })
}

function resolvedPush(): Promise<GitPushResponseDto> {
  return Promise.resolve({ remote: 'origin', branch: 'main', updates: [] })
}

function renderVcsSidebar(
  status: RepositoryStatusView,
  onLoadDiff: VcsSidebarProps['onLoadDiff'],
) {
  const props: VcsSidebarProps = {
    open: true,
    projectId: 'project-1',
    status,
    branchLabel: 'main',
    onClose: vi.fn(),
    onRefreshStatus: vi.fn(),
    onLoadDiff,
    onStage: vi.fn(() => Promise.resolve()),
    onUnstage: vi.fn(() => Promise.resolve()),
    onDiscard: vi.fn(() => Promise.resolve()),
    onCommit: vi.fn(resolvedCommit),
    onFetch: vi.fn(resolvedFetch),
    onPull: vi.fn(resolvedPull),
    onPush: vi.fn(resolvedPush),
  }

  return render(<VcsSidebar {...props} />)
}

function selectRepositoryStatusCount(state: ReturnType<XeroHighChurnStore['getSnapshot']>): number {
  return state.repositoryStatus?.statusCount ?? 0
}

function RepositoryShellProbe({ store }: { store: XeroHighChurnStore }) {
  const statusCount = useSelectorStoreValue(store, selectRepositoryStatusCount, Object.is)

  return <span data-testid="repository-status-count">{statusCount}</span>
}

interface LargeFilterItem {
  id: string
  name: string
  detail: string
}

function LargeFilterProbe({ items }: { items: readonly LargeFilterItem[] }) {
  const [query, setQuery] = useState('')
  const deferredQuery = useDeferredFilterQuery(query)
  const index = useMemo(
    () => createSearchIndex(items, (item) => [item.name, item.detail]),
    [items],
  )
  const filtered = useMemo(
    () => filterSearchIndex(index, deferredQuery),
    [deferredQuery, index],
  )

  return (
    <label>
      Large filter
      <input
        aria-label="Large filter"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <output data-testid="large-filter-count">{filtered.length}</output>
    </label>
  )
}

describe('UI latency performance smoke replays', () => {
  it('coalesces a high-volume runtime stream burst into one flush', async () => {
    await measureReplay('runtime-stream-burst', () => {
      let stream: RuntimeStreamView | null = makeRuntimeStream()
      let flushCallback: (() => void) | null = null
      let scheduledFlushCount = 0
      const updateRuntimeStream = vi.fn(
        (
          _projectId: string,
          _agentSessionId: string,
          updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
        ) => {
          stream = updater(stream)
        },
      )
      const buffer = createRuntimeStreamEventBuffer({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runtimeKind: 'openai_codex',
        runId: 'run-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript'],
        runtimeActionRefreshKeysRef: { current: {} },
        updateRuntimeStream,
        scheduleRuntimeMetadataRefresh: vi.fn(),
        scheduleFlush: (callback) => {
          scheduledFlushCount += 1
          flushCallback = callback
          return vi.fn()
        },
      })

      for (let sequence = 1; sequence <= 1_000; sequence += 1) {
        buffer.enqueue(makeRuntimeStreamEvent(sequence))
      }

      expect(scheduledFlushCount).toBe(1)
      expect(updateRuntimeStream).not.toHaveBeenCalled()

      const flush = flushCallback as (() => void) | null
      flush?.()

      expect(updateRuntimeStream).toHaveBeenCalledTimes(1)
      expect(stream?.lastSequence).toBe(1_000)

      smokeReport.replays.runtimeStreamBurst = {
        enqueuedItems: 1_000,
        retainedStreamBytes: stream ? estimateRuntimeStreamViewBytes(stream) : 0,
        scheduledFlushCount,
        streamUpdateCount: updateRuntimeStream.mock.calls.length,
        lastSequence: stream?.lastSequence ?? null,
      }
    })
  })

  it('keeps repository shell renders quiet during diff-only status churn', async () => {
    await measureReplay('repository-status-churn', () => {
      const store = createXeroHighChurnStore()
      let commitCount = 0
      render(
        <Profiler id="repository-shell-probe" onRender={() => { commitCount += 1 }}>
          <RepositoryShellProbe store={store} />
        </Profiler>,
      )

      act(() => {
        store.setRepositoryStatus(makeRepositoryStatus())
      })
      const commitsAfterInitialStatus = commitCount

      act(() => {
        for (let index = 0; index < 250; index += 1) {
          store.setRepositoryStatus(
            makeRepositoryStatus({
              entries: [
                {
                  path: `file-${index}.txt`,
                  staged: index % 2 === 0 ? 'modified' : null,
                  unstaged: index % 2 === 0 ? null : 'modified',
                  untracked: false,
                },
              ],
            }),
          )
        }
      })

      expect(commitCount).toBe(commitsAfterInitialStatus)
      const commitsAfterDiffOnlyChurn = commitCount

      act(() => {
        store.setRepositoryStatus(makeRepositoryStatus({ statusCount: 2, additions: 5 }))
      })

      expect(screen.getByTestId('repository-status-count')).toHaveTextContent('2')
      expect(commitCount).toBe(commitsAfterInitialStatus + 1)

      smokeReport.replays.repositoryStatusChurn = {
        churnedStatusEvents: 250,
        commitsAfterInitialStatus,
        commitsAfterDiffOnlyChurn,
        finalCommitCount: commitCount,
      }
    })
  })

  it('coalesces editor cursor reports during typing-like updates', async () => {
    await measureReplay('editor-typing-cursor-coalescing', () => {
      const callbacks: Array<() => void> = []
      const cursorReport = vi.fn()
      const scheduler = createEditorFrameScheduler({
        requestFrame: (callback) => {
          callbacks.push(callback)
          return { id: callbacks.length, type: 'animation-frame' }
        },
        cancelFrame: vi.fn(),
      })

      for (let index = 0; index < 300; index += 1) {
        scheduler.schedule(cursorReport)
      }

      expect(callbacks).toHaveLength(1)
      callbacks[0]()
      expect(cursorReport).toHaveBeenCalledTimes(1)

      smokeReport.replays.editorTyping = {
        requestedReports: 300,
        scheduledFrames: callbacks.length,
        deliveredReports: cursorReport.mock.calls.length,
      }
    })
  })

  it('keeps large filter input responsive while projecting deferred results', async () => {
    await measureReplay('large-filter-input-priority', async () => {
      const items: LargeFilterItem[] = Array.from({ length: 5_000 }, (_, index) => ({
        id: `item-${index}`,
        name: `target-${index}`,
        detail: index % 2 === 0 ? 'settings provider diagnostic' : 'runtime model catalog',
      }))
      let commitCount = 0

      render(
        <Profiler id="large-filter-probe" onRender={() => { commitCount += 1 }}>
          <LargeFilterProbe items={items} />
        </Profiler>,
      )

      const input = screen.getByRole('textbox', { name: 'Large filter' })
      fireEvent.change(input, { target: { value: 'target-4999' } })

      expect(input).toHaveValue('target-4999')
      await waitFor(() => expect(screen.getByTestId('large-filter-count')).toHaveTextContent('1'))

      smokeReport.replays.largeFilterInputPriority = {
        indexedRows: items.length,
        finalMatchedRows: 1,
        profilerCommits: commitCount,
      }
    })
  })

  it('bounds sidebar resize scheduling to one frame per resize burst', async () => {
    await measureReplay('sidebar-resize-scheduling', () => {
      const node = document.createElement('div')
      installRect(node, { height: 240, left: 10, top: 20, width: 400 })
      const frames = createFrameController()
      const calls: ViewportRect[] = []
      const scheduler = createBrowserResizeScheduler({
        cancelFrame: frames.cancelFrame,
        getEnabled: () => true,
        getNode: () => node,
        getTabId: () => 'tab-1',
        onResize: (rect) => calls.push(rect),
        requestFrame: frames.requestFrame,
      })

      for (let index = 0; index < 200; index += 1) {
        scheduler.schedule()
      }

      expect(frames.pendingCount).toBe(1)
      frames.flushFrame()
      expect(calls).toHaveLength(1)

      scheduler.schedule()
      frames.flushFrame()
      expect(calls).toHaveLength(1)

      scheduler.schedule({ force: true })
      frames.flushFrame()
      expect(calls).toHaveLength(2)

      smokeReport.replays.sidebarResize = {
        resizeRequests: 200,
        resizeIpcCallsAfterBurst: 1,
        resizeIpcCallsAfterSteadyState: 1,
        resizeIpcCallsAfterForcedFrame: calls.length,
      }
    })
  })

  it('reports frame governance for pointer streams and hidden native frames', async () => {
    await measureReplay('frame-pointer-governance', () => {
      const pointerFrames = createFrameController()
      const widthFlushes: number[] = []
      const pointerCoalescer = createFrameCoalescer<number>({
        cancelFrame: pointerFrames.cancelFrame,
        onFlush: (value) => widthFlushes.push(value),
        requestFrame: pointerFrames.requestFrame,
      })

      for (let index = 0; index < 240; index += 1) {
        pointerCoalescer.schedule(320 + index)
      }

      const pointerRafLoopsAfterBurst = pointerFrames.pendingCount
      expect(pointerRafLoopsAfterBurst).toBe(1)
      pointerFrames.flushFrame()
      expect(widthFlushes).toEqual([559])

      const nativeFrames = createFrameController()
      const renderedFrames: number[] = []
      let visible = false
      const nativeFrameCoalescer = createFrameCoalescer<number>({
        cancelFrame: nativeFrames.cancelFrame,
        getEnabled: () => visible,
        onFlush: (seq) => renderedFrames.push(seq),
        requestFrame: nativeFrames.requestFrame,
      })

      for (let seq = 1; seq <= 120; seq += 1) {
        nativeFrameCoalescer.schedule(seq)
      }
      expect(nativeFrames.pendingCount).toBe(0)
      expect(renderedFrames).toEqual([])

      visible = true
      for (let seq = 121; seq <= 240; seq += 1) {
        nativeFrameCoalescer.schedule(seq)
      }
      expect(nativeFrames.pendingCount).toBe(1)
      nativeFrames.flushFrame()
      expect(renderedFrames).toEqual([240])

      const pointerMetrics = pointerCoalescer.getMetrics()
      const nativeFrameMetrics = nativeFrameCoalescer.getMetrics()

      smokeReport.replays.frameGovernance = {
        hiddenNativeFramesDropped: nativeFrameMetrics.disabledDrops,
        nativeFrameEvents: nativeFrameMetrics.scheduledValues,
        nativeFrameFlushes: nativeFrameMetrics.flushes,
        pointerMoveEvents: pointerMetrics.scheduledValues,
        pointerMoveFlushes: pointerMetrics.flushes,
        pointerMovesCoalesced: pointerMetrics.coalescedDrops,
        pointerRafLoopsAfterBurst,
      }
    })
  })

  it('bounds large file-tree and diff visible ranges', async () => {
    await measureReplay('large-list-windowing', () => {
      const fileTreeRows = flattenFileTreeRows({
        root: makeLargeFileTree(2_500),
        expandedFolders: new Set(),
        search: null,
        creatingEntry: null,
      })
      const selectedFileIndex = 2_250
      const fileTreeRange = calculateVirtualRange({
        itemCount: fileTreeRows.length,
        itemSize: 26,
        viewportSize: 480,
        scrollOffset: selectedFileIndex * 26,
        overscan: 12,
      })

      expect(fileTreeRows).toHaveLength(2_500)
      expect(fileTreeRange.startIndex).toBeLessThanOrEqual(selectedFileIndex)
      expect(fileTreeRange.endIndex).toBeGreaterThan(selectedFileIndex)
      expect(fileTreeRange.renderedCount).toBeLessThan(60)

      const changedFileCount = 1_200
      const changedFilesRange = calculateVirtualRange({
        itemCount: changedFileCount + 2,
        itemSize: 28,
        viewportSize: 480,
        scrollOffset: 0,
        overscan: 10,
      })
      const diffLines = parseDiffLines(makeLargeDiffPatch(2_000))
      const diffRange = calculateVirtualRange({
        itemCount: diffLines.length,
        itemSize: 22,
        viewportSize: 640,
        scrollOffset: 0,
        overscan: 24,
      })

      expect(changedFilesRange.renderedCount).toBeLessThan(40)
      expect(diffLines).toHaveLength(2_004)
      expect(diffRange.renderedCount).toBeLessThan(60)

      smokeReport.replays.largeListWindowing = {
        fileTreeRows: fileTreeRows.length,
        visibleFileTreeRows: fileTreeRange.renderedCount,
        changedFileRows: changedFileCount,
        visibleChangedFileRows: changedFilesRange.renderedCount,
        diffRows: diffLines.length,
        visibleDiffRows: diffRange.renderedCount,
      }
    })
  })

  it('reports markdown parse reuse and visible diff tokenization batches', async () => {
    await measureReplay('rich-text-bounds', () => {
      resetMarkdownSegmentCacheForTests()
      const markdown = [
        'Assistant response with code:',
        '```',
        'plain code block',
        '```',
        'done',
      ].join('\n')
      const { rerender } = render(<Markdown messageId="smoke-turn-1" text={markdown} />)

      for (let index = 0; index < 100; index += 1) {
        rerender(<Markdown messageId="smoke-turn-1" text={markdown} />)
      }
      const markdownStatsAfterStableRenders = getMarkdownSegmentStats()

      rerender(<Markdown messageId="smoke-turn-1" text={`${markdown}\nstreamed tail`} />)
      const markdownStatsAfterRevision = getMarkdownSegmentStats()

      const diffLines = parseDiffLines(makeLargeDiffPatch(2_000))
      const diffRange = calculateVirtualRange({
        itemCount: diffLines.length,
        itemSize: 22,
        viewportSize: 640,
        scrollOffset: 0,
        overscan: 24,
      })
      const diffBatches = createDiffTokenizationBatches({
        indexes: getVirtualIndexes(diffRange),
        lines: diffLines,
      })

      expect(markdownStatsAfterStableRenders.parses).toBe(1)
      expect(markdownStatsAfterRevision.parses).toBe(2)
      expect(diffBatches.length).toBeLessThanOrEqual(
        Math.ceil(diffRange.renderedCount / DIFF_TOKENIZATION_BATCH_SIZE),
      )

      smokeReport.replays.richTextBounds = {
        stableMarkdownRerenders: 100,
        markdownCacheBytes: markdownStatsAfterRevision.byteSize,
        markdownParsesAfterStableRenders: markdownStatsAfterStableRenders.parses,
        markdownParsesAfterTextRevision: markdownStatsAfterRevision.parses,
        visibleDiffRows: diffRange.renderedCount,
        diffTokenizationBatchCount: diffBatches.length,
        diffTokenizationBatchSize: DIFF_TOKENIZATION_BATCH_SIZE,
      }
    })
  })

  it('reports retained cache bytes for phase-17 memory budgets', async () => {
    await measureReplay('cache-memory-budgets', () => {
      resetMarkdownSegmentCacheForTests()
      render(<Markdown messageId="cache-smoke" text={['Cache sample:', '```ts', 'const value = 1', '```'].join('\n')} />)

      const patch = makeLargeDiffPatch(500)
      parseDiffLinesForPatchKey('cache-smoke-diff', patch)

      const diffPatchCache = createDiffPatchCache()
      setCachedDiffPatch(diffPatchCache, 'project-1\u0000rev\u0000unstaged\u0000file.txt', patch)

      const projectTreeStore = applyProjectFileListing(
        createEmptyProjectFileTreeStore(),
        {
          projectId: 'project-1',
          path: '/',
          root: {
            name: 'root',
            path: '/',
            type: 'folder',
            childrenLoaded: true,
            children: Array.from({ length: 400 }, (_, index) => ({
              name: `file-${index}.ts`,
              path: `/src/file-${index}.ts`,
              type: 'file' as const,
              childrenLoaded: true,
            })),
          },
          truncated: false,
          omittedEntryCount: 0,
        },
      )

      const stream = mergeRuntimeStreamEvents(
        makeRuntimeStream(),
        Array.from({ length: 80 }, (_, index) =>
          makeRuntimeStreamEvent(index + 1),
        ),
      )
      const providerCatalog = makeProviderModelCatalog(250)

      const markdownStats = getMarkdownSegmentStats()
      const diffParsingStats = getDiffParsingStats()
      const diffPatchStats = getDiffPatchCacheStats(diffPatchCache)
      const projectTreeStats = getProjectFileTreeStoreStats(projectTreeStore)
      const shikiStats = getShikiTokenCacheStats()

      expect(markdownStats.byteSize).toBeGreaterThan(0)
      expect(diffParsingStats.byteSize).toBeGreaterThan(0)
      expect(diffPatchStats.byteSize).toBeGreaterThan(0)
      expect(projectTreeStats.byteSize).toBeGreaterThan(0)

      smokeReport.replays.cacheMemoryBudgets = {
        diffParseCacheBytes: diffParsingStats.byteSize,
        diffParseCacheEntries: diffParsingStats.entries,
        diffPatchCacheBytes: diffPatchStats.byteSize,
        diffPatchCacheEntries: diffPatchStats.entries,
        markdownCacheBytes: markdownStats.byteSize,
        markdownCacheEntries: markdownStats.entries,
        projectTreeBytes: projectTreeStats.byteSize,
        projectTreeNodes: projectTreeStats.nodeCount,
        providerCatalogBytes: estimateProviderModelCatalogBytes(providerCatalog),
        providerCatalogModels: providerCatalog.models.length,
        runtimeStreamBytes: stream ? estimateRuntimeStreamViewBytes(stream) : 0,
        shikiTokenCacheBytes: shikiStats.byteSize,
        shikiTokenCacheEntries: shikiStats.entries,
      }
    })
  })

  it('reports IPC payload sizes for representative command and event DTOs', async () => {
    await measureReplay('ipc-payload-budgets', () => {
      resetIpcPayloadBudgetMetricsForTests()
      const repositoryStatus = makeRepositoryStatus({
        entries: Array.from({ length: 400 }, (_, index) => ({
          path: `src/file-${String(index).padStart(4, '0')}.ts`,
          staged: null,
          unstaged: 'modified',
          untracked: false,
        })),
      })
      const projectTree = makeLargeFileTree(2_500)
      const searchResults = {
        projectId: 'project-1',
        totalMatches: 500,
        totalFiles: 50,
        truncated: false,
        files: Array.from({ length: 50 }, (_, fileIndex) => ({
          path: `/src/file-${String(fileIndex).padStart(4, '0')}.ts`,
          matches: Array.from({ length: 10 }, (_, matchIndex) => ({
            line: matchIndex + 1,
            column: 4,
            previewPrefix: 'const result = ',
            previewMatch: 'target',
            previewSuffix: ' + value',
          })),
        })),
      }

      recordIpcPayloadSample({
        boundary: 'event',
        name: 'repository:status_changed',
        payload: { projectId: 'project-1', repositoryId: 'repo-project-1', status: repositoryStatus },
      })
      recordIpcPayloadSample({
        boundary: 'command',
        name: 'list_project_files',
        payload: { projectId: 'project-1', path: '/', root: projectTree, truncated: false, omittedEntryCount: 0 },
      })
      recordIpcPayloadSample({
        boundary: 'command',
        name: 'search_project',
        payload: searchResults,
      })
      for (let sequence = 1; sequence <= 120; sequence += 1) {
        recordIpcPayloadSample({
          boundary: 'channel',
          name: 'subscribe_runtime_stream:item',
          payload: makeRuntimeStreamEvent(sequence).item,
        })
      }

      const metrics = getIpcPayloadBudgetMetrics()
      const byKey = Object.fromEntries(
        metrics.map((metric) => [
          metric.budgetKey,
          {
            largestBytes: metric.largestBytes,
            overBudgetCount: metric.overBudgetCount,
            sampleCount: metric.sampleCount,
          },
        ]),
      )

      expect(byKey.repositoryStatus).toBeDefined()
      expect(byKey.projectTree).toBeDefined()
      expect(byKey.projectSearchResults).toBeDefined()
      expect(byKey.runtimeStreamItem).toBeDefined()

      smokeReport.replays.ipcPayloadBudgets = byKey
    })
  })

  it('keeps VCS diff loading stable across count churn and invalidates by revision', async () => {
    await measureReplay('vcs-diff-cache-invalidation', async () => {
      const onLoadDiff = vi
        .fn(async () => makeDiff(makeSingleFilePatch('fallback revision')))
        .mockResolvedValueOnce(makeDiff(makeSingleFilePatch('stable revision')))
        .mockResolvedValueOnce(makeDiff(makeSingleFilePatch('changed revision')))

      const initialStatus = makeRepositoryStatus()
      const { rerender } = renderVcsSidebar(initialStatus, onLoadDiff)

      await waitFor(() => expect(screen.getByText('stable revision')).toBeInTheDocument())

      for (let index = 0; index < 25; index += 1) {
        rerender(
          <VcsSidebar
            open
            projectId="project-1"
            status={makeRepositoryStatus({
              additions: initialStatus.additions + index + 1,
              deletions: initialStatus.deletions + index,
              statusCount: initialStatus.statusCount + index,
              entries: initialStatus.entries.map((entry) => ({ ...entry })),
            })}
            branchLabel="main"
            onRefreshStatus={vi.fn()}
            onLoadDiff={onLoadDiff}
            onStage={vi.fn(() => Promise.resolve())}
            onUnstage={vi.fn(() => Promise.resolve())}
            onDiscard={vi.fn(() => Promise.resolve())}
            onCommit={vi.fn(resolvedCommit)}
            onFetch={vi.fn(resolvedFetch)}
            onPull={vi.fn(resolvedPull)}
            onPush={vi.fn(resolvedPush)}
          />,
        )
      }

      await waitFor(() => expect(onLoadDiff).toHaveBeenCalledTimes(1))

      rerender(
        <VcsSidebar
          open
          projectId="project-1"
          status={makeRepositoryStatus({ headShaLabel: 'def5678' })}
          branchLabel="main"
          onRefreshStatus={vi.fn()}
          onLoadDiff={onLoadDiff}
          onStage={vi.fn(() => Promise.resolve())}
          onUnstage={vi.fn(() => Promise.resolve())}
          onDiscard={vi.fn(() => Promise.resolve())}
          onCommit={vi.fn(resolvedCommit)}
          onFetch={vi.fn(resolvedFetch)}
          onPull={vi.fn(resolvedPull)}
          onPush={vi.fn(resolvedPush)}
        />,
      )

      await waitFor(() => expect(screen.getByText('changed revision')).toBeInTheDocument())
      expect(onLoadDiff).toHaveBeenCalledTimes(2)

      smokeReport.replays.vcsDiffCache = {
        countChurnRerenders: 25,
        diffLoadsAfterCountChurn: 1,
        diffLoadsAfterRevisionChange: onLoadDiff.mock.calls.length,
      }
    })
  })
})

afterAll(() => {
  smokeReport.slowestTasks.sort((left, right) => right.durationMs - left.durationMs)

  const reportPath = process.env.XERO_PERF_SMOKE_REPORT
  if (!reportPath) {
    return
  }

  mkdirSync(dirname(reportPath), { recursive: true })
  writeFileSync(reportPath, `${JSON.stringify(smokeReport, null, 2)}\n`, 'utf8')
})
