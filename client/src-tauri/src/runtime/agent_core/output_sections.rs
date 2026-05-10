use crate::commands::{AgentOutputSectionEmphasisDto, RuntimeAgentOutputContractDto};

/// Static, hand-authored description of one section of an agent's output
/// contract. Section ids are stable across versions so DB touchpoints and
/// other inspector references can target them by id.
#[derive(Debug, Clone, Copy)]
pub struct OutputSectionEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub emphasis: AgentOutputSectionEmphasisDto,
    pub produced_by_tools: &'static [&'static str],
}

const ANSWER_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "answer",
        label: "Answer",
        description: "The direct, prose answer to the user's question.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Read", "Grep", "Glob", "RetrieveContext"],
    },
    OutputSectionEntry {
        id: "citations",
        label: "Citations",
        description: "File-and-line references for every claim that touches the repository.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Read", "Grep", "Glob"],
    },
    OutputSectionEntry {
        id: "uncertainty",
        label: "Uncertainty Calls",
        description: "Explicit notes about anything the agent could not confirm.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
];

const PLAN_PACK_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "goal",
        label: "Goal",
        description: "The single outcome the plan commits to deliver.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "non_goals",
        label: "Non-Goals",
        description: "Things the plan deliberately does not address.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "constraints",
        label: "Constraints",
        description: "Hard limits, dependencies, or invariants that bound the solution space.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["RetrieveContext"],
    },
    OutputSectionEntry {
        id: "context_used",
        label: "Context Used",
        description: "Files and durable records the plan was grounded in.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Read", "Grep", "Glob", "RetrieveContext"],
    },
    OutputSectionEntry {
        id: "decisions",
        label: "Decisions",
        description: "Each accepted design choice with its rationale and rejected alternatives.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "build_strategy",
        label: "Build Strategy",
        description: "High-level approach that frames the slice ordering.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "slices",
        label: "Slices",
        description: "Ordered, executable units of work with stable ids (P0-S1, P0-S2, …) — the live plan tray.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["todo"],
    },
    OutputSectionEntry {
        id: "build_handoff",
        label: "Build Handoff",
        description: "The seed prompt and starting slice that hand control to the Engineer agent.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "risks",
        label: "Risks",
        description: "Known failure modes and the mitigations baked into the plan.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "open_questions",
        label: "Open Questions",
        description: "Items the user must resolve before or during the build.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
];

const CRAWL_REPORT_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "overview",
        label: "Overview",
        description: "High-level summary of what the project is and how it is organised.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Read", "Glob"],
    },
    OutputSectionEntry {
        id: "tech_stack",
        label: "Tech Stack",
        description: "Languages, frameworks, runtimes, and major libraries detected.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Read", "Glob"],
    },
    OutputSectionEntry {
        id: "commands",
        label: "Commands",
        description: "Build, test, lint, and run commands extracted from manifests and docs.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Read", "Grep"],
    },
    OutputSectionEntry {
        id: "tests",
        label: "Tests",
        description: "Test runners, conventions, and coverage hot spots.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Glob", "Grep"],
    },
    OutputSectionEntry {
        id: "architecture",
        label: "Architecture",
        description: "Module map, key boundaries, and where the major flows live.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Read", "Grep", "Glob"],
    },
    OutputSectionEntry {
        id: "hotspots",
        label: "Hotspots",
        description: "Files or modules that are central, fragile, or frequently modified.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Grep"],
    },
    OutputSectionEntry {
        id: "constraints",
        label: "Constraints",
        description: "Conventions, style rules, and other discovered guardrails.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Read"],
    },
    OutputSectionEntry {
        id: "unknowns",
        label: "Unknowns",
        description: "Things the crawl could not confidently answer; flagged for follow-up.",
        emphasis: AgentOutputSectionEmphasisDto::Optional,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "freshness",
        label: "Freshness",
        description: "When the crawl ran and which inputs are likely to drift first.",
        emphasis: AgentOutputSectionEmphasisDto::Optional,
        produced_by_tools: &[],
    },
];

const ENGINEERING_SUMMARY_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "files_changed",
        label: "Files Changed",
        description: "Per-file edit summary with a one-line rationale per change.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Edit", "Write", "NotebookEdit"],
    },
    OutputSectionEntry {
        id: "verification",
        label: "Verification",
        description: "Commands run, tests executed, and their pass/fail outcome.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Bash"],
    },
    OutputSectionEntry {
        id: "blockers",
        label: "Blockers",
        description: "Anything that prevented completion or required user input.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "handoff_context",
        label: "Handoff Context",
        description: "Durable notes for the next agent or future runs.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
];

const DEBUG_SUMMARY_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "symptom",
        label: "Symptom",
        description: "What the user observed and the expected behaviour.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "root_cause",
        label: "Root Cause",
        description: "The underlying defect, supported by the evidence collected.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Read", "Grep", "Bash"],
    },
    OutputSectionEntry {
        id: "fix",
        label: "Fix",
        description: "The narrowest change that resolves the root cause.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Edit", "Write"],
    },
    OutputSectionEntry {
        id: "files_changed",
        label: "Files Changed",
        description: "Per-file edit summary for the fix.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &["Edit", "Write"],
    },
    OutputSectionEntry {
        id: "verification",
        label: "Verification",
        description: "Reproduction confirmed gone, plus checks for adjacent regressions.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &["Bash"],
    },
    OutputSectionEntry {
        id: "saved_knowledge",
        label: "Saved Knowledge",
        description: "Durable findings persisted for future debugging sessions.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "remaining_risks",
        label: "Remaining Risks",
        description: "Anything still uncertain or worth watching after the fix.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
];

const AGENT_DEFINITION_DRAFT_SECTIONS: &[OutputSectionEntry] = &[
    OutputSectionEntry {
        id: "definition_draft",
        label: "Definition Draft",
        description: "Reviewable agent definition: id, prompts, tool policy, safety limits.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "validation",
        label: "Validation Diagnostics",
        description: "Schema and policy lint results, with blocking errors highlighted.",
        emphasis: AgentOutputSectionEmphasisDto::Core,
        produced_by_tools: &[],
    },
    OutputSectionEntry {
        id: "activation",
        label: "Activation Outcome",
        description: "On approval, the persisted version id and definition row.",
        emphasis: AgentOutputSectionEmphasisDto::Standard,
        produced_by_tools: &[],
    },
];

pub const fn output_sections_for(
    contract: RuntimeAgentOutputContractDto,
) -> &'static [OutputSectionEntry] {
    match contract {
        RuntimeAgentOutputContractDto::Answer => ANSWER_SECTIONS,
        RuntimeAgentOutputContractDto::PlanPack => PLAN_PACK_SECTIONS,
        RuntimeAgentOutputContractDto::CrawlReport => CRAWL_REPORT_SECTIONS,
        RuntimeAgentOutputContractDto::EngineeringSummary => ENGINEERING_SUMMARY_SECTIONS,
        RuntimeAgentOutputContractDto::DebugSummary => DEBUG_SUMMARY_SECTIONS,
        RuntimeAgentOutputContractDto::AgentDefinitionDraft => AGENT_DEFINITION_DRAFT_SECTIONS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_pack_includes_decisions_and_slices_as_core() {
        let sections = output_sections_for(RuntimeAgentOutputContractDto::PlanPack);
        let decisions = sections
            .iter()
            .find(|section| section.id == "decisions")
            .expect("plan pack must include a `decisions` section");
        let slices = sections
            .iter()
            .find(|section| section.id == "slices")
            .expect("plan pack must include a `slices` section");
        assert_eq!(decisions.emphasis, AgentOutputSectionEmphasisDto::Core);
        assert_eq!(slices.emphasis, AgentOutputSectionEmphasisDto::Core);
    }

    #[test]
    fn engineering_summary_lists_files_changed() {
        let sections = output_sections_for(RuntimeAgentOutputContractDto::EngineeringSummary);
        assert!(sections.iter().any(|section| section.id == "files_changed"));
        assert!(sections.iter().any(|section| section.id == "verification"));
    }

    #[test]
    fn every_contract_has_at_least_one_core_section() {
        for contract in [
            RuntimeAgentOutputContractDto::Answer,
            RuntimeAgentOutputContractDto::PlanPack,
            RuntimeAgentOutputContractDto::CrawlReport,
            RuntimeAgentOutputContractDto::EngineeringSummary,
            RuntimeAgentOutputContractDto::DebugSummary,
            RuntimeAgentOutputContractDto::AgentDefinitionDraft,
        ] {
            let sections = output_sections_for(contract);
            assert!(
                sections
                    .iter()
                    .any(|section| section.emphasis == AgentOutputSectionEmphasisDto::Core),
                "{contract:?} must have at least one Core section"
            );
        }
    }
}
