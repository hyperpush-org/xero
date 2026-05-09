// Canvas-palette kinds. The visible palette toolbar was removed in favor of
// drag-from-handle node creation; the kind union is kept here because both
// makeEditingNode and the drag-end node factory need a stable list of node
// kinds the canvas can emit.
export const CANVAS_PALETTE_KINDS = [
  'prompt',
  'tool',
  'output-section',
  'db-table',
  'consumed-artifact',
] as const
export type CanvasPaletteKind = (typeof CANVAS_PALETTE_KINDS)[number]
