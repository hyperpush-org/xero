import { useMemo } from 'react'
import { useReducedMotion, type Transition } from 'motion/react'

const SIDEBAR_REVEAL_EASE: [number, number, number, number] = [0.22, 1, 0.36, 1]
const SIDEBAR_REVEAL_EASE_CSS = 'cubic-bezier(0.22, 1, 0.36, 1)'

const SIDEBAR_WIDTH_DURATION_MS = 200
const SIDEBAR_REVEAL_DURATION_MS = 160
const SIDEBAR_LAYOUT_DURATION_MS = 200

// Kept for any motion-based callers; transform/opacity content slides still
// run on motion since those are GPU-accelerated and don't trigger layout.
export const SIDEBAR_REVEAL_TRANSITION: Transition = {
  duration: SIDEBAR_REVEAL_DURATION_MS / 1000,
  ease: SIDEBAR_REVEAL_EASE,
}

export const SIDEBAR_LAYOUT_TRANSITION: Transition = {
  duration: SIDEBAR_LAYOUT_DURATION_MS / 1000,
  ease: SIDEBAR_REVEAL_EASE,
}

// Width transitions are driven by CSS, not motion, so animating elements no
// longer need a motion `transition` prop for width. This export remains for
// backwards-compat callers that still use `motion.aside animate={{ width }}`
// — it matches the CSS timing so the two paths feel identical.
export const SIDEBAR_WIDTH_TRANSITION: Transition = {
  duration: SIDEBAR_WIDTH_DURATION_MS / 1000,
  ease: SIDEBAR_REVEAL_EASE,
}

export const SIDEBAR_INSTANT_TRANSITION: Transition = { duration: 0 }

export interface SidebarWidthMotion {
  /** Class to apply to the sidebar root for compositor isolation + paint containment. */
  islandClassName: string
  /**
   * Inline style for the sidebar root. Apply this so width is set + the CSS
   * transition is configured to animate width changes. Returns no transition
   * during resize so the panel tracks the cursor 1:1.
   */
  style: React.CSSProperties
}

/**
 * CSS-driven width animation primitives. Returns a className + style that
 * animate `width` via the browser's CSS engine — this avoids motion's per-
 * frame React/JS spring loop for layout-triggering properties, which is the
 * single biggest source of jank in the sidebars (animating width on a flex
 * sibling forces the main content to reflow on every frame).
 *
 * The associated `.sidebar-motion-island` class (defined in globals.css)
 * adds `contain: layout paint style`, GPU-layer promotion, and
 * `will-change: width` so the browser can isolate the work to a separate
 * compositor layer instead of repainting the rest of the UI.
 */
export function useSidebarWidthMotion(
  width: number,
  options: { animate?: boolean; isResizing?: boolean; durationMs?: number } = {},
): SidebarWidthMotion {
  const shouldReduceMotion = useReducedMotion()
  const { animate = true, isResizing = false, durationMs = SIDEBAR_WIDTH_DURATION_MS } = options

  return useMemo<SidebarWidthMotion>(() => {
    const noTransition = !animate || shouldReduceMotion || isResizing
    return {
      islandClassName: animate ? 'sidebar-motion-island' : 'sidebar-layout-island',
      style: {
        width,
        transition: noTransition
          ? 'none'
          : `width ${durationMs}ms ${SIDEBAR_REVEAL_EASE_CSS}`,
      },
    }
  }, [animate, durationMs, isResizing, shouldReduceMotion, width])
}

/**
 * Legacy hook — still used by motion-based content slides (transform/opacity
 * inside the sidebar). The width transition returned here is now timed to
 * match the CSS path, but callers that have switched to plain `<aside>` no
 * longer pass `widthTransition` to anything.
 */
export function useSidebarMotion(isResizing = false) {
  const shouldReduceMotion = useReducedMotion()

  return {
    contentTransition: shouldReduceMotion
      ? SIDEBAR_INSTANT_TRANSITION
      : SIDEBAR_REVEAL_TRANSITION,
    layoutTransition: shouldReduceMotion
      ? SIDEBAR_INSTANT_TRANSITION
      : SIDEBAR_LAYOUT_TRANSITION,
    widthTransition: isResizing || shouldReduceMotion
      ? SIDEBAR_INSTANT_TRANSITION
      : SIDEBAR_WIDTH_TRANSITION,
  }
}
