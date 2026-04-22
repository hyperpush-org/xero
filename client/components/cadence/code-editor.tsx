"use client"

import { useEffect, useMemo, useRef } from 'react'
import { EditorView, basicSetup } from 'codemirror'
import { Compartment, EditorState, Prec, type Extension } from '@codemirror/state'
import {
  HighlightStyle,
  StreamLanguage,
  syntaxHighlighting,
} from '@codemirror/language'
import type { StreamParser } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { indentWithTab } from '@codemirror/commands'
import { highlightSelectionMatches, search } from '@codemirror/search'
import { keymap } from '@codemirror/view'
import { autocompletion } from '@codemirror/autocomplete'
import { javascript } from '@codemirror/lang-javascript'
import { python } from '@codemirror/lang-python'
import { json } from '@codemirror/lang-json'
import { markdown } from '@codemirror/lang-markdown'
import { css } from '@codemirror/lang-css'
import { html } from '@codemirror/lang-html'
import { rust } from '@codemirror/lang-rust'
import { cpp } from '@codemirror/lang-cpp'
import { java } from '@codemirror/lang-java'
import { go } from '@codemirror/lang-go'
import { sql } from '@codemirror/lang-sql'
import { yaml } from '@codemirror/lang-yaml'
import { xml } from '@codemirror/lang-xml'
import { php } from '@codemirror/lang-php'
import { vue } from '@codemirror/lang-vue'
import { sass } from '@codemirror/lang-sass'
import { less } from '@codemirror/lang-less'
import { angular } from '@codemirror/lang-angular'
import { cn } from '@/lib/utils'
import { getLangFromPath } from '@/lib/shiki'
import { useTheme } from '@/src/features/theme/theme-provider'
import type {
  EditorPalette,
  ThemeDefinition,
} from '@/src/features/theme/theme-definitions'

interface CodeEditorProps {
  value: string
  onChange: (value: string) => void
  filePath: string
  readOnly?: boolean
  onSave?: () => void
  onCursorChange?: (position: { line: number; column: number }) => void
  onOpenFind?: (options: { withReplace: boolean; initialQuery: string }) => void
  onViewReady?: (view: EditorView | null) => void
  className?: string
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

// ---------------------------------------------------------------------------
// Language resolution
// ---------------------------------------------------------------------------

// Legacy-mode parsers are heavy / numerous — load lazily so the editor's
// initial chunk only pulls in what the active file actually needs.
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

// Cache resolved stream parsers so re-opening the same language is synchronous.
const resolvedStreamParsers: Record<string, StreamParser<unknown>> = {}

function cachedStreamExtension(key: string): Extension | null {
  const cached = resolvedStreamParsers[key]
  return cached ? StreamLanguage.define(cached) : null
}

function loadStreamExtension(key: string): Promise<Extension | null> {
  const cached = resolvedStreamParsers[key]
  if (cached) return Promise.resolve(StreamLanguage.define(cached))
  const loader = streamParserLoaders[key]
  if (!loader) return Promise.resolve(null)
  return loader().then((parser) => {
    resolvedStreamParsers[key] = parser
    return StreamLanguage.define(parser)
  })
}

function streamKeyForLang(lang: string): string | null {
  switch (lang) {
    case 'bash':
      return 'shell'
    default:
      return lang in streamParserLoaders ? lang : null
  }
}

/**
 * Resolve a first-party (synchronous) CodeMirror grammar for a given lang id.
 * Returns null when the language is served by a legacy StreamLanguage parser,
 * which must be loaded asynchronously via {@link resolveLanguageAsync}.
 */
function firstPartyExtension(lang: string): Extension | null {
  switch (lang) {
    case 'typescript':
      return javascript({ typescript: true })
    case 'tsx':
      return javascript({ jsx: true, typescript: true })
    case 'jsx':
      return javascript({ jsx: true })
    case 'javascript':
      return javascript()
    case 'python':
      return python()
    case 'json':
    case 'jsonc':
      return json()
    case 'markdown':
    case 'mdx':
      return markdown()
    case 'css':
      return css()
    case 'scss':
      return sass({ indented: false })
    case 'sass':
      return sass({ indented: true })
    case 'less':
      return less()
    case 'html':
      return html()
    case 'xml':
      return xml()
    case 'rust':
      return rust()
    case 'c':
    case 'cpp':
      return cpp()
    case 'java':
      return java()
    case 'go':
      return go()
    case 'sql':
      return sql()
    case 'yaml':
      return yaml()
    case 'php':
      return php()
    case 'vue':
      return vue()
    case 'angular':
      return angular()
    // GraphQL has no official CM6 grammar; JS tokenization is a fair approximation.
    case 'graphql':
      return javascript()
    default:
      return null
  }
}

/**
 * Best-effort synchronous resolution — used for the initial editor state.
 * Falls back to an empty extension for legacy-mode langs that haven't been
 * loaded yet; {@link resolveLanguageAsync} then upgrades the compartment.
 */
function languageExtension(filePath: string): Extension {
  const lang = getLangFromPath(filePath)
  if (!lang) return []
  const firstParty = firstPartyExtension(lang)
  if (firstParty) return firstParty
  const streamKey = streamKeyForLang(lang)
  if (streamKey) {
    const cached = cachedStreamExtension(streamKey)
    if (cached) return cached
  }
  return []
}

/** Full async resolution — resolves the correct grammar for any supported lang. */
function resolveLanguageAsync(filePath: string): Promise<Extension> {
  const lang = getLangFromPath(filePath)
  if (!lang) return Promise.resolve([])
  const firstParty = firstPartyExtension(lang)
  if (firstParty) return Promise.resolve(firstParty)
  const streamKey = streamKeyForLang(lang)
  if (streamKey) {
    return loadStreamExtension(streamKey).then((ext) => ext ?? [])
  }
  return Promise.resolve([])
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CodeEditor({
  value,
  onChange,
  filePath,
  readOnly = false,
  onSave,
  onCursorChange,
  onOpenFind,
  onViewReady,
  className,
}: CodeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onSaveRef = useRef(onSave)
  const onCursorChangeRef = useRef(onCursorChange)
  const onOpenFindRef = useRef(onOpenFind)
  const onViewReadyRef = useRef(onViewReady)
  const langCompartment = useMemo(() => new Compartment(), [])
  const readOnlyCompartment = useMemo(() => new Compartment(), [])
  const themeCompartment = useMemo(() => new Compartment(), [])
  const { theme } = useTheme()

  onChangeRef.current = onChange
  onSaveRef.current = onSave
  onCursorChangeRef.current = onCursorChange
  onOpenFindRef.current = onOpenFind
  onViewReadyRef.current = onViewReady

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
              onSaveRef.current?.()
              return true
            },
          },
        ]),
        langCompartment.of(languageExtension(filePath)),
        readOnlyCompartment.of(EditorState.readOnly.of(readOnly)),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current?.(update.state.doc.toString())
          }
          if (update.selectionSet || update.docChanged) {
            const head = update.state.selection.main.head
            const line = update.state.doc.lineAt(head)
            onCursorChangeRef.current?.({ line: line.number, column: head - line.from + 1 })
          }
        }),
      ],
    })

    const view = new EditorView({ state, parent: hostRef.current })
    viewRef.current = view
    onViewReadyRef.current?.(view)

    return () => {
      onViewReadyRef.current?.(null)
      view.destroy()
      viewRef.current = null
    }
  }, [langCompartment, readOnlyCompartment, themeCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({ effects: themeCompartment.reconfigure(buildThemeExtension(theme)) })
  }, [theme, themeCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    // Synchronous best-effort: first-party grammars resolve immediately,
    // legacy-mode grammars fall back to an empty extension here.
    view.dispatch({ effects: langCompartment.reconfigure(languageExtension(filePath)) })
    // Async upgrade: resolves the real grammar for legacy-mode languages
    // (lazy import) and swaps it in if the editor and path are still live.
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
    const current = view.state.doc.toString()
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      })
    }
  }, [value])

  return <div ref={hostRef} className={cn('h-full w-full overflow-hidden', className)} />
}
