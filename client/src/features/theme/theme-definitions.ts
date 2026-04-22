/**
 * Theme registry for the Cadence desktop shell.
 *
 * Each theme defines three coordinated layers:
 *   1. `colors` — CSS custom property values applied via a `.theme-<id>` class
 *      on the `<html>` element. These drive the entire UI palette.
 *   2. `editor` — the CodeMirror palette used by `components/cadence/code-editor`.
 *      Every token color is explicit so each theme renders self-consistent syntax
 *      highlighting rather than leaking across themes.
 *   3. `shiki` — the bundled Shiki theme id used by `lib/shiki` when tokenizing
 *      diff / snippet blocks outside the editor.
 *
 * To add a new theme, append an entry to {@link THEMES}. No other code needs
 * to change — the theme picker, CodeMirror, Shiki, and the `globals.css` class
 * selectors all read from this registry.
 */
export type ThemeAppearance = 'light' | 'dark'

export interface ThemeColors {
  background: string
  foreground: string
  card: string
  cardForeground: string
  popover: string
  popoverForeground: string
  primary: string
  primaryForeground: string
  secondary: string
  secondaryForeground: string
  muted: string
  mutedForeground: string
  accent: string
  accentForeground: string
  destructive: string
  destructiveForeground: string
  success: string
  successForeground: string
  border: string
  input: string
  ring: string
  chart1: string
  chart2: string
  chart3: string
  chart4: string
  chart5: string
  sidebar: string
  sidebarForeground: string
  sidebarPrimary: string
  sidebarPrimaryForeground: string
  sidebarAccent: string
  sidebarAccentForeground: string
  sidebarBorder: string
  sidebarRing: string
  /** Base background color rendered by the HTML shell before the app mounts. */
  shellBackground: string
  /** Scrollbar thumb in the default (resting) state. */
  scrollbarThumb: string
  /** Scrollbar thumb on hover. */
  scrollbarThumbHover: string
}

export interface EditorPalette {
  background: string
  foreground: string
  gutter: string
  gutterActive: string
  lineActive: string
  selection: string
  selectionMatch: string
  cursor: string
  border: string
  panelBackground: string

  keyword: string
  control: string
  storage: string
  string: string
  stringSpecial: string
  number: string
  bool: string
  constant: string
  comment: string
  function: string
  type: string
  property: string
  variable: string
  variableDef: string
  operator: string
  punctuation: string
  tagName: string
  tagBracket: string
  attribute: string
  meta: string
  link: string
  heading: string
  invalid: string

  matchingBracketBg: string
  searchMatchBg: string
  searchMatchBorder: string
  searchMatchSelectedBg: string
  autocompleteSelectedBg: string
  foldGutter: string
  foldPlaceholderText: string
}

export interface ThemeDefinition {
  id: string
  name: string
  description: string
  appearance: ThemeAppearance
  /** Shiki theme bundled for diff / snippet tokenization. */
  shiki: string
  colors: ThemeColors
  editor: EditorPalette
}

const DUSK: ThemeDefinition = {
  id: 'dusk',
  name: 'Cadence Dusk',
  description: 'Warm gold on near-black — the original Cadence palette.',
  appearance: 'dark',
  shiki: 'github-dark',
  colors: {
    background: '#121212',
    foreground: '#f8f9fa',
    card: '#1a1a1a',
    cardForeground: '#f8f9fa',
    popover: '#1a1a1a',
    popoverForeground: '#f8f9fa',
    primary: '#d4a574',
    primaryForeground: '#0a0e12',
    secondary: '#242423',
    secondaryForeground: '#f8f9fa',
    muted: '#2d2d2d',
    mutedForeground: '#a8aeb5',
    accent: '#242423',
    accentForeground: '#f8f9fa',
    destructive: '#ef4444',
    destructiveForeground: '#f8f9fa',
    success: '#4ade80',
    successForeground: '#0a0e12',
    border: '#2d2d2d',
    input: '#2d2d2d',
    ring: '#d4a574',
    chart1: '#d4a574',
    chart2: '#c19460',
    chart3: '#ad834c',
    chart4: '#997338',
    chart5: '#856324',
    sidebar: '#161616',
    sidebarForeground: '#f8f9fa',
    sidebarPrimary: '#d4a574',
    sidebarPrimaryForeground: '#0a0e12',
    sidebarAccent: '#242423',
    sidebarAccentForeground: '#f8f9fa',
    sidebarBorder: '#2d2d2d',
    sidebarRing: '#d4a574',
    shellBackground: '#101010',
    scrollbarThumb: 'oklch(0.25 0 0)',
    scrollbarThumbHover: 'oklch(0.32 0 0)',
  },
  editor: {
    background: '#121212',
    foreground: '#e4e1d6',
    gutter: 'rgba(168, 174, 181, 0.28)',
    gutterActive: '#b5b0a4',
    lineActive: 'rgba(212, 165, 116, 0.045)',
    selection: 'rgba(212, 165, 116, 0.22)',
    selectionMatch: 'rgba(212, 165, 116, 0.1)',
    cursor: '#d4a574',
    border: 'rgba(45, 45, 45, 0.9)',
    panelBackground: '#1a1a1a',

    keyword: '#d4a574',
    control: '#e89d5c',
    storage: '#c89668',
    string: '#a5b68d',
    stringSpecial: '#c8a06b',
    number: '#e88e65',
    bool: '#e88e65',
    constant: '#e88e65',
    comment: '#6e6a60',
    function: '#e8c890',
    type: '#d4b678',
    property: '#cbbfa8',
    variable: '#e4e1d6',
    variableDef: '#f0dcb5',
    operator: '#8a8780',
    punctuation: '#6e7278',
    tagName: '#e0747c',
    tagBracket: '#9a5a5f',
    attribute: '#a0c0cc',
    meta: '#8fa9c4',
    link: '#a0c0cc',
    heading: '#e8c890',
    invalid: '#ef4444',

    matchingBracketBg: 'rgba(212, 165, 116, 0.14)',
    searchMatchBg: 'rgba(212, 165, 116, 0.22)',
    searchMatchBorder: 'rgba(212, 165, 116, 0.5)',
    searchMatchSelectedBg: 'rgba(212, 165, 116, 0.4)',
    autocompleteSelectedBg: 'rgba(212, 165, 116, 0.16)',
    foldGutter: 'rgba(168, 174, 181, 0.4)',
    foldPlaceholderText: '#a8aeb5',
  },
}

const MIDNIGHT: ThemeDefinition = {
  id: 'midnight',
  name: 'Midnight',
  description: 'Cool slate blue — VSCode Dark+ inspired.',
  appearance: 'dark',
  shiki: 'dark-plus',
  colors: {
    background: '#1e1e1e',
    foreground: '#d4d4d4',
    card: '#252526',
    cardForeground: '#d4d4d4',
    popover: '#252526',
    popoverForeground: '#d4d4d4',
    primary: '#4ea1ff',
    primaryForeground: '#0a1220',
    secondary: '#2a2d2e',
    secondaryForeground: '#d4d4d4',
    muted: '#303033',
    mutedForeground: '#9ca4b0',
    accent: '#094771',
    accentForeground: '#ffffff',
    destructive: '#f48771',
    destructiveForeground: '#1e1e1e',
    success: '#89d185',
    successForeground: '#0a1220',
    border: '#3c3c3c',
    input: '#3c3c3c',
    ring: '#4ea1ff',
    chart1: '#4ea1ff',
    chart2: '#6fb4ff',
    chart3: '#3b8fe0',
    chart4: '#2e76bf',
    chart5: '#235b96',
    sidebar: '#181818',
    sidebarForeground: '#cccccc',
    sidebarPrimary: '#4ea1ff',
    sidebarPrimaryForeground: '#0a1220',
    sidebarAccent: '#2a2d2e',
    sidebarAccentForeground: '#ffffff',
    sidebarBorder: '#252526',
    sidebarRing: '#4ea1ff',
    shellBackground: '#1e1e1e',
    scrollbarThumb: 'rgba(121, 121, 121, 0.4)',
    scrollbarThumbHover: 'rgba(100, 100, 100, 0.7)',
  },
  editor: {
    background: '#1e1e1e',
    foreground: '#d4d4d4',
    gutter: '#858585',
    gutterActive: '#c6c6c6',
    lineActive: 'rgba(255, 255, 255, 0.04)',
    selection: 'rgba(38, 79, 120, 0.6)',
    selectionMatch: 'rgba(87, 87, 87, 0.4)',
    cursor: '#aeafad',
    border: 'rgba(60, 60, 60, 0.9)',
    panelBackground: '#252526',

    keyword: '#569cd6',
    control: '#c586c0',
    storage: '#569cd6',
    string: '#ce9178',
    stringSpecial: '#d7ba7d',
    number: '#b5cea8',
    bool: '#569cd6',
    constant: '#4fc1ff',
    comment: '#6a9955',
    function: '#dcdcaa',
    type: '#4ec9b0',
    property: '#9cdcfe',
    variable: '#d4d4d4',
    variableDef: '#9cdcfe',
    operator: '#d4d4d4',
    punctuation: '#808080',
    tagName: '#569cd6',
    tagBracket: '#808080',
    attribute: '#9cdcfe',
    meta: '#c586c0',
    link: '#4ea1ff',
    heading: '#569cd6',
    invalid: '#f48771',

    matchingBracketBg: 'rgba(78, 161, 255, 0.18)',
    searchMatchBg: 'rgba(234, 92, 0, 0.33)',
    searchMatchBorder: 'rgba(234, 92, 0, 0.66)',
    searchMatchSelectedBg: 'rgba(234, 92, 0, 0.55)',
    autocompleteSelectedBg: 'rgba(9, 71, 113, 0.9)',
    foldGutter: 'rgba(133, 133, 133, 0.6)',
    foldPlaceholderText: '#b0b0b0',
  },
}

const DAYLIGHT: ThemeDefinition = {
  id: 'daylight',
  name: 'Daylight',
  description: 'Clean white surfaces — VSCode Light+ inspired.',
  appearance: 'light',
  shiki: 'light-plus',
  colors: {
    background: '#ffffff',
    foreground: '#1f2328',
    card: '#ffffff',
    cardForeground: '#1f2328',
    popover: '#ffffff',
    popoverForeground: '#1f2328',
    primary: '#0969da',
    primaryForeground: '#ffffff',
    secondary: '#f3f4f6',
    secondaryForeground: '#1f2328',
    muted: '#eef1f4',
    mutedForeground: '#596773',
    accent: '#dbeafe',
    accentForeground: '#0f2a4a',
    destructive: '#cf222e',
    destructiveForeground: '#ffffff',
    success: '#1a7f37',
    successForeground: '#ffffff',
    border: '#d0d7de',
    input: '#d0d7de',
    ring: '#0969da',
    chart1: '#0969da',
    chart2: '#388bfd',
    chart3: '#2a6ac2',
    chart4: '#1f4d8b',
    chart5: '#153a6a',
    sidebar: '#f6f8fa',
    sidebarForeground: '#1f2328',
    sidebarPrimary: '#0969da',
    sidebarPrimaryForeground: '#ffffff',
    sidebarAccent: '#dbeafe',
    sidebarAccentForeground: '#0f2a4a',
    sidebarBorder: '#d0d7de',
    sidebarRing: '#0969da',
    shellBackground: '#ffffff',
    scrollbarThumb: 'rgba(100, 116, 139, 0.35)',
    scrollbarThumbHover: 'rgba(71, 85, 105, 0.55)',
  },
  editor: {
    background: '#ffffff',
    foreground: '#1f2328',
    gutter: '#8c959f',
    gutterActive: '#24292f',
    lineActive: 'rgba(9, 105, 218, 0.04)',
    selection: 'rgba(9, 105, 218, 0.18)',
    selectionMatch: 'rgba(9, 105, 218, 0.08)',
    cursor: '#0969da',
    border: 'rgba(208, 215, 222, 0.9)',
    panelBackground: '#f6f8fa',

    keyword: '#cf222e',
    control: '#cf222e',
    storage: '#cf222e',
    string: '#0a3069',
    stringSpecial: '#116329',
    number: '#0550ae',
    bool: '#0550ae',
    constant: '#0550ae',
    comment: '#6e7781',
    function: '#8250df',
    type: '#953800',
    property: '#0550ae',
    variable: '#1f2328',
    variableDef: '#24292f',
    operator: '#cf222e',
    punctuation: '#57606a',
    tagName: '#116329',
    tagBracket: '#57606a',
    attribute: '#0550ae',
    meta: '#8250df',
    link: '#0969da',
    heading: '#0550ae',
    invalid: '#cf222e',

    matchingBracketBg: 'rgba(9, 105, 218, 0.16)',
    searchMatchBg: 'rgba(255, 223, 93, 0.55)',
    searchMatchBorder: 'rgba(212, 167, 44, 0.9)',
    searchMatchSelectedBg: 'rgba(212, 167, 44, 0.75)',
    autocompleteSelectedBg: 'rgba(9, 105, 218, 0.14)',
    foldGutter: 'rgba(87, 96, 106, 0.5)',
    foldPlaceholderText: '#57606a',
  },
}

export const THEMES: ThemeDefinition[] = [DUSK, MIDNIGHT, DAYLIGHT]

export const DEFAULT_THEME_ID = DUSK.id

export function getThemeById(id: string | null | undefined): ThemeDefinition {
  if (id) {
    const match = THEMES.find((theme) => theme.id === id)
    if (match) return match
  }
  return THEMES[0]
}

export function themeClassName(id: string): string {
  return `theme-${id}`
}

export const THEME_STORAGE_KEY = 'cadence:theme'
