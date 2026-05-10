use crate::commands::{RuntimeAgentIdDto, RuntimeAgentOutputContractDto};

/// Static description of an upstream artifact this agent reads from another
/// agent. Authored per agent so the inspector can render a "Consumes" lane
/// showing real handoffs rather than treating each agent as an island.
#[derive(Debug, Clone, Copy)]
pub struct ConsumedArtifactEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub source_agent: RuntimeAgentIdDto,
    pub contract: RuntimeAgentOutputContractDto,
    pub sections: &'static [&'static str],
    pub required: bool,
}

const ENGINEER_CONSUMES: &[ConsumedArtifactEntry] = &[ConsumedArtifactEntry {
    id: "plan_pack",
    label: "Accepted Plan Pack",
    description:
        "The xero.plan_pack.v1 the operator accepted. Engineer treats Slices as the live build queue and Build Handoff as its seed prompt.",
    source_agent: RuntimeAgentIdDto::Plan,
    contract: RuntimeAgentOutputContractDto::PlanPack,
    sections: &["decisions", "build_strategy", "slices", "build_handoff"],
    required: true,
}];

const DEBUG_CONSUMES: &[ConsumedArtifactEntry] = &[
    ConsumedArtifactEntry {
        id: "engineering_summary",
        label: "Latest Engineering Summary",
        description: "Recent file changes and verification results to scope the failure window.",
        source_agent: RuntimeAgentIdDto::Engineer,
        contract: RuntimeAgentOutputContractDto::EngineeringSummary,
        sections: &["files_changed", "verification"],
        required: false,
    },
    ConsumedArtifactEntry {
        id: "plan_pack",
        label: "Accepted Plan Pack",
        description: "Optional reference to the plan that produced the failing implementation.",
        source_agent: RuntimeAgentIdDto::Plan,
        contract: RuntimeAgentOutputContractDto::PlanPack,
        sections: &["decisions", "slices"],
        required: false,
    },
];

pub const fn consumed_artifacts_for(id: RuntimeAgentIdDto) -> &'static [ConsumedArtifactEntry] {
    match id {
        RuntimeAgentIdDto::Engineer => ENGINEER_CONSUMES,
        RuntimeAgentIdDto::Debug => DEBUG_CONSUMES,
        RuntimeAgentIdDto::Ask
        | RuntimeAgentIdDto::Plan
        | RuntimeAgentIdDto::Crawl
        | RuntimeAgentIdDto::AgentCreate => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engineer_consumes_plan_pack_with_required_flag() {
        let consumes = consumed_artifacts_for(RuntimeAgentIdDto::Engineer);
        assert_eq!(consumes.len(), 1);
        let plan_pack = &consumes[0];
        assert_eq!(plan_pack.id, "plan_pack");
        assert_eq!(plan_pack.source_agent, RuntimeAgentIdDto::Plan);
        assert_eq!(plan_pack.contract, RuntimeAgentOutputContractDto::PlanPack);
        assert!(plan_pack.required);
        assert!(plan_pack.sections.contains(&"slices"));
    }

    #[test]
    fn debug_consumes_engineering_summary() {
        let consumes = consumed_artifacts_for(RuntimeAgentIdDto::Debug);
        assert!(consumes
            .iter()
            .any(|entry| entry.contract == RuntimeAgentOutputContractDto::EngineeringSummary));
    }

    #[test]
    fn ask_and_plan_consume_nothing() {
        assert!(consumed_artifacts_for(RuntimeAgentIdDto::Ask).is_empty());
        assert!(consumed_artifacts_for(RuntimeAgentIdDto::Plan).is_empty());
    }
}
