'use client'

import { Fragment, useCallback, useEffect, useMemo, useState } from 'react'
import type { ComponentProps, ReactNode } from 'react'
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  AlertCircle,
  Check,
  Code2,
  Copy,
  ExternalLink,
  FileAudio2,
  FileText,
  FileVideo2,
  Grid3X3,
  Image as ImageIcon,
  Info,
  Maximize2,
  Minimize2,
  MonitorX,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { formatBytes } from '@/lib/agent-attachments'
import { cn } from '@/lib/utils'
import {
  shouldSkipTokenization,
  tokenizeCode,
  type TokenizedLine,
} from '@/lib/shiki'
import { useTheme } from '@/src/features/theme/theme-provider'

const IMAGE_ZOOM_MIN = 0.25
const IMAGE_ZOOM_MAX = 4
const IMAGE_ZOOM_STEP = 0.25
const CSV_PREVIEW_ROW_LIMIT = 1_000
const CSV_PREVIEW_COLUMN_LIMIT = 80
const MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT = 96 * 1024

type ImageScaleMode = 'fit' | 'actual'

export interface ResolveAssetPreviewUrl {
  (projectPath: string): Promise<string | null>
}

interface PreviewFileActions {
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
}

export function ImagePreview({
  filePath,
  src,
  byteLength,
  mimeType,
  testId = 'image-preview',
  alt,
}: {
  filePath: string
  src: string
  byteLength: number
  mimeType: string
  testId?: string
  alt?: string
}) {
  const [scaleMode, setScaleMode] = useState<ImageScaleMode>('fit')
  const [zoom, setZoom] = useState(1)
  const [showCheckerboard, setShowCheckerboard] = useState(true)
  const [dimensions, setDimensions] = useState<{ width: number; height: number } | null>(null)
  const [loadState, setLoadState] = useState<'loading' | 'loaded' | 'error'>('loading')
  const fileName = basename(filePath)
  const displayAlt = alt?.trim() || `Preview of ${fileName}`
  const zoomPercent = Math.round(zoom * 100)

  useEffect(() => {
    setLoadState('loading')
    setDimensions(null)
  }, [src])

  const setActualSize = () => {
    setScaleMode('actual')
    setZoom(1)
  }

  const changeZoom = (delta: number) => {
    setScaleMode('actual')
    setZoom((current) => clamp(roundZoom(current + delta), IMAGE_ZOOM_MIN, IMAGE_ZOOM_MAX))
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-testid={testId}>
      <div
        className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border bg-secondary/10 px-3 py-1.5"
        role="toolbar"
        aria-label={`${fileName} image preview toolbar`}
      >
        <div className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
          <ImageIcon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="truncate font-mono text-foreground/85">{fileName}</span>
          <span className="text-muted-foreground/40" aria-hidden="true">
            ·
          </span>
          <span className="tabular-nums" data-testid={`${testId}-dimensions`}>
            {dimensions ? `${dimensions.width} x ${dimensions.height}` : 'Dimensions pending'}
          </span>
          <span className="text-muted-foreground/40" aria-hidden="true">
            ·
          </span>
          <span className="tabular-nums">{formatBytes(byteLength)}</span>
          {mimeType ? (
            <>
              <span className="text-muted-foreground/40" aria-hidden="true">
                ·
              </span>
              <span className="hidden font-mono sm:inline">{mimeType}</span>
            </>
          ) : null}
        </div>

        <div className="flex items-center gap-1">
          <ToggleGroup
            type="single"
            size="sm"
            value={scaleMode}
            onValueChange={(value) => {
              if (value === 'fit') {
                setScaleMode('fit')
                return
              }
              if (value === 'actual') {
                setActualSize()
              }
            }}
            aria-label="Image sizing"
          >
            <TooltipIconToggle value="fit" label="Fit to editor">
              <Minimize2 className="h-3.5 w-3.5" aria-hidden="true" />
            </TooltipIconToggle>
            <TooltipIconToggle value="actual" label="Actual size">
              <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
            </TooltipIconToggle>
          </ToggleGroup>

          <Separator orientation="vertical" className="mx-1 h-5" />

          <TooltipIconButton
            label="Zoom out"
            onClick={() => changeZoom(-IMAGE_ZOOM_STEP)}
            disabled={scaleMode === 'actual' && zoom <= IMAGE_ZOOM_MIN}
          >
            <ZoomOut className="h-3.5 w-3.5" aria-hidden="true" />
          </TooltipIconButton>
          <span
            className="min-w-10 text-center font-mono text-[11px] text-muted-foreground tabular-nums"
            aria-live="polite"
          >
            {scaleMode === 'fit' ? 'Fit' : `${zoomPercent}%`}
          </span>
          <TooltipIconButton
            label="Zoom in"
            onClick={() => changeZoom(IMAGE_ZOOM_STEP)}
            disabled={scaleMode === 'actual' && zoom >= IMAGE_ZOOM_MAX}
          >
            <ZoomIn className="h-3.5 w-3.5" aria-hidden="true" />
          </TooltipIconButton>

          <Separator orientation="vertical" className="mx-1 h-5" />

          <TooltipIconButton
            label={showCheckerboard ? 'Hide transparent background grid' : 'Show transparent background grid'}
            onClick={() => setShowCheckerboard((current) => !current)}
            aria-pressed={showCheckerboard}
          >
            <Grid3X3 className="h-3.5 w-3.5" aria-hidden="true" />
          </TooltipIconButton>
        </div>
      </div>

      <div
        className={cn(
          'relative flex min-h-0 flex-1 items-center justify-center overflow-auto p-4',
          showCheckerboard ? checkerboardClassName : 'bg-background',
        )}
      >
        {loadState === 'loading' ? (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-6">
            <Skeleton className="h-28 w-44" aria-label={`Loading preview of ${fileName}`} />
          </div>
        ) : null}

        {loadState === 'error' ? (
          <PreviewErrorState
            title={`Xero could not render ${fileName}`}
            description="The file was classified as an image, but the preview surface could not decode it."
          />
        ) : (
          <img
            src={src}
            alt={displayAlt}
            className={cn(
              'block rounded-sm transition-[opacity]',
              loadState === 'loading' && 'opacity-0',
              scaleMode === 'fit' ? 'max-h-full max-w-full object-contain' : 'max-w-none',
            )}
            style={
              scaleMode === 'actual' && dimensions
                ? {
                    height: dimensions.height * zoom,
                    width: dimensions.width * zoom,
                  }
                : undefined
            }
            onLoad={(event) => {
              const image = event.currentTarget
              setDimensions({
                width: image.naturalWidth,
                height: image.naturalHeight,
              })
              setLoadState('loaded')
            }}
            onError={() => setLoadState('error')}
          />
        )}
      </div>
    </div>
  )
}

export function SvgTextPreview({
  filePath,
  text,
  mimeType,
}: {
  filePath: string
  text: string
  mimeType: string
}) {
  const src = useSvgObjectUrl(text, mimeType)

  if (!src) {
    return (
      <div className="flex min-h-0 flex-1" data-testid="svg-preview">
        <PreviewErrorState
          title="SVG preview is unavailable"
          description="This environment does not expose a URL API for the preview surface."
        />
      </div>
    )
  }

  return (
    <div className="flex min-h-0 flex-1" data-testid="svg-preview">
      <ImagePreview
        filePath={filePath}
        src={src}
        byteLength={utf8ByteLength(text)}
        mimeType={mimeType || 'image/svg+xml'}
        testId="svg-image-preview"
        alt={`SVG preview of ${basename(filePath)}`}
      />
    </div>
  )
}

export function MarkdownPreview({
  filePath,
  text,
  onResolveAssetPreviewUrl,
}: {
  filePath: string
  text: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  const components = useMemo<Components>(
    () => createMarkdownComponents(filePath, onResolveAssetPreviewUrl),
    [filePath, onResolveAssetPreviewUrl],
  )

  if (text.trim().length === 0) {
    return (
      <PreviewEmptyState
        testId="markdown-preview"
        title="Empty Markdown file"
        description="Switch to Source to add content."
      />
    )
  }

  return (
    <ScrollArea className="min-h-0 flex-1 bg-background" data-testid="markdown-preview">
      <article
        aria-label={`Markdown preview of ${filePath}`}
        className={cn(
          'mx-auto w-full max-w-4xl px-8 py-7 text-[13px] leading-6 text-foreground',
          'prose-xero',
        )}
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={components}
          skipHtml
          urlTransform={sanitizeMarkdownUrl}
        >
          {text}
        </ReactMarkdown>
      </article>
    </ScrollArea>
  )
}

export function CsvPreview({
  filePath,
  mimeType,
  text,
}: {
  filePath: string
  mimeType: string
  text: string
}) {
  const delimiter = isTabSeparated(filePath, mimeType) ? '\t' : ','
  const parsed = useMemo(
    () =>
      parseDelimitedText(text, delimiter, {
        columnLimit: CSV_PREVIEW_COLUMN_LIMIT,
        rowLimit: CSV_PREVIEW_ROW_LIMIT,
      }),
    [delimiter, text],
  )
  const fileName = basename(filePath)

  if (parsed.rows.length === 0) {
    return (
      <PreviewEmptyState
        testId="csv-preview"
        title="Empty table file"
        description="Switch to Source to add rows."
      />
    )
  }

  const columnCount = Math.max(...parsed.rows.map((row) => row.length), 1)
  const headerRow = parsed.rows[0] ?? []
  const bodyRows = parsed.rows.slice(1)
  const headers = Array.from({ length: columnCount }, (_, index) => {
    const header = headerRow[index]?.trim()
    return header && header.length > 0 ? header : `Column ${index + 1}`
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-testid="csv-preview">
      <div
        className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border bg-secondary/10 px-3 py-1.5"
        role="toolbar"
        aria-label={`${fileName} table preview toolbar`}
      >
        <div className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
          <span className="truncate font-mono text-foreground/85">{fileName}</span>
          <span className="text-muted-foreground/40" aria-hidden="true">
            ·
          </span>
          <span>
            {parsed.totalRows.toLocaleString()} row{parsed.totalRows === 1 ? '' : 's'}
          </span>
          <span className="text-muted-foreground/40" aria-hidden="true">
            ·
          </span>
          <span>
            {parsed.maxColumns.toLocaleString()} column{parsed.maxColumns === 1 ? '' : 's'}
          </span>
        </div>
        {parsed.truncatedRows || parsed.truncatedColumns ? (
          <Badge variant="outline" className="text-[10px]">
            Preview limited to {CSV_PREVIEW_ROW_LIMIT.toLocaleString()} rows and{' '}
            {CSV_PREVIEW_COLUMN_LIMIT.toLocaleString()} columns
          </Badge>
        ) : null}
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="min-w-full p-3">
          <Table aria-label={`Table preview of ${filePath}`} className="text-[12px]">
            <TableHeader className="sticky top-0 z-10 bg-background">
              <TableRow>
                <TableHead className="w-12 bg-muted/60 text-right text-muted-foreground">
                  #
                </TableHead>
                {headers.map((header, index) => (
                  <TableHead
                    key={`${header}-${index}`}
                    className="max-w-64 border-l border-border/60 bg-muted/60 font-mono"
                    title={header}
                  >
                    <span className="block truncate">{header}</span>
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {bodyRows.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={headers.length + 1}
                    className="h-20 text-center text-muted-foreground"
                  >
                    No data rows.
                  </TableCell>
                </TableRow>
              ) : (
                bodyRows.map((row, rowIndex) => (
                  <TableRow key={rowIndex}>
                    <TableCell className="bg-muted/30 text-right font-mono text-muted-foreground">
                      {rowIndex + 1}
                    </TableCell>
                    {headers.map((_, columnIndex) => {
                      const cell = row[columnIndex] ?? ''
                      return (
                        <TableCell
                          key={columnIndex}
                          className="max-w-64 border-l border-border/40 font-mono"
                          title={cell}
                        >
                          <span className="block truncate">{cell}</span>
                        </TableCell>
                      )
                    })}
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
      </ScrollArea>
    </div>
  )
}

export function PdfPreview({
  byteLength,
  filePath,
  mimeType,
  src,
  onCopyPath,
  onOpenExternal,
}: PreviewFileActions & {
  byteLength: number
  filePath: string
  mimeType: string
  src: string
}) {
  const fileName = basename(filePath)

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-testid="pdf-preview">
      <PreviewToolbar
        ariaLabel={`${fileName} PDF preview toolbar`}
        fileName={fileName}
        icon={<FileText className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />}
        metadata={[formatBytes(byteLength), mimeType]}
        onCopyPath={onCopyPath ? () => onCopyPath(filePath) : undefined}
        onOpenExternal={onOpenExternal ? () => onOpenExternal(filePath) : undefined}
      />
      <object
        aria-label={`PDF preview of ${filePath}`}
        className="min-h-0 flex-1 border-0"
        data={src}
        type="application/pdf"
      >
        <PdfFallbackState
          fileName={fileName}
          filePath={filePath}
          onCopyPath={onCopyPath}
          onOpenExternal={onOpenExternal}
        />
      </object>
    </div>
  )
}

export function MediaPreview({
  byteLength,
  filePath,
  mimeType,
  rendererKind,
  src,
  onCopyPath,
  onOpenExternal,
}: PreviewFileActions & {
  byteLength: number
  filePath: string
  mimeType: string
  rendererKind: 'audio' | 'video'
  src: string
}) {
  const [loadState, setLoadState] = useState<'ready' | 'error'>('ready')
  const fileName = basename(filePath)
  const isAudio = rendererKind === 'audio'
  const Icon = isAudio ? FileAudio2 : FileVideo2

  useEffect(() => {
    setLoadState('ready')
  }, [src])

  return (
    <div
      className="flex min-h-0 flex-1 flex-col bg-background"
      data-testid={`${rendererKind}-preview`}
    >
      <PreviewToolbar
        ariaLabel={`${fileName} ${rendererKind} preview toolbar`}
        fileName={fileName}
        icon={<Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />}
        metadata={[formatBytes(byteLength), mimeType]}
        onCopyPath={onCopyPath ? () => onCopyPath(filePath) : undefined}
        onOpenExternal={onOpenExternal ? () => onOpenExternal(filePath) : undefined}
      />

      <div className="flex min-h-0 flex-1 items-center justify-center bg-background p-6">
        {loadState === 'error' ? (
          <PreviewErrorState
            title={`Xero could not play ${fileName}`}
            description="The file was classified as media, but the platform preview surface could not decode it."
          />
        ) : isAudio ? (
          <audio
            src={src}
            controls
            preload="metadata"
            className="w-full max-w-xl"
            onError={() => setLoadState('error')}
          />
        ) : (
          <video
            src={src}
            controls
            playsInline
            preload="metadata"
            className="max-h-full max-w-full rounded-sm bg-black"
            onError={() => setLoadState('error')}
          />
        )}
      </div>
    </div>
  )
}

export function UnsupportedFilePanel({
  byteLength,
  contentHash,
  filePath,
  mimeType,
  modifiedAt,
  reason,
  rendererKind,
  onCopyPath,
  onOpenExternal,
}: PreviewFileActions & {
  byteLength: number
  contentHash: string
  filePath: string
  mimeType: string | null
  modifiedAt: string
  reason: string
  rendererKind: string | null
}) {
  const fileName = basename(filePath)
  const reasonLabel = reason.replace(/_/g, ' ')
  const copyPath = useCallback(() => {
    if (onCopyPath) {
      onCopyPath(filePath)
      return
    }
    void writeClipboardText(filePath).catch(() => {})
  }, [filePath, onCopyPath])

  return (
    <div
      className="flex flex-1 items-center justify-center bg-background p-6"
      data-testid="unsupported-file-panel"
    >
      <div className="flex w-full max-w-xl flex-col items-center gap-4 text-center">
        <div className="flex h-12 w-12 items-center justify-center rounded-md border border-border bg-card">
          <Info className="h-6 w-6 text-muted-foreground" aria-hidden="true" />
        </div>
        <div>
          <p className="text-[14px] font-medium text-foreground">Xero cannot preview {fileName}</p>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">{reasonLabel}.</p>
        </div>

        <dl className="grid w-full grid-cols-[minmax(84px,auto)_minmax(0,1fr)] gap-x-3 gap-y-2 rounded-md border border-border bg-card/30 p-3 text-left text-[11px]">
          <dt className="text-right text-muted-foreground/70">Path</dt>
          <dd className="truncate font-mono text-foreground/80">{filePath}</dd>
          <dt className="text-right text-muted-foreground/70">Type</dt>
          <dd className="truncate font-mono text-foreground/80">{mimeType ?? 'unknown'}</dd>
          <dt className="text-right text-muted-foreground/70">Renderer</dt>
          <dd className="font-mono text-foreground/80">{rendererKind ?? 'binary'}</dd>
          <dt className="text-right text-muted-foreground/70">Size</dt>
          <dd className="font-mono text-foreground/80">{formatBytes(byteLength)}</dd>
          <dt className="text-right text-muted-foreground/70">Modified</dt>
          <dd className="truncate font-mono text-foreground/80">{formatModifiedAt(modifiedAt)}</dd>
          <dt className="text-right text-muted-foreground/70">Hash</dt>
          <dd className="truncate font-mono text-foreground/80">{shortHash(contentHash)}</dd>
        </dl>

        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button type="button" variant="outline" size="sm" onClick={copyPath}>
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
            Copy path
          </Button>
          {onOpenExternal ? (
            <Button type="button" variant="secondary" size="sm" onClick={() => onOpenExternal(filePath)}>
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
              Open externally
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}

export function PreviewUnavailablePanel({
  rendererKind,
  filePath,
}: {
  rendererKind: string
  filePath: string
}) {
  return (
    <PreviewEmptyState
      testId={`text-preview-placeholder:${rendererKind}`}
      title={`${rendererKind.toUpperCase()} preview is not available yet`}
      description={`Switch to Source to view or edit ${basename(filePath)}.`}
    />
  )
}

interface ParsedDelimitedText {
  maxColumns: number
  rows: string[][]
  totalRows: number
  truncatedColumns: boolean
  truncatedRows: boolean
}

export function parseDelimitedText(
  text: string,
  delimiter: ',' | '\t',
  limits: { rowLimit: number; columnLimit: number },
): ParsedDelimitedText {
  if (text.length === 0) {
    return {
      maxColumns: 0,
      rows: [],
      totalRows: 0,
      truncatedColumns: false,
      truncatedRows: false,
    }
  }

  const rows: string[][] = []
  let row: string[] = []
  let cell = ''
  let inQuotes = false
  let totalRows = 0
  let maxColumns = 0
  let truncatedColumns = false
  let truncatedRows = false

  const pushRow = () => {
    const completedRow = [...row, cell]
    totalRows += 1
    maxColumns = Math.max(maxColumns, completedRow.length)
    if (completedRow.length > limits.columnLimit) {
      truncatedColumns = true
    }
    if (rows.length < limits.rowLimit) {
      rows.push(completedRow.slice(0, limits.columnLimit))
    } else {
      truncatedRows = true
    }
    row = []
    cell = ''
  }

  for (let index = 0; index < text.length; index += 1) {
    const char = text[index]

    if (char === '"') {
      if (inQuotes && text[index + 1] === '"') {
        cell += '"'
        index += 1
      } else {
        inQuotes = !inQuotes
      }
      continue
    }

    if (!inQuotes && char === delimiter) {
      row.push(cell)
      cell = ''
      continue
    }

    if (!inQuotes && (char === '\n' || char === '\r')) {
      if (char === '\r' && text[index + 1] === '\n') {
        index += 1
      }
      pushRow()
      continue
    }

    cell += char
  }

  if (cell.length > 0 || row.length > 0 || text.endsWith(delimiter)) {
    pushRow()
  }

  return {
    maxColumns,
    rows,
    totalRows,
    truncatedColumns,
    truncatedRows,
  }
}

function createMarkdownComponents(
  filePath: string,
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl,
): Components {
  return {
    a({ children, href }) {
      const safeHref = sanitizeMarkdownUrl(href ?? '', 'href')
      if (!safeHref) {
        return <span className="text-foreground">{children}</span>
      }
      const external = isExternalHttpUrl(safeHref)
      return (
        <a
          href={safeHref}
          target={external ? '_blank' : undefined}
          rel={external ? 'noreferrer noopener' : undefined}
          className="text-primary underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          {children}
        </a>
      )
    },
    blockquote({ children }) {
      return (
        <blockquote className="my-3 border-l-2 border-primary/40 pl-3 text-muted-foreground">
          {children}
        </blockquote>
      )
    },
    code({ children, className }) {
      const code = String(children).replace(/\n$/, '')
      const language = /language-([^\s]+)/.exec(className ?? '')?.[1] ?? null

      if (language) {
        return <CodePreviewBlock code={code} lang={language} />
      }

      return (
        <code className="rounded bg-muted/70 px-1 py-px font-mono text-[0.88em] text-foreground">
          {children}
        </code>
      )
    },
    h1({ children }) {
      return <h1 className="mb-4 mt-0 text-[24px] font-semibold leading-tight">{children}</h1>
    },
    h2({ children }) {
      return <h2 className="mb-3 mt-6 border-b border-border pb-1 text-[19px] font-semibold">{children}</h2>
    },
    h3({ children }) {
      return <h3 className="mb-2 mt-5 text-[16px] font-semibold">{children}</h3>
    },
    h4({ children }) {
      return <h4 className="mb-2 mt-4 text-[14px] font-semibold">{children}</h4>
    },
    hr() {
      return <Separator className="my-5" />
    },
    img({ alt, src }) {
      return (
        <MarkdownImage
          alt={alt ?? ''}
          filePath={filePath}
          src={src ?? ''}
          onResolveAssetPreviewUrl={onResolveAssetPreviewUrl}
        />
      )
    },
    li({ children }) {
      return <li className="my-0.5 pl-1">{children}</li>
    },
    ol({ children }) {
      return <ol className="my-3 list-decimal space-y-1 pl-6">{children}</ol>
    },
    p({ children }) {
      return <p className="my-3 whitespace-pre-wrap break-words">{children}</p>
    },
    pre({ children }) {
      return <>{children}</>
    },
    table({ children }) {
      return (
        <div className="my-4 overflow-x-auto rounded-md border border-border">
          <Table className="text-[12px]">{children}</Table>
        </div>
      )
    },
    tbody({ children }) {
      return <TableBody>{children}</TableBody>
    },
    td({ children }) {
      return <TableCell className="border-l border-border/50 font-mono">{children}</TableCell>
    },
    th({ children }) {
      return <TableHead className="border-l border-border/50 bg-muted/60 font-mono">{children}</TableHead>
    },
    thead({ children }) {
      return <TableHeader>{children}</TableHeader>
    },
    tr({ children }) {
      return <TableRow>{children}</TableRow>
    },
    ul({ children }) {
      return <ul className="my-3 list-disc space-y-1 pl-6">{children}</ul>
    },
  }
}

function MarkdownImage({
  alt,
  filePath,
  src,
  onResolveAssetPreviewUrl,
}: {
  alt: string
  filePath: string
  src: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  const [state, setState] = useState<{
    status: 'loading' | 'loaded' | 'error'
    url: string | null
  }>({ status: 'loading', url: null })
  const externalUrl = useMemo(() => sanitizeExternalImageUrl(src), [src])
  const projectPath = useMemo(
    () => (externalUrl ? null : normalizeProjectReference(filePath, src)),
    [externalUrl, filePath, src],
  )
  const altText = alt.trim() || basenameFromReference(src) || 'Markdown image'

  useEffect(() => {
    if (externalUrl) {
      setState({ status: 'loaded', url: externalUrl })
      return
    }

    if (!projectPath || !onResolveAssetPreviewUrl) {
      setState({ status: 'error', url: null })
      return
    }

    let cancelled = false
    setState({ status: 'loading', url: null })
    onResolveAssetPreviewUrl(projectPath).then((url) => {
      if (cancelled) return
      setState(url ? { status: 'loaded', url } : { status: 'error', url: null })
    })

    return () => {
      cancelled = true
    }
  }, [externalUrl, onResolveAssetPreviewUrl, projectPath])

  return (
    <figure className="my-4" data-testid="markdown-image">
      <div
        className={cn(
          'flex min-h-24 items-center justify-center overflow-hidden rounded-md border border-border',
          checkerboardClassName,
        )}
      >
        {state.status === 'loading' ? (
          <Skeleton className="h-24 w-full max-w-sm" aria-label={`Loading ${altText}`} />
        ) : null}
        {state.status === 'error' ? (
          <div className="flex items-center gap-2 px-4 py-3 text-[12px] text-muted-foreground">
            <AlertCircle className="h-3.5 w-3.5" aria-hidden="true" />
            Image unavailable: <span className="font-mono">{src}</span>
          </div>
        ) : null}
        {state.status === 'loaded' && state.url ? (
          <img
            src={state.url}
            alt={altText}
            referrerPolicy="no-referrer"
            className="max-h-[520px] max-w-full object-contain"
            onError={() => setState({ status: 'error', url: null })}
          />
        ) : null}
      </div>
      {alt.trim().length > 0 ? (
        <figcaption className="mt-1 text-center text-[11px] text-muted-foreground">{alt}</figcaption>
      ) : null}
    </figure>
  )
}

function CodePreviewBlock({ code, lang }: { code: string; lang: string }) {
  const { theme } = useTheme()
  const [tokens, setTokens] = useState<TokenizedLine[] | null>(null)
  const [copied, setCopied] = useState(false)
  const [renderingPlain, setRenderingPlain] = useState(false)
  const isTooLargeToHighlight = shouldSkipTokenization(
    code,
    MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT,
  )

  useEffect(() => {
    let cancelled = false
    setTokens(null)
    if (isTooLargeToHighlight) {
      setRenderingPlain(true)
      return () => {
        cancelled = true
      }
    }

    setRenderingPlain(false)
    tokenizeCode(code, lang, theme.shiki, {
      maxBytes: MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT,
    }).then((result) => {
      if (cancelled) return
      setTokens(result)
      setRenderingPlain(result === null)
    })

    return () => {
      cancelled = true
    }
  }, [code, isTooLargeToHighlight, lang, theme.shiki])

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
      // Clipboard writes can fail under test or restricted WebView contexts.
    }
  }

  return (
    <div className="group my-3 overflow-hidden rounded-md border border-border bg-card/60">
      <div className="flex items-center justify-between border-b border-border bg-muted/40 px-2.5 py-1">
        <span className="flex items-center gap-2">
          <Code2 className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
          <span className="font-mono text-[10px] uppercase text-muted-foreground">{lang}</span>
          {renderingPlain ? (
            <Badge variant="outline" className="px-1.5 py-0 text-[9px]">
              Plain
            </Badge>
          ) : null}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={handleCopy}
          aria-label={copied ? 'Copied code to clipboard' : 'Copy code'}
          className="h-6 px-1.5 text-[10px] text-muted-foreground"
        >
          {copied ? <Check className="h-3 w-3" aria-hidden="true" /> : <Copy className="h-3 w-3" aria-hidden="true" />}
          {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>
      <pre className="m-0 overflow-x-auto px-3 py-2 font-mono text-[12px] leading-[1.55]">
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

function PreviewToolbar({
  ariaLabel,
  fileName,
  icon,
  metadata,
  onCopyPath,
  onOpenExternal,
}: {
  ariaLabel: string
  fileName: string
  icon: ReactNode
  metadata: string[]
  onCopyPath?: () => void
  onOpenExternal?: () => void
}) {
  return (
    <div
      className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border bg-secondary/10 px-3 py-1.5"
      role="toolbar"
      aria-label={ariaLabel}
    >
      <div className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
        {icon}
        <span className="truncate font-mono text-foreground/85">{fileName}</span>
        {metadata.filter(Boolean).map((item, index) => (
          <Fragment key={`${item}-${index}`}>
            <span className="text-muted-foreground/40" aria-hidden="true">
              ·
            </span>
            <span className="truncate font-mono tabular-nums">{item}</span>
          </Fragment>
        ))}
      </div>

      {onCopyPath || onOpenExternal ? (
        <div className="flex items-center gap-1">
          {onCopyPath ? (
            <TooltipIconButton label="Copy path" onClick={onCopyPath}>
              <Copy className="h-3.5 w-3.5" aria-hidden="true" />
            </TooltipIconButton>
          ) : null}
          {onOpenExternal ? (
            <TooltipIconButton label="Open externally" onClick={onOpenExternal}>
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            </TooltipIconButton>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function PdfFallbackState({
  fileName,
  filePath,
  onCopyPath,
  onOpenExternal,
}: PreviewFileActions & {
  fileName: string
  filePath: string
}) {
  return (
    <div className="flex h-full min-h-64 flex-1 items-center justify-center bg-background p-6 text-center">
      <div className="flex max-w-md flex-col items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-md border border-border bg-card">
          <MonitorX className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
        </div>
        <div>
          <p className="text-[14px] font-medium text-foreground">PDF preview is unavailable</p>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            {fileName} cannot be rendered by this WebView.
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-center gap-2">
          {onCopyPath ? (
            <Button type="button" variant="outline" size="sm" onClick={() => onCopyPath(filePath)}>
              <Copy className="h-3.5 w-3.5" aria-hidden="true" />
              Copy path
            </Button>
          ) : null}
          {onOpenExternal ? (
            <Button type="button" variant="secondary" size="sm" onClick={() => onOpenExternal(filePath)}>
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
              Open externally
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}

function PreviewEmptyState({
  description,
  testId,
  title,
}: {
  description: string
  testId: string
  title: string
}) {
  return (
    <div
      className="flex flex-1 items-center justify-center bg-background p-6 text-center"
      data-testid={testId}
    >
      <div className="max-w-md">
        <p className="text-[14px] font-medium text-foreground">{title}</p>
        <p className="mt-1 text-[12px] leading-5 text-muted-foreground">{description}</p>
      </div>
    </div>
  )
}

function PreviewErrorState({
  description,
  title,
}: {
  description: string
  title: string
}) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background p-6 text-center">
      <div className="flex max-w-md flex-col items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-md border border-border bg-card">
          <AlertCircle className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
        </div>
        <div>
          <p className="text-[14px] font-medium text-foreground">{title}</p>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">{description}</p>
        </div>
      </div>
    </div>
  )
}

function TooltipIconButton({
  children,
  label,
  ...props
}: Omit<ComponentProps<typeof Button>, 'size' | 'variant'> & {
  children: ReactNode
  label: string
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button type="button" variant="ghost" size="icon-sm" aria-label={label} {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

function TooltipIconToggle({
  children,
  label,
  value,
}: {
  children: ReactNode
  label: string
  value: ImageScaleMode
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <ToggleGroupItem value={value} aria-label={label} className="h-8 w-8 flex-none px-0">
          {children}
        </ToggleGroupItem>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

function useSvgObjectUrl(text: string, mimeType: string): string | null {
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    const type = mimeType || 'image/svg+xml'
    if (
      typeof URL !== 'undefined' &&
      typeof URL.createObjectURL === 'function' &&
      typeof Blob !== 'undefined'
    ) {
      const nextUrl = URL.createObjectURL(new Blob([text], { type }))
      setUrl(nextUrl)
      return () => URL.revokeObjectURL(nextUrl)
    }

    setUrl(`data:${type};charset=utf-8,${encodeURIComponent(text)}`)
    return undefined
  }, [mimeType, text])

  return url
}

function sanitizeMarkdownUrl(value: string, key = 'href'): string {
  const safe = defaultUrlTransform(value)
  if (!safe) return ''
  if (key === 'src' && !isExternalHttpUrl(safe) && !isRelativeReference(safe)) {
    return ''
  }
  return safe
}

function sanitizeExternalImageUrl(value: string): string | null {
  const safe = defaultUrlTransform(value)
  if (!safe || !isExternalHttpUrl(safe)) {
    return null
  }
  return safe
}

function isExternalHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value)
}

function isRelativeReference(value: string): boolean {
  return !/^[a-z][a-z0-9+.-]*:/i.test(value)
}

function normalizeProjectReference(filePath: string, reference: string): string | null {
  const trimmed = reference.trim()
  if (!trimmed || !isRelativeReference(trimmed) || trimmed.startsWith('#')) {
    return null
  }

  const pathPart = trimmed.split(/[?#]/, 1)[0]?.replace(/\\/g, '/') ?? ''
  if (!pathPart) {
    return null
  }

  let decoded = pathPart
  try {
    decoded = decodeURIComponent(pathPart)
  } catch {
    return null
  }

  const basePath = decoded.startsWith('/') ? decoded : `${parentProjectPath(filePath)}/${decoded}`
  const segments: string[] = []
  for (const rawSegment of basePath.split('/')) {
    const segment = rawSegment.trim()
    if (!segment || segment === '.') continue
    if (segment === '..') {
      if (segments.length === 0) {
        return null
      }
      segments.pop()
      continue
    }
    segments.push(segment)
  }

  return `/${segments.join('/')}`
}

function parentProjectPath(filePath: string): string {
  const parts = filePath.split('/').filter(Boolean)
  parts.pop()
  return `/${parts.join('/')}`.replace(/\/$/, '') || '/'
}

function basename(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? path
}

function basenameFromReference(reference: string): string {
  const withoutQuery = reference.split(/[?#]/, 1)[0] ?? reference
  return basename(withoutQuery)
}

function isTabSeparated(filePath: string, mimeType: string): boolean {
  return filePath.toLowerCase().endsWith('.tsv') || mimeType.includes('tab-separated-values')
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function roundZoom(value: number): number {
  return Math.round(value / IMAGE_ZOOM_STEP) * IMAGE_ZOOM_STEP
}

function utf8ByteLength(value: string): number {
  if (typeof TextEncoder !== 'undefined') {
    return new TextEncoder().encode(value).byteLength
  }
  return value.length
}

function shortHash(value: string): string {
  return value.length > 16 ? `${value.slice(0, 16)}...` : value
}

function formatModifiedAt(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }
  return date.toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

async function writeClipboardText(value: string): Promise<void> {
  if (typeof navigator === 'undefined' || !navigator.clipboard) {
    return
  }
  await navigator.clipboard.writeText(value)
}

const checkerboardClassName = cn(
  'bg-[length:16px_16px]',
  'bg-[linear-gradient(45deg,rgba(0,0,0,0.05)_25%,transparent_25%,transparent_75%,rgba(0,0,0,0.05)_75%),linear-gradient(45deg,rgba(0,0,0,0.05)_25%,transparent_25%,transparent_75%,rgba(0,0,0,0.05)_75%)]',
  'bg-[position:0_0,8px_8px]',
)
