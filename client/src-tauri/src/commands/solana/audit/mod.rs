//! Audit engine — Phase 6 of the Solana workbench.
//!
//! Four independent capabilities bound together by a shared `Finding`
//! shape so the frontend can render them under a single "Safety" tab:
//!
//! * `static_lints` — fast, zero-dependency Rust source scan for the
//!   Anchor footgun checklist (missing `Signer`, unchecked
//!   `AccountInfo`, arithmetic overflow, realloc-without-rent, …).
//! * `sec3` — pluggable wrapper over Sec3/Soteria/Aderyn binaries; reports
//!   `analyzer_not_installed` when the tool isn't on PATH.
//! * `trident` — fuzz harness generator + run driver.
//! * `coverage` — `cargo-llvm-cov` orchestration with lcov parsing.
//! * `replay` — exploit replay library against forked-mainnet snapshots.
//!
//! Every module is trait-mockable so the integration tests drive the
//! surface without spawning any real tool.

pub mod coverage;
pub mod replay;
pub mod sec3;
pub mod static_lints;
pub mod trident;

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use self::coverage::{CoverageRunner, SystemCoverageRunner};
use self::replay::{ExploitLibrary, ReplayRunner, SystemReplayRunner};
use self::sec3::{ExternalAnalyzerRunner, SystemExternalAnalyzerRunner};
use self::trident::{SystemTridentRunner, TridentRunner};

pub use coverage::{
    CoverageReport, CoverageRequest, FunctionCoverage, InstructionCoverage, LcovRecord,
};
pub use replay::{
    ExploitDescriptor, ExploitKey, ReplayOutcome, ReplayReport, ReplayRequest, ReplayStep,
};
pub use sec3::{AnalyzerKind, ExternalAnalyzerReport, ExternalAnalyzerRequest};
pub use static_lints::{
    run as run_static_lints, AnchorFinding, StaticLintReport, StaticLintRequest, StaticLintRule,
};
pub use trident::{
    FuzzCrash, FuzzReport, FuzzRequest, TridentHarnessRequest, TridentHarnessResult,
};

/// Severity ladder shared by every audit finding. Maps directly to the
/// colour chips in the frontend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

impl FindingSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingSeverity::Critical => "critical",
            FindingSeverity::High => "high",
            FindingSeverity::Medium => "medium",
            FindingSeverity::Low => "low",
            FindingSeverity::Informational => "informational",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            FindingSeverity::Critical => 0,
            FindingSeverity::High => 1,
            FindingSeverity::Medium => 2,
            FindingSeverity::Low => 3,
            FindingSeverity::Informational => 4,
        }
    }
}

/// Identifies which upstream analyzer produced the finding. Critical for
/// the agent's dedup logic (same rule id from two sources = same bug, not
/// two).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FindingSource {
    /// Built-in Anchor-footgun lints.
    AnchorLints,
    /// Sec3 / Soteria / Aderyn — any external analyzer wrapped by `sec3.rs`.
    External,
    /// Trident fuzzer result.
    Fuzz,
    /// Exploit replay library.
    Replay,
}

impl FindingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingSource::AnchorLints => "anchor_lints",
            FindingSource::External => "external",
            FindingSource::Fuzz => "fuzz",
            FindingSource::Replay => "replay",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub source: FindingSource,
    pub rule_id: String,
    pub severity: FindingSeverity,
    pub title: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub fix_hint: Option<String>,
    pub reference_url: Option<String>,
}

impl Finding {
    pub fn new(
        source: FindingSource,
        rule_id: impl Into<String>,
        severity: FindingSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let rule = rule_id.into();
        Self {
            id: format!("{}:{}", source.as_str(), rule),
            source,
            rule_id: rule,
            severity,
            title: title.into(),
            message: message.into(),
            file: None,
            line: None,
            column: None,
            fix_hint: None,
            reference_url: None,
        }
    }

    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.file = Some(path.into());
        self
    }

    pub fn with_location(mut self, line: u32, column: u32) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_fix_hint(mut self, hint: impl Into<String>) -> Self {
        self.fix_hint = Some(hint.into());
        self
    }

    pub fn with_reference(mut self, url: impl Into<String>) -> Self {
        self.reference_url = Some(url.into());
        self
    }
}

/// Roll-up the frontend renders in the severity filter chips.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SeverityCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub informational: u32,
}

impl SeverityCounts {
    pub fn record(&mut self, severity: FindingSeverity) {
        match severity {
            FindingSeverity::Critical => self.critical += 1,
            FindingSeverity::High => self.high += 1,
            FindingSeverity::Medium => self.medium += 1,
            FindingSeverity::Low => self.low += 1,
            FindingSeverity::Informational => self.informational += 1,
        }
    }

    pub fn from_findings(findings: &[Finding]) -> Self {
        let mut counts = Self::default();
        for f in findings {
            counts.record(f.severity);
        }
        counts
    }

    pub fn total(&self) -> u32 {
        self.critical + self.high + self.medium + self.low + self.informational
    }
}

/// Event emitted as the engine streams findings (static run, fuzz run,
/// replay) so the frontend can render a live feed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventPhase {
    Started,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventPayload {
    pub run_id: String,
    pub kind: AuditRunKind,
    pub phase: AuditEventPhase,
    pub finding: Option<Finding>,
    pub message: Option<String>,
    pub ts_ms: u64,
}

impl AuditEventPayload {
    pub fn new(run_id: impl Into<String>, kind: AuditRunKind, phase: AuditEventPhase) -> Self {
        Self {
            run_id: run_id.into(),
            kind,
            phase,
            finding: None,
            message: None,
            ts_ms: now_ms(),
        }
    }

    pub fn with_finding(mut self, finding: Finding) -> Self {
        self.finding = Some(finding);
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditRunKind {
    Static,
    External,
    Fuzz,
    Coverage,
    Replay,
}

impl AuditRunKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditRunKind::Static => "static",
            AuditRunKind::External => "external",
            AuditRunKind::Fuzz => "fuzz",
            AuditRunKind::Coverage => "coverage",
            AuditRunKind::Replay => "replay",
        }
    }
}

pub trait AuditEventSink: Send + Sync + std::fmt::Debug {
    fn emit(&self, payload: AuditEventPayload);
}

#[derive(Debug, Default, Clone)]
pub struct NullAuditEventSink;

impl AuditEventSink for NullAuditEventSink {
    fn emit(&self, _payload: AuditEventPayload) {}
}

/// Entry point used by both the Tauri command surface and the
/// autonomous-runtime wrapper. Holds trait-objects for every external
/// tool so tests can swap in scripted runners.
#[derive(Clone)]
pub struct AuditEngine {
    trident: Arc<dyn TridentRunner>,
    coverage: Arc<dyn CoverageRunner>,
    replay: Arc<dyn ReplayRunner>,
    external: Arc<dyn ExternalAnalyzerRunner>,
    library: ExploitLibrary,
}

impl std::fmt::Debug for AuditEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditEngine")
            .field("trident", &"<runner>")
            .field("coverage", &"<runner>")
            .field("replay", &"<runner>")
            .field("external", &"<runner>")
            .field("library_size", &self.library.all().len())
            .finish()
    }
}

impl Default for AuditEngine {
    fn default() -> Self {
        Self::system()
    }
}

impl AuditEngine {
    pub fn system() -> Self {
        Self {
            trident: Arc::new(SystemTridentRunner::new()),
            coverage: Arc::new(SystemCoverageRunner::new()),
            replay: Arc::new(SystemReplayRunner::new()),
            external: Arc::new(SystemExternalAnalyzerRunner::new()),
            library: ExploitLibrary::builtin(),
        }
    }

    pub fn with_runners(
        trident: Arc<dyn TridentRunner>,
        coverage: Arc<dyn CoverageRunner>,
        replay: Arc<dyn ReplayRunner>,
        external: Arc<dyn ExternalAnalyzerRunner>,
    ) -> Self {
        Self {
            trident,
            coverage,
            replay,
            external,
            library: ExploitLibrary::builtin(),
        }
    }

    pub fn library(&self) -> &ExploitLibrary {
        &self.library
    }

    pub fn run_static_lints(
        &self,
        request: &StaticLintRequest,
        sink: &dyn AuditEventSink,
    ) -> CommandResult<StaticLintReport> {
        let run_id = new_run_id(AuditRunKind::Static);
        sink.emit(AuditEventPayload::new(
            run_id.clone(),
            AuditRunKind::Static,
            AuditEventPhase::Started,
        ));

        let root = Path::new(&request.project_root);
        if !root.is_dir() {
            sink.emit(
                AuditEventPayload::new(
                    run_id.clone(),
                    AuditRunKind::Static,
                    AuditEventPhase::Failed,
                )
                .with_message(format!(
                    "project_root {} is not a directory",
                    root.display()
                )),
            );
            return Err(CommandError::user_fixable(
                "solana_audit_static_bad_root",
                format!("Static audit root {} is not a directory.", root.display()),
            ));
        }

        let mut report = static_lints::run(root, request)?;
        report.run_id = run_id.clone();
        for finding in &report.findings {
            sink.emit(
                AuditEventPayload::new(
                    run_id.clone(),
                    AuditRunKind::Static,
                    AuditEventPhase::Progress,
                )
                .with_finding(finding.clone()),
            );
        }
        sink.emit(
            AuditEventPayload::new(
                run_id.clone(),
                AuditRunKind::Static,
                AuditEventPhase::Completed,
            )
            .with_message(format!(
                "{} files scanned, {} findings",
                report.files_scanned,
                report.findings.len()
            )),
        );
        Ok(report)
    }

    pub fn run_external_analyzer(
        &self,
        request: &ExternalAnalyzerRequest,
        sink: &dyn AuditEventSink,
    ) -> CommandResult<ExternalAnalyzerReport> {
        let run_id = new_run_id(AuditRunKind::External);
        sink.emit(AuditEventPayload::new(
            run_id.clone(),
            AuditRunKind::External,
            AuditEventPhase::Started,
        ));
        let mut report = sec3::run(self.external.as_ref(), request)?;
        report.run_id = run_id.clone();
        for finding in &report.findings {
            sink.emit(
                AuditEventPayload::new(
                    run_id.clone(),
                    AuditRunKind::External,
                    AuditEventPhase::Progress,
                )
                .with_finding(finding.clone()),
            );
        }
        let phase = if report.analyzer_installed {
            AuditEventPhase::Completed
        } else {
            AuditEventPhase::Failed
        };
        sink.emit(
            AuditEventPayload::new(run_id.clone(), AuditRunKind::External, phase)
                .with_message(report.summary.clone()),
        );
        Ok(report)
    }

    pub fn run_fuzz(
        &self,
        request: &FuzzRequest,
        sink: &dyn AuditEventSink,
    ) -> CommandResult<FuzzReport> {
        let run_id = new_run_id(AuditRunKind::Fuzz);
        sink.emit(AuditEventPayload::new(
            run_id.clone(),
            AuditRunKind::Fuzz,
            AuditEventPhase::Started,
        ));
        let mut report = trident::run_fuzz(self.trident.as_ref(), request)?;
        report.run_id = run_id.clone();
        for finding in &report.findings {
            sink.emit(
                AuditEventPayload::new(
                    run_id.clone(),
                    AuditRunKind::Fuzz,
                    AuditEventPhase::Progress,
                )
                .with_finding(finding.clone()),
            );
        }
        sink.emit(
            AuditEventPayload::new(
                run_id.clone(),
                AuditRunKind::Fuzz,
                AuditEventPhase::Completed,
            )
            .with_message(format!(
                "{} crashes, coverage delta {:+}",
                report.crashes.len(),
                report.coverage_delta
            )),
        );
        Ok(report)
    }

    pub fn generate_fuzz_harness(
        &self,
        request: &TridentHarnessRequest,
    ) -> CommandResult<TridentHarnessResult> {
        trident::generate_harness(self.trident.as_ref(), request)
    }

    pub fn run_coverage(
        &self,
        request: &CoverageRequest,
        sink: &dyn AuditEventSink,
    ) -> CommandResult<CoverageReport> {
        let run_id = new_run_id(AuditRunKind::Coverage);
        sink.emit(AuditEventPayload::new(
            run_id.clone(),
            AuditRunKind::Coverage,
            AuditEventPhase::Started,
        ));
        let mut report = coverage::run(self.coverage.as_ref(), request)?;
        report.run_id = run_id.clone();
        sink.emit(
            AuditEventPayload::new(
                run_id.clone(),
                AuditRunKind::Coverage,
                AuditEventPhase::Completed,
            )
            .with_message(format!(
                "line {:.1}% / fn {:.1}% / {} files",
                report.line_coverage_percent,
                report.function_coverage_percent,
                report.files.len()
            )),
        );
        Ok(report)
    }

    pub fn run_replay(
        &self,
        request: &ReplayRequest,
        sink: &dyn AuditEventSink,
    ) -> CommandResult<ReplayReport> {
        let run_id = new_run_id(AuditRunKind::Replay);
        sink.emit(AuditEventPayload::new(
            run_id.clone(),
            AuditRunKind::Replay,
            AuditEventPhase::Started,
        ));
        let mut report = replay::run(self.replay.as_ref(), &self.library, request)?;
        report.run_id = run_id.clone();
        for finding in &report.findings {
            sink.emit(
                AuditEventPayload::new(
                    run_id.clone(),
                    AuditRunKind::Replay,
                    AuditEventPhase::Progress,
                )
                .with_finding(finding.clone()),
            );
        }
        let phase = match report.outcome {
            ReplayOutcome::ExpectedBadState => AuditEventPhase::Completed,
            ReplayOutcome::Mitigated => AuditEventPhase::Completed,
            ReplayOutcome::UnexpectedFailure => AuditEventPhase::Failed,
            ReplayOutcome::Inconclusive => AuditEventPhase::Completed,
        };
        sink.emit(
            AuditEventPayload::new(run_id.clone(), AuditRunKind::Replay, phase).with_message(
                format!(
                    "{} — {} step(s)",
                    report.outcome.as_str(),
                    report.steps.len()
                ),
            ),
        );
        Ok(report)
    }
}

pub fn new_run_id(kind: AuditRunKind) -> String {
    format!("{}-{}", kind.as_str(), now_ms())
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_rank_orders_critical_first() {
        assert!(FindingSeverity::Critical.rank() < FindingSeverity::High.rank());
        assert!(FindingSeverity::High.rank() < FindingSeverity::Medium.rank());
        assert!(FindingSeverity::Medium.rank() < FindingSeverity::Low.rank());
        assert!(FindingSeverity::Low.rank() < FindingSeverity::Informational.rank());
    }

    #[test]
    fn severity_counts_totals_findings() {
        let mut counts = SeverityCounts::default();
        counts.record(FindingSeverity::High);
        counts.record(FindingSeverity::High);
        counts.record(FindingSeverity::Low);
        assert_eq!(counts.high, 2);
        assert_eq!(counts.low, 1);
        assert_eq!(counts.total(), 3);
    }

    #[test]
    fn finding_id_combines_source_and_rule() {
        let f = Finding::new(
            FindingSource::AnchorLints,
            "missing_signer",
            FindingSeverity::High,
            "Missing signer check",
            "Add #[account(signer)]",
        );
        assert_eq!(f.id, "anchor_lints:missing_signer");
    }

    #[test]
    fn run_id_contains_kind_label() {
        let id = new_run_id(AuditRunKind::Static);
        assert!(id.starts_with("static-"));
    }
}
