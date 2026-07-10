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
