'use client'

import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  AlertCircle,
  Check,
  Code2,
  Copy,
  ExternalLink,
  Grid3X3,
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
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { formatBytes } from '@/lib/agent-attachments'
import { cn } from '@/lib/utils'
import {
  shouldSkipTokenization,
  tokenizeCode,
  type TokenizedLine,
} from '@/lib/shiki'
import { useTheme } from '@/src/features/theme/theme-provider'
import type {
  ProjectFileCsvPreviewDto,
  ProjectFileMarkdownPreviewDto,
} from '@/src/lib/xero-model/project'

const IMAGE_ZOOM_MIN = 0.25
const IMAGE_ZOOM_MAX = 4
const IMAGE_ZOOM_STEP = 0.25
const CSV_PREVIEW_ROW_LIMIT = 1_000
const CSV_PREVIEW_COLUMN_LIMIT = 80
const MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT = 96 * 1024
const HTML_PREVIEW_IMAGE_LIMIT = 40

export type ImageScaleMode = 'fit' | 'actual'

export interface ImageControlsState {
  scaleMode: ImageScaleMode
  zoom: number
  showCheckerboard: boolean
}

export const DEFAULT_IMAGE_CONTROLS: ImageControlsState = {
  scaleMode: 'fit',
  zoom: 1,
  showCheckerboard: true,
}

export interface ImageDimensions {
  width: number
  height: number
}

export interface ResolveAssetPreviewUrl {
  (projectPath: string): Promise<AssetPreviewResolution | string | null>
}

export type AssetPreviewIssueReason =
  | 'missing'
  | 'oversized'
  | 'unsupportedType'
  | 'blocked'
  | 'unavailable'

export interface AssetPreviewResolution {
  path: string
  url: string | null
  reason?: AssetPreviewIssueReason
  mimeType?: string | null
  byteLength?: number
  rendererKind?: string | null
  message?: string
}

interface PreviewFileActions {
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
}

interface PreviewDiagnostic {
  code: string
  message: string
  severity: 'info' | 'warning' | 'error'
}

export function ImagePreview({
  filePath,
  src,
  testId = 'image-preview',
  alt,
  controls = DEFAULT_IMAGE_CONTROLS,
  onDimensionsChange,
}: {
  filePath: string
  src: string
  testId?: string
  alt?: string
  controls?: ImageControlsState
  onDimensionsChange?: (dimensions: ImageDimensions | null) => void
}) {
  const { scaleMode, zoom, showCheckerboard } = controls
  const [dimensions, setDimensions] = useState<ImageDimensions | null>(null)
  const [loadState, setLoadState] = useState<'loading' | 'loaded' | 'error'>('loading')
  const fileName = basename(filePath)
  const displayAlt = alt?.trim() || `Preview of ${fileName}`

  useEffect(() => {
    setLoadState('loading')
    setDimensions(null)
    onDimensionsChange?.(null)
  }, [src, onDimensionsChange])

  return (
    <div
      className={cn(
        'relative flex min-h-0 flex-1 items-center justify-center overflow-auto p-4',
        showCheckerboard ? checkerboardClassName : 'bg-background',
      )}
      data-testid={testId}
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
            const next = { width: image.naturalWidth, height: image.naturalHeight }
            setDimensions(next)
            setLoadState('loaded')
            onDimensionsChange?.(next)
          }}
          onError={() => setLoadState('error')}
        />
      )}
    </div>
  )
}

export function ImageControls({
  controls,
  onControlsChange,
}: {
  controls: ImageControlsState
  onControlsChange: (next: ImageControlsState) => void
}) {
  const { scaleMode, zoom, showCheckerboard } = controls
  const zoomPercent = Math.round(zoom * 100)

  const setScaleMode = (mode: ImageScaleMode) => {
    onControlsChange({ ...controls, scaleMode: mode, zoom: mode === 'actual' ? 1 : zoom })
  }

  const changeZoom = (delta: number) => {
    onControlsChange({
      ...controls,
      scaleMode: 'actual',
      zoom: clamp(roundZoom(zoom + delta), IMAGE_ZOOM_MIN, IMAGE_ZOOM_MAX),
    })
  }

  return (
    <div className="flex items-center gap-1.5" role="group" aria-label="Image controls">
      <div
        role="radiogroup"
        aria-label="Image sizing"
        className="inline-flex h-6 items-center rounded-md bg-secondary/40 p-0.5"
      >
        <CompactToggleIcon
          active={scaleMode === 'fit'}
          label="Fit to editor"
          onClick={() => setScaleMode('fit')}
        >
          <Minimize2 className="h-3 w-3" aria-hidden="true" />
        </CompactToggleIcon>
        <CompactToggleIcon
          active={scaleMode === 'actual'}
          label="Actual size"
          onClick={() => setScaleMode('actual')}
        >
          <Maximize2 className="h-3 w-3" aria-hidden="true" />
        </CompactToggleIcon>
      </div>

      <div className="flex items-center gap-0.5">
        <CompactIconButton
          label="Zoom out"
          onClick={() => changeZoom(-IMAGE_ZOOM_STEP)}
          disabled={scaleMode === 'actual' && zoom <= IMAGE_ZOOM_MIN}
        >
          <ZoomOut className="h-3 w-3" aria-hidden="true" />
        </CompactIconButton>
        <span
          className="min-w-9 text-center font-mono text-[10px] text-muted-foreground tabular-nums"
          aria-live="polite"
        >
          {scaleMode === 'fit' ? 'Fit' : `${zoomPercent}%`}
        </span>
        <CompactIconButton
          label="Zoom in"
          onClick={() => changeZoom(IMAGE_ZOOM_STEP)}
          disabled={scaleMode === 'actual' && zoom >= IMAGE_ZOOM_MAX}
        >
          <ZoomIn className="h-3 w-3" aria-hidden="true" />
        </CompactIconButton>
      </div>

      <CompactIconButton
        label={showCheckerboard ? 'Hide transparent background grid' : 'Show transparent background grid'}
        onClick={() => onControlsChange({ ...controls, showCheckerboard: !showCheckerboard })}
        active={showCheckerboard}
      >
        <Grid3X3 className="h-3 w-3" aria-hidden="true" />
      </CompactIconButton>
    </div>
  )
}

function CompactIconButton({
  active,
  children,
  disabled,
  label,
  onClick,
}: {
  active?: boolean
  children: ReactNode
  disabled?: boolean
  label: string
  onClick: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-pressed={active}
          disabled={disabled}
          onClick={onClick}
          className={cn(
            'inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
            'hover:bg-secondary/50 hover:text-foreground disabled:pointer-events-none disabled:opacity-40',
            active && 'bg-secondary/60 text-foreground',
          )}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

function CompactToggleIcon({
  active,
  children,
  label,
  onClick,
}: {
  active: boolean
  children: ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          role="radio"
          type="button"
          aria-label={label}
          aria-checked={active}
          onClick={onClick}
          className={cn(
            'inline-flex h-5 w-6 items-center justify-center rounded transition-colors',
            active
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground',
          )}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

export function SvgTextPreview({
  filePath,
  text,
  mimeType,
  controls,
  onDimensionsChange,
}: {
  filePath: string
  text: string
  mimeType: string
  controls?: ImageControlsState
  onDimensionsChange?: (dimensions: ImageDimensions | null) => void
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
        testId="svg-image-preview"
        alt={`SVG preview of ${basename(filePath)}`}
        controls={controls}
        onDimensionsChange={onDimensionsChange}
      />
    </div>
  )
}

export function MarkdownPreview({
  filePath,
  preview,
  sourceLine,
  text,
  onResolveAssetPreviewUrl,
}: {
  filePath: string
  preview?: ProjectFileMarkdownPreviewDto | null
  sourceLine?: number
  text: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  const scrollRootRef = useRef<HTMLDivElement | null>(null)
  const markdownAssetsBySource = useMemo(
    () =>
      preview
        ? new Map(preview.assetRefs.map((assetRef) => [assetRef.source, assetRef]))
        : null,
    [preview],
  )
  const previewDiagnostics = useMemo(
    () => buildMarkdownPreviewDiagnostics(filePath, text, preview),
    [filePath, preview, text],
  )
  const components = useMemo<Components>(
    () => createMarkdownComponents(filePath, markdownAssetsBySource, onResolveAssetPreviewUrl),
    [filePath, markdownAssetsBySource, onResolveAssetPreviewUrl],
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

  useEffect(() => {
    if (!sourceLine || sourceLine < 1) return
    const root = scrollRootRef.current
    const viewport = root?.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]')
    if (!viewport) return
    const sourceBlocks = Array.from(
      viewport.querySelectorAll<HTMLElement>('[data-source-line]'),
    )
    let target: HTMLElement | null = null
    let targetLine = 0
    for (const block of sourceBlocks) {
      const line = Number(block.dataset.sourceLine)
      if (!Number.isFinite(line) || line < 1 || line > sourceLine || line < targetLine) {
        continue
      }
      target = block
      targetLine = line
    }
    if (!target) return
    viewport.scrollTop = Math.max(0, target.offsetTop - 24)
  }, [sourceLine, text])

  return (
    <ScrollArea
      ref={scrollRootRef}
      className="min-h-0 flex-1 bg-background"
      data-testid="markdown-preview"
    >
      <article
        aria-label={`Markdown preview of ${filePath}`}
        className={cn(
          'mx-auto w-full max-w-4xl px-8 py-7 text-[13px] leading-6 text-foreground',
          'prose-xero',
        )}
      >
        <PreviewDiagnosticsPanel diagnostics={previewDiagnostics} />
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

export function HtmlPreview({
  filePath,
  text,
  onResolveAssetPreviewUrl,
}: {
  filePath: string
  text: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  const [state, setState] = useState<{
    diagnostics: PreviewDiagnostic[]
    srcDoc: string
  }>(() => buildHtmlPreviewDocument(text, filePath))

  useEffect(() => {
    let cancelled = false
    const build = async () => {
      const next = await buildHtmlPreviewDocumentAsync(text, filePath, onResolveAssetPreviewUrl)
      if (!cancelled) {
        setState(next)
      }
    }
    void build()
    return () => {
      cancelled = true
    }
  }, [filePath, onResolveAssetPreviewUrl, text])

  if (text.trim().length === 0) {
    return (
      <PreviewEmptyState
        testId="html-preview"
        title="Empty HTML file"
        description="Switch to Source to add markup."
      />
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-testid="html-preview">
      <PreviewDiagnosticsPanel diagnostics={state.diagnostics} />
      <iframe
        title={`HTML preview of ${filePath}`}
        aria-label={`HTML preview of ${filePath}`}
        className="min-h-0 flex-1 border-0 bg-white"
        referrerPolicy="no-referrer"
        sandbox=""
        srcDoc={state.srcDoc}
      />
    </div>
  )
}

export function CsvPreview({
  filePath,
  mimeType,
  preview,
  text,
}: {
  filePath: string
  mimeType: string
  preview?: ProjectFileCsvPreviewDto | null
  text: string
}) {
  const delimiter = isTabSeparated(filePath, mimeType) ? '\t' : ','
  const parsedFallback = useMemo(
    () =>
      preview
        ? null
        : parseDelimitedText(text, delimiter, {
            columnLimit: CSV_PREVIEW_COLUMN_LIMIT,
            rowLimit: CSV_PREVIEW_ROW_LIMIT,
          }),
    [delimiter, preview, text],
  )
  const table = preview ? csvPreviewTable(preview) : parsedFallback ?? EMPTY_PARSED_DELIMITED_TEXT
  const fileName = basename(filePath)

  if (table.rows.length === 0) {
    return (
      <PreviewEmptyState
        testId="csv-preview"
        title="Empty table file"
        description="Switch to Source to add rows."
      />
    )
  }

  const columnCount = Math.max(table.maxColumns, ...table.rows.map((row) => row.length), 1)
  const headerRow = table.rows[0] ?? []
  const bodyRows = table.rows.slice(1)
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
            {table.totalRows.toLocaleString()} row{table.totalRows === 1 ? '' : 's'}
          </span>
          <span className="text-muted-foreground/40" aria-hidden="true">
            ·
          </span>
          <span>
            {table.maxColumns.toLocaleString()} column{table.maxColumns === 1 ? '' : 's'}
          </span>
        </div>
        {table.truncatedRows || table.truncatedColumns ? (
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
  filePath,
  src,
  onCopyPath,
  onOpenExternal,
}: PreviewFileActions & {
  filePath: string
  src: string
}) {
  const fileName = basename(filePath)

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-testid="pdf-preview">
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
  filePath,
  rendererKind,
  src,
}: {
  filePath: string
  rendererKind: 'audio' | 'video'
  src: string
}) {
  const [loadState, setLoadState] = useState<'ready' | 'error'>('ready')
  const fileName = basename(filePath)
  const isAudio = rendererKind === 'audio'

  useEffect(() => {
    setLoadState('ready')
  }, [src])

  return (
    <div
      className="flex min-h-0 flex-1 flex-col bg-background"
      data-testid={`${rendererKind}-preview`}
    >
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

function PreviewDiagnosticsPanel({ diagnostics }: { diagnostics: PreviewDiagnostic[] }) {
  if (diagnostics.length === 0) {
    return null
  }

  const visibleDiagnostics = diagnostics.slice(0, 3)
  const hiddenCount = diagnostics.length - visibleDiagnostics.length
  const hasError = diagnostics.some((diagnostic) => diagnostic.severity === 'error')

  return (
    <div
      className={cn(
        'mb-4 flex flex-wrap items-start gap-2 rounded-md border px-3 py-2 text-[11px] leading-5',
        hasError
          ? 'border-destructive/30 bg-destructive/5 text-destructive'
          : 'border-border bg-secondary/20 text-muted-foreground',
      )}
      role="status"
      aria-live="polite"
    >
      <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1 space-y-1">
        {visibleDiagnostics.map((diagnostic) => (
          <p key={`${diagnostic.code}:${diagnostic.message}`} className="break-words">
            {diagnostic.message}
          </p>
        ))}
      </div>
      {hiddenCount > 0 ? (
        <Badge variant="outline" className="shrink-0 text-[10px]">
          +{hiddenCount}
        </Badge>
      ) : null}
    </div>
  )
}

function buildMarkdownPreviewDiagnostics(
  filePath: string,
  text: string,
  preview?: ProjectFileMarkdownPreviewDto | null,
): PreviewDiagnostic[] {
  const diagnostics: PreviewDiagnostic[] = []
  const unavailableSavedAssets = preview?.assetRefs.filter((assetRef) => !assetRef.previewUrl) ?? []
  if (unavailableSavedAssets.length > 0) {
    diagnostics.push({
      code: 'markdown.assets.unavailable',
      severity: 'warning',
      message: `${pluralize(unavailableSavedAssets.length, 'saved image reference')} could not be previewed. Check that the path exists, the file is an image, and the asset is within the preview size limit.`,
    })
  }

  const imageReferences = extractMarkdownImageReferences(text)
  const blockedRelativeImages = imageReferences.filter(
    (reference) => !sanitizeExternalImageUrl(reference) && !normalizeProjectReference(filePath, reference),
  )
  if (blockedRelativeImages.length > 0) {
    diagnostics.push({
      code: 'markdown.assets.blocked',
      severity: 'warning',
      message: `${pluralize(blockedRelativeImages.length, 'image reference')} could not be resolved inside this project.`,
    })
  }

  if (containsMarkdownHtml(text)) {
    diagnostics.push({
      code: 'markdown.html.omitted',
      severity: 'info',
      message: 'Embedded HTML is omitted from Markdown preview for safety.',
    })
  }

  return diagnostics
}

function buildHtmlPreviewDocument(
  text: string,
  filePath: string,
): { diagnostics: PreviewDiagnostic[]; srcDoc: string } {
  const { diagnostics, doc } = sanitizeHtmlPreviewDocument(text, filePath)
  return {
    diagnostics,
    srcDoc: serializeHtmlPreviewDocument(doc, text),
  }
}

async function buildHtmlPreviewDocumentAsync(
  text: string,
  filePath: string,
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl,
): Promise<{ diagnostics: PreviewDiagnostic[]; srcDoc: string }> {
  const { diagnostics, doc, imageSources } = sanitizeHtmlPreviewDocument(text, filePath)
  if (!onResolveAssetPreviewUrl || imageSources.length === 0) {
    return {
      diagnostics,
      srcDoc: serializeHtmlPreviewDocument(doc, text),
    }
  }

  const limitedImageSources = imageSources.slice(0, HTML_PREVIEW_IMAGE_LIMIT)
  if (imageSources.length > HTML_PREVIEW_IMAGE_LIMIT) {
    diagnostics.push({
      code: 'html.assets.limited',
      severity: 'warning',
      message: `Resolved the first ${HTML_PREVIEW_IMAGE_LIMIT.toLocaleString()} image references. Extra images stay inert until the preview is saved or simplified.`,
    })
  }

  const resolutions = await Promise.all(
    limitedImageSources.map(async (imageSource) => {
      const projectPath = normalizeProjectReference(filePath, imageSource.source)
      if (!projectPath) {
        return {
          imageSource,
          resolution: {
            path: imageSource.source,
            reason: 'blocked' as const,
            url: null,
          },
        }
      }

      try {
        return {
          imageSource,
          resolution: normalizeAssetPreviewResolution(
            projectPath,
            await onResolveAssetPreviewUrl(projectPath),
          ),
        }
      } catch (error) {
        return {
          imageSource,
          resolution: {
            path: projectPath,
            reason: 'missing' as const,
            url: null,
            message: error instanceof Error ? error.message : String(error),
          },
        }
      }
    }),
  )

  for (const { imageSource, resolution } of resolutions) {
    if (resolution.url) {
      imageSource.element.setAttribute('src', resolution.url)
      continue
    }

    imageSource.element.removeAttribute('src')
    imageSource.element.setAttribute('data-xero-preview-missing-src', imageSource.source)
    diagnostics.push({
      code: `html.assets.${resolution.reason ?? 'unavailable'}`,
      severity: 'warning',
      message: `Image unavailable: ${imageSource.source} (${assetPreviewIssueLabel(resolution)}).`,
    })
  }

  return {
    diagnostics,
    srcDoc: serializeHtmlPreviewDocument(doc, text),
  }
}

function sanitizeHtmlPreviewDocument(
  text: string,
  _filePath: string,
): {
  diagnostics: PreviewDiagnostic[]
  doc: Document | null
  imageSources: Array<{ element: HTMLImageElement; source: string }>
} {
  if (typeof DOMParser === 'undefined') {
    return {
      diagnostics: [
        {
          code: 'html.parser.unavailable',
          severity: 'warning',
          message: 'HTML preview is rendered as escaped text because this WebView has no DOM parser.',
        },
      ],
      doc: null,
      imageSources: [],
    }
  }

  const diagnostics: PreviewDiagnostic[] = []
  const doc = new DOMParser().parseFromString(text, 'text/html')
  let removedElements = 0
  let removedAttributes = 0

  for (const selector of ['script', 'iframe', 'object', 'embed', 'applet', 'base']) {
    for (const element of Array.from(doc.querySelectorAll(selector))) {
      element.remove()
      removedElements += 1
    }
  }

  for (const element of Array.from(doc.querySelectorAll('meta[http-equiv]'))) {
    if (element.getAttribute('http-equiv')?.trim().toLowerCase() === 'refresh') {
      element.remove()
      removedElements += 1
    }
  }

  for (const element of Array.from(doc.querySelectorAll('*'))) {
    for (const attribute of Array.from(element.attributes)) {
      const attributeName = attribute.name.toLowerCase()
      if (
        attributeName.startsWith('on') ||
        attributeName === 'srcdoc' ||
        (attributeName === 'style' && containsUnsafeCssUrl(attribute.value)) ||
        (HTML_URL_ATTRIBUTES.has(attributeName) &&
          !isSafeHtmlAttributeUrl(attribute.value, attributeName, element.tagName))
      ) {
        element.removeAttribute(attribute.name)
        removedAttributes += 1
      }
    }
  }

  if (removedElements > 0 || removedAttributes > 0) {
    diagnostics.push({
      code: 'html.sanitized',
      severity: 'warning',
      message: `Removed ${pluralize(removedElements, 'blocked element')} and ${pluralize(removedAttributes, 'unsafe attribute')} from the preview.`,
    })
  }

  const imageSources = Array.from(doc.querySelectorAll<HTMLImageElement>('img[src]'))
    .map((element) => ({
      element,
      source: element.getAttribute('src')?.trim() ?? '',
    }))
    .filter(({ source }) => source.length > 0 && !sanitizeExternalImageUrl(source) && isRelativeReference(source))

  injectHtmlPreviewStyles(doc)

  return { diagnostics, doc, imageSources }
}

function injectHtmlPreviewStyles(doc: Document): void {
  const style = doc.createElement('style')
  style.setAttribute('data-xero-preview', 'true')
  style.textContent = [
    ':root { color-scheme: light dark; }',
    'html, body { min-height: 100%; }',
    'body { margin: 0; padding: 24px; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; line-height: 1.5; }',
    'img, video, canvas, svg { max-width: 100%; height: auto; }',
    '[data-xero-preview-missing-src] { display: inline-block; min-width: 120px; min-height: 36px; border: 1px dashed #94a3b8; background: #f8fafc; color: #475569; }',
  ].join('\n')
  doc.head.prepend(style)
}

function serializeHtmlPreviewDocument(doc: Document | null, fallbackText: string): string {
  if (!doc?.documentElement) {
    return `<!doctype html><html><body><pre>${escapeHtml(fallbackText)}</pre></body></html>`
  }
  return `<!doctype html>${doc.documentElement.outerHTML}`
}

function normalizeAssetPreviewResolution(
  path: string,
  result: AssetPreviewResolution | string | null,
): AssetPreviewResolution {
  if (typeof result === 'string') {
    return {
      path,
      url: result.trim().length > 0 ? result : null,
      reason: result.trim().length > 0 ? undefined : 'unavailable',
    }
  }

  if (!result) {
    return {
      path,
      reason: 'unavailable',
      url: null,
    }
  }

  return {
    ...result,
    path: result.path || path,
  }
}

function assetPreviewIssueLabel(issue: AssetPreviewResolution): string {
  if (issue.message) return issue.message
  if (issue.reason === 'oversized') return 'too large for preview'
  if (issue.reason === 'unsupportedType') {
    return issue.mimeType ? `unsupported type ${issue.mimeType}` : 'unsupported type'
  }
  if (issue.reason === 'blocked') return 'outside the project or blocked by preview safety'
  if (issue.reason === 'missing') return 'file not found'
  return 'not previewable'
}

interface ParsedDelimitedText {
  maxColumns: number
  rows: string[][]
  totalRows: number
  truncatedColumns: boolean
  truncatedRows: boolean
}

const EMPTY_PARSED_DELIMITED_TEXT: ParsedDelimitedText = {
  maxColumns: 0,
  rows: [],
  totalRows: 0,
  truncatedColumns: false,
  truncatedRows: false,
}

function csvPreviewTable(preview: ProjectFileCsvPreviewDto): ParsedDelimitedText {
  return {
    maxColumns: preview.totalColumns,
    rows: preview.totalRows === 0 ? [] : [preview.headers, ...preview.rows],
    totalRows: preview.totalRows,
    truncatedColumns: preview.truncatedColumns,
    truncatedRows: preview.truncatedRows,
  }
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
  markdownAssetsBySource: Map<string, ProjectFileMarkdownPreviewDto['assetRefs'][number]> | null,
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
    blockquote({ children, node }) {
      return (
        <blockquote
          className="my-3 border-l-2 border-primary/40 pl-3 text-muted-foreground"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
          {children}
        </blockquote>
      )
    },
    code({ children, className, node }) {
      const code = String(children).replace(/\n$/, '')
      const language = /language-([^\s]+)/.exec(className ?? '')?.[1] ?? null

      if (language) {
        return <CodePreviewBlock code={code} lang={language} sourceLine={sourceLineFromMarkdownNode(node)} />
      }

      return (
        <code className="rounded bg-muted/70 px-1 py-px font-mono text-[0.88em] text-foreground">
          {children}
        </code>
      )
    },
    h1({ children, node }) {
      return (
        <h1
          className="mb-4 mt-0 text-[24px] font-semibold leading-tight"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
          {children}
        </h1>
      )
    },
    h2({ children, node }) {
      return (
        <h2
          className="mb-3 mt-6 border-b border-border pb-1 text-[19px] font-semibold"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
          {children}
        </h2>
      )
    },
    h3({ children, node }) {
      return (
        <h3
          className="mb-2 mt-5 text-[16px] font-semibold"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
          {children}
        </h3>
      )
    },
    h4({ children, node }) {
      return (
        <h4
          className="mb-2 mt-4 text-[14px] font-semibold"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
          {children}
        </h4>
      )
    },
    hr() {
      return <Separator className="my-5" />
    },
    img({ alt, src }) {
      const assetRef = markdownAssetsBySource?.get(src ?? '')
      return (
        <MarkdownImage
          alt={alt ?? ''}
          filePath={filePath}
          hasRustAssetRef={assetRef !== undefined}
          rustAssetPreviewUrl={assetRef?.previewUrl ?? null}
          src={src ?? ''}
          onResolveAssetPreviewUrl={onResolveAssetPreviewUrl}
        />
      )
    },
    li({ children, node }) {
      return (
        <li className="my-0.5 pl-1" data-source-line={sourceLineFromMarkdownNode(node)}>
          {children}
        </li>
      )
    },
    ol({ children, node }) {
      return (
        <ol className="my-3 list-decimal space-y-1 pl-6" data-source-line={sourceLineFromMarkdownNode(node)}>
          {children}
        </ol>
      )
    },
    p({ children, node }) {
      if (markdownNodeContainsTag(node, 'img')) {
        return (
          <div className="my-3 whitespace-pre-wrap break-words" data-source-line={sourceLineFromMarkdownNode(node)}>
            {children}
          </div>
        )
      }

      return (
        <p className="my-3 whitespace-pre-wrap break-words" data-source-line={sourceLineFromMarkdownNode(node)}>
          {children}
        </p>
      )
    },
    pre({ children }) {
      return <>{children}</>
    },
    table({ children, node }) {
      return (
        <div
          className="my-4 overflow-x-auto rounded-md border border-border"
          data-source-line={sourceLineFromMarkdownNode(node)}
        >
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
    ul({ children, node }) {
      return (
        <ul className="my-3 list-disc space-y-1 pl-6" data-source-line={sourceLineFromMarkdownNode(node)}>
          {children}
        </ul>
      )
    },
  }
}

function markdownNodeContainsTag(node: unknown, tagName: string): boolean {
  if (!node || typeof node !== 'object') {
    return false
  }

  const candidate = node as { children?: unknown; tagName?: unknown }
  if (candidate.tagName === tagName) {
    return true
  }

  if (!Array.isArray(candidate.children)) {
    return false
  }

  return candidate.children.some((child) => markdownNodeContainsTag(child, tagName))
}

function MarkdownImage({
  alt,
  filePath,
  hasRustAssetRef,
  rustAssetPreviewUrl,
  src,
  onResolveAssetPreviewUrl,
}: {
  alt: string
  filePath: string
  hasRustAssetRef?: boolean
  rustAssetPreviewUrl?: string | null
  src: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  const [state, setState] = useState<{
    status: 'loading' | 'loaded' | 'error'
    issue: AssetPreviewResolution | null
    url: string | null
  }>({ status: 'loading', issue: null, url: null })
  const externalUrl = useMemo(() => sanitizeExternalImageUrl(src), [src])
  const projectPath = useMemo(
    () => (externalUrl ? null : normalizeProjectReference(filePath, src)),
    [externalUrl, filePath, src],
  )
  const altText = alt.trim() || basenameFromReference(src) || 'Markdown image'

  useEffect(() => {
    if (externalUrl) {
      setState({ status: 'loaded', issue: null, url: externalUrl })
      return
    }

    if (rustAssetPreviewUrl) {
      setState({ status: 'loaded', issue: null, url: rustAssetPreviewUrl })
      return
    }

    if (hasRustAssetRef) {
      setState({
        status: 'error',
        issue: {
          path: projectPath ?? src,
          reason: 'unavailable',
          url: null,
        },
        url: null,
      })
      return
    }

    if (!projectPath || !onResolveAssetPreviewUrl) {
      setState({
        status: 'error',
        issue: {
          path: projectPath ?? src,
          reason: 'blocked',
          url: null,
        },
        url: null,
      })
      return
    }

    let cancelled = false
    setState({ status: 'loading', issue: null, url: null })
    onResolveAssetPreviewUrl(projectPath)
      .then((result) => {
        if (cancelled) return
        const resolution = normalizeAssetPreviewResolution(projectPath, result)
        setState(
          resolution.url
            ? { status: 'loaded', issue: null, url: resolution.url }
            : { status: 'error', issue: resolution, url: null },
        )
      })
      .catch((error) => {
        if (cancelled) return
        setState({
          status: 'error',
          issue: {
            path: projectPath,
            reason: 'missing',
            url: null,
            message: error instanceof Error ? error.message : String(error),
          },
          url: null,
        })
      })

    return () => {
      cancelled = true
    }
  }, [externalUrl, hasRustAssetRef, onResolveAssetPreviewUrl, projectPath, rustAssetPreviewUrl, src])

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
            {state.issue?.reason ? (
              <span className="text-muted-foreground/70">
                ({assetPreviewIssueLabel(state.issue)})
              </span>
            ) : null}
          </div>
        ) : null}
        {state.status === 'loaded' && state.url ? (
          <img
            src={state.url}
            alt={altText}
            referrerPolicy="no-referrer"
            className="max-h-[520px] max-w-full object-contain"
            onError={() =>
              setState({
                status: 'error',
                issue: {
                  path: state.issue?.path ?? projectPath ?? src,
                  reason: 'unavailable',
                  url: null,
                },
                url: null,
              })
            }
          />
        ) : null}
      </div>
      {alt.trim().length > 0 ? (
        <figcaption className="mt-1 text-center text-[11px] text-muted-foreground">{alt}</figcaption>
      ) : null}
    </figure>
  )
}

function CodePreviewBlock({
  code,
  lang,
  sourceLine,
}: {
  code: string
  lang: string
  sourceLine?: number
}) {
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
    <div
      className="group my-3 overflow-hidden rounded-md border border-border bg-card/60"
      data-source-line={sourceLine}
    >
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

const HTML_URL_ATTRIBUTES = new Set([
  'action',
  'formaction',
  'href',
  'poster',
  'src',
  'xlink:href',
])

function sourceLineFromMarkdownNode(node: unknown): number | undefined {
  const line = (node as { position?: { start?: { line?: unknown } } } | null)?.position?.start?.line
  return typeof line === 'number' && Number.isFinite(line) && line > 0 ? line : undefined
}

function extractMarkdownImageReferences(text: string): string[] {
  const references: string[] = []
  const pattern = /!\[[^\]]*]\(\s*(?:"([^"]+)"|'([^']+)'|([^)\s]+))(?:\s+["'][^"']*["'])?\s*\)/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(text)) !== null) {
    const reference = (match[1] ?? match[2] ?? match[3] ?? '').trim()
    if (reference.length > 0) {
      references.push(reference)
    }
  }
  return references
}

function containsMarkdownHtml(text: string): boolean {
  return /<\/?[a-z][\w:-]*(?:\s|>|\/>)/i.test(text)
}

function containsUnsafeCssUrl(value: string): boolean {
  return /expression\s*\(/i.test(value) || /url\s*\(\s*['"]?\s*(?:javascript|vbscript):/i.test(value)
}

function isSafeHtmlAttributeUrl(value: string, attributeName: string, tagName: string): boolean {
  const trimmed = value.trim()
  if (!trimmed || trimmed.startsWith('#')) {
    return true
  }

  const normalizedTagName = tagName.toLowerCase()
  if (isRelativeReference(trimmed) && !trimmed.startsWith('//')) {
    return true
  }

  if (/^data:/i.test(trimmed)) {
    return (
      normalizedTagName === 'img' &&
      attributeName === 'src' &&
      /^data:image\/(?:avif|gif|jpe?g|png|webp);/i.test(trimmed)
    )
  }

  return /^(?:https?:|mailto:|tel:|blob:|xero-asset:)/i.test(trimmed)
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
  if (!trimmed || !isRelativeReference(trimmed) || trimmed.startsWith('#') || trimmed.startsWith('//')) {
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

function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
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
