"use client"

import { useCallback, useMemo, useState } from "react"
import {
  AlertTriangle,
  CheckCircle2,
  ExternalLink,
  Hammer,
  Loader2,
  Rocket,
  ShieldCheck,
  Sparkles,
  Undo2,
  XCircle,
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
  BuildReport,
  ClusterKind,
  DeployAuthority,
  DeployResult,
  RollbackResult,
  SquadsProposalDescriptor,
  UpgradeSafetyReport,
  UpgradeSafetyVerdict,
  VerifiedBuildResult,
} from "@/src/features/solana/use-solana-workbench"

type AuthorityMode = "direct" | "squads"
const NO_PERSONA = "__none__"

interface SolanaDeployPanelProps {
  cluster: ClusterKind
  clusterRunning: boolean
  busy: boolean
  personaNames: string[]
  lastBuildReport: BuildReport | null
  lastUpgradeSafety: UpgradeSafetyReport | null
  lastDeployResult: DeployResult | null
  lastSquadsProposal: SquadsProposalDescriptor | null
  lastVerifiedBuild: VerifiedBuildResult | null
  lastRollback: RollbackResult | null
  onBuild: (args: { manifestPath: string; profile: "dev" | "release"; program: string | null }) => Promise<BuildReport | null>
  onUpgradeCheck: (args: {
    programId: string
    cluster: ClusterKind
    localSoPath: string
    expectedAuthority: string
    localIdlPath: string | null
  }) => Promise<UpgradeSafetyReport | null>
  onDeploy: (args: {
    programId: string
    cluster: ClusterKind
    soPath: string
    authority: DeployAuthority
    idlPath: string | null
    isFirstDeploy: boolean
  }) => Promise<DeployResult | null>
  onSubmitVerified: (args: {
    programId: string
    cluster: ClusterKind
    manifestPath: string
    githubUrl: string
    commitHash: string | null
    libraryName: string | null
  }) => Promise<VerifiedBuildResult | null>
  onRollback: (args: {
    programId: string
    cluster: ClusterKind
    previousSha256: string
    authority: DeployAuthority
  }) => Promise<RollbackResult | null>
}

export function SolanaDeployPanel({
  cluster,
  clusterRunning,
  busy,
  personaNames,
  lastBuildReport,
  lastUpgradeSafety,
  lastDeployResult,
  lastSquadsProposal,
  lastVerifiedBuild,
  lastRollback,
  onBuild,
  onUpgradeCheck,
  onDeploy,
  onSubmitVerified,
  onRollback,
}: SolanaDeployPanelProps) {
  const [manifestPath, setManifestPath] = useState("")
  const [programFilter, setProgramFilter] = useState("")
  const [profile, setProfile] = useState<"dev" | "release">("release")

  const [programId, setProgramId] = useState("")
  const [soPath, setSoPath] = useState("")
  const [idlPath, setIdlPath] = useState("")

  const [authorityMode, setAuthorityMode] = useState<AuthorityMode>("direct")
  const [persona, setPersona] = useState<string>(personaNames[0] ?? "")
  const [keypairPath, setKeypairPath] = useState("")
  const [multisigPda, setMultisigPda] = useState("")
  const [vaultIndex, setVaultIndex] = useState("0")
  const [creator, setCreator] = useState("")
  const [creatorKeypairPath, setCreatorKeypairPath] = useState("")
  const [memo, setMemo] = useState("")

  const [githubUrl, setGithubUrl] = useState("")
  const [commitHash, setCommitHash] = useState("")
  const [libraryName, setLibraryName] = useState("")

  const [rollbackSha, setRollbackSha] = useState("")

  const [status, setStatus] = useState<string | null>(null)

  const isMainnetTarget = cluster === "mainnet"
  const requiresSquads = isMainnetTarget

  const authority: DeployAuthority | null = useMemo(() => {
    if (authorityMode === "direct") {
      const path = keypairPath.trim()
      if (!path) return null
      return { kind: "direct_keypair", keypairPath: path }
    }
    const ms = multisigPda.trim()
    const c = creator.trim()
    const ckp = creatorKeypairPath.trim()
    if (!ms || !c || !ckp) return null
    const idx = Number.parseInt(vaultIndex, 10)
    return {
      kind: "squads_vault",
      multisigPda: ms,
      vaultIndex: Number.isFinite(idx) ? idx : 0,
      creator: c,
      creatorKeypairPath: ckp,
      spill: null,
      memo: memo.trim() || null,
    }
  }, [authorityMode, keypairPath, multisigPda, vaultIndex, creator, creatorKeypairPath, memo])

  const handleBuild = useCallback(async () => {
    const path = manifestPath.trim()
    if (!path) {
      setStatus("Provide an Anchor.toml or Cargo.toml path.")
      return
    }
    setStatus(null)
    const report = await onBuild({
      manifestPath: path,
      profile,
      program: programFilter.trim() || null,
    })
    if (report) {
      if (report.success && report.artifacts.length > 0) {
        const first = report.artifacts[0]
        setSoPath(first.soPath)
        if (first.idlPath) setIdlPath(first.idlPath)
        setStatus(`Built ${first.program} (${first.soSizeBytes.toLocaleString()} B).`)
      } else if (!report.success) {
        setStatus(`Build failed (exit ${report.exitCode ?? "?"}).`)
      } else {
        setStatus("Build succeeded but produced no .so artifacts.")
      }
    }
  }, [manifestPath, profile, programFilter, onBuild])

  const handleUpgradeCheck = useCallback(async () => {
    const pid = programId.trim()
    const so = soPath.trim()
    if (!pid || !so) {
      setStatus("Program id and built .so path are required.")
      return
    }
    const expected = authorityMode === "direct" ? persona.trim() : multisigPda.trim()
    if (!expected) {
      setStatus("Provide the expected authority (persona pubkey or vault PDA).")
      return
    }
    setStatus(null)
    const report = await onUpgradeCheck({
      programId: pid,
      cluster,
      localSoPath: so,
      expectedAuthority: expected,
      localIdlPath: idlPath.trim() || null,
    })
    if (report) {
      setStatus(verdictMessage(report.verdict))
    }
  }, [authorityMode, cluster, idlPath, multisigPda, persona, programId, soPath, onUpgradeCheck])

  const handleDeploy = useCallback(async () => {
    const pid = programId.trim()
    const so = soPath.trim()
    if (!pid || !so) {
      setStatus("Program id and built .so path are required.")
      return
    }
    const auth = authority
    if (!auth) {
      setStatus("Authority configuration is incomplete.")
      return
    }
    if (lastUpgradeSafety && lastUpgradeSafety.verdict === "block") {
      setStatus("Upgrade safety BLOCK — refuse to deploy. Resolve the issues above.")
      return
    }
    setStatus(null)
    const result = await onDeploy({
      programId: pid,
      cluster,
      soPath: so,
      authority: auth,
      idlPath: idlPath.trim() || null,
      isFirstDeploy: false,
    })
    if (result) {
      if (result.kind === "direct") {
        if (result.outcome.success) {
          setStatus(
            `Deploy landed${result.outcome.signature ? ` (sig ${shorten(result.outcome.signature)})` : ""}.`,
          )
        } else {
          setStatus(`Deploy failed (exit ${result.outcome.exitCode ?? "?"}).`)
        }
      } else {
        setStatus(`Buffer uploaded; Squads proposal ready.`)
      }
    }
  }, [authority, cluster, idlPath, lastUpgradeSafety, programId, soPath, onDeploy])

  const handleSubmitVerified = useCallback(async () => {
    const pid = programId.trim()
    const repo = githubUrl.trim()
    const path = manifestPath.trim()
    if (!pid || !repo || !path) {
      setStatus("Program id, GitHub URL, and manifest path are required for verified builds.")
      return
    }
    setStatus(null)
    const report = await onSubmitVerified({
      programId: pid,
      cluster,
      manifestPath: path,
      githubUrl: repo,
      commitHash: commitHash.trim() || null,
      libraryName: libraryName.trim() || null,
    })
    if (report) {
      setStatus(report.success ? "Verified build submitted." : "Verified build failed.")
    }
  }, [cluster, commitHash, githubUrl, libraryName, manifestPath, programId, onSubmitVerified])

  const handleRollback = useCallback(async () => {
    const pid = programId.trim()
    const sha = rollbackSha.trim()
    if (!pid || sha.length !== 64) {
      setStatus("Program id and a 64-char sha256 are required to rollback.")
      return
    }
    const auth = authority
    if (!auth) {
      setStatus("Authority configuration is incomplete.")
      return
    }
    setStatus(null)
    const result = await onRollback({
      programId: pid,
      cluster,
      previousSha256: sha,
      authority: auth,
    })
    if (result) {
      setStatus(`Rollback restored ${shorten(result.restoredSha256)}.`)
    }
  }, [authority, cluster, programId, rollbackSha, onRollback])

  return (
    <div className="flex flex-col gap-4 text-[12px]">
      <SectionHeader icon={Hammer} label="Build" />
      <Field label="Manifest">
        <input
          aria-label="Manifest path"
          className={inputClass}
          onChange={(e) => setManifestPath(e.target.value)}
          placeholder="path/to/Anchor.toml or Cargo.toml"
          spellCheck={false}
          value={manifestPath}
        />
      </Field>
      <Field label="Program (optional)">
        <input
          aria-label="Program filter"
          className={inputClass}
          onChange={(e) => setProgramFilter(e.target.value)}
          placeholder="my_program (workspace filter)"
          spellCheck={false}
          value={programFilter}
        />
      </Field>
      <Field label="Profile">
        <div className="flex gap-1.5">
          {(["release", "dev"] as const).map((p) => (
            <button
              type="button"
              key={p}
              onClick={() => setProfile(p)}
              className={cn(toggleClass, profile === p && toggleActive)}
            >
              {p}
            </button>
          ))}
        </div>
      </Field>
      <PrimaryButton busy={busy} onClick={handleBuild}>
        <Hammer className="h-3.5 w-3.5" /> Build
      </PrimaryButton>
      {lastBuildReport ? <BuildSummary report={lastBuildReport} /> : null}

      <Divider />
      <SectionHeader icon={ShieldCheck} label="Upgrade safety" />
      <Field label="Program ID">
        <input
          aria-label="Program ID"
          className={inputClass}
          onChange={(e) => setProgramId(e.target.value)}
          placeholder="base58 program id"
          spellCheck={false}
          value={programId}
        />
      </Field>
      <Field label=".so path">
        <input
          aria-label="Built .so path"
          className={inputClass}
          onChange={(e) => setSoPath(e.target.value)}
          placeholder="target/deploy/my_program.so"
          spellCheck={false}
          value={soPath}
        />
      </Field>
      <Field label="IDL (optional)">
        <input
          aria-label="Local IDL path"
          className={inputClass}
          onChange={(e) => setIdlPath(e.target.value)}
          placeholder="target/idl/my_program.json"
          spellCheck={false}
          value={idlPath}
        />
      </Field>

      <SectionHeader icon={Rocket} label="Authority" />
      <div className="flex gap-1.5">
        <button
          type="button"
          onClick={() => setAuthorityMode("direct")}
          disabled={isMainnetTarget}
          className={cn(toggleClass, authorityMode === "direct" && toggleActive, isMainnetTarget && "opacity-50")}
          title={isMainnetTarget ? "Direct keypair deploys are blocked on mainnet." : undefined}
        >
          Direct keypair
        </button>
        <button
          type="button"
          onClick={() => setAuthorityMode("squads")}
          className={cn(toggleClass, authorityMode === "squads" && toggleActive)}
        >
          Squads vault
        </button>
      </div>

      {authorityMode === "direct" ? (
        <>
          <Field label="Persona">
            <Select
              value={persona || NO_PERSONA}
              onValueChange={(value) => setPersona(value === NO_PERSONA ? "" : value)}
            >
              <SelectTrigger aria-label="Persona" className={inputClass} size="sm">
                <SelectValue placeholder="Select persona" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_PERSONA}>Select persona</SelectItem>
                {personaNames.map((name) => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Keypair path">
            <input
              aria-label="Keypair path"
              className={inputClass}
              onChange={(e) => setKeypairPath(e.target.value)}
              placeholder="~/.config/solana/id.json"
              spellCheck={false}
              value={keypairPath}
            />
          </Field>
        </>
      ) : (
        <>
          <Field label="Multisig PDA">
            <input
              aria-label="Multisig PDA"
              className={inputClass}
              onChange={(e) => setMultisigPda(e.target.value)}
              placeholder="base58 Squads multisig"
              spellCheck={false}
              value={multisigPda}
            />
          </Field>
          <Field label="Vault index">
            <input
              aria-label="Vault index"
              className={inputClass}
              inputMode="numeric"
              onChange={(e) => setVaultIndex(e.target.value)}
              value={vaultIndex}
            />
          </Field>
          <Field label="Creator">
            <input
              aria-label="Creator pubkey"
              className={inputClass}
              onChange={(e) => setCreator(e.target.value)}
              placeholder="multisig member pubkey"
              spellCheck={false}
              value={creator}
            />
          </Field>
          <Field label="Creator keypair">
            <input
              aria-label="Creator keypair path"
              className={inputClass}
              onChange={(e) => setCreatorKeypairPath(e.target.value)}
              placeholder="~/.config/solana/creator.json"
              spellCheck={false}
              value={creatorKeypairPath}
            />
          </Field>
          <Field label="Memo (optional)">
            <input
              aria-label="Proposal memo"
              className={inputClass}
              onChange={(e) => setMemo(e.target.value)}
              placeholder="upgrade context for reviewers"
              value={memo}
            />
          </Field>
        </>
      )}

      <div className="flex flex-wrap items-center gap-1.5">
        <PrimaryButton busy={busy} onClick={handleUpgradeCheck}>
          <ShieldCheck className="h-3.5 w-3.5" /> Run safety check
        </PrimaryButton>
        <PrimaryButton
          busy={busy}
          disabled={requiresSquads && authorityMode !== "squads"}
          onClick={handleDeploy}
        >
          <Rocket className="h-3.5 w-3.5" /> Deploy
        </PrimaryButton>
      </div>
      {requiresSquads ? (
        <p className="text-[11px] text-amber-400">
          Mainnet deploys require a Squads vault authority — the workbench refuses direct keypair deploys to mainnet.
        </p>
      ) : null}

      {lastUpgradeSafety ? <SafetySummary report={lastUpgradeSafety} /> : null}
      {lastDeployResult ? <DeploySummary result={lastDeployResult} /> : null}
      {lastSquadsProposal ? <SquadsSummary proposal={lastSquadsProposal} /> : null}

      <Divider />
      <SectionHeader icon={Sparkles} label="Verified build" />
      <Field label="GitHub URL">
        <input
          aria-label="GitHub repo URL"
          className={inputClass}
          onChange={(e) => setGithubUrl(e.target.value)}
          placeholder="https://github.com/owner/repo"
          spellCheck={false}
          value={githubUrl}
        />
      </Field>
      <Field label="Commit (optional)">
        <input
          aria-label="Commit hash"
          className={inputClass}
          onChange={(e) => setCommitHash(e.target.value)}
          placeholder="abc1234"
          spellCheck={false}
          value={commitHash}
        />
      </Field>
      <Field label="Library (optional)">
        <input
          aria-label="Library name"
          className={inputClass}
          onChange={(e) => setLibraryName(e.target.value)}
          placeholder="my_program"
          spellCheck={false}
          value={libraryName}
        />
      </Field>
      <PrimaryButton busy={busy} onClick={handleSubmitVerified}>
        <Sparkles className="h-3.5 w-3.5" /> Submit verified build
      </PrimaryButton>
      {lastVerifiedBuild ? <VerifiedSummary report={lastVerifiedBuild} /> : null}

      <Divider />
      <SectionHeader icon={Undo2} label="Rollback" />
      <Field label="Previous .so sha256">
        <input
          aria-label="Previous sha256"
          className={inputClass}
          onChange={(e) => setRollbackSha(e.target.value)}
          placeholder="64-char hex (from a prior deploy archive)"
          spellCheck={false}
          value={rollbackSha}
        />
      </Field>
      <PrimaryButton busy={busy} onClick={handleRollback}>
        <Undo2 className="h-3.5 w-3.5" /> Rollback
      </PrimaryButton>
      {lastRollback ? (
        <p className="text-[11px] text-muted-foreground">
          Restored {shorten(lastRollback.restoredSha256)}
        </p>
      ) : null}

      {status ? <p className="text-[11.5px] text-foreground/85">{status}</p> : null}

      {!clusterRunning && cluster !== "mainnet" && cluster !== "devnet" ? (
        <p className="text-[11px] text-muted-foreground">
          Local cluster is stopped — start it from the Validator section before deploying.
        </p>
      ) : null}
    </div>
  )
}

const inputClass =
  "w-full rounded-md border border-border/70 bg-background/60 px-2 py-1 text-[12px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/60 focus:outline-none"

const toggleClass =
  "rounded-md border border-border/70 bg-background/40 px-2 py-1 text-[11px] text-foreground/85 transition-colors hover:border-primary/40"

const toggleActive = "border-primary/50 bg-primary/15 text-primary hover:border-primary/60"

function PrimaryButton({
  busy,
  disabled,
  onClick,
  children,
}: {
  busy: boolean
  disabled?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      disabled={busy || disabled}
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50",
      )}
    >
      {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
      {children}
    </button>
  )
}

function SectionHeader({
  icon: Icon,
  label,
}: {
  icon: React.ComponentType<{ className?: string }>
  label: string
}) {
  return (
    <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
      <Icon className="h-3 w-3 text-primary/80" /> {label}
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</span>
      {children}
    </label>
  )
}

function Divider() {
  return <div className="h-px w-full bg-border/40" />
}

function BuildSummary({ report }: { report: BuildReport }) {
  return (
    <div className="rounded-md border border-border/60 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5 font-medium">
          {report.success ? (
            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
          ) : (
            <XCircle className="h-3.5 w-3.5 text-destructive" />
          )}
          {report.kind === "anchor" ? "anchor build" : "cargo build-sbf"}
          <span className="text-muted-foreground">· {report.profile}</span>
        </div>
        <span className="font-mono text-[10.5px] text-muted-foreground">
          {report.elapsedMs.toLocaleString()} ms
        </span>
      </div>
      {report.artifacts.length > 0 ? (
        <ul className="mt-1 flex flex-col divide-y divide-border/40">
          {report.artifacts.map((art) => (
            <li key={art.soPath} className="py-1">
              <div className="font-medium text-foreground">{art.program}</div>
              <div className="font-mono text-[10.5px] text-muted-foreground">
                {art.soSizeBytes.toLocaleString()} B · sha {art.soSha256.slice(0, 12)}…
              </div>
              <div className="truncate font-mono text-[10.5px] text-muted-foreground">
                {art.soPath}
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className="mt-1 text-[11px] text-muted-foreground">No .so artifacts produced.</p>
      )}
      {!report.success && report.stderrExcerpt ? (
        <pre className="mt-2 max-h-24 overflow-y-auto rounded bg-background/60 p-1.5 font-mono text-[10px] text-destructive">
          {report.stderrExcerpt}
        </pre>
      ) : null}
    </div>
  )
}

function SafetySummary({ report }: { report: UpgradeSafetyReport }) {
  const badge = verdictBadge(report.verdict)
  return (
    <div className="rounded-md border border-border/60 bg-background/30 p-2 text-[11px]">
      <div className="mb-2 flex items-center gap-1.5">
        {badge.icon}
        <span className={cn("font-semibold", badge.color)}>{badge.label}</span>
      </div>
      <Row label="Authority" detail={report.authority.detail} />
      <Row label="Size" detail={report.size.detail} />
      <Row
        label="Layout"
        detail={report.layout.detail}
        emphasised={!!(report.layout.drift && report.layout.drift.breakingCount > 0)}
      />
      {report.breakingChanges.length > 0 ? (
        <div className="mt-2">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-destructive">
            Breaking changes
          </div>
          <ul className="mt-1 space-y-1">
            {report.breakingChanges.slice(0, 8).map((c, i) => (
              <li key={`${c.path}-${i}`} className="font-mono text-[10.5px]">
                <span className="text-destructive">{c.kind}</span>
                <span className="text-muted-foreground"> · {c.path}</span>
                <div className="text-foreground/85">{c.detail}</div>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  )
}

function DeploySummary({ result }: { result: DeployResult }) {
  if (result.kind === "direct") {
    return (
      <div className="rounded-md border border-border/60 bg-background/30 p-2 text-[11px]">
        <div className="flex items-center gap-1.5 font-medium">
          {result.outcome.success ? (
            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
          ) : (
            <XCircle className="h-3.5 w-3.5 text-destructive" />
          )}
          Direct deploy · {result.cluster}
        </div>
        {result.outcome.signature ? (
          <div className="mt-1 font-mono text-[10.5px] text-muted-foreground">
            sig {shorten(result.outcome.signature)}
          </div>
        ) : null}
        {result.idlPublish ? (
          <div className="mt-1 text-[10.5px] text-foreground/85">
            IDL {result.idlPublish.mode}{" "}
            {result.idlPublish.success ? "published" : `failed (exit ${result.idlPublish.exitCode ?? "?"})`}
          </div>
        ) : null}
        {result.codama ? (
          <div className="text-[10.5px] text-foreground/85">
            Codama: {result.codama.allSucceeded ? "ok" : "failed"} ({result.codama.targets.length} target/s)
          </div>
        ) : null}
        {result.archive ? (
          <div className="font-mono text-[10.5px] text-muted-foreground">
            archive {shorten(result.archive.sha256)} · {result.archive.sizeBytes.toLocaleString()} B
          </div>
        ) : null}
        {!result.outcome.success && result.outcome.stderrExcerpt ? (
          <pre className="mt-2 max-h-24 overflow-y-auto rounded bg-background/60 p-1.5 font-mono text-[10px] text-destructive">
            {result.outcome.stderrExcerpt}
          </pre>
        ) : null}
      </div>
    )
  }
  return (
    <div className="rounded-md border border-border/60 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center gap-1.5 font-medium">
        <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
        Squads-pending · {result.cluster}
      </div>
      <div className="mt-1 font-mono text-[10.5px] text-muted-foreground">
        buffer {result.bufferWrite.bufferAddress ?? "(unknown)"}
      </div>
      <div className="mt-1 text-foreground/85">{result.proposal.summary}</div>
    </div>
  )
}

function SquadsSummary({ proposal }: { proposal: SquadsProposalDescriptor }) {
  return (
    <div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-2 text-[11px]">
      <div className="mb-1 flex items-center gap-1.5 font-medium text-amber-200">
        <AlertTriangle className="h-3.5 w-3.5" /> Squads proposal — review &amp; approve
      </div>
      <div className="font-mono text-[10.5px] text-muted-foreground">
        vault {shorten(proposal.vaultPda)} · index {proposal.vaultIndex}
      </div>
      <a
        className="mt-1 inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
        href={proposal.squadsAppUrl}
        target="_blank"
        rel="noreferrer noopener"
      >
        Open in Squads <ExternalLink className="h-3 w-3" />
      </a>
      <details className="mt-2 text-[10.5px]">
        <summary className="cursor-pointer text-muted-foreground">CLI argv</summary>
        <pre className="mt-1 overflow-x-auto rounded bg-background/60 p-1.5 font-mono">
          {proposal.vaultTransactionCreateArgv.join(" \\\n  ")}
        </pre>
        <pre className="mt-1 overflow-x-auto rounded bg-background/60 p-1.5 font-mono">
          {proposal.proposalCreateArgv.join(" \\\n  ")}
        </pre>
      </details>
    </div>
  )
}

function VerifiedSummary({ report }: { report: VerifiedBuildResult }) {
  return (
    <div className="rounded-md border border-border/60 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center gap-1.5 font-medium">
        {report.success ? (
          <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
        ) : (
          <XCircle className="h-3.5 w-3.5 text-destructive" />
        )}
        Verified build {report.success ? "submitted" : "failed"}
      </div>
      {report.programHash ? (
        <div className="mt-1 font-mono text-[10.5px] text-muted-foreground">
          hash {shorten(report.programHash)}
        </div>
      ) : null}
      {report.registryUrl ? (
        <a
          className="mt-1 inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
          href={report.registryUrl}
          target="_blank"
          rel="noreferrer noopener"
        >
          Open in registry <ExternalLink className="h-3 w-3" />
        </a>
      ) : null}
      {!report.success && report.stderrExcerpt ? (
        <pre className="mt-2 max-h-24 overflow-y-auto rounded bg-background/60 p-1.5 font-mono text-[10px] text-destructive">
          {report.stderrExcerpt}
        </pre>
      ) : null}
    </div>
  )
}

function Row({
  label,
  detail,
  emphasised,
}: {
  label: string
  detail: string
  emphasised?: boolean
}) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="w-16 shrink-0 text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span className={cn("min-w-0 flex-1 text-foreground/85", emphasised && "text-destructive")}>
        {detail}
      </span>
    </div>
  )
}

function shorten(value: string): string {
  if (value.length <= 12) return value
  return `${value.slice(0, 6)}…${value.slice(-4)}`
}

function verdictMessage(verdict: UpgradeSafetyVerdict): string {
  switch (verdict) {
    case "ok":
      return "Safety check OK — deploy is safe to proceed."
    case "warn":
      return "Safety check WARN — review the report before deploying."
    case "block":
      return "Safety check BLOCK — deploy will not proceed."
  }
}

function verdictBadge(verdict: UpgradeSafetyVerdict): {
  icon: React.ReactElement
  label: string
  color: string
} {
  switch (verdict) {
    case "ok":
      return {
        icon: <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />,
        label: "OK",
        color: "text-emerald-400",
      }
    case "warn":
      return {
        icon: <AlertTriangle className="h-3.5 w-3.5 text-amber-400" />,
        label: "WARN",
        color: "text-amber-400",
      }
    case "block":
      return {
        icon: <XCircle className="h-3.5 w-3.5 text-destructive" />,
        label: "BLOCK",
        color: "text-destructive",
      }
  }
}
