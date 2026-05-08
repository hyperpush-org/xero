use crate::commands::{AgentDbTouchpointKindDto, AgentTriggerLifecycleEventDto, RuntimeAgentIdDto};

/// A typed reference describing what causes the agent to touch a table.
/// Mirrors the wire-side `AgentTriggerRefDto` so the descriptor authors keep
/// stable references that survive translation to the inspector contract.
#[derive(Debug, Clone, Copy)]
pub enum TriggerRef {
    Tool(&'static str),
    OutputSection(&'static str),
    Lifecycle(AgentTriggerLifecycleEventDto),
    UpstreamArtifact(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct DbTouchpointEntry {
    pub table: &'static str,
    pub kind: AgentDbTouchpointKindDto,
    pub purpose: &'static str,
    pub triggers: &'static [TriggerRef],
    pub columns: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct DbTouchpoints {
    pub entries: &'static [DbTouchpointEntry],
}

const RUN_LIFECYCLE_TRIGGERS: &[TriggerRef] = &[
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::RunStart),
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::StateTransition),
];

const RUN_PERSISTENCE_TRIGGERS: &[TriggerRef] = &[
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::StateTransition),
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::RunComplete),
];

const MESSAGE_PERSISTENCE_TRIGGERS: &[TriggerRef] = &[TriggerRef::Lifecycle(
    AgentTriggerLifecycleEventDto::MessagePersisted,
)];

const TOOL_EVENT_TRIGGERS: &[TriggerRef] = &[
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::ToolCall),
    TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::StateTransition),
];

const SESSION_READ_TRIGGERS: &[TriggerRef] = &[TriggerRef::Lifecycle(
    AgentTriggerLifecycleEventDto::RunStart,
)];

/// Common reads that every agent performs at run boot. Keeping these in one
/// const removes drift across agents — if you change the descriptor of one
/// of these, you change it for everybody.
const COMMON_READ_ENTRIES: &[DbTouchpointEntry] = &[
    DbTouchpointEntry {
        table: "agent_sessions",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "loads the current chat session before resuming or starting a run",
        triggers: SESSION_READ_TRIGGERS,
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_runs",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "looks up the active run, its state, and any prior stop reason",
        triggers: RUN_LIFECYCLE_TRIGGERS,
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_messages",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads the conversation transcript for context window assembly",
        triggers: SESSION_READ_TRIGGERS,
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_events",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "replays prior runtime events when resuming or compacting",
        triggers: SESSION_READ_TRIGGERS,
        columns: &[],
    },
];

const COMMON_WRITE_ENTRIES: &[DbTouchpointEntry] = &[
    DbTouchpointEntry {
        table: "agent_runs",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "records every state-machine transition, stop reason, and final outcome",
        triggers: RUN_PERSISTENCE_TRIGGERS,
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_messages",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "persists each user / assistant turn as it streams in",
        triggers: MESSAGE_PERSISTENCE_TRIGGERS,
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_events",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "appends one structured event per tool call, state transition, and gate decision",
        triggers: TOOL_EVENT_TRIGGERS,
        columns: &[],
    },
];

const ASK_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "retrieves durable facts, decisions, and prior answers to ground the response",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_memory",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks long-term memory for relevant prior conversations",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    COMMON_WRITE_ENTRIES[2],
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "captures discovered facts and citations as durable context for future runs",
        triggers: &[TriggerRef::OutputSection("citations")],
        columns: &[],
    },
];

const PLAN_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "loads prior decisions, constraints, and accepted plans for context",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_memory",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks long-term memory for prior planning conversations",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    DbTouchpointEntry {
        table: "agent_events",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "emits PlanUpdated events as the live plan tray and accepted Plan Pack evolve",
        triggers: &[
            TriggerRef::Tool("todo"),
            TriggerRef::OutputSection("slices"),
            TriggerRef::OutputSection("decisions"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::PlanUpdate),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "persists the accepted Plan Pack as a durable `plan` record on acceptance",
        triggers: &[
            TriggerRef::OutputSection("slices"),
            TriggerRef::OutputSection("decisions"),
            TriggerRef::OutputSection("build_handoff"),
        ],
        columns: &[],
    },
];

const ENGINEER_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "agent_file_reservations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks for conflicting edits before opening a file for write",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Tool("NotebookEdit"),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "looks up prior file mutations to compute deltas and rollback points",
        triggers: &[TriggerRef::Tool("Read")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_rollback_operations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "inspects pending rollbacks before re-running edits",
        triggers: &[TriggerRef::Lifecycle(
            AgentTriggerLifecycleEventDto::StateTransition,
        )],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads the current workspace head before staging new edits",
        triggers: &[TriggerRef::Tool("Edit"), TriggerRef::Tool("Write")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "consumes the accepted Plan Pack and prior Engineering Summaries",
        triggers: &[
            TriggerRef::Tool("RetrieveContext"),
            TriggerRef::UpstreamArtifact("plan_pack"),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_memory",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks long-term memory for relevant prior implementation work",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    COMMON_WRITE_ENTRIES[2],
    DbTouchpointEntry {
        table: "agent_file_reservations",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "claims a reservation per file before opening it, releases on completion",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Tool("NotebookEdit"),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "appends one operation row per file mutation for diff and rollback",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Tool("NotebookEdit"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::FileEdit),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "advances the workspace head pointer after each successful mutation",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::FileEdit),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "captures handoff notes, findings, and verification results for future agents",
        triggers: &[TriggerRef::OutputSection("handoff_context")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_mailbox_items",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "drops a mailbox item when delegating follow-up to another agent",
        triggers: &[],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_coordination_events",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "records cross-agent coordination signals for the workflow timeline",
        triggers: &[],
        columns: &[],
    },
];

const DEBUG_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "agent_file_reservations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks for conflicting edits before applying a fix",
        triggers: &[TriggerRef::Tool("Edit"), TriggerRef::Tool("Write")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reviews prior mutations while building a hypothesis",
        triggers: &[TriggerRef::Tool("Read")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_rollback_operations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "inspects pending rollbacks when narrowing in on a regression",
        triggers: &[TriggerRef::Lifecycle(
            AgentTriggerLifecycleEventDto::StateTransition,
        )],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads the workspace head before applying an experimental fix",
        triggers: &[TriggerRef::Tool("Edit"), TriggerRef::Tool("Write")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "consumes prior Engineering Summaries and saved debug findings",
        triggers: &[
            TriggerRef::Tool("RetrieveContext"),
            TriggerRef::UpstreamArtifact("engineering_summary"),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_memory",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "checks long-term memory for similar prior incidents",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    COMMON_WRITE_ENTRIES[2],
    DbTouchpointEntry {
        table: "agent_file_reservations",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "claims a reservation per file when applying the narrowest fix",
        triggers: &[TriggerRef::Tool("Edit"), TriggerRef::Tool("Write")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "appends an operation row per fix attempt for rollback if it fails",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::FileEdit),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "advances the workspace head when a fix verifies cleanly",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::VerificationGate),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "captures the symptom, root cause, and saved knowledge for future incidents",
        triggers: &[
            TriggerRef::OutputSection("root_cause"),
            TriggerRef::OutputSection("saved_knowledge"),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_mailbox_items",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "delegates remaining cleanup work back to Engineer when applicable",
        triggers: &[],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_coordination_events",
        kind: AgentDbTouchpointKindDto::Encouraged,
        purpose: "records cross-agent coordination signals for the workflow timeline",
        triggers: &[],
        columns: &[],
    },
];

const CRAWL_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    DbTouchpointEntry {
        table: "agent_events",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "emits one event per file probed and per section emitted into the report",
        triggers: &[
            TriggerRef::Tool("Read"),
            TriggerRef::Tool("Glob"),
            TriggerRef::Tool("Grep"),
            TriggerRef::OutputSection("tech_stack"),
            TriggerRef::OutputSection("architecture"),
        ],
        columns: &[],
    },
];

const AGENT_CREATE_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "agent_definitions",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads existing definitions so drafts do not collide with a registered id",
        triggers: &[TriggerRef::Lifecycle(
            AgentTriggerLifecycleEventDto::RunStart,
        )],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_definition_versions",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "loads the prior version when revising an existing custom agent",
        triggers: &[TriggerRef::Lifecycle(
            AgentTriggerLifecycleEventDto::RunStart,
        )],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "project_context_records",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "looks up project facts the new agent's prompt should ground in",
        triggers: &[TriggerRef::Tool("RetrieveContext")],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    COMMON_WRITE_ENTRIES[2],
    DbTouchpointEntry {
        table: "agent_definitions",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "upserts the agent definition row when the operator activates a draft",
        triggers: &[
            TriggerRef::OutputSection("definition_draft"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::DefinitionPersisted),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "agent_definition_versions",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "writes a new version snapshot per activation, preserving prior versions",
        triggers: &[
            TriggerRef::OutputSection("definition_draft"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::DefinitionPersisted),
        ],
        columns: &[],
    },
];

const TEST_ENTRIES: &[DbTouchpointEntry] = &[
    COMMON_READ_ENTRIES[0],
    COMMON_READ_ENTRIES[1],
    COMMON_READ_ENTRIES[2],
    COMMON_READ_ENTRIES[3],
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads the canonical edit ledger to compare against harness expectations",
        triggers: &[TriggerRef::Tool("Read")],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Read,
        purpose: "reads the workspace head to verify the harness ended on the expected commit",
        triggers: &[TriggerRef::Lifecycle(
            AgentTriggerLifecycleEventDto::VerificationGate,
        )],
        columns: &[],
    },
    COMMON_WRITE_ENTRIES[0],
    COMMON_WRITE_ENTRIES[1],
    COMMON_WRITE_ENTRIES[2],
    DbTouchpointEntry {
        table: "code_history_operations",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "writes harness-driven mutations through the same ledger as real edits",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Tool("Write"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::FileEdit),
        ],
        columns: &[],
    },
    DbTouchpointEntry {
        table: "code_workspace_heads",
        kind: AgentDbTouchpointKindDto::Write,
        purpose: "advances the workspace head as the harness executes its scripted edits",
        triggers: &[
            TriggerRef::Tool("Edit"),
            TriggerRef::Lifecycle(AgentTriggerLifecycleEventDto::FileEdit),
        ],
        columns: &[],
    },
];

pub const fn db_touchpoints_for_runtime_agent(id: RuntimeAgentIdDto) -> DbTouchpoints {
    let entries = match id {
        RuntimeAgentIdDto::Ask => ASK_ENTRIES,
        RuntimeAgentIdDto::Plan => PLAN_ENTRIES,
        RuntimeAgentIdDto::Engineer => ENGINEER_ENTRIES,
        RuntimeAgentIdDto::Debug => DEBUG_ENTRIES,
        RuntimeAgentIdDto::Crawl => CRAWL_ENTRIES,
        RuntimeAgentIdDto::AgentCreate => AGENT_CREATE_ENTRIES,
        RuntimeAgentIdDto::Test => TEST_ENTRIES,
    };
    DbTouchpoints { entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_table(touchpoints: DbTouchpoints, kind: AgentDbTouchpointKindDto, table: &str) -> bool {
        touchpoints
            .entries
            .iter()
            .any(|entry| entry.kind == kind && entry.table == table)
    }

    #[test]
    fn every_runtime_agent_has_at_least_common_writes() {
        for id in [
            RuntimeAgentIdDto::Ask,
            RuntimeAgentIdDto::Plan,
            RuntimeAgentIdDto::Engineer,
            RuntimeAgentIdDto::Debug,
            RuntimeAgentIdDto::Crawl,
            RuntimeAgentIdDto::AgentCreate,
            RuntimeAgentIdDto::Test,
        ] {
            let touchpoints = db_touchpoints_for_runtime_agent(id);
            assert!(has_table(
                touchpoints,
                AgentDbTouchpointKindDto::Write,
                "agent_runs"
            ));
            assert!(has_table(
                touchpoints,
                AgentDbTouchpointKindDto::Write,
                "agent_messages"
            ));
            assert!(has_table(
                touchpoints,
                AgentDbTouchpointKindDto::Write,
                "agent_events"
            ));
        }
    }

    #[test]
    fn engineer_writes_code_history() {
        let touchpoints = db_touchpoints_for_runtime_agent(RuntimeAgentIdDto::Engineer);
        assert!(has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "code_history_operations"
        ));
        assert!(has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "code_workspace_heads"
        ));
    }

    #[test]
    fn ask_does_not_write_code() {
        let touchpoints = db_touchpoints_for_runtime_agent(RuntimeAgentIdDto::Ask);
        assert!(!has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "code_history_operations"
        ));
        assert!(!has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "code_workspace_heads"
        ));
    }

    #[test]
    fn agent_create_touches_definition_tables() {
        let touchpoints = db_touchpoints_for_runtime_agent(RuntimeAgentIdDto::AgentCreate);
        assert!(has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "agent_definitions"
        ));
        assert!(has_table(
            touchpoints,
            AgentDbTouchpointKindDto::Write,
            "agent_definition_versions"
        ));
    }

    #[test]
    fn engineer_code_history_write_lists_edit_tool_trigger() {
        let touchpoints = db_touchpoints_for_runtime_agent(RuntimeAgentIdDto::Engineer);
        let entry = touchpoints
            .entries
            .iter()
            .find(|entry| {
                entry.kind == AgentDbTouchpointKindDto::Write
                    && entry.table == "code_history_operations"
            })
            .expect("engineer code_history_operations write entry");
        assert!(!entry.purpose.is_empty(), "purpose must be authored");
        assert!(
            entry
                .triggers
                .iter()
                .any(|trigger| matches!(trigger, TriggerRef::Tool("Edit"))),
            "expected Edit tool trigger, got {:?}",
            entry.triggers
        );
    }

    #[test]
    fn plan_encouraged_project_records_links_to_slices_section() {
        let touchpoints = db_touchpoints_for_runtime_agent(RuntimeAgentIdDto::Plan);
        let entry = touchpoints
            .entries
            .iter()
            .find(|entry| {
                entry.kind == AgentDbTouchpointKindDto::Encouraged
                    && entry.table == "project_context_records"
            })
            .expect("plan encouraged project_context_records entry");
        assert!(
            entry
                .triggers
                .iter()
                .any(|trigger| matches!(trigger, TriggerRef::OutputSection(id) if *id == "slices")),
            "expected slices output section trigger, got {:?}",
            entry.triggers
        );
    }
}
