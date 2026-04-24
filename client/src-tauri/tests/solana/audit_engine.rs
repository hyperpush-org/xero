//! Integration coverage for the Phase 6 audit engine.
//!
//! The engine is trait-mockable, so these tests exercise the public
//! `AuditEngine` API end-to-end without requiring Trident, Sec3,
//! cargo-llvm-cov, or a live validator on the CI host. They assert:
//!
//!   * Anchor-footgun lints find real issues on a fixture program.
//!   * Static-lint events stream through the `AuditEventSink` in order
//!     (`Started` → per-finding `Progress` → `Completed`).
//!   * External-analyzer JSON is parsed into the unified `Finding`.
//!   * Trident fuzz JSON output produces crashes + coverage delta.
//!   * Coverage reports parse lcov + roll up per-instruction totals.
//!   * Replay library returns four catalogue entries and refuses to
//!     drive mainnet.

use std::fs;
use std::sync::{Arc, Mutex};

use cadence_desktop_lib::commands::solana::{
    audit::{
        coverage::{test_support::ScriptedCoverageRunner, CoverageOutcome},
        replay::test_support::ScriptedReplayRunner,
        sec3::{
            test_support::ScriptedAnalyzerRunner, AnalyzerOutcome, AnalyzerProbe,
            ExternalAnalyzerReport, ExternalAnalyzerRequest,
        },
        static_lints::{StaticLintRequest, StaticLintReport},
        trident::{
            test_support::ScriptedTridentRunner, FuzzReport, FuzzRequest, TridentOutcome,
            TridentProbe,
        },
        AuditEngine, AuditEventPayload, AuditEventPhase, AuditEventSink, AuditRunKind,
        ReplayOutcome, ReplayReport, ReplayRequest,
    },
    AnalyzerKind, ClusterKind, CoverageReport, CoverageRequest, ExploitKey, Finding,
    FindingSeverity, NullAuditEventSink,
};
use tempfile::TempDir;

const FIXTURE_LCOV: &str = "SF:programs/p/src/lib.rs\nFN:10,prog::deposit\nFN:30,prog::withdraw\nFNDA:4,prog::deposit\nFNDA:0,prog::withdraw\nFNF:2\nFNH:1\nLF:20\nLH:12\nBRF:4\nBRH:2\nend_of_record\n";

#[derive(Debug, Default)]
struct RecordingSink {
    events: Mutex<Vec<AuditEventPayload>>,
}

impl RecordingSink {
    fn snapshot(&self) -> Vec<AuditEventPayload> {
        self.events.lock().unwrap().clone()
    }
}

impl AuditEventSink for RecordingSink {
    fn emit(&self, payload: AuditEventPayload) {
        self.events.lock().unwrap().push(payload);
    }
}

pub fn static_lints_stream_findings_in_phase_order() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("programs/p/src")).unwrap();
    fs::write(
        tmp.path().join("programs/p/src/lib.rs"),
        r#"
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub vault: AccountInfo<'info>,
}

pub fn tally(a: u64, b: u64) -> u64 {
    a + b
}
"#,
    )
    .unwrap();

    let engine = AuditEngine::system();
    let sink = Arc::new(RecordingSink::default());
    let sink_ref: &dyn AuditEventSink = sink.as_ref();
    let report: StaticLintReport = engine
        .run_static_lints(
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec![],
                skip_paths: vec![],
            },
            sink_ref,
        )
        .expect("static audit runs");

    assert!(!report.findings.is_empty(), "fixture should produce at least one finding");
    assert!(
        report
            .findings
            .iter()
            .any(|f: &Finding| f.rule_id == "missing_signer"),
        "expected missing_signer in {:?}",
        report
            .findings
            .iter()
            .map(|f| &f.rule_id)
            .collect::<Vec<_>>()
    );

    let events = sink.snapshot();
    assert!(events.len() >= 2, "expected at least started + completed");
    assert_eq!(events.first().unwrap().phase, AuditEventPhase::Started);
    assert_eq!(events.last().unwrap().phase, AuditEventPhase::Completed);
    // Every Progress event must carry a finding.
    for event in &events {
        if event.phase == AuditEventPhase::Progress {
            assert!(event.finding.is_some(), "progress events carry findings");
        }
        assert!(matches!(event.kind, AuditRunKind::Static));
    }
}

pub fn external_analyzer_not_installed_returns_informational_finding() {
    let tmp = TempDir::new().unwrap();
    let analyzer = Arc::new(ScriptedAnalyzerRunner::new());
    analyzer.set_probe(AnalyzerProbe {
        installed: false,
        binary_path: None,
    });
    let engine = AuditEngine::with_runners(
        Arc::new(ScriptedTridentRunner::new()),
        Arc::new(ScriptedCoverageRunner::new()),
        Arc::new(ScriptedReplayRunner::new()),
        analyzer.clone(),
    );
    let sink = NullAuditEventSink;
    let report: ExternalAnalyzerReport = engine
        .run_external_analyzer(
            &ExternalAnalyzerRequest {
                project_root: tmp.path().display().to_string(),
                analyzer: AnalyzerKind::Auto,
                timeout_s: Some(30),
            },
            &sink,
        )
        .expect("external audit runs");
    assert!(!report.analyzer_installed);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].rule_id, "analyzer_not_installed");
    assert_eq!(report.findings[0].severity, FindingSeverity::Informational);
}

pub fn external_analyzer_parses_scripted_json_output() {
    let tmp = TempDir::new().unwrap();
    let analyzer = Arc::new(ScriptedAnalyzerRunner::new());
    analyzer.set_probe(AnalyzerProbe {
        installed: true,
        binary_path: Some("/opt/sec3/bin/sec3".into()),
    });
    analyzer.set_outcome(AnalyzerOutcome {
        exit_code: Some(0),
        success: true,
        stdout: r#"{"findings":[{"id":"SEC3_0001","title":"Missing signer","severity":"high","message":"Withdraw lacks Signer","file":"lib.rs","line":10}]}"#.into(),
        stderr: String::new(),
    });
    let engine = AuditEngine::with_runners(
        Arc::new(ScriptedTridentRunner::new()),
        Arc::new(ScriptedCoverageRunner::new()),
        Arc::new(ScriptedReplayRunner::new()),
        analyzer,
    );
    let sink = NullAuditEventSink;
    let report = engine
        .run_external_analyzer(
            &ExternalAnalyzerRequest {
                project_root: tmp.path().display().to_string(),
                analyzer: AnalyzerKind::Sec3,
                timeout_s: Some(30),
            },
            &sink,
        )
        .expect("external run");
    assert!(report.analyzer_installed);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].severity, FindingSeverity::High);
}

pub fn fuzz_engine_reports_crashes_with_reproducer() {
    let tmp = TempDir::new().unwrap();
    let trident = Arc::new(ScriptedTridentRunner::new());
    trident.set_probe(TridentProbe {
        installed: true,
        binary_path: Some("/tmp/trident".into()),
    });
    trident.set_outcome(TridentOutcome {
        exit_code: Some(0),
        success: true,
        stdout: r#"{"crashes":[{"id":"c0","instruction":"withdraw","panic":"overflow","reproducer":["trident","fuzz","repro","c0"]}],"coverage":{"lines":128}}"#.into(),
        stderr: String::new(),
    });
    let engine = AuditEngine::with_runners(
        trident,
        Arc::new(ScriptedCoverageRunner::new()),
        Arc::new(ScriptedReplayRunner::new()),
        Arc::new(ScriptedAnalyzerRunner::new()),
    );
    let sink = NullAuditEventSink;
    let report: FuzzReport = engine
        .run_fuzz(
            &FuzzRequest {
                project_root: tmp.path().display().to_string(),
                target: "my_prog".into(),
                duration_s: Some(30),
                corpus: None,
                baseline_coverage_lines: Some(64),
            },
            &sink,
        )
        .expect("fuzz run");
    assert_eq!(report.crashes.len(), 1);
    assert_eq!(report.coverage_delta, 64);
    assert!(report.findings.iter().any(|f: &Finding| f.rule_id.starts_with("crash:")));
}

pub fn coverage_parses_instruction_rollups() {
    let tmp = TempDir::new().unwrap();
    let coverage = Arc::new(ScriptedCoverageRunner::new());
    coverage.set_lcov_body(FIXTURE_LCOV);
    coverage.set_outcome(CoverageOutcome {
        exit_code: Some(0),
        success: true,
        stdout: String::new(),
        stderr: String::new(),
        lcov_path: String::new(),
    });
    let engine = AuditEngine::with_runners(
        Arc::new(ScriptedTridentRunner::new()),
        coverage,
        Arc::new(ScriptedReplayRunner::new()),
        Arc::new(ScriptedAnalyzerRunner::new()),
    );
    let sink = NullAuditEventSink;
    let report: CoverageReport = engine
        .run_coverage(
            &CoverageRequest {
                project_root: tmp.path().display().to_string(),
                package: Some("p".into()),
                test_filter: None,
                lcov_path: None,
                instruction_names: vec!["deposit".into(), "withdraw".into()],
                timeout_s: Some(30),
            },
            &sink,
        )
        .expect("coverage run");
    assert!(report.success);
    assert_eq!(report.total_lines_found, 20);
    assert_eq!(report.total_functions_hit, 1);
    assert_eq!(report.instructions.len(), 2);
    let deposit = report
        .instructions
        .iter()
        .find(|i| i.instruction == "deposit")
        .unwrap();
    assert_eq!(deposit.functions_hit, 1);
    assert_eq!(deposit.functions_found, 1);
}

pub fn replay_catalog_returns_four_exploits_and_refuses_mainnet() {
    let engine = AuditEngine::system();
    let descriptors = engine.library().all();
    assert_eq!(descriptors.len(), 4);
    let keys: Vec<_> = descriptors.iter().map(|d| d.key).collect();
    assert!(keys.contains(&ExploitKey::WormholeSigSkip));
    assert!(keys.contains(&ExploitKey::CashioFakeCollateral));
    assert!(keys.contains(&ExploitKey::MangoOracleManip));
    assert!(keys.contains(&ExploitKey::NirvanaFlashLoan));

    let sink = NullAuditEventSink;
    let err = engine
        .run_replay(
            &ReplayRequest {
                exploit: ExploitKey::WormholeSigSkip,
                target_program: "Prog111".into(),
                cluster: ClusterKind::Mainnet,
                rpc_url: None,
                dry_run: true,
                snapshot_slot: None,
            },
            &sink,
        )
        .unwrap_err();
    assert_eq!(
        err.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied,
        "mainnet replay must be policy-denied"
    );
}

pub fn replay_scripted_runner_emits_expected_bad_state_finding() {
    let replay = Arc::new(ScriptedReplayRunner::new());
    replay.set_outcome(ReplayOutcome::ExpectedBadState, "target vulnerable");
    let engine = AuditEngine::with_runners(
        Arc::new(ScriptedTridentRunner::new()),
        Arc::new(ScriptedCoverageRunner::new()),
        replay,
        Arc::new(ScriptedAnalyzerRunner::new()),
    );
    let sink = NullAuditEventSink;
    let report: ReplayReport = engine
        .run_replay(
            &ReplayRequest {
                exploit: ExploitKey::CashioFakeCollateral,
                target_program: "Cashio111".into(),
                cluster: ClusterKind::MainnetFork,
                rpc_url: None,
                dry_run: false,
                snapshot_slot: None,
            },
            &sink,
        )
        .expect("replay runs against forked mainnet");
    assert_eq!(report.outcome, ReplayOutcome::ExpectedBadState);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].severity, FindingSeverity::Critical);
}

pub fn twenty_instruction_anchor_program_audit_is_fast() {
    // Acceptance: agent runs full static audit on a 20-instruction
    // Anchor program in <10s. We synthesise 20 dummy Anchor structs
    // (each missing a Signer) and assert the audit completes comfortably
    // under that wall clock.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("programs/p/src")).unwrap();
    let mut body = String::from("use anchor_lang::prelude::*;\n\n");
    for idx in 0..20 {
        body.push_str(&format!(
            "#[derive(Accounts)]\npub struct Ix{idx}<'info> {{\n    #[account(mut)]\n    pub vault: AccountInfo<'info>,\n}}\n\n",
        ));
    }
    fs::write(root.join("programs/p/src/lib.rs"), body).unwrap();

    let engine = AuditEngine::system();
    let sink = NullAuditEventSink;
    let start = std::time::Instant::now();
    let report = engine
        .run_static_lints(
            &StaticLintRequest {
                project_root: root.display().to_string(),
                rule_ids: vec![],
                skip_paths: vec![],
            },
            &sink,
        )
        .expect("lint run");
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "static audit took too long: {}ms",
        elapsed.as_millis()
    );
    let missing_signers = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "missing_signer")
        .count();
    assert!(missing_signers >= 20, "expected ≥20 missing_signer findings");
}
