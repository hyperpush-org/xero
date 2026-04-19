"use client"

import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  ExecutionPaneView,
  RepositoryDiffState,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type { RepositoryDiffScope } from '@/src/lib/cadence-model'
import {
  AlertCircle,
  Check,
  FileCode,
  FileMinus,
  FilePlus,
  FileSymlink,
  FileWarning,
  GitBranch,
  Hash,
  Loader2,
  RefreshCw,
  Terminal,
  ChevronRight,
} from 'lucide-react'
import { CenteredEmptyState } from '@/components/cadence/centered-empty-state'
import { getLangFromPath, tokenizeCode, type TokenizedLine } from '@/lib/shiki'

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface ExecutionViewProps {
  execution: ExecutionPaneView
  activeDiffScope: RepositoryDiffScope
  activeDiff: RepositoryDiffState
  onSelectDiffScope: (scope: RepositoryDiffScope) => void
  onRetryDiff: () => void
}

type ExecutionTab = 'waves' | 'changes' | 'verify'

const TAB_LABELS: Record<ExecutionTab, string> = {
  waves: 'Execution',
  changes: 'Changes',
  verify: 'Verify',
}

// ---------------------------------------------------------------------------
// Diff parsing — unified patch → per-file structures
// ---------------------------------------------------------------------------

type DiffLineKind = 'add' | 'del' | 'hunk' | 'context'

interface DiffLine {
  kind: DiffLineKind
  content: string
  /** Line number in the new file (null for deletions and hunk headers) */
  newNum: number | null
  /** Line number in the old file (null for additions and hunk headers) */
  oldNum: number | null
}

interface FileDiff {
  path: string
  name: string
  changeKind: 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'unknown'
  additions: number
  deletions: number
  lines: DiffLine[]
}

const HUNK_HEADER_RE = /^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/

function parseUnifiedPatch(patch: string): FileDiff[] {
  if (!patch.trim()) return []

  const files: FileDiff[] = []
  let current: {
    path: string
    changeKind: FileDiff['changeKind']
    lines: DiffLine[]
    adds: number
    dels: number
  } | null = null

  let oldLine = 0
  let newLine = 0

  function flush() {
    if (!current) return
    const parts = current.path.split('/')
    files.push({
      path: current.path,
      name: parts[parts.length - 1],
      changeKind: current.changeKind,
      additions: current.adds,
      deletions: current.dels,
      lines: current.lines,
    })
    current = null
  }

  const rawLines = patch.split('\n')

  for (let i = 0; i < rawLines.length; i++) {
    const line = rawLines[i]

    if (line.startsWith('diff --git ')) {
      flush()
      const match = line.match(/^diff --git a\/(.+?) b\/(.+)$/)
      const filePath = match ? match[2] : 'unknown'

      let changeKind: FileDiff['changeKind'] = 'modified'
      let j = i + 1
      while (j < rawLines.length && !rawLines[j].startsWith('diff --git ') && !rawLines[j].startsWith('@@')) {
        if (rawLines[j].startsWith('new file mode')) changeKind = 'added'
        else if (rawLines[j].startsWith('deleted file mode')) changeKind = 'deleted'
        else if (rawLines[j].startsWith('rename from') || rawLines[j].startsWith('similarity index')) changeKind = 'renamed'
        else if (rawLines[j].startsWith('copy from')) changeKind = 'copied'
        j++
      }

      current = { path: filePath, changeKind, lines: [], adds: 0, dels: 0 }
      continue
    }

    if (!current) continue

    if (
      line.startsWith('index ') || line.startsWith('--- ') || line.startsWith('+++ ') ||
      line.startsWith('old mode') || line.startsWith('new mode') ||
      line.startsWith('new file mode') || line.startsWith('deleted file mode') ||
      line.startsWith('rename from') || line.startsWith('rename to') ||
      line.startsWith('copy from') || line.startsWith('copy to') ||
      line.startsWith('similarity index') || line.startsWith('dissimilarity index')
    ) {
      continue
    }

    const hunkMatch = line.match(HUNK_HEADER_RE)
    if (hunkMatch) {
      oldLine = parseInt(hunkMatch[1], 10)
      newLine = parseInt(hunkMatch[2], 10)
      current.lines.push({ kind: 'hunk', content: line, oldNum: null, newNum: null })
      continue
    }

    if (line.startsWith('+')) {
      current.lines.push({ kind: 'add', content: line.slice(1), oldNum: null, newNum: newLine })
      current.adds++
      newLine++
    } else if (line.startsWith('-')) {
      current.lines.push({ kind: 'del', content: line.slice(1), oldNum: oldLine, newNum: null })
      current.dels++
      oldLine++
    } else if (line.startsWith('\\')) {
      continue
    } else {
      const text = line.startsWith(' ') ? line.slice(1) : line
      current.lines.push({ kind: 'context', content: text, oldNum: oldLine, newNum: newLine })
      oldLine++
      newLine++
    }
  }

  flush()
  return files
}

// ---------------------------------------------------------------------------
// Syntax highlighting — reconstruct old/new code, tokenize, map back
// ---------------------------------------------------------------------------

/**
 * For each diff line, build a mapping to the token array from either
 * the "new" tokenization (context + adds) or the "old" tokenization (context + dels).
 */
function useHighlightedDiff(file: FileDiff | null): Map<number, TokenizedLine> {
  const [tokenMap, setTokenMap] = useState<Map<number, TokenizedLine>>(new Map())
  const filePathRef = useRef<string | null>(null)

  useEffect(() => {
    if (!file) {
      setTokenMap(new Map())
      return
    }

    // Reset immediately on file change to avoid stale tokens
    if (filePathRef.current !== file.path) {
      filePathRef.current = file.path
      setTokenMap(new Map())
    }

    // Capture non-null file for use inside closures (TS narrowing doesn't cross async boundaries)
    const currentFile = file

    const lang = getLangFromPath(currentFile.path)
    if (!lang) return

    let cancelled = false

    const newCodeLines: string[] = []
    const newDiffIndices: number[] = []
    const oldCodeLines: string[] = []
    const oldDiffIndices: number[] = []

    for (let i = 0; i < currentFile.lines.length; i++) {
      const line = currentFile.lines[i]
      if (line.kind === 'hunk') continue
      if (line.kind === 'context' || line.kind === 'add') {
        newCodeLines.push(line.content)
        newDiffIndices.push(i)
      }
      if (line.kind === 'context' || line.kind === 'del') {
        oldCodeLines.push(line.content)
        oldDiffIndices.push(i)
      }
    }

    async function run() {
      const results = new Map<number, TokenizedLine>()

      const newTokens = await tokenizeCode(newCodeLines.join('\n'), lang!)
      if (cancelled) return
      if (newTokens) {
        for (let i = 0; i < newTokens.length && i < newDiffIndices.length; i++) {
          results.set(newDiffIndices[i], newTokens[i])
        }
      }

      const oldTokens = await tokenizeCode(oldCodeLines.join('\n'), lang!)
      if (cancelled) return
      if (oldTokens) {
        for (let i = 0; i < oldTokens.length && i < oldDiffIndices.length; i++) {
          const diffIdx = oldDiffIndices[i]
          if (currentFile.lines[diffIdx].kind === 'del') {
            results.set(diffIdx, oldTokens[i])
          }
        }
      }

      if (!cancelled) setTokenMap(results)
    }

    run()
    return () => { cancelled = true }
  }, [file])

  return tokenMap
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function getKindBadge(changeKind: FileDiff['changeKind']): { label: string; cls: string } {
  switch (changeKind) {
    case 'added': return { label: 'A', cls: 'bg-success/15 text-success' }
    case 'deleted': return { label: 'D', cls: 'bg-destructive/15 text-destructive-foreground' }
    case 'renamed': return { label: 'R', cls: 'bg-chart-4/15 text-chart-4' }
    case 'copied': return { label: 'C', cls: 'bg-chart-2/15 text-chart-2' }
    case 'modified': return { label: 'M', cls: 'bg-chart-1/15 text-chart-1' }
    default: return { label: '?', cls: 'bg-muted text-muted-foreground' }
  }
}

function getFileIcon(kind: FileDiff['changeKind']) {
  switch (kind) {
    case 'added': return FilePlus
    case 'deleted': return FileMinus
    case 'renamed': case 'copied': return FileSymlink
    default: return FileCode
  }
}

/** Render a line's code content with syntax tokens when available, or plain text. */
function TokenizedContent({ content, tokens }: { content: string; tokens?: TokenizedLine }) {
  if (!tokens || tokens.length === 0) {
    return <>{content || ' '}</>
  }

  return (
    <>
      {tokens.map((tok, i) => (
        <span key={i} style={tok.color ? { color: tok.color } : undefined}>
          {tok.content}
        </span>
      ))}
    </>
  )
}

// ---------------------------------------------------------------------------
// Diff viewer table
// ---------------------------------------------------------------------------

function DiffViewer({ file, baseLabel }: { file: FileDiff; baseLabel: string }) {
  const tokenMap = useHighlightedDiff(file)

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      {/* File path header */}
      <div className="flex items-center gap-2 border-b border-border bg-secondary/20 px-3 py-1.5 shrink-0">
        {(() => {
          const Icon = getFileIcon(file.changeKind)
          return <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        })()}
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground/80">
          {file.path}
        </span>
        <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
          {baseLabel}
        </span>
      </div>

      {/* Diff lines */}
      <div className="flex-1 overflow-auto scrollbar-thin">
        <table className="w-full border-collapse font-mono text-[11px] leading-[18px]">
          <tbody>
            {file.lines.map((line, idx) => {
              if (line.kind === 'hunk') {
                return (
                  <tr key={idx} className="bg-chart-1/[0.04]">
                    <td className="w-[1px] whitespace-nowrap border-r border-border select-none">
                      <span className="block px-2 py-px text-right text-[10px] text-chart-1/40">···</span>
                    </td>
                    <td className="py-px pl-3 pr-4 text-chart-1/60 select-none">{line.content}</td>
                  </tr>
                )
              }

              // Single gutter: show new-file line number for context/add, old-file for del
              const lineNum = line.kind === 'del' ? line.oldNum : line.newNum

              const rowBg =
                line.kind === 'add'
                  ? 'bg-success/[0.06]'
                  : line.kind === 'del'
                    ? 'bg-destructive/[0.08]'
                    : ''

              const gutterColor =
                line.kind === 'add'
                  ? 'text-success/30'
                  : line.kind === 'del'
                    ? 'text-destructive-foreground/30'
                    : 'text-muted-foreground/40'

              // Prefix indicator
              const prefix = line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '

              // For add/del with no syntax tokens, tint the text; for context + highlighted, use token colors
              const hasTokens = tokenMap.has(idx)
              const plainColor =
                line.kind === 'add'
                  ? 'text-success/90'
                  : line.kind === 'del'
                    ? 'text-destructive-foreground/90'
                    : 'text-foreground/70'

              return (
                <tr key={idx} className={rowBg}>
                  <td className={`w-[1px] whitespace-nowrap border-r border-border px-2 py-px text-right select-none tabular-nums ${gutterColor}`}>
                    {lineNum ?? ''}
                  </td>
                  <td className="whitespace-pre py-px pl-3 pr-4">
                    <span className={line.kind === 'add' ? 'text-success/50' : line.kind === 'del' ? 'text-destructive-foreground/50' : 'text-transparent'} aria-hidden>
                      {prefix}
                    </span>
                    {hasTokens ? (
                      <TokenizedContent content={line.content} tokens={tokenMap.get(idx)} />
                    ) : (
                      <span className={plainColor}>{line.content || ' '}</span>
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Changes tab
// ---------------------------------------------------------------------------

function ChangesView({
  execution,
  activeDiffScope,
  activeDiff,
  onSelectDiffScope,
  onRetryDiff,
}: Omit<ExecutionViewProps, 'execution'> & { execution: ExecutionPaneView }) {
  const fileDiffs = useMemo(() => parseUnifiedPatch(activeDiff.diff?.patch ?? ''), [activeDiff.diff?.patch])
  const [selectedIdx, setSelectedIdx] = useState(0)

  const selectedFile = fileDiffs[selectedIdx] ?? fileDiffs[0] ?? null
  const totalAdds = fileDiffs.reduce((s, f) => s + f.additions, 0)
  const totalDels = fileDiffs.reduce((s, f) => s + f.deletions, 0)

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b border-border px-3 py-1.5 shrink-0 bg-secondary/20">
        <div className="flex items-center gap-1">
          {execution.diffScopes.map((ds) => (
            <button
              className={`rounded px-2.5 py-1 text-[11px] font-medium transition-colors ${
                activeDiffScope === ds.scope
                  ? 'bg-secondary text-foreground'
                  : 'text-muted-foreground hover:text-foreground hover:bg-secondary/50'
              }`}
              key={ds.scope}
              onClick={() => { onSelectDiffScope(ds.scope); setSelectedIdx(0) }}
              type="button"
            >
              {ds.label}
              {ds.count > 0 ? <span className="ml-1.5 tabular-nums opacity-60">{ds.count}</span> : null}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <GitBranch className="h-3 w-3" />
            {execution.branchLabel}
          </span>
          <span className="flex items-center gap-1 font-mono">
            <Hash className="h-3 w-3" />
            {execution.headShaLabel.slice(0, 8)}
          </span>
          <button
            className="flex items-center gap-1 rounded px-1.5 py-0.5 transition-colors hover:bg-secondary/50 hover:text-foreground"
            onClick={onRetryDiff}
            type="button"
          >
            <RefreshCw className="h-3 w-3" />
          </button>
        </div>
      </div>

      {/* Loading */}
      {activeDiff.status === 'loading' ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>Loading {activeDiffScope} diff…</span>
          </div>
        </div>
      ) : null}

      {/* Error */}
      {activeDiff.status === 'error' ? (
        <div className="flex flex-1 items-center justify-center p-6">
          <div className="max-w-sm rounded-lg border border-destructive/20 bg-destructive/5 p-4">
            <div className="flex items-start gap-2.5">
              <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-destructive-foreground" />
              <div className="flex-1">
                <p className="text-[13px] font-medium text-destructive-foreground">Failed to load diff</p>
                <p className="mt-1 text-[12px] leading-5 text-destructive-foreground/70">
                  {activeDiff.errorMessage ?? 'Unknown error.'}
                </p>
                <button
                  className="mt-3 rounded border border-destructive/30 px-2.5 py-1 text-[11px] font-medium text-destructive-foreground transition-colors hover:bg-destructive/10"
                  onClick={onRetryDiff}
                  type="button"
                >
                  Retry
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {/* Empty */}
      {activeDiff.status === 'ready' && activeDiff.diff?.isEmpty ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="flex flex-col items-center gap-2 text-center">
            <div className="flex h-10 w-10 items-center justify-center rounded-full bg-success/10">
              <Check className="h-5 w-5 text-success" />
            </div>
            <p className="text-[13px] font-medium text-foreground/80">No {activeDiffScope} changes</p>
            <p className="max-w-xs text-[12px] leading-5 text-muted-foreground">Working tree is clean for this scope.</p>
          </div>
        </div>
      ) : null}

      {/* File list + diff viewer */}
      {activeDiff.status === 'ready' && activeDiff.diff && !activeDiff.diff.isEmpty && fileDiffs.length > 0 ? (
        <div className="flex min-h-0 flex-1">
          {/* File sidebar */}
          <div className="flex w-64 shrink-0 flex-col border-r border-border">
            <div className="flex items-center justify-between border-b border-border px-3 py-1.5">
              <span className="text-[11px] text-muted-foreground">
                {fileDiffs.length} {fileDiffs.length === 1 ? 'file' : 'files'}
              </span>
              <div className="flex items-center gap-2 font-mono text-[11px]">
                {totalAdds > 0 ? <span className="text-success">+{totalAdds}</span> : null}
                {totalDels > 0 ? <span className="text-destructive-foreground">−{totalDels}</span> : null}
              </div>
            </div>

            <div className="flex-1 overflow-y-auto scrollbar-thin">
              {fileDiffs.map((file, i) => {
                const badge = getKindBadge(file.changeKind)
                const selected = i === selectedIdx || (selectedIdx >= fileDiffs.length && i === 0)
                return (
                  <button
                    className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors ${
                      selected
                        ? 'bg-secondary text-foreground'
                        : 'text-foreground/70 hover:bg-secondary/40 hover:text-foreground'
                    }`}
                    key={`${file.path}-${i}`}
                    onClick={() => setSelectedIdx(i)}
                    type="button"
                  >
                    <span className={`flex h-4 w-4 shrink-0 items-center justify-center rounded text-[9px] font-bold ${badge.cls}`}>
                      {badge.label}
                    </span>
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={file.path}>
                      {file.path}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] text-muted-foreground tabular-nums">
                      {file.additions > 0 ? `+${file.additions}` : ''}
                      {file.additions > 0 && file.deletions > 0 ? ' ' : ''}
                      {file.deletions > 0 ? `−${file.deletions}` : ''}
                    </span>
                  </button>
                )
              })}
            </div>

            {activeDiff.diff.truncated ? (
              <div className="border-t border-border px-3 py-1.5">
                <span className="text-[10px] text-muted-foreground">
                  <FileWarning className="mr-1 inline h-3 w-3" />
                  Output was truncated
                </span>
              </div>
            ) : null}
          </div>

          {/* Diff viewer */}
          {selectedFile ? (
            <DiffViewer file={selectedFile} baseLabel={activeDiff.diff.baseRevisionLabel} />
          ) : null}
        </div>
      ) : null}

      {/* Malformed patch fallback */}
      {activeDiff.status === 'ready' && activeDiff.diff && !activeDiff.diff.isEmpty && fileDiffs.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="flex flex-col items-center gap-2 text-center">
            <FileWarning className="h-5 w-5 text-muted-foreground" />
            <p className="text-[13px] font-medium text-foreground/80">Could not parse diff</p>
            <p className="max-w-xs text-[12px] leading-5 text-muted-foreground">
              The patch was not empty but contained no parseable file diffs.
            </p>
          </div>
        </div>
      ) : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Top-level ExecutionView
// ---------------------------------------------------------------------------

export function ExecutionView({
  execution,
  activeDiffScope,
  activeDiff,
  onSelectDiffScope,
  onRetryDiff,
}: ExecutionViewProps) {
  const [activeTab, setActiveTab] = useState<ExecutionTab>('waves')

  const handleSelectTab = (tab: ExecutionTab) => {
    setActiveTab(tab)
    if (tab === 'changes') onSelectDiffScope(activeDiffScope)
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex items-center border-b border-border bg-card/30 shrink-0">
        <div className="border-r border-border px-4 py-[10px]">
          <div className="flex items-center gap-3 text-[12px]">
            <span className="text-muted-foreground">Phase</span>
            <ChevronRight className="h-3 w-3 text-muted-foreground/40" />
            <h2 className="font-medium text-foreground/80">{execution.activePhase?.name ?? 'None active'}</h2>
          </div>
        </div>

        <nav className="flex h-full items-center">
          {(['waves', 'changes', 'verify'] as const).map((tab) => (
            <button
              className={`-mb-0.5 border-b-2 px-4 py-[10px] text-[12px] font-medium transition-colors ${
                activeTab === tab
                  ? 'border-foreground text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground/70'
              }`}
              key={tab}
              onClick={() => handleSelectTab(tab)}
              type="button"
            >
              {TAB_LABELS[tab]}
            </button>
          ))}
        </nav>
      </div>

      {activeTab === 'waves' ? (
        <div className="flex-1 overflow-y-auto scrollbar-thin">
          <CenteredEmptyState
            description="Execution activity will appear here once this project records live run output or backend execution views become available."
            icon={Terminal}
            title="No execution activity yet"
          />
        </div>
      ) : null}

      {activeTab === 'changes' ? (
        <ChangesView
          activeDiff={activeDiff}
          activeDiffScope={activeDiffScope}
          execution={execution}
          onRetryDiff={onRetryDiff}
          onSelectDiffScope={onSelectDiffScope}
        />
      ) : null}

      {activeTab === 'verify' ? (
        <div className="flex-1 overflow-y-auto scrollbar-thin">
          <CenteredEmptyState
            description="Verification results will appear here once this project records durable verification outcomes or resume history."
            icon={Check}
            title="No verification activity yet"
          />
        </div>
      ) : null}
    </div>
  )
}
