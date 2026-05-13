'use client'

import { lazy, Suspense } from 'react'
import type { EditorView as CodeMirrorView } from '@codemirror/view'
import type { ProjectDiagnosticDto } from '@/src/lib/xero-model'
import type { ProjectFileResource } from './use-execution-workspace-controller'
import type { EditorRenderPreferences } from '../code-editor'
import type { EditorSelectionContext } from './agent-aware-editor-hooks'
import type { EditorGitDiffLineMarker } from './git-aware-editing'
import { LoadingState } from './editor-empty-state'
import {
  CsvPreview,
  HtmlPreview,
  ImagePreview,
  MarkdownPreview,
  MediaPreview,
  PdfPreview,
  PreviewUnavailablePanel,
  SvgTextPreview,
  UnsupportedFilePanel,
  type ImageControlsState,
  type ImageDimensions,
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
  diagnostics?: ProjectDiagnosticDto[]
  gitDiffMarkers?: EditorGitDiffLineMarker[]
  onDocumentStatsChange?: (stats: { lineCount: number }) => void
  onSave?: (snapshot: string) => void
  onCursorChange?: (position: { line: number; column: number }) => void
  onSelectionChange?: (selection: EditorSelectionContext | null) => void
  onOpenFind?: (options: { withReplace: boolean; initialQuery: string }) => void
  onGitDiffLineClick?: (marker: EditorGitDiffLineMarker) => void
  onViewReady?: (view: CodeMirrorView | null) => void
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
  sourceLine?: number
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
  preferences?: EditorRenderPreferences
  // Mode toggle (per-tab state owned by parent — toggle UI lives in EditorTopBar)
  mode: FileEditorMode
  // Image controls (lifted to parent; rendered in EditorTopBar)
  imageControls?: ImageControlsState
  onImageDimensionsChange?: (dimensions: ImageDimensions | null) => void
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
        imageControls={props.imageControls}
        onImageDimensionsChange={props.onImageDimensionsChange}
      />
    )
  }

  const supportsToggle = resourceSupportsPreviewToggle(resource)
  const effectiveMode: FileEditorMode = supportsToggle ? mode : 'source'

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {effectiveMode === 'source' ? (
        <Suspense fallback={<LoadingState path={filePath} />}>
          <LazyCodeEditor
            key={filePath}
            value={props.textValue}
            savedValue={props.textSavedValue}
            documentVersion={props.textDocumentVersion}
            filePath={filePath}
            preferences={props.preferences}
            onSnapshotChange={props.onSnapshotChange}
            onDirtyChange={props.onDirtyChange}
            diagnostics={props.diagnostics}
            gitDiffMarkers={props.gitDiffMarkers}
            onDocumentStatsChange={props.onDocumentStatsChange}
            onSave={props.onSave}
            onCursorChange={props.onCursorChange}
            onSelectionChange={props.onSelectionChange}
            onOpenFind={props.onOpenFind}
            onGitDiffLineClick={props.onGitDiffLineClick}
            onViewReady={props.onViewReady}
          />
        </Suspense>
      ) : (
        <TextBackedPreview
          filePath={filePath}
          rendererKind={resource.rendererKind}
          mimeType={resource.mimeType}
          text={props.textValue}
          preview={props.textValue === props.textSavedValue ? resource.preview : null}
          savedPreview={resource.preview ?? null}
          onResolveAssetPreviewUrl={props.onResolveAssetPreviewUrl}
          sourceLine={props.sourceLine}
          imageControls={props.imageControls}
          onImageDimensionsChange={props.onImageDimensionsChange}
        />
      )}
    </div>
  )
}

function TextBackedPreview({
  filePath,
  rendererKind,
  mimeType,
  text,
  preview,
  savedPreview,
  sourceLine,
  onResolveAssetPreviewUrl,
  imageControls,
  onImageDimensionsChange,
}: {
  filePath: string
  rendererKind: TextRendererKind
  mimeType: string
  text: string
  preview?: Extract<ProjectFileResource, { kind: 'text' }>['preview']
  savedPreview?: Extract<ProjectFileResource, { kind: 'text' }>['preview']
  sourceLine?: number
  onResolveAssetPreviewUrl?: ResolveAssetPreviewUrl
  imageControls?: ImageControlsState
  onImageDimensionsChange?: (dimensions: ImageDimensions | null) => void
}) {
  if (rendererKind === 'svg') {
    return (
      <SvgTextPreview
        filePath={filePath}
        text={text}
        mimeType={mimeType}
        controls={imageControls}
        onDimensionsChange={onImageDimensionsChange}
      />
    )
  }

  if (rendererKind === 'markdown') {
    return (
      <MarkdownPreview
        filePath={filePath}
        text={text}
        preview={
          preview?.kind === 'markdown'
            ? preview
            : savedPreview?.kind === 'markdown'
              ? savedPreview
              : null
        }
        sourceLine={sourceLine}
        onResolveAssetPreviewUrl={onResolveAssetPreviewUrl}
      />
    )
  }

  if (rendererKind === 'csv') {
    return (
      <CsvPreview
        filePath={filePath}
        text={text}
        mimeType={mimeType}
        preview={preview?.kind === 'csv' ? preview : null}
      />
    )
  }

  if (rendererKind === 'html') {
    return (
      <HtmlPreview
        filePath={filePath}
        text={text}
        onResolveAssetPreviewUrl={onResolveAssetPreviewUrl}
      />
    )
  }

  return <PreviewUnavailablePanel rendererKind={rendererKind} filePath={filePath} />
}

function RenderablePreview({
  filePath,
  onCopyPath,
  onOpenExternal,
  resource,
  imageControls,
  onImageDimensionsChange,
}: {
  filePath: string
  onCopyPath?: (path: string) => void
  onOpenExternal?: (path: string) => void
  resource: Extract<ProjectFileResource, { kind: 'renderable' }>
  imageControls?: ImageControlsState
  onImageDimensionsChange?: (dimensions: ImageDimensions | null) => void
}) {
  const { previewUrl, rendererKind } = resource

  if (rendererKind === 'image') {
    return (
      <ImagePreview
        filePath={filePath}
        src={previewUrl}
        testId="image-preview"
        controls={imageControls}
        onDimensionsChange={onImageDimensionsChange}
      />
    )
  }

  if (rendererKind === 'audio') {
    return <MediaPreview filePath={filePath} src={previewUrl} rendererKind="audio" />
  }

  if (rendererKind === 'video') {
    return <MediaPreview filePath={filePath} src={previewUrl} rendererKind="video" />
  }

  // pdf
  return (
    <PdfPreview
      filePath={filePath}
      src={previewUrl}
      onCopyPath={onCopyPath}
      onOpenExternal={onOpenExternal}
    />
  )
}
