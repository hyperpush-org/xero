"use client"

import { useMemo, useState } from "react"
import { CheckCircle2, Copy, FolderPlus, Loader2, Wallet } from "lucide-react"
import { cn } from "@/lib/utils"
import type {
  ClusterKind,
  WalletDescriptor,
  WalletKind,
  WalletScaffoldRequest,
  WalletScaffoldResult,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaWalletPanelProps {
  cluster: ClusterKind
  busy: boolean
  descriptors: WalletDescriptor[]
  lastScaffold: WalletScaffoldResult | null
  onGenerate: (
    request: WalletScaffoldRequest,
  ) => Promise<WalletScaffoldResult | null>
}

export function SolanaWalletPanel({
  cluster,
  busy,
  descriptors,
  lastScaffold,
  onGenerate,
}: SolanaWalletPanelProps) {
  const [selectedKind, setSelectedKind] = useState<WalletKind>("wallet_standard")
  const [outputDir, setOutputDir] = useState<string>("")
  const [projectSlug, setProjectSlug] = useState<string>("")
  const [appName, setAppName] = useState<string>("My Solana dapp")
  const [appId, setAppId] = useState<string>("")
  const [rpcUrl, setRpcUrl] = useState<string>("")
  const [overwrite, setOverwrite] = useState<boolean>(false)

  const selected = useMemo(
    () => descriptors.find((d) => d.kind === selectedKind) ?? null,
    [descriptors, selectedKind],
  )

  const submit = async () => {
    if (!outputDir.trim()) return
    const request: WalletScaffoldRequest = {
      kind: selectedKind,
      outputDir: outputDir.trim(),
      projectSlug: projectSlug.trim() || null,
      cluster,
      appName: appName.trim() || null,
      appId: appId.trim() || null,
      rpcUrl: rpcUrl.trim() || null,
      overwrite,
    }
    await onGenerate(request)
  }

  return (
    <div className="flex flex-col gap-3">
      {descriptors.length === 0 ? (
        <p className="text-[11.5px] text-muted-foreground">
          Loading wallet scaffold catalogue…
        </p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {descriptors.map((desc) => (
            <li key={desc.kind}>
              <button
                type="button"
                onClick={() => setSelectedKind(desc.kind)}
                aria-pressed={selectedKind === desc.kind}
                className={cn(
                  "flex w-full flex-col gap-1 rounded-md border px-2.5 py-2 text-left transition-colors",
                  selectedKind === desc.kind
                    ? "border-primary/60 bg-primary/10"
                    : "border-border/60 bg-background/40 hover:border-primary/40",
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[12px] font-medium text-foreground">
                    {desc.label}
                  </span>
                  {desc.requiresApiKey ? (
                    <span className="rounded-full border border-amber-400/40 bg-amber-400/10 px-1.5 py-0.5 text-[9.5px] text-amber-300">
                      needs key
                    </span>
                  ) : null}
                </div>
                <span className="text-[11px] leading-snug text-foreground/75">
                  {desc.summary}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {selected ? (
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            void submit()
          }}
        >
          <Field label="Output directory">
            <input
              value={outputDir}
              onChange={(event) => setOutputDir(event.target.value)}
              placeholder="/absolute/path/to/parent"
              className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
              required
            />
          </Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label="Project slug (optional)">
              <input
                value={projectSlug}
                onChange={(event) => setProjectSlug(event.target.value)}
                placeholder="derived from kind"
                className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
              />
            </Field>
            <Field label="App name">
              <input
                value={appName}
                onChange={(event) => setAppName(event.target.value)}
                className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
              />
            </Field>
          </div>
          <Field label="Override RPC URL (optional)">
            <input
              value={rpcUrl}
              onChange={(event) => setRpcUrl(event.target.value)}
              placeholder={`uses cluster default for ${cluster}`}
              className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
            />
          </Field>
          {selected.requiresApiKey ? (
            <Field
              label={`API key / id (${selected.kind === "privy" ? "Privy App ID" : "Dynamic environment id"})`}
            >
              <input
                value={appId}
                onChange={(event) => setAppId(event.target.value)}
                placeholder="leave blank to paste later"
                className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
              />
            </Field>
          ) : null}
          <label className="inline-flex items-center gap-2 text-[11px] text-foreground/80">
            <input
              type="checkbox"
              checked={overwrite}
              onChange={(event) => setOverwrite(event.target.checked)}
            />
            Overwrite existing files in the target directory
          </label>
          <button
            type="submit"
            disabled={busy || !outputDir.trim()}
            className={cn(
              "inline-flex items-center justify-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-3 py-1.5 text-[11.5px] font-medium text-primary",
              "hover:bg-primary/25 disabled:opacity-50",
            )}
          >
            {busy ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <FolderPlus className="h-3 w-3" />
            )}
            Generate scaffold
          </button>
        </form>
      ) : null}

      {lastScaffold ? <ScaffoldReport result={lastScaffold} /> : null}
    </div>
  )
}

function ScaffoldReport({ result }: { result: WalletScaffoldResult }) {
  return (
    <div className="rounded border border-border/70 bg-background/40 p-2.5 text-[11px]">
      <div className="flex items-center gap-1.5">
        <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
        <span className="font-medium">
          {result.files.length} files written to{" "}
          <code className="font-mono text-[10.5px]">{result.root}</code>
        </span>
      </div>
      <p className="mt-1 text-muted-foreground">
        Cluster: {result.cluster} · RPC: {result.rpcUrl}
      </p>
      {result.apiKeyEnv ? (
        <p className="mt-1 text-[10.5px] text-amber-400">
          Populate <code className="font-mono">{result.apiKeyEnv}</code> in{" "}
          <code className="font-mono">.env</code> before running the scaffold.
        </p>
      ) : null}
      {result.nextSteps.length > 0 ? (
        <ol className="mt-2 list-decimal space-y-0.5 pl-4 text-[11px]">
          {result.nextSteps.map((step, idx) => (
            <li key={idx}>{step}</li>
          ))}
        </ol>
      ) : null}
      <div className="mt-2 flex items-center gap-1.5">
        <button
          type="button"
          onClick={() => {
            void navigator.clipboard?.writeText(result.runHint).catch(() => {})
          }}
          className="inline-flex items-center gap-1 rounded border border-border/70 bg-background/60 px-2 py-0.5 text-[10.5px] text-foreground/85 hover:border-primary/40"
        >
          <Copy className="h-3 w-3" />
          Copy run command
        </button>
        <code className="truncate font-mono text-[10.5px] text-muted-foreground">
          {result.runHint}
        </code>
      </div>
      <details className="mt-2">
        <summary className="cursor-pointer text-[10.5px] text-muted-foreground">
          Files
        </summary>
        <ul className="mt-1 space-y-0.5 font-mono text-[10px] text-muted-foreground">
          {result.files.map((file) => (
            <li key={file.path}>
              {file.path}{" "}
              <span className="opacity-60">
                · {file.bytesWritten}B · {file.sha256.slice(0, 10)}
              </span>
            </li>
          ))}
        </ul>
      </details>
    </div>
  )
}

function Field({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      {children}
    </label>
  )
}

// Mark Wallet as used to satisfy the bundler when tree-shaking strips
// the icon — the sidebar uses it as the tab icon.
export const WalletPanelTabIcon = Wallet
