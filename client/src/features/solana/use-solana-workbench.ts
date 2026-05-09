import { useCallback, useEffect, useMemo, useRef, useState } from "react"
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

export type ToolchainComponent = "agave" | "anchor"

export interface ToolchainComponentStatus {
  component: ToolchainComponent
  label: string
  detail: string
  installed: boolean
  installable: boolean
  required: boolean
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
  managedRoot?: string | null
  bundledRoot?: string | null
  installing?: boolean
  installSupported?: boolean
  installableComponents?: ToolchainComponentStatus[]
}

export type ToolchainInstallPhase =
  | "starting"
  | "downloading"
  | "installing"
  | "verifying"
  | "completed"
  | "skipped"
  | "failed"

export interface ToolchainInstallEvent {
  component?: ToolchainComponent | null
  phase: ToolchainInstallPhase
  message?: string | null
  progress?: number | null
  error?: string | null
}

export interface ToolchainInstallStatus {
  inProgress: boolean
  managedRoot: string
  components: ToolchainComponentStatus[]
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

// Phase 6 — audit / fuzz / coverage / replay types.
export type FindingSeverity =
  | "critical"
  | "high"
  | "medium"
  | "low"
  | "informational"

export type FindingSource = "anchor_lints" | "external" | "fuzz" | "replay"

export interface Finding {
  id: string
  source: FindingSource
  ruleId: string
  severity: FindingSeverity
  title: string
  message: string
  file?: string | null
  line?: number | null
  column?: number | null
  fixHint?: string | null
  referenceUrl?: string | null
}

export interface SeverityCounts {
  critical: number
  high: number
  medium: number
  low: number
  informational: number
}

export type AuditRunKind =
  | "static"
  | "external"
  | "fuzz"
  | "coverage"
  | "replay"

export type AuditEventPhase = "started" | "progress" | "completed" | "failed"

export interface AuditEventPayload {
  runId: string
  kind: AuditRunKind
  phase: AuditEventPhase
  finding?: Finding | null
  message?: string | null
  tsMs: number
}

export type StaticLintRuleId =
  | "missing_signer"
  | "missing_owner_check"
  | "missing_has_one"
  | "unchecked_account_info"
  | "arithmetic_overflow"
  | "realloc_without_rent"
  | "seed_spoof"

export interface AnchorFinding {
  rule: StaticLintRuleId
  file: string
  line: number
  column: number
  snippet: string
  context: string
}

export interface StaticLintReport {
  runId: string
  projectRoot: string
  rules: string[]
  findings: Finding[]
  anchorFindings: AnchorFinding[]
  filesScanned: number
  elapsedMs: number
  severityCounts: SeverityCounts
}

export type AnalyzerKind = "auto" | "sec3" | "soteria" | "aderyn"

export interface ExternalAnalyzerReport {
  runId: string
  analyzer: AnalyzerKind
  analyzerInstalled: boolean
  binaryPath?: string | null
  argv: string[]
  exitCode?: number | null
  elapsedMs: number
  findings: Finding[]
  stdoutExcerpt: string
  stderrExcerpt: string
  summary: string
}

export interface FuzzCrash {
  id: string
  instruction?: string | null
  panicMessage?: string | null
  reproducerArgv: string[]
  backtraceExcerpt: string
}

export interface FuzzReport {
  runId: string
  target: string
  projectRoot: string
  argv: string[]
  exitCode?: number | null
  success: boolean
  durationS: number
  elapsedMs: number
  crashes: FuzzCrash[]
  coverageLines: number
  coverageDelta: number
  findings: Finding[]
  stdoutExcerpt: string
  stderrExcerpt: string
}

export interface TridentHarnessResult {
  root: string
  generatedFiles: string[]
  skippedFiles: string[]
  target: string
}

export interface FunctionCoverage {
  name: string
  line: number
  hits: number
}

export interface LcovRecord {
  file: string
  linesFound: number
  linesHit: number
  functionsFound: number
  functionsHit: number
  branchesFound: number
  branchesHit: number
  functions: FunctionCoverage[]
}

export interface InstructionCoverage {
  instruction: string
  functionsFound: number
  functionsHit: number
  linesFound: number
  linesHit: number
  files: string[]
}

export interface CoverageReport {
  runId: string
  projectRoot: string
  argv: string[]
  exitCode?: number | null
  success: boolean
  elapsedMs: number
  lineCoveragePercent: number
  functionCoveragePercent: number
  totalLinesFound: number
  totalLinesHit: number
  totalFunctionsFound: number
  totalFunctionsHit: number
  files: LcovRecord[]
  instructions: InstructionCoverage[]
  stdoutExcerpt: string
  stderrExcerpt: string
  lcovPath?: string | null
}

export type ExploitKey =
  | "wormhole_sig_skip"
  | "cashio_fake_collateral"
  | "mango_oracle_manip"
  | "nirvana_flash_loan"

export type ReplayStep =
  | { kind: "fork"; slot: number; note: string }
  | { kind: "clone_account"; address: string; note: string }
  | {
      kind: "send_tx"
      label: string
      description: string
      affectedProgram: string
      rationale: string
    }
  | {
      kind: "assert_bad_state"
      description: string
      rationale: string
    }

export interface ExploitDescriptor {
  key: ExploitKey
  slug: string
  title: string
  summary: string
  exploitSlot: number
  referenceUrl: string
  impactedProgram: string
  cloneAccounts: string[]
  clonePrograms: string[]
  steps: ReplayStep[]
  expectedBadState: string
}

export type ReplayOutcome =
  | "expected_bad_state"
  | "mitigated"
  | "unexpected_failure"
  | "inconclusive"

export interface ReplayStepTrace {
  stepIndex: number
  label: string
  success: boolean
  message: string
  signature?: string | null
}

export interface ReplayReport {
  runId: string
  exploit: ExploitKey
  targetProgram: string
  cluster: ClusterKind
  snapshotSlot: number
  outcome: ReplayOutcome
  dryRun: boolean
  steps: ReplayStepTrace[]
  summary: string
  findings: Finding[]
  referenceUrl: string
}

// Phase 7 — logs + indexer.
export interface AnchorEvent {
  programId: string
  eventName?: string | null
  discriminatorHex: string
  payloadBase64: string
  payloadBytesLen: number
}

export interface LogEntry {
  cluster: ClusterKind
  signature: string
  slot?: number | null
  blockTimeS?: number | null
  rawLogs: string[]
  programsInvoked: string[]
  explanation: Explanation
  anchorEvents: AnchorEvent[]
  err?: unknown
  receivedMs: number
}

export interface LogFilter {
  cluster: ClusterKind
  programIds: string[]
  includeDecoded: boolean
}

export interface LogsRecentResponse {
  cluster: ClusterKind
  programIds: string[]
  fetched: number
  entries: LogEntry[]
}

export type LogFeedFilter = "all" | "errors" | "events"
export type LogFeedOrder = "newestFirst" | "chronological"

export interface LogsViewCounts {
  all: number
  errors: number
  events: number
}

export interface LogsViewResponse {
  cluster: ClusterKind
  programIds: string[]
  filter: LogFeedFilter
  order: LogFeedOrder
  limit: number
  totalAvailable: number
  decodedEventCount: number
  counts: LogsViewCounts
  entries: LogEntry[]
}

export interface LogsActiveSubscription {
  token: string
  filter: LogFilter
}

export interface LogRawEventPayload {
  token: string
  entry: LogEntry
}

export interface LogDecodedEventPayload {
  token: string
  signature?: string | null
  cluster: ClusterKind
  slot?: number | null
  programsInvoked: string[]
  anchorEvents: AnchorEvent[]
  explanation: Explanation
  err?: unknown
  receivedMs: number
}

export type IndexerKind = "carbon" | "log_parser" | "helius_webhook"

export interface ScaffoldFile {
  path: string
  bytesWritten: number
  sha256: string
}

export interface ScaffoldResult {
  kind: IndexerKind
  root: string
  projectSlug: string
  programId: string
  programName: string
  files: ScaffoldFile[]
  entrypoint?: string | null
  runHint: string
  startCommand?: string | null
}

export interface ProgramEventCount {
  programId: string
  transactions: number
  anchorEvents: number
}

export interface IndexerRunReport {
  cluster: ClusterKind
  programIds: string[]
  fetchedSignatures: number
  eventsByProgram: ProgramEventCount[]
  entries: LogEntry[]
}

// Phase 8 — token + metaplex + wallet types.
export type TokenExtension =
  | "transfer_fee"
  | "transfer_hook"
  | "metadata_pointer"
  | "token_metadata"
  | "interest_bearing"
  | "non_transferable"
  | "permanent_delegate"
  | "default_account_state"
  | "mint_close_authority"
  | "confidential_transfer"
  | "memo_transfer"
  | "cpi_guard"
  | "immutable_owner"
  | "group_pointer"
  | "group_member_pointer"
  | "scaled_ui_amount"

export type TokenSupportLevel = "full" | "partial" | "unsupported" | "unknown"

export interface SdkCompat {
  sdk: string
  versionRange: string
  supportLevel: TokenSupportLevel
  remediationHint: string
}

export interface ExtensionEntry {
  extension: TokenExtension
  label: string
  summary: string
  requiresProgram: string
  sdkSupport: SdkCompat[]
}

export interface ExtensionMatrix {
  manifestVersion: string
  generatedAt: string
  entries: ExtensionEntry[]
}

export interface Incompatibility {
  extension: TokenExtension
  sdk: string
  versionRange: string
  supportLevel: TokenSupportLevel
  remediationHint: string
}

export type TokenProgram = "spl" | "spl_token_2022"

export interface TokenExtensionConfig {
  transferFeeBasisPoints?: number | null
  transferFeeMaximum?: number | null
  interestRateBps?: number | null
  transferHookProgramId?: string | null
  transferFeeWithdrawAuthority?: string | null
}

export interface TokenCreateSpec {
  cluster: ClusterKind
  program?: TokenProgram
  authorityPersona: string
  decimals: number
  mintKeypairPath?: string | null
  extensions?: TokenExtension[]
  config?: TokenExtensionConfig
  splTokenCli?: string | null
  rpcUrl?: string | null
}

export interface TokenCreateReport {
  cluster: ClusterKind
  program: TokenProgram
  argv: string[]
  success: boolean
  exitCode?: number | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
  mintAddress?: string | null
  extensions: TokenExtension[]
  incompatibilities: Incompatibility[]
}

export type MetaplexStandard = "non_fungible" | "fungible" | "programmable_non_fungible"

export interface MetaplexMintRequest {
  cluster: ClusterKind
  authorityPersona: string
  metadataUri: string
  name: string
  symbol: string
  recipient?: string | null
  collectionMint?: string | null
  sellerFeeBps?: number | null
  standard?: MetaplexStandard
  nodeBin?: string | null
  refreshWorker?: boolean
  rpcUrl?: string | null
}

export interface MetaplexMintResult {
  cluster: ClusterKind
  standard: MetaplexStandard
  argv: string[]
  workerPath: string
  workerSha256: string
  success: boolean
  exitCode?: number | null
  stdoutExcerpt: string
  stderrExcerpt: string
  elapsedMs: number
  mintAddress?: string | null
  signature?: string | null
}

export type WalletKind =
  | "wallet_standard"
  | "privy"
  | "dynamic"
  | "mwa_stub"

export interface WalletDescriptor {
  kind: WalletKind
  label: string
  summary: string
  requiresApiKey: boolean
  supportedClusters: ClusterKind[]
}

export interface WalletScaffoldRequest {
  kind: WalletKind
  outputDir: string
  projectSlug?: string | null
  cluster?: ClusterKind
  rpcUrl?: string | null
  appName?: string | null
  appId?: string | null
  overwrite?: boolean
}

export interface WalletScaffoldFile {
  path: string
  bytesWritten: number
  sha256: string
}

export interface WalletScaffoldResult {
  kind: WalletKind
  root: string
  projectSlug: string
  cluster: ClusterKind
  rpcUrl: string
  appName: string
  files: WalletScaffoldFile[]
  entrypoint?: string | null
  runHint: string
  apiKeyEnv?: string | null
  nextSteps: string[]
}

// -- Phase 9 — safety (secrets / drift / cost / docs) ---------------------

export type SecretSeverity = "critical" | "high" | "medium" | "low"

export type SecretPatternKind =
  | "solana_keypair_json"
  | "regex"
  | "literal_marker"

export interface SecretPattern {
  ruleId: string
  title: string
  severity: SecretSeverity
  kind: SecretPatternKind
  description: string
  pattern: string
  fileGlobs: string[]
  remediation: string
  referenceUrl?: string | null
}

export interface SecretFinding {
  ruleId: string
  title: string
  severity: SecretSeverity
  path: string
  line?: number | null
  evidence: string
  remediation: string
  referenceUrl?: string | null
}

export interface SecretScanReport {
  projectRoot: string
  filesScanned: number
  filesSkipped: number
  durationMs: number
  findings: SecretFinding[]
  blocksDeploy: boolean
  patternsApplied: number
}

export type ScopeWarningKind =
  | "cross_cluster_reuse"
  | "mainnet_label_on_non_mainnet"
  | "suspected_real_key_on_fork_or_devnet"

export interface ScopeWarning {
  kind: ScopeWarningKind
  severity: SecretSeverity
  persona: string
  cluster: ClusterKind
  relatedClusters: ClusterKind[]
  pubkey: string
  message: string
  remediation: string
}

export interface ScopeCheckReport {
  warnings: ScopeWarning[]
  personasInspected: number
  blocksDeploy: boolean
}

export interface TrackedProgram {
  label: string
  programId: string
  description: string
  referenceUrl?: string | null
}

export type DriftStatus =
  | "in_sync"
  | "drift"
  | "partially_deployed"
  | "inconclusive"

export interface DriftProbe {
  cluster: ClusterKind
  rpcUrl: string
  programDataSha256?: string | null
  programDataLength?: number | null
  upgradeAuthority?: string | null
  lastDeployedSlot?: number | null
  error?: string | null
}

export interface DriftEntry {
  program: TrackedProgram
  status: DriftStatus
  probes: DriftProbe[]
  summary: string
}

export interface ClusterDriftReport {
  entries: DriftEntry[]
  clustersChecked: ClusterKind[]
  durationMs: number
  hasDrift: boolean
}

export type ProviderKind =
  | "helius_free"
  | "triton_free"
  | "quick_node_free"
  | "alchemy_free"
  | "solana_public"
  | "localnet"
  | "unknown"

export type ProviderHealth = "healthy" | "degraded" | "unknown"

export interface ProviderUsage {
  cluster: ClusterKind
  endpointId: string
  endpointUrl: string
  kind: ProviderKind
  health: ProviderHealth
  usageAvailable: boolean
  requestsLastWindow?: number | null
  quotaLimit?: number | null
  windowSeconds?: number | null
  warning?: string | null
}

export interface ClusterCostBreakdown {
  cluster: ClusterKind
  txCount: number
  lamportsSpent: number
  computeUnitsUsed: number
  rentLockedLamports: number
}

export interface LocalCostSummary {
  txCount: number
  lamportsSpent: number
  computeUnitsUsed: number
  rentLockedLamports: number
  byCluster: ClusterCostBreakdown[]
}

export interface CostTotals {
  lamportsSpent: number
  computeUnitsUsed: number
  txCount: number
  rentLockedLamports: number
  providersHealthy: number
  providersDegraded: number
}

export interface CostSnapshot {
  generatedAtMs: number
  windowS?: number | null
  clustersIncluded: ClusterKind[]
  local: LocalCostSummary
  providers: ProviderUsage[]
  totals: CostTotals
}

export interface DocSnippet {
  tool: string
  title: string
  referenceUrl: string
  version: number
  body: string
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
  toolchainInstallStatus: ToolchainInstallStatus | null
  toolchainInstallEvent: ToolchainInstallEvent | null
  toolchainInstalling: boolean
  status: ClusterStatus
  lastEvent: ValidatorStatusPayload | null
  rpcHealth: EndpointHealth[]
  snapshots: SnapshotMeta[]
  isStarting: boolean
  isStopping: boolean
  error: string | null
  refreshToolchain: () => Promise<void>
  installToolchain: (components?: ToolchainComponent[]) => Promise<ToolchainInstallStatus | null>
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
  // Phase 6 — audit / fuzz / coverage / replay.
  auditBusy: boolean
  auditFindings: Finding[]
  auditEvents: AuditEventPayload[]
  lastStaticReport: StaticLintReport | null
  lastExternalReport: ExternalAnalyzerReport | null
  lastFuzzReport: FuzzReport | null
  lastCoverageReport: CoverageReport | null
  lastReplayReport: ReplayReport | null
  replayCatalog: ExploitDescriptor[]
  clearAuditFeed: () => void
  refreshReplayCatalog: () => Promise<void>
  runStaticAudit: (args: {
    projectRoot: string
    ruleIds?: string[]
    skipPaths?: string[]
  }) => Promise<StaticLintReport | null>
  runExternalAudit: (args: {
    projectRoot: string
    analyzer?: AnalyzerKind
    timeoutS?: number | null
  }) => Promise<ExternalAnalyzerReport | null>
  runFuzzAudit: (args: {
    projectRoot: string
    target: string
    durationS?: number | null
    corpus?: string | null
    baselineCoverageLines?: number | null
  }) => Promise<FuzzReport | null>
  scaffoldFuzzHarness: (args: {
    projectRoot: string
    target: string
    idlPath?: string | null
    overwrite?: boolean
  }) => Promise<TridentHarnessResult | null>
  runCoverageAudit: (args: {
    projectRoot: string
    package?: string | null
    testFilter?: string | null
    lcovPath?: string | null
    instructionNames?: string[]
    timeoutS?: number | null
  }) => Promise<CoverageReport | null>
  runReplay: (args: {
    exploit: ExploitKey
    targetProgram: string
    cluster: ClusterKind
    dryRun?: boolean
    snapshotSlot?: number | null
    rpcUrl?: string | null
  }) => Promise<ReplayReport | null>
  // Phase 7 — logs + indexer.
  logBusy: boolean
  logEntries: LogEntry[]
  logFeedView: LogsViewResponse | null
  logFeedVersion: number
  decodedLogEvents: LogDecodedEventPayload[]
  activeLogSubscriptions: LogsActiveSubscription[]
  lastLogFetch: LogsRecentResponse | null
  clearLogFeed: () => void
  refreshActiveLogSubscriptions: () => Promise<void>
  subscribeLogs: (filter: LogFilter) => Promise<string | null>
  unsubscribeLogs: (token: string) => Promise<boolean>
  fetchRecentLogs: (args: {
    cluster: ClusterKind
    programIds?: string[]
    lastN?: number
    rpcUrl?: string | null
    cachedOnly?: boolean
  }) => Promise<LogsRecentResponse | null>
  refreshLogView: (args: {
    cluster: ClusterKind
    programIds?: string[]
    filter?: LogFeedFilter
    order?: LogFeedOrder
    limit?: number
  }) => Promise<LogsViewResponse | null>
  indexerBusy: boolean
  lastIndexerScaffold: ScaffoldResult | null
  lastIndexerRun: IndexerRunReport | null
  scaffoldIndexer: (args: {
    kind: IndexerKind
    idlPath: string
    outputDir: string
    projectSlug?: string | null
    overwrite?: boolean
    rpcUrl?: string | null
  }) => Promise<ScaffoldResult | null>
  runIndexer: (args: {
    cluster: ClusterKind
    programIds: string[]
    lastN?: number
    rpcUrl?: string | null
  }) => Promise<IndexerRunReport | null>
  // Phase 8 — token + metaplex + wallet scaffolds.
  tokenBusy: boolean
  extensionMatrix: ExtensionMatrix | null
  lastTokenCreate: TokenCreateReport | null
  lastMetaplexMint: MetaplexMintResult | null
  walletBusy: boolean
  walletDescriptors: WalletDescriptor[]
  lastWalletScaffold: WalletScaffoldResult | null
  refreshExtensionMatrix: () => Promise<void>
  refreshWalletDescriptors: () => Promise<void>
  createToken: (spec: TokenCreateSpec) => Promise<TokenCreateReport | null>
  mintMetaplex: (
    request: MetaplexMintRequest,
  ) => Promise<MetaplexMintResult | null>
  generateWalletScaffold: (
    request: WalletScaffoldRequest,
  ) => Promise<WalletScaffoldResult | null>
  // Phase 9 — safety (secrets / drift / cost / docs).
  safetyBusy: boolean
  secretPatterns: SecretPattern[]
  lastSecretScan: SecretScanReport | null
  lastScopeCheck: ScopeCheckReport | null
  trackedPrograms: TrackedProgram[]
  lastClusterDrift: ClusterDriftReport | null
  lastCostSnapshot: CostSnapshot | null
  docCatalog: DocSnippet[]
  scanSecrets: (args: {
    projectRoot: string
    skipPaths?: string[]
    minSeverity?: SecretSeverity | null
  }) => Promise<SecretScanReport | null>
  refreshSecretPatterns: () => Promise<void>
  runScopeCheck: () => Promise<ScopeCheckReport | null>
  refreshTrackedPrograms: () => Promise<void>
  checkClusterDrift: (args?: {
    additional?: TrackedProgram[]
    clusters?: ClusterKind[]
    skipBuiltins?: boolean
  }) => Promise<ClusterDriftReport | null>
  refreshCostSnapshot: (args?: {
    clusters?: ClusterKind[]
    windowS?: number | null
    skipProviderProbes?: boolean
  }) => Promise<CostSnapshot | null>
  resetCostLedger: () => Promise<void>
  refreshDocCatalog: () => Promise<void>
}

const SOLANA_VALIDATOR_STATUS_EVENT = "solana:validator:status"
const SOLANA_TOOLCHAIN_INSTALL_EVENT = "solana:toolchain:install"
const SOLANA_PERSONA_EVENT = "solana:persona"
const SOLANA_SCENARIO_EVENT = "solana:scenario"
const SOLANA_TX_EVENT = "solana:tx"
const SOLANA_IDL_CHANGED_EVENT = "solana:idl:changed"
const SOLANA_DEPLOY_PROGRESS_EVENT = "solana:deploy:progress"
const SOLANA_AUDIT_EVENT = "solana:audit"
const SOLANA_LOG_EVENT = "solana:log"
const SOLANA_LOG_DECODED_EVENT = "solana:log:decoded"

const MAX_AUDIT_FEED_EVENTS = 200
const MAX_AUDIT_FEED_FINDINGS = 500
const MAX_LOG_FEED_ENTRIES = 500
const MAX_DECODED_LOG_EVENTS = 500

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

function scheduleWorkbenchIdleTask(callback: () => void): () => void {
  if (typeof window === "undefined") {
    return () => {}
  }

  const idleWindow = window as Window & {
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number
    cancelIdleCallback?: (handle: number) => void
  }

  if (typeof idleWindow.requestIdleCallback === "function") {
    const handle = idleWindow.requestIdleCallback(callback, { timeout: 600 })
    return () => idleWindow.cancelIdleCallback?.(handle)
  }

  const handle = window.setTimeout(callback, 80)
  return () => window.clearTimeout(handle)
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
  const [toolchainInstallStatus, setToolchainInstallStatus] =
    useState<ToolchainInstallStatus | null>(null)
  const [toolchainInstallEvent, setToolchainInstallEvent] =
    useState<ToolchainInstallEvent | null>(null)
  const [toolchainInstalling, setToolchainInstalling] = useState(false)
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

  // Phase 6 — audit / fuzz / coverage / replay.
  const [auditBusy, setAuditBusy] = useState(false)
  const [auditFindings, setAuditFindings] = useState<Finding[]>([])
  const [auditEvents, setAuditEvents] = useState<AuditEventPayload[]>([])
  const [lastStaticReport, setLastStaticReport] =
    useState<StaticLintReport | null>(null)
  const [lastExternalReport, setLastExternalReport] =
    useState<ExternalAnalyzerReport | null>(null)
  const [lastFuzzReport, setLastFuzzReport] = useState<FuzzReport | null>(null)
  const [lastCoverageReport, setLastCoverageReport] =
    useState<CoverageReport | null>(null)
  const [lastReplayReport, setLastReplayReport] =
    useState<ReplayReport | null>(null)
  const [replayCatalog, setReplayCatalog] = useState<ExploitDescriptor[]>([])

  // Phase 7 — logs + indexer.
  const [logBusy, setLogBusy] = useState(false)
  const [logEntries, setLogEntries] = useState<LogEntry[]>([])
  const [logFeedView, setLogFeedView] = useState<LogsViewResponse | null>(null)
  const [logFeedVersion, setLogFeedVersion] = useState(0)
  const [decodedLogEvents, setDecodedLogEvents] =
    useState<LogDecodedEventPayload[]>([])
  const [activeLogSubscriptions, setActiveLogSubscriptions] =
    useState<LogsActiveSubscription[]>([])
  const [lastLogFetch, setLastLogFetch] =
    useState<LogsRecentResponse | null>(null)
  const [indexerBusy, setIndexerBusy] = useState(false)
  const [lastIndexerScaffold, setLastIndexerScaffold] =
    useState<ScaffoldResult | null>(null)
  const [lastIndexerRun, setLastIndexerRun] =
    useState<IndexerRunReport | null>(null)

  // Phase 8 — token + wallet state.
  const [tokenBusy, setTokenBusy] = useState(false)
  const [extensionMatrix, setExtensionMatrix] =
    useState<ExtensionMatrix | null>(null)
  const [lastTokenCreate, setLastTokenCreate] =
    useState<TokenCreateReport | null>(null)
  const [lastMetaplexMint, setLastMetaplexMint] =
    useState<MetaplexMintResult | null>(null)
  const [walletBusy, setWalletBusy] = useState(false)
  const [walletDescriptors, setWalletDescriptors] =
    useState<WalletDescriptor[]>([])
  const [lastWalletScaffold, setLastWalletScaffold] =
    useState<WalletScaffoldResult | null>(null)

  // Phase 9 — safety (secrets, drift, cost, docs).
  const [safetyBusy, setSafetyBusy] = useState(false)
  const [secretPatterns, setSecretPatterns] = useState<SecretPattern[]>([])
  const [lastSecretScan, setLastSecretScan] = useState<SecretScanReport | null>(
    null,
  )
  const [lastScopeCheck, setLastScopeCheck] = useState<ScopeCheckReport | null>(
    null,
  )
  const [trackedPrograms, setTrackedPrograms] = useState<TrackedProgram[]>([])
  const [lastDriftReport2, setLastDriftReport2] =
    useState<ClusterDriftReport | null>(null)
  const [lastCostSnapshot, setLastCostSnapshot] = useState<CostSnapshot | null>(
    null,
  )
  const [docCatalog, setDocCatalog] = useState<DocSnippet[]>([])

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
      setToolchainInstalling(Boolean(next.installing))
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setToolchainLoading(false)
    }
  }, [])

  const refreshToolchainInstallStatus = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<ToolchainInstallStatus>(
      "solana_toolchain_install_status",
    )
    if (next) {
      setToolchainInstallStatus(next)
      setToolchainInstalling(next.inProgress)
    }
  }, [])

  const installToolchain = useCallback(
    async (
      components: ToolchainComponent[] = [],
    ): Promise<ToolchainInstallStatus | null> => {
      if (!isTauri()) return null
      setToolchainInstalling(true)
      setError(null)
      try {
        const status = await invoke<ToolchainInstallStatus>(
          "solana_toolchain_install",
          { request: { components } },
        )
        setToolchainInstallStatus(status)
        setToolchainInstalling(status.inProgress)
        await refreshToolchain()
        return status
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        await refreshToolchainInstallStatus()
      }
    },
    [refreshToolchain, refreshToolchainInstallStatus],
  )

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

  const clearAuditFeed = useCallback(() => {
    setAuditFindings([])
    setAuditEvents([])
  }, [])

  const refreshReplayCatalog = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<ExploitDescriptor[]>("solana_replay_list")
    if (next) setReplayCatalog(next)
  }, [])

  const mergeFindings = useCallback((incoming: Finding[]) => {
    if (incoming.length === 0) return
    setAuditFindings((current) => {
      const seen = new Set(current.map((f) => f.id))
      const next = [...current]
      for (const finding of incoming) {
        if (seen.has(finding.id)) continue
        next.push(finding)
        seen.add(finding.id)
      }
      if (next.length > MAX_AUDIT_FEED_FINDINGS) {
        return next.slice(next.length - MAX_AUDIT_FEED_FINDINGS)
      }
      return next
    })
  }, [])

  const runStaticAudit = useCallback(
    async (args: {
      projectRoot: string
      ruleIds?: string[]
      skipPaths?: string[]
    }): Promise<StaticLintReport | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        const report = await invoke<StaticLintReport>("solana_audit_static", {
          request: {
            request: {
              projectRoot: args.projectRoot,
              ruleIds: args.ruleIds ?? [],
              skipPaths: args.skipPaths ?? [],
            },
          },
        })
        setLastStaticReport(report)
        mergeFindings(report.findings)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [mergeFindings],
  )

  const runExternalAudit = useCallback(
    async (args: {
      projectRoot: string
      analyzer?: AnalyzerKind
      timeoutS?: number | null
    }): Promise<ExternalAnalyzerReport | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        const report = await invoke<ExternalAnalyzerReport>(
          "solana_audit_external",
          {
            request: {
              request: {
                projectRoot: args.projectRoot,
                analyzer: args.analyzer ?? "auto",
                timeoutS: args.timeoutS ?? null,
              },
            },
          },
        )
        setLastExternalReport(report)
        mergeFindings(report.findings)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [mergeFindings],
  )

  const runFuzzAudit = useCallback(
    async (args: {
      projectRoot: string
      target: string
      durationS?: number | null
      corpus?: string | null
      baselineCoverageLines?: number | null
    }): Promise<FuzzReport | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        const report = await invoke<FuzzReport>("solana_audit_fuzz", {
          request: {
            request: {
              projectRoot: args.projectRoot,
              target: args.target,
              durationS: args.durationS ?? null,
              corpus: args.corpus ?? null,
              baselineCoverageLines: args.baselineCoverageLines ?? null,
            },
          },
        })
        setLastFuzzReport(report)
        mergeFindings(report.findings)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [mergeFindings],
  )

  const scaffoldFuzzHarness = useCallback(
    async (args: {
      projectRoot: string
      target: string
      idlPath?: string | null
      overwrite?: boolean
    }): Promise<TridentHarnessResult | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        return await invoke<TridentHarnessResult>(
          "solana_audit_fuzz_scaffold",
          {
            request: {
              request: {
                projectRoot: args.projectRoot,
                target: args.target,
                idlPath: args.idlPath ?? null,
                overwrite: args.overwrite ?? false,
              },
            },
          },
        )
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [],
  )

  const runCoverageAudit = useCallback(
    async (args: {
      projectRoot: string
      package?: string | null
      testFilter?: string | null
      lcovPath?: string | null
      instructionNames?: string[]
      timeoutS?: number | null
    }): Promise<CoverageReport | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        const report = await invoke<CoverageReport>("solana_audit_coverage", {
          request: {
            request: {
              projectRoot: args.projectRoot,
              package: args.package ?? null,
              testFilter: args.testFilter ?? null,
              lcovPath: args.lcovPath ?? null,
              instructionNames: args.instructionNames ?? [],
              timeoutS: args.timeoutS ?? null,
            },
          },
        })
        setLastCoverageReport(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [],
  )

  const runReplay = useCallback(
    async (args: {
      exploit: ExploitKey
      targetProgram: string
      cluster: ClusterKind
      dryRun?: boolean
      snapshotSlot?: number | null
      rpcUrl?: string | null
    }): Promise<ReplayReport | null> => {
      if (!isTauri()) return null
      setAuditBusy(true)
      setError(null)
      try {
        const report = await invoke<ReplayReport>("solana_replay_exploit", {
          request: {
            request: {
              exploit: args.exploit,
              targetProgram: args.targetProgram,
              cluster: args.cluster,
              dryRun: args.dryRun ?? true,
              snapshotSlot: args.snapshotSlot ?? null,
              rpcUrl: args.rpcUrl ?? null,
            },
          },
        })
        setLastReplayReport(report)
        mergeFindings(report.findings)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setAuditBusy(false)
      }
    },
    [mergeFindings],
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

  const mergeLogEntries = useCallback((incoming: LogEntry[]) => {
    if (incoming.length === 0) return
    setLogEntries((current) => {
      const next =
        incoming.length >= MAX_LOG_FEED_ENTRIES
          ? incoming.slice(incoming.length - MAX_LOG_FEED_ENTRIES)
          : [...current, ...incoming]
      if (next.length > MAX_LOG_FEED_ENTRIES) {
        return next.slice(next.length - MAX_LOG_FEED_ENTRIES)
      }
      return next
    })
    setLogFeedVersion((current) => current + 1)
  }, [])

  const clearLogFeed = useCallback(() => {
    setLogEntries([])
    setLogFeedView(null)
    setLogFeedVersion((current) => current + 1)
    setDecodedLogEvents([])
    setLastLogFetch(null)
  }, [])

  const refreshActiveLogSubscriptions = useCallback(async () => {
    if (!isTauri()) return
    const next = await tauriInvoke<LogsActiveSubscription[]>("solana_logs_active")
    if (next) setActiveLogSubscriptions(next)
  }, [])

  const subscribeLogs = useCallback(
    async (filter: LogFilter): Promise<string | null> => {
      if (!isTauri()) return null
      setLogBusy(true)
      setError(null)
      try {
        const token = await invoke<string>("solana_logs_subscribe", {
          request: { filter },
        })
        setActiveLogSubscriptions((current) => {
          if (current.some((entry) => entry.token === token)) return current
          return [...current, { token, filter }]
        })
        return token
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setLogBusy(false)
      }
    },
    [],
  )

  const unsubscribeLogs = useCallback(async (token: string): Promise<boolean> => {
    if (!isTauri()) return false
    setLogBusy(true)
    setError(null)
    try {
      const ok = await invoke<boolean>("solana_logs_unsubscribe", {
        request: { token },
      })
      setActiveLogSubscriptions((current) =>
        current.filter((entry) => entry.token !== token),
      )
      return ok
    } catch (err) {
      setError(errorMessage(err))
      return false
    } finally {
      setLogBusy(false)
    }
  }, [])

  const fetchRecentLogs = useCallback(
    async (args: {
      cluster: ClusterKind
      programIds?: string[]
      lastN?: number
      rpcUrl?: string | null
      cachedOnly?: boolean
    }): Promise<LogsRecentResponse | null> => {
      if (!isTauri()) return null
      setLogBusy(true)
      setError(null)
      try {
        const response = await invoke<LogsRecentResponse>("solana_logs_recent", {
          request: {
            cluster: args.cluster,
            programIds: args.programIds ?? [],
            lastN: args.lastN ?? 25,
            rpcUrl: args.rpcUrl ?? null,
            cachedOnly: args.cachedOnly ?? false,
          },
        })
        setLastLogFetch(response)
        mergeLogEntries(response.entries)
        return response
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setLogBusy(false)
      }
    },
    [mergeLogEntries],
  )

  const refreshLogView = useCallback(
    async (args: {
      cluster: ClusterKind
      programIds?: string[]
      filter?: LogFeedFilter
      order?: LogFeedOrder
      limit?: number
    }): Promise<LogsViewResponse | null> => {
      if (!isTauri()) {
        setLogFeedView(null)
        return null
      }
      try {
        const response = await invoke<LogsViewResponse>("solana_logs_view", {
          request: {
            cluster: args.cluster,
            programIds: args.programIds ?? [],
            filter: args.filter ?? "all",
            order: args.order ?? "newestFirst",
            limit: args.limit ?? 100,
          },
        })
        setLogFeedView(response)
        return response
      } catch (err) {
        setError(errorMessage(err))
        return null
      }
    },
    [],
  )

  const scaffoldIndexer = useCallback(
    async (args: {
      kind: IndexerKind
      idlPath: string
      outputDir: string
      projectSlug?: string | null
      overwrite?: boolean
      rpcUrl?: string | null
    }): Promise<ScaffoldResult | null> => {
      if (!isTauri()) return null
      setIndexerBusy(true)
      setError(null)
      try {
        const result = await invoke<ScaffoldResult>("solana_indexer_scaffold", {
          request: {
            request: {
              kind: args.kind,
              idlPath: args.idlPath,
              outputDir: args.outputDir,
              projectSlug: args.projectSlug ?? null,
              overwrite: args.overwrite ?? false,
              rpcUrl: args.rpcUrl ?? null,
            },
          },
        })
        setLastIndexerScaffold(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIndexerBusy(false)
      }
    },
    [],
  )

  const runIndexer = useCallback(
    async (args: {
      cluster: ClusterKind
      programIds: string[]
      lastN?: number
      rpcUrl?: string | null
    }): Promise<IndexerRunReport | null> => {
      if (!isTauri()) return null
      setIndexerBusy(true)
      setError(null)
      try {
        const result = await invoke<IndexerRunReport>("solana_indexer_run", {
          request: {
            request: {
              cluster: args.cluster,
              programIds: args.programIds,
              lastN: args.lastN ?? 25,
              rpcUrl: args.rpcUrl ?? null,
            },
          },
        })
        setLastIndexerRun(result)
        mergeLogEntries(result.entries)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIndexerBusy(false)
      }
    },
    [mergeLogEntries],
  )

  // ---------------------- Phase 8 — token + wallet actions ----------------
  const refreshExtensionMatrix = useCallback(async () => {
    if (!isTauri()) return
    try {
      const next = await invoke<ExtensionMatrix>("solana_token_extension_matrix")
      setExtensionMatrix(next)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  const refreshWalletDescriptors = useCallback(async () => {
    if (!isTauri()) return
    try {
      const next = await invoke<WalletDescriptor[]>(
        "solana_wallet_scaffold_list",
      )
      setWalletDescriptors(next)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  const createToken = useCallback(
    async (spec: TokenCreateSpec): Promise<TokenCreateReport | null> => {
      if (!isTauri()) return null
      setTokenBusy(true)
      setError(null)
      try {
        const report = await invoke<TokenCreateReport>("solana_token_create", {
          request: { spec },
        })
        setLastTokenCreate(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setTokenBusy(false)
      }
    },
    [],
  )

  const mintMetaplex = useCallback(
    async (
      request: MetaplexMintRequest,
    ): Promise<MetaplexMintResult | null> => {
      if (!isTauri()) return null
      setTokenBusy(true)
      setError(null)
      try {
        const result = await invoke<MetaplexMintResult>(
          "solana_metaplex_mint",
          { request: { request } },
        )
        setLastMetaplexMint(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setTokenBusy(false)
      }
    },
    [],
  )

  const generateWalletScaffold = useCallback(
    async (
      request: WalletScaffoldRequest,
    ): Promise<WalletScaffoldResult | null> => {
      if (!isTauri()) return null
      setWalletBusy(true)
      setError(null)
      try {
        const result = await invoke<WalletScaffoldResult>(
          "solana_wallet_scaffold_generate",
          { request: { request } },
        )
        setLastWalletScaffold(result)
        return result
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setWalletBusy(false)
      }
    },
    [],
  )

  // --------- Phase 9 — safety actions --------------------------------------
  const scanSecrets = useCallback(
    async (args: {
      projectRoot: string
      skipPaths?: string[]
      minSeverity?: SecretSeverity | null
    }): Promise<SecretScanReport | null> => {
      if (!isTauri()) return null
      setSafetyBusy(true)
      setError(null)
      try {
        const report = await invoke<SecretScanReport>("solana_secrets_scan", {
          request: {
            request: {
              projectRoot: args.projectRoot,
              skipPaths: args.skipPaths ?? [],
              minSeverity: args.minSeverity ?? null,
            },
          },
        })
        setLastSecretScan(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setSafetyBusy(false)
      }
    },
    [],
  )

  const refreshSecretPatterns = useCallback(async () => {
    if (!isTauri()) return
    try {
      const next = await invoke<SecretPattern[]>("solana_secrets_patterns")
      setSecretPatterns(next)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  const runScopeCheck = useCallback(async (): Promise<ScopeCheckReport | null> => {
    if (!isTauri()) return null
    setSafetyBusy(true)
    setError(null)
    try {
      const report = await invoke<ScopeCheckReport>(
        "solana_secrets_scope_check",
      )
      setLastScopeCheck(report)
      return report
    } catch (err) {
      setError(errorMessage(err))
      return null
    } finally {
      setSafetyBusy(false)
    }
  }, [])

  const refreshTrackedPrograms = useCallback(async () => {
    if (!isTauri()) return
    try {
      const next = await invoke<TrackedProgram[]>(
        "solana_cluster_drift_tracked_programs",
      )
      setTrackedPrograms(next)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  const checkClusterDrift = useCallback(
    async (args?: {
      additional?: TrackedProgram[]
      clusters?: ClusterKind[]
      skipBuiltins?: boolean
    }): Promise<ClusterDriftReport | null> => {
      if (!isTauri()) return null
      setSafetyBusy(true)
      setError(null)
      try {
        const report = await invoke<ClusterDriftReport>(
          "solana_cluster_drift_check",
          {
            request: {
              request: {
                additional: args?.additional ?? [],
                clusters: args?.clusters ?? [],
                rpcUrls: {},
                skipBuiltins: args?.skipBuiltins ?? false,
                timeoutMs: null,
              },
            },
          },
        )
        setLastDriftReport2(report)
        return report
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setSafetyBusy(false)
      }
    },
    [],
  )

  const refreshCostSnapshot = useCallback(
    async (args?: {
      clusters?: ClusterKind[]
      windowS?: number | null
      skipProviderProbes?: boolean
    }): Promise<CostSnapshot | null> => {
      if (!isTauri()) return null
      setSafetyBusy(true)
      setError(null)
      try {
        const snap = await invoke<CostSnapshot>("solana_cost_snapshot", {
          request: {
            request: {
              clusters: args?.clusters ?? [],
              windowS: args?.windowS ?? null,
              skipProviderProbes: args?.skipProviderProbes ?? false,
            },
          },
        })
        setLastCostSnapshot(snap)
        return snap
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setSafetyBusy(false)
      }
    },
    [],
  )

  const resetCostLedger = useCallback(async () => {
    if (!isTauri()) return
    try {
      await invoke("solana_cost_reset")
      setLastCostSnapshot(null)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  const refreshDocCatalog = useCallback(async () => {
    if (!isTauri()) return
    try {
      const catalog = await invoke<DocSnippet[]>("solana_doc_catalog")
      setDocCatalog(catalog)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [])

  // Activation: show the cluster/toolchain tab first, then hydrate optional
  // workbench catalogs during idle slices so opening the sidebar stays light.
  useEffect(() => {
    if (!active || !isTauri()) return
    let cancelled = false
    let cancelIdleTask: (() => void) | null = null

    void refreshClusters()
    void refreshToolchain()
    void refreshToolchainInstallStatus()
    void refreshStatus()

    const backgroundRefreshes = [
      refreshSnapshots,
      refreshPersonaRoles,
      refreshScenarios,
      refreshActiveLogSubscriptions,
      refreshReplayCatalog,
      refreshExtensionMatrix,
      refreshWalletDescriptors,
      refreshSecretPatterns,
      refreshTrackedPrograms,
      refreshDocCatalog,
    ]

    const runNextBackgroundRefresh = () => {
      if (cancelled || !activeRef.current) return
      const refresh = backgroundRefreshes.shift()
      if (!refresh) return
      void refresh().finally(() => {
        if (cancelled || !activeRef.current) return
        cancelIdleTask = scheduleWorkbenchIdleTask(runNextBackgroundRefresh)
      })
    }

    cancelIdleTask = scheduleWorkbenchIdleTask(runNextBackgroundRefresh)

    return () => {
      cancelled = true
      cancelIdleTask?.()
    }
  }, [
    active,
    refreshClusters,
    refreshToolchain,
    refreshToolchainInstallStatus,
    refreshStatus,
    refreshSnapshots,
    refreshPersonaRoles,
    refreshScenarios,
    refreshReplayCatalog,
    refreshActiveLogSubscriptions,
    refreshExtensionMatrix,
    refreshWalletDescriptors,
    refreshSecretPatterns,
    refreshTrackedPrograms,
    refreshDocCatalog,
  ])

  // Listen for status events while the sidebar is visible.
  useEffect(() => {
    if (!active || !isTauri()) return
    let cancelled = false
    const unsubs: UnlistenFn[] = []

    void listen<ToolchainInstallEvent>(
      SOLANA_TOOLCHAIN_INSTALL_EVENT,
      (event) => {
        if (cancelled) return
        const payload = event.payload
        setToolchainInstallEvent(payload)
        setToolchainInstalling(
          !["completed", "failed", "skipped"].includes(payload.phase),
        )
        if (payload.phase === "completed" || payload.phase === "skipped") {
          void refreshToolchain()
          void refreshToolchainInstallStatus()
        }
        if (payload.phase === "failed" && payload.error) {
          setError(payload.error)
        }
      },
    ).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

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

    void listen<AuditEventPayload>(SOLANA_AUDIT_EVENT, (event) => {
      if (cancelled) return
      const payload = event.payload
      setAuditEvents((current) => {
        const next = [...current, payload]
        return next.length > MAX_AUDIT_FEED_EVENTS
          ? next.slice(next.length - MAX_AUDIT_FEED_EVENTS)
          : next
      })
      if (payload.finding) {
        const incoming = payload.finding
        setAuditFindings((current) => {
          if (current.some((f) => f.id === incoming.id)) return current
          const next = [...current, incoming]
          return next.length > MAX_AUDIT_FEED_FINDINGS
            ? next.slice(next.length - MAX_AUDIT_FEED_FINDINGS)
            : next
        })
      }
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<LogRawEventPayload>(SOLANA_LOG_EVENT, (event) => {
      if (cancelled) return
      mergeLogEntries([event.payload.entry])
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<LogDecodedEventPayload>(SOLANA_LOG_DECODED_EVENT, (event) => {
      if (cancelled) return
      const payload = event.payload
      setDecodedLogEvents((current) => {
        const next = [...current, payload]
        return next.length > MAX_DECODED_LOG_EVENTS
          ? next.slice(next.length - MAX_DECODED_LOG_EVENTS)
          : next
      })
    }).then((unsub) => {
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
  }, [active, refreshToolchain, refreshToolchainInstallStatus, mergeLogEntries])

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

  return useMemo<UseSolanaWorkbench>(() => ({
    clusters,
    toolchain,
    toolchainLoading,
    toolchainInstallStatus,
    toolchainInstallEvent,
    toolchainInstalling,
    status,
    lastEvent,
    rpcHealth,
    snapshots,
    isStarting,
    isStopping,
    error,
    refreshToolchain,
    installToolchain,
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
    auditBusy,
    auditFindings,
    auditEvents,
    lastStaticReport,
    lastExternalReport,
    lastFuzzReport,
    lastCoverageReport,
    lastReplayReport,
    replayCatalog,
    clearAuditFeed,
    refreshReplayCatalog,
    runStaticAudit,
    runExternalAudit,
    runFuzzAudit,
    scaffoldFuzzHarness,
    runCoverageAudit,
    runReplay,
    logBusy,
    logEntries,
    logFeedView,
    logFeedVersion,
    decodedLogEvents,
    activeLogSubscriptions,
    lastLogFetch,
    clearLogFeed,
    refreshActiveLogSubscriptions,
    subscribeLogs,
    unsubscribeLogs,
    fetchRecentLogs,
    refreshLogView,
    indexerBusy,
    lastIndexerScaffold,
    lastIndexerRun,
    scaffoldIndexer,
    runIndexer,
    tokenBusy,
    extensionMatrix,
    lastTokenCreate,
    lastMetaplexMint,
    walletBusy,
    walletDescriptors,
    lastWalletScaffold,
    refreshExtensionMatrix,
    refreshWalletDescriptors,
    createToken,
    mintMetaplex,
    generateWalletScaffold,
    // Phase 9 — safety
    safetyBusy,
    secretPatterns,
    lastSecretScan,
    lastScopeCheck,
    trackedPrograms,
    lastClusterDrift: lastDriftReport2,
    lastCostSnapshot,
    docCatalog,
    scanSecrets,
    refreshSecretPatterns,
    runScopeCheck,
    refreshTrackedPrograms,
    checkClusterDrift,
    refreshCostSnapshot,
    resetCostLedger,
    refreshDocCatalog,
  }), [
    activeIdlWatches,
    activeLogSubscriptions,
    analyseBumpPda,
    auditBusy,
    auditEvents,
    auditFindings,
    buildProgram,
    buildTx,
    checkClusterDrift,
    clearAuditFeed,
    clearLogFeed,
    clusters,
    createPersona,
    createSquadsProposal,
    createToken,
    decodedLogEvents,
    deletePersona,
    deployProgram,
    derivePda,
    docCatalog,
    driftIdl,
    error,
    estimatePriorityFee,
    explainTx,
    extensionMatrix,
    fetchIdl,
    fetchRecentLogs,
    fundPersona,
    generateCodama,
    generateWalletScaffold,
    idlBusy,
    idls,
    indexerBusy,
    installToolchain,
    isStarting,
    isStopping,
    lastBuildReport,
    lastCodamaReport,
    lastCostSnapshot,
    lastDeployProgress,
    lastDeployResult,
    lastDriftReport,
    lastDriftReport2,
    lastEvent,
    lastExplanation,
    lastExternalReport,
    lastFuzzReport,
    lastIdlEvent,
    lastIndexerRun,
    lastIndexerScaffold,
    lastLogFetch,
    logFeedVersion,
    logFeedView,
    lastMetaplexMint,
    lastPersonaEvent,
    lastPublishReport,
    lastReplayReport,
    lastRollback,
    lastScenarioEvent,
    lastScenarioRun,
    lastScopeCheck,
    lastSecretScan,
    lastSend,
    lastSimulation,
    lastSquadsProposal,
    lastStaticReport,
    lastTokenCreate,
    lastTxEvent,
    lastTxPlan,
    lastUpgradeSafety,
    lastVerifiedBuild,
    lastWalletScaffold,
    loadIdl,
    logBusy,
    logEntries,
    mintMetaplex,
    personas,
    personaBusy,
    personaRoles,
    predictPda,
    programBusy,
    publishIdl,
    refreshActiveLogSubscriptions,
    refreshLogView,
    refreshCostSnapshot,
    refreshDocCatalog,
    refreshExtensionMatrix,
    refreshPersonas,
    refreshReplayCatalog,
    refreshRpcHealth,
    refreshScenarios,
    refreshSecretPatterns,
    refreshSnapshots,
    refreshToolchain,
    refreshTrackedPrograms,
    refreshWalletDescriptors,
    replayCatalog,
    resetCostLedger,
    resolveAlt,
    resolveCpi,
    rollbackProgram,
    rpcHealth,
    runCoverageAudit,
    runExternalAudit,
    runFuzzAudit,
    runIndexer,
    runReplay,
    runScenario,
    runScopeCheck,
    runStaticAudit,
    safetyBusy,
    scanPda,
    scanSecrets,
    scaffoldFuzzHarness,
    scaffoldIndexer,
    scenarios,
    scenarioBusy,
    secretPatterns,
    sendTx,
    snapshots,
    start,
    startIdlWatch,
    status,
    stop,
    stopIdlWatch,
    subscribeLogs,
    submitVerifiedBuild,
    tokenBusy,
    toolchain,
    toolchainInstalling,
    toolchainInstallEvent,
    toolchainInstallStatus,
    toolchainLoading,
    trackedPrograms,
    txBusy,
    unsubscribeLogs,
    upgradeCheck,
    walletBusy,
    walletDescriptors,
  ])
}
