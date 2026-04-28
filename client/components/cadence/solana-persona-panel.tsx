"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import {
  Loader2,
  Plus,
  RefreshCw,
  Sparkles,
  Trash2,
  Wallet,
} from "lucide-react"
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
  FundingDelta,
  FundingReceipt,
  Persona,
  PersonaRole,
  RoleDescriptor,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaPersonaPanelProps {
  cluster: ClusterKind
  personas: Persona[]
  roles: RoleDescriptor[]
  busy: boolean
  onRefresh: () => void
  onCreate: (name: string, role: PersonaRole, note: string | null) => Promise<FundingReceipt | null>
  onDelete: (name: string) => Promise<boolean>
  onFund: (name: string, delta: FundingDelta) => Promise<FundingReceipt | null>
  clusterRunning: boolean
}

export function SolanaPersonaPanel({
  cluster,
  personas,
  roles,
  busy,
  onRefresh,
  onCreate,
  onDelete,
  onFund,
  clusterRunning,
}: SolanaPersonaPanelProps) {
  const [newName, setNewName] = useState("")
  const [newRole, setNewRole] = useState<PersonaRole>("whale")
  const [newNote, setNewNote] = useState("")
  const [expandedName, setExpandedName] = useState<string | null>(null)
  const [lastReceipt, setLastReceipt] = useState<FundingReceipt | null>(null)
  const [statusMessage, setStatusMessage] = useState<string | null>(null)

  useEffect(() => {
    onRefresh()
  }, [cluster, onRefresh])

  const rolePresets = useMemo(() => {
    const map = new Map<PersonaRole, RoleDescriptor>()
    for (const descriptor of roles) {
      map.set(descriptor.id, descriptor)
    }
    return map
  }, [roles])

  const handleCreate = useCallback(async () => {
    const trimmed = newName.trim()
    if (!trimmed) {
      setStatusMessage("Provide a persona name")
      return
    }
    setStatusMessage(null)
    const receipt = await onCreate(trimmed, newRole, newNote.trim() || null)
    if (receipt) {
      setLastReceipt(receipt)
      setStatusMessage(
        receipt.succeeded
          ? `Created ${trimmed} — ${receipt.steps.length} funding step(s)`
          : `Created ${trimmed} with ${countFailures(receipt)} failing step(s)`,
      )
      setNewName("")
      setNewNote("")
    }
  }, [newName, newNote, newRole, onCreate])

  const handleRefund = useCallback(
    async (persona: Persona) => {
      const preset = rolePresets.get(persona.role)
      const delta: FundingDelta = preset
        ? {
            solLamports: preset.preset.lamports,
            tokens: preset.preset.tokens,
            nfts: preset.preset.nfts,
          }
        : persona.seed
      const receipt = await onFund(persona.name, delta)
      if (receipt) {
        setLastReceipt(receipt)
        setStatusMessage(
          receipt.succeeded
            ? `Re-funded ${persona.name}`
            : `Re-fund for ${persona.name} had ${countFailures(receipt)} failure(s)`,
        )
      }
    },
    [onFund, rolePresets],
  )

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[11.5px] font-medium text-foreground">
            New persona
            <span className="ml-1.5 font-normal text-muted-foreground">
              on {clusterLabel(cluster)}
            </span>
          </span>
          <button
            aria-label="Refresh personas"
            type="button"
            onClick={onRefresh}
            disabled={busy}
            className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
          >
            <RefreshCw className={cn("h-3 w-3", busy && "animate-spin")} />
          </button>
        </div>
        <div className="flex flex-col gap-1.5">
          <input
            aria-label="Persona name"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[12px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            onChange={(event) => setNewName(event.target.value)}
            placeholder="e.g. whale-1"
            value={newName}
          />
          <Select
            onValueChange={(value) => setNewRole(value as PersonaRole)}
            value={newRole}
          >
            <SelectTrigger
              aria-label="Persona role"
              className="h-8 w-full border-border/60 bg-background text-[12px] focus:border-primary/60"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {roles.map((role) => (
                <SelectItem key={role.id} value={role.id}>
                  {role.preset.displayLabel} · {formatLamports(role.preset.lamports)} SOL
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <input
            aria-label="Persona note"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
            onChange={(event) => setNewNote(event.target.value)}
            placeholder="note (optional)"
            value={newNote}
          />
          <button
            type="button"
            onClick={handleCreate}
            disabled={busy}
            className={cn(
              "mt-0.5 inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-[12px] font-medium text-primary-foreground transition-colors",
              "hover:bg-primary/90 disabled:opacity-50",
            )}
          >
            {busy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Plus className="h-3.5 w-3.5" />
            )}
            Create + fund
          </button>
          {!clusterRunning ? (
            <p className="text-[11px] text-muted-foreground">
              Cluster is stopped — persona is created locally without funding until you start a validator.
            </p>
          ) : null}
          {statusMessage ? (
            <p className="text-[11px] text-foreground/80">{statusMessage}</p>
          ) : null}
        </div>
      </div>

      {personas.length === 0 ? (
        <p className="text-[11.5px] text-muted-foreground">
          No personas yet. Create one to seed a named wallet on {clusterLabel(cluster)}.
        </p>
      ) : (
        <ul className="flex flex-col">
          {personas.map((persona) => {
            const expanded = expandedName === persona.name
            return (
              <li
                key={`${persona.cluster}-${persona.name}`}
                className={cn(
                  "rounded-md transition-colors",
                  expanded ? "bg-muted/40" : "hover:bg-muted/25",
                )}
              >
                <button
                  type="button"
                  onClick={() =>
                    setExpandedName((prev) => (prev === persona.name ? null : persona.name))
                  }
                  className="flex w-full items-center justify-between gap-2 px-2 py-1.5 text-left"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <Wallet className="h-3.5 w-3.5 shrink-0 text-primary" />
                      <span className="truncate text-[12.5px] font-medium text-foreground">
                        {persona.name}
                      </span>
                      <span className="shrink-0 rounded bg-muted/50 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                        {persona.role}
                      </span>
                    </div>
                    <div className="mt-0.5 truncate font-mono text-[10.5px] text-muted-foreground">
                      {persona.pubkey}
                    </div>
                  </div>
                </button>

                {expanded ? (
                  <div className="px-2 pb-2 pt-1">
                    <div className="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 text-[11px]">
                      <span className="text-muted-foreground">Lamports</span>
                      <span className="font-mono tabular-nums text-foreground/85">
                        {persona.seed.solLamports ?? 0}
                      </span>
                      <span className="text-muted-foreground">Tokens</span>
                      <span className="text-foreground/85">
                        {(persona.seed.tokens ?? [])
                          .map((t) => `${t.symbol ?? t.mint ?? "?"}·${t.amount}`)
                          .join(", ") || "—"}
                      </span>
                      <span className="text-muted-foreground">NFTs</span>
                      <span className="text-foreground/85">
                        {(persona.seed.nfts ?? [])
                          .map((n) => `${n.collection}×${n.count}`)
                          .join(", ") || "—"}
                      </span>
                      {persona.note ? (
                        <>
                          <span className="text-muted-foreground">Note</span>
                          <span className="text-foreground/85">{persona.note}</span>
                        </>
                      ) : null}
                    </div>
                    <div className="mt-2.5 flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => void handleRefund(persona)}
                        disabled={busy || !clusterRunning}
                        className="inline-flex items-center gap-1 rounded-md border border-primary/40 bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
                      >
                        <Sparkles className="h-3 w-3" />
                        Re-fund
                      </button>
                      <button
                        type="button"
                        onClick={() => void onDelete(persona.name)}
                        disabled={busy}
                        className="inline-flex items-center gap-1 rounded-md border border-border/60 bg-background/40 px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:border-destructive/60 hover:text-destructive disabled:opacity-50"
                      >
                        <Trash2 className="h-3 w-3" />
                        Delete
                      </button>
                    </div>
                  </div>
                ) : null}
              </li>
            )
          })}
        </ul>
      )}

      {lastReceipt ? (
        <div className="border-t border-border/50 pt-3">
          <div className="text-[11px] font-medium text-foreground/85">
            Last funding receipt
            <span className="ml-1 font-normal text-muted-foreground">
              · {lastReceipt.persona}
            </span>
          </div>
          <ul className="mt-1.5 flex flex-col gap-0.5 text-[11px]">
            {lastReceipt.steps.map((step, idx) => (
              <li key={idx} className={cn(step.ok ? "text-foreground/85" : "text-destructive")}>
                {describeStep(step)}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  )
}

function describeStep(step: FundingReceipt["steps"][number]): string {
  switch (step.kind) {
    case "airdrop":
      return `airdrop ${step.lamports} lamports${step.ok ? " ✓" : ` ✗ ${step.error ?? ""}`}`
    case "tokenMint":
      return `mint ${step.amount} of ${short(step.mint)}${step.ok ? " ✓" : ` ✗ ${step.error ?? ""}`}`
    case "tokenTransfer":
      return `transfer ${step.amount} of ${short(step.mint)}${step.ok ? " ✓" : ` ✗ ${step.error ?? ""}`}`
    case "nftFixture":
      return `nft ${step.collection}${step.ok ? " ✓" : ` ✗ ${step.error ?? ""}`}`
    default:
      return "unknown step"
  }
}

function short(value: string): string {
  return value.length > 10 ? `${value.slice(0, 4)}…${value.slice(-4)}` : value
}

function countFailures(receipt: FundingReceipt): number {
  return receipt.steps.filter((s) => !s.ok).length
}

function formatLamports(lamports: number): string {
  const sol = lamports / 1_000_000_000
  if (sol >= 1) return `${sol.toLocaleString("en-US", { maximumFractionDigits: 2 })}`
  return sol.toFixed(3)
}

function clusterLabel(cluster: ClusterKind): string {
  switch (cluster) {
    case "localnet":
      return "localnet"
    case "mainnet_fork":
      return "forked mainnet"
    case "devnet":
      return "devnet"
    case "mainnet":
      return "mainnet"
  }
}
