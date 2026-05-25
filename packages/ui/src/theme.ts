/**
 * Shared theme registry for the Xero desktop and cloud shells.
 *
 * Each theme defines three coordinated layers:
 *   1. `colors` — CSS custom property values applied via a `.theme-<id>` class
 *      on the `<html>` element. These drive the entire UI palette.
 *   2. `editor` — the CodeMirror palette used by `components/xero/code-editor`.
 *      Every token color is explicit so each theme renders self-consistent syntax
 *      highlighting rather than leaking across themes.
 *   3. `shiki` — the bundled Shiki theme id used by `lib/shiki` when tokenizing
 *      diff / snippet blocks outside the editor.
 *
 * To add a new theme, append an entry to {@link THEMES} and the matching
 * selector in `packages/ui/src/styles.css`. Both app shells consume this
 * registry; cloud receives only a theme id for built-in themes.
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
  warning: string
  warningForeground: string
  info: string
  infoForeground: string
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

export function isThemeDefinition(value: unknown): value is ThemeDefinition {
  if (!value || typeof value !== 'object') return false
  const t = value as Partial<ThemeDefinition>
  return (
    typeof t.id === 'string' &&
    typeof t.name === 'string' &&
    typeof t.description === 'string' &&
    (t.appearance === 'dark' || t.appearance === 'light') &&
    typeof t.shiki === 'string' &&
    typeof t.colors === 'object' &&
    t.colors !== null &&
    typeof t.editor === 'object' &&
    t.editor !== null
  )
}

const DUSK: ThemeDefinition = {
  id: 'dusk',
  name: 'Xero Dusk',
  description: 'Warm gold on near-black — the original Xero palette.',
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
    warning: '#f5b962',
    warningForeground: '#0a0e12',
    info: '#7cc1e8',
    infoForeground: '#0a0e12',
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
    warning: '#fbbf24',
    warningForeground: '#0a1220',
    info: '#4ea1ff',
    infoForeground: '#0a1220',
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
    mutedForeground: '#4b5763',
    accent: '#dbeafe',
    accentForeground: '#0f2a4a',
    destructive: '#cf222e',
    destructiveForeground: '#ffffff',
    success: '#1a7f37',
    successForeground: '#ffffff',
    warning: '#b45309',
    warningForeground: '#ffffff',
    info: '#1d4ed8',
    infoForeground: '#ffffff',
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

const NORD: ThemeDefinition = {
  id: 'nord',
  name: 'Nord',
  description: 'Arctic minimalism — cool frost blues on polar night.',
  appearance: 'dark',
  shiki: 'nord',
  colors: {
    background: '#2e3440',
    foreground: '#d8dee9',
    card: '#3b4252',
    cardForeground: '#eceff4',
    popover: '#3b4252',
    popoverForeground: '#eceff4',
    primary: '#88c0d0',
    primaryForeground: '#2e3440',
    secondary: '#434c5e',
    secondaryForeground: '#eceff4',
    muted: '#434c5e',
    mutedForeground: '#929aac',
    accent: '#4c566a',
    accentForeground: '#eceff4',
    destructive: '#bf616a',
    destructiveForeground: '#eceff4',
    success: '#a3be8c',
    successForeground: '#2e3440',
    warning: '#ebcb8b',
    warningForeground: '#2e3440',
    info: '#81a1c1',
    infoForeground: '#2e3440',
    border: '#434c5e',
    input: '#434c5e',
    ring: '#88c0d0',
    chart1: '#88c0d0',
    chart2: '#81a1c1',
    chart3: '#5e81ac',
    chart4: '#8fbcbb',
    chart5: '#a3be8c',
    sidebar: '#272c36',
    sidebarForeground: '#d8dee9',
    sidebarPrimary: '#88c0d0',
    sidebarPrimaryForeground: '#2e3440',
    sidebarAccent: '#3b4252',
    sidebarAccentForeground: '#eceff4',
    sidebarBorder: '#3b4252',
    sidebarRing: '#88c0d0',
    shellBackground: '#2e3440',
    scrollbarThumb: 'rgba(136, 192, 208, 0.22)',
    scrollbarThumbHover: 'rgba(136, 192, 208, 0.42)',
  },
  editor: {
    background: '#2e3440',
    foreground: '#d8dee9',
    gutter: '#4c566a',
    gutterActive: '#d8dee9',
    lineActive: 'rgba(136, 192, 208, 0.06)',
    selection: 'rgba(136, 192, 208, 0.28)',
    selectionMatch: 'rgba(136, 192, 208, 0.14)',
    cursor: '#88c0d0',
    border: 'rgba(67, 76, 94, 0.9)',
    panelBackground: '#3b4252',

    keyword: '#81a1c1',
    control: '#81a1c1',
    storage: '#81a1c1',
    string: '#a3be8c',
    stringSpecial: '#ebcb8b',
    number: '#b48ead',
    bool: '#81a1c1',
    constant: '#b48ead',
    comment: '#4c566a',
    function: '#88c0d0',
    type: '#8fbcbb',
    property: '#d8dee9',
    variable: '#eceff4',
    variableDef: '#eceff4',
    operator: '#81a1c1',
    punctuation: '#eceff4',
    tagName: '#81a1c1',
    tagBracket: '#81a1c1',
    attribute: '#8fbcbb',
    meta: '#d08770',
    link: '#88c0d0',
    heading: '#88c0d0',
    invalid: '#bf616a',

    matchingBracketBg: 'rgba(136, 192, 208, 0.2)',
    searchMatchBg: 'rgba(235, 203, 139, 0.3)',
    searchMatchBorder: 'rgba(235, 203, 139, 0.7)',
    searchMatchSelectedBg: 'rgba(235, 203, 139, 0.5)',
    autocompleteSelectedBg: 'rgba(94, 129, 172, 0.55)',
    foldGutter: 'rgba(76, 86, 106, 0.7)',
    foldPlaceholderText: '#d8dee9',
  },
}

const DRACULA: ThemeDefinition = {
  id: 'dracula',
  name: 'Dracula',
  description: 'Vibrant purple, pink, and cyan on deep noir.',
  appearance: 'dark',
  shiki: 'dracula',
  colors: {
    background: '#282a36',
    foreground: '#f8f8f2',
    card: '#343746',
    cardForeground: '#f8f8f2',
    popover: '#343746',
    popoverForeground: '#f8f8f2',
    primary: '#bd93f9',
    primaryForeground: '#1a1b24',
    secondary: '#44475a',
    secondaryForeground: '#f8f8f2',
    muted: '#44475a',
    mutedForeground: '#a6adcb',
    accent: '#44475a',
    accentForeground: '#f8f8f2',
    destructive: '#ff5555',
    destructiveForeground: '#1a1b24',
    success: '#50fa7b',
    successForeground: '#1a1b24',
    warning: '#ffb86c',
    warningForeground: '#1a1b24',
    info: '#8be9fd',
    infoForeground: '#1a1b24',
    border: '#44475a',
    input: '#44475a',
    ring: '#bd93f9',
    chart1: '#bd93f9',
    chart2: '#ff79c6',
    chart3: '#8be9fd',
    chart4: '#50fa7b',
    chart5: '#ffb86c',
    sidebar: '#21222c',
    sidebarForeground: '#f8f8f2',
    sidebarPrimary: '#bd93f9',
    sidebarPrimaryForeground: '#1a1b24',
    sidebarAccent: '#44475a',
    sidebarAccentForeground: '#f8f8f2',
    sidebarBorder: '#343746',
    sidebarRing: '#bd93f9',
    shellBackground: '#282a36',
    scrollbarThumb: 'rgba(189, 147, 249, 0.22)',
    scrollbarThumbHover: 'rgba(189, 147, 249, 0.42)',
  },
  editor: {
    background: '#282a36',
    foreground: '#f8f8f2',
    gutter: '#6272a4',
    gutterActive: '#f8f8f2',
    lineActive: 'rgba(189, 147, 249, 0.07)',
    selection: 'rgba(189, 147, 249, 0.32)',
    selectionMatch: 'rgba(189, 147, 249, 0.16)',
    cursor: '#f8f8f2',
    border: 'rgba(68, 71, 90, 0.9)',
    panelBackground: '#343746',

    keyword: '#ff79c6',
    control: '#ff79c6',
    storage: '#ff79c6',
    string: '#f1fa8c',
    stringSpecial: '#ffb86c',
    number: '#bd93f9',
    bool: '#bd93f9',
    constant: '#bd93f9',
    comment: '#6272a4',
    function: '#50fa7b',
    type: '#8be9fd',
    property: '#f8f8f2',
    variable: '#f8f8f2',
    variableDef: '#f8f8f2',
    operator: '#ff79c6',
    punctuation: '#f8f8f2',
    tagName: '#ff79c6',
    tagBracket: '#ff79c6',
    attribute: '#50fa7b',
    meta: '#ffb86c',
    link: '#8be9fd',
    heading: '#ff79c6',
    invalid: '#ff5555',

    matchingBracketBg: 'rgba(189, 147, 249, 0.24)',
    searchMatchBg: 'rgba(241, 250, 140, 0.28)',
    searchMatchBorder: 'rgba(241, 250, 140, 0.7)',
    searchMatchSelectedBg: 'rgba(241, 250, 140, 0.55)',
    autocompleteSelectedBg: 'rgba(98, 114, 164, 0.55)',
    foldGutter: 'rgba(98, 114, 164, 0.7)',
    foldPlaceholderText: '#a6adcb',
  },
}

const SOLARIZED_DARK: ThemeDefinition = {
  id: 'solarized-dark',
  name: 'Solarized Dark',
  description: 'Low-contrast teal base with warm yellow accents.',
  appearance: 'dark',
  shiki: 'solarized-dark',
  colors: {
    background: '#002b36',
    foreground: '#93a1a1',
    card: '#073642',
    cardForeground: '#eee8d5',
    popover: '#073642',
    popoverForeground: '#eee8d5',
    primary: '#b58900',
    primaryForeground: '#002b36',
    secondary: '#073642',
    secondaryForeground: '#93a1a1',
    muted: '#073642',
    mutedForeground: '#839496',
    accent: '#586e75',
    accentForeground: '#eee8d5',
    destructive: '#dc322f',
    destructiveForeground: '#eee8d5',
    success: '#859900',
    successForeground: '#002b36',
    warning: '#cb4b16',
    warningForeground: '#002b36',
    info: '#268bd2',
    infoForeground: '#002b36',
    border: '#073642',
    input: '#073642',
    ring: '#b58900',
    chart1: '#b58900',
    chart2: '#268bd2',
    chart3: '#2aa198',
    chart4: '#859900',
    chart5: '#cb4b16',
    sidebar: '#00212b',
    sidebarForeground: '#93a1a1',
    sidebarPrimary: '#b58900',
    sidebarPrimaryForeground: '#002b36',
    sidebarAccent: '#073642',
    sidebarAccentForeground: '#eee8d5',
    sidebarBorder: '#073642',
    sidebarRing: '#b58900',
    shellBackground: '#002b36',
    scrollbarThumb: 'rgba(147, 161, 161, 0.22)',
    scrollbarThumbHover: 'rgba(147, 161, 161, 0.42)',
  },
  editor: {
    background: '#002b36',
    foreground: '#93a1a1',
    gutter: '#586e75',
    gutterActive: '#93a1a1',
    lineActive: 'rgba(181, 137, 0, 0.06)',
    selection: 'rgba(7, 54, 66, 0.9)',
    selectionMatch: 'rgba(88, 110, 117, 0.35)',
    cursor: '#b58900',
    border: 'rgba(7, 54, 66, 0.9)',
    panelBackground: '#073642',

    keyword: '#859900',
    control: '#cb4b16',
    storage: '#859900',
    string: '#2aa198',
    stringSpecial: '#6c71c4',
    number: '#d33682',
    bool: '#d33682',
    constant: '#d33682',
    comment: '#586e75',
    function: '#268bd2',
    type: '#b58900',
    property: '#93a1a1',
    variable: '#93a1a1',
    variableDef: '#eee8d5',
    operator: '#859900',
    punctuation: '#657b83',
    tagName: '#268bd2',
    tagBracket: '#586e75',
    attribute: '#268bd2',
    meta: '#6c71c4',
    link: '#268bd2',
    heading: '#b58900',
    invalid: '#dc322f',

    matchingBracketBg: 'rgba(181, 137, 0, 0.2)',
    searchMatchBg: 'rgba(181, 137, 0, 0.3)',
    searchMatchBorder: 'rgba(181, 137, 0, 0.7)',
    searchMatchSelectedBg: 'rgba(181, 137, 0, 0.55)',
    autocompleteSelectedBg: 'rgba(38, 139, 210, 0.35)',
    foldGutter: 'rgba(88, 110, 117, 0.7)',
    foldPlaceholderText: '#93a1a1',
  },
}

const MONOKAI: ThemeDefinition = {
  id: 'monokai',
  name: 'Monokai',
  description: 'Classic high-contrast — neon pink, green, and amber.',
  appearance: 'dark',
  shiki: 'monokai',
  colors: {
    background: '#272822',
    foreground: '#f8f8f2',
    card: '#2f302a',
    cardForeground: '#f8f8f2',
    popover: '#2f302a',
    popoverForeground: '#f8f8f2',
    primary: '#f92672',
    primaryForeground: '#1a1b14',
    secondary: '#3e3d32',
    secondaryForeground: '#f8f8f2',
    muted: '#3e3d32',
    mutedForeground: '#aba897',
    accent: '#49483e',
    accentForeground: '#f8f8f2',
    destructive: '#f92672',
    destructiveForeground: '#f8f8f2',
    success: '#a6e22e',
    successForeground: '#1a1b14',
    warning: '#fd971f',
    warningForeground: '#1a1b14',
    info: '#66d9ef',
    infoForeground: '#1a1b14',
    border: '#3e3d32',
    input: '#3e3d32',
    ring: '#f92672',
    chart1: '#f92672',
    chart2: '#a6e22e',
    chart3: '#66d9ef',
    chart4: '#fd971f',
    chart5: '#ae81ff',
    sidebar: '#1e1f1c',
    sidebarForeground: '#f8f8f2',
    sidebarPrimary: '#f92672',
    sidebarPrimaryForeground: '#1a1b14',
    sidebarAccent: '#3e3d32',
    sidebarAccentForeground: '#f8f8f2',
    sidebarBorder: '#2f302a',
    sidebarRing: '#f92672',
    shellBackground: '#272822',
    scrollbarThumb: 'rgba(249, 38, 114, 0.22)',
    scrollbarThumbHover: 'rgba(249, 38, 114, 0.42)',
  },
  editor: {
    background: '#272822',
    foreground: '#f8f8f2',
    gutter: '#75715e',
    gutterActive: '#f8f8f2',
    lineActive: 'rgba(249, 38, 114, 0.06)',
    selection: 'rgba(73, 72, 62, 0.95)',
    selectionMatch: 'rgba(73, 72, 62, 0.5)',
    cursor: '#f8f8f2',
    border: 'rgba(62, 61, 50, 0.9)',
    panelBackground: '#2f302a',

    keyword: '#f92672',
    control: '#f92672',
    storage: '#66d9ef',
    string: '#e6db74',
    stringSpecial: '#ae81ff',
    number: '#ae81ff',
    bool: '#ae81ff',
    constant: '#ae81ff',
    comment: '#75715e',
    function: '#a6e22e',
    type: '#66d9ef',
    property: '#f8f8f2',
    variable: '#f8f8f2',
    variableDef: '#a6e22e',
    operator: '#f92672',
    punctuation: '#f8f8f2',
    tagName: '#f92672',
    tagBracket: '#75715e',
    attribute: '#a6e22e',
    meta: '#fd971f',
    link: '#66d9ef',
    heading: '#f92672',
    invalid: '#f92672',

    matchingBracketBg: 'rgba(249, 38, 114, 0.2)',
    searchMatchBg: 'rgba(230, 219, 116, 0.25)',
    searchMatchBorder: 'rgba(230, 219, 116, 0.7)',
    searchMatchSelectedBg: 'rgba(230, 219, 116, 0.5)',
    autocompleteSelectedBg: 'rgba(73, 72, 62, 0.9)',
    foldGutter: 'rgba(117, 113, 94, 0.7)',
    foldPlaceholderText: '#f8f8f2',
  },
}

const CARBON: ThemeDefinition = {
  id: 'carbon',
  name: 'Carbon',
  description: 'Neutral achromatic dark — pure grays with Atom One syntax colors.',
  appearance: 'dark',
  shiki: 'one-dark-pro',
  colors: {
    background: '#0a0a0a',
    foreground: '#e6e6e6',
    card: '#141414',
    cardForeground: '#e6e6e6',
    popover: '#141414',
    popoverForeground: '#e6e6e6',
    primary: '#fafafa',
    primaryForeground: '#0a0a0a',
    secondary: '#1f1f1f',
    secondaryForeground: '#e6e6e6',
    muted: '#1f1f1f',
    mutedForeground: '#999999',
    accent: '#262626',
    accentForeground: '#fafafa',
    destructive: '#dc2626',
    destructiveForeground: '#fafafa',
    success: '#16a34a',
    successForeground: '#0a0a0a',
    warning: '#fbbf24',
    warningForeground: '#0a0a0a',
    info: '#7cc8f5',
    infoForeground: '#0a0a0a',
    border: '#262626',
    input: '#262626',
    ring: '#fafafa',
    chart1: '#fafafa',
    chart2: '#d4d4d4',
    chart3: '#a3a3a3',
    chart4: '#737373',
    chart5: '#525252',
    sidebar: '#0d0d0d',
    sidebarForeground: '#e6e6e6',
    sidebarPrimary: '#fafafa',
    sidebarPrimaryForeground: '#0a0a0a',
    sidebarAccent: '#262626',
    sidebarAccentForeground: '#fafafa',
    sidebarBorder: '#1f1f1f',
    sidebarRing: '#fafafa',
    shellBackground: '#050505',
    scrollbarThumb: 'rgba(255, 255, 255, 0.12)',
    scrollbarThumbHover: 'rgba(255, 255, 255, 0.22)',
  },
  editor: {
    background: '#0a0a0a',
    foreground: '#e6e6e6',
    gutter: '#525252',
    gutterActive: '#d4d4d4',
    lineActive: 'rgba(255, 255, 255, 0.04)',
    selection: 'rgba(255, 255, 255, 0.16)',
    selectionMatch: 'rgba(255, 255, 255, 0.08)',
    cursor: '#fafafa',
    border: 'rgba(38, 38, 38, 0.9)',
    panelBackground: '#141414',

    keyword: '#c678dd',
    control: '#c678dd',
    storage: '#c678dd',
    string: '#98c379',
    stringSpecial: '#56b6c2',
    number: '#d19a66',
    bool: '#d19a66',
    constant: '#d19a66',
    comment: '#5c6370',
    function: '#61afef',
    type: '#e5c07b',
    property: '#e06c75',
    variable: '#e06c75',
    variableDef: '#e06c75',
    operator: '#56b6c2',
    punctuation: '#abb2bf',
    tagName: '#e06c75',
    tagBracket: '#abb2bf',
    attribute: '#d19a66',
    meta: '#61afef',
    link: '#61afef',
    heading: '#e06c75',
    invalid: '#be5046',

    matchingBracketBg: 'rgba(255, 255, 255, 0.12)',
    searchMatchBg: 'rgba(255, 255, 255, 0.18)',
    searchMatchBorder: 'rgba(255, 255, 255, 0.4)',
    searchMatchSelectedBg: 'rgba(255, 255, 255, 0.32)',
    autocompleteSelectedBg: 'rgba(255, 255, 255, 0.1)',
    foldGutter: 'rgba(82, 82, 82, 0.6)',
    foldPlaceholderText: '#999999',
  },
}

const GRUVBOX: ThemeDefinition = {
  id: 'gruvbox-dark',
  name: 'Gruvbox Dark',
  description: 'Warm retro — amber, rust, and forest greens on weathered brown.',
  appearance: 'dark',
  shiki: 'gruvbox-dark-medium',
  colors: {
    background: '#282828',
    foreground: '#ebdbb2',
    card: '#32302f',
    cardForeground: '#ebdbb2',
    popover: '#32302f',
    popoverForeground: '#ebdbb2',
    primary: '#fabd2f',
    primaryForeground: '#282828',
    secondary: '#3c3836',
    secondaryForeground: '#ebdbb2',
    muted: '#3c3836',
    mutedForeground: '#a89984',
    accent: '#504945',
    accentForeground: '#ebdbb2',
    destructive: '#fb4934',
    destructiveForeground: '#282828',
    success: '#b8bb26',
    successForeground: '#282828',
    warning: '#fe8019',
    warningForeground: '#282828',
    info: '#83a598',
    infoForeground: '#282828',
    border: '#3c3836',
    input: '#3c3836',
    ring: '#fabd2f',
    chart1: '#fabd2f',
    chart2: '#fe8019',
    chart3: '#b8bb26',
    chart4: '#83a598',
    chart5: '#d3869b',
    sidebar: '#1d2021',
    sidebarForeground: '#ebdbb2',
    sidebarPrimary: '#fabd2f',
    sidebarPrimaryForeground: '#282828',
    sidebarAccent: '#504945',
    sidebarAccentForeground: '#ebdbb2',
    sidebarBorder: '#3c3836',
    sidebarRing: '#fabd2f',
    shellBackground: '#1d2021',
    scrollbarThumb: 'rgba(250, 189, 47, 0.22)',
    scrollbarThumbHover: 'rgba(250, 189, 47, 0.42)',
  },
  editor: {
    background: '#282828',
    foreground: '#ebdbb2',
    gutter: '#7c6f64',
    gutterActive: '#ebdbb2',
    lineActive: 'rgba(250, 189, 47, 0.06)',
    selection: 'rgba(80, 73, 69, 0.9)',
    selectionMatch: 'rgba(80, 73, 69, 0.5)',
    cursor: '#fabd2f',
    border: 'rgba(60, 56, 54, 0.9)',
    panelBackground: '#32302f',

    keyword: '#fb4934',
    control: '#fe8019',
    storage: '#fb4934',
    string: '#b8bb26',
    stringSpecial: '#fabd2f',
    number: '#d3869b',
    bool: '#d3869b',
    constant: '#d3869b',
    comment: '#928374',
    function: '#b8bb26',
    type: '#fabd2f',
    property: '#83a598',
    variable: '#ebdbb2',
    variableDef: '#ebdbb2',
    operator: '#fe8019',
    punctuation: '#ebdbb2',
    tagName: '#fb4934',
    tagBracket: '#928374',
    attribute: '#b8bb26',
    meta: '#d3869b',
    link: '#83a598',
    heading: '#fabd2f',
    invalid: '#fb4934',

    matchingBracketBg: 'rgba(250, 189, 47, 0.18)',
    searchMatchBg: 'rgba(250, 189, 47, 0.3)',
    searchMatchBorder: 'rgba(250, 189, 47, 0.7)',
    searchMatchSelectedBg: 'rgba(250, 189, 47, 0.5)',
    autocompleteSelectedBg: 'rgba(80, 73, 69, 0.6)',
    foldGutter: 'rgba(124, 111, 100, 0.7)',
    foldPlaceholderText: '#a89984',
  },
}

const SOLARIZED_LIGHT: ThemeDefinition = {
  id: 'solarized-light',
  name: 'Solarized Light',
  description: 'Warm ivory base with soft yellow accents — the classic light pair.',
  appearance: 'light',
  shiki: 'solarized-light',
  colors: {
    background: '#fdf6e3',
    foreground: '#586e75',
    card: '#fdf6e3',
    cardForeground: '#586e75',
    popover: '#fdf6e3',
    popoverForeground: '#586e75',
    primary: '#b58900',
    primaryForeground: '#fdf6e3',
    secondary: '#eee8d5',
    secondaryForeground: '#586e75',
    muted: '#eee8d5',
    mutedForeground: '#657b83',
    accent: '#eee8d5',
    accentForeground: '#073642',
    destructive: '#dc322f',
    destructiveForeground: '#fdf6e3',
    success: '#859900',
    successForeground: '#fdf6e3',
    warning: '#cb4b16',
    warningForeground: '#fdf6e3',
    info: '#268bd2',
    infoForeground: '#fdf6e3',
    border: '#d8d0b3',
    input: '#d8d0b3',
    ring: '#b58900',
    chart1: '#b58900',
    chart2: '#268bd2',
    chart3: '#2aa198',
    chart4: '#859900',
    chart5: '#cb4b16',
    sidebar: '#eee8d5',
    sidebarForeground: '#586e75',
    sidebarPrimary: '#b58900',
    sidebarPrimaryForeground: '#fdf6e3',
    sidebarAccent: '#fdf6e3',
    sidebarAccentForeground: '#073642',
    sidebarBorder: '#cdc4a4',
    sidebarRing: '#b58900',
    shellBackground: '#fdf6e3',
    scrollbarThumb: 'rgba(88, 110, 117, 0.25)',
    scrollbarThumbHover: 'rgba(88, 110, 117, 0.45)',
  },
  editor: {
    background: '#fdf6e3',
    foreground: '#586e75',
    gutter: '#93a1a1',
    gutterActive: '#586e75',
    lineActive: 'rgba(181, 137, 0, 0.06)',
    selection: 'rgba(238, 232, 213, 0.9)',
    selectionMatch: 'rgba(147, 161, 161, 0.35)',
    cursor: '#b58900',
    border: 'rgba(238, 232, 213, 0.9)',
    panelBackground: '#eee8d5',

    keyword: '#859900',
    control: '#cb4b16',
    storage: '#859900',
    string: '#2aa198',
    stringSpecial: '#6c71c4',
    number: '#d33682',
    bool: '#d33682',
    constant: '#d33682',
    comment: '#93a1a1',
    function: '#268bd2',
    type: '#b58900',
    property: '#586e75',
    variable: '#586e75',
    variableDef: '#073642',
    operator: '#859900',
    punctuation: '#657b83',
    tagName: '#268bd2',
    tagBracket: '#93a1a1',
    attribute: '#268bd2',
    meta: '#6c71c4',
    link: '#268bd2',
    heading: '#b58900',
    invalid: '#dc322f',

    matchingBracketBg: 'rgba(181, 137, 0, 0.18)',
    searchMatchBg: 'rgba(181, 137, 0, 0.3)',
    searchMatchBorder: 'rgba(181, 137, 0, 0.6)',
    searchMatchSelectedBg: 'rgba(181, 137, 0, 0.5)',
    autocompleteSelectedBg: 'rgba(38, 139, 210, 0.18)',
    foldGutter: 'rgba(147, 161, 161, 0.6)',
    foldPlaceholderText: '#657b83',
  },
}

const TOKYO_NIGHT: ThemeDefinition = {
  id: 'tokyo-night',
  name: 'Tokyo Night',
  description: 'Deep navy with electric blue and lavender — neon city after dark.',
  appearance: 'dark',
  shiki: 'tokyo-night',
  colors: {
    background: '#1a1b26',
    foreground: '#c0caf5',
    card: '#24283b',
    cardForeground: '#c0caf5',
    popover: '#24283b',
    popoverForeground: '#c0caf5',
    primary: '#7aa2f7',
    primaryForeground: '#1a1b26',
    secondary: '#2f334d',
    secondaryForeground: '#c0caf5',
    muted: '#292e42',
    mutedForeground: '#a9b1d6',
    accent: '#414868',
    accentForeground: '#c0caf5',
    destructive: '#f7768e',
    destructiveForeground: '#1a1b26',
    success: '#9ece6a',
    successForeground: '#1a1b26',
    warning: '#e0af68',
    warningForeground: '#1a1b26',
    info: '#7dcfff',
    infoForeground: '#1a1b26',
    border: '#2f334d',
    input: '#2f334d',
    ring: '#7aa2f7',
    chart1: '#7aa2f7',
    chart2: '#bb9af7',
    chart3: '#7dcfff',
    chart4: '#9ece6a',
    chart5: '#e0af68',
    sidebar: '#16161e',
    sidebarForeground: '#c0caf5',
    sidebarPrimary: '#7aa2f7',
    sidebarPrimaryForeground: '#1a1b26',
    sidebarAccent: '#414868',
    sidebarAccentForeground: '#c0caf5',
    sidebarBorder: '#2f334d',
    sidebarRing: '#7aa2f7',
    shellBackground: '#16161e',
    scrollbarThumb: 'rgba(122, 162, 247, 0.22)',
    scrollbarThumbHover: 'rgba(122, 162, 247, 0.42)',
  },
  editor: {
    background: '#1a1b26',
    foreground: '#c0caf5',
    gutter: '#3b4261',
    gutterActive: '#c0caf5',
    lineActive: 'rgba(122, 162, 247, 0.06)',
    selection: 'rgba(122, 162, 247, 0.22)',
    selectionMatch: 'rgba(122, 162, 247, 0.12)',
    cursor: '#c0caf5',
    border: 'rgba(47, 51, 77, 0.9)',
    panelBackground: '#24283b',

    keyword: '#bb9af7',
    control: '#bb9af7',
    storage: '#bb9af7',
    string: '#9ece6a',
    stringSpecial: '#b4f9f8',
    number: '#ff9e64',
    bool: '#ff9e64',
    constant: '#ff9e64',
    comment: '#565f89',
    function: '#7aa2f7',
    type: '#2ac3de',
    property: '#73daca',
    variable: '#c0caf5',
    variableDef: '#c0caf5',
    operator: '#89ddff',
    punctuation: '#a9b1d6',
    tagName: '#f7768e',
    tagBracket: '#565f89',
    attribute: '#bb9af7',
    meta: '#7dcfff',
    link: '#7aa2f7',
    heading: '#bb9af7',
    invalid: '#f7768e',

    matchingBracketBg: 'rgba(122, 162, 247, 0.18)',
    searchMatchBg: 'rgba(224, 175, 104, 0.28)',
    searchMatchBorder: 'rgba(224, 175, 104, 0.65)',
    searchMatchSelectedBg: 'rgba(224, 175, 104, 0.5)',
    autocompleteSelectedBg: 'rgba(65, 72, 104, 0.6)',
    foldGutter: 'rgba(86, 95, 137, 0.7)',
    foldPlaceholderText: '#a9b1d6',
  },
}

const CATPPUCCIN_MOCHA: ThemeDefinition = {
  id: 'catppuccin-mocha',
  name: 'Catppuccin Mocha',
  description: 'Pastel mauve, peach, and rosewater on warm cocoa.',
  appearance: 'dark',
  shiki: 'catppuccin-mocha',
  colors: {
    background: '#1e1e2e',
    foreground: '#cdd6f4',
    card: '#181825',
    cardForeground: '#cdd6f4',
    popover: '#181825',
    popoverForeground: '#cdd6f4',
    primary: '#cba6f7',
    primaryForeground: '#1e1e2e',
    secondary: '#313244',
    secondaryForeground: '#cdd6f4',
    muted: '#313244',
    mutedForeground: '#a6adc8',
    accent: '#45475a',
    accentForeground: '#cdd6f4',
    destructive: '#f38ba8',
    destructiveForeground: '#1e1e2e',
    success: '#a6e3a1',
    successForeground: '#1e1e2e',
    warning: '#fab387',
    warningForeground: '#1e1e2e',
    info: '#89b4fa',
    infoForeground: '#1e1e2e',
    border: '#45475a',
    input: '#45475a',
    ring: '#cba6f7',
    chart1: '#cba6f7',
    chart2: '#f5c2e7',
    chart3: '#89b4fa',
    chart4: '#a6e3a1',
    chart5: '#fab387',
    sidebar: '#11111b',
    sidebarForeground: '#cdd6f4',
    sidebarPrimary: '#cba6f7',
    sidebarPrimaryForeground: '#1e1e2e',
    sidebarAccent: '#45475a',
    sidebarAccentForeground: '#cdd6f4',
    sidebarBorder: '#313244',
    sidebarRing: '#cba6f7',
    shellBackground: '#11111b',
    scrollbarThumb: 'rgba(203, 166, 247, 0.22)',
    scrollbarThumbHover: 'rgba(203, 166, 247, 0.42)',
  },
  editor: {
    background: '#1e1e2e',
    foreground: '#cdd6f4',
    gutter: '#6c7086',
    gutterActive: '#cdd6f4',
    lineActive: 'rgba(203, 166, 247, 0.06)',
    selection: 'rgba(203, 166, 247, 0.25)',
    selectionMatch: 'rgba(203, 166, 247, 0.13)',
    cursor: '#f5e0dc',
    border: 'rgba(69, 71, 90, 0.9)',
    panelBackground: '#181825',

    keyword: '#cba6f7',
    control: '#cba6f7',
    storage: '#f38ba8',
    string: '#a6e3a1',
    stringSpecial: '#94e2d5',
    number: '#fab387',
    bool: '#fab387',
    constant: '#fab387',
    comment: '#6c7086',
    function: '#89b4fa',
    type: '#f9e2af',
    property: '#b4befe',
    variable: '#cdd6f4',
    variableDef: '#f5e0dc',
    operator: '#94e2d5',
    punctuation: '#bac2de',
    tagName: '#f38ba8',
    tagBracket: '#6c7086',
    attribute: '#fab387',
    meta: '#f5c2e7',
    link: '#89b4fa',
    heading: '#cba6f7',
    invalid: '#f38ba8',

    matchingBracketBg: 'rgba(203, 166, 247, 0.2)',
    searchMatchBg: 'rgba(249, 226, 175, 0.25)',
    searchMatchBorder: 'rgba(249, 226, 175, 0.65)',
    searchMatchSelectedBg: 'rgba(249, 226, 175, 0.5)',
    autocompleteSelectedBg: 'rgba(69, 71, 90, 0.7)',
    foldGutter: 'rgba(108, 112, 134, 0.7)',
    foldPlaceholderText: '#a6adc8',
  },
}

const ROSE_PINE_DAWN: ThemeDefinition = {
  id: 'rose-pine-dawn',
  name: 'Rosé Pine Dawn',
  description: 'Soft rose and gold on warm ivory — sunlit and gentle.',
  appearance: 'light',
  shiki: 'rose-pine-dawn',
  colors: {
    background: '#faf4ed',
    foreground: '#575279',
    card: '#fffaf3',
    cardForeground: '#575279',
    popover: '#fffaf3',
    popoverForeground: '#575279',
    primary: '#d7827e',
    primaryForeground: '#faf4ed',
    secondary: '#f2e9e1',
    secondaryForeground: '#575279',
    muted: '#f2e9e1',
    mutedForeground: '#797593',
    accent: '#dfdad9',
    accentForeground: '#575279',
    destructive: '#b4637a',
    destructiveForeground: '#faf4ed',
    success: '#56949f',
    successForeground: '#faf4ed',
    warning: '#ea9d34',
    warningForeground: '#faf4ed',
    info: '#286983',
    infoForeground: '#faf4ed',
    border: '#cecacd',
    input: '#cecacd',
    ring: '#d7827e',
    chart1: '#d7827e',
    chart2: '#ea9d34',
    chart3: '#56949f',
    chart4: '#286983',
    chart5: '#907aa9',
    sidebar: '#f2e9e1',
    sidebarForeground: '#575279',
    sidebarPrimary: '#d7827e',
    sidebarPrimaryForeground: '#faf4ed',
    sidebarAccent: '#fffaf3',
    sidebarAccentForeground: '#575279',
    sidebarBorder: '#cecacd',
    sidebarRing: '#d7827e',
    shellBackground: '#faf4ed',
    scrollbarThumb: 'rgba(152, 147, 165, 0.35)',
    scrollbarThumbHover: 'rgba(121, 117, 147, 0.55)',
  },
  editor: {
    background: '#faf4ed',
    foreground: '#575279',
    gutter: '#9893a5',
    gutterActive: '#575279',
    lineActive: 'rgba(215, 130, 126, 0.06)',
    selection: 'rgba(215, 130, 126, 0.18)',
    selectionMatch: 'rgba(215, 130, 126, 0.1)',
    cursor: '#d7827e',
    border: 'rgba(206, 202, 205, 0.9)',
    panelBackground: '#fffaf3',

    keyword: '#286983',
    control: '#b4637a',
    storage: '#286983',
    string: '#ea9d34',
    stringSpecial: '#56949f',
    number: '#d7827e',
    bool: '#d7827e',
    constant: '#d7827e',
    comment: '#9893a5',
    function: '#56949f',
    type: '#907aa9',
    property: '#575279',
    variable: '#575279',
    variableDef: '#1f1d2e',
    operator: '#b4637a',
    punctuation: '#797593',
    tagName: '#b4637a',
    tagBracket: '#9893a5',
    attribute: '#ea9d34',
    meta: '#907aa9',
    link: '#286983',
    heading: '#d7827e',
    invalid: '#b4637a',

    matchingBracketBg: 'rgba(215, 130, 126, 0.2)',
    searchMatchBg: 'rgba(234, 157, 52, 0.3)',
    searchMatchBorder: 'rgba(234, 157, 52, 0.6)',
    searchMatchSelectedBg: 'rgba(234, 157, 52, 0.5)',
    autocompleteSelectedBg: 'rgba(40, 105, 131, 0.18)',
    foldGutter: 'rgba(152, 147, 165, 0.6)',
    foldPlaceholderText: '#797593',
  },
}

const SYNTHWAVE_84: ThemeDefinition = {
  id: 'synthwave-84',
  name: "Synthwave '84",
  description: 'Neon hot pink and electric cyan on deep violet — retro CRT glow.',
  appearance: 'dark',
  shiki: 'synthwave-84',
  colors: {
    background: '#2a2139',
    foreground: '#f8f8f2',
    card: '#34294f',
    cardForeground: '#f8f8f2',
    popover: '#34294f',
    popoverForeground: '#f8f8f2',
    primary: '#ff7edb',
    primaryForeground: '#241b2f',
    secondary: '#34294f',
    secondaryForeground: '#f8f8f2',
    muted: '#34294f',
    mutedForeground: '#b6b1cd',
    accent: '#495495',
    accentForeground: '#f8f8f2',
    destructive: '#fe4450',
    destructiveForeground: '#241b2f',
    success: '#72f1b8',
    successForeground: '#241b2f',
    warning: '#fede5d',
    warningForeground: '#241b2f',
    info: '#03edf9',
    infoForeground: '#241b2f',
    border: '#495495',
    input: '#495495',
    ring: '#ff7edb',
    chart1: '#ff7edb',
    chart2: '#03edf9',
    chart3: '#fede5d',
    chart4: '#72f1b8',
    chart5: '#f97e72',
    sidebar: '#241b2f',
    sidebarForeground: '#f8f8f2',
    sidebarPrimary: '#ff7edb',
    sidebarPrimaryForeground: '#241b2f',
    sidebarAccent: '#495495',
    sidebarAccentForeground: '#f8f8f2',
    sidebarBorder: '#34294f',
    sidebarRing: '#ff7edb',
    shellBackground: '#241b2f',
    scrollbarThumb: 'rgba(255, 126, 219, 0.22)',
    scrollbarThumbHover: 'rgba(255, 126, 219, 0.42)',
  },
  editor: {
    background: '#2a2139',
    foreground: '#f8f8f2',
    gutter: '#495495',
    gutterActive: '#f8f8f2',
    lineActive: 'rgba(255, 126, 219, 0.06)',
    selection: 'rgba(255, 126, 219, 0.22)',
    selectionMatch: 'rgba(255, 126, 219, 0.12)',
    cursor: '#ff7edb',
    border: 'rgba(73, 84, 149, 0.9)',
    panelBackground: '#34294f',

    keyword: '#fede5d',
    control: '#fe4450',
    storage: '#fede5d',
    string: '#ff8b39',
    stringSpecial: '#b893ce',
    number: '#f97e72',
    bool: '#f97e72',
    constant: '#f97e72',
    comment: '#495495',
    function: '#36f9f6',
    type: '#fede5d',
    property: '#ff7edb',
    variable: '#f8f8f2',
    variableDef: '#f8f8f2',
    operator: '#f97e72',
    punctuation: '#f8f8f2',
    tagName: '#fede5d',
    tagBracket: '#495495',
    attribute: '#ff7edb',
    meta: '#03edf9',
    link: '#36f9f6',
    heading: '#ff7edb',
    invalid: '#fe4450',

    matchingBracketBg: 'rgba(255, 126, 219, 0.2)',
    searchMatchBg: 'rgba(254, 222, 93, 0.28)',
    searchMatchBorder: 'rgba(254, 222, 93, 0.65)',
    searchMatchSelectedBg: 'rgba(254, 222, 93, 0.5)',
    autocompleteSelectedBg: 'rgba(73, 84, 149, 0.6)',
    foldGutter: 'rgba(73, 84, 149, 0.7)',
    foldPlaceholderText: '#b6b1cd',
  },
}

const KANAGAWA_WAVE: ThemeDefinition = {
  id: 'kanagawa-wave',
  name: 'Kanagawa Wave',
  description: 'Muted crystal blue and autumn ochre on sumi ink — Hokusai stillness.',
  appearance: 'dark',
  shiki: 'kanagawa-wave',
  colors: {
    background: '#1f1f28',
    foreground: '#dcd7ba',
    card: '#2a2a37',
    cardForeground: '#dcd7ba',
    popover: '#2a2a37',
    popoverForeground: '#dcd7ba',
    primary: '#7e9cd8',
    primaryForeground: '#1f1f28',
    secondary: '#363646',
    secondaryForeground: '#dcd7ba',
    muted: '#363646',
    mutedForeground: '#a6a69c',
    accent: '#54546d',
    accentForeground: '#dcd7ba',
    destructive: '#c34043',
    destructiveForeground: '#dcd7ba',
    success: '#98bb6c',
    successForeground: '#1f1f28',
    warning: '#e6c384',
    warningForeground: '#1f1f28',
    info: '#7fb4ca',
    infoForeground: '#1f1f28',
    border: '#54546d',
    input: '#54546d',
    ring: '#7e9cd8',
    chart1: '#7e9cd8',
    chart2: '#957fb8',
    chart3: '#7aa89f',
    chart4: '#e6c384',
    chart5: '#d27e99',
    sidebar: '#181820',
    sidebarForeground: '#dcd7ba',
    sidebarPrimary: '#7e9cd8',
    sidebarPrimaryForeground: '#1f1f28',
    sidebarAccent: '#54546d',
    sidebarAccentForeground: '#dcd7ba',
    sidebarBorder: '#363646',
    sidebarRing: '#7e9cd8',
    shellBackground: '#181820',
    scrollbarThumb: 'rgba(126, 156, 216, 0.22)',
    scrollbarThumbHover: 'rgba(126, 156, 216, 0.42)',
  },
  editor: {
    background: '#1f1f28',
    foreground: '#dcd7ba',
    gutter: '#54546d',
    gutterActive: '#dcd7ba',
    lineActive: 'rgba(126, 156, 216, 0.06)',
    selection: 'rgba(126, 156, 216, 0.22)',
    selectionMatch: 'rgba(126, 156, 216, 0.12)',
    cursor: '#c8c093',
    border: 'rgba(84, 84, 109, 0.9)',
    panelBackground: '#2a2a37',

    keyword: '#957fb8',
    control: '#957fb8',
    storage: '#957fb8',
    string: '#98bb6c',
    stringSpecial: '#e6c384',
    number: '#d27e99',
    bool: '#d27e99',
    constant: '#d27e99',
    comment: '#727169',
    function: '#7e9cd8',
    type: '#7aa89f',
    property: '#c0a36e',
    variable: '#dcd7ba',
    variableDef: '#c8c093',
    operator: '#c0a36e',
    punctuation: '#9cabca',
    tagName: '#c34043',
    tagBracket: '#727169',
    attribute: '#c0a36e',
    meta: '#ffa066',
    link: '#7fb4ca',
    heading: '#7e9cd8',
    invalid: '#c34043',

    matchingBracketBg: 'rgba(126, 156, 216, 0.2)',
    searchMatchBg: 'rgba(230, 195, 132, 0.28)',
    searchMatchBorder: 'rgba(230, 195, 132, 0.65)',
    searchMatchSelectedBg: 'rgba(230, 195, 132, 0.5)',
    autocompleteSelectedBg: 'rgba(84, 84, 109, 0.7)',
    foldGutter: 'rgba(114, 113, 105, 0.7)',
    foldPlaceholderText: '#a6a69c',
  },
}

const AYU_MIRAGE: ThemeDefinition = {
  id: 'ayu-mirage',
  name: 'Ayu Mirage',
  description: 'Soft amber and aqua on dusky teal — easy on the eyes at any hour.',
  appearance: 'dark',
  shiki: 'ayu-mirage',
  colors: {
    background: '#1f2430',
    foreground: '#cbccc6',
    card: '#232834',
    cardForeground: '#cbccc6',
    popover: '#232834',
    popoverForeground: '#cbccc6',
    primary: '#ffcc66',
    primaryForeground: '#1f2430',
    secondary: '#2c313a',
    secondaryForeground: '#cbccc6',
    muted: '#2c313a',
    mutedForeground: '#8a9199',
    accent: '#34455a',
    accentForeground: '#cbccc6',
    destructive: '#ff3333',
    destructiveForeground: '#1f2430',
    success: '#bae67e',
    successForeground: '#1f2430',
    warning: '#ffa759',
    warningForeground: '#1f2430',
    info: '#5ccfe6',
    infoForeground: '#1f2430',
    border: '#34455a',
    input: '#34455a',
    ring: '#ffcc66',
    chart1: '#ffcc66',
    chart2: '#ffa759',
    chart3: '#5ccfe6',
    chart4: '#bae67e',
    chart5: '#d4bfff',
    sidebar: '#191e2a',
    sidebarForeground: '#cbccc6',
    sidebarPrimary: '#ffcc66',
    sidebarPrimaryForeground: '#1f2430',
    sidebarAccent: '#34455a',
    sidebarAccentForeground: '#cbccc6',
    sidebarBorder: '#2c313a',
    sidebarRing: '#ffcc66',
    shellBackground: '#191e2a',
    scrollbarThumb: 'rgba(255, 204, 102, 0.22)',
    scrollbarThumbHover: 'rgba(255, 204, 102, 0.42)',
  },
  editor: {
    background: '#1f2430',
    foreground: '#cbccc6',
    gutter: '#5c6773',
    gutterActive: '#cbccc6',
    lineActive: 'rgba(255, 204, 102, 0.06)',
    selection: 'rgba(255, 204, 102, 0.22)',
    selectionMatch: 'rgba(255, 204, 102, 0.12)',
    cursor: '#ffcc66',
    border: 'rgba(52, 69, 90, 0.9)',
    panelBackground: '#232834',

    keyword: '#ffa759',
    control: '#ffa759',
    storage: '#ffa759',
    string: '#bae67e',
    stringSpecial: '#95e6cb',
    number: '#d4bfff',
    bool: '#d4bfff',
    constant: '#d4bfff',
    comment: '#5c6773',
    function: '#ffd580',
    type: '#73d0ff',
    property: '#f29e74',
    variable: '#cbccc6',
    variableDef: '#cbccc6',
    operator: '#f29e74',
    punctuation: '#cbccc6',
    tagName: '#5ccfe6',
    tagBracket: '#5c6773',
    attribute: '#ffd580',
    meta: '#f28779',
    link: '#5ccfe6',
    heading: '#ffcc66',
    invalid: '#ff3333',

    matchingBracketBg: 'rgba(255, 204, 102, 0.2)',
    searchMatchBg: 'rgba(186, 230, 126, 0.28)',
    searchMatchBorder: 'rgba(186, 230, 126, 0.65)',
    searchMatchSelectedBg: 'rgba(186, 230, 126, 0.5)',
    autocompleteSelectedBg: 'rgba(52, 69, 90, 0.7)',
    foldGutter: 'rgba(92, 103, 115, 0.7)',
    foldPlaceholderText: '#8a9199',
  },
}

export const THEMES: ThemeDefinition[] = [
  DUSK,
  CARBON,
  MIDNIGHT,
  TOKYO_NIGHT,
  CATPPUCCIN_MOCHA,
  KANAGAWA_WAVE,
  AYU_MIRAGE,
  NORD,
  DRACULA,
  SOLARIZED_DARK,
  MONOKAI,
  GRUVBOX,
  SYNTHWAVE_84,
  DAYLIGHT,
  SOLARIZED_LIGHT,
  ROSE_PINE_DAWN,
]

export const DEFAULT_THEME_ID = DUSK.id

/**
 * Look up a theme by id within an `available` pool. Defaults to the built-in
 * registry, but the provider passes `[...THEMES, ...customThemes]` so custom
 * user-defined themes resolve too. Falls back to the first built-in theme
 * when nothing matches.
 */
export function getThemeById(
  id: string | null | undefined,
  available: ThemeDefinition[] = THEMES,
): ThemeDefinition {
  if (id) {
    const match = available.find((theme) => theme.id === id)
    if (match) return match
  }
  return THEMES[0]
}

export function themeClassName(id: string): string {
  return `theme-${id}`
}

export const THEME_STORAGE_KEY = 'xero:theme'
export const CUSTOM_THEMES_STORAGE_KEY = 'xero:custom-themes'
export const CUSTOM_THEME_ID_PREFIX = 'custom-'

export function isCustomThemeId(id: string): boolean {
  return id.startsWith(CUSTOM_THEME_ID_PREFIX)
}

/**
 * Map of `ThemeColors` keys to the CSS custom property names rendered in
 * `styles.css`. Used by the provider to push a custom palette directly onto
 * `<html>` as inline style — bypassing the need for a per-theme class entry
 * in the stylesheet, so users can author themes at runtime.
 */
const COLOR_CSS_VAR_MAP: Array<[keyof ThemeColors, string]> = [
  ['background', '--background'],
  ['foreground', '--foreground'],
  ['card', '--card'],
  ['cardForeground', '--card-foreground'],
  ['popover', '--popover'],
  ['popoverForeground', '--popover-foreground'],
  ['primary', '--primary'],
  ['primaryForeground', '--primary-foreground'],
  ['secondary', '--secondary'],
  ['secondaryForeground', '--secondary-foreground'],
  ['muted', '--muted'],
  ['mutedForeground', '--muted-foreground'],
  ['accent', '--accent'],
  ['accentForeground', '--accent-foreground'],
  ['destructive', '--destructive'],
  ['destructiveForeground', '--destructive-foreground'],
  ['success', '--success'],
  ['successForeground', '--success-foreground'],
  ['warning', '--warning'],
  ['warningForeground', '--warning-foreground'],
  ['info', '--info'],
  ['infoForeground', '--info-foreground'],
  ['border', '--border'],
  ['input', '--input'],
  ['ring', '--ring'],
  ['chart1', '--chart-1'],
  ['chart2', '--chart-2'],
  ['chart3', '--chart-3'],
  ['chart4', '--chart-4'],
  ['chart5', '--chart-5'],
  ['sidebar', '--sidebar'],
  ['sidebarForeground', '--sidebar-foreground'],
  ['sidebarPrimary', '--sidebar-primary'],
  ['sidebarPrimaryForeground', '--sidebar-primary-foreground'],
  ['sidebarAccent', '--sidebar-accent'],
  ['sidebarAccentForeground', '--sidebar-accent-foreground'],
  ['sidebarBorder', '--sidebar-border'],
  ['sidebarRing', '--sidebar-ring'],
  ['shellBackground', '--shell-background'],
  ['scrollbarThumb', '--scrollbar-thumb'],
  ['scrollbarThumbHover', '--scrollbar-thumb-hover'],
]

export function themeColorsToCSSVars(colors: ThemeColors): Array<[string, string]> {
  return COLOR_CSS_VAR_MAP.map(([key, cssVar]) => [cssVar, colors[key]])
}

export interface ApplyThemeToDocumentOptions {
  /**
   * Built-in themes should generally rely on their shared stylesheet classes.
   * Custom themes need inline tokens because they are authored at runtime.
   */
  inlineColors?: boolean
}

export function applyThemeToDocument(
  theme: ThemeDefinition,
  options: ApplyThemeToDocumentOptions = {},
): void {
  if (typeof document === 'undefined') return
  const { inlineColors = true } = options
  const root = document.documentElement

  for (const className of Array.from(root.classList)) {
    if (className.startsWith('theme-')) {
      root.classList.remove(className)
    }
  }
  root.classList.add(themeClassName(theme.id))

  root.classList.remove('dark', 'light')
  root.classList.add(theme.appearance)
  root.style.colorScheme = theme.appearance
  root.dataset.theme = theme.id

  for (const [cssVar, value] of themeColorsToCSSVars(theme.colors)) {
    if (inlineColors) {
      root.style.setProperty(cssVar, value)
    } else {
      root.style.removeProperty(cssVar)
    }
  }
}

/**
 * The 9 user-editable color slots surfaced in the advanced theme editor. We
 * intentionally hide the long tail (chart palette, sidebar variants, scrollbar
 * tints) — those are derived from the visible inputs by `expandCustomColors`
 * to keep the editor approachable.
 */
export const EDITABLE_COLOR_KEYS = [
  'background',
  'foreground',
  'card',
  'primary',
  'secondary',
  'accent',
  'border',
  'muted',
  'mutedForeground',
] as const satisfies readonly (keyof ThemeColors)[]

export type EditableColorKey = (typeof EDITABLE_COLOR_KEYS)[number]

/**
 * Take the 9 user-edited colors plus a base preset and produce a complete
 * `ThemeColors` object. The base provides accent semantics (success/warning/
 * info/destructive, chart palette) while the user's primary, background, and
 * surface choices flow through to sidebar + scrollbar so the whole UI tracks.
 */
export function expandCustomColors(
  edits: Record<EditableColorKey, string>,
  base: ThemeColors,
): ThemeColors {
  const primaryFg = base.primaryForeground
  return {
    ...base,
    background: edits.background,
    foreground: edits.foreground,
    card: edits.card,
    cardForeground: edits.foreground,
    popover: edits.card,
    popoverForeground: edits.foreground,
    primary: edits.primary,
    primaryForeground: primaryFg,
    secondary: edits.secondary,
    secondaryForeground: edits.foreground,
    muted: edits.muted,
    mutedForeground: edits.mutedForeground,
    accent: edits.accent,
    accentForeground: edits.foreground,
    border: edits.border,
    input: edits.border,
    ring: edits.primary,
    sidebar: edits.card,
    sidebarForeground: edits.foreground,
    sidebarPrimary: edits.primary,
    sidebarPrimaryForeground: primaryFg,
    sidebarAccent: edits.accent,
    sidebarAccentForeground: edits.foreground,
    sidebarBorder: edits.border,
    sidebarRing: edits.primary,
    shellBackground: edits.background,
  }
}

/**
 * Editor palette for a custom theme: clone the base preset's syntax colors
 * (so highlighting stays coherent) but retarget the surface colors to the
 * user's own palette so the editor blends with the rest of the app.
 */
export function deriveCustomEditorPalette(
  base: EditorPalette,
  colors: ThemeColors,
): EditorPalette {
  return {
    ...base,
    background: colors.background,
    foreground: colors.foreground,
    panelBackground: colors.card,
    cursor: colors.primary,
    border: colors.border,
  }
}

export const EDITABLE_COLOR_LABELS: Record<EditableColorKey, string> = {
  background: 'Background',
  foreground: 'Foreground',
  card: 'Card / Surface',
  primary: 'Primary',
  secondary: 'Secondary',
  accent: 'Accent',
  border: 'Border',
  muted: 'Muted',
  mutedForeground: 'Muted Text',
}

export function pickEditableColors(colors: ThemeColors): Record<EditableColorKey, string> {
  return EDITABLE_COLOR_KEYS.reduce(
    (acc, key) => {
      acc[key] = colors[key]
      return acc
    },
    {} as Record<EditableColorKey, string>,
  )
}

/**
 * Restrict a hex string to the 6-digit `#rrggbb` form supported by the native
 * `<input type="color">`. Falls back to the provided default when the input is
 * unparseable. Accepts `#rgb`, `#rrggbb`, or `#rrggbbaa` (alpha is dropped).
 */
export function normalizeHexColor(value: string, fallback: string): string {
  const trimmed = value.trim().toLowerCase()
  if (/^#[0-9a-f]{6}$/.test(trimmed)) return trimmed
  if (/^#[0-9a-f]{8}$/.test(trimmed)) return trimmed.slice(0, 7)
  if (/^#[0-9a-f]{3}$/.test(trimmed)) {
    const [, r, g, b] = trimmed
    return `#${r}${r}${g}${g}${b}${b}`
  }
  return fallback
}
