/**
 * Lightweight markdown rendering for agent transcripts.
 *
 * Handles fenced code blocks (```lang ... ```), inline code (`code`),
 * bold (**), italic (* / _), links, headings, lists, blockquotes, and
 * horizontal rules. Code blocks are syntax-highlighted by the shared
 * shiki highlighter using the active theme.
 *
 * This is intentionally a small, allocation-free subset rather than a
 * full CommonMark parser — the agent's transcript output uses a
 * predictable subset and bringing in `marked` / `remark` would add
 * meaningful weight to the bundle for limited gain.
 */

import { Fragment, useEffect, useState } from 'react'
import { Check, Copy } from 'lucide-react'

import { cn } from '@/lib/utils'
import { tokenizeCode, type TokenizedLine } from '@/lib/shiki'
import { useTheme } from '@/src/features/theme/theme-provider'

type Segment =
  | { kind: 'code'; lang: string | null; code: string }
  | { kind: 'text'; text: string }

const FENCE_RE = /(^|\n)```([^\n`]*)\n([\s\S]*?)(?:\n```|$)/g

function splitFencedSegments(input: string): Segment[] {
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

export interface MarkdownProps {
  /** Source text. */
  text: string
  /** When true, renders as muted/italic — used inside thinking blocks. */
  muted?: boolean
  /** When true, renders at a smaller text size — used for collapsed previews. */
  compact?: boolean
}

export function Markdown({ text, muted = false, compact = false }: MarkdownProps) {
  const segments = splitFencedSegments(text)
  return (
    <div
      className={cn(
        'flex flex-col leading-relaxed [&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
        compact ? 'gap-1.5 text-[11.5px]' : 'gap-2.5 text-sm',
        muted && 'text-muted-foreground italic',
      )}
    >
      {segments.map((segment, index) =>
        segment.kind === 'code' ? (
          <CodeBlock key={index} code={segment.code} lang={segment.lang} />
        ) : (
          <Fragment key={index}>
            {renderTextBlock(segment.text)}
          </Fragment>
        ),
      )}
    </div>
  )
}

function renderTextBlock(text: string): React.ReactNode {
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
        <ul key={blocks.length} className="m-0 list-disc space-y-1 pl-5">
          {items}
        </ul>
      ) : (
        <ol key={blocks.length} className="m-0 list-decimal space-y-1 pl-5">
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

  for (const rawLine of lines) {
    const line = rawLine.replace(/\s+$/, '')

    if (line.length === 0) {
      flushAll()
      continue
    }

    const headingMatch = /^(#{1,6})\s+(.*)$/.exec(line)
    if (headingMatch) {
      flushAll()
      const level = headingMatch[1].length
      const content = headingMatch[2]
      const sizeClass =
        level <= 1
          ? 'text-base font-semibold'
          : level === 2
            ? 'text-[15px] font-semibold'
            : 'text-sm font-semibold'
      blocks.push(
        <p key={blocks.length} className={cn('m-0', sizeClass)}>
          {renderInline(content)}
        </p>,
      )
      continue
    }

    if (/^---+\s*$/.test(line)) {
      flushAll()
      blocks.push(<hr key={blocks.length} className="my-1 border-border/60" />)
      continue
    }

    const bulletMatch = /^[-*+]\s+(.*)$/.exec(line)
    if (bulletMatch) {
      flushParagraph()
      flushBlockquote()
      if (listKind && listKind !== 'ul') flushList()
      listKind = 'ul'
      listItems.push(bulletMatch[1])
      continue
    }

    const orderedMatch = /^\d+\.\s+(.*)$/.exec(line)
    if (orderedMatch) {
      flushParagraph()
      flushBlockquote()
      if (listKind && listKind !== 'ol') flushList()
      listKind = 'ol'
      listItems.push(orderedMatch[1])
      continue
    }

    const blockquoteMatch = /^>\s?(.*)$/.exec(line)
    if (blockquoteMatch) {
      flushParagraph()
      flushList()
      blockquoteBuffer.push(blockquoteMatch[1])
      continue
    }

    flushList()
    flushBlockquote()
    buffer.push(line)
  }

  flushAll()
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
}

function CodeBlock({ code, lang }: CodeBlockProps) {
  const { theme } = useTheme()
  const [tokens, setTokens] = useState<TokenizedLine[] | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    let cancelled = false
    if (!lang) {
      setTokens(null)
      return () => {
        cancelled = true
      }
    }
    tokenizeCode(code, lang, theme.shiki).then((result) => {
      if (cancelled) return
      setTokens(result)
    })
    return () => {
      cancelled = true
    }
  }, [code, lang, theme.shiki])

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
    <div className="group relative my-1 overflow-hidden rounded-md border border-border/60 bg-card/60">
      <div className="flex items-center justify-between border-b border-border/60 bg-muted/40 px-3 py-1">
        <span className="select-none font-mono text-[10.5px] uppercase tracking-wider text-muted-foreground">
          {displayLang}
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
              <Check className="h-3 w-3" />
              Copied
            </>
          ) : (
            <>
              <Copy className="h-3 w-3" />
              Copy
            </>
          )}
        </button>
      </div>
      <pre className="m-0 overflow-x-auto px-3 py-2 font-mono text-[12.5px] leading-[1.55]">
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
