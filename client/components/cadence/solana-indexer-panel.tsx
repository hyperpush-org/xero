"use client"

import { useMemo, useState } from "react"
import { FileCode2, FolderCog, Loader2, Play, Wrench } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type {
  ClusterKind,
  IndexerKind,
  IndexerRunReport,
  ScaffoldResult,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaIndexerPanelProps {
  cluster: ClusterKind
  busy: boolean
  lastScaffold: ScaffoldResult | null
  lastRun: IndexerRunReport | null
  onScaffold: (args: {
    kind: IndexerKind
    idlPath: string
    outputDir: string
    projectSlug?: string | null
    overwrite?: boolean
    rpcUrl?: string | null
  }) => Promise<ScaffoldResult | null>
  onRun: (args: {
    cluster: ClusterKind
    programIds: string[]
    lastN?: number
    rpcUrl?: string | null
  }) => Promise<IndexerRunReport | null>
}

export function SolanaIndexerPanel({
  cluster,
  busy,
  lastScaffold,
  lastRun,
  onScaffold,
  onRun,
}: SolanaIndexerPanelProps) {
  const [kind, setKind] = useState<IndexerKind>("carbon")
  const [idlPath, setIdlPath] = useState("")
  const [outputDir, setOutputDir] = useState("")
  const [projectSlug, setProjectSlug] = useState("")
  const [rpcUrl, setRpcUrl] = useState("")
  const [overwrite, setOverwrite] = useState(false)

  const [runProgramsInput, setRunProgramsInput] = useState("")
  const [runLastNInput, setRunLastNInput] = useState("25")
  const [runRpcUrl, setRunRpcUrl] = useState("")

  const [status, setStatus] = useState<string | null>(null)

  const runProgramIds = useMemo(
    () =>
      runProgramsInput
        .split(/[\s,]+/)
        .map((value) => value.trim())
        .filter(Boolean),
    [runProgramsInput],
  )

  const handleScaffold = async () => {
    if (!idlPath.trim() || !outputDir.trim()) {
      setStatus("IDL path and output directory are required.")
      return
    }

    const result = await onScaffold({
      kind,
      idlPath: idlPath.trim(),
      outputDir: outputDir.trim(),
      projectSlug: projectSlug.trim() || null,
      overwrite,
      rpcUrl: rpcUrl.trim() || null,
    })

    if (result) {
      setStatus(`Generated ${result.files.length} files in ${result.root}`)
    } else {
      setStatus("Scaffold failed")
    }
  }

  const handleRun = async () => {
    if (runProgramIds.length === 0) {
      setStatus("Provide at least one program id for the local runner.")
      return
    }

    const parsedLastN = Number.parseInt(runLastNInput, 10)
    const result = await onRun({
      cluster,
      programIds: runProgramIds,
      lastN: Number.isFinite(parsedLastN) ? parsedLastN : 25,
      rpcUrl: runRpcUrl.trim() || null,
    })

    if (result) {
      setStatus(`Runner decoded ${result.entries.length} entries.`)
    } else {
      setStatus("Indexer runner failed")
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <section className="space-y-1.5">
        <div className="text-[11.5px] font-medium text-foreground">Scaffold</div>
        <Select
          onValueChange={(value) => setKind(value as IndexerKind)}
          value={kind}
        >
          <SelectTrigger
            aria-label="Indexer kind"
            className="h-8 w-full border-border/60 bg-background text-[12px] focus:border-primary/60"
            size="sm"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="carbon">carbon (Rust)</SelectItem>
            <SelectItem value="log_parser">log_parser (TypeScript)</SelectItem>
            <SelectItem value="helius_webhook">helius_webhook (Express)</SelectItem>
          </SelectContent>
        </Select>
        <input
          aria-label="IDL path"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[12px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
          onChange={(event) => setIdlPath(event.target.value)}
          placeholder="IDL path (e.g. target/idl/my_program.json)"
          value={idlPath}
        />
        <input
          aria-label="Output directory"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[12px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
          onChange={(event) => setOutputDir(event.target.value)}
          placeholder="Output directory"
          value={outputDir}
        />
        <div className="grid grid-cols-2 gap-1.5">
          <input
            aria-label="Project slug"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            onChange={(event) => setProjectSlug(event.target.value)}
            placeholder="project slug (optional)"
            value={projectSlug}
          />
          <input
            aria-label="Scaffold RPC URL"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            onChange={(event) => setRpcUrl(event.target.value)}
            placeholder="rpc URL (optional)"
            value={rpcUrl}
          />
        </div>
        <label className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
          <input
            checked={overwrite}
            onChange={(event) => setOverwrite(event.target.checked)}
            type="checkbox"
          />
          overwrite existing files
        </label>
        <button
          type="button"
          onClick={handleScaffold}
          disabled={busy}
          className={cn(
            "inline-flex h-8 items-center gap-1 rounded-md border border-primary/50 bg-primary/10 px-2.5 text-[11px] font-medium text-primary",
            "hover:bg-primary/20 disabled:opacity-50",
          )}
        >
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FolderCog className="h-3.5 w-3.5" />}
          Generate scaffold
        </button>
      </section>

      <section className="space-y-1.5">
        <div className="text-[11.5px] font-medium text-foreground">Local runner</div>
        <input
          aria-label="Program IDs"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[12px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
          onChange={(event) => setRunProgramsInput(event.target.value)}
          placeholder="Program IDs (comma-separated)"
          value={runProgramsInput}
        />
        <div className="grid grid-cols-2 gap-1.5">
          <input
            aria-label="Last N signatures"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            inputMode="numeric"
            onChange={(event) => setRunLastNInput(event.target.value)}
            placeholder="last N"
            value={runLastNInput}
          />
          <input
            aria-label="Runner RPC URL"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            onChange={(event) => setRunRpcUrl(event.target.value)}
            placeholder="rpc URL (optional)"
            value={runRpcUrl}
          />
        </div>
        <button
          type="button"
          onClick={handleRun}
          disabled={busy}
          className={cn(
            "inline-flex h-8 items-center gap-1 rounded-md border border-border/70 bg-background px-2.5 text-[11px] text-foreground/85",
            "hover:border-primary/40 hover:text-foreground disabled:opacity-50",
          )}
        >
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
          Replay last N slots
        </button>
      </section>

      {status ? <p className="text-[11px] text-muted-foreground">{status}</p> : null}

      {lastScaffold ? (
        <section className="space-y-1.5 border-t border-border/50 pt-3">
          <div className="flex items-center gap-1 text-[11px] uppercase tracking-wider text-muted-foreground">
            <Wrench className="h-3 w-3" />
            Last scaffold
          </div>
          <p className="text-[11px] text-foreground/85">{lastScaffold.root}</p>
          <p className="text-[11px] text-muted-foreground">{lastScaffold.runHint}</p>
          <ul className="max-h-28 space-y-1 overflow-auto">
            {lastScaffold.files.map((file) => (
              <li
                key={file.path}
                className="truncate rounded border border-border/40 bg-background/40 px-2 py-1 font-mono text-[10px] text-foreground/75"
              >
                {file.path}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {lastRun ? (
        <section className="space-y-1.5 border-t border-border/50 pt-3">
          <div className="flex items-center gap-1 text-[11px] uppercase tracking-wider text-muted-foreground">
            <FileCode2 className="h-3 w-3" />
            Last run
          </div>
          <p className="text-[11px] text-foreground/85">
            {lastRun.fetchedSignatures} signature(s) · {lastRun.entries.length} decoded entries
          </p>
          <ul className="space-y-1">
            {lastRun.eventsByProgram.map((count) => (
              <li
                key={count.programId}
                className="flex items-center justify-between rounded border border-border/40 bg-background/40 px-2 py-1 text-[10.5px]"
              >
                <span className="truncate font-mono text-muted-foreground">{count.programId}</span>
                <span className="tabular-nums text-foreground/80">
                  tx {count.transactions} · events {count.anchorEvents}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  )
}
