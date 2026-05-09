import type { ThemeDefinition } from './theme-definitions'

const DOCK_ICON_SIZE = 512
const DOCK_ICON_TILE_INSET = 30
const DOCK_ICON_TILE_RADIUS = 112
const LOGO_VIEWBOX_SIZE = 455
const LOGO_SCALE = 0.5

const LOGO_PATHS = {
  primaryBottomRight:
    'M256.391 256.395H454.326V404.33C454.326 431.944 431.941 454.33 404.326 454.33H256.391V256.395Z',
  primaryTopLeft:
    'M197.936 197.941L0.000289917 197.941L0.000276984 50.0064C0.00027457 22.3921 22.386 0.00637826 50.0003 0.00637585L197.936 0.00636292L197.936 197.941Z',
  mutedBottomLeft:
    'M0 256.395H197.935V454.33H50.0001C22.3858 454.33 0 431.944 0 404.33L0 256.395Z',
  mutedTopRight: 'M256.392 0L404.327 0C431.941 0 454.327 22.3858 454.327 50V197.935H256.392V0Z',
}

let dockIconSyncSequence = 0

export async function syncThemeDockIcon(theme: ThemeDefinition): Promise<void> {
  try {
    if (typeof window === 'undefined' || typeof document === 'undefined') return

    const sequence = ++dockIconSyncSequence
    const { invoke, isTauri } = await import('@tauri-apps/api/core')
    if (sequence !== dockIconSyncSequence || !isTauri()) return

    const pngDataUrl = createThemeDockIconDataUrl(theme)
    if (sequence !== dockIconSyncSequence) return

    await invoke('set_theme_dock_icon', {
      request: { pngDataUrl },
    })
  } catch {
    // The bundled icon remains the fallback if the runtime cannot update it.
  }
}

export function createThemeDockIconDataUrl(theme: ThemeDefinition): string {
  const canvas = document.createElement('canvas')
  canvas.width = DOCK_ICON_SIZE
  canvas.height = DOCK_ICON_SIZE

  const ctx = canvas.getContext('2d')
  if (!ctx) {
    throw new Error('Canvas 2D context is unavailable.')
  }

  ctx.clearRect(0, 0, DOCK_ICON_SIZE, DOCK_ICON_SIZE)
  ctx.fillStyle = theme.colors.shellBackground || theme.colors.background
  roundedRect(
    ctx,
    DOCK_ICON_TILE_INSET,
    DOCK_ICON_TILE_INSET,
    DOCK_ICON_SIZE - DOCK_ICON_TILE_INSET * 2,
    DOCK_ICON_SIZE - DOCK_ICON_TILE_INSET * 2,
    DOCK_ICON_TILE_RADIUS,
  )
  ctx.fill()

  const logoSize = LOGO_VIEWBOX_SIZE * LOGO_SCALE
  const offset = (DOCK_ICON_SIZE - logoSize) / 2

  ctx.save()
  ctx.translate(offset, offset)
  ctx.scale(LOGO_SCALE, LOGO_SCALE)

  ctx.fillStyle = theme.colors.primary
  ctx.fill(new Path2D(LOGO_PATHS.primaryTopLeft))
  ctx.fill(new Path2D(LOGO_PATHS.primaryBottomRight))

  ctx.globalAlpha = 0.32
  ctx.fillStyle = theme.colors.foreground
  ctx.fill(new Path2D(LOGO_PATHS.mutedTopRight))
  ctx.fill(new Path2D(LOGO_PATHS.mutedBottomLeft))

  ctx.restore()

  return canvas.toDataURL('image/png')
}

function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  const right = x + width
  const bottom = y + height

  ctx.beginPath()
  ctx.moveTo(x + radius, y)
  ctx.lineTo(right - radius, y)
  ctx.quadraticCurveTo(right, y, right, y + radius)
  ctx.lineTo(right, bottom - radius)
  ctx.quadraticCurveTo(right, bottom, right - radius, bottom)
  ctx.lineTo(x + radius, bottom)
  ctx.quadraticCurveTo(x, bottom, x, bottom - radius)
  ctx.lineTo(x, y + radius)
  ctx.quadraticCurveTo(x, y, x + radius, y)
  ctx.closePath()
}
