import { cn } from "../lib/utils"

interface AppLogoProps {
  className?: string
  /**
   * Optional aria-label. Defaults to empty so the logo is treated as
   * decorative (most callers pair it with adjacent text).
   */
  "aria-label"?: string
}

/**
 * Inlined Xero brand glyph. Inlined (rather than served via `<img>`) so the
 * fills can resolve `var(--primary)` / `var(--foreground)` from the active
 * theme — external SVGs loaded through `<img>` cannot read CSS variables
 * from the host document.
 */
export function AppLogo({ className, "aria-label": ariaLabel }: AppLogoProps) {
  const decorative = !ariaLabel
  return (
    <svg
      viewBox="0 0 455 455"
      xmlns="http://www.w3.org/2000/svg"
      className={cn("shrink-0", className)}
      role={decorative ? undefined : "img"}
      aria-hidden={decorative ? true : undefined}
      aria-label={ariaLabel}
    >
      <path
        d="M256.391 256.395H454.326V404.33C454.326 431.944 431.941 454.33 404.326 454.33H256.391V256.395Z"
        fill="var(--primary)"
      />
      <path
        d="M197.936 197.941L0.000289917 197.941L0.000276984 50.0064C0.00027457 22.3921 22.386 0.00637826 50.0003 0.00637585L197.936 0.00636292L197.936 197.941Z"
        fill="var(--primary)"
      />
      <path
        d="M0 256.395H197.935V454.33H50.0001C22.3858 454.33 0 431.944 0 404.33L0 256.395Z"
        fill="var(--foreground)"
        fillOpacity="0.32"
      />
      <path
        d="M256.392 0L404.327 0C431.941 0 454.327 22.3858 454.327 50V197.935H256.392V0Z"
        fill="var(--foreground)"
        fillOpacity="0.32"
      />
    </svg>
  )
}
