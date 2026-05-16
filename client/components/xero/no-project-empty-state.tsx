import { FolderPlus, Loader2, Lock } from "lucide-react"
import { Button } from "@/components/ui/button"
import { AppLogo } from "@xero/ui/components/app-logo"

interface NoProjectEmptyStateProps {
  isDesktopRuntime: boolean
  isImporting: boolean
  onImport: () => void
}

export function NoProjectEmptyState({ isDesktopRuntime, isImporting, onImport }: NoProjectEmptyStateProps) {
  return (
    <div className="relative flex flex-1 items-center justify-center overflow-hidden bg-background">
      {/* Subtle single-glow background — uses primary so it picks up the active theme. */}
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[680px] -translate-x-1/2 -translate-y-[55%] rounded-full bg-primary/[0.06] blur-[120px]"
      />

      <div className="relative flex w-full max-w-sm flex-col items-center px-8 text-center">
        <BrandGlyph />

        <h2 className="mt-6 text-xl font-semibold tracking-tight text-foreground">
          {isDesktopRuntime ? "Add your first project" : "Open Xero desktop to continue"}
        </h2>
        <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
          {isDesktopRuntime
            ? "Import a local Git repository to start planning and running work."
            : "Project import is only available inside the Tauri desktop runtime."}
        </p>

        <div className="mt-6">
          {isDesktopRuntime ? (
            <Button
              onClick={onImport}
              disabled={isImporting}
              size="sm"
              className="h-9 gap-2 bg-primary px-4 text-[12px] font-medium hover:bg-primary/90"
            >
              {isImporting ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  Importing…
                </>
              ) : (
                <>
                  <FolderPlus className="h-3.5 w-3.5" />
                  Import repository
                </>
              )}
            </Button>
          ) : (
            <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-card/40 px-2.5 py-1 text-[11px] text-muted-foreground">
              <Lock className="h-3 w-3" />
              Desktop runtime required
            </span>
          )}
        </div>
      </div>
    </div>
  )
}

function BrandGlyph() {
  return (
    <div className="relative">
      <div className="absolute inset-0 -z-10 rounded-3xl bg-primary/10 blur-2xl" />
      <AppLogo className="h-12 w-12" />
    </div>
  )
}
