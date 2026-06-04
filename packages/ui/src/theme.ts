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
  primaryBadge: string
  primaryBadgeForeground: string
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
    primaryBadge: '#b98755',
    primaryBadgeForeground: '#0a0e12',
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
    primaryBadge: '#2f80d8',
    primaryBadgeForeground: '#ffffff',
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
    primaryBadge: '#0753ad',
    primaryBadgeForeground: '#ffffff',
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
    primaryBadge: '#c7c7c7',
    primaryBadgeForeground: '#0a0a0a',
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
    primaryBadge: '#5f83d6',
    primaryBadgeForeground: '#ffffff',
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

export const THEMES: ThemeDefinition[] = [
  DUSK,
  CARBON,
  MIDNIGHT,
  TOKYO_NIGHT,
  DAYLIGHT,
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
  ['primaryBadge', '--primary-badge'],
  ['primaryBadgeForeground', '--primary-badge-foreground'],
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

function parseHexColor(value: string): [number, number, number] | null {
  const normalized = normalizeHexColor(value, '')
  if (!/^#[0-9a-f]{6}$/.test(normalized)) return null

  return [
    Number.parseInt(normalized.slice(1, 3), 16),
    Number.parseInt(normalized.slice(3, 5), 16),
    Number.parseInt(normalized.slice(5, 7), 16),
  ]
}

function toHexChannel(value: number): string {
  return Math.min(255, Math.max(0, Math.round(value))).toString(16).padStart(2, '0')
}

function mixHexColor(
  sourceColor: string,
  targetColor: string,
  targetWeight: number,
  fallback: string,
): string {
  const source = parseHexColor(sourceColor)
  const target = parseHexColor(targetColor)
  if (!source || !target) return fallback

  const clampedTargetWeight = Math.min(1, Math.max(0, targetWeight))
  const sourceWeight = 1 - clampedTargetWeight
  const channels = source.map((channel, index) =>
    toHexChannel(channel * sourceWeight + target[index] * clampedTargetWeight),
  )

  return `#${channels.join('')}`
}

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
  const primaryBadge = mixHexColor(edits.primary, '#000000', 0.22, base.primaryBadge)
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
    primaryBadge,
    primaryBadgeForeground: base.primaryBadgeForeground,
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
