/**
 * Lightweight markdown rendering for agent transcripts.
 *
 * Handles fenced code blocks (```lang ... ```), inline code (`code`),
 * bold (**), italic (* / _), links, headings, lists, blockquotes,
 * horizontal rules, GFM-style tables, and Mermaid diagrams in
 * ```mermaid blocks. Code blocks are syntax-highlighted by the shared
 * shiki highlighter using the active theme; mermaid blocks lazy-load
 * the mermaid library and render to SVG once streaming completes.
 *
 * This is intentionally a small, allocation-free subset rather than a
 * full CommonMark parser — the agent's transcript output uses a
 * predictable subset and bringing in `marked` / `remark` would add
 * meaningful weight to the bundle for limited gain.
 */

import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Check, Copy, Maximize2, RotateCcw, X, ZoomIn, ZoomOut } from 'lucide-react'

import {
  createByteBudgetCache,
  estimateUtf16Bytes,
  type ByteBudgetCacheStats,
} from '@/lib/byte-budget-cache'
import { cn } from '@/lib/utils'
import {
  hashCodeContent,
  shouldSkipTokenization,
  tokenizeCode,
  type TokenizedLine,
} from '@/lib/shiki'
import { useTheme } from '@/src/features/theme/theme-provider'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogPortal,
  DialogOverlay,
} from '@/components/ui/dialog'

type Segment =
  | { kind: 'code'; lang: string | null; code: string }
  | { kind: 'text'; text: string }

const FENCE_RE = /(^|\n)```([^\n`]*)\n([\s\S]*?)(?:\n```|$)/g
export const MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT = 96 * 1024
const MARKDOWN_SEGMENT_CACHE_MAX_ENTRIES = 240
export const MARKDOWN_SEGMENT_CACHE_MAX_BYTES = 2 * 1024 * 1024

interface MarkdownSegmentCacheEntry {
  segments: Segment[]
}

interface MarkdownSegmentStats extends ByteBudgetCacheStats {
  parses: number
}

const markdownSegmentCache = createByteBudgetCache<string, MarkdownSegmentCacheEntry>({
  maxBytes: MARKDOWN_SEGMENT_CACHE_MAX_BYTES,
  maxEntries: MARKDOWN_SEGMENT_CACHE_MAX_ENTRIES,
})
const markdownSegmentStats = {
  parses: 0,
}

function createMarkdownSegmentCacheKey(input: string, messageId: string | null): string {
  return [messageId ?? 'anonymous', input.length, hashCodeContent(input)].join('\u0000')
}

function estimateMarkdownSegmentBytes(
  input: string,
  messageId: string | null,
  segments: Segment[],
): number {
  let bytes = estimateUtf16Bytes(input) + estimateUtf16Bytes(messageId ?? '') + 32
  for (const segment of segments) {
    bytes += 24
    if (segment.kind === 'code') {
      bytes += estimateUtf16Bytes(segment.lang ?? '')
      bytes += estimateUtf16Bytes(segment.code)
    } else {
      bytes += estimateUtf16Bytes(segment.text)
    }
  }
  return bytes
}

export function splitFencedSegments(input: string): Segment[] {
  const segments: Segment[] = []
  let lastIndex = 0
  let match: RegExpExecArray | null

  FENCE_RE.lastIndex = 0
  while ((match = FENCE_RE.exec(input)) !== null) {
    const [whole, leading, langRaw, body] = match
    const start = match.index + leading.length
    if (start > lastIndex) {
      segments.push({ kind: 'text', text: input.slice(lastIndex, start) })
    }
    segments.push({
      kind: 'code',
      lang: langRaw.trim().length > 0 ? langRaw.trim().toLowerCase() : null,
      code: body.replace(/\s+$/g, ''),
    })
    lastIndex = match.index + whole.length
  }

  if (lastIndex < input.length) {
    segments.push({ kind: 'text', text: input.slice(lastIndex) })
  }

  return segments
}

function getCachedFencedSegments(input: string, messageId: string | null): Segment[] {
  const key = createMarkdownSegmentCacheKey(input, messageId)
  const cached = markdownSegmentCache.get(key)
  if (cached) {
    return cached.segments
  }

  markdownSegmentStats.parses += 1
  const segments = splitFencedSegments(input)
  markdownSegmentCache.set(
    key,
    { segments },
    estimateMarkdownSegmentBytes(input, messageId, segments),
  )

  return segments
}

export function getMarkdownSegmentStats(): MarkdownSegmentStats {
  const cacheStats = markdownSegmentCache.getStats()
  return {
    ...cacheStats,
    parses: markdownSegmentStats.parses,
  }
}

export function resetMarkdownSegmentCacheForTests(): void {
  markdownSegmentCache.clear()
  markdownSegmentStats.parses = 0
}

export interface MarkdownProps {
  /** Source text. */
  text: string
  /** Stable transcript/message id used to keep fenced parsing reusable across renders. */
  messageId?: string | null
  /** True while this message is still receiving token deltas. */
  streaming?: boolean
  /** When true, renders as muted/italic — used inside thinking blocks. */
  muted?: boolean
  /** When true, renders at a smaller text size — used for collapsed previews. */
  compact?: boolean
  /** Fine-grained transcript scale. Dense is used only by condensed agent panes. */
  scale?: 'default' | 'compact' | 'dense'
  /** Optional inline node appended to the very last text paragraph, in line.
   * Used to anchor the streaming caret to the trailing character without
   * dropping it onto a new line. */
  trailing?: React.ReactNode
}

export function Markdown({
  text,
  messageId = null,
  streaming = false,
  muted = false,
  compact = false,
  scale,
  trailing,
}: MarkdownProps) {
  const resolvedScale = scale ?? (compact ? 'compact' : 'default')
  const segments = useMemo(() => getCachedFencedSegments(text, messageId), [messageId, text])
  const lastTextSegmentIndex = (() => {
    for (let i = segments.length - 1; i >= 0; i -= 1) {
      if (segments[i].kind === 'text') return i
    }
    return -1
  })()

  return (
    <div
      className={cn(
        'flex flex-col [&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
        resolvedScale === 'dense'
          ? 'gap-1.5 text-[12px] leading-[1.45]'
          : resolvedScale === 'compact'
            ? 'gap-1 text-[12.5px] leading-relaxed'
            : 'gap-2 text-[14px] leading-relaxed',
        muted && 'text-muted-foreground italic',
      )}
    >
      {segments.map((segment, index) =>
        segment.kind === 'code' && segment.lang === 'mermaid' ? (
          <MermaidBlock key={index} code={segment.code} streaming={streaming} scale={resolvedScale} />
        ) : segment.kind === 'code' ? (
          <CodeBlock key={index} code={segment.code} lang={segment.lang} streaming={streaming} scale={resolvedScale} />
        ) : (
          <Fragment key={index}>
            {renderTextBlock(segment.text, index === lastTextSegmentIndex ? trailing : null, resolvedScale)}
          </Fragment>
        ),
      )}
      {/* If there are no text segments, anchor any trailing inline node so it
       * still renders (e.g. an empty stream that hasn't produced text yet). */}
      {trailing && lastTextSegmentIndex === -1 ? <p className="m-0">{trailing}</p> : null}
    </div>
  )
}

type TableAlignment = 'left' | 'center' | 'right' | null

const TABLE_SEPARATOR_CELL_RE = /^\s*:?-+:?\s*$/

function splitTableRow(line: string): string[] {
  let trimmed = line.trim()
  if (trimmed.startsWith('|')) trimmed = trimmed.slice(1)
  if (trimmed.endsWith('|')) trimmed = trimmed.slice(0, -1)
  const cells: string[] = []
  let current = ''
  for (let i = 0; i < trimmed.length; i += 1) {
    const ch = trimmed[i]
    if (ch === '\\' && trimmed[i + 1] === '|') {
      current += '|'
      i += 1
      continue
    }
    if (ch === '|') {
      cells.push(current.trim())
      current = ''
      continue
    }
    current += ch
  }
  cells.push(current.trim())
  return cells
}

function isTableRow(line: string): boolean {
  if (!line.includes('|')) return false
  const stripped = line.trim().replace(/^\|/, '').replace(/\|$/, '')
  return stripped.includes('|')
}

function isTableSeparator(line: string): boolean {
  if (!line.includes('|') || !line.includes('-')) return false
  const cells = splitTableRow(line)
  if (cells.length < 2) return false
  return cells.every((cell) => TABLE_SEPARATOR_CELL_RE.test(cell))
}

function parseTableAlignments(separatorRow: string): TableAlignment[] {
  return splitTableRow(separatorRow).map((cell) => {
    const c = cell.trim()
    const left = c.startsWith(':')
    const right = c.endsWith(':')
    if (left && right) return 'center'
    if (right) return 'right'
    if (left) return 'left'
    return null
  })
}

function alignmentClass(alignment: TableAlignment): string {
  if (alignment === 'center') return 'text-center'
  if (alignment === 'right') return 'text-right'
  return 'text-left'
}

type MarkdownScale = NonNullable<MarkdownProps['scale']>

function headingSizeClass(level: number, scale: MarkdownScale): string {
  if (scale === 'dense') {
    if (level <= 1) return 'text-[13.5px] font-semibold'
    if (level === 2) return 'text-[13px] font-semibold'
    return 'text-[12.5px] font-semibold'
  }

  if (scale === 'compact') {
    if (level <= 1) return 'text-[14.5px] font-semibold'
    if (level === 2) return 'text-[13.5px] font-semibold'
    return 'text-[13px] font-semibold'
  }

  if (level <= 1) return 'text-[16px] font-semibold'
  if (level === 2) return 'text-[15px] font-semibold'
  return 'text-[14px] font-semibold'
}

function renderTextBlock(
  text: string,
  trailing: React.ReactNode = null,
  scale: MarkdownScale = 'default',
): React.ReactNode {
  const lines = text.split('\n')
  const blocks: React.ReactNode[] = []
  let buffer: string[] = []
  let listKind: 'ul' | 'ol' | null = null
  let listItems: string[] = []
  let blockquoteBuffer: string[] = []

  const flushParagraph = () => {
    if (buffer.length === 0) return
    const joined = buffer.join('\n').trim()
    if (joined.length > 0) {
      blocks.push(
        <p key={blocks.length} className="m-0 whitespace-pre-wrap break-words">
          {renderInline(joined)}
        </p>,
      )
    }
    buffer = []
  }

  const flushList = () => {
    if (listKind == null || listItems.length === 0) {
      listKind = null
      listItems = []
      return
    }
    const items = listItems.map((item, idx) => (
      <li key={idx} className="leading-relaxed [&>p]:m-0">
        {renderInline(item)}
      </li>
    ))
    blocks.push(
      listKind === 'ul' ? (
        <ul key={blocks.length} className="m-0 list-disc space-y-0.5 pl-4">
          {items}
        </ul>
      ) : (
        <ol key={blocks.length} className="m-0 list-decimal space-y-0.5 pl-4">
          {items}
        </ol>
      ),
    )
    listKind = null
    listItems = []
  }

  const flushBlockquote = () => {
    if (blockquoteBuffer.length === 0) return
    const joined = blockquoteBuffer.join('\n').trim()
    blocks.push(
      <blockquote
        key={blocks.length}
        className="m-0 border-l-2 border-primary/40 pl-3 text-muted-foreground"
      >
        {renderInline(joined)}
      </blockquote>,
    )
    blockquoteBuffer = []
  }

  const flushAll = () => {
    flushParagraph()
    flushList()
    flushBlockquote()
  }

  let i = 0
  while (i < lines.length) {
    const rawLine = lines[i]
    const line = rawLine.replace(/\s+$/, '')

    if (line.length === 0) {
      flushAll()
      i += 1
      continue
    }

    // Table detection: a row of pipe-delimited cells followed immediately by
    // a separator row (`| --- | :---: |`). We only commit the table when both
    // are present so a partially-streamed table renders as paragraph text
    // until the separator arrives.
    const nextRaw = i + 1 < lines.length ? lines[i + 1].replace(/\s+$/, '') : null
    if (
      isTableRow(line) &&
      nextRaw != null &&
      isTableSeparator(nextRaw)
    ) {
      flushAll()
      const headers = splitTableRow(line)
      const alignments = parseTableAlignments(nextRaw)
      const headerCount = headers.length
      const rows: string[][] = []
      i += 2
      while (i < lines.length) {
        const bodyLine = lines[i].replace(/\s+$/, '')
        // Once we are inside a table, any line that contains a `|` is a body
        // row — even single-cell rows like `| value |` that the stricter
        // header-detection rule would reject.
        if (bodyLine.length === 0 || !bodyLine.includes('|')) break
        const cells = splitTableRow(bodyLine)
        if (cells.length < headerCount) {
          while (cells.length < headerCount) cells.push('')
        } else if (cells.length > headerCount) {
          cells.length = headerCount
        }
        rows.push(cells)
        i += 1
      }
      blocks.push(
        <TableBlock
          key={blocks.length}
          headers={headers}
          alignments={alignments}
          rows={rows}
          scale={scale}
        />,
      )
      continue
    }

    const headingMatch = /^(#{1,6})\s+(.*)$/.exec(line)
    if (headingMatch) {
      flushAll()
      const level = headingMatch[1].length
      const content = headingMatch[2]
      const sizeClass = headingSizeClass(level, scale)
      blocks.push(
        <p key={blocks.length} className={cn('m-0', sizeClass)}>
          {renderInline(content)}
        </p>,
      )
      i += 1
      continue
    }

    if (/^---+\s*$/.test(line)) {
      flushAll()
      blocks.push(<hr key={blocks.length} className="my-1 border-border/60" />)
      i += 1
      continue
    }

    const bulletMatch = /^[-*+]\s+(.*)$/.exec(line)
    if (bulletMatch) {
      flushParagraph()
      flushBlockquote()
      if (listKind && listKind !== 'ul') flushList()
      listKind = 'ul'
      listItems.push(bulletMatch[1])
      i += 1
      continue
    }

    const orderedMatch = /^\d+\.\s+(.*)$/.exec(line)
    if (orderedMatch) {
      flushParagraph()
      flushBlockquote()
      if (listKind && listKind !== 'ol') flushList()
      listKind = 'ol'
      listItems.push(orderedMatch[1])
      i += 1
      continue
    }

    const blockquoteMatch = /^>\s?(.*)$/.exec(line)
    if (blockquoteMatch) {
      flushParagraph()
      flushList()
      blockquoteBuffer.push(blockquoteMatch[1])
      i += 1
      continue
    }

    flushList()
    flushBlockquote()
    buffer.push(line)
    i += 1
  }

  flushAll()

  if (trailing && blocks.length > 0) {
    const last = blocks[blocks.length - 1]
    // Append the trailing node into the final block so it sits inline with
    // the trailing word rather than dropping onto a new line. We only know
    // how to do this for paragraphs and headings; for lists/blockquotes/hr
    // we fall back to appending after the block.
    if (
      last &&
      typeof last === 'object' &&
      'type' in last &&
      (last as { type?: unknown }).type === 'p'
    ) {
      const original = last as React.ReactElement<{ children?: React.ReactNode; className?: string }>
      const merged = (
        <p key={original.key ?? blocks.length} className={original.props.className}>
          {original.props.children}
          {trailing}
        </p>
      )
      blocks[blocks.length - 1] = merged
    } else {
      blocks.push(<p key={blocks.length} className="m-0">{trailing}</p>)
    }
  } else if (trailing && blocks.length === 0) {
    blocks.push(<p key={blocks.length} className="m-0">{trailing}</p>)
  }

  return <>{blocks}</>
}

const INLINE_PATTERNS: Array<{
  re: RegExp
  render: (match: RegExpExecArray, key: number) => React.ReactNode
}> = [
  {
    re: /`([^`\n]+)`/,
    render: (match, key) => (
      <code
        key={key}
        className="rounded bg-muted/70 px-1 py-px font-mono text-[0.85em] text-foreground"
      >
        {match[1]}
      </code>
    ),
  },
  {
    re: /\*\*([^*\n]+)\*\*/,
    render: (match, key) => (
      <strong key={key} className="font-semibold text-foreground">
        {match[1]}
      </strong>
    ),
  },
  {
    re: /(?<!\w)\*([^*\n]+)\*(?!\w)/,
    render: (match, key) => <em key={key}>{match[1]}</em>,
  },
  {
    re: /(?<!\w)_([^_\n]+)_(?!\w)/,
    render: (match, key) => <em key={key}>{match[1]}</em>,
  },
  {
    re: /\[([^\]\n]+)\]\(([^)\s]+)\)/,
    render: (match, key) => (
      <a
        key={key}
        href={match[2]}
        target="_blank"
        rel="noreferrer noopener"
        className="text-primary underline-offset-2 hover:underline"
      >
        {match[1]}
      </a>
    ),
  },
]

function renderInline(text: string): React.ReactNode {
  if (!text) return null

  let earliest: { idx: number; len: number; rendered: React.ReactNode; render: typeof INLINE_PATTERNS[number]['render']; match: RegExpExecArray } | null = null
  let earliestPattern: typeof INLINE_PATTERNS[number] | null = null

  for (const pattern of INLINE_PATTERNS) {
    const re = new RegExp(pattern.re.source, pattern.re.flags)
    const m = re.exec(text)
    if (!m) continue
    if (earliest == null || m.index < earliest.idx) {
      earliest = {
        idx: m.index,
        len: m[0].length,
        rendered: null,
        render: pattern.render,
        match: m,
      }
      earliestPattern = pattern
    }
  }

  if (!earliest || !earliestPattern) {
    return text
  }

  const before = text.slice(0, earliest.idx)
  const rendered = earliest.render(earliest.match, 0)
  const after = text.slice(earliest.idx + earliest.len)

  return (
    <>
      {before}
      {rendered}
      {renderInline(after)}
    </>
  )
}

interface CodeBlockProps {
  code: string
  lang: string | null
  streaming?: boolean
  scale: MarkdownScale
}

function CodeBlock({ code, lang, streaming = false, scale }: CodeBlockProps) {
  const { theme } = useTheme()
  const [tokenState, setTokenState] = useState<{
    key: string
    tokens: TokenizedLine[]
  } | null>(null)
  const [copied, setCopied] = useState(false)
  const [renderingPlain, setRenderingPlain] = useState(false)
  const isTooLargeToHighlight = lang
    ? shouldSkipTokenization(code, MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT)
    : false
  const tokenKey = lang ? `${lang}\u0000${theme.shiki}\u0000${code.length}\u0000${hashCodeContent(code)}` : null
  const tokens = tokenKey && tokenState?.key === tokenKey ? tokenState.tokens : null

  useEffect(() => {
    let cancelled = false
    if (!lang) {
      setTokenState(null)
      setRenderingPlain(false)
      return () => {
        cancelled = true
      }
    }
    if (isTooLargeToHighlight) {
      setTokenState(null)
      setRenderingPlain(true)
      return () => {
        cancelled = true
      }
    }
    if (streaming) {
      setRenderingPlain(false)
      return () => {
        cancelled = true
      }
    }
    setRenderingPlain(false)
    tokenizeCode(code, lang, theme.shiki, {
      maxBytes: MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT,
    }).then((result) => {
      if (cancelled) return
      if (result) {
        setTokenState({ key: tokenKey ?? '', tokens: result })
        setRenderingPlain(false)
        return
      }
      setTokenState(null)
      setRenderingPlain(true)
    })
    return () => {
      cancelled = true
    }
  }, [code, isTooLargeToHighlight, lang, streaming, theme.shiki, tokenKey])

  useEffect(() => {
    if (!copied) return
    const id = window.setTimeout(() => setCopied(false), 1500)
    return () => window.clearTimeout(id)
  }, [copied])

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
    } catch {
      // Clipboard write can fail in non-secure contexts; fall back silently.
    }
  }

  const displayLang = lang ?? 'text'

  return (
    <div className="group relative my-0.5 overflow-hidden rounded-md border border-border/60 bg-card/60">
      <div className="flex items-center justify-between border-b border-border/60 bg-muted/40 px-2.5 py-1">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="select-none font-mono text-[10.5px] uppercase tracking-[0.08em] text-muted-foreground">
            {displayLang}
          </span>
          {renderingPlain ? (
            <span className="rounded border border-border/60 px-1 py-px text-[10px] font-medium text-muted-foreground">
              Plain
            </span>
          ) : null}
        </span>
        <button
          type="button"
          onClick={handleCopy}
          aria-label={copied ? 'Copied to clipboard' : 'Copy code'}
          className={cn(
            'flex items-center gap-1 rounded px-1.5 py-0.5 text-[10.5px] font-medium text-muted-foreground transition-opacity',
            'hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none',
            copied ? 'opacity-100 text-success' : 'opacity-0 group-hover:opacity-100',
          )}
        >
          {copied ? (
            <>
              <Check className="h-2.5 w-2.5" />
              Copied
            </>
          ) : (
            <>
              <Copy className="h-2.5 w-2.5" />
              Copy
            </>
          )}
        </button>
      </div>
      <pre
        className={cn(
          'm-0 overflow-x-auto font-mono',
          scale === 'dense'
            ? 'px-2 py-1.5 text-[11.5px] leading-snug'
            : 'px-2.5 py-2 text-[12.5px] leading-[1.55]',
        )}
      >
        {tokens ? <ShikiTokens tokens={tokens} /> : <code>{code}</code>}
      </pre>
    </div>
  )
}

function ShikiTokens({ tokens }: { tokens: TokenizedLine[] }) {
  return (
    <code>
      {tokens.map((line, lineIndex) => (
        <Fragment key={lineIndex}>
          {line.map((token, tokenIndex) => (
            <span
              key={tokenIndex}
              style={{
                color: token.color,
                fontStyle: token.fontStyle === 1 ? 'italic' : undefined,
                fontWeight: token.fontStyle === 2 ? 600 : undefined,
                textDecoration: token.fontStyle === 4 ? 'underline' : undefined,
              }}
            >
              {token.content}
            </span>
          ))}
          {lineIndex < tokens.length - 1 ? '\n' : null}
        </Fragment>
      ))}
    </code>
  )
}

interface TableBlockProps {
  headers: string[]
  alignments: TableAlignment[]
  rows: string[][]
  scale: MarkdownScale
}

function TableBlock({ headers, alignments, rows, scale }: TableBlockProps) {
  const dense = scale === 'dense'
  return (
    <div className="my-0.5 overflow-x-auto rounded-md border border-border/60 bg-card/60">
      <Table className={dense ? 'text-[11.5px]' : 'text-[12.5px]'}>
        <TableHeader>
          <TableRow>
            {headers.map((header, idx) => (
              <TableHead
                key={idx}
                className={cn(
                  dense
                    ? 'h-6 px-2 py-1 text-[11.5px] font-semibold text-foreground'
                    : 'h-8 px-2 py-1 text-[12px] font-semibold text-foreground',
                  alignmentClass(alignments[idx] ?? null),
                )}
              >
                {renderInline(header)}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row, rowIdx) => (
            <TableRow key={rowIdx}>
              {row.map((cell, cellIdx) => (
                <TableCell
                  key={cellIdx}
                  className={cn(
                    dense ? 'whitespace-normal px-2 py-0.5 align-top' : 'whitespace-normal px-2 py-1 align-top',
                    alignmentClass(alignments[cellIdx] ?? null),
                  )}
                >
                  {renderInline(cell)}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

type MermaidApi = {
  initialize: (config: {
    startOnLoad?: boolean
    securityLevel?: 'strict' | 'loose' | 'antiscript' | 'sandbox'
    theme?: string
    fontFamily?: string
  }) => void
  render: (id: string, text: string) => Promise<{ svg: string }>
}

let mermaidApiPromise: Promise<MermaidApi> | null = null
let lastMermaidTheme: string | null = null
let mermaidIdCounter = 0

function nextMermaidId(): string {
  mermaidIdCounter += 1
  return `mermaid-${mermaidIdCounter}`
}

async function loadMermaidApi(theme: string): Promise<MermaidApi> {
  if (!mermaidApiPromise) {
    mermaidApiPromise = import('mermaid').then((mod) => {
      const api = (mod as unknown as { default: MermaidApi }).default
      api.initialize({
        startOnLoad: false,
        securityLevel: 'strict',
        theme,
        fontFamily: 'inherit',
      })
      lastMermaidTheme = theme
      return api
    })
  }
  const api = await mermaidApiPromise
  if (lastMermaidTheme !== theme) {
    api.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme,
      fontFamily: 'inherit',
    })
    lastMermaidTheme = theme
  }
  return api
}

const MERMAID_SVG_CACHE_MAX_BYTES = 4 * 1024 * 1024
const MERMAID_SVG_CACHE_MAX_ENTRIES = 64
export const MERMAID_DIAGRAM_BYTE_LIMIT = 32 * 1024

const mermaidSvgCache = createByteBudgetCache<string, { svg: string }>({
  maxBytes: MERMAID_SVG_CACHE_MAX_BYTES,
  maxEntries: MERMAID_SVG_CACHE_MAX_ENTRIES,
})

export function resetMermaidSvgCacheForTests(): void {
  mermaidSvgCache.clear()
  mermaidApiPromise = null
  lastMermaidTheme = null
  mermaidIdCounter = 0
}

interface MermaidBlockProps {
  code: string
  streaming: boolean
  scale: MarkdownScale
}

function MermaidBlock({ code, streaming, scale }: MermaidBlockProps) {
  const { theme } = useTheme()
  const mermaidTheme = theme.appearance === 'dark' ? 'dark' : 'default'
  const cacheKey = `${mermaidTheme} ${code.length} ${hashCodeContent(code)}`
  const tooLargeToRender = estimateUtf16Bytes(code) > MERMAID_DIAGRAM_BYTE_LIMIT

  const [renderState, setRenderState] = useState<
    | { kind: 'pending' }
    | { kind: 'success'; key: string; svg: string }
    | { kind: 'error'; message: string }
  >(() => {
    if (streaming || tooLargeToRender) return { kind: 'pending' }
    const cached = mermaidSvgCache.get(cacheKey)
    if (cached) return { kind: 'success', key: cacheKey, svg: cached.svg }
    return { kind: 'pending' }
  })
  const [copied, setCopied] = useState(false)
  const [fullscreenOpen, setFullscreenOpen] = useState(false)

  useEffect(() => {
    if (streaming || tooLargeToRender) {
      setRenderState({ kind: 'pending' })
      return
    }
    const cached = mermaidSvgCache.get(cacheKey)
    if (cached) {
      setRenderState({ kind: 'success', key: cacheKey, svg: cached.svg })
      return
    }
    let cancelled = false
    setRenderState({ kind: 'pending' })
    loadMermaidApi(mermaidTheme)
      .then((api) => api.render(nextMermaidId(), code))
      .then(({ svg }) => {
        if (cancelled) return
        mermaidSvgCache.set(cacheKey, { svg }, estimateUtf16Bytes(svg) + 64)
        setRenderState({ kind: 'success', key: cacheKey, svg })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        const message = err instanceof Error ? err.message : String(err)
        setRenderState({ kind: 'error', message })
      })
    return () => {
      cancelled = true
    }
  }, [cacheKey, code, mermaidTheme, streaming, tooLargeToRender])

  useEffect(() => {
    if (!copied) return
    const id = window.setTimeout(() => setCopied(false), 1500)
    return () => window.clearTimeout(id)
  }, [copied])

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
    } catch {
      // Clipboard write can fail in non-secure contexts; fall back silently.
    }
  }

  const isError = renderState.kind === 'error'
  const headerLabel = isError ? 'Mermaid syntax error' : 'Mermaid'
  const canShowFullscreen = renderState.kind === 'success'

  return (
    <div className="group relative my-0.5 overflow-hidden rounded-md border border-border/60 bg-card/60">
      <div className="flex items-center justify-between border-b border-border/60 bg-muted/40 px-2.5 py-1">
        <span className="flex min-w-0 items-center gap-1.5">
          <span
            className={cn(
              'select-none font-mono text-[10.5px] uppercase tracking-[0.08em]',
              isError ? 'text-destructive' : 'text-muted-foreground',
            )}
          >
            {headerLabel}
          </span>
          {streaming || tooLargeToRender ? (
            <span className="rounded border border-border/60 px-1 py-px text-[10px] font-medium text-muted-foreground">
              {tooLargeToRender ? 'Plain' : 'Streaming'}
            </span>
          ) : null}
        </span>
        <div className="flex items-center gap-0.5">
          {canShowFullscreen ? (
            <button
              type="button"
              onClick={() => setFullscreenOpen(true)}
              aria-label="Open diagram fullscreen"
              className={cn(
                'flex items-center gap-1 rounded px-1.5 py-0.5 text-[10.5px] font-medium text-muted-foreground transition-opacity',
                'hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none',
                'opacity-0 group-hover:opacity-100',
              )}
            >
              <Maximize2 className="h-2.5 w-2.5" />
              Fullscreen
            </button>
          ) : null}
          <button
            type="button"
            onClick={handleCopy}
            aria-label={copied ? 'Copied to clipboard' : 'Copy diagram source'}
            className={cn(
              'flex items-center gap-1 rounded px-1.5 py-0.5 text-[10.5px] font-medium text-muted-foreground transition-opacity',
              'hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none',
              copied ? 'opacity-100 text-success' : 'opacity-0 group-hover:opacity-100',
            )}
          >
            {copied ? (
              <>
                <Check className="h-2.5 w-2.5" />
                Copied
              </>
            ) : (
              <>
                <Copy className="h-2.5 w-2.5" />
                Copy
              </>
            )}
          </button>
        </div>
      </div>
      {renderState.kind === 'success' ? (
        <div
          className={cn(
            'flex max-h-[460px] items-start justify-center overflow-auto p-3',
            // Mermaid produces an SVG sized to its natural diagram dimensions.
            // Cap the inline preview width to the chat column and let very
            // tall diagrams (mindmap, journey, gantt) scroll vertically inside
            // the box. Detailed inspection happens in the fullscreen dialog.
            '[&_svg]:max-w-full [&_svg]:h-auto',
          )}
          // The SVG is produced by mermaid with securityLevel: 'strict', which
          // strips click handlers and embedded HTML. The diagram source itself
          // comes from the LLM, so trusting mermaid's strict sanitization here
          // is the same trust model that Shiki gets for code highlighting.
          dangerouslySetInnerHTML={{ __html: renderState.svg }}
        />
      ) : (
        <pre
          className={cn(
            'm-0 overflow-x-auto font-mono',
            scale === 'dense'
              ? 'px-2 py-1.5 text-[11.5px] leading-snug'
              : 'px-2.5 py-2 text-[12.5px] leading-[1.55]',
          )}
        >
          <code>{code}</code>
        </pre>
      )}
      {renderState.kind === 'error' ? (
        <div className="border-t border-border/60 bg-destructive/10 px-2.5 py-1 text-[11px] text-destructive">
          {renderState.message || 'Failed to render diagram'}
        </div>
      ) : null}
      {renderState.kind === 'success' ? (
        <MermaidFullscreenView
          open={fullscreenOpen}
          onOpenChange={setFullscreenOpen}
          svg={renderState.svg}
          source={code}
        />
      ) : null}
    </div>
  )
}

interface MermaidFullscreenViewProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  svg: string
  source: string
}

const MERMAID_FULLSCREEN_MIN_SCALE = 0.2
const MERMAID_FULLSCREEN_MAX_SCALE = 10
const MERMAID_FULLSCREEN_WHEEL_FACTOR = 1.15

function parseMermaidNaturalSize(svg: string): { w: number; h: number } | null {
  const viewBoxMatch = /viewBox=["']\s*[-\d.]+\s+[-\d.]+\s+([\d.]+)\s+([\d.]+)\s*["']/i.exec(svg)
  if (viewBoxMatch) {
    const w = parseFloat(viewBoxMatch[1])
    const h = parseFloat(viewBoxMatch[2])
    if (w > 0 && h > 0) return { w, h }
  }
  const widthMatch = /\swidth=["']?([\d.]+)/i.exec(svg)
  const heightMatch = /\sheight=["']?([\d.]+)/i.exec(svg)
  if (widthMatch && heightMatch) {
    const w = parseFloat(widthMatch[1])
    const h = parseFloat(heightMatch[1])
    if (w > 0 && h > 0) return { w, h }
  }
  return null
}

function rewriteMermaidSvgSize(svg: string, width: number, height: number): string {
  return svg.replace(/<svg([^>]*)>/i, (_match, attrs: string) => {
    let cleaned = attrs
      .replace(/\swidth=["'][^"']*["']/gi, '')
      .replace(/\sheight=["'][^"']*["']/gi, '')
    cleaned = cleaned.replace(/\sstyle=["']([^"']*)["']/i, (_styleMatch, style: string) => {
      const remaining = style
        .split(';')
        .map((s) => s.trim())
        .filter((s) => s.length > 0 && !/^max-(width|height)\s*:/i.test(s))
        .join('; ')
      return remaining.length > 0 ? ` style="${remaining}"` : ''
    })
    return `<svg${cleaned} width="${width}" height="${height}">`
  })
}

function MermaidFullscreenView({ open, onOpenChange, svg, source }: MermaidFullscreenViewProps) {
  const [scale, setScale] = useState(1)
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const [isDragging, setIsDragging] = useState(false)
  const dragRef = useRef<{
    startClientX: number
    startClientY: number
    startX: number
    startY: number
  } | null>(null)
  const [copied, setCopied] = useState(false)

  const reset = useCallback(() => {
    setScale(1)
    setOffset({ x: 0, y: 0 })
  }, [])

  const zoomBy = useCallback((factor: number) => {
    setScale((prev) =>
      Math.min(MERMAID_FULLSCREEN_MAX_SCALE, Math.max(MERMAID_FULLSCREEN_MIN_SCALE, prev * factor)),
    )
  }, [])

  // Reset transform every time the dialog opens so the user always starts at
  // the default centered view regardless of prior interaction.
  useEffect(() => {
    if (open) reset()
  }, [open, reset])

  // Parse the SVG's intrinsic dimensions out of the source string. Source-side
  // parsing is more reliable than reading from the DOM because it does not
  // depend on the SVG being mounted by the time effects run.
  const naturalSize = useMemo(() => parseMermaidNaturalSize(svg), [svg])

  // Produce a scaled SVG string by rewriting the root `<svg>` tag's width and
  // height attributes (and stripping any `max-width`/`max-height` inline
  // styles mermaid sets). dangerouslySetInnerHTML re-renders this on every
  // scale change so the browser re-rasterizes the vector cleanly at the new
  // size — vector-crisp at any zoom, with no CSS transform pixel-magnify and
  // no DOM-mutation race against React commits.
  const scaledSvg = useMemo(() => {
    if (!naturalSize) return svg
    const w = Math.max(1, Math.round(naturalSize.w * scale))
    const h = Math.max(1, Math.round(naturalSize.h * scale))
    return rewriteMermaidSvgSize(svg, w, h)
  }, [svg, scale, naturalSize])

  const handleWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    const factor =
      event.deltaY < 0 ? MERMAID_FULLSCREEN_WHEEL_FACTOR : 1 / MERMAID_FULLSCREEN_WHEEL_FACTOR
    zoomBy(factor)
  }

  useEffect(() => {
    if (!copied) return
    const id = window.setTimeout(() => setCopied(false), 1500)
    return () => window.clearTimeout(id)
  }, [copied])

  const handleCopySource = async () => {
    try {
      await navigator.clipboard.writeText(source)
      setCopied(true)
    } catch {
      // Clipboard write can fail in non-secure contexts; fall back silently.
    }
  }

  const handleMouseDown = (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    dragRef.current = {
      startClientX: event.clientX,
      startClientY: event.clientY,
      startX: offset.x,
      startY: offset.y,
    }
    setIsDragging(true)
  }

  const handleMouseMove = (event: React.MouseEvent<HTMLDivElement>) => {
    const drag = dragRef.current
    if (!drag) return
    setOffset({
      x: drag.startX + (event.clientX - drag.startClientX),
      y: drag.startY + (event.clientY - drag.startClientY),
    })
  }

  const handleMouseEnd = () => {
    dragRef.current = null
    setIsDragging(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPortal>
        <DialogOverlay />
        <DialogContent
          showCloseButton={false}
          className="grid h-[92vh] w-[95vw] max-w-[1600px] grid-rows-[auto_1fr] gap-0 overflow-hidden bg-background p-0 sm:max-w-[1600px]"
          onOpenAutoFocus={(event) => {
            // Keep focus on the dialog content rather than racing with the
            // pan surface — the toolbar buttons should be reachable first.
            event.preventDefault()
          }}
        >
          <DialogTitle className="sr-only">Mermaid diagram fullscreen view</DialogTitle>
          <div className="flex items-center justify-between border-b border-border/60 bg-muted/40 px-3 py-1.5">
            <span className="select-none font-mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
              Mermaid · drag to pan, scroll to zoom
            </span>
            <div className="flex items-center gap-0.5">
              <FullscreenIconButton
                label="Zoom out"
                onClick={() => zoomBy(1 / MERMAID_FULLSCREEN_WHEEL_FACTOR)}
                disabled={scale <= MERMAID_FULLSCREEN_MIN_SCALE + 1e-6}
              >
                <ZoomOut className="h-3.5 w-3.5" />
              </FullscreenIconButton>
              <span
                className="min-w-[3.5rem] select-none text-center font-mono text-[11px] tabular-nums text-muted-foreground"
                aria-live="polite"
              >
                {Math.round(scale * 100)}%
              </span>
              <FullscreenIconButton
                label="Zoom in"
                onClick={() => zoomBy(MERMAID_FULLSCREEN_WHEEL_FACTOR)}
                disabled={scale >= MERMAID_FULLSCREEN_MAX_SCALE - 1e-6}
              >
                <ZoomIn className="h-3.5 w-3.5" />
              </FullscreenIconButton>
              <FullscreenIconButton label="Reset view" onClick={reset}>
                <RotateCcw className="h-3.5 w-3.5" />
              </FullscreenIconButton>
              <span className="mx-1 h-4 w-px bg-border/60" aria-hidden="true" />
              <FullscreenIconButton
                label={copied ? 'Source copied' : 'Copy diagram source'}
                onClick={handleCopySource}
              >
                {copied ? (
                  <Check className="h-3.5 w-3.5 text-success" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </FullscreenIconButton>
              <FullscreenIconButton label="Close" onClick={() => onOpenChange(false)}>
                <X className="h-3.5 w-3.5" />
              </FullscreenIconButton>
            </div>
          </div>
          <div
            role="presentation"
            onWheel={handleWheel}
            onMouseDown={handleMouseDown}
            onMouseMove={handleMouseMove}
            onMouseUp={handleMouseEnd}
            onMouseLeave={handleMouseEnd}
            onDoubleClick={reset}
            className={cn(
              'relative h-full w-full select-none overflow-hidden bg-background',
              isDragging ? 'cursor-grabbing' : 'cursor-grab',
            )}
          >
            <div
              className="absolute left-1/2 top-1/2 [&_svg]:!max-w-none [&_svg]:!max-h-none"
              style={{
                transform: `translate(-50%, -50%) translate(${offset.x}px, ${offset.y}px)`,
                willChange: 'transform',
              }}
              dangerouslySetInnerHTML={{ __html: scaledSvg }}
            />
          </div>
        </DialogContent>
      </DialogPortal>
    </Dialog>
  )
}

function FullscreenIconButton({
  children,
  label,
  onClick,
  disabled = false,
}: {
  children: React.ReactNode
  label: string
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      className={cn(
        'flex h-7 w-7 items-center justify-center rounded text-muted-foreground transition-colors',
        'hover:bg-muted hover:text-foreground',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
        'disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground',
      )}
    >
      {children}
    </button>
  )
}
