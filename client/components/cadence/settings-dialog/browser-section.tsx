import { Cookie, Loader2, RefreshCw } from "lucide-react"
import { useEffect } from "react"
import {
  useCookieImport,
  type DetectedBrowser,
} from "@/components/cadence/browser-cookie-import"
import { cn } from "@/lib/utils"

export function BrowserSection() {
  const { browsers, status, refresh, importFrom } = useCookieImport({
    autoLoad: true,
  })

  useEffect(() => {
    if (status.kind !== "success") return
    const t = setTimeout(() => {
      void refresh()
    }, 0)
    return () => clearTimeout(t)
  }, [status, refresh])

  const available = browsers.filter((b) => b.available)
  const unavailable = browsers.filter((b) => !b.available)
  const running = status.kind === "running"

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="text-[13px] font-semibold text-foreground">Browser</h3>
        <p className="mt-1 text-[12px] text-muted-foreground">
          Copy cookies from other installed browsers into Cadence's in-app
          browser so you stay signed in while developing.
        </p>
      </div>

      <div className="rounded-lg border border-border bg-card px-4 py-3">
        <div className="flex items-start gap-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Cookie className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-medium text-foreground">Import cookies</p>
            <p className="text-[11px] text-muted-foreground">
              Pick a source browser. The first import from a given browser may
              prompt once for Keychain access; cookies apply on the next reload.
              The in-app browser must be open at least once.
            </p>
          </div>
          <button
            aria-label="Rescan installed browsers"
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            onClick={() => void refresh()}
            title="Rescan"
            type="button"
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
        </div>

        {available.length === 0 ? (
          <p className="mt-3 text-[11.5px] text-muted-foreground/80">
            No supported browsers detected on this machine.
          </p>
        ) : (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {available.map((browser) => (
              <BrowserChip
                key={browser.id}
                browser={browser}
                running={running && status.kind === "running" && status.source === browser.id}
                disabled={running}
                onClick={() => void importFrom(browser)}
              />
            ))}
          </div>
        )}

        {status.kind === "success" ? (
          <p className="mt-3 text-[11.5px] text-foreground/85">
            Imported {status.result.imported} cookies across{" "}
            {status.result.domains} domains
            {status.result.skipped > 0
              ? ` (${status.result.skipped} skipped)`
              : ""}
            .
          </p>
        ) : null}
        {status.kind === "error" ? (
          <p className="mt-3 text-[11.5px] text-destructive">{status.message}</p>
        ) : null}

        {unavailable.length > 0 ? (
          <p className="mt-3 text-[10.5px] text-muted-foreground/60">
            Not detected: {unavailable.map((b) => b.label).join(", ")}.
          </p>
        ) : null}
      </div>
    </div>
  )
}

interface BrowserChipProps {
  browser: DetectedBrowser
  running: boolean
  disabled: boolean
  onClick: () => void
}

function BrowserChip({ browser, running, disabled, onClick }: BrowserChipProps) {
  return (
    <button
      className={cn(
        "flex items-center gap-1.5 rounded-md border border-border/70 bg-background/60 px-2.5 py-1 text-[11.5px] text-foreground transition-colors",
        "hover:border-primary/40 hover:bg-background",
        "disabled:cursor-not-allowed disabled:opacity-60",
      )}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {running ? (
        <Loader2 className="h-3 w-3 animate-spin" />
      ) : (
        <Cookie className="h-3 w-3 text-muted-foreground" />
      )}
      <span>{browser.label}</span>
    </button>
  )
}
