"use client"

import { useEffect, useMemo, useRef } from 'react'
import { EditorView, basicSetup } from 'codemirror'
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { indentWithTab } from '@codemirror/commands'
import { highlightSelectionMatches } from '@codemirror/search'
import { keymap } from '@codemirror/view'
import { autocompletion } from '@codemirror/autocomplete'
import { javascript } from '@codemirror/lang-javascript'
import { python } from '@codemirror/lang-python'
import { json } from '@codemirror/lang-json'
import { markdown } from '@codemirror/lang-markdown'
import { css } from '@codemirror/lang-css'
import { html } from '@codemirror/lang-html'
import { cn } from '@/lib/utils'
import { getLangFromPath } from '@/lib/shiki'

interface CodeEditorProps {
  value: string
  onChange: (value: string) => void
  filePath: string
  readOnly?: boolean
  onSave?: () => void
  onCursorChange?: (position: { line: number; column: number }) => void
  className?: string
}

// ---------------------------------------------------------------------------
// Theme — "Cadence Dusk": quiet, warm, low-contrast palette that matches the
// app's warm-gold accent on near-black. Most identifiers stay in the default
// soft off-white, with only a handful of accent tones reserved for structural
// tokens (keywords, strings, comments, types).
// ---------------------------------------------------------------------------

const PALETTE = {
  background: '#121212',
  foreground: '#e4e1d6',
  gutter: 'rgba(168, 174, 181, 0.28)',
  gutterActive: '#b5b0a4',
  lineActive: 'rgba(212, 165, 116, 0.045)',
  selection: 'rgba(212, 165, 116, 0.22)',
  selectionMatch: 'rgba(212, 165, 116, 0.1)',
  cursor: '#d4a574',
  border: 'rgba(45, 45, 45, 0.9)',

  // syntax tones — warm dusk palette with complementary accents
  keyword: '#d4a574', // warm gold — import/export/const/function
  control: '#e89d5c', // amber — return/if/else/for/while
  storage: '#c89668', // dimmer gold — modifiers, async, await
  string: '#a5b68d', // muted sage
  stringSpecial: '#c8a06b', // tan — template literals, escapes
  number: '#e88e65', // warm coral
  bool: '#e88e65', // warm coral
  constant: '#e88e65',
  comment: '#6e6a60', // olive-gray italic
  function: '#e8c890', // soft cream
  type: '#d4b678', // straw yellow
  property: '#cbbfa8', // warm neutral
  variable: '#e4e1d6', // off-white default
  variableDef: '#f0dcb5', // slightly warmer for var definitions
  operator: '#8a8780', // warm gray
  punctuation: '#6e7278', // quiet gray
  tagName: '#e0747c', // dusty rose — JSX/HTML tags
  tagBracket: '#9a5a5f', // darker rose for angle brackets
  attribute: '#a0c0cc', // muted sky — JSX/HTML attributes
  meta: '#8fa9c4', // cool blue-gray — annotations, decorators
  link: '#a0c0cc',
  heading: '#e8c890',
  invalid: '#ef4444',
}

const appSyntaxHighlight = HighlightStyle.define([
  // keywords — layered warmth
  { tag: [t.keyword, t.moduleKeyword, t.definitionKeyword], color: PALETTE.keyword },
  { tag: t.controlKeyword, color: PALETTE.control, fontStyle: 'italic' },
  { tag: t.modifier, color: PALETTE.storage },

  // identifiers
  { tag: [t.name, t.character, t.macroName], color: PALETTE.variable },
  { tag: [t.definition(t.variableName), t.separator], color: PALETTE.variableDef },
  { tag: [t.propertyName], color: PALETTE.property },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.labelName], color: PALETTE.function },

  // types / classes
  { tag: [t.typeName, t.className, t.namespace], color: PALETTE.type, fontStyle: 'italic' },

  // literals
  { tag: [t.number], color: PALETTE.number },
  { tag: [t.bool, t.null, t.atom, t.self, t.special(t.variableName)], color: PALETTE.bool },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: PALETTE.constant },

  // strings
  { tag: [t.string], color: PALETTE.string },
  { tag: [t.special(t.string), t.regexp, t.escape], color: PALETTE.stringSpecial },
  { tag: [t.processingInstruction, t.inserted], color: PALETTE.string },

  // punctuation / operators
  { tag: [t.operator, t.operatorKeyword], color: PALETTE.operator },
  { tag: [t.punctuation, t.bracket, t.brace, t.paren, t.derefOperator, t.squareBracket], color: PALETTE.punctuation },
  { tag: t.angleBracket, color: PALETTE.tagBracket },

  // JSX / HTML / XML
  { tag: [t.tagName], color: PALETTE.tagName },
  { tag: [t.attributeName], color: PALETTE.attribute },
  { tag: [t.attributeValue], color: PALETTE.string },

  // markdown
  { tag: t.heading, color: PALETTE.heading, fontWeight: '600' },
  { tag: [t.heading1, t.heading2], color: PALETTE.heading, fontWeight: '700' },
  { tag: t.link, color: PALETTE.link, textDecoration: 'underline' },
  { tag: t.url, color: PALETTE.link, textDecoration: 'underline' },
  { tag: t.quote, color: PALETTE.property, fontStyle: 'italic' },
  { tag: t.monospace, color: PALETTE.stringSpecial },
  { tag: t.strong, color: PALETTE.control, fontWeight: '700' },
  { tag: t.emphasis, color: PALETTE.string, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.list, color: PALETTE.control },

  // comments / meta
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: PALETTE.comment, fontStyle: 'italic' },
  { tag: [t.meta, t.annotation], color: PALETTE.meta },
  { tag: t.changed, color: PALETTE.number },
  { tag: t.invalid, color: PALETTE.invalid, textDecoration: 'underline wavy' },
])

const appEditorTheme = EditorView.theme(
  {
    '&': {
      color: PALETTE.foreground,
      backgroundColor: PALETTE.background,
      height: '100%',
      fontSize: '13px',
    },
    '&.cm-editor.cm-focused': { outline: 'none' },
    '.cm-scroller': {
      overflow: 'auto',
      fontFamily: "ui-monospace, 'SF Mono', Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
      lineHeight: '1.6',
    },
    '.cm-content': {
      minHeight: '100%',
      caretColor: PALETTE.cursor,
      padding: '6px 0',
    },
    '.cm-gutters': {
      backgroundColor: PALETTE.background,
      borderRight: `1px solid ${PALETTE.border}`,
      color: PALETTE.gutter,
    },
    '.cm-lineNumbers .cm-gutterElement': {
      padding: '0 14px 0 10px',
      minWidth: '28px',
      fontVariantNumeric: 'tabular-nums',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'transparent',
      color: PALETTE.gutterActive,
    },
    '.cm-activeLine': { backgroundColor: PALETTE.lineActive },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: PALETTE.cursor, borderLeftWidth: '2px' },
    '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-content ::selection':
      { backgroundColor: `${PALETTE.selection} !important` },
    '.cm-selectionMatch': { backgroundColor: PALETTE.selectionMatch },
    '.cm-matchingBracket, .cm-nonmatchingBracket': {
      backgroundColor: 'rgba(212, 165, 116, 0.14)',
      color: PALETTE.foreground,
      outline: 'none',
    },
    '.cm-searchMatch': {
      backgroundColor: 'rgba(212, 165, 116, 0.22)',
      outline: `1px solid rgba(212, 165, 116, 0.5)`,
    },
    '.cm-searchMatch.cm-searchMatch-selected': {
      backgroundColor: 'rgba(212, 165, 116, 0.4)',
    },
    '.cm-panels': { backgroundColor: '#1a1a1a', color: PALETTE.foreground },
    '.cm-panels.cm-panels-bottom': { borderTop: `1px solid ${PALETTE.border}` },
    '.cm-tooltip': {
      backgroundColor: '#1a1a1a',
      borderColor: PALETTE.border,
      color: PALETTE.foreground,
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'rgba(212, 165, 116, 0.16)',
      color: PALETTE.foreground,
    },
    '.cm-foldGutter .cm-gutterElement': {
      color: 'rgba(168, 174, 181, 0.4)',
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'transparent',
      border: `1px solid ${PALETTE.border}`,
      color: '#a8aeb5',
      padding: '0 4px',
    },
  },
  { dark: true },
)

// ---------------------------------------------------------------------------
// Language resolution
// ---------------------------------------------------------------------------

function languageExtension(filePath: string): Extension[] {
  const lang = getLangFromPath(filePath)
  if (!lang) return []

  switch (lang) {
    case 'typescript':
      return [javascript({ typescript: true })]
    case 'tsx':
      return [javascript({ jsx: true, typescript: true })]
    case 'jsx':
      return [javascript({ jsx: true })]
    case 'javascript':
      return [javascript()]
    case 'python':
      return [python()]
    case 'json':
    case 'jsonc':
      return [json()]
    case 'markdown':
    case 'mdx':
      return [markdown()]
    case 'css':
    case 'scss':
      return [css()]
    case 'html':
      return [html()]
    default:
      return []
  }
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
  className,
}: CodeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onSaveRef = useRef(onSave)
  const onCursorChangeRef = useRef(onCursorChange)
  const langCompartment = useMemo(() => new Compartment(), [])
  const readOnlyCompartment = useMemo(() => new Compartment(), [])

  onChangeRef.current = onChange
  onSaveRef.current = onSave
  onCursorChangeRef.current = onCursorChange

  useEffect(() => {
    if (!hostRef.current) return

    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        syntaxHighlighting(appSyntaxHighlight),
        appEditorTheme,
        highlightSelectionMatches(),
        autocompletion(),
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

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [langCompartment, readOnlyCompartment])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({ effects: langCompartment.reconfigure(languageExtension(filePath)) })
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
