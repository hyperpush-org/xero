"use client"

import { useMemo, useState } from "react"
import { AlertTriangle, CheckCircle2, Coins, Image, Loader2 } from "lucide-react"
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
  ExtensionMatrix,
  MetaplexMintRequest,
  MetaplexMintResult,
  MetaplexStandard,
  TokenCreateReport,
  TokenCreateSpec,
  TokenExtension,
  TokenExtensionConfig,
  TokenSupportLevel,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaTokenPanelProps {
  cluster: ClusterKind
  clusterRunning: boolean
  busy: boolean
  personaNames: string[]
  matrix: ExtensionMatrix | null
  lastTokenCreate: TokenCreateReport | null
  lastMetaplexMint: MetaplexMintResult | null
  onCreateToken: (spec: TokenCreateSpec) => Promise<TokenCreateReport | null>
  onMintMetaplex: (
    request: MetaplexMintRequest,
  ) => Promise<MetaplexMintResult | null>
}

type Tab = "matrix" | "create" | "mint"

const SUPPORT_LEVEL_COLORS: Record<TokenSupportLevel, string> = {
  full: "text-emerald-400 border-emerald-400/30 bg-emerald-400/10",
  partial: "text-amber-400 border-amber-400/30 bg-amber-400/10",
  unsupported: "text-rose-400 border-rose-400/30 bg-rose-400/10",
  unknown: "text-muted-foreground border-border/60 bg-background/40",
}

export function SolanaTokenPanel({
  cluster,
  clusterRunning,
  busy,
  personaNames,
  matrix,
  lastTokenCreate,
  lastMetaplexMint,
  onCreateToken,
  onMintMetaplex,
}: SolanaTokenPanelProps) {
  const [activeTab, setActiveTab] = useState<Tab>("matrix")

  return (
    <div className="flex flex-col gap-3">
      <div role="tablist" className="flex gap-0.5 border-b border-border/60">
        <TabButton
          icon={Coins}
          label="Extensions"
          active={activeTab === "matrix"}
          onClick={() => setActiveTab("matrix")}
        />
        <TabButton
          icon={Coins}
          label="Create"
          active={activeTab === "create"}
          onClick={() => setActiveTab("create")}
        />
        <TabButton
          icon={Image}
          label="NFT"
          active={activeTab === "mint"}
          onClick={() => setActiveTab("mint")}
        />
      </div>

      {activeTab === "matrix" ? <MatrixView matrix={matrix} /> : null}

      {activeTab === "create" ? (
        <CreateTokenForm
          cluster={cluster}
          busy={busy}
          clusterRunning={clusterRunning}
          personaNames={personaNames}
          matrix={matrix}
          lastReport={lastTokenCreate}
          onSubmit={onCreateToken}
        />
      ) : null}

      {activeTab === "mint" ? (
        <MetaplexForm
          cluster={cluster}
          busy={busy}
          clusterRunning={clusterRunning}
          personaNames={personaNames}
          lastResult={lastMetaplexMint}
          onSubmit={onMintMetaplex}
        />
      ) : null}
    </div>
  )
}

function TabButton({
  icon: Icon,
  label,
  active,
  onClick,
}: {
  icon: React.ComponentType<{ className?: string }>
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] transition-colors",
        active
          ? "text-foreground border-b-[1.5px] border-primary"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      {label}
    </button>
  )
}

function MatrixView({ matrix }: { matrix: ExtensionMatrix | null }) {
  if (!matrix) {
    return (
      <p className="text-[11.5px] text-muted-foreground">
        Loading Token-2022 extension matrix…
      </p>
    )
  }
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[11px] text-muted-foreground">
        Manifest version <code>{matrix.manifestVersion}</code>, generated{" "}
        <code>{matrix.generatedAt}</code>.
      </p>
      <ul className="flex flex-col gap-1.5">
        {matrix.entries.map((entry) => (
          <li
            key={entry.extension}
            className="rounded-md border border-border/70 bg-background/40 p-2.5"
          >
            <div className="flex items-center justify-between gap-2">
              <div className="flex flex-col">
                <span className="text-[12px] font-medium text-foreground">
                  {entry.label}
                </span>
                <span className="text-[10.5px] font-mono text-muted-foreground">
                  {entry.extension}
                </span>
              </div>
              <span className="text-[10px] text-muted-foreground">
                {entry.requiresProgram}
              </span>
            </div>
            <p className="mt-1.5 text-[11px] text-foreground/80">{entry.summary}</p>
            <ul className="mt-2 grid grid-cols-1 gap-1">
              {entry.sdkSupport.map((sdk) => (
                <li
                  key={`${entry.extension}-${sdk.sdk}-${sdk.versionRange}`}
                  className={cn(
                    "flex items-start justify-between gap-2 rounded border px-2 py-1 text-[10.5px]",
                    SUPPORT_LEVEL_COLORS[sdk.supportLevel],
                  )}
                >
                  <div className="flex min-w-0 flex-col">
                    <span className="truncate font-medium">{sdk.sdk}</span>
                    <span className="truncate text-[10px] font-mono opacity-80">
                      {sdk.versionRange}
                    </span>
                    {sdk.remediationHint ? (
                      <span className="mt-0.5 text-[10px] leading-snug opacity-90">
                        {sdk.remediationHint}
                      </span>
                    ) : null}
                  </div>
                  <span className="shrink-0 text-[9.5px] uppercase tracking-wide">
                    {sdk.supportLevel}
                  </span>
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </div>
  )
}

function CreateTokenForm({
  cluster,
  busy,
  clusterRunning,
  personaNames,
  matrix,
  lastReport,
  onSubmit,
}: {
  cluster: ClusterKind
  busy: boolean
  clusterRunning: boolean
  personaNames: string[]
  matrix: ExtensionMatrix | null
  lastReport: TokenCreateReport | null
  onSubmit: (spec: TokenCreateSpec) => Promise<TokenCreateReport | null>
}) {
  const [authority, setAuthority] = useState<string>(personaNames[0] ?? "")
  const [decimals, setDecimals] = useState<number>(6)
  const [extensions, setExtensions] = useState<TokenExtension[]>([])
  const [transferFeeBps, setTransferFeeBps] = useState<number>(25)
  const [transferFeeMax, setTransferFeeMax] = useState<number>(1_000_000)
  const [interestBps, setInterestBps] = useState<number>(250)
  const [hookProgram, setHookProgram] = useState<string>("")

  const availableExtensions = useMemo(
    () => (matrix ? matrix.entries.map((e) => e.extension) : []),
    [matrix],
  )

  const toggleExtension = (ext: TokenExtension) => {
    setExtensions((current) =>
      current.includes(ext) ? current.filter((e) => e !== ext) : [...current, ext],
    )
  }

  const submit = async () => {
    if (!authority) return
    const config: TokenExtensionConfig = {}
    if (extensions.includes("transfer_fee")) {
      config.transferFeeBasisPoints = transferFeeBps
      config.transferFeeMaximum = transferFeeMax
    }
    if (extensions.includes("interest_bearing")) {
      config.interestRateBps = interestBps
    }
    if (extensions.includes("transfer_hook")) {
      config.transferHookProgramId = hookProgram
    }
    await onSubmit({
      cluster,
      program: "spl_token_2022",
      authorityPersona: authority,
      decimals,
      extensions,
      config,
    })
  }

  return (
    <div className="flex flex-col gap-2.5">
      {!clusterRunning ? (
        <p className="text-[11px] text-amber-400">
          Validator is not running — start the cluster before creating a token.
        </p>
      ) : null}
      <Field label="Authority persona">
        <Select
          value={authority}
          onValueChange={setAuthority}
          disabled={personaNames.length === 0}
        >
          <SelectTrigger
            aria-label="Authority persona"
            className="h-7 w-full border-border/70 bg-background/40 px-2 text-[11px]"
            size="sm"
          >
            <SelectValue placeholder="No personas yet — create one first" />
          </SelectTrigger>
          <SelectContent>
            {personaNames.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field label="Decimals">
        <input
          type="number"
          min={0}
          max={18}
          value={decimals}
          onChange={(event) =>
            setDecimals(Number.parseInt(event.target.value, 10) || 0)
          }
          className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
        />
      </Field>
      <div>
        <span className="block text-[10px] uppercase tracking-wider text-muted-foreground mb-1">
          Extensions
        </span>
        <div className="flex flex-wrap gap-1.5">
          {availableExtensions.map((ext) => (
            <button
              key={ext}
              type="button"
              onClick={() => toggleExtension(ext)}
              className={cn(
                "rounded border px-2 py-0.5 text-[10.5px]",
                extensions.includes(ext)
                  ? "border-primary/60 bg-primary/10 text-primary"
                  : "border-border/60 bg-background/40 text-foreground/75",
              )}
            >
              {ext}
            </button>
          ))}
        </div>
      </div>
      {extensions.includes("transfer_fee") ? (
        <div className="grid grid-cols-2 gap-2">
          <Field label="Fee bps">
            <input
              type="number"
              min={0}
              max={10000}
              value={transferFeeBps}
              onChange={(event) =>
                setTransferFeeBps(Number.parseInt(event.target.value, 10) || 0)
              }
              className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
            />
          </Field>
          <Field label="Fee max">
            <input
              type="number"
              min={0}
              value={transferFeeMax}
              onChange={(event) =>
                setTransferFeeMax(Number.parseInt(event.target.value, 10) || 0)
              }
              className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
            />
          </Field>
        </div>
      ) : null}
      {extensions.includes("interest_bearing") ? (
        <Field label="Interest rate bps / year">
          <input
            type="number"
            value={interestBps}
            onChange={(event) =>
              setInterestBps(Number.parseInt(event.target.value, 10) || 0)
            }
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
          />
        </Field>
      ) : null}
      {extensions.includes("transfer_hook") ? (
        <Field label="Hook program id">
          <input
            value={hookProgram}
            onChange={(event) => setHookProgram(event.target.value)}
            placeholder="Program public key"
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
          />
        </Field>
      ) : null}
      <button
        type="button"
        onClick={() => void submit()}
        disabled={busy || !authority || !clusterRunning}
        className={cn(
          "mt-1 inline-flex items-center justify-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-3 py-1.5 text-[11.5px] font-medium text-primary",
          "hover:bg-primary/25 disabled:opacity-50",
        )}
      >
        {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <Coins className="h-3 w-3" />}
        Create token
      </button>

      {lastReport ? <TokenReport report={lastReport} /> : null}
    </div>
  )
}

function TokenReport({ report }: { report: TokenCreateReport }) {
  return (
    <div className="rounded border border-border/70 bg-background/40 p-2.5 text-[11px]">
      <div className="flex items-center gap-1.5">
        {report.success ? (
          <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
        ) : (
          <AlertTriangle className="h-3.5 w-3.5 text-rose-400" />
        )}
        <span className="font-medium">
          {report.success ? "Token created" : "spl-token create-token failed"}
        </span>
      </div>
      {report.mintAddress ? (
        <p className="mt-1 font-mono text-[10.5px]">
          Mint: {report.mintAddress}
        </p>
      ) : null}
      <p className="mt-1 text-muted-foreground">
        exit {report.exitCode ?? "?"} · {report.elapsedMs}ms
      </p>
      {report.incompatibilities.length > 0 ? (
        <div className="mt-2 space-y-1">
          <p className="text-[10.5px] font-medium text-amber-400">
            {report.incompatibilities.length} SDK incompatibility
            {report.incompatibilities.length === 1 ? "" : "s"}:
          </p>
          <ul className="flex flex-col gap-1">
            {report.incompatibilities.map((hit, idx) => (
              <li
                key={`${hit.extension}-${hit.sdk}-${idx}`}
                className="rounded border border-amber-400/30 bg-amber-400/5 px-2 py-1 text-[10.5px]"
              >
                <span className="font-mono">{hit.extension}</span> ·{" "}
                <span>{hit.sdk}</span>{" "}
                <span className="text-muted-foreground">{hit.versionRange}</span>
                {hit.remediationHint ? (
                  <p className="mt-0.5 leading-snug">{hit.remediationHint}</p>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {report.stderrExcerpt ? (
        <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded bg-black/30 p-1.5 font-mono text-[10.5px] text-rose-300/90">
          {report.stderrExcerpt}
        </pre>
      ) : null}
    </div>
  )
}

function MetaplexForm({
  cluster,
  busy,
  clusterRunning,
  personaNames,
  lastResult,
  onSubmit,
}: {
  cluster: ClusterKind
  busy: boolean
  clusterRunning: boolean
  personaNames: string[]
  lastResult: MetaplexMintResult | null
  onSubmit: (
    request: MetaplexMintRequest,
  ) => Promise<MetaplexMintResult | null>
}) {
  const [authority, setAuthority] = useState<string>(personaNames[0] ?? "")
  const [name, setName] = useState<string>("Demo NFT")
  const [symbol, setSymbol] = useState<string>("DEMO")
  const [uri, setUri] = useState<string>("https://example.com/meta.json")
  const [recipient, setRecipient] = useState<string>("")
  const [collection, setCollection] = useState<string>("")
  const [sellerFeeBps, setSellerFeeBps] = useState<number>(500)
  const [standard, setStandard] = useState<MetaplexStandard>("non_fungible")

  const submit = async () => {
    await onSubmit({
      cluster,
      authorityPersona: authority,
      name,
      symbol,
      metadataUri: uri,
      recipient: recipient.trim() || null,
      collectionMint: collection.trim() || null,
      sellerFeeBps,
      standard,
    })
  }

  return (
    <div className="flex flex-col gap-2.5">
      {!clusterRunning ? (
        <p className="text-[11px] text-amber-400">
          Validator is not running — start a cluster or point at a remote RPC first.
        </p>
      ) : null}
      <Field label="Authority persona">
        <Select
          value={authority}
          onValueChange={setAuthority}
          disabled={personaNames.length === 0}
        >
          <SelectTrigger
            aria-label="Authority persona"
            className="h-7 w-full border-border/70 bg-background/40 px-2 text-[11px]"
            size="sm"
          >
            <SelectValue placeholder="No personas yet" />
          </SelectTrigger>
          <SelectContent>
            {personaNames.map((p) => (
              <SelectItem key={p} value={p}>
                {p}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <div className="grid grid-cols-2 gap-2">
        <Field label="Name">
          <input
            value={name}
            maxLength={32}
            onChange={(event) => setName(event.target.value)}
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
          />
        </Field>
        <Field label="Symbol">
          <input
            value={symbol}
            maxLength={10}
            onChange={(event) => setSymbol(event.target.value)}
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
          />
        </Field>
      </div>
      <Field label="Metadata URI">
        <input
          value={uri}
          onChange={(event) => setUri(event.target.value)}
          className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
        />
      </Field>
      <div className="grid grid-cols-2 gap-2">
        <Field label="Recipient (optional)">
          <input
            value={recipient}
            onChange={(event) => setRecipient(event.target.value)}
            placeholder="Authority"
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
          />
        </Field>
        <Field label="Collection (optional)">
          <input
            value={collection}
            onChange={(event) => setCollection(event.target.value)}
            placeholder="—"
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 font-mono text-[10.5px]"
          />
        </Field>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Field label="Seller fee bps">
          <input
            type="number"
            min={0}
            max={10000}
            value={sellerFeeBps}
            onChange={(event) =>
              setSellerFeeBps(Number.parseInt(event.target.value, 10) || 0)
            }
            className="w-full rounded border border-border/70 bg-background/40 px-2 py-1 text-[11px]"
          />
        </Field>
        <Field label="Standard">
          <Select
            value={standard}
            onValueChange={(value) => setStandard(value as MetaplexStandard)}
          >
            <SelectTrigger
              aria-label="Standard"
              className="h-7 w-full border-border/70 bg-background/40 px-2 text-[11px]"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="non_fungible">Non-fungible</SelectItem>
              <SelectItem value="programmable_non_fungible">pNFT</SelectItem>
              <SelectItem value="fungible">Fungible</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      </div>
      <button
        type="button"
        onClick={() => void submit()}
        disabled={busy || !authority || !clusterRunning}
        className={cn(
          "mt-1 inline-flex items-center justify-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-3 py-1.5 text-[11.5px] font-medium text-primary",
          "hover:bg-primary/25 disabled:opacity-50",
        )}
      >
        {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <Image className="h-3 w-3" />}
        Mint Metaplex NFT
      </button>

      {lastResult ? (
        <div className="rounded border border-border/70 bg-background/40 p-2.5 text-[11px]">
          <div className="flex items-center gap-1.5">
            {lastResult.success ? (
              <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
            ) : (
              <AlertTriangle className="h-3.5 w-3.5 text-rose-400" />
            )}
            <span className="font-medium">
              {lastResult.success ? "Mint landed" : "Mint failed"}
            </span>
          </div>
          {lastResult.mintAddress ? (
            <p className="mt-1 font-mono text-[10.5px]">
              Mint: {lastResult.mintAddress}
            </p>
          ) : null}
          {lastResult.signature ? (
            <p className="font-mono text-[10.5px] text-muted-foreground">
              Sig: {lastResult.signature}
            </p>
          ) : null}
          <p className="mt-1 text-muted-foreground">
            Worker <code>{lastResult.workerSha256.slice(0, 10)}</code> · exit{" "}
            {lastResult.exitCode ?? "?"} · {lastResult.elapsedMs}ms
          </p>
          {lastResult.stderrExcerpt ? (
            <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded bg-black/30 p-1.5 font-mono text-[10.5px] text-rose-300/90">
              {lastResult.stderrExcerpt}
            </pre>
          ) : null}
        </div>
      ) : null}
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
