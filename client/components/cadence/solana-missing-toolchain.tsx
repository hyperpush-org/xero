"use client"

import { Download, ExternalLink, Loader2, RefreshCw } from "lucide-react"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
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

// Panel shown above the cluster picker when the host is missing the
// minimum Solana toolchain (the Solana CLI). Other tools are only flagged
// if they would be actively used by the current cluster flow.
export function SolanaMissingToolchain({
  status,
  loading,
  installing,
  installEvent,
  onInstall,
  onRefresh,
}: Props) {
  if (!status) return null
  const panel = buildPanel(status)
  if (!panel) return null
  const progress =
    typeof installEvent?.progress === "number"
      ? Math.round(installEvent.progress * 100)
      : null
  const canInstall = Boolean(status.installSupported ?? true)

  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-amber-200">{panel.title}</div>
      <div className="text-muted-foreground">{panel.detail}</div>
      <div className="flex flex-wrap items-center gap-1.5">
        {(status.installableComponents ?? []).map((component) => (
          <Badge
            className={cn(
              "h-5 border-border/70 bg-background/50 px-1.5 text-[10px]",
              component.installed
                ? "text-emerald-300"
                : "text-amber-200",
            )}
            key={component.component}
            variant="outline"
          >
            {component.label}
          </Badge>
        ))}
      </div>
      {installing || installEvent ? (
        <div className="space-y-1">
          <div className="flex items-center gap-1.5 text-[10.5px] text-muted-foreground">
            {installing ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
            <span>
              {installEvent?.error ??
                installEvent?.message ??
                "Preparing managed Solana tools"}
            </span>
          </div>
          {installing ? (
            <Progress
              className={cn("h-1.5", progress == null && "opacity-70")}
              value={progress ?? 0}
            />
          ) : null}
        </div>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          className="h-7 px-2 text-[11px]"
          disabled={!canInstall || installing}
          onClick={onInstall}
          size="sm"
          type="button"
          variant="default"
        >
          {installing ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Download className="h-3 w-3" />
          )}
          Install managed tools
        </Button>
        {panel.actions.map((action) => (
          <Button
            asChild
            className="h-7 px-2 text-[11px]"
            key={action.label}
            size="sm"
            variant="outline"
          >
            <a href={action.href} rel="noreferrer" target="_blank">
              {action.label}
              <ExternalLink className="h-3 w-3" />
            </a>
          </Button>
        ))}
        <Button
          aria-label="Re-detect toolchain"
          className="h-7 px-2 text-[11px]"
          disabled={loading}
          onClick={onRefresh}
          size="sm"
          type="button"
          variant="outline"
        >
          <RefreshCw className={cn("h-3 w-3", loading && "animate-spin")} />
          Re-detect
        </Button>
      </div>
    </div>
  )
}

interface Panel {
  title: string
  detail: string
  actions: Array<{ label: string; href: string }>
}

function buildPanel(status: ToolchainStatus): Panel | null {
  if (!status.solanaCli.present) {
    return {
      title: "Solana CLI not found",
      detail:
        "Install the managed Solana tool suite so Cadence can spin up local validators and submit transactions without relying on a host PATH setup.",
      actions: [
        {
          label: "Solana CLI docs",
          href: "https://docs.solanalabs.com/cli/install",
        },
      ],
    }
  }

  // Anchor is optional for pure Rust programs, but advise if both rust
  // and anchor are missing so the user understands the full toolchain.
  if (!status.anchor.present && !status.cargoBuildSbf.present) {
    return {
      title: "Program build tooling not found",
      detail:
        "Install the managed build tools so Cadence can run Anchor or cargo-build-sbf for deployable .so artifacts.",
      actions: [
        { label: "Anchor docs", href: "https://www.anchor-lang.com/docs/installation" },
        { label: "Solana CLI docs", href: "https://docs.solanalabs.com/cli/install" },
      ],
    }
  }

  return null
}
