/**
 * Theme registry for the Xero desktop shell.
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
    mutedForeground: '#7b8597',
    accent: '#4c566a',
    accentForeground: '#eceff4',
    destructive: '#bf616a',
    destructiveForeground: '#eceff4',
    success: '#a3be8c',
    successForeground: '#2e3440',
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
    mutedForeground: '#657b83',
    accent: '#586e75',
    accentForeground: '#eee8d5',
    destructive: '#dc322f',
    destructiveForeground: '#eee8d5',
    success: '#859900',
    successForeground: '#002b36',
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

export const THEMES: ThemeDefinition[] = [
  DUSK,
  MIDNIGHT,
  NORD,
  DRACULA,
  SOLARIZED_DARK,
  MONOKAI,
  DAYLIGHT,
]

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

export const THEME_STORAGE_KEY = 'xero:theme'
