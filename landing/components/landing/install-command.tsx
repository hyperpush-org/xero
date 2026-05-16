"use client"

import { useEffect, useState } from "react"
import { Check, Copy, Terminal } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

type InstallCommandProps = {
  command: string
  label: string
  tone?: "primary" | "secondary"
}

export function InstallCommand({
  command,
  label,
  tone = "secondary",
}: InstallCommandProps) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!copied) {
      return
    }

    const timeout = window.setTimeout(() => setCopied(false), 1600)
    return () => window.clearTimeout(timeout)
  }, [copied])

  async function copyCommand() {
    await navigator.clipboard.writeText(command)
    setCopied(true)
  }

  return (
    <div
      className={cn(
        "overflow-hidden rounded-md border text-left shadow-[0_18px_50px_-32px_black]",
        tone === "primary"
          ? "border-primary/30 bg-primary/10"
          : "border-border/70 bg-secondary/25",
      )}
    >
      <div className="flex min-h-11 items-center justify-between gap-3 border-b border-border/60 px-3">
        <div className="flex min-w-0 items-center gap-2 text-xs font-medium text-muted-foreground">
          <Terminal className="h-3.5 w-3.5 shrink-0 text-primary" />
          <span className="truncate">{label}</span>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={copyCommand}
          className="h-8 shrink-0 gap-1.5 px-2 text-xs text-muted-foreground hover:text-foreground"
          aria-label={`Copy ${label} install command`}
        >
          {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <pre className="overflow-x-auto px-4 py-3 font-mono text-[12px] leading-relaxed text-foreground sm:text-sm">
        <code>{command}</code>
      </pre>
    </div>
  )
}
