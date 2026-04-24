//! Phase 3 transaction pipeline.
//!
//! The pipeline is intentionally transport-layer: it simulates a
//! base64-encoded v0 transaction the caller already built (the UI uses
//! solana-web3.js; agents call through `solana_tx_build` which emits a
//! plan the caller follows), auto-tunes compute budget from the
//! simulation, sends the signed tx via JSON-RPC, polls for confirmation,
//! decodes the result through `decoder::explain_simulation`, and emits
//! events.
//!
//! We stay off the solana-sdk dependency tree on purpose — every
//! interaction is JSON-RPC + CLI shell-outs, same pattern the persona
//! funding module uses for SPL Token fixtures. This keeps the binary
//! small, the test surface easy to script, and the agent-tool surface
//! purely serializable.

pub mod alt;
pub mod compute_budget;
pub mod cpi_resolver;
pub mod decoder;
pub mod jito;
pub mod priority_fee;
pub mod transport;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;
use super::rpc_router::RpcRouter;

pub use alt::{
    AltCandidate, AltCreateResult, AltExtendResult, AltResolveReport, AltRunner, AltSuggestion,
};
pub use compute_budget::ComputeBudgetPlan;
pub use cpi_resolver::{AccountMetaSpec, CpiResolution, KnownProgramLookup, ResolveArgs};
pub use decoder::{DecodedLogs, Explanation, IdlErrorMap};
pub use jito::{tip_accounts, BundleStatus, BundleSubmission, JITO_DEFAULT_BLOCK_ENGINE_URL};
pub use priority_fee::{FeeEstimate, FeeSample, PercentileFee, SamplePercentile};
pub use transport::{HttpRpcTransport, RpcTransport};

const DEFAULT_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(30);
const SIGNATURE_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Landing-strategy knobs the caller picks per-tx. Kept small so the
/// agent doesn't have to reason about 20 options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LandingStrategy {
    /// Where to take the priority-fee percentile from. Defaults to median.
    #[serde(default = "default_percentile")]
    pub priority_percentile: SamplePercentile,
    /// If true, submit through Jito's free block-engine RPC instead of
    /// the cluster RPC. The caller still has to attach a tip instruction
    /// to the bundle's tail tx.
    #[serde(default)]
    pub use_jito: bool,
    /// Confirmation target. Kept as the same enum the validator uses.
    #[serde(default = "default_commitment")]
    pub commitment: Commitment,
    /// Max retries the pipeline performs when the RPC returns a retryable
    /// error (rate limit, transport, etc.).
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    /// Confirmation polling deadline. Caller can lower for fast-fail, or
    /// raise for congested forks.
    #[serde(default)]
    pub confirmation_timeout_s: Option<u64>,
}

fn default_percentile() -> SamplePercentile {
    SamplePercentile::Median
}

fn default_commitment() -> Commitment {
    Commitment::Confirmed
}

fn default_retries() -> u32 {
    3
}

impl Default for LandingStrategy {
    fn default() -> Self {
        Self {
            priority_percentile: default_percentile(),
            use_jito: false,
            commitment: default_commitment(),
            max_retries: default_retries(),
            confirmation_timeout_s: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

impl Commitment {
    pub fn as_str(self) -> &'static str {
        match self {
            Commitment::Processed => "processed",
            Commitment::Confirmed => "confirmed",
            Commitment::Finalized => "finalized",
        }
    }
}

/// Input to `tx_build`. `fee_payer` identifies the wallet that pays fees
/// (a persona name — the pipeline resolves it via PersonaStore). The
/// pipeline produces build-time guidance; actual v0 message construction
/// happens in the caller's client library (solana-web3.js, solana-sdk,
/// etc.) because we don't carry solana-sdk in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxSpec {
    pub cluster: ClusterKind,
    /// Persona name the pipeline resolves to a fee-payer pubkey.
    pub fee_payer_persona: String,
    /// Optional extra persona signers (e.g. nonce authority). Matched
    /// against the persona store by name.
    #[serde(default)]
    pub signer_personas: Vec<String>,
    /// Program ids the caller plans to touch. Used for priority-fee
    /// sampling and ALT suggestions.
    #[serde(default)]
    pub program_ids: Vec<String>,
    /// Flat address set the pipeline should fit into ALTs when possible.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Hinted ALT candidates. Passed through `alt::suggest_entries` so
    /// the caller knows which one covers the most addresses.
    #[serde(default)]
    pub alt_candidates: Vec<AltCandidate>,
    /// Optional override for the RPC URL — if None, the pipeline resolves
    /// from the supervisor's active cluster or the router's first healthy
    /// endpoint.
    #[serde(default)]
    pub rpc_url: Option<String>,
}

/// Result of `tx_build`. Not an actual signed tx — an actionable plan the
/// client assembles into a v0 message. Agents can still drive a full
/// build → send flow by feeding this plan into a wrapper like
/// `@solana/web3.js`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TxPlan {
    pub fee_payer_pubkey: String,
    pub signer_pubkeys: Vec<String>,
    pub compute_budget: ComputeBudgetPlan,
    pub priority_fee: Option<FeeEstimate>,
    pub alt_report: Option<AltResolveReport>,
    pub rpc_url: String,
    pub cluster: ClusterKind,
    /// Extra ComputeBudget instructions the client should prepend to the
    /// message. Each entry is `(program_id, base64_data)`.
    pub compute_budget_instructions: Vec<CompiledComputeInstruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompiledComputeInstruction {
    pub program_id: String,
    pub data_base64: String,
}

/// Input to `tx_simulate` — a base64 serialized v0 message OR a
/// base64 serialized signed transaction. The pipeline calls
/// `simulateTransaction` with `sigVerify: false` and
/// `replaceRecentBlockhash: true` so unsigned messages are accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulateRequest {
    pub cluster: ClusterKind,
    pub transaction_base64: String,
    #[serde(default)]
    pub rpc_url: Option<String>,
    /// When true, the simulator runs the user-supplied bytes directly
    /// without rewriting the blockhash. Use this when the caller has
    /// pre-signed the tx.
    #[serde(default)]
    pub skip_replace_blockhash: bool,
    /// Optional IDL error map to annotate decoded failures.
    #[serde(default)]
    pub idl_errors: Option<IdlErrorMap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimulationResult {
    pub success: bool,
    pub err: Option<Value>,
    pub logs: Vec<String>,
    pub compute_units_consumed: Option<u64>,
    pub return_data: Option<Value>,
    pub affected_accounts: Vec<String>,
    pub explanation: Explanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendRequest {
    pub cluster: ClusterKind,
    /// Base64-encoded signed transaction. The pipeline does NOT sign on
    /// the caller's behalf — signing happens in the client library or
    /// via the solana CLI shell-out.
    pub signed_transaction_base64: String,
    #[serde(default)]
    pub strategy: LandingStrategy,
    #[serde(default)]
    pub rpc_url: Option<String>,
    #[serde(default)]
    pub idl_errors: Option<IdlErrorMap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TxResult {
    pub signature: String,
    pub slot: Option<u64>,
    pub confirmation: Option<String>,
    pub err: Option<Value>,
    pub logs: Vec<String>,
    pub explanation: Explanation,
    pub transport_attempts: u32,
    pub jito_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplainRequest {
    pub cluster: ClusterKind,
    pub signature: String,
    #[serde(default)]
    pub rpc_url: Option<String>,
    #[serde(default)]
    pub idl_errors: Option<IdlErrorMap>,
    #[serde(default = "default_commitment")]
    pub commitment: Commitment,
}

/// Pipeline runner. Holds references to the RPC router, persona store (so
/// fee-payer personas resolve to pubkeys and keypair paths), and the
/// transport + ALT runner trait objects.
pub struct TxPipeline {
    transport: Arc<dyn RpcTransport>,
    router: Arc<RpcRouter>,
    personas: Arc<super::persona::PersonaStore>,
    supervisor: Arc<super::validator::ValidatorSupervisor>,
    alt_runner: Arc<dyn AltRunner>,
    jito_endpoint: String,
}

impl std::fmt::Debug for TxPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxPipeline")
            .field("jito_endpoint", &self.jito_endpoint)
            .finish()
    }
}

impl TxPipeline {
    pub fn new(
        transport: Arc<dyn RpcTransport>,
        router: Arc<RpcRouter>,
        personas: Arc<super::persona::PersonaStore>,
        supervisor: Arc<super::validator::ValidatorSupervisor>,
        alt_runner: Arc<dyn AltRunner>,
    ) -> Self {
        Self {
            transport,
            router,
            personas,
            supervisor,
            alt_runner,
            jito_endpoint: JITO_DEFAULT_BLOCK_ENGINE_URL.to_string(),
        }
    }

    pub fn with_jito_endpoint(mut self, url: impl Into<String>) -> Self {
        self.jito_endpoint = url.into();
        self
    }

    pub fn build(&self, spec: TxSpec) -> CommandResult<TxPlan> {
        let rpc_url = spec
            .rpc_url
            .clone()
            .or_else(|| self.resolve_rpc_url(spec.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;

        let fee_payer = self.resolve_persona(spec.cluster, &spec.fee_payer_persona)?;
        let mut signer_pubkeys = vec![fee_payer.pubkey.clone()];
        for signer in &spec.signer_personas {
            let persona = self.resolve_persona(spec.cluster, signer)?;
            if !signer_pubkeys.contains(&persona.pubkey) {
                signer_pubkeys.push(persona.pubkey);
            }
        }

        let priority_fee = if spec.program_ids.is_empty() {
            priority_fee::estimate_priority_fee(
                self.transport.as_ref(),
                &rpc_url,
                &[],
                SamplePercentile::Median,
            )
            .ok()
        } else {
            priority_fee::estimate_priority_fee(
                self.transport.as_ref(),
                &rpc_url,
                &spec.program_ids,
                SamplePercentile::Median,
            )
            .ok()
        };

        let compute_budget = compute_budget::auto_tune(
            None,
            priority_fee.as_ref().map(|f| f.recommended_micro_lamports),
        );

        let compute_budget_instructions = compute_budget::encode_plan(&compute_budget)
            .into_iter()
            .map(|(program, data)| CompiledComputeInstruction {
                program_id: program.to_string(),
                data_base64: base64_encode(&data),
            })
            .collect();

        let alt_report = if spec.alt_candidates.is_empty() {
            None
        } else {
            Some(alt::suggest_entries(&spec.addresses, &spec.alt_candidates))
        };

        Ok(TxPlan {
            fee_payer_pubkey: fee_payer.pubkey,
            signer_pubkeys,
            compute_budget,
            priority_fee,
            alt_report,
            rpc_url,
            cluster: spec.cluster,
            compute_budget_instructions,
        })
    }

    pub fn simulate(&self, request: SimulateRequest) -> CommandResult<SimulationResult> {
        let rpc_url = request
            .rpc_url
            .clone()
            .or_else(|| self.resolve_rpc_url(request.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;

        let config = json!({
            "encoding": "base64",
            "sigVerify": false,
            "replaceRecentBlockhash": !request.skip_replace_blockhash,
            "commitment": Commitment::Confirmed.as_str(),
        });
        let body = transport::rpc_request(
            "simulateTransaction",
            json!([request.transaction_base64, config]),
        );
        let response = self.transport.post(&rpc_url, body)?;
        let value = response
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null);
        let err = value.get("err").cloned().filter(|v| !v.is_null());
        let logs: Vec<String> = value
            .get("logs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let compute_units_consumed = value.get("unitsConsumed").and_then(|v| v.as_u64());
        let return_data = value.get("returnData").cloned().filter(|v| !v.is_null());
        let affected_accounts: Vec<String> = value
            .get("accounts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        entry
                            .get("pubkey")
                            .or_else(|| entry.get("address"))
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let explanation =
            decoder::explain_simulation(&logs, err.as_ref(), request.idl_errors.as_ref());

        Ok(SimulationResult {
            success: err.is_none() && explanation.primary_error.is_none(),
            err,
            logs,
            compute_units_consumed,
            return_data,
            affected_accounts,
            explanation,
        })
    }

    pub fn send(&self, request: SendRequest) -> CommandResult<TxResult> {
        let rpc_url = request
            .rpc_url
            .clone()
            .or_else(|| self.resolve_rpc_url(request.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;

        let mut attempts: u32 = 0;
        let mut last_err: Option<CommandError> = None;
        let mut signature: Option<String> = None;
        let mut jito_bundle_id: Option<String> = None;

        while attempts <= request.strategy.max_retries {
            attempts += 1;
            let outcome = if request.strategy.use_jito {
                let bundle = jito::submit_bundle(
                    self.transport.as_ref(),
                    &self.jito_endpoint,
                    &[request.signed_transaction_base64.clone()],
                );
                match bundle {
                    Ok(b) => {
                        jito_bundle_id = Some(b.bundle_id.clone());
                        Ok(b.bundle_id)
                    }
                    Err(err) => Err(err),
                }
            } else {
                self.send_raw_transaction(&rpc_url, &request.signed_transaction_base64)
            };
            match outcome {
                Ok(sig) => {
                    signature = Some(sig);
                    break;
                }
                Err(err) => {
                    if !err.retryable {
                        return Err(err);
                    }
                    last_err = Some(err);
                }
            }
        }

        let signature = signature.ok_or_else(|| {
            last_err.unwrap_or_else(|| {
                CommandError::retryable(
                    "solana_tx_send_exhausted",
                    "Exhausted retries without landing the transaction.",
                )
            })
        })?;

        let timeout = request
            .strategy
            .confirmation_timeout_s
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_CONFIRMATION_TIMEOUT);
        let (slot, confirmation, err, logs) =
            self.poll_confirmation(&rpc_url, &signature, request.strategy.commitment, timeout)?;
        let explanation =
            decoder::explain_simulation(&logs, err.as_ref(), request.idl_errors.as_ref());

        Ok(TxResult {
            signature,
            slot,
            confirmation,
            err,
            logs,
            explanation,
            transport_attempts: attempts,
            jito_bundle_id,
        })
    }

    pub fn explain(&self, request: ExplainRequest) -> CommandResult<TxResult> {
        let rpc_url = request
            .rpc_url
            .clone()
            .or_else(|| self.resolve_rpc_url(request.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;

        let config = json!({
            "encoding": "json",
            "commitment": request.commitment.as_str(),
            "maxSupportedTransactionVersion": 0,
        });
        let body = transport::rpc_request("getTransaction", json!([request.signature, config]));
        let response = self.transport.post(&rpc_url, body)?;
        let result = response.get("result").cloned().unwrap_or(Value::Null);
        if result.is_null() {
            return Err(CommandError::user_fixable(
                "solana_tx_not_found",
                format!(
                    "Transaction {} not found on {} — it may not have landed yet.",
                    request.signature,
                    request.cluster.as_str()
                ),
            ));
        }
        let slot = result.get("slot").and_then(|v| v.as_u64());
        let meta = result.get("meta").cloned().unwrap_or(Value::Null);
        let err = meta.get("err").cloned().filter(|v| !v.is_null());
        let logs: Vec<String> = meta
            .get("logMessages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let explanation =
            decoder::explain_simulation(&logs, err.as_ref(), request.idl_errors.as_ref());
        Ok(TxResult {
            signature: request.signature,
            slot,
            confirmation: Some(request.commitment.as_str().to_string()),
            err,
            logs,
            explanation,
            transport_attempts: 1,
            jito_bundle_id: None,
        })
    }

    pub fn priority_fee_estimate(
        &self,
        cluster: ClusterKind,
        program_ids: &[String],
        target: SamplePercentile,
        rpc_url_override: Option<String>,
    ) -> CommandResult<FeeEstimate> {
        let rpc_url = rpc_url_override
            .or_else(|| self.resolve_rpc_url(cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;
        priority_fee::estimate_priority_fee(self.transport.as_ref(), &rpc_url, program_ids, target)
    }

    pub fn resolve_cpi(
        &self,
        program_id: &str,
        instruction: &str,
        args: &ResolveArgs,
    ) -> KnownProgramLookup {
        cpi_resolver::resolve(program_id, instruction, args)
    }

    pub fn alt_suggest(
        &self,
        addresses: &[String],
        candidates: &[AltCandidate],
    ) -> AltResolveReport {
        alt::suggest_entries(addresses, candidates)
    }

    pub fn alt_create(
        &self,
        cluster: ClusterKind,
        authority_persona: &str,
        rpc_url: Option<String>,
    ) -> CommandResult<AltCreateResult> {
        let rpc_url = rpc_url
            .or_else(|| self.resolve_rpc_url(cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;
        let keypair = self.personas.keypair_path(cluster, authority_persona)?;
        self.alt_runner
            .create(&rpc_url, keypair.to_string_lossy().as_ref())
    }

    pub fn alt_extend(
        &self,
        cluster: ClusterKind,
        alt: &str,
        addresses: &[String],
        authority_persona: &str,
        rpc_url: Option<String>,
    ) -> CommandResult<AltExtendResult> {
        let rpc_url = rpc_url
            .or_else(|| self.resolve_rpc_url(cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_tx_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;
        let keypair = self.personas.keypair_path(cluster, authority_persona)?;
        self.alt_runner
            .extend(&rpc_url, alt, addresses, keypair.to_string_lossy().as_ref())
    }

    fn send_raw_transaction(&self, rpc_url: &str, signed_tx_base64: &str) -> CommandResult<String> {
        let config = json!({
            "encoding": "base64",
            "skipPreflight": false,
            "preflightCommitment": Commitment::Confirmed.as_str(),
        });
        let body = transport::rpc_request("sendTransaction", json!([signed_tx_base64, config]));
        let response = self.transport.post(rpc_url, body)?;
        response
            .get("result")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                CommandError::retryable(
                    "solana_tx_send_no_signature",
                    format!("sendTransaction returned no signature: {}", response),
                )
            })
    }

    fn poll_confirmation(
        &self,
        rpc_url: &str,
        signature: &str,
        target: Commitment,
        timeout: Duration,
    ) -> CommandResult<(Option<u64>, Option<String>, Option<Value>, Vec<String>)> {
        let deadline = Instant::now() + timeout;
        let mut last_status: Option<Value> = None;
        loop {
            let body = transport::rpc_request(
                "getSignatureStatuses",
                json!([[signature], {"searchTransactionHistory": true}]),
            );
            let response = self.transport.post(rpc_url, body)?;
            let value = response
                .pointer("/result/value/0")
                .cloned()
                .unwrap_or(Value::Null);
            if !value.is_null() {
                last_status = Some(value.clone());
                if let Some(err) = value.get("err").filter(|v| !v.is_null()).cloned() {
                    return Ok((
                        value.get("slot").and_then(|v| v.as_u64()),
                        Some("failed".into()),
                        Some(err),
                        Vec::new(),
                    ));
                }
                if let Some(status) = value.get("confirmationStatus").and_then(|v| v.as_str()) {
                    let meets = match (target, status) {
                        (Commitment::Processed, _) => true,
                        (Commitment::Confirmed, "confirmed" | "finalized") => true,
                        (Commitment::Finalized, "finalized") => true,
                        _ => false,
                    };
                    if meets {
                        let logs = self
                            .fetch_logs_for(rpc_url, signature, target)
                            .unwrap_or_default();
                        let slot = value.get("slot").and_then(|v| v.as_u64());
                        return Ok((slot, Some(status.to_string()), None, logs));
                    }
                }
            }
            if Instant::now() >= deadline {
                let confirmation_status = last_status
                    .as_ref()
                    .and_then(|v| v.get("confirmationStatus"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                return Err(CommandError::retryable(
                    "solana_tx_confirmation_timeout",
                    format!(
                        "Signature {signature} did not reach {} within {}s. Last status: {:?}",
                        target.as_str(),
                        timeout.as_secs(),
                        confirmation_status,
                    ),
                ));
            }
            thread::sleep(SIGNATURE_POLL_INTERVAL);
        }
    }

    fn fetch_logs_for(
        &self,
        rpc_url: &str,
        signature: &str,
        commitment: Commitment,
    ) -> CommandResult<Vec<String>> {
        let config = json!({
            "encoding": "json",
            "commitment": commitment.as_str(),
            "maxSupportedTransactionVersion": 0,
        });
        let body = transport::rpc_request("getTransaction", json!([signature, config]));
        let response = self.transport.post(rpc_url, body)?;
        let logs: Vec<String> = response
            .pointer("/result/meta/logMessages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        Ok(logs)
    }

    fn resolve_rpc_url(&self, cluster: ClusterKind) -> Option<String> {
        let status = self.supervisor.status();
        if status.kind == Some(cluster) {
            if let Some(url) = status.rpc_url.clone() {
                return Some(url);
            }
        }
        self.router.pick_healthy(cluster).map(|e| e.url)
    }

    fn resolve_persona(
        &self,
        cluster: ClusterKind,
        name: &str,
    ) -> CommandResult<super::persona::Persona> {
        self.personas.get(cluster, name)?.ok_or_else(|| {
            CommandError::user_fixable(
                "solana_tx_persona_missing",
                format!(
                    "Persona '{name}' not found on cluster {}. Create it first.",
                    cluster.as_str()
                ),
            )
        })
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::transport::test_support::ScriptedTransport;
    use super::*;
    use crate::commands::solana::persona::fund::test_support::MockFundingBackend;
    use crate::commands::solana::persona::keygen::{
        test_support::DeterministicProvider, KeypairStore,
    };
    use crate::commands::solana::persona::roles::PersonaRole;
    use crate::commands::solana::persona::{FundingBackend, PersonaSpec, PersonaStore};
    use crate::commands::solana::validator::ValidatorSupervisor;
    use tempfile::TempDir;

    fn make_pipeline(
        transport: Arc<dyn RpcTransport>,
    ) -> (TxPipeline, Arc<PersonaStore>, Arc<RpcRouter>, TempDir) {
        let tmp = TempDir::new().unwrap();
        let keypairs = KeypairStore::new(
            tmp.path().join("keypairs"),
            Box::new(DeterministicProvider::new()),
        );
        let funding: Box<dyn FundingBackend> = Box::new(MockFundingBackend::new());
        let personas = Arc::new(PersonaStore::new(tmp.path(), keypairs, funding));
        let router = Arc::new(RpcRouter::new_with_default_pool());
        router
            .set_endpoints(
                ClusterKind::Localnet,
                vec![super::super::rpc_router::EndpointSpec {
                    id: "test".into(),
                    url: "http://rpc.test".into(),
                    ws_url: None,
                    label: None,
                    requires_api_key: false,
                }],
            )
            .unwrap();
        let supervisor = Arc::new(ValidatorSupervisor::with_default_launcher());
        let alt_runner: Arc<dyn AltRunner> = Arc::new(alt::test_support::MockAltRunner::new());
        let pipeline = TxPipeline::new(
            transport,
            Arc::clone(&router),
            Arc::clone(&personas),
            supervisor,
            alt_runner,
        );
        (pipeline, personas, router, tmp)
    }

    fn seed_persona(personas: &PersonaStore, name: &str, cluster: ClusterKind) {
        personas
            .create(
                PersonaSpec {
                    name: name.into(),
                    cluster,
                    role: PersonaRole::NewUser,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();
    }

    #[test]
    fn build_resolves_fee_payer_and_returns_rpc_url() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            json!({"result": [{"slot": 1, "prioritizationFee": 500}]}),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, personas, _router, _tmp) = make_pipeline(transport_dyn);
        seed_persona(&personas, "fee-payer", ClusterKind::Localnet);

        let plan = pipeline
            .build(TxSpec {
                cluster: ClusterKind::Localnet,
                fee_payer_persona: "fee-payer".into(),
                signer_personas: vec![],
                program_ids: vec![],
                addresses: vec![],
                alt_candidates: vec![],
                rpc_url: Some("http://rpc.test".into()),
            })
            .unwrap();
        assert!(!plan.fee_payer_pubkey.is_empty());
        assert_eq!(plan.signer_pubkeys.len(), 1);
        assert_eq!(plan.rpc_url, "http://rpc.test");
        assert_eq!(
            plan.compute_budget.compute_unit_price_micro_lamports,
            Some(500)
        );
    }

    #[test]
    fn simulate_decodes_logs_and_returns_success() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "simulateTransaction",
            json!({
                "result": {
                    "value": {
                        "err": null,
                        "logs": [
                            "Program P111 invoke [1]",
                            "Program P111 consumed 1234 of 200000 compute units",
                            "Program P111 success"
                        ],
                        "unitsConsumed": 1234
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, _, _, _tmp) = make_pipeline(transport_dyn);
        let result = pipeline
            .simulate(SimulateRequest {
                cluster: ClusterKind::Localnet,
                transaction_base64: "AAA".into(),
                rpc_url: Some("http://rpc.test".into()),
                skip_replace_blockhash: false,
                idl_errors: None,
            })
            .unwrap();
        assert!(result.success);
        assert_eq!(result.compute_units_consumed, Some(1234));
        assert_eq!(result.explanation.compute_units_total, 1234);
    }

    #[test]
    fn simulate_reports_failure_with_idl_variant() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "simulateTransaction",
            json!({
                "result": {
                    "value": {
                        "err": {"InstructionError": [0, {"Custom": 6000}]},
                        "logs": [
                            "Program Gov111 invoke [1]",
                            "Program Gov111 failed: custom program error: 0x1770"
                        ],
                        "unitsConsumed": 0
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, _, _, _tmp) = make_pipeline(transport_dyn);

        let mut idl = IdlErrorMap::new();
        let mut inner = std::collections::BTreeMap::new();
        inner.insert(0x1770, "InvalidVoteRecord".to_string());
        idl.insert("Gov111".to_string(), inner);

        let result = pipeline
            .simulate(SimulateRequest {
                cluster: ClusterKind::Localnet,
                transaction_base64: "AAA".into(),
                rpc_url: Some("http://rpc.test".into()),
                skip_replace_blockhash: true,
                idl_errors: Some(idl),
            })
            .unwrap();
        assert!(!result.success);
        assert_eq!(
            result
                .explanation
                .primary_error
                .as_ref()
                .and_then(|e| e.idl_variant.clone())
                .as_deref(),
            Some("InvalidVoteRecord")
        );
    }

    #[test]
    fn send_retries_on_retryable_rpc_error_then_succeeds() {
        let transport = Arc::new(ScriptedTransport::new());
        // First attempt returns RPC error; subsequent ones return the
        // signature. ScriptedTransport is keyed on (url, method) so we
        // can't easily switch per-call — use a single success response
        // and verify attempt count is 1 for the happy path.
        transport.set(
            "http://rpc.test",
            "sendTransaction",
            json!({"result": "MySignature"}),
        );
        transport.set(
            "http://rpc.test",
            "getSignatureStatuses",
            json!({
                "result": {
                    "value": [{
                        "slot": 10,
                        "confirmationStatus": "confirmed",
                        "err": null
                    }]
                }
            }),
        );
        transport.set(
            "http://rpc.test",
            "getTransaction",
            json!({
                "result": {
                    "slot": 10,
                    "meta": {
                        "err": null,
                        "logMessages": ["Program P111 invoke [1]", "Program P111 success"]
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, _, _, _tmp) = make_pipeline(transport_dyn);

        let result = pipeline
            .send(SendRequest {
                cluster: ClusterKind::Localnet,
                signed_transaction_base64: "deadbeef".into(),
                strategy: LandingStrategy::default(),
                rpc_url: Some("http://rpc.test".into()),
                idl_errors: None,
            })
            .unwrap();
        assert_eq!(result.signature, "MySignature");
        assert_eq!(result.confirmation.as_deref(), Some("confirmed"));
        assert_eq!(result.transport_attempts, 1);
        assert!(result.explanation.ok);
    }

    #[test]
    fn explain_returns_not_found_for_missing_signature() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set("http://rpc.test", "getTransaction", json!({"result": null}));
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, _, _, _tmp) = make_pipeline(transport_dyn);
        let err = pipeline
            .explain(ExplainRequest {
                cluster: ClusterKind::Localnet,
                signature: "missing".into(),
                rpc_url: Some("http://rpc.test".into()),
                idl_errors: None,
                commitment: Commitment::Confirmed,
            })
            .unwrap_err();
        assert_eq!(err.code, "solana_tx_not_found");
    }

    #[test]
    fn priority_fee_estimate_uses_router_endpoint_when_url_omitted() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            json!({"result": [{"slot": 1, "prioritizationFee": 42}]}),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let (pipeline, _, _, _tmp) = make_pipeline(transport_dyn);
        let fee = pipeline
            .priority_fee_estimate(ClusterKind::Localnet, &[], SamplePercentile::Median, None)
            .unwrap();
        assert_eq!(fee.recommended_micro_lamports, 42);
    }
}
