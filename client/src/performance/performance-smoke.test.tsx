/** @vitest-environment jsdom */

import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { Profiler } from 'react'
import { act, render, screen, waitFor } from '@testing-library/react'
import { afterAll, describe, expect, it, vi } from 'vitest'

import { createBrowserResizeScheduler, type ViewportRect } from '@/components/xero/browser-resize-scheduler'
import { createEditorFrameScheduler } from '@/components/xero/code-editor'
import { flattenFileTreeRows } from '@/components/xero/file-tree'
import { parseDiffLines, VcsSidebar, type VcsSidebarProps } from '@/components/xero/vcs-sidebar'
import { calculateVirtualRange } from '@/lib/virtual-list'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import {
  createRuntimeStreamEventBuffer,
} from '@/src/features/xero/use-xero-desktop-state/runtime-stream'
import {
  createXeroHighChurnStore,
  useSelectorStoreValue,
  type XeroHighChurnStore,
} from '@/src/features/xero/use-xero-desktop-state/high-churn-store'
import {
  createRuntimeStreamView,
  type RuntimeStreamEventDto,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'
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
  const status = {
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

describe('UI latency performance smoke replays', () => {
  it('coalesces a high-volume runtime stream burst into one flush', async () => {
    await measureReplay('runtime-stream-burst', () => {
      let stream: RuntimeStreamView | null = makeRuntimeStream()
      let flushCallback: (() => void) | null = null
      let scheduledFlushCount = 0
      const updateRuntimeStream = vi.fn(
        (_projectId: string, updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null) => {
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

      flushCallback?.()

      expect(updateRuntimeStream).toHaveBeenCalledTimes(1)
      expect(stream?.lastSequence).toBe(1_000)

      smokeReport.replays.runtimeStreamBurst = {
        enqueuedItems: 1_000,
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
