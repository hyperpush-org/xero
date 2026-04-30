import {
  AlertTriangle,
  Check,
  Cookie,
  Globe2,
  Loader2,
  Lock,
  RefreshCw,
  ShieldCheck,
} from "lucide-react"
import { useEffect } from "react"
import {
  useCookieImport,
  type CookieImportStatus,
  type DetectedBrowser,
} from "@/components/xero/browser-cookie-import"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

type StatusTone = "ok" | "warn" | "bad" | "muted"

const TONE_BG: Record<StatusTone, string> = {
  ok: "bg-emerald-500/10",
  warn: "bg-amber-500/10",
  bad: "bg-destructive/10",
  muted: "bg-muted/40",
}

const TONE_RING: Record<StatusTone, string> = {
  ok: "ring-emerald-500/20",
  warn: "ring-amber-500/25",
  bad: "ring-destructive/25",
  muted: "ring-border/60",
}

const TONE_TEXT: Record<StatusTone, string> = {
  ok: "text-emerald-600 dark:text-emerald-400",
  warn: "text-amber-600 dark:text-amber-400",
  bad: "text-destructive",
  muted: "text-muted-foreground",
}

const TONE_DOT: Record<StatusTone, string> = {
  ok: "bg-emerald-500 dark:bg-emerald-400",
  warn: "bg-amber-500 dark:bg-amber-400",
  bad: "bg-destructive",
  muted: "bg-muted-foreground/60",
}

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
  const summary = summarize(available.length, status)

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Browser"
        description="Copy cookies from other installed browsers into Xero's in-app browser so you stay signed in while developing."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={running}
            onClick={() => void refresh()}
            aria-label="Rescan installed browsers"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", running && "animate-spin")} />
            Rescan
          </Button>
        }
      />

      <ReadinessCard summary={summary} availableCount={available.length} status={status} />

      <section className="flex flex-col gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Import from
        </h4>

        {available.length === 0 ? (
          <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/70 bg-secondary/15 px-5 py-7 text-center">
            <div className="flex size-9 items-center justify-center rounded-full border border-border/60 bg-background/60 text-muted-foreground">
              <Globe2 className="h-4 w-4" />
            </div>
            <p className="text-[12.5px] font-medium text-foreground">No supported browsers detected</p>
            <p className="max-w-sm text-[12px] leading-[1.55] text-muted-foreground">
              Xero didn't find any cookie sources on this machine. Install Chrome, Safari, Firefox, Edge, Brave, or Arc, then rescan.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {available.map((browser) => (
              <BrowserCard
                key={browser.id}
                browser={browser}
                running={running && status.kind === "running" && status.source === browser.id}
                disabled={running}
                onClick={() => void importFrom(browser)}
                lastResult={
                  status.kind === "success" && status.source === browser.id
                    ? status.result
                    : null
                }
              />
            ))}
          </div>
        )}

        {status.kind === "error" ? (
          <div
            role="alert"
            className="flex items-start gap-3 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3"
          >
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
            <div className="min-w-0 flex-1">
              <p className="text-[12.5px] font-medium text-destructive">Import failed</p>
              <p className="mt-0.5 text-[12px] leading-[1.5] text-destructive/85">{status.message}</p>
            </div>
          </div>
        ) : null}

        {unavailable.length > 0 ? (
          <p className="text-[11.5px] text-muted-foreground/80">
            <span className="font-medium text-muted-foreground">Not detected on this machine:</span>{" "}
            {unavailable.map((b) => b.label).join(", ")}.
          </p>
        ) : null}
      </section>

      <ImportDetails />
    </div>
  )
}

function ReadinessCard({
  summary,
  availableCount,
  status,
}: {
  summary: { tone: StatusTone; label: string; body: string }
  availableCount: number
  status: CookieImportStatus
}) {
  const lastImport = status.kind === "success" ? status.result : null

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <div
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
            TONE_BG[summary.tone],
            TONE_RING[summary.tone],
          )}
          aria-hidden
        >
          <Cookie className={cn("h-5 w-5", TONE_TEXT[summary.tone])} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">
              In-app browser cookies
            </p>
            <StatusPill tone={summary.tone} label={summary.label} />
            {status.kind === "running" ? (
              <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
                <Loader2 className="h-3 w-3 animate-spin" />
                Importing…
              </span>
            ) : null}
          </div>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">{summary.body}</p>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={Globe2} label="Sources" value={`${availableCount} detected`} />
        {lastImport ? (
          <>
            <MetaItem icon={Cookie} label="Imported" value={`${lastImport.imported} cookies`} />
            <MetaItem icon={Globe2} label="Domains" value={String(lastImport.domains)} />
            {lastImport.skipped > 0 ? (
              <MetaItem icon={ShieldCheck} label="Skipped" value={String(lastImport.skipped)} />
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  )
}

interface BrowserCardProps {
  browser: DetectedBrowser
  running: boolean
  disabled: boolean
  onClick: () => void
  lastResult: { imported: number; domains: number; skipped: number } | null
}

function BrowserCard({ browser, running, disabled, onClick, lastResult }: BrowserCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={`Import cookies from ${browser.label}`}
      className={cn(
        "group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card/30 px-3.5 py-3 text-left transition-all motion-fast",
        "hover:-translate-y-px hover:border-primary/40 hover:bg-card/60 hover:shadow-sm",
        "disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0 disabled:hover:shadow-none",
      )}
    >
      <span
        className={cn(
          "flex size-9 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 transition-colors",
          running ? "text-primary" : "text-muted-foreground group-hover:text-primary",
        )}
        aria-hidden
      >
        {running ? (
          <Loader2 className="h-4 w-4 animate-spin" />
        ) : (
          <Globe2 className="h-4 w-4" />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">{browser.label}</p>
        <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground">
          {running
            ? "Importing cookies…"
            : lastResult
              ? `Imported ${lastResult.imported} cookies · ${lastResult.domains} domains`
              : "Import cookies from this browser"}
        </p>
      </div>
      {lastResult && !running ? (
        <span
          className="flex size-5 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
          aria-hidden
        >
          <Check className="h-3 w-3" />
        </span>
      ) : null}
    </button>
  )
}

function ImportDetails() {
  return (
    <section className="flex flex-col gap-3">
      <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
        How cookie sync works
      </h4>
      <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <DetailRow
          icon={ShieldCheck}
          title="Stays on this machine"
          body="Xero reads cookies from your installed browser's profile on disk. Nothing is uploaded — the import never leaves your device."
        />
        <DetailRow
          icon={Lock}
          title="Keychain prompt on first run"
          body="macOS may prompt once for Keychain access to decrypt cookies. Approve it to allow Xero to read encrypted entries."
        />
        <DetailRow
          icon={RefreshCw}
          title="Applies on next reload"
          body="Open the in-app browser at least once before importing. New cookies take effect when the page next loads."
        />
      </ul>
    </section>
  )
}

function DetailRow({
  icon: Icon,
  title,
  body,
}: {
  icon: React.ElementType
  title: string
  body: string
}) {
  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </li>
  )
}

function StatusPill({ tone, label }: { tone: StatusTone; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em] ring-1 ring-inset",
        TONE_BG[tone],
        TONE_RING[tone],
        TONE_TEXT[tone],
      )}
    >
      <span className={cn("size-1.5 rounded-full", TONE_DOT[tone])} aria-hidden />
      {label}
    </span>
  )
}

function MetaItem({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ElementType
  label: string
  value: string
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className="text-foreground/80">{value}</span>
    </span>
  )
}

function summarize(
  availableCount: number,
  status: CookieImportStatus,
): { tone: StatusTone; label: string; body: string } {
  if (status.kind === "running") {
    return {
      tone: "warn",
      label: "Importing",
      body: "Reading cookies from your selected browser. macOS may prompt once for Keychain access — approve it to continue.",
    }
  }
  if (status.kind === "success") {
    const skipped = status.result.skipped
    return {
      tone: "ok",
      label: "Imported",
      body: `Imported ${status.result.imported} cookies across ${status.result.domains} domains${
        skipped > 0 ? ` (${skipped} skipped)` : ""
      }. Reload the in-app browser to apply.`,
    }
  }
  if (status.kind === "error") {
    return {
      tone: "bad",
      label: "Failed",
      body: "The last import didn't complete. Check the message below and try again — your existing cookies are unchanged.",
    }
  }
  if (availableCount === 0) {
    return {
      tone: "muted",
      label: "No sources",
      body: "Xero didn't detect any installed browsers. Install a supported browser and rescan to pull existing sessions in.",
    }
  }
  return {
    tone: "ok",
    label: "Ready",
    body: `${availableCount} ${
      availableCount === 1 ? "browser is" : "browsers are"
    } ready to import from. Pick a source below — Xero reads cookies locally and never uploads them.`,
  }
}
