"use client"

import type { CSSProperties, ReactNode } from "react"
import { AnimatePresence, motion, useReducedMotion } from "motion/react"

import { cn } from "@/lib/utils"
import {
  FLOATING_RIGHT_SIDEBAR_TRANSITION,
  SIDEBAR_INSTANT_TRANSITION,
} from "@/lib/sidebar-motion"

interface FloatingRightSidebarFrameProps {
  open: boolean
  label: string
  children: ReactNode
  width: CSSProperties["width"]
  ariaBusy?: boolean
  overlayClassName?: string
  panelClassName?: string
  panelStyle?: CSSProperties
  onOverlayClick?: () => void
}

export function FloatingRightSidebarFrame({
  open,
  label,
  children,
  width,
  ariaBusy,
  overlayClassName,
  panelClassName,
  panelStyle,
  onOverlayClick,
}: FloatingRightSidebarFrameProps) {
  const shouldReduceMotion = useReducedMotion()
  const transition = shouldReduceMotion
    ? SIDEBAR_INSTANT_TRANSITION
    : FLOATING_RIGHT_SIDEBAR_TRANSITION

  return (
    <AnimatePresence>
      {open ? (
        <>
          <motion.div
            aria-hidden="true"
            className={cn("fixed inset-0 z-40 bg-black/30", overlayClassName)}
            data-slot="floating-right-sidebar-overlay"
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            initial={{ opacity: 0 }}
            onClick={onOverlayClick}
            transition={transition}
          />
          <motion.aside
            aria-busy={ariaBusy || undefined}
            aria-label={label}
            className={cn(
              "gpu-layer fixed inset-y-0 right-0 z-50 flex flex-col overflow-hidden border-l border-border/80 bg-sidebar shadow-2xl",
              panelClassName,
            )}
            data-slot="floating-right-sidebar-panel"
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            initial={{ x: "100%" }}
            style={{
              width,
              contain: "layout paint style",
              ...panelStyle,
            }}
            transition={transition}
          >
            {children}
          </motion.aside>
        </>
      ) : null}
    </AnimatePresence>
  )
}
