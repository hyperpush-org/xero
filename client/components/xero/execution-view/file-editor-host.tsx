'use client'

import { lazy, Suspense } from 'react'
import type { EditorView as CodeMirrorView } from '@codemirror/view'
import { Code2, Eye } from 'lucide-react'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { ProjectFileResource } from './use-execution-workspace-controller'
import { LoadingState } from './editor-empty-state'
import {
  CsvPreview,
  ImagePreview,
  MarkdownPreview,
  MediaPreview,
  PdfPreview,
  PreviewUnavailablePanel,
  SvgTextPreview,
  UnsupportedFilePanel,
  type ResolveAssetPreviewUrl,
} from './file-renderers'

const LazyCodeEditor = lazy(() => import('../code-editor').then((module) => ({ default: module.CodeEditor })))

export type FileEditorMode = 'source' | 'preview'
type TextRendererKind = Extract<ProjectFileResource, { kind: 'text' }>['rendererKind']

const TEXT_MODES_WITH_PREVIEW = new Set<TextRendererKind>([
  'svg',
  'markdown',
  'csv',
  'html',
])

export function resourceSupportsPreviewToggle(resource: ProjectFileResource | null): boolean {
  return resource?.kind === 'text' && TEXT_MODES_WITH_PREVIEW.has(resource.rendererKind)
}

export function defaultModeForResource(resource: ProjectFileResource | null): FileEditorMode {
  if (!resource || resource.kind !== 'text') {
    return 'preview'
  }

  // Markdown, CSV, HTML default to source per the product rules.
  // SVG defaults to rendered preview.
  return resource.rendererKind === 'svg' ? 'preview' : 'source'
}

interface FileEditorHostProps {
  filePath: string
  resource: ProjectFileResource
  // Text-mode props (only used when resource.kind === 'text')
  textValue: string
  textSavedValue: string
  textDocumentVersion: number
  onSnapshotChange?: (value: string) => void
  onDirtyChange?: (dirty: boolean) => void
  onDocumentStatsChange?: (stats: { lineCount: number }) => void
  onSave?: (snapshot: string) => void
  onCursorChange?: (position: { line: number; column: number }) => void
  onOpenFind?: (options: { withReplace: boolean; initialQuery: string }) => void
  onViewReady?: (view: CodeMirrorView | null) => void
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
  // Mode toggle (per-tab state owned by parent)
  mode: FileEditorMode
  onModeChange: (mode: FileEditorMode) => void
}

export function FileEditorHost(props: FileEditorHostProps) {
  const { filePath, resource, mode } = props

  if (resource.kind === 'unsupported') {
    return (
      <UnsupportedFilePanel
        filePath={filePath}
        byteLength={resource.byteLength}
        contentHash={resource.contentHash}
        mimeType={resource.mimeType}
        modifiedAt={resource.modifiedAt}
        reason={resource.reason}
        rendererKind={resource.rendererKind}
        onCopyPath={props.onCopyPath}
        onOpenExternal={props.onOpenExternal}
      />
    )
  }

  if (resource.kind === 'renderable') {
    return (
      <RenderablePreview
        filePath={filePath}
        resource={resource}
        onCopyPath={props.onCopyPath}
        onOpenExternal={props.onOpenExternal}
      />
    )
  }

  const supportsToggle = resourceSupportsPreviewToggle(resource)
  const effectiveMode: FileEditorMode = supportsToggle ? mode : 'source'

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {supportsToggle ? (
        <SourcePreviewToggle mode={effectiveMode} onModeChange={props.onModeChange} />
      ) : null}
      <div className="flex min-h-0 flex-1 flex-col">
        {effectiveMode === 'source' ? (
          <Suspense fallback={<LoadingState path={filePath} />}>
            <LazyCodeEditor
              key={filePath}
              value={props.textValue}
              savedValue={props.textSavedValue}
              documentVersion={props.textDocumentVersion}
              filePath={filePath}
              onSnapshotChange={props.onSnapshotChange}
              onDirtyChange={props.onDirtyChange}
              onDocumentStatsChange={props.onDocumentStatsChange}
              onSave={props.onSave}
              onCursorChange={props.onCursorChange}
              onOpenFind={props.onOpenFind}
              onViewReady={props.onViewReady}
            />
          </Suspense>
        ) : (
          <TextBackedPreview
            filePath={filePath}
            rendererKind={resource.rendererKind}
            mimeType={resource.mimeType}
            text={props.textValue}
            onResolveAssetPreviewUrl={props.onResolveAssetPreviewUrl}
          />
        )}
      </div>
    </div>
  )
}

function SourcePreviewToggle({
  mode,
  onModeChange,
}: {
  mode: FileEditorMode
  onModeChange: (mode: FileEditorMode) => void
}) {
  return (
    <div
      className="flex shrink-0 items-center justify-between border-b border-border bg-secondary/10 px-3 py-1"
      data-testid="file-editor-host-toolbar"
      role="toolbar"
      aria-label="Source and preview controls"
    >
      <ToggleGroup
        type="single"
        size="sm"
        value={mode}
        onValueChange={(value) => {
          if (value === 'source' || value === 'preview') {
            onModeChange(value)
          }
        }}
        aria-label="Editor mode"
      >
        <ToggleGroupItem value="source" aria-label="Show source">
          <Code2 className="mr-1 h-3 w-3" />
          Source
        </ToggleGroupItem>
        <ToggleGroupItem value="preview" aria-label="Show preview">
          <Eye className="mr-1 h-3 w-3" />
          Preview
        </ToggleGroupItem>
      </ToggleGroup>
    </div>
  )
}

function TextBackedPreview({
  filePath,
  rendererKind,
  mimeType,
  text,
  onResolveAssetPreviewUrl,
}: {
  filePath: string
  rendererKind: TextRendererKind
  mimeType: string
  text: string
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
}) {
  if (rendererKind === 'svg') {
    return <SvgTextPreview filePath={filePath} text={text} mimeType={mimeType} />
  }

  if (rendererKind === 'markdown') {
    return (
      <MarkdownPreview
        filePath={filePath}
        text={text}
        onResolveAssetPreviewUrl={onResolveAssetPreviewUrl}
      />
    )
  }

  if (rendererKind === 'csv') {
    return <CsvPreview filePath={filePath} text={text} mimeType={mimeType} />
  }

  return <PreviewUnavailablePanel rendererKind={rendererKind} filePath={filePath} />
}

function RenderablePreview({
  filePath,
  onCopyPath,
  onOpenExternal,
  resource,
}: {
  filePath: string
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
  resource: Extract<ProjectFileResource, { kind: 'renderable' }>
}) {
  const { previewUrl, rendererKind, mimeType } = resource

  if (rendererKind === 'image') {
    return (
      <ImagePreview
        filePath={filePath}
        src={previewUrl}
        byteLength={resource.byteLength}
        mimeType={mimeType}
        testId="image-preview"
      />
    )
  }

  if (rendererKind === 'audio') {
    return (
      <MediaPreview
        filePath={filePath}
        src={previewUrl}
        byteLength={resource.byteLength}
        mimeType={mimeType}
        rendererKind="audio"
        onCopyPath={onCopyPath}
        onOpenExternal={onOpenExternal}
      />
    )
  }

  if (rendererKind === 'video') {
    return (
      <MediaPreview
        filePath={filePath}
        src={previewUrl}
        byteLength={resource.byteLength}
        mimeType={mimeType}
        rendererKind="video"
        onCopyPath={onCopyPath}
        onOpenExternal={onOpenExternal}
      />
    )
  }

  // pdf
  return (
    <PdfPreview
      filePath={filePath}
      src={previewUrl}
      byteLength={resource.byteLength}
      mimeType={mimeType}
      onCopyPath={onCopyPath}
      onOpenExternal={onOpenExternal}
    />
  )
}
