'use client'

import { useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  EyeOff,
  Hash,
  Loader2,
  ListTree,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { ProjectFileIndexEntryDto } from '@/src/lib/xero-model'
import { getFileIcon } from '../file-tree'

export type EditorNavigationMode = 'quick-open' | 'go-line' | 'go-symbol'
export type EditorFileIndexStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface EditorDocumentSymbol {
  name: string
  kind: string
  line: number
  column: number
  detail: string
}

interface EditorNavigationDialogProps {
  mode: EditorNavigationMode | null
  open: boolean
  activePath: string | null
  activeContent: string
  activeLineCount: number
  cursor: { line: number; column: number }
  files: ProjectFileIndexEntryDto[]
  fileIndexStatus: EditorFileIndexStatus
  fileIndexError: string | null
  fileIndexTruncated: boolean
  includeHidden: boolean
  onIncludeHiddenChange: (includeHidden: boolean) => void
  onRefreshFileIndex: () => void
  onOpenChange: (open: boolean) => void
  onOpenFile: (path: string) => void
  onGoToLine: (line: number, column: number) => void
  onCloseFocus?: () => void
}

export function EditorNavigationDialog({
  mode,
  open,
  activePath,
  activeContent,
  activeLineCount,
  cursor,
  files,
  fileIndexStatus,
  fileIndexError,
  fileIndexTruncated,
  includeHidden,
  onIncludeHiddenChange,
  onRefreshFileIndex,
  onOpenChange,
  onOpenFile,
  onGoToLine,
  onCloseFocus,
}: EditorNavigationDialogProps) {
  const title =
    mode === 'go-line'
      ? 'Go to line'
      : mode === 'go-symbol'
        ? 'Go to symbol'
        : 'Quick open'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="overflow-hidden p-0 sm:max-w-2xl"
        onCloseAutoFocus={(event) => {
          if (onCloseFocus) {
            event.preventDefault()
            onCloseFocus()
          }
        }}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>Navigate inside the current project.</DialogDescription>
        </DialogHeader>
        {mode === 'go-line' ? (
          <GoToLineSurface
            activeLineCount={activeLineCount}
            cursor={cursor}
            onGoToLine={(line, column) => {
              onGoToLine(line, column)
              onOpenChange(false)
            }}
          />
        ) : mode === 'go-symbol' ? (
          <GoToSymbolSurface
            activeContent={activeContent}
            activePath={activePath}
            onGoToLine={(line, column) => {
              onGoToLine(line, column)
              onOpenChange(false)
            }}
          />
        ) : (
          <QuickOpenSurface
            files={files}
            includeHidden={includeHidden}
            onIncludeHiddenChange={onIncludeHiddenChange}
            onOpenFile={(path) => {
              onOpenFile(path)
              onOpenChange(false)
            }}
            onRefreshFileIndex={onRefreshFileIndex}
            status={fileIndexStatus}
            error={fileIndexError}
            truncated={fileIndexTruncated}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function QuickOpenSurface({
  files,
  status,
  error,
  truncated,
  includeHidden,
  onIncludeHiddenChange,
  onRefreshFileIndex,
  onOpenFile,
}: {
  files: ProjectFileIndexEntryDto[]
  status: EditorFileIndexStatus
  error: string | null
  truncated: boolean
  includeHidden: boolean
  onIncludeHiddenChange: (includeHidden: boolean) => void
  onRefreshFileIndex: () => void
  onOpenFile: (path: string) => void
}) {
  const [query, setQuery] = useState('')
  const results = useMemo(() => rankQuickOpenFiles(files, query).slice(0, 80), [files, query])

  useEffect(() => {
    setQuery('')
  }, [includeHidden])

  return (
    <Command shouldFilter={false} className="rounded-none">
      <CommandInput
        autoFocus
        placeholder="Open file by name or path"
        value={query}
        onValueChange={setQuery}
      />
      <div className="flex h-9 items-center justify-between gap-3 border-b border-border/70 px-3">
        <label className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
          <Checkbox
            checked={includeHidden}
            onCheckedChange={(checked) => onIncludeHiddenChange(checked === true)}
          />
          <EyeOff className="h-3 w-3" aria-hidden="true" />
          <span>Include hidden files</span>
        </label>
        <div className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
          {status === 'loading' ? (
            <>
              <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
              <span>Indexing files</span>
            </>
          ) : (
            <span>
              {files.length.toLocaleString()} file{files.length === 1 ? '' : 's'}
            </span>
          )}
          <Button
            className="h-6 rounded px-2 text-[11px]"
            onClick={onRefreshFileIndex}
            size="sm"
            type="button"
            variant="ghost"
          >
            Refresh
          </Button>
        </div>
      </div>
      <CommandList className="max-h-[420px]">
        <CommandEmpty>
          {status === 'loading' ? 'Loading project files...' : 'No files match.'}
        </CommandEmpty>
        {error ? (
          <div className="flex items-start gap-2 px-3 py-3 text-[12px] text-destructive">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}
        {truncated ? (
          <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2 text-[11px] text-warning">
            <AlertTriangle className="h-3 w-3" aria-hidden="true" />
            <span>File list truncated. Type a more specific path if the target is missing.</span>
          </div>
        ) : null}
        <CommandGroup heading="Files">
          {results.map((entry) => (
            <CommandItem
              key={entry.path}
              value={entry.path}
              onSelect={() => onOpenFile(entry.path)}
              className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-0.5 py-2"
            >
              <span className="mt-0.5 flex h-4 w-4 items-center justify-center">
                {getFileIcon(entry.name)}
              </span>
              <span className="min-w-0 truncate font-mono text-[12px] text-foreground">
                {entry.name}
              </span>
              <span aria-hidden />
              <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground">
                {entry.path}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

function GoToLineSurface({
  activeLineCount,
  cursor,
  onGoToLine,
}: {
  activeLineCount: number
  cursor: { line: number; column: number }
  onGoToLine: (line: number, column: number) => void
}) {
  const [value, setValue] = useState(() => String(cursor.line || 1))
  const target = parseLineTarget(value, activeLineCount, cursor.column)
  const disabled = !target

  useEffect(() => {
    setValue(String(cursor.line || 1))
  }, [cursor.line])

  return (
    <form
      className="space-y-4 p-5"
      onSubmit={(event) => {
        event.preventDefault()
        if (target) onGoToLine(target.line, target.column)
      }}
    >
      <div className="flex items-start gap-3">
        <span className="mt-0.5 flex h-8 w-8 items-center justify-center rounded-md bg-secondary text-muted-foreground">
          <Hash className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1 space-y-1.5">
          <Label htmlFor="editor-go-to-line" className="text-[13px] font-medium">
            Go to line
          </Label>
          <Input
            id="editor-go-to-line"
            autoFocus
            aria-label="Line and column"
            placeholder="Line or line:column"
            value={value}
            onChange={(event) => setValue(event.target.value)}
          />
          <p className="text-[11px] text-muted-foreground">
            Current file has {Math.max(0, activeLineCount).toLocaleString()} lines.
          </p>
        </div>
      </div>
      <div className="flex justify-end">
        <Button disabled={disabled} type="submit">
          Go
        </Button>
      </div>
    </form>
  )
}

function GoToSymbolSurface({
  activeContent,
  activePath,
  onGoToLine,
}: {
  activeContent: string
  activePath: string | null
  onGoToLine: (line: number, column: number) => void
}) {
  const [query, setQuery] = useState('')
  const symbols = useMemo(() => collectDocumentSymbols(activeContent), [activeContent])
  const results = useMemo(() => rankSymbols(symbols, query).slice(0, 80), [symbols, query])

  return (
    <Command shouldFilter={false} className="rounded-none">
      <CommandInput
        autoFocus
        placeholder={activePath ? 'Search symbols in file' : 'Open a file to search symbols'}
        value={query}
        onValueChange={setQuery}
      />
      <CommandList className="max-h-[420px]">
        <CommandEmpty>
          {activePath ? 'No symbols found.' : 'Open a source file first.'}
        </CommandEmpty>
        <CommandGroup heading="Symbols">
          {results.map((symbol) => (
            <CommandItem
              key={`${symbol.kind}:${symbol.name}:${symbol.line}:${symbol.column}`}
              value={`${symbol.name} ${symbol.detail}`}
              onSelect={() => onGoToLine(symbol.line, symbol.column)}
              className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-2 py-2"
            >
              <ListTree className="mt-0.5 h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
              <span className="min-w-0 truncate font-mono text-[12px]">{symbol.name}</span>
              <span className="font-mono text-[11px] text-muted-foreground">
                {symbol.line}:{symbol.column}
              </span>
              <span aria-hidden />
              <span className="col-span-2 min-w-0 truncate text-[11px] text-muted-foreground">
                {symbol.kind} - {symbol.detail.trim()}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

export function collectDocumentSymbols(content: string): EditorDocumentSymbol[] {
  const lines = content.split('\n')
  const symbols: EditorDocumentSymbol[] = []
  const patterns: Array<{ kind: string; pattern: RegExp }> = [
    { kind: 'function', pattern: /^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'class', pattern: /^\s*(?:export\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'interface', pattern: /^\s*(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'type', pattern: /^\s*(?:export\s+)?type\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'enum', pattern: /^\s*(?:export\s+)?enum\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'variable', pattern: /^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)/ },
    { kind: 'function', pattern: /^\s*(?:export\s+)?(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\(/ },
    { kind: 'function', pattern: /^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][\w]*)/ },
    { kind: 'struct', pattern: /^\s*(?:pub\s+)?struct\s+([A-Za-z_][\w]*)/ },
    { kind: 'enum', pattern: /^\s*(?:pub\s+)?enum\s+([A-Za-z_][\w]*)/ },
    { kind: 'function', pattern: /^\s*def\s+([A-Za-z_][\w]*)/ },
    { kind: 'class', pattern: /^\s*class\s+([A-Za-z_][\w]*)/ },
    { kind: 'function', pattern: /^\s*func\s+(?:\([^)]+\)\s*)?([A-Za-z_][\w]*)/ },
  ]

  for (let index = 0; index < lines.length && symbols.length < 500; index += 1) {
    const line = lines[index] ?? ''
    const heading = /^(#{1,6})\s+(.+)$/.exec(line)
    if (heading) {
      const name = heading[2]?.trim() ?? ''
      if (name) {
        symbols.push({
          name,
          kind: 'heading',
          line: index + 1,
          column: (heading[1]?.length ?? 0) + 2,
          detail: line.trim(),
        })
      }
      continue
    }

    for (const candidate of patterns) {
      const match = candidate.pattern.exec(line)
      const name = match?.[1]
      if (!name) continue
      symbols.push({
        name,
        kind: candidate.kind,
        line: index + 1,
        column: line.indexOf(name) + 1,
        detail: line.trim(),
      })
      break
    }
  }

  return symbols
}

function rankQuickOpenFiles(files: ProjectFileIndexEntryDto[], query: string): ProjectFileIndexEntryDto[] {
  const normalizedQuery = query.trim()
  if (!normalizedQuery) return files.slice(0, 80)

  return files
    .map((file) => ({
      file,
      score: Math.min(
        fuzzyScore(normalizedQuery, file.name) ?? Number.POSITIVE_INFINITY,
        fuzzyScore(normalizedQuery, file.path) ?? Number.POSITIVE_INFINITY,
      ),
    }))
    .filter((entry) => Number.isFinite(entry.score))
    .sort((left, right) => left.score - right.score || left.file.path.localeCompare(right.file.path))
    .map((entry) => entry.file)
}

function rankSymbols(symbols: EditorDocumentSymbol[], query: string): EditorDocumentSymbol[] {
  const normalizedQuery = query.trim()
  if (!normalizedQuery) return symbols

  return symbols
    .map((symbol) => ({
      symbol,
      score: Math.min(
        fuzzyScore(normalizedQuery, symbol.name) ?? Number.POSITIVE_INFINITY,
        fuzzyScore(normalizedQuery, symbol.detail) ?? Number.POSITIVE_INFINITY,
      ),
    }))
    .filter((entry) => Number.isFinite(entry.score))
    .sort((left, right) => left.score - right.score || left.symbol.line - right.symbol.line)
    .map((entry) => entry.symbol)
}

export function fuzzyScore(query: string, candidate: string): number | null {
  const q = query.toLowerCase()
  const c = candidate.toLowerCase()
  let queryIndex = 0
  let score = c.length * 0.01
  let lastMatch = -1

  for (let index = 0; index < c.length && queryIndex < q.length; index += 1) {
    if (c[index] !== q[queryIndex]) continue
    score += lastMatch >= 0 ? Math.max(0, index - lastMatch - 1) : index * 0.15
    if (lastMatch + 1 === index) score -= 0.25
    lastMatch = index
    queryIndex += 1
  }

  return queryIndex === q.length ? score : null
}

function parseLineTarget(
  raw: string,
  lineCount: number,
  fallbackColumn: number,
): { line: number; column: number } | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  const match = /^(\d+)(?::(\d+))?$/.exec(trimmed)
  if (!match) return null
  const line = Number.parseInt(match[1] ?? '', 10)
  const column = Number.parseInt(match[2] ?? String(fallbackColumn || 1), 10)
  if (!Number.isFinite(line) || !Number.isFinite(column) || line < 1 || column < 1) {
    return null
  }

  return {
    line: Math.min(line, Math.max(1, lineCount)),
    column,
  }
}
