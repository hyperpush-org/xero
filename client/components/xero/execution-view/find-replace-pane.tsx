import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import type { EditorView } from '@codemirror/view'
import {
  SearchQuery,
  findNext,
  findPrevious,
  replaceAll as cmReplaceAll,
  replaceNext as cmReplaceNext,
  setSearchQuery,
} from '@codemirror/search'
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  Loader2,
  Regex,
  WholeWord,
  X,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  createBackendRequestCoordinator,
  isStaleBackendRequestError,
  searchProjectRequestKey,
} from '@/src/lib/backend-request-coordinator'
import type {
  ReplaceInProjectRequestDto,
  ReplaceInProjectResponseDto,
  SearchFileResultDto,
  SearchProjectRequestDto,
  SearchProjectResponseDto,
} from '@/src/lib/xero-model'
import { useDebouncedValue } from '@/lib/input-priority'
import { getFileIcon } from '../file-tree'

export type SearchScope = 'file' | 'project'
const FIND_REPLACE_PROJECT_SEARCH_SCOPE = 'find-replace-project-search'
const PROJECT_SEARCH_PAGE_SIZE = 40

interface FindReplacePaneProps {
  view: EditorView | null
  projectId: string
  activePath: string | null
  activeContent: string
  onClose: () => void
  onOpenAtLine: (path: string, line: number, column: number) => void
  searchProject: (request: SearchProjectRequestDto) => Promise<SearchProjectResponseDto>
  replaceInProject: (request: ReplaceInProjectRequestDto) => Promise<ReplaceInProjectResponseDto>
  initialQuery: string
  /** Monotonic token; bump when the user re-triggers Cmd+F so we reset focus/selection. */
  openToken: number
}

// ---------------------------------------------------------------------------
// Local match derivation for "this file" scope — runs in the renderer so
// results update at keystroke speed without a backend round-trip.
// ---------------------------------------------------------------------------

interface LocalMatch {
  line: number
  column: number
  previewPrefix: string
  previewMatch: string
  previewSuffix: string
}

function buildLocalRegex(
  query: string,
  caseSensitive: boolean,
  wholeWord: boolean,
  isRegex: boolean,
): RegExp | null {
  if (!query) return null
  const core = isRegex ? query : escapeRegExp(query)
  const source = wholeWord ? `\\b(?:${core})\\b` : core
  const flags = caseSensitive ? 'g' : 'gi'
  try {
    return new RegExp(source, flags)
  } catch {
    return null
  }
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function collectLocalMatches(content: string, re: RegExp, cap: number): LocalMatch[] {
  const out: LocalMatch[] = []
  const lines = content.split('\n')
  for (let i = 0; i < lines.length; i++) {
    if (out.length >= cap) break
    const line = lines[i]!
    const lineRe = new RegExp(re.source, re.flags)
    let m: RegExpExecArray | null
    while ((m = lineRe.exec(line))) {
      if (out.length >= cap) break
      const matched = m[0]
      if (matched.length === 0) {
        lineRe.lastIndex++
        continue
      }
      const start = m.index
      const end = start + matched.length
      out.push({
        line: i + 1,
        column: start + 1,
        previewPrefix: trimPrefix(line.slice(Math.max(0, start - 60), start)),
        previewMatch: matched,
        previewSuffix: trimSuffix(line.slice(end, Math.min(line.length, end + 120))),
      })
    }
  }
  return out
}

function trimPrefix(text: string): string {
  if (text.length <= 60) return text
  return '…' + text.slice(text.length - 60)
}

function trimSuffix(text: string): string {
  if (text.length <= 120) return text
  return text.slice(0, 120) + '…'
}

function mergeProjectSearchResponses(
  current: SearchProjectResponseDto | null,
  next: SearchProjectResponseDto,
): SearchProjectResponseDto {
  if (!current) return next

  const filesByPath = new Map(current.files.map((file) => [file.path, file]))
  for (const file of next.files) {
    const existing = filesByPath.get(file.path)
    filesByPath.set(file.path, existing ? { ...file, matches: [...existing.matches, ...file.matches] } : file)
  }
  const files = Array.from(filesByPath.values()).sort((left, right) => left.path.localeCompare(right.path))

  return {
    ...next,
    files,
    totalFiles: files.length,
    totalMatches: files.reduce((sum, file) => sum + file.matches.length, 0),
    truncated: Boolean(next.nextCursor) || next.truncated,
  }
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

export function FindReplacePane({
  view,
  projectId,
  activePath,
  activeContent,
  onClose,
  onOpenAtLine,
  searchProject,
  replaceInProject,
  initialQuery,
  openToken,
}: FindReplacePaneProps) {
  const [scope, setScope] = useState<SearchScope>('file')
  const [searchText, setSearchText] = useState(initialQuery)
  const [replaceText, setReplaceText] = useState('')
  const [caseSensitive, setCaseSensitive] = useState(false)
  const [useRegex, setUseRegex] = useState(false)
  const [wholeWord, setWholeWord] = useState(false)
  const [includeGlobs, setIncludeGlobs] = useState('')
  const [excludeGlobs, setExcludeGlobs] = useState('')
  const [projectResponse, setProjectResponse] = useState<SearchProjectResponseDto | null>(null)
  const [projectSearchStatus, setProjectSearchStatus] = useState<'idle' | 'loading' | 'loadingMore' | 'error'>('idle')
  const [projectSearchError, setProjectSearchError] = useState<string | null>(null)
  const [replaceStatus, setReplaceStatus] = useState<'idle' | 'running' | 'error'>('idle')
  const [replaceError, setReplaceError] = useState<string | null>(null)
  const [collapsedFiles, setCollapsedFiles] = useState<Set<string>>(new Set())
  const [tick, setTick] = useState(0)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const searchEpoch = useRef(0)
  const projectSearchCoordinatorRef = useRef(createBackendRequestCoordinator())
  const deferredSearchText = useDeferredValue(searchText)
  const deferredActiveContent = useDeferredValue(activeContent)
  const debouncedProjectSearchText = useDebouncedValue(searchText, 250)
  const debouncedIncludeGlobs = useDebouncedValue(includeGlobs, 250)
  const debouncedExcludeGlobs = useDebouncedValue(excludeGlobs, 250)

  // ------------------------------------------------------------------
  // CodeMirror query (file scope)
  // ------------------------------------------------------------------

  const cmQuery = useMemo<SearchQuery | null>(() => {
    if (!searchText) return null
    try {
      return new SearchQuery({
        search: searchText,
        caseSensitive,
        regexp: useRegex,
        wholeWord,
        replace: replaceText,
      })
    } catch {
      return null
    }
  }, [searchText, replaceText, caseSensitive, useRegex, wholeWord])

  useEffect(() => {
    if (!view) return
    // Clear the editor's search highlights when we're not in file scope so
    // we don't leave stale .cm-searchMatch rings on text the user isn't
    // navigating through.
    const effective = scope === 'file' && cmQuery ? cmQuery : new SearchQuery({ search: '' })
    view.dispatch({ effects: setSearchQuery.of(effective) })
    if (scope !== 'file' || !cmQuery?.valid) return
    const sel = view.state.selection.main
    let onMatch = false
    const probe = cmQuery.getCursor(view.state)
    while (true) {
      const step = probe.next()
      if (step.done) break
      if (step.value.from === sel.from && step.value.to === sel.to) {
        onMatch = true
        break
      }
    }
    if (!onMatch) {
      findNext(view)
      setTick((t) => t + 1)
    }
  }, [view, cmQuery, scope])

  // ------------------------------------------------------------------
  // Focus / reopen handling
  // ------------------------------------------------------------------

  useEffect(() => {
    const input = searchInputRef.current
    if (!input) return
    input.focus()
    input.select()
    if (initialQuery) setSearchText(initialQuery)
  }, [openToken, initialQuery])

  // ------------------------------------------------------------------
  // File-scope local matches
  // ------------------------------------------------------------------

  const localRegex = useMemo(
    () => buildLocalRegex(deferredSearchText, caseSensitive, wholeWord, useRegex),
    [deferredSearchText, caseSensitive, wholeWord, useRegex],
  )

  const localMatches = useMemo(() => {
    if (scope !== 'file' || !localRegex || !activePath) return []
    return collectLocalMatches(deferredActiveContent, localRegex, 2000)
  }, [scope, localRegex, activePath, deferredActiveContent])

  const countQuery = useMemo<SearchQuery | null>(() => {
    if (!deferredSearchText) return null
    try {
      return new SearchQuery({
        search: deferredSearchText,
        caseSensitive,
        regexp: useRegex,
        wholeWord,
      })
    } catch {
      return null
    }
  }, [caseSensitive, deferredSearchText, useRegex, wholeWord])

  const { fileCurrent, fileTotal } = useMemo(() => {
    if (scope !== 'file' || !view || !countQuery?.valid) {
      return { fileCurrent: 0, fileTotal: localMatches.length }
    }
    const sel = view.state.selection.main
    let total = 0
    let current = 0
    const cursor = countQuery.getCursor(view.state)
    while (true) {
      const step = cursor.next()
      if (step.done) break
      total++
      if (!current && step.value.from === sel.from && step.value.to === sel.to) {
        current = total
      }
      if (total > 9999) break
    }
    return { fileCurrent: current, fileTotal: total }
    // `tick` forces recompute after navigation.
  }, [scope, view, countQuery, tick, localMatches.length])

  const fileQueryInvalid = !!searchText && !(cmQuery?.valid ?? false)

  // ------------------------------------------------------------------
  // Project-scope search (debounced)
  // ------------------------------------------------------------------

  const buildProjectSearchRequest = useCallback(
    (cursor?: string | null): SearchProjectRequestDto => ({
      projectId,
      query: debouncedProjectSearchText,
      cursor: cursor ?? undefined,
      caseSensitive,
      wholeWord,
      regex: useRegex,
      includeGlobs: parseGlobList(debouncedIncludeGlobs),
      excludeGlobs: parseGlobList(debouncedExcludeGlobs),
      maxFiles: PROJECT_SEARCH_PAGE_SIZE,
    }),
    [
      caseSensitive,
      debouncedExcludeGlobs,
      debouncedIncludeGlobs,
      debouncedProjectSearchText,
      projectId,
      useRegex,
      wholeWord,
    ],
  )

  const runProjectSearch = useCallback(
    async ({ append, cursor }: { append: boolean; cursor?: string | null }) => {
      const epoch = ++searchEpoch.current
      setProjectSearchStatus(append ? 'loadingMore' : 'loading')
      setProjectSearchError(null)

      const request = buildProjectSearchRequest(cursor)
      try {
        const response = await projectSearchCoordinatorRef.current.runLatest(
          FIND_REPLACE_PROJECT_SEARCH_SCOPE,
          searchProjectRequestKey(request),
          () => searchProject(request),
        )
        if (epoch !== searchEpoch.current) return
        setProjectResponse((current) => (append ? mergeProjectSearchResponses(current, response) : response))
        setProjectSearchStatus('idle')
      } catch (error) {
        if (isStaleBackendRequestError(error)) return
        if (epoch !== searchEpoch.current) return
        if (!append) {
          setProjectResponse(null)
        }
        setProjectSearchStatus('error')
        setProjectSearchError(error instanceof Error ? error.message : String(error))
      }
    },
    [buildProjectSearchRequest, searchProject],
  )

  useEffect(() => {
    if (scope !== 'project') {
      projectSearchCoordinatorRef.current.cancelScope(FIND_REPLACE_PROJECT_SEARCH_SCOPE)
      setProjectResponse(null)
      setProjectSearchStatus('idle')
      setProjectSearchError(null)
      return
    }
    if (!debouncedProjectSearchText) {
      projectSearchCoordinatorRef.current.cancelScope(FIND_REPLACE_PROJECT_SEARCH_SCOPE)
      setProjectResponse(null)
      setProjectSearchStatus('idle')
      setProjectSearchError(null)
      return
    }

    setProjectSearchStatus('loading')
    setProjectSearchError(null)
    setProjectResponse(null)
    void runProjectSearch({ append: false })

    return () => {
      projectSearchCoordinatorRef.current.cancelScope(FIND_REPLACE_PROJECT_SEARCH_SCOPE)
    }
  }, [debouncedProjectSearchText, runProjectSearch, scope])

  const projectSearchInputPending =
    scope === 'project' &&
    (searchText !== debouncedProjectSearchText ||
      includeGlobs !== debouncedIncludeGlobs ||
      excludeGlobs !== debouncedExcludeGlobs)
  const effectiveProjectResponse = projectSearchInputPending ? null : projectResponse
  const effectiveProjectSearchStatus =
    projectSearchInputPending && searchText.trim().length > 0 ? 'loading' : projectSearchStatus
  const effectiveProjectSearchError = projectSearchInputPending ? null : projectSearchError

  // Reset collapse memory whenever the set of result files changes.
  useEffect(() => {
    setCollapsedFiles(new Set())
  }, [effectiveProjectResponse])

  // ------------------------------------------------------------------
  // Keyboard handlers
  // ------------------------------------------------------------------

  const handleFindNext = useCallback(() => {
    if (!view || !cmQuery?.valid) return
    findNext(view)
    setTick((t) => t + 1)
  }, [view, cmQuery])

  const handleFindPrev = useCallback(() => {
    if (!view || !cmQuery?.valid) return
    findPrevious(view)
    setTick((t) => t + 1)
  }, [view, cmQuery])

  const handleReplaceFileNext = useCallback(() => {
    if (!view || !cmQuery?.valid) return
    cmReplaceNext(view)
    setTick((t) => t + 1)
  }, [view, cmQuery])

  const handleReplaceFileAll = useCallback(() => {
    if (!view || !cmQuery?.valid) return
    cmReplaceAll(view)
    setTick((t) => t + 1)
  }, [view, cmQuery])

  const handleReplaceProjectAll = useCallback(async () => {
    if (!searchText || !effectiveProjectResponse) return
    if (effectiveProjectResponse.totalMatches === 0) return
    setReplaceStatus('running')
    setReplaceError(null)
    try {
      const request: ReplaceInProjectRequestDto = {
        projectId,
        query: searchText,
        replacement: replaceText,
        caseSensitive,
        wholeWord,
        regex: useRegex,
        includeGlobs: parseGlobList(includeGlobs),
        excludeGlobs: parseGlobList(excludeGlobs),
        targetPaths: effectiveProjectResponse.files.map((f) => f.path),
      }
      await replaceInProject(request)
      setReplaceStatus('idle')
      await runProjectSearch({ append: false })
    } catch (error) {
      if (isStaleBackendRequestError(error)) return
      setReplaceStatus('error')
      setReplaceError(error instanceof Error ? error.message : String(error))
    }
  }, [
    projectId,
    searchText,
    replaceText,
    caseSensitive,
    wholeWord,
    useRegex,
    includeGlobs,
    excludeGlobs,
    effectiveProjectResponse,
    replaceInProject,
    runProjectSearch,
  ])

  const handleLoadMoreProjectResults = useCallback(() => {
    const cursor = effectiveProjectResponse?.nextCursor
    if (!cursor || projectSearchStatus === 'loading' || projectSearchStatus === 'loadingMore') {
      return
    }
    void runProjectSearch({ append: true, cursor })
  }, [effectiveProjectResponse?.nextCursor, projectSearchStatus, runProjectSearch])

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      if (scope === 'file') {
        if (e.shiftKey) handleFindPrev()
        else handleFindNext()
      }
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }
    if (e.altKey && !e.metaKey && !e.ctrlKey) {
      if (e.key === 'c' || e.key === 'C') {
        e.preventDefault()
        setCaseSensitive((v) => !v)
      } else if (e.key === 'w' || e.key === 'W') {
        e.preventDefault()
        setWholeWord((v) => !v)
      } else if (e.key === 'r' || e.key === 'R') {
        e.preventDefault()
        setUseRegex((v) => !v)
      }
    }
  }

  const handleReplaceKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      if (scope === 'file') {
        if (e.altKey) handleReplaceFileAll()
        else handleReplaceFileNext()
      } else if (e.altKey) {
        void handleReplaceProjectAll()
      }
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
    }
  }

  // ------------------------------------------------------------------
  // Derived display values
  // ------------------------------------------------------------------

  const fileResults: SearchFileResultDto[] = useMemo(() => {
    if (scope !== 'file') return []
    if (!activePath) return []
    if (localMatches.length === 0) return []
    return [
      {
        path: activePath,
        matches: localMatches.map((m) => ({
          line: m.line,
          column: m.column,
          previewPrefix: m.previewPrefix,
          previewMatch: m.previewMatch,
          previewSuffix: m.previewSuffix,
        })),
      },
    ]
  }, [scope, activePath, localMatches])

  const resultsToRender = scope === 'project' ? effectiveProjectResponse?.files ?? [] : fileResults

  return (
    <aside className="motion-layout-island flex w-[300px] shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="flex shrink-0 items-center justify-between gap-2 px-3 pt-2.5 pb-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Search
        </span>
        <button
          aria-label="Close search"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onClick={onClose}
          title="Close (Esc)"
          type="button"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="shrink-0 space-y-2.5 px-3 pb-3">
        <ScopeToggle scope={scope} onChange={setScope} />

        <div className="relative">
          <input
            ref={searchInputRef}
            aria-invalid={fileQueryInvalid}
            aria-label="Find"
            className={cn(
              'placeholder:text-muted-foreground/70 selection:bg-primary selection:text-primary-foreground',
              'h-8 w-full rounded-md border bg-background pl-2.5 pr-[4.75rem] text-[12px] shadow-xs outline-none transition-[color,box-shadow]',
              'focus-visible:ring-ring/40 focus-visible:ring-[2px]',
              fileQueryInvalid
                ? 'border-destructive focus-visible:border-destructive focus-visible:ring-destructive/30'
                : 'border-input focus-visible:border-ring',
            )}
            onChange={(e) => setSearchText(e.target.value)}
            onKeyDown={handleSearchKeyDown}
            placeholder="Search"
            spellCheck={false}
            type="text"
            value={searchText}
          />
          <div className="absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5">
            <ToggleIcon
              active={caseSensitive}
              label="Match case (Alt+C)"
              onClick={() => setCaseSensitive((v) => !v)}
            >
              <CaseSensitive className="h-3.5 w-3.5" />
            </ToggleIcon>
            <ToggleIcon
              active={wholeWord}
              label="Match whole word (Alt+W)"
              onClick={() => setWholeWord((v) => !v)}
            >
              <WholeWord className="h-3.5 w-3.5" />
            </ToggleIcon>
            <ToggleIcon
              active={useRegex}
              label="Use regular expression (Alt+R)"
              onClick={() => setUseRegex((v) => !v)}
            >
              <Regex className="h-3.5 w-3.5" />
            </ToggleIcon>
          </div>
        </div>

        <input
          aria-label="Replace"
          className={cn(
            'placeholder:text-muted-foreground/70 selection:bg-primary selection:text-primary-foreground',
            'h-8 w-full rounded-md border border-input bg-background px-2.5 text-[12px] shadow-xs outline-none transition-[color,box-shadow]',
            'focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[2px]',
          )}
          onChange={(e) => setReplaceText(e.target.value)}
          onKeyDown={handleReplaceKeyDown}
          placeholder="Replace"
          spellCheck={false}
          type="text"
          value={replaceText}
        />

        {scope === 'project' ? (
          <div className="space-y-1.5">
            <input
              aria-label="Files to include"
              className="placeholder:text-muted-foreground/60 h-7 w-full rounded-md border border-input bg-background px-2.5 text-[11px] shadow-xs outline-none focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[2px]"
              onChange={(e) => setIncludeGlobs(e.target.value)}
              placeholder="Files to include (e.g. src/**/*.ts)"
              spellCheck={false}
              type="text"
              value={includeGlobs}
            />
            <input
              aria-label="Files to exclude"
              className="placeholder:text-muted-foreground/60 h-7 w-full rounded-md border border-input bg-background px-2.5 text-[11px] shadow-xs outline-none focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[2px]"
              onChange={(e) => setExcludeGlobs(e.target.value)}
              placeholder="Files to exclude (e.g. **/*.test.ts)"
              spellCheck={false}
              type="text"
              value={excludeGlobs}
            />
          </div>
        ) : null}

        <ActionRow
          scope={scope}
          hasQuery={!!searchText}
          hasReplace={replaceText.length > 0}
          fileCurrent={fileCurrent}
          fileTotal={fileTotal}
          fileQueryInvalid={fileQueryInvalid}
          projectStatus={effectiveProjectSearchStatus}
          projectResponse={effectiveProjectResponse}
          replaceStatus={replaceStatus}
          onFindPrev={handleFindPrev}
          onFindNext={handleFindNext}
          onReplaceFile={handleReplaceFileNext}
          onReplaceFileAll={handleReplaceFileAll}
          onReplaceProjectAll={handleReplaceProjectAll}
        />

        {effectiveProjectSearchError ? (
          <InlineBanner tone="error" icon={<AlertTriangle className="h-3 w-3" />}>
            {effectiveProjectSearchError}
          </InlineBanner>
        ) : null}
        {replaceError ? (
          <InlineBanner tone="error" icon={<AlertTriangle className="h-3 w-3" />}>
            {replaceError}
          </InlineBanner>
        ) : null}
        {effectiveProjectResponse?.nextCursor ? (
          <InlineBanner tone="warn" icon={<AlertTriangle className="h-3 w-3" />}>
            Showing {effectiveProjectResponse.totalMatches} reviewed matches so far. Load more before replacing additional files.
          </InlineBanner>
        ) : effectiveProjectResponse?.truncated ? (
          <InlineBanner tone="warn" icon={<AlertTriangle className="h-3 w-3" />}>
            Showing first {effectiveProjectResponse.totalMatches} matches. Narrow your query or add filters.
          </InlineBanner>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-auto border-t border-border/60">
        {resultsToRender.length === 0 ? (
          <EmptyResults
            scope={scope}
            hasQuery={!!searchText}
            status={effectiveProjectSearchStatus}
            fileQueryInvalid={fileQueryInvalid}
          />
        ) : (
          <ul className="py-1">
            {resultsToRender.map((file) => (
              <FileResultGroup
                key={file.path}
                file={file}
                collapsed={collapsedFiles.has(file.path)}
                onToggle={() => {
                  setCollapsedFiles((current) => {
                    const next = new Set(current)
                    if (next.has(file.path)) next.delete(file.path)
                    else next.add(file.path)
                    return next
                  })
                }}
                onClickMatch={(line, column) => onOpenAtLine(file.path, line, column)}
              />
            ))}
            {scope === 'project' && effectiveProjectResponse?.nextCursor ? (
              <li className="px-2 py-2">
                <button
                  className="flex h-7 w-full items-center justify-center rounded-md border border-border/70 bg-background text-[11px] font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-60"
                  disabled={effectiveProjectSearchStatus === 'loadingMore'}
                  onClick={handleLoadMoreProjectResults}
                  type="button"
                >
                  {effectiveProjectSearchStatus === 'loadingMore' ? 'Loading…' : 'Load more results'}
                </button>
              </li>
            ) : null}
          </ul>
        )}
      </div>

      <div className="shrink-0 border-t border-border/60 bg-sidebar/60 px-3 py-2.5">
        <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1.5 text-[10.5px] text-muted-foreground">
          <KbdHint keys={['↵']} label="next" />
          <KbdHint keys={['⇧', '↵']} label="prev" />
          <KbdHint keys={['⎋']} label="close" />
        </div>
      </div>
    </aside>
  )
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function ScopeToggle({
  scope,
  onChange,
}: {
  scope: SearchScope
  onChange: (scope: SearchScope) => void
}) {
  return (
    <div className="flex items-center gap-0.5 rounded-md border border-border/60 bg-background p-0.5">
      <ScopeButton active={scope === 'file'} onClick={() => onChange('file')}>
        This file
      </ScopeButton>
      <ScopeButton active={scope === 'project'} onClick={() => onChange('project')}>
        Project
      </ScopeButton>
    </div>
  )
}

function ScopeButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      aria-pressed={active}
      className={cn(
        'flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors',
        active
          ? 'bg-secondary text-foreground'
          : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground',
      )}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  )
}

function ActionRow({
  scope,
  hasQuery,
  hasReplace,
  fileCurrent,
  fileTotal,
  fileQueryInvalid,
  projectStatus,
  projectResponse,
  replaceStatus,
  onFindPrev,
  onFindNext,
  onReplaceFile,
  onReplaceFileAll,
  onReplaceProjectAll,
}: {
  scope: SearchScope
  hasQuery: boolean
  hasReplace: boolean
  fileCurrent: number
  fileTotal: number
  fileQueryInvalid: boolean
  projectStatus: 'idle' | 'loading' | 'loadingMore' | 'error'
  projectResponse: SearchProjectResponseDto | null
  replaceStatus: 'idle' | 'running' | 'error'
  onFindPrev: () => void
  onFindNext: () => void
  onReplaceFile: () => void
  onReplaceFileAll: () => void
  onReplaceProjectAll: () => void
}) {
  if (scope === 'file') {
    const status = !hasQuery
      ? { text: 'No query', tone: 'muted' as const }
      : fileQueryInvalid
        ? { text: 'Invalid pattern', tone: 'error' as const }
        : fileTotal === 0
          ? { text: 'No results', tone: 'muted' as const }
          : { text: `${fileCurrent || '—'} of ${fileTotal}`, tone: 'normal' as const }

    const hasResults = fileTotal > 0

    return (
      <div className="space-y-1.5">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              'min-w-0 flex-1 truncate text-[11px] tabular-nums',
              status.tone === 'error' && 'text-destructive',
              status.tone === 'muted' && 'text-muted-foreground',
              status.tone === 'normal' && 'text-foreground/80',
            )}
          >
            {status.text}
          </span>
          <IconButton disabled={!hasResults} label="Previous (Shift+Enter)" onClick={onFindPrev}>
            <ArrowUp className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton disabled={!hasResults} label="Next (Enter)" onClick={onFindNext}>
            <ArrowDown className="h-3.5 w-3.5" />
          </IconButton>
        </div>
        {hasReplace ? (
          <div className="flex items-center gap-1.5">
            <TextButton disabled={!hasResults} label="Replace next (Enter)" onClick={onReplaceFile}>
              Replace
            </TextButton>
            <TextButton
              disabled={!hasResults}
              label="Replace all (Alt+Enter)"
              onClick={onReplaceFileAll}
            >
              Replace all
            </TextButton>
          </div>
        ) : null}
      </div>
    )
  }

  // project scope
  const status = !hasQuery
    ? { text: 'No query', tone: 'muted' as const }
    : projectStatus === 'loading'
      ? { text: 'Searching…', tone: 'muted' as const }
      : projectStatus === 'loadingMore'
        ? { text: 'Loading more…', tone: 'muted' as const }
      : projectStatus === 'error'
        ? { text: 'Search failed', tone: 'error' as const }
        : projectResponse == null
          ? { text: ' ', tone: 'muted' as const }
          : projectResponse.totalMatches === 0
            ? { text: 'No results', tone: 'muted' as const }
            : {
                text: `${projectResponse.totalMatches} result${projectResponse.totalMatches === 1 ? '' : 's'} in ${projectResponse.totalFiles} file${projectResponse.totalFiles === 1 ? '' : 's'}`,
                tone: 'normal' as const,
              }

  const totalMatches = projectResponse?.totalMatches ?? 0
  const canReplace = totalMatches > 0 && replaceStatus !== 'running'

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-1.5">
        <span
          className={cn(
            'min-w-0 flex-1 truncate text-[11px]',
            status.tone === 'error' && 'text-destructive',
            status.tone === 'muted' && 'text-muted-foreground',
            status.tone === 'normal' && 'text-foreground/80',
          )}
        >
          {status.text}
        </span>
        {projectStatus === 'loading' ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
        ) : null}
      </div>
      {hasReplace ? (
        <TextButton
          disabled={!canReplace}
          label="Replace reviewed project matches (Alt+Enter)"
          onClick={onReplaceProjectAll}
          fullWidth
        >
          {replaceStatus === 'running' ? 'Replacing…' : 'Replace shown'}
        </TextButton>
      ) : null}
    </div>
  )
}

function FileResultGroup({
  file,
  collapsed,
  onToggle,
  onClickMatch,
}: {
  file: SearchFileResultDto
  collapsed: boolean
  onToggle: () => void
  onClickMatch: (line: number, column: number) => void
}) {
  return (
    <li>
      <button
        aria-expanded={!collapsed}
        className="group flex w-full items-center gap-1 px-2 py-1 text-left hover:bg-muted/60"
        onClick={onToggle}
        type="button"
      >
        {collapsed ? (
          <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground/70" />
        ) : (
          <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground/70" />
        )}
        <span className="flex h-4 w-4 shrink-0 items-center justify-center">
          {getFileIcon(displayFileName(file.path))}
        </span>
        <span className="min-w-0 flex-1 truncate text-[11.5px] font-medium text-foreground/90">
          {displayFileName(file.path)}
        </span>
        <span className="shrink-0 tabular-nums text-[10px] text-muted-foreground">
          {file.matches.length}
        </span>
      </button>
      {file.path.includes('/') ? (
        <div className="truncate pb-0.5 pl-[2.25rem] pr-2 text-[10px] text-muted-foreground/70">
          {file.path}
        </div>
      ) : null}
      {!collapsed ? (
        <ul className="mb-0.5">
          {file.matches.map((match, idx) => (
            <li key={`${match.line}:${match.column}:${idx}`}>
              <button
                className="group flex w-full items-start gap-2 px-2 py-1 text-left transition-colors hover:bg-muted/60"
                onClick={() => onClickMatch(match.line, match.column)}
                type="button"
              >
                <span className="w-8 shrink-0 pt-[1px] text-right tabular-nums text-[10px] text-muted-foreground/70">
                  {match.line}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] leading-5 text-foreground/80">
                  <span>{match.previewPrefix}</span>
                  <span className="rounded-sm bg-primary/25 px-0.5 text-foreground">
                    {match.previewMatch}
                  </span>
                  <span>{match.previewSuffix}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  )
}

function EmptyResults({
  scope,
  hasQuery,
  status,
  fileQueryInvalid,
}: {
  scope: SearchScope
  hasQuery: boolean
  status: 'idle' | 'loading' | 'loadingMore' | 'error'
  fileQueryInvalid: boolean
}) {
  if (!hasQuery) {
    return (
      <p className="px-4 py-6 text-center text-[11px] leading-relaxed text-muted-foreground/70">
        Type to search {scope === 'project' ? 'the entire project' : 'the current file'}.
      </p>
    )
  }
  if (fileQueryInvalid) {
    return (
      <p className="px-4 py-6 text-center text-[11px] text-destructive">
        The regular expression isn't valid.
      </p>
    )
  }
  if (scope === 'project' && status === 'loading') {
    return (
      <div className="flex items-center justify-center gap-2 px-4 py-6 text-[11px] text-muted-foreground">
        <Loader2 className="h-3.5 w-3.5 animate-spin" /> Searching…
      </div>
    )
  }
  return (
    <p className="px-4 py-6 text-center text-[11px] text-muted-foreground/70">
      No matches.
    </p>
  )
}

function InlineBanner({
  tone,
  icon,
  children,
}: {
  tone: 'error' | 'warn'
  icon: ReactNode
  children: ReactNode
}) {
  return (
    <div
      className={cn(
        'flex items-start gap-1.5 rounded-md border px-2 py-1.5 text-[10.5px] leading-relaxed',
        tone === 'error' && 'border-destructive/30 bg-destructive/10 text-destructive',
        tone === 'warn' &&
          'border-warning/30 bg-warning/10 text-warning dark:text-warning',
      )}
    >
      <span className="shrink-0 pt-[2px]">{icon}</span>
      <span className="min-w-0 flex-1">{children}</span>
    </div>
  )
}

function ToggleIcon({
  active,
  label,
  onClick,
  children,
}: {
  active: boolean
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={active}
      className={cn(
        'flex h-5 w-5 items-center justify-center rounded transition-colors',
        active
          ? 'bg-primary/15 text-primary'
          : 'text-muted-foreground/80 hover:bg-muted hover:text-foreground',
      )}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  )
}

function IconButton({
  disabled,
  label,
  onClick,
  children,
}: {
  disabled?: boolean
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      aria-label={label}
      className={cn(
        'flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
        disabled ? 'cursor-not-allowed opacity-40' : 'hover:bg-muted hover:text-foreground',
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  )
}

function TextButton({
  disabled,
  label,
  onClick,
  children,
  fullWidth,
}: {
  disabled?: boolean
  label: string
  onClick: () => void
  children: ReactNode
  fullWidth?: boolean
}) {
  return (
    <button
      aria-label={label}
      className={cn(
        'h-7 rounded-md border text-[11.5px] font-medium transition-colors',
        fullWidth ? 'w-full' : 'flex-1',
        disabled
          ? 'cursor-not-allowed border-border/40 text-muted-foreground/40'
          : 'border-border bg-background text-foreground/85 hover:bg-muted hover:text-foreground',
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  )
}

function KbdHint({ keys, label }: { keys: string[]; label: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="inline-flex items-center gap-0.5">
        {keys.map((k) => (
          <kbd
            key={k}
            className="inline-flex h-4 min-w-[16px] items-center justify-center rounded border border-border bg-muted px-1 font-sans text-[10px] font-medium text-foreground/80"
          >
            {k}
          </kbd>
        ))}
      </span>
      <span>{label}</span>
    </span>
  )
}

function displayFileName(path: string): string {
  const idx = path.lastIndexOf('/')
  if (idx === -1 || idx === path.length - 1) return path
  return path.slice(idx + 1)
}

function parseGlobList(raw: string): string[] {
  return raw
    .split(',')
    .map((part) => part.trim())
    .filter((part) => part.length > 0)
}
