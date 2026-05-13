"use client"

import { useEffect, useMemo, useRef } from 'react'
import { EditorView, basicSetup } from 'codemirror'
import { Annotation, Compartment, EditorState, Prec, type Extension } from '@codemirror/state'
import {
  HighlightStyle,
  StreamLanguage,
  indentUnit,
  syntaxHighlighting,
} from '@codemirror/language'
import type { StreamParser } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { indentWithTab } from '@codemirror/commands'
import { highlightSelectionMatches, search } from '@codemirror/search'
import { Decoration, GutterMarker, gutter, keymap } from '@codemirror/view'
import { autocompletion } from '@codemirror/autocomplete'
import { cn } from '@/lib/utils'
import { getLangFromPath } from '@/lib/language-detection'
import { useTheme } from '@/src/features/theme/theme-provider'
import type {
  EditorPalette,
  ThemeDefinition,
} from '@/src/features/theme/theme-definitions'
import type { ProjectDiagnosticDto } from '@/src/lib/xero-model'
import type { EditorSelectionContext } from './execution-view/agent-aware-editor-hooks'
import type { EditorGitDiffLineMarker } from './execution-view/git-aware-editing'

export const EDITOR_SNAPSHOT_DEBOUNCE_MS = 250

interface ScheduledFrame {
  id: number
  type: 'animation-frame' | 'timeout'
}

interface EditorFrameSchedulerOptions {
  requestFrame?: (callback: () => void) => ScheduledFrame
  cancelFrame?: (frame: ScheduledFrame) => void
}

export interface EditorCursorPosition {
  line: number
  column: number
}

export interface EditorDocumentStats {
  lineCount: number
}

export interface EditorRenderPreferences {
  fontSize: number
  tabSize: number
  insertSpaces: boolean
  lineWrapping: boolean
}

export const DEFAULT_EDITOR_RENDER_PREFERENCES: EditorRenderPreferences = {
  fontSize: 13,
  tabSize: 2,
  insertSpaces: true,
  lineWrapping: true,
}

export interface CodeEditorProps {
  value: string
  savedValue?: string
  documentVersion?: number
  onSnapshotChange?: (value: string) => void
  onDirtyChange?: (dirty: boolean) => void
  diagnostics?: ProjectDiagnosticDto[]
  gitDiffMarkers?: EditorGitDiffLineMarker[]
  filePath: string
  readOnly?: boolean
  preferences?: EditorRenderPreferences
  onSave?: (snapshot: string) => void
  onCursorChange?: (position: EditorCursorPosition) => void
  onSelectionChange?: (selection: EditorSelectionContext | null) => void
  onDocumentStatsChange?: (stats: EditorDocumentStats) => void
  onOpenFind?: (options: { withReplace: boolean; initialQuery: string }) => void
  onGitDiffLineClick?: (marker: EditorGitDiffLineMarker) => void
  onViewReady?: (view: EditorView | null) => void
  className?: string
}

const externalDocumentSync = Annotation.define<boolean>()

export function countEditorLines(value: string): number {
  return value.length === 0 ? 1 : value.split('\n').length
}

export function shouldReplaceEditorDocument(options: {
  externalValue: string
  lastSnapshot: string
  documentVersionChanged: boolean
}): boolean {
  return options.documentVersionChanged || options.externalValue !== options.lastSnapshot
}

function requestEditorFrame(callback: () => void): ScheduledFrame {
  if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
    return {
      id: window.requestAnimationFrame(callback),
      type: 'animation-frame',
    }
  }

  return {
    id: window.setTimeout(callback, 16),
    type: 'timeout',
  }
}

function cancelEditorFrame(frame: ScheduledFrame): void {
  if (
    frame.type === 'animation-frame' &&
    typeof window !== 'undefined' &&
    typeof window.cancelAnimationFrame === 'function'
  ) {
    window.cancelAnimationFrame(frame.id)
    return
  }

  window.clearTimeout(frame.id)
}

export function createEditorFrameScheduler(options: EditorFrameSchedulerOptions = {}) {
  const requestFrame = options.requestFrame ?? requestEditorFrame
  const cancelFrame = options.cancelFrame ?? cancelEditorFrame
  let pendingFrame: ScheduledFrame | null = null

  return {
    schedule(callback: () => void): void {
      if (pendingFrame) return
      pendingFrame = requestFrame(() => {
        pendingFrame = null
        callback()
      })
    },
    cancel(): void {
      if (!pendingFrame) return
      cancelFrame(pendingFrame)
      pendingFrame = null
    },
    isPending(): boolean {
      return pendingFrame !== null
    },
  }
}

export function getEditorSelectionContext(state: EditorState): EditorSelectionContext | null {
  const selection = state.selection.main
  if (selection.from === selection.to) {
    return null
  }

  const fromLine = state.doc.lineAt(selection.from)
  const toLine = state.doc.lineAt(selection.to)
  return {
    text: state.sliceDoc(selection.from, selection.to),
    fromLine: fromLine.number,
    fromColumn: selection.from - fromLine.from + 1,
    toLine: toLine.number,
    toColumn: selection.to - toLine.from + 1,
  }
}

// ---------------------------------------------------------------------------
// Theme — palette is driven by the active `ThemeDefinition` from
// `features/theme/theme-provider`. All token colors and editor chrome live
// in the theme registry; this module only translates that palette into
// CodeMirror extensions.
// ---------------------------------------------------------------------------

function buildSyntaxHighlight(p: EditorPalette): Extension {
  return syntaxHighlighting(
    HighlightStyle.define([
      { tag: [t.keyword, t.moduleKeyword, t.definitionKeyword], color: p.keyword },
      { tag: t.controlKeyword, color: p.control, fontStyle: 'italic' },
      { tag: t.modifier, color: p.storage },

      { tag: [t.name, t.character, t.macroName], color: p.variable },
      { tag: [t.definition(t.variableName), t.separator], color: p.variableDef },
      { tag: [t.propertyName], color: p.property },
      {
        tag: [t.function(t.variableName), t.function(t.propertyName), t.labelName],
        color: p.function,
      },

      { tag: [t.typeName, t.className, t.namespace], color: p.type, fontStyle: 'italic' },

      { tag: [t.number], color: p.number },
      { tag: [t.bool, t.null, t.atom, t.self, t.special(t.variableName)], color: p.bool },
      { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: p.constant },

      { tag: [t.string], color: p.string },
      { tag: [t.special(t.string), t.regexp, t.escape], color: p.stringSpecial },
      { tag: [t.processingInstruction, t.inserted], color: p.string },

      { tag: [t.operator, t.operatorKeyword], color: p.operator },
      {
        tag: [t.punctuation, t.bracket, t.brace, t.paren, t.derefOperator, t.squareBracket],
        color: p.punctuation,
      },
      { tag: t.angleBracket, color: p.tagBracket },

      { tag: [t.tagName], color: p.tagName },
      { tag: [t.attributeName], color: p.attribute },
      { tag: [t.attributeValue], color: p.string },

      { tag: t.heading, color: p.heading, fontWeight: '600' },
      { tag: [t.heading1, t.heading2], color: p.heading, fontWeight: '700' },
      { tag: t.link, color: p.link, textDecoration: 'underline' },
      { tag: t.url, color: p.link, textDecoration: 'underline' },
      { tag: t.quote, color: p.property, fontStyle: 'italic' },
      { tag: t.monospace, color: p.stringSpecial },
      { tag: t.strong, color: p.control, fontWeight: '700' },
      { tag: t.emphasis, color: p.string, fontStyle: 'italic' },
      { tag: t.strikethrough, textDecoration: 'line-through' },
      { tag: t.list, color: p.control },

      {
        tag: [t.comment, t.lineComment, t.blockComment, t.docComment],
        color: p.comment,
        fontStyle: 'italic',
      },
      { tag: [t.meta, t.annotation], color: p.meta },
      { tag: t.changed, color: p.number },
      { tag: t.invalid, color: p.invalid, textDecoration: 'underline wavy' },
    ]),
  )
}

function buildEditorChrome(p: EditorPalette, dark: boolean): Extension {
  return EditorView.theme(
    {
      '&': {
        color: p.foreground,
        backgroundColor: p.background,
        height: '100%',
        fontSize: '13px',
      },
      '&.cm-editor.cm-focused': { outline: 'none' },
      '.cm-scroller': {
        overflow: 'auto',
        fontFamily:
          "ui-monospace, 'SF Mono', Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
        lineHeight: '1.6',
      },
      '.cm-content': {
        minHeight: '100%',
        caretColor: p.cursor,
        padding: '6px 0',
      },
      '.cm-gutters': {
        backgroundColor: p.background,
        borderRight: `1px solid ${p.border}`,
        color: p.gutter,
      },
      '.cm-lineNumbers .cm-gutterElement': {
        padding: '0 14px 0 10px',
        minWidth: '28px',
        fontVariantNumeric: 'tabular-nums',
      },
      '.cm-activeLineGutter': {
        backgroundColor: 'transparent',
        color: p.gutterActive,
      },
      '.cm-activeLine': { backgroundColor: p.lineActive },
      '.cm-cursor, .cm-dropCursor': { borderLeftColor: p.cursor, borderLeftWidth: '2px' },
      '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-content ::selection':
        { backgroundColor: `${p.selection} !important` },
      '.cm-selectionMatch': { backgroundColor: p.selectionMatch },
      '.cm-matchingBracket, .cm-nonmatchingBracket': {
        backgroundColor: p.matchingBracketBg,
        color: p.foreground,
        outline: 'none',
      },
      '.cm-searchMatch': {
        backgroundColor: p.searchMatchBg,
        outline: `1px solid ${p.searchMatchBorder}`,
      },
      '.cm-searchMatch.cm-searchMatch-selected': {
        backgroundColor: p.searchMatchSelectedBg,
      },
      '.cm-panels': { backgroundColor: p.panelBackground, color: p.foreground },
      '.cm-panels.cm-panels-bottom': { borderTop: `1px solid ${p.border}` },
      '.cm-tooltip': {
        backgroundColor: p.panelBackground,
        borderColor: p.border,
        color: p.foreground,
      },
      '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
        backgroundColor: p.autocompleteSelectedBg,
        color: p.foreground,
      },
      '.cm-foldGutter .cm-gutterElement': {
        color: p.foldGutter,
      },
      '.cm-foldPlaceholder': {
        backgroundColor: 'transparent',
        border: `1px solid ${p.border}`,
        color: p.foldPlaceholderText,
        padding: '0 4px',
      },
    },
    { dark },
  )
}

function buildThemeExtension(theme: ThemeDefinition): Extension {
  return [
    buildSyntaxHighlight(theme.editor),
    buildEditorChrome(theme.editor, theme.appearance === 'dark'),
  ]
}

function buildPreferenceExtensions(preferences: EditorRenderPreferences): Extension {
  const indentValue = preferences.insertSpaces
    ? ' '.repeat(Math.max(1, preferences.tabSize))
    : '\t'
  const extensions: Extension[] = [
    EditorState.tabSize.of(preferences.tabSize),
    indentUnit.of(indentValue),
  ]
  if (preferences.lineWrapping) {
    extensions.push(EditorView.lineWrapping)
  }
  return extensions
}

function buildFontSizeExtension(fontSize: number): Extension {
  return EditorView.theme({
    '&': { fontSize: `${fontSize}px` },
  })
}

function buildDiagnosticsExtension(diagnostics: readonly ProjectDiagnosticDto[] = []): Extension {
  const lineDiagnostics = diagnostics.filter((diagnostic) => diagnostic.line)
  return [
    EditorView.theme({
      '.xero-editor-diagnostic-error': {
        textDecorationLine: 'underline',
        textDecorationStyle: 'wavy',
        textDecorationColor: 'hsl(var(--destructive))',
        textUnderlineOffset: '3px',
      },
      '.xero-editor-diagnostic-warning': {
        textDecorationLine: 'underline',
        textDecorationStyle: 'wavy',
        textDecorationColor: 'hsl(var(--warning))',
        textUnderlineOffset: '3px',
      },
      '.cm-line.xero-editor-line-diagnostic-error': {
        backgroundColor: 'hsl(var(--destructive) / 0.07)',
      },
      '.cm-line.xero-editor-line-diagnostic-warning': {
        backgroundColor: 'hsl(var(--warning) / 0.08)',
      },
    }),
    EditorView.decorations.compute(['doc'], (state) => {
      const ranges = []
      for (const diagnostic of lineDiagnostics) {
        const lineNumber = diagnostic.line ?? 0
        if (lineNumber < 1 || lineNumber > state.doc.lines) continue
        const line = state.doc.line(lineNumber)
        const column = Math.max(1, diagnostic.column ?? 1)
        const from = Math.min(line.to, line.from + column - 1)
        const to = Math.max(from + 1, Math.min(line.to, from + 1))
        const severityClass =
          diagnostic.severity === 'warning'
            ? 'xero-editor-diagnostic-warning'
            : 'xero-editor-diagnostic-error'
        ranges.push(Decoration.mark({ class: severityClass }).range(from, to))
        ranges.push(
          Decoration.line({
            class:
              diagnostic.severity === 'warning'
                ? 'xero-editor-line-diagnostic-warning'
                : 'xero-editor-line-diagnostic-error',
          }).range(line.from),
        )
      }
      return Decoration.set(ranges, true)
    }),
  ]
}

class GitDiffGutterMarker extends GutterMarker {
  constructor(
    private readonly marker: EditorGitDiffLineMarker,
    private readonly onClick?: (marker: EditorGitDiffLineMarker) => void,
  ) {
    super()
  }

  override eq(other: GutterMarker): boolean {
    return (
      other instanceof GitDiffGutterMarker &&
      other.marker.line === this.marker.line &&
      other.marker.kind === this.marker.kind &&
      other.marker.hunkIndex === this.marker.hunkIndex
    )
  }

  override toDOM(): HTMLElement {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = `xero-editor-git-marker xero-editor-git-marker-${this.marker.kind}`
    button.title = gitDiffMarkerLabel(this.marker.kind)
    button.setAttribute('aria-label', gitDiffMarkerLabel(this.marker.kind))
    button.addEventListener('mousedown', (event) => {
      event.preventDefault()
      event.stopPropagation()
      this.onClick?.(this.marker)
    })
    return button
  }
}

function gitDiffMarkerLabel(kind: EditorGitDiffLineMarker['kind']): string {
  switch (kind) {
    case 'added':
      return 'Git added line'
    case 'changed':
      return 'Git changed line'
    case 'deleted':
      return 'Git deleted line'
  }
}

function buildGitDiffExtension(
  markers: readonly EditorGitDiffLineMarker[] = [],
  onClick?: (marker: EditorGitDiffLineMarker) => void,
): Extension {
  const markersByLine = new Map(markers.map((marker) => [marker.line, marker]))
  return [
    EditorView.theme({
      '.xero-editor-git-gutter': {
        width: '5px',
      },
      '.xero-editor-git-marker': {
        display: 'block',
        width: '3px',
        minWidth: '3px',
        height: '100%',
        minHeight: '14px',
        margin: '0 auto',
        border: '0',
        borderRadius: '2px',
        padding: '0',
        cursor: 'pointer',
        backgroundColor: 'transparent',
      },
      '.xero-editor-git-marker-added': {
        backgroundColor: 'hsl(var(--success))',
      },
      '.xero-editor-git-marker-changed': {
        backgroundColor: 'hsl(var(--primary))',
      },
      '.xero-editor-git-marker-deleted': {
        backgroundColor: 'hsl(var(--destructive))',
      },
      '.cm-line.xero-editor-line-git-added': {
        backgroundColor: 'hsl(var(--success) / 0.06)',
      },
      '.cm-line.xero-editor-line-git-changed': {
        backgroundColor: 'hsl(var(--primary) / 0.05)',
      },
      '.cm-line.xero-editor-line-git-deleted': {
        backgroundColor: 'hsl(var(--destructive) / 0.06)',
      },
    }),
    gutter({
      class: 'xero-editor-git-gutter',
      lineMarker: (view, line) => {
        const lineNumber = view.state.doc.lineAt(line.from).number
        const marker = markersByLine.get(lineNumber)
        return marker ? new GitDiffGutterMarker(marker, onClick) : null
      },
    }),
    EditorView.decorations.compute(['doc'], (state) => {
      const ranges = []
      for (const marker of markers) {
        if (marker.line < 1 || marker.line > state.doc.lines) continue
        const line = state.doc.line(marker.line)
        ranges.push(
          Decoration.line({
            class: `xero-editor-line-git-${marker.kind}`,
          }).range(line.from),
        )
      }
      return Decoration.set(ranges, true)
    }),
  ]
}

// ---------------------------------------------------------------------------
// Language resolution
// ---------------------------------------------------------------------------

type LanguageLoader = () => Promise<Extension>

// Language packages are loaded on demand. Keeping this table as dynamic
// imports prevents the app/editor shell from pulling every grammar into the
// first interaction path.
const firstPartyLanguageLoaders: Record<string, LanguageLoader> = {
  typescript: () => import('@codemirror/lang-javascript').then((m) => m.javascript({ typescript: true })),
  tsx: () => import('@codemirror/lang-javascript').then((m) => m.javascript({ jsx: true, typescript: true })),
  jsx: () => import('@codemirror/lang-javascript').then((m) => m.javascript({ jsx: true })),
  javascript: () => import('@codemirror/lang-javascript').then((m) => m.javascript()),
  python: () => import('@codemirror/lang-python').then((m) => m.python()),
  json: () => import('@codemirror/lang-json').then((m) => m.json()),
  jsonc: () => import('@codemirror/lang-json').then((m) => m.json()),
  markdown: () => import('@codemirror/lang-markdown').then((m) => m.markdown()),
  mdx: () => import('@codemirror/lang-markdown').then((m) => m.markdown()),
  css: () => import('@codemirror/lang-css').then((m) => m.css()),
  scss: () => import('@codemirror/lang-sass').then((m) => m.sass({ indented: false })),
  sass: () => import('@codemirror/lang-sass').then((m) => m.sass({ indented: true })),
  less: () => import('@codemirror/lang-less').then((m) => m.less()),
  html: () => import('@codemirror/lang-html').then((m) => m.html()),
  xml: () => import('@codemirror/lang-xml').then((m) => m.xml()),
  rust: () => import('@codemirror/lang-rust').then((m) => m.rust()),
  c: () => import('@codemirror/lang-cpp').then((m) => m.cpp()),
  cpp: () => import('@codemirror/lang-cpp').then((m) => m.cpp()),
  java: () => import('@codemirror/lang-java').then((m) => m.java()),
  go: () => import('@codemirror/lang-go').then((m) => m.go()),
  sql: () => import('@codemirror/lang-sql').then((m) => m.sql()),
  yaml: () => import('@codemirror/lang-yaml').then((m) => m.yaml()),
  php: () => import('@codemirror/lang-php').then((m) => m.php()),
  vue: () => import('@codemirror/lang-vue').then((m) => m.vue()),
  angular: () => import('@codemirror/lang-angular').then((m) => m.angular()),
  // GraphQL has no official CM6 grammar; JS tokenization is a fair approximation.
  graphql: () => import('@codemirror/lang-javascript').then((m) => m.javascript()),
}

// Legacy-mode parsers are heavy / numerous; load lazily so the editor only
// pulls in what the active file actually needs.
const streamParserLoaders: Record<string, () => Promise<StreamParser<unknown>>> = {
  shell: () => import('@codemirror/legacy-modes/mode/shell').then((m) => m.shell),
  ruby: () => import('@codemirror/legacy-modes/mode/ruby').then((m) => m.ruby),
  toml: () => import('@codemirror/legacy-modes/mode/toml').then((m) => m.toml),
  swift: () => import('@codemirror/legacy-modes/mode/swift').then((m) => m.swift),
  kotlin: () => import('@codemirror/legacy-modes/mode/clike').then((m) => m.kotlin),
  scala: () => import('@codemirror/legacy-modes/mode/clike').then((m) => m.scala),
  csharp: () => import('@codemirror/legacy-modes/mode/clike').then((m) => m.csharp),
  dart: () => import('@codemirror/legacy-modes/mode/clike').then((m) => m.dart),
  objectivec: () => import('@codemirror/legacy-modes/mode/clike').then((m) => m.objectiveC),
  dockerfile: () => import('@codemirror/legacy-modes/mode/dockerfile').then((m) => m.dockerFile),
  nginx: () => import('@codemirror/legacy-modes/mode/nginx').then((m) => m.nginx),
  properties: () => import('@codemirror/legacy-modes/mode/properties').then((m) => m.properties),
  diff: () => import('@codemirror/legacy-modes/mode/diff').then((m) => m.diff),
  lua: () => import('@codemirror/legacy-modes/mode/lua').then((m) => m.lua),
  perl: () => import('@codemirror/legacy-modes/mode/perl').then((m) => m.perl),
  r: () => import('@codemirror/legacy-modes/mode/r').then((m) => m.r),
  powershell: () => import('@codemirror/legacy-modes/mode/powershell').then((m) => m.powerShell),
  haskell: () => import('@codemirror/legacy-modes/mode/haskell').then((m) => m.haskell),
  clojure: () => import('@codemirror/legacy-modes/mode/clojure').then((m) => m.clojure),
  erlang: () => import('@codemirror/legacy-modes/mode/erlang').then((m) => m.erlang),
  elm: () => import('@codemirror/legacy-modes/mode/elm').then((m) => m.elm),
  julia: () => import('@codemirror/legacy-modes/mode/julia').then((m) => m.julia),
  fsharp: () => import('@codemirror/legacy-modes/mode/mllike').then((m) => m.fSharp),
  ocaml: () => import('@codemirror/legacy-modes/mode/mllike').then((m) => m.oCaml),
  groovy: () => import('@codemirror/legacy-modes/mode/groovy').then((m) => m.groovy),
  stylus: () => import('@codemirror/legacy-modes/mode/stylus').then((m) => m.stylus),
  tcl: () => import('@codemirror/legacy-modes/mode/tcl').then((m) => m.tcl),
  protobuf: () => import('@codemirror/legacy-modes/mode/protobuf').then((m) => m.protobuf),
  cmake: () => import('@codemirror/legacy-modes/mode/cmake').then((m) => m.cmake),
}

const resolvedLanguageExtensions = new Map<string, Extension>()

function streamKeyForLang(lang: string): string | null {
  switch (lang) {
    case 'bash':
      return 'shell'
    default:
      return lang in streamParserLoaders ? lang : null
  }
}

function languageDescriptor(filePath: string): { cacheKey: string; load: LanguageLoader } | null {
  const lang = getLangFromPath(filePath)
  if (!lang) return null

  const firstPartyLoader = firstPartyLanguageLoaders[lang]
  if (firstPartyLoader) {
    return {
      cacheKey: `first-party:${lang}`,
      load: firstPartyLoader,
    }
  }

  const streamKey = streamKeyForLang(lang)
  const streamLoader = streamKey ? streamParserLoaders[streamKey] : null
  if (streamKey && streamLoader) {
    return {
      cacheKey: `stream:${streamKey}`,
      load: () => streamLoader().then((parser) => StreamLanguage.define(parser)),
    }
  }

  return null
}

/**
 * Best-effort synchronous resolution used for initial editor state. If a
 * grammar has been loaded before, re-opening that language is immediate;
 * otherwise the async resolver upgrades the language compartment shortly after
 * mount without blocking editor creation.
 */
function languageExtension(filePath: string): Extension {
  const descriptor = languageDescriptor(filePath)
  return descriptor ? resolvedLanguageExtensions.get(descriptor.cacheKey) ?? [] : []
}

/** Full async resolution — resolves the correct grammar for any supported lang. */
function resolveLanguageAsync(filePath: string): Promise<Extension> {
  const descriptor = languageDescriptor(filePath)
  if (!descriptor) return Promise.resolve([])

  const cached = resolvedLanguageExtensions.get(descriptor.cacheKey)
  if (cached) {
    return Promise.resolve(cached)
  }

  return descriptor.load().then((extension) => {
    resolvedLanguageExtensions.set(descriptor.cacheKey, extension)
    return extension
  })
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CodeEditor({
  value,
  savedValue,
  documentVersion = 0,
  onSnapshotChange,
  onDirtyChange,
  diagnostics = [],
  gitDiffMarkers = [],
  filePath,
  readOnly = false,
  preferences = DEFAULT_EDITOR_RENDER_PREFERENCES,
  onSave,
  onCursorChange,
  onSelectionChange,
  onDocumentStatsChange,
  onOpenFind,
  onGitDiffLineClick,
  onViewReady,
  className,
}: CodeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const snapshotTimerRef = useRef<number | null>(null)
  const cursorSchedulerRef = useRef(createEditorFrameScheduler())
  const lastSnapshotRef = useRef(value)
  const savedValueRef = useRef(savedValue ?? value)
  const dirtyRef = useRef(value !== (savedValue ?? value))
  const documentVersionRef = useRef(documentVersion)
  const onSnapshotChangeRef = useRef(onSnapshotChange)
  const onDirtyChangeRef = useRef(onDirtyChange)
  const onSaveRef = useRef(onSave)
  const onCursorChangeRef = useRef(onCursorChange)
  const onSelectionChangeRef = useRef(onSelectionChange)
  const onDocumentStatsChangeRef = useRef(onDocumentStatsChange)
  const onOpenFindRef = useRef(onOpenFind)
  const onGitDiffLineClickRef = useRef(onGitDiffLineClick)
  const onViewReadyRef = useRef(onViewReady)
  const langCompartment = useMemo(() => new Compartment(), [])
  const readOnlyCompartment = useMemo(() => new Compartment(), [])
  const themeCompartment = useMemo(() => new Compartment(), [])
  const diagnosticsCompartment = useMemo(() => new Compartment(), [])
  const gitDiffCompartment = useMemo(() => new Compartment(), [])
  const preferencesCompartment = useMemo(() => new Compartment(), [])
  const fontSizeCompartment = useMemo(() => new Compartment(), [])
  const { theme } = useTheme()

  onSnapshotChangeRef.current = onSnapshotChange
  onDirtyChangeRef.current = onDirtyChange
  onSaveRef.current = onSave
  onCursorChangeRef.current = onCursorChange
  onSelectionChangeRef.current = onSelectionChange
  onDocumentStatsChangeRef.current = onDocumentStatsChange
  onOpenFindRef.current = onOpenFind
  onGitDiffLineClickRef.current = onGitDiffLineClick
  onViewReadyRef.current = onViewReady

  function emitDirtyChange(dirty: boolean): void {
    if (dirtyRef.current === dirty) return
    dirtyRef.current = dirty
    onDirtyChangeRef.current?.(dirty)
  }

  function clearSnapshotTimer(): void {
    if (snapshotTimerRef.current === null) return
    window.clearTimeout(snapshotTimerRef.current)
    snapshotTimerRef.current = null
  }

  function cancelCursorFrame(): void {
    cursorSchedulerRef.current.cancel()
  }

  function flushSnapshot(): string {
    const view = viewRef.current
    if (!view) return lastSnapshotRef.current

    clearSnapshotTimer()
    const snapshot = view.state.doc.toString()
    lastSnapshotRef.current = snapshot
    onSnapshotChangeRef.current?.(snapshot)
    onDocumentStatsChangeRef.current?.({ lineCount: view.state.doc.lines })
    emitDirtyChange(snapshot !== savedValueRef.current)
    return snapshot
  }

  function scheduleSnapshot(): void {
    clearSnapshotTimer()
    snapshotTimerRef.current = window.setTimeout(() => {
      snapshotTimerRef.current = null
      flushSnapshot()
    }, EDITOR_SNAPSHOT_DEBOUNCE_MS)
  }

  function scheduleCursorReport(): void {
    cursorSchedulerRef.current.schedule(() => {
      const view = viewRef.current
      if (!view) return

      const head = view.state.selection.main.head
      const line = view.state.doc.lineAt(head)
      onCursorChangeRef.current?.({ line: line.number, column: head - line.from + 1 })
      onSelectionChangeRef.current?.(getEditorSelectionContext(view.state))
      onDocumentStatsChangeRef.current?.({ lineCount: view.state.doc.lines })
    })
  }

  useEffect(() => {
    if (!hostRef.current) return

    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        // basicSetup ships the search keymap but not the search state field;
        // without this, setSearchQuery is a no-op and findNext falls through
        // to openSearchPanel, showing the default bottom panel.
        search(),
        themeCompartment.of(buildThemeExtension(theme)),
        highlightSelectionMatches(),
        autocompletion(),
        Prec.highest(
          keymap.of([
            {
              key: 'Mod-f',
              preventDefault: true,
              run: (view) => {
                const sel = view.state.selection.main
                const initial = sel.empty
                  ? ''
                  : view.state.sliceDoc(sel.from, sel.to)
                onOpenFindRef.current?.({ withReplace: false, initialQuery: initial })
                return true
              },
            },
            {
              key: 'Mod-Alt-f',
              preventDefault: true,
              run: (view) => {
                const sel = view.state.selection.main
                const initial = sel.empty
                  ? ''
                  : view.state.sliceDoc(sel.from, sel.to)
                onOpenFindRef.current?.({ withReplace: true, initialQuery: initial })
                return true
              },
            },
          ]),
        ),
        keymap.of([
          indentWithTab,
          {
            key: 'Mod-s',
            preventDefault: true,
            run: () => {
              onSaveRef.current?.(flushSnapshot())
              return true
            },
          },
        ]),
        langCompartment.of(languageExtension(filePath)),
        diagnosticsCompartment.of(buildDiagnosticsExtension([])),
        gitDiffCompartment.of(buildGitDiffExtension([], (marker) => {
          onGitDiffLineClickRef.current?.(marker)
        })),
        readOnlyCompartment.of(EditorState.readOnly.of(readOnly)),
        preferencesCompartment.of(buildPreferenceExtensions(preferences)),
        fontSizeCompartment.of(buildFontSizeExtension(preferences.fontSize)),
        EditorView.updateListener.of((update) => {
          const isExternalSync = update.transactions.some((transaction) =>
            transaction.annotation(externalDocumentSync),
          )

          if (update.docChanged && !isExternalSync) {
            emitDirtyChange(true)
            scheduleSnapshot()
          }

          if (update.selectionSet || update.docChanged) {
            scheduleCursorReport()
          }
        }),
      ],
    })

    const view = new EditorView({ state, parent: hostRef.current })
    viewRef.current = view
    onViewReadyRef.current?.(view)
    onDocumentStatsChangeRef.current?.({ lineCount: view.state.doc.lines })
    onSelectionChangeRef.current?.(getEditorSelectionContext(view.state))
    scheduleCursorReport()

    return () => {
      onViewReadyRef.current?.(null)
      clearSnapshotTimer()
      cancelCursorFrame()
      view.destroy()
      viewRef.current = null
    }
  }, [
    diagnosticsCompartment,
    fontSizeCompartment,
    gitDiffCompartment,
    langCompartment,
    preferencesCompartment,
    readOnlyCompartment,
    themeCompartment,
  ])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: preferencesCompartment.reconfigure(buildPreferenceExtensions(preferences)),
    })
  }, [
    preferences.tabSize,
    preferences.insertSpaces,
    preferences.lineWrapping,
    preferencesCompartment,
    preferences,
  ])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: fontSizeCompartment.reconfigure(buildFontSizeExtension(preferences.fontSize)),
    })
  }, [preferences.fontSize, fontSizeCompartment])

  useEffect(() => {
    savedValueRef.current = savedValue ?? value
    emitDirtyChange(lastSnapshotRef.current !== savedValueRef.current)
  }, [savedValue, value])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({ effects: themeCompartment.reconfigure(buildThemeExtension(theme)) })
  }, [theme, themeCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    // Synchronous best-effort: reuse a previously loaded grammar immediately.
    view.dispatch({ effects: langCompartment.reconfigure(languageExtension(filePath)) })
    // Async upgrade: load first-party or legacy-mode grammars on demand and
    // swap them in if the editor and path are still live.
    let cancelled = false
    resolveLanguageAsync(filePath).then((ext) => {
      if (cancelled) return
      const current = viewRef.current
      if (!current) return
      current.dispatch({ effects: langCompartment.reconfigure(ext) })
    })
    return () => {
      cancelled = true
    }
  }, [filePath, langCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({ effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(readOnly)) })
  }, [readOnly, readOnlyCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({ effects: diagnosticsCompartment.reconfigure(buildDiagnosticsExtension(diagnostics)) })
  }, [diagnostics, diagnosticsCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: gitDiffCompartment.reconfigure(
        buildGitDiffExtension(gitDiffMarkers, (marker) => {
          onGitDiffLineClickRef.current?.(marker)
        }),
      ),
    })
  }, [gitDiffCompartment, gitDiffMarkers])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const documentVersionChanged = documentVersionRef.current !== documentVersion
    documentVersionRef.current = documentVersion

    if (
      shouldReplaceEditorDocument({
        externalValue: value,
        lastSnapshot: lastSnapshotRef.current,
        documentVersionChanged,
      })
    ) {
      clearSnapshotTimer()
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        annotations: externalDocumentSync.of(true),
      })
      lastSnapshotRef.current = value
      savedValueRef.current = savedValue ?? value
      onDocumentStatsChangeRef.current?.({ lineCount: countEditorLines(value) })
      onSelectionChangeRef.current?.(getEditorSelectionContext(view.state))
      emitDirtyChange(value !== savedValueRef.current)
      scheduleCursorReport()
    }
  }, [documentVersion, savedValue, value])

  return <div ref={hostRef} className={cn('h-full w-full overflow-hidden', className)} />
}
