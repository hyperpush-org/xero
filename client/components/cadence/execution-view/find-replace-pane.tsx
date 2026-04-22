import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { EditorView } from '@codemirror/view'
import {
  SearchQuery,
  findNext,
  findPrevious,
  replaceAll,
  replaceNext,
  setSearchQuery,
} from '@codemirror/search'
import {
  ArrowDown,
  ArrowUp,
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  Regex,
  WholeWord,
  X,
} from 'lucide-react'
import { cn } from '@/lib/utils'

interface FindReplacePaneProps {
  view: EditorView | null
  onClose: () => void
  initialQuery: string
  showReplace: boolean
  /** Monotonic token; bump when the user re-triggers Cmd+F so we reset focus/selection. */
  openToken: number
}

export function FindReplacePane({
  view,
  onClose,
  initialQuery,
  showReplace: initialShowReplace,
  openToken,
}: FindReplacePaneProps) {
  const [searchText, setSearchText] = useState(initialQuery)
  const [replaceText, setReplaceText] = useState('')
  const [caseSensitive, setCaseSensitive] = useState(false)
  const [useRegex, setUseRegex] = useState(false)
  const [wholeWord, setWholeWord] = useState(false)
  const [showReplace, setShowReplace] = useState(initialShowReplace)
  const [tick, setTick] = useState(0)
  const searchInputRef = useRef<HTMLInputElement>(null)

  const query = useMemo<SearchQuery | null>(() => {
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
    const effective = query ?? new SearchQuery({ search: '' })
    view.dispatch({ effects: setSearchQuery.of(effective) })
  }, [view, query])

  useEffect(() => {
    const input = searchInputRef.current
    if (!input) return
    input.focus()
    input.select()
    if (initialQuery) setSearchText(initialQuery)
    if (initialShowReplace) setShowReplace(true)
  }, [openToken, initialQuery, initialShowReplace])

  const { total, current } = useMemo(() => {
    if (!view || !query || !query.valid) return { total: 0, current: 0 }
    const state = view.state
    const selFrom = state.selection.main.from
    const selTo = state.selection.main.to
    let total = 0
    let current = 0
    const cursor = query.getCursor(state)
    while (true) {
      const step = cursor.next()
      if (step.done) break
      total++
      if (!current && step.value.from === selFrom && step.value.to === selTo) {
        current = total
      }
      if (total > 9999) break
    }
    return { total, current }
    // `tick` forces recompute after navigation / replace.
  }, [view, query, tick])

  const hasResults = total > 0
  const queryInvalid = !!searchText && !(query?.valid ?? false)

  const runAndTick = (cmd: (v: EditorView) => boolean) => {
    if (!view || !query?.valid) return
    cmd(view)
    setTick((t) => t + 1)
  }

  const handleFindNext = () => runAndTick(findNext)
  const handleFindPrev = () => runAndTick(findPrevious)
  const handleReplace = () => runAndTick(replaceNext)
  const handleReplaceAll = () => runAndTick(replaceAll)

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      if (e.shiftKey) handleFindPrev()
      else handleFindNext()
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
      if (e.altKey) handleReplaceAll()
      else handleReplace()
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
    }
  }

  const matchStatus = !searchText
    ? null
    : queryInvalid
      ? { text: 'Invalid pattern', tone: 'error' as const }
      : hasResults
        ? { text: `${current || '—'} of ${total}`, tone: 'normal' as const }
        : { text: 'No results', tone: 'muted' as const }

  return (
    <aside className="flex w-[260px] shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="flex shrink-0 items-center justify-between gap-2 px-3 pt-2.5 pb-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Find
        </span>
        <button
          aria-label="Close find"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onClick={onClose}
          title="Close (Esc)"
          type="button"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="shrink-0 px-2 pb-2">
        <div className="flex items-stretch gap-1">
          <button
            aria-expanded={showReplace}
            aria-label={showReplace ? 'Hide replace' : 'Show replace'}
            className="flex w-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onClick={() => setShowReplace((v) => !v)}
            title={showReplace ? 'Hide replace' : 'Show replace'}
            type="button"
          >
            {showReplace ? (
              <ChevronDown className="h-3.5 w-3.5" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5" />
            )}
          </button>

          <div className="min-w-0 flex-1 space-y-1.5">
            <div className="relative">
              <input
                ref={searchInputRef}
                aria-invalid={queryInvalid}
                aria-label="Find"
                className={cn(
                  'placeholder:text-muted-foreground/70 selection:bg-primary selection:text-primary-foreground',
                  'h-7 w-full rounded-md border bg-background pl-2 pr-[4.25rem] text-[11px] shadow-xs outline-none transition-[color,box-shadow]',
                  'focus-visible:ring-ring/40 focus-visible:ring-[2px]',
                  queryInvalid
                    ? 'border-destructive focus-visible:border-destructive focus-visible:ring-destructive/30'
                    : 'border-input focus-visible:border-ring',
                )}
                onChange={(e) => setSearchText(e.target.value)}
                onKeyDown={handleSearchKeyDown}
                placeholder="Find"
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

            <div className="flex items-center gap-1">
              <span
                className={cn(
                  'min-w-0 flex-1 truncate text-[10px] tabular-nums',
                  matchStatus?.tone === 'error' && 'text-destructive',
                  matchStatus?.tone === 'muted' && 'text-muted-foreground',
                  matchStatus?.tone === 'normal' && 'text-foreground/80',
                )}
              >
                {matchStatus?.text ?? ' '}
              </span>
              <IconButton
                disabled={!hasResults}
                label="Previous match (Shift+Enter)"
                onClick={handleFindPrev}
              >
                <ArrowUp className="h-3.5 w-3.5" />
              </IconButton>
              <IconButton
                disabled={!hasResults}
                label="Next match (Enter)"
                onClick={handleFindNext}
              >
                <ArrowDown className="h-3.5 w-3.5" />
              </IconButton>
            </div>

            {showReplace ? (
              <div className="space-y-1.5 pt-1">
                <input
                  aria-label="Replace"
                  className={cn(
                    'placeholder:text-muted-foreground/70 selection:bg-primary selection:text-primary-foreground',
                    'h-7 w-full rounded-md border border-input bg-background px-2 text-[11px] shadow-xs outline-none transition-[color,box-shadow]',
                    'focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[2px]',
                  )}
                  onChange={(e) => setReplaceText(e.target.value)}
                  onKeyDown={handleReplaceKeyDown}
                  placeholder="Replace"
                  spellCheck={false}
                  type="text"
                  value={replaceText}
                />
                <div className="flex items-center gap-1">
                  <TextButton
                    disabled={!hasResults}
                    label="Replace next (Enter)"
                    onClick={handleReplace}
                  >
                    Replace
                  </TextButton>
                  <TextButton
                    disabled={!hasResults}
                    label="Replace all (Alt+Enter)"
                    onClick={handleReplaceAll}
                  >
                    Replace all
                  </TextButton>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <div className="mt-auto shrink-0 border-t border-border/60 px-3 py-2 text-[10px] leading-tight text-muted-foreground/70">
        <span className="text-foreground/70">↵</span> next ·{' '}
        <span className="text-foreground/70">⇧↵</span> prev ·{' '}
        <span className="text-foreground/70">⎋</span> close
      </div>
    </aside>
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
        'h-6 flex-1 rounded-md border text-[11px] font-medium transition-colors',
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
