import { useCallback, useEffect, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

export type ClusterKind = "localnet" | "mainnet_fork" | "devnet" | "mainnet"

export interface ClusterDescriptor {
  kind: ClusterKind
  label: string
  startable: boolean
  defaultRpcUrl: string
}

export type ValidatorPhase =
  | "idle"
  | "booting"
  | "ready"
  | "stopping"
  | "stopped"
  | "error"

export interface ValidatorStatusPayload {
  phase: ValidatorPhase
  kind?: string | null
  rpcUrl?: string | null
  wsUrl?: string | null
  message?: string | null
}

export interface ClusterStatus {
  running: boolean
  kind?: ClusterKind | null
  rpcUrl?: string | null
  wsUrl?: string | null
  ledgerDir?: string | null
  startedAtMs?: number | null
  uptimeS?: number | null
}

export interface ClusterHandle {
  kind: ClusterKind
  rpcUrl: string
  wsUrl: string
  pid?: number | null
  ledgerDir: string
  startedAtMs: number
}

export interface ToolProbe {
  present: boolean
  path?: string | null
  version?: string | null
}

export interface ToolchainStatus {
  solanaCli: ToolProbe
  anchor: ToolProbe
  cargoBuildSbf: ToolProbe
  rust: ToolProbe
  node: ToolProbe
  pnpm: ToolProbe
  surfpool: ToolProbe
  trident: ToolProbe
  codama: ToolProbe
  solanaVerify: ToolProbe
  wsl2?: ToolProbe | null
}

export interface EndpointHealth {
  cluster: ClusterKind
  id: string
  url: string
  label?: string | null
  healthy: boolean
  latencyMs?: number | null
  lastError?: string | null
  lastCheckedMs?: number | null
  consecutiveFailures: number
}

export interface SnapshotMeta {
  id: string
  label: string
  cluster: string
  createdAtMs: number
  accountCount: number
  path: string
}

export type PersonaRole =
  | "whale"
  | "lp"
  | "voter"
  | "liquidator"
  | "new_user"
  | "custom"

export interface TokenAllocation {
  symbol?: string | null
  mint?: string | null
  amount: number
}

export interface NftAllocation {
  collection: string
  count: number
}

export interface FundingDelta {
  solLamports?: number
  tokens?: TokenAllocation[]
  nfts?: NftAllocation[]
}

export interface RolePreset {
  displayLabel: string
  description: string
  lamports: number
  tokens: TokenAllocation[]
  nfts: NftAllocation[]
}

export interface RoleDescriptor {
  id: PersonaRole
  preset: RolePreset
}

export interface Persona {
  name: string
  role: PersonaRole
  cluster: ClusterKind
  pubkey: string
  keypairPath: string
  createdAtMs: number
  seed: FundingDelta
  note?: string | null
}

export type FundingStep =
  | {
      kind: "airdrop"
      signature?: string | null
      lamports: number
      ok: boolean
      error?: string | null
    }
  | {
      kind: "tokenMint"
      mint: string
      amount: number
      signature?: string | null
      ok: boolean
      error?: string | null
    }
  | {
      kind: "tokenTransfer"
      mint: string
      amount: number
      signature?: string | null
      ok: boolean
      error?: string | null
    }
  | {
      kind: "nftFixture"
      collection: string
      mint?: string | null
      signature?: string | null
      ok: boolean
      error?: string | null
    }

export interface FundingReceipt {
  persona: string
  cluster: string
  steps: FundingStep[]
  succeeded: boolean
  startedAtMs: number
  finishedAtMs: number
}

export interface PersonaCreateResponse {
  persona: Persona
  receipt: FundingReceipt
}

export interface PersonaSpec {
  name: string
  cluster: ClusterKind
  role?: PersonaRole
  seedOverride?: FundingDelta | null
  note?: string | null
}

export type ScenarioKind = "self_contained" | "pipeline_required"

export interface ScenarioDescriptor {
  id: string
  label: string
  description: string
  supportedClusters: ClusterKind[]
  requiredClonePrograms: string[]
  requiredRoles: PersonaRole[]
  kind: ScenarioKind
}

export type ScenarioStatus = "succeeded" | "failed" | "pendingPipeline"

export interface ScenarioRun {
  id: string
  cluster: ClusterKind
  persona: string
  status: ScenarioStatus
  signatures: string[]
  steps: string[]
  fundingReceipts: FundingReceipt[]
  pipelineHint?: string | null
  startedAtMs: number
  finishedAtMs: number
}

export interface ScenarioSpec {
  id: string
  cluster: ClusterKind
  persona: string
  params?: unknown
}

export interface PersonaEventPayload {
  kind: "created" | "updated" | "funded" | "deleted" | "imported"
  cluster: string
  name: string
  pubkey?: string | null
  tsMs: number
  message?: string | null
}

export interface ScenarioEventPayload {
  kind: "started" | "progress" | "completed" | "failed" | "pending_pipeline"
  id: string
  cluster: string
  persona: string
  tsMs: number
  message?: string | null
  signatureCount: number
}

// Phase 3 — tx pipeline types.
export type SamplePercentile = "low" | "median" | "high" | "very_high" | "max"

export interface FeeSample {
  slot: number
  prioritizationFee: number
}

export interface PercentileFee {
  percentile: SamplePercentile
  microLamports: number
}

export interface FeeEstimate {
  samples: FeeSample[]
  percentiles: PercentileFee[]
  recommendedMicroLamports: number
  recommendedPercentile: SamplePercentile
  programIds: string[]
  source: string
}

export interface ComputeBudgetPlan {
  computeUnitLimit?: number | null
  computeUnitPriceMicroLamports?: number | null
  rationale: string
}

export interface AccountMetaSpec {
  pubkey: string
  isSigner: boolean
  isWritable: boolean
  label?: string | null
}

export interface CpiResolution {
  programId: string
  programLabel: string
  instruction: string
  accounts: AccountMetaSpec[]
  notes: string[]
}

export type KnownProgramLookup =
  | { outcome: "hit"; resolution: CpiResolution }
  | { outcome: "unknownProgram"; programId: string }
  | {
      outcome: "unknownInstruction"
      programId: string
      programLabel: string
      knownInstructions: string[]
    }

export interface AltCandidate {
  pubkey: string
  contents: string[]
}

export interface AltSuggestion {
  alt: string
  covered: string[]
  missing: string[]
  score: number
}

export interface AltResolveReport {
  addresses: string[]
  suggestions: AltSuggestion[]
  recommended?: string | null
  uncovered: string[]
}

export interface AltCreateResult {
  pubkey: string
  signature?: string | null
  stdout: string
  stderrExcerpt?: string | null
}

export interface AltExtendResult {
  alt: string
  added: string[]
  signature?: string | null
  stdout: string
  stderrExcerpt?: string | null
}

export interface CompiledComputeInstruction {
  programId: string
  dataBase64: string
}

export interface TxPlan {
  feePayerPubkey: string
  signerPubkeys: string[]
  computeBudget: ComputeBudgetPlan
  priorityFee?: FeeEstimate | null
  altReport?: AltResolveReport | null
  rpcUrl: string
  cluster: ClusterKind
  computeBudgetInstructions: CompiledComputeInstruction[]
}

export interface TxSpec {
  cluster: ClusterKind
  feePayerPersona: string
  signerPersonas?: string[]
  programIds?: string[]
  addresses?: string[]
  altCandidates?: AltCandidate[]
  rpcUrl?: string | null
}

export type Commitment = "processed" | "confirmed" | "finalized"

export interface LandingStrategy {
  priorityPercentile?: SamplePercentile
  useJito?: boolean
  commitment?: Commitment
  maxRetries?: number
  confirmationTimeoutS?: number | null
}

export type IdlErrorMap = Record<string, Record<number, string>>

export interface SimulateRequest {
  cluster: ClusterKind
  transactionBase64: string
  rpcUrl?: string | null
  skipReplaceBlockhash?: boolean
  idlErrors?: IdlErrorMap | null
}

export type DecodedLogEntry =
  | {
      kind: "invoke"
      programId: string
      programLabel?: string | null
      depth: number
    }
  | { kind: "success"; programId: string }
  | {
      kind: "failure"
      programId: string
      programLabel?: string | null
      code?: number | null
      idlVariant?: string | null
      raw: string
    }
  | { kind: "log"; programId?: string | null; message: string }
  | { kind: "data"; programId?: string | null; base64: string }
  | {
      kind: "computeUsage"
      programId: string
      consumed: number
      allocated: number
    }
  | { kind: "unparsed"; raw: string }

export interface DecodedLogs {
  entries: DecodedLogEntry[]
  programsInvoked: string[]
  totalComputeUnits: number
}

export interface ErrorDetail {
  programId: string
  programLabel?: string | null
  code?: number | null
  idlVariant?: string | null
  raw: string
}

export interface Explanation {
  ok: boolean
  summary: string
  primaryError?: ErrorDetail | null
  decodedLogs: DecodedLogs
  affectedPrograms: string[]
  computeUnitsTotal: number
}

export interface SimulationResult {
  success: boolean
  err?: unknown
  logs: string[]
  computeUnitsConsumed?: number | null
  returnData?: unknown
  affectedAccounts: string[]
  explanation: Explanation
}

export interface SendRequest {
  cluster: ClusterKind
  signedTransactionBase64: string
  strategy?: LandingStrategy
  rpcUrl?: string | null
  idlErrors?: IdlErrorMap | null
}

export interface ExplainRequest {
  cluster: ClusterKind
  signature: string
  rpcUrl?: string | null
  idlErrors?: IdlErrorMap | null
  commitment?: Commitment
}

export interface TxResult {
  signature: string
  slot?: number | null
  confirmation?: string | null
  err?: unknown
  logs: string[]
  explanation: Explanation
  transportAttempts: number
  jitoBundleId?: string | null
}

export interface TxEventPayload {
  kind: "building" | "simulated" | "sent" | "confirmed" | "failed" | "decoded"
  cluster: string
  signature?: string | null
  summary?: string | null
  tsMs: number
}

// Phase 4 — IDL / PDA types.
export type IdlSource =
  | { kind: "file"; path: string }
  | { kind: "chain"; cluster: ClusterKind; idlAddress: string }
  | { kind: "synthetic" }

export interface Idl {
  value: unknown
  hash: string
  source: IdlSource
  fetchedAtMs: number
}

export type IdlChangePhase = "initial" | "updated" | "removed" | "invalid"

export interface IdlChangedEvent {
  token: string
  path: string
  programId?: string | null
  programName?: string | null
  hash: string
  tsMs: number
  phase: IdlChangePhase
}

export type DriftSeverity = "breaking" | "risky" | "non_breaking"

export interface DriftChange {
  severity: DriftSeverity
  kind: string
  path: string
  detail: string
}

export interface DriftReport {
  localHash: string
  chainHash?: string | null
  identical: boolean
  changes: DriftChange[]
  breakingCount: number
  riskyCount: number
  nonBreakingCount: number
}

export type CodamaTarget = "ts" | "rust" | "umi"

export interface CodamaTargetResult {
  target: CodamaTarget
  outputSubdir: string
  success: boolean
  exitCode?: number | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
}

export interface CodamaGenerationReport {
  idlPath: string
  outputDir: string
  targets: CodamaTargetResult[]
  elapsedMs: number
  allSucceeded: boolean
}

export type IdlPublishMode = "init" | "upgrade"

export type DeployProgressPhase =
  | "planning"
  | "uploading"
  | "finalising"
  | "completed"
  | "failed"

export interface DeployProgressPayload {
  programId: string
  cluster: string
  phase: DeployProgressPhase
  detail: string
  tsMs: number
}

export interface IdlPublishReport {
  programId: string
  cluster: ClusterKind
  mode: IdlPublishMode
  success: boolean
  signature?: string | null
  idlAddress?: string | null
  exitCode?: number | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
  argv: string[]
}

// Phase 5 — program build / upgrade-safety / deploy / Squads / verified-build.
export type BuildKind = "anchor" | "cargo_build_sbf"
export type BuildProfile = "dev" | "release"

export interface BuiltArtifact {
  program: string
  soPath: string
  soSizeBytes: number
  soSha256: string
  idlPath?: string | null
}

export interface BuildReport {
  kind: BuildKind
  profile: BuildProfile
  manifestPath: string
  argv: string[]
  success: boolean
  exitCode?: number | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
  artifacts: BuiltArtifact[]
}

export type AuthorityCheckOutcome =
  | "match"
  | "mismatch"
  | "immutable"
  | "program_not_deployed"
  | "indeterminate"

export type SizeCheckOutcome =
  | "fits"
  | "over_program_data_allocation"
  | "over_absolute_cap"
  | "first_deploy"
  | "indeterminate"

export type UpgradeSafetyVerdict = "ok" | "warn" | "block"

export interface AuthorityCheck {
  outcome: AuthorityCheckOutcome
  expectedAuthority: string
  onChainAuthority?: string | null
  programDataAddress?: string | null
  detail: string
}

export interface SizeCheck {
  outcome: SizeCheckOutcome
  localSoSizeBytes: number
  onChainProgramDataBytes?: number | null
  absoluteCapBytes: number
  detail: string
}

export interface LayoutCheck {
  drift?: DriftReport | null
  skipped: boolean
  detail: string
}

export interface UpgradeSafetyReport {
  programId: string
  cluster: ClusterKind
  verdict: UpgradeSafetyVerdict
  layout: LayoutCheck
  size: SizeCheck
  authority: AuthorityCheck
  breakingChanges: DriftChange[]
}

export type DeployAuthority =
  | { kind: "direct_keypair"; keypairPath: string }
  | {
      kind: "squads_vault"
      multisigPda: string
      vaultIndex?: number | null
      creator: string
      creatorKeypairPath: string
      spill?: string | null
      memo?: string | null
    }

export interface PostDeployOptions {
  publishIdl?: boolean
  idlPublishMode?: IdlPublishMode | null
  runCodama?: boolean
  codamaTargets?: CodamaTarget[]
  codamaOutputDir?: string | null
  archiveArtifact?: boolean
  programArchiveRoot?: string | null
}

export interface DirectDeployOutcome {
  argv: string[]
  success: boolean
  exitCode?: number | null
  signature?: string | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
}

export interface BufferWriteOutcome {
  argv: string[]
  success: boolean
  exitCode?: number | null
  bufferAddress?: string | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
}

export interface ArchiveRecord {
  path: string
  sha256: string
  sizeBytes: number
}

export interface UpgradeInstructionAccount {
  pubkey: string
  isSigner: boolean
  isWritable: boolean
  label: string
}

export interface UpgradeInstruction {
  programId: string
  instructionTag: number
  accounts: UpgradeInstructionAccount[]
  dataHex: string
}

export interface SquadsProposalDescriptor {
  programId: string
  cluster: ClusterKind
  multisigPda: string
  vaultPda: string
  vaultIndex: number
  programDataAddress: string
  upgradeInstruction: UpgradeInstruction
  vaultTransactionCreateArgv: string[]
  proposalCreateArgv: string[]
  squadsAppUrl: string
  summary: string
}

export type DeployResult =
  | {
      kind: "direct"
      programId: string
      cluster: ClusterKind
      outcome: DirectDeployOutcome
      idlPublish?: IdlPublishReport | null
      codama?: CodamaGenerationReport | null
      archive?: ArchiveRecord | null
    }
  | {
      kind: "squads"
      programId: string
      cluster: ClusterKind
      bufferWrite: BufferWriteOutcome
      proposal: SquadsProposalDescriptor
      archive?: ArchiveRecord | null
    }

export interface RollbackResult {
  programId: string
  cluster: ClusterKind
  restoredSha256: string
  deploy: DeployResult
}

export interface VerifiedBuildResult {
  programId: string
  cluster: ClusterKind
  argv: string[]
  success: boolean
  exitCode?: number | null
  programHash?: string | null
  registryUrl?: string | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
}

// PDA types.
export type SeedPart =
  | { kind: "utf8"; value: string }
  | { kind: "pubkey"; value: string }
  | { kind: "base58"; value: string }
  | { kind: "hex"; value: string }
  | { kind: "u64_le"; value: number }
  | { kind: "u32_le"; value: number }
  | { kind: "u8"; value: number }

export interface DerivedAddress {
  pubkey: string
  bump: number
  canonical: boolean
  seedBytes: number[][]
  programId: string
}

export type PdaSiteSeedKind = "find_program_address" | "create_program_address"

export interface PdaSite {
  path: string
  line: number
  column: number
  call: string
  seedExpression: string
  seedKind: PdaSiteSeedKind
  hasLiteralBump: boolean
  hardcodedBump?: number | null
}

export interface ClusterPda {
  cluster: ClusterKind
  pubkey: string
  bump: number
  programId: string
}

export interface BumpAnalysis {
  canonicalBump: number
  isCanonical: boolean
  canonicalPubkey: string
  suppliedPubkey?: string | null
}

export interface StartOpts {
  clonePrograms?: string[]
  cloneAccounts?: string[]
  reset?: boolean
  rpcPort?: number
  wsPort?: number
  bootTimeoutSecs?: number
  seedPersonas?: boolean
  limitLedger?: number
}

export interface UseSolanaWorkbench {
  clusters: ClusterDescriptor[]
  toolchain: ToolchainStatus | null
  toolchainLoading: boolean
  status: ClusterStatus
  lastEvent: ValidatorStatusPayload | null
  rpcHealth: EndpointHealth[]
  snapshots: SnapshotMeta[]
  isStarting: boolean
  isStopping: boolean
  error: string | null
  refreshToolchain: () => Promise<void>
  refreshRpcHealth: () => Promise<void>
  refreshSnapshots: () => Promise<void>
  start: (kind: ClusterKind, opts?: StartOpts) => Promise<ClusterHandle | null>
  stop: () => Promise<void>
  // Phase 2 — personas
  personas: Persona[]
  personaRoles: RoleDescriptor[]
  personaBusy: boolean
  lastPersonaEvent: PersonaEventPayload | null
  refreshPersonas: (cluster: ClusterKind) => Promise<void>
  createPersona: (
    spec: PersonaSpec,
    rpcUrl?: string | null,
  ) => Promise<PersonaCreateResponse | null>
  fundPersona: (
    cluster: ClusterKind,
    name: string,
    delta: FundingDelta,
    rpcUrl?: string | null,
  ) => Promise<FundingReceipt | null>
  deletePersona: (cluster: ClusterKind, name: string) => Promise<boolean>
  // Phase 2 — scenarios
  scenarios: ScenarioDescriptor[]
  lastScenarioRun: ScenarioRun | null
  lastScenarioEvent: ScenarioEventPayload | null
  scenarioBusy: boolean
  refreshScenarios: () => Promise<void>
  runScenario: (spec: ScenarioSpec) => Promise<ScenarioRun | null>
  // Phase 3 — tx pipeline
  txBusy: boolean
  lastTxEvent: TxEventPayload | null
  lastTxPlan: TxPlan | null
  lastSimulation: SimulationResult | null
  lastSend: TxResult | null
  lastExplanation: TxResult | null
  buildTx: (spec: TxSpec) => Promise<TxPlan | null>
  simulateTx: (request: SimulateRequest) => Promise<SimulationResult | null>
  sendTx: (request: SendRequest) => Promise<TxResult | null>
  explainTx: (request: ExplainRequest) => Promise<TxResult | null>
  estimatePriorityFee: (
    cluster: ClusterKind,
    programIds: string[],
    target?: SamplePercentile,
    rpcUrl?: string | null,
  ) => Promise<FeeEstimate | null>
  resolveCpi: (
    programId: string,
    instruction: string,
    args?: Record<string, string | undefined>,
  ) => Promise<KnownProgramLookup | null>
  resolveAlt: (
    addresses: string[],
    candidates?: AltCandidate[],
  ) => Promise<AltResolveReport | null>
  // Phase 4 — IDL / PDA.
  idls: Record<string, Idl>
  idlBusy: boolean
  lastIdlEvent: IdlChangedEvent | null
  lastDriftReport: DriftReport | null
  lastCodamaReport: CodamaGenerationReport | null
  lastPublishReport: IdlPublishReport | null
  lastDeployProgress: DeployProgressPayload | null
  activeIdlWatches: string[]
  loadIdl: (path: string) => Promise<Idl | null>
  fetchIdl: (
    programId: string,
    cluster: ClusterKind,
    rpcUrl?: string | null,
  ) => Promise<Idl | null>
  driftIdl: (
    programId: string,
    cluster: ClusterKind,
    localPath: string,
    rpcUrl?: string | null,
  ) => Promise<DriftReport | null>
  publishIdl: (args: {
    programId: string
    cluster: ClusterKind
    idlPath: string
    authorityPersona: string
    mode: IdlPublishMode
    rpcUrl?: string | null
  }) => Promise<IdlPublishReport | null>
  generateCodama: (
    idlPath: string,
    targets: CodamaTarget[],
    outputDir: string,
  ) => Promise<CodamaGenerationReport | null>
  startIdlWatch: (path: string) => Promise<string | null>
  stopIdlWatch: (token: string) => Promise<boolean>
  // PDA.
  derivePda: (
    programId: string,
    seeds: SeedPart[],
    bump?: number | null,
  ) => Promise<DerivedAddress | null>
  scanPda: (projectRoot: string) => Promise<PdaSite[] | null>
  predictPda: (
    programId: string,
    seeds: SeedPart[],
    clusters: ClusterKind[],
  ) => Promise<ClusterPda[] | null>
  analyseBumpPda: (
    programId: string,
    seeds: SeedPart[],
    bump?: number | null,
  ) => Promise<BumpAnalysis | null>
  // Phase 5 — program build / deploy / upgrade-safety / Squads / verified-build.
  programBusy: boolean
  lastBuildReport: BuildReport | null
  lastUpgradeSafety: UpgradeSafetyReport | null
  lastDeployResult: DeployResult | null
  lastSquadsProposal: SquadsProposalDescriptor | null
  lastVerifiedBuild: VerifiedBuildResult | null
  lastRollback: RollbackResult | null
  buildProgram: (args: {
    manifestPath: string
    profile?: BuildProfile
    kind?: BuildKind | null
    program?: string | null
  }) => Promise<BuildReport | null>
  upgradeCheck: (args: {
    programId: string
    cluster: ClusterKind
    localSoPath: string
    expectedAuthority: string
    localIdlPath?: string | null
    maxProgramSizeBytes?: number | null
    localSoSizeBytes?: number | null
    rpcUrl?: string | null
  }) => Promise<UpgradeSafetyReport | null>
  deployProgram: (args: {
    programId: string
    cluster: ClusterKind
    soPath: string
    authority: DeployAuthority
    idlPath?: string | null
    isFirstDeploy?: boolean
    post?: PostDeployOptions | null
    rpcUrl?: string | null
  }) => Promise<DeployResult | null>
  rollbackProgram: (args: {
    programId: string
    cluster: ClusterKind
    previousSha256: string
    authority: DeployAuthority
    programArchiveRoot?: string | null
    post?: PostDeployOptions | null
    rpcUrl?: string | null
  }) => Promise<RollbackResult | null>
  createSquadsProposal: (args: {
    programId: string
    cluster: ClusterKind
    multisigPda: string
    buffer: string
    spill: string
    creator: string
    vaultIndex?: number | null
    memo?: string | null
  }) => Promise<SquadsProposalDescriptor | null>
  submitVerifiedBuild: (args: {
    programId: string
    cluster: ClusterKind
    manifestPath: string
    githubUrl: string
    commitHash?: string | null
    libraryName?: string | null
    skipRemoteSubmit?: boolean
  }) => Promise<VerifiedBuildResult | null>
}

const SOLANA_VALIDATOR_STATUS_EVENT = "solana:validator:status"
const SOLANA_PERSONA_EVENT = "solana:persona"
const SOLANA_SCENARIO_EVENT = "solana:scenario"
const SOLANA_TX_EVENT = "solana:tx"
const SOLANA_IDL_CHANGED_EVENT = "solana:idl:changed"
const SOLANA_DEPLOY_PROGRESS_EVENT = "solana:deploy:progress"

interface Options {
  /** When false, the hook releases listeners and doesn't probe. */
  active: boolean
}

function tauriInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri()) return Promise.resolve(null)
  return invoke<T>(command, args).catch(() => null)
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === "string" && message.length > 0) return message
  }
  if (typeof error === "string" && error.length > 0) return error
  return "Solana workbench command failed"
}

export function useSolanaWorkbench({ active }: Options): UseSolanaWorkbench {
  const [clusters, setClusters] = useState<ClusterDescriptor[]>([])
  const [toolchain, setToolchain] = useState<ToolchainStatus | null>(null)
  const [toolchainLoading, setToolchainLoading] = useState(false)
  const [status, setStatus] = useState<ClusterStatus>({ running: false })
  const [lastEvent, setLastEvent] = useState<ValidatorStatusPayload | null>(null)
  const [rpcHealth, setRpcHealth] = useState<EndpointHealth[]>([])
  const [snapshots, setSnapshots] = useState<SnapshotMeta[]>([])
  const [isStarting, setIsStarting] = useState(false)
  const [isStopping, setIsStopping] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [personas, setPersonas] = useState<Persona[]>([])
  const [personaRoles, setPersonaRoles] = useState<RoleDescriptor[]>([])
  const [personaBusy, setPersonaBusy] = useState(false)
  const [lastPersonaEvent, setLastPersonaEvent] = useState<PersonaEventPayload | null>(null)

  const [scenarios, setScenarios] = useState<ScenarioDescriptor[]>([])
  const [lastScenarioRun, setLastScenarioRun] = useState<ScenarioRun | null>(null)
  const [lastScenarioEvent, setLastScenarioEvent] = useState<ScenarioEventPayload | null>(null)
  const [scenarioBusy, setScenarioBusy] = useState(false)

  const [txBusy, setTxBusy] = useState(false)
  const [lastTxEvent, setLastTxEvent] = useState<TxEventPayload | null>(null)
  const [lastTxPlan, setLastTxPlan] = useState<TxPlan | null>(null)
  const [lastSimulation, setLastSimulation] = useState<SimulationResult | null>(null)
  const [lastSend, setLastSend] = useState<TxResult | null>(null)
  const [lastExplanation, setLastExplanation] = useState<TxResult | null>(null)

  const [idls, setIdls] = useState<Record<string, Idl>>({})
  const [idlBusy, setIdlBusy] = useState(false)
  const [lastIdlEvent, setLastIdlEvent] = useState<IdlChangedEvent | null>(null)
  const [lastDriftReport, setLastDriftReport] = useState<DriftReport | null>(null)
  const [lastCodamaReport, setLastCodamaReport] =
    useState<CodamaGenerationReport | null>(null)
  const [lastPublishReport, setLastPublishReport] =
    useState<IdlPublishReport | null>(null)
  const [lastDeployProgress, setLastDeployProgress] =
    useState<DeployProgressPayload | null>(null)
  const [activeIdlWatches, setActiveIdlWatches] = useState<string[]>([])

  // Phase 5 — program build / deploy / upgrade-safety / Squads / verified-build.
  const [programBusy, setProgramBusy] = useState(false)
  const [lastBuildReport, setLastBuildReport] = useState<BuildReport | null>(null)
  const [lastUpgradeSafety, setLastUpgradeSafety] =
    useState<UpgradeSafetyReport | null>(null)
  const [lastDeployResult, setLastDeployResult] =
    useState<DeployResult | null>(null)
  const [lastSquadsProposal, setLastSquadsProposal] =
    useState<SquadsProposalDescriptor | null>(null)
  const [lastVerifiedBuild, setLastVerifiedBuild] =
    useState<VerifiedBuildResult | null>(null)
  const [lastRollback, setLastRollback] = useState<RollbackResult | null>(null)

  const activeRef = useRef(active)
  activeRef.current = active

  const cacheIdl = useCallback((idl: Idl) => {
    const keyBase =
      typeof idl.value === "object" && idl.value
        ? ((idl.value as { address?: string }).address ??
            (idl.value as { metadata?: { address?: string } }).metadata?.address ??
            (idl.value as { metadata?: { name?: string } }).metadata?.name ??
            idl.hash)
        : idl.hash
    setIdls((current) => ({ ...current, [keyBase]: idl }))
  }, [])

  const refreshToolchain = useCallback(async () => {
    if (!isTauri()) return
    setToolchainLoading(true)
    try {
      const next = await invoke<ToolchainStatus>("solana_toolchain_status")
      setToolchain(next)
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setToolchainLoading(false)
    }
  }, [])

  const refreshClusters = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<ClusterDescriptor[]>("solana_cluster_list")
    if (next) setClusters(next)
  }, [])

  const refreshStatus = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<ClusterStatus>("solana_cluster_status")
    if (next) setStatus(next)
  }, [])

  const refreshRpcHealth = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<EndpointHealth[]>("solana_rpc_health")
    if (next) setRpcHealth(next)
  }, [])

  const refreshSnapshots = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<SnapshotMeta[]>("solana_snapshot_list")
    if (next) setSnapshots(next)
  }, [])

  const refreshPersonaRoles = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<RoleDescriptor[]>("solana_persona_roles")
    if (next) setPersonaRoles(next)
  }, [])

  const refreshPersonas = useCallback(async (cluster: ClusterKind) => {
    if (!isTauri()) return
    const next = await tauriInvoke<Persona[]>("solana_persona_list", {
      request: { cluster },
    })
    if (next) setPersonas(next)
  }, [])

  const refreshScenarios = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<ScenarioDescriptor[]>("solana_scenario_list")
    if (next) setScenarios(next)
  }, [])

  const createPersona = useCallback(
    async (
      spec: PersonaSpec,
      rpcUrl?: string | null,
    ): Promise<PersonaCreateResponse | null> => {
      if (!isTauri()) return null
      setPersonaBusy(true)
      setError(null)
      try {
        const response = await invoke<PersonaCreateResponse>("solana_persona_create", {
          request: {
            spec,
            rpcUrl: rpcUrl ?? null,
          },
        })
        await refreshPersonas(spec.cluster)
        return response
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setPersonaBusy(false)
      }
    },
    [refreshPersonas],
  )

  const fundPersona = useCallback(
    async (
      cluster: ClusterKind,
      name: string,
      delta: FundingDelta,
      rpcUrl?: string | null,
    ): Promise<FundingReceipt | null> => {
      if (!isTauri()) return null
      setPersonaBusy(true)
      setError(null)
      try {
        const receipt = await invoke<FundingReceipt>("solana_persona_fund", {
          request: {
            cluster,
            name,
            delta,
            rpcUrl: rpcUrl ?? null,
          },
        })
        await refreshPersonas(cluster)
        return receipt
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setPersonaBusy(false)
      }
    },
    [refreshPersonas],
  )

  const deletePersona = useCallback(
    async (cluster: ClusterKind, name: string): Promise<boolean> => {
      if (!isTauri()) return false
      setPersonaBusy(true)
      setError(null)
      try {
        await invoke("solana_persona_delete", {
          request: { cluster, name },
        })
        await refreshPersonas(cluster)
        return true
      } catch (err) {
        setError(errorMessage(err))
        return false
      } finally {
        setPersonaBusy(false)
      }
    },
    [refreshPersonas],
  )

  const runScenario = useCallback(
    async (spec: ScenarioSpec): Promise<ScenarioRun | null> => {
      if (!isTauri()) return null
      setScenarioBusy(true)
      setError(null)
      try {
        const run = await invoke<ScenarioRun>("solana_scenario_run", {
          request: { spec },
        })
        setLastScenarioRun(run)
        await refreshPersonas(spec.cluster)
        return run
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setScenarioBusy(false)
      }
    },
    [refreshPersonas],
  )

  const buildTx = useCallback(async (spec: TxSpec): Promise<TxPlan | null> => {
    if (!isTauri()) return null
    setTxBusy(true)
    setError(null)
    try {
      const plan = await invoke<TxPlan>("solana_tx_build", {
        request: { spec },
      })
      setLastTxPlan(plan)
      return plan
    } catch (err) {
      setError(errorMessage(err))
      return null
    } finally {
      setTxBusy(false)
    }
  }, [])

  const simulateTx = useCallback(
    async (request: SimulateRequest): Promise<SimulationResult | null> => {
      if (!isTauri()) return null
      setTxBusy(true)
      setError(null)
      try {
        const result = await invoke<SimulationResult>("solana_tx_simulate", {
          request: { request },
        })
        setLastSimulation(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setTxBusy(false)
      }
    },
    [],
  )

  const sendTx = useCallback(
    async (request: SendRequest): Promise<TxResult | null> => {
      if (!isTauri()) return null
      setTxBusy(true)
      setError(null)
      try {
        const result = await invoke<TxResult>("solana_tx_send", {
          request: { request },
        })
        setLastSend(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setTxBusy(false)
      }
    },
    [],
  )

  const explainTx = useCallback(
    async (request: ExplainRequest): Promise<TxResult | null> => {
      if (!isTauri()) return null
      setTxBusy(true)
      setError(null)
      try {
        const result = await invoke<TxResult>("solana_tx_explain", {
          request: { request },
        })
        setLastExplanation(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setTxBusy(false)
      }
    },
    [],
  )

  const estimatePriorityFee = useCallback(
    async (
      cluster: ClusterKind,
      programIds: string[],
      target: SamplePercentile = "median",
      rpcUrl?: string | null,
    ): Promise<FeeEstimate | null> => {
      if (!isTauri()) return null
      return tauriInvoke<FeeEstimate>("solana_priority_fee_estimate", {
        request: {
          cluster,
          programIds,
          target,
          rpcUrl: rpcUrl ?? null,
        },
      })
    },
    [],
  )

  const resolveCpi = useCallback(
    async (
      programId: string,
      instruction: string,
      args?: Record<string, string | undefined>,
    ): Promise<KnownProgramLookup | null> => {
      if (!isTauri()) return null
      return tauriInvoke<KnownProgramLookup>("solana_cpi_resolve", {
        request: {
          programId,
          instruction,
          args: args ?? {},
        },
      })
    },
    [],
  )

  const resolveAlt = useCallback(
    async (
      addresses: string[],
      candidates: AltCandidate[] = [],
    ): Promise<AltResolveReport | null> => {
      if (!isTauri()) return null
      return tauriInvoke<AltResolveReport>("solana_alt_resolve", {
        request: { addresses, candidates },
      })
    },
    [],
  )

  // Phase 4 — IDL / PDA actions.
  const loadIdl = useCallback(
    async (path: string): Promise<Idl | null> => {
      if (!isTauri()) return null
      setIdlBusy(true)
      setError(null)
      try {
        const idl = await invoke<Idl>("solana_idl_load", {
          request: { path },
        })
        cacheIdl(idl)
        return idl
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIdlBusy(false)
      }
    },
    [cacheIdl],
  )

  const fetchIdl = useCallback(
    async (
      programId: string,
      cluster: ClusterKind,
      rpcUrl?: string | null,
    ): Promise<Idl | null> => {
      if (!isTauri()) return null
      setIdlBusy(true)
      setError(null)
      try {
        const idl = await invoke<Idl | null>("solana_idl_fetch", {
          request: { programId, cluster, rpcUrl: rpcUrl ?? null },
        })
        if (idl) cacheIdl(idl)
        return idl
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIdlBusy(false)
      }
    },
    [cacheIdl],
  )

  const driftIdl = useCallback(
    async (
      programId: string,
      cluster: ClusterKind,
      localPath: string,
      rpcUrl?: string | null,
    ): Promise<DriftReport | null> => {
      if (!isTauri()) return null
      setIdlBusy(true)
      setError(null)
      try {
        const report = await invoke<DriftReport>("solana_idl_drift", {
          request: {
            programId,
            cluster,
            localPath,
            rpcUrl: rpcUrl ?? null,
          },
        })
        setLastDriftReport(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIdlBusy(false)
      }
    },
    [],
  )

  const publishIdl = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      idlPath: string
      authorityPersona: string
      mode: IdlPublishMode
      rpcUrl?: string | null
    }): Promise<IdlPublishReport | null> => {
      if (!isTauri()) return null
      setIdlBusy(true)
      setError(null)
      try {
        const report = await invoke<IdlPublishReport>("solana_idl_publish", {
          request: {
            programId: args.programId,
            cluster: args.cluster,
            idlPath: args.idlPath,
            authorityPersona: args.authorityPersona,
            mode: args.mode,
            rpcUrl: args.rpcUrl ?? null,
          },
        })
        setLastPublishReport(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIdlBusy(false)
      }
    },
    [],
  )

  const generateCodama = useCallback(
    async (
      idlPath: string,
      targets: CodamaTarget[],
      outputDir: string,
    ): Promise<CodamaGenerationReport | null> => {
      if (!isTauri()) return null
      setIdlBusy(true)
      setError(null)
      try {
        const report = await invoke<CodamaGenerationReport>(
          "solana_codama_generate",
          {
            request: { idlPath, targets, outputDir },
          },
        )
        setLastCodamaReport(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIdlBusy(false)
      }
    },
    [],
  )

  const startIdlWatch = useCallback(
    async (path: string): Promise<string | null> => {
      if (!isTauri()) return null
      try {
        const token = await invoke<string>("solana_idl_watch", {
          request: { path },
        })
        setActiveIdlWatches((current) =>
          current.includes(token) ? current : [...current, token],
        )
        return token
      } catch (err) {
        setError(errorMessage(err))
        return null
      }
    },
    [],
  )

  const stopIdlWatch = useCallback(async (token: string): Promise<boolean> => {
    if (!isTauri()) return false
    try {
      const ok = await invoke<boolean>("solana_idl_unwatch", {
        request: { token },
      })
      setActiveIdlWatches((current) => current.filter((t) => t !== token))
      return ok
    } catch (err) {
      setError(errorMessage(err))
      return false
    }
  }, [])

  const derivePda = useCallback(
    async (
      programId: string,
      seeds: SeedPart[],
      bump?: number | null,
    ): Promise<DerivedAddress | null> => {
      if (!isTauri()) return null
      return tauriInvoke<DerivedAddress>("solana_pda_derive", {
        request: { programId, seeds, bump: bump ?? null },
      })
    },
    [],
  )

  const scanPda = useCallback(
    async (projectRoot: string): Promise<PdaSite[] | null> => {
      if (!isTauri()) return null
      return tauriInvoke<PdaSite[]>("solana_pda_scan", {
        request: { projectRoot },
      })
    },
    [],
  )

  const predictPda = useCallback(
    async (
      programId: string,
      seeds: SeedPart[],
      clusters: ClusterKind[],
    ): Promise<ClusterPda[] | null> => {
      if (!isTauri()) return null
      return tauriInvoke<ClusterPda[]>("solana_pda_predict", {
        request: { programId, seeds, clusters },
      })
    },
    [],
  )

  const analyseBumpPda = useCallback(
    async (
      programId: string,
      seeds: SeedPart[],
      bump?: number | null,
    ): Promise<BumpAnalysis | null> => {
      if (!isTauri()) return null
      return tauriInvoke<BumpAnalysis>("solana_pda_analyse_bump", {
        request: { programId, seeds, bump: bump ?? null },
      })
    },
    [],
  )

  const buildProgram = useCallback(
    async (args: {
      manifestPath: string
      profile?: BuildProfile
      kind?: BuildKind | null
      program?: string | null
    }): Promise<BuildReport | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const report = await invoke<BuildReport>("solana_program_build", {
          request: {
            manifestPath: args.manifestPath,
            profile: args.profile ?? "release",
            kind: args.kind ?? null,
            program: args.program ?? null,
          },
        })
        setLastBuildReport(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  const upgradeCheck = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      localSoPath: string
      expectedAuthority: string
      localIdlPath?: string | null
      maxProgramSizeBytes?: number | null
      localSoSizeBytes?: number | null
      rpcUrl?: string | null
    }): Promise<UpgradeSafetyReport | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const report = await invoke<UpgradeSafetyReport>(
          "solana_program_upgrade_check",
          {
            request: {
              programId: args.programId,
              cluster: args.cluster,
              localSoPath: args.localSoPath,
              expectedAuthority: args.expectedAuthority,
              localIdlPath: args.localIdlPath ?? null,
              maxProgramSizeBytes: args.maxProgramSizeBytes ?? null,
              localSoSizeBytes: args.localSoSizeBytes ?? null,
              rpcUrl: args.rpcUrl ?? null,
            },
          },
        )
        setLastUpgradeSafety(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  const deployProgram = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      soPath: string
      authority: DeployAuthority
      idlPath?: string | null
      isFirstDeploy?: boolean
      post?: PostDeployOptions | null
      rpcUrl?: string | null
    }): Promise<DeployResult | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const result = await invoke<DeployResult>("solana_program_deploy", {
          request: {
            programId: args.programId,
            cluster: args.cluster,
            soPath: args.soPath,
            authority: args.authority,
            idlPath: args.idlPath ?? null,
            isFirstDeploy: args.isFirstDeploy ?? false,
            post: args.post ?? null,
            rpcUrl: args.rpcUrl ?? null,
          },
        })
        setLastDeployResult(result)
        if (result.kind === "squads") {
          setLastSquadsProposal(result.proposal)
        }
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  const rollbackProgram = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      previousSha256: string
      authority: DeployAuthority
      programArchiveRoot?: string | null
      post?: PostDeployOptions | null
      rpcUrl?: string | null
    }): Promise<RollbackResult | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const result = await invoke<RollbackResult>("solana_program_rollback", {
          request: {
            programId: args.programId,
            cluster: args.cluster,
            previousSha256: args.previousSha256,
            authority: args.authority,
            programArchiveRoot: args.programArchiveRoot ?? null,
            post: args.post ?? null,
            rpcUrl: args.rpcUrl ?? null,
          },
        })
        setLastRollback(result)
        setLastDeployResult(result.deploy)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  const createSquadsProposal = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      multisigPda: string
      buffer: string
      spill: string
      creator: string
      vaultIndex?: number | null
      memo?: string | null
    }): Promise<SquadsProposalDescriptor | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const descriptor = await invoke<SquadsProposalDescriptor>(
          "solana_squads_proposal_create",
          {
            request: {
              programId: args.programId,
              cluster: args.cluster,
              multisigPda: args.multisigPda,
              buffer: args.buffer,
              spill: args.spill,
              creator: args.creator,
              vaultIndex: args.vaultIndex ?? null,
              memo: args.memo ?? null,
            },
          },
        )
        setLastSquadsProposal(descriptor)
        return descriptor
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  const submitVerifiedBuild = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      manifestPath: string
      githubUrl: string
      commitHash?: string | null
      libraryName?: string | null
      skipRemoteSubmit?: boolean
    }): Promise<VerifiedBuildResult | null> => {
      if (!isTauri()) return null
      setProgramBusy(true)
      setError(null)
      try {
        const report = await invoke<VerifiedBuildResult>(
          "solana_verified_build_submit",
          {
            request: {
              programId: args.programId,
              cluster: args.cluster,
              manifestPath: args.manifestPath,
              githubUrl: args.githubUrl,
              commitHash: args.commitHash ?? null,
              libraryName: args.libraryName ?? null,
              skipRemoteSubmit: args.skipRemoteSubmit ?? false,
            },
          },
        )
        setLastVerifiedBuild(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setProgramBusy(false)
      }
    },
    [],
  )

  // Mount: probe toolchain + cluster catalogue + status + persona catalog.
  useEffect(() => {
    if (!active || !isTauri()) return
    void refreshClusters()
    void refreshToolchain()
    void refreshStatus()
    void refreshSnapshots()
    void refreshPersonaRoles()
    void refreshScenarios()
  }, [
    active,
    refreshClusters,
    refreshToolchain,
    refreshStatus,
    refreshSnapshots,
    refreshPersonaRoles,
    refreshScenarios,
  ])

  // Listen for status events while the sidebar is visible.
  useEffect(() => {
    if (!active || !isTauri()) return
    let cancelled = false
    const unsubs: UnlistenFn[] = []

    void listen<ValidatorStatusPayload>(
      SOLANA_VALIDATOR_STATUS_EVENT,
      (event) => {
        if (cancelled) return
        setLastEvent(event.payload)
        if (event.payload.phase === "ready") {
          setStatus((current) => ({
            running: true,
            kind: (event.payload.kind as ClusterKind | undefined) ?? current.kind ?? null,
            rpcUrl: event.payload.rpcUrl ?? current.rpcUrl ?? null,
            wsUrl: event.payload.wsUrl ?? current.wsUrl ?? null,
            ledgerDir: current.ledgerDir ?? null,
            startedAtMs: current.startedAtMs ?? null,
            uptimeS: current.uptimeS ?? null,
          }))
        }
        if (
          event.payload.phase === "stopped" ||
          event.payload.phase === "idle"
        ) {
          setStatus({ running: false })
        }
        if (event.payload.phase === "error" && event.payload.message) {
          setError(event.payload.message)
        }
      },
    ).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<PersonaEventPayload>(SOLANA_PERSONA_EVENT, (event) => {
      if (cancelled) return
      setLastPersonaEvent(event.payload)
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<ScenarioEventPayload>(SOLANA_SCENARIO_EVENT, (event) => {
      if (cancelled) return
      setLastScenarioEvent(event.payload)
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<TxEventPayload>(SOLANA_TX_EVENT, (event) => {
      if (cancelled) return
      setLastTxEvent(event.payload)
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<IdlChangedEvent>(SOLANA_IDL_CHANGED_EVENT, (event) => {
      if (cancelled) return
      setLastIdlEvent(event.payload)
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<DeployProgressPayload>(
      SOLANA_DEPLOY_PROGRESS_EVENT,
      (event) => {
        if (cancelled) return
        setLastDeployProgress(event.payload)
      },
    ).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    // Nudge the backend to re-emit the current status so the UI syncs.
    void invoke("solana_subscribe_ready").catch(() => {
      /* idempotent no-op */
    })

    return () => {
      cancelled = true
      for (const unsub of unsubs) unsub()
    }
  }, [active])

  const start = useCallback(
    async (kind: ClusterKind, opts?: StartOpts): Promise<ClusterHandle | null> => {
      if (!isTauri()) return null
      setIsStarting(true)
      setError(null)
      try {
        const handle = await invoke<ClusterHandle>("solana_cluster_start", {
          request: { kind, opts: opts ?? {} },
        })
        await refreshStatus()
        await refreshRpcHealth()
        return handle
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIsStarting(false)
      }
    },
    [refreshRpcHealth, refreshStatus],
  )

  const stop = useCallback(async () => {
    if (!isTauri()) return
    setIsStopping(true)
    setError(null)
    try {
      await invoke("solana_cluster_stop")
      await refreshStatus()
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setIsStopping(false)
    }
  }, [refreshStatus])

  return {
    clusters,
    toolchain,
    toolchainLoading,
    status,
    lastEvent,
    rpcHealth,
    snapshots,
    isStarting,
    isStopping,
    error,
    refreshToolchain,
    refreshRpcHealth,
    refreshSnapshots,
    start,
    stop,
    personas,
    personaRoles,
    personaBusy,
    lastPersonaEvent,
    refreshPersonas,
    createPersona,
    fundPersona,
    deletePersona,
    scenarios,
    lastScenarioRun,
    lastScenarioEvent,
    scenarioBusy,
    refreshScenarios,
    runScenario,
    txBusy,
    lastTxEvent,
    lastTxPlan,
    lastSimulation,
    lastSend,
    lastExplanation,
    buildTx,
    simulateTx,
    sendTx,
    explainTx,
    estimatePriorityFee,
    resolveCpi,
    resolveAlt,
    idls,
    idlBusy,
    lastIdlEvent,
    lastDriftReport,
    lastCodamaReport,
    lastPublishReport,
    lastDeployProgress,
    activeIdlWatches,
    loadIdl,
    fetchIdl,
    driftIdl,
    publishIdl,
    generateCodama,
    startIdlWatch,
    stopIdlWatch,
    derivePda,
    scanPda,
    predictPda,
    analyseBumpPda,
    programBusy,
    lastBuildReport,
    lastUpgradeSafety,
    lastDeployResult,
    lastSquadsProposal,
    lastVerifiedBuild,
    lastRollback,
    buildProgram,
    upgradeCheck,
    deployProgram,
    rollbackProgram,
    createSquadsProposal,
    submitVerifiedBuild,
  }
}
