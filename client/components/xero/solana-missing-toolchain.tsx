"use client"

import { useId } from "react"
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Loader2,
  TerminalSquare,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import type {
  ToolchainInstallEvent,
  ToolchainStatus,
} from "@/src/features/solana/use-solana-workbench"

interface Props {
  status: ToolchainStatus | null
  loading: boolean
  installing: boolean
  installEvent: ToolchainInstallEvent | null
  onInstall: () => void
  onRefresh: () => void
}

// Strip shown above the cluster picker when the host is missing the minimum
// Solana toolchain needed by the active flow.
export function SolanaMissingToolchain({
  status,
  installing,
  installEvent,
  onInstall,
}: Props) {
  const titleId = useId()
  if (!status) return null
  const panel = buildPanel(status)
  if (!panel) return null
  const progress =
    typeof installEvent?.progress === "number"
      ? Math.round(installEvent.progress * 100)
      : null
  const canInstall = Boolean(status.installSupported ?? true)
  const installEventSucceeded =
    installEvent?.phase === "completed" || installEvent?.phase === "skipped"
  const InstallEventIcon = installEventSucceeded ? CheckCircle2 : AlertCircle
  const failed = Boolean(installEvent?.error)
  const showProgress = installing || installEvent

  return (
    <section
      aria-labelledby={titleId}
      aria-live="polite"
      className={cn(
        "flex shrink-0 flex-col gap-1.5 border-b px-3 py-2 text-[11px]",
        failed
          ? "border-destructive/40 bg-destructive/10"
          : installEventSucceeded
            ? "border-success/35 bg-success/10"
            : "border-border/70 bg-warning/[0.075]",
      )}
      role="region"
    >
      <div className="flex min-w-0 items-center gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <TerminalSquare
            className={cn(
              "h-3.5 w-3.5 shrink-0",
              failed
                ? "text-destructive"
                : installEventSucceeded
                  ? "text-success"
                  : "text-warning",
            )}
            aria-hidden="true"
          />
          <h2
            className={cn(
              "min-w-0 truncate text-[12px] font-semibold leading-5",
              failed
                ? "text-destructive"
                : installEventSucceeded
                  ? "text-success"
                  : "text-warning",
            )}
            id={titleId}
          >
            {panel.title}
          </h2>
        </div>
        {showProgress ? (
          <div className="ml-auto flex min-w-0 items-center gap-1.5">
            {installing ? (
              <Loader2 className="h-3 w-3 shrink-0 animate-spin text-warning" />
            ) : (
              <InstallEventIcon
                className={cn(
                  "h-3 w-3 shrink-0",
                  failed
                    ? "text-destructive"
                    : installEventSucceeded
                      ? "text-success"
                      : "text-muted-foreground",
                )}
              />
            )}
            <span
              className={cn(
                "truncate text-[10.5px]",
                failed
                  ? "text-destructive"
                  : installEventSucceeded
                    ? "text-success"
                    : "text-muted-foreground",
              )}
            >
              {failed
                ? "Failed"
                : installEventSucceeded
                  ? "Installed"
                  : "Installing"}
            </span>
            {progress != null && installing ? (
              <span className="shrink-0 text-[10.5px] text-muted-foreground">
                {progress}%
              </span>
            ) : null}
          </div>
        ) : (
          <Button
            className={cn(
              "ml-auto h-6 min-w-0 px-2 text-[10.5px]",
              !canInstall && "text-muted-foreground",
            )}
            disabled={!canInstall}
            onClick={onInstall}
            size="sm"
            type="button"
            variant="default"
          >
            <Download className="h-3 w-3" />
            <span className="truncate">
              {canInstall ? "Install" : "Unavailable"}
            </span>
          </Button>
        )}
      </div>
      {showProgress ? (
        <>
          <div className="h-1 w-full overflow-hidden rounded-full bg-border/70">
            <div
              aria-label="Solana toolchain install progress"
              aria-valuemax={100}
              aria-valuemin={0}
              aria-valuenow={progress ?? undefined}
              className={cn(
                "h-full motion-progress",
                failed
                  ? "bg-destructive"
                  : installEventSucceeded
                    ? "bg-success"
                    : progress != null
                      ? "bg-warning"
                      : "animate-pulse bg-warning/60",
              )}
              style={{
                transform: `scaleX(${
                  progress != null
                    ? Math.max(0, Math.min(100, progress)) / 100
                    : 1
                })`,
              }}
              role="progressbar"
            />
          </div>
          {installEvent?.error ? (
            <div
              className="truncate text-[10.5px] text-destructive"
              title={installEvent.error}
            >
              {installEvent.error}
            </div>
          ) : installEvent?.message ? (
            <div
              className="truncate text-[10.5px] text-muted-foreground"
              title={installEvent.message}
            >
              {installEvent.message}
            </div>
          ) : null}
        </>
      ) : null}
    </section>
  )
}

interface Panel {
  title: string
}

function buildPanel(status: ToolchainStatus): Panel | null {
  if (!status.solanaCli.present) {
    return {
      title: "Solana CLI not found",
    }
  }

  // Anchor is optional for pure Rust programs, but advise if both rust
  // and anchor are missing so the user understands the full toolchain.
  if (!status.anchor.present && !status.cargoBuildSbf.present) {
    return {
      title: "Program build tooling not found",
    }
  }

  return null
}
