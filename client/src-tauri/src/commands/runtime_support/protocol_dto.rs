use crate::{
    commands::{
        AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleCacheDto,
        AutonomousSkillLifecycleDiagnosticDto, AutonomousSkillLifecycleResultDto,
        AutonomousSkillLifecycleSourceDto, AutonomousSkillLifecycleStageDto,
        BrowserComputerUseActionStatusDto, BrowserComputerUseSurfaceDto,
        BrowserComputerUseToolResultSummaryDto, CommandToolResultSummaryDto,
        FileToolResultSummaryDto, GitToolResultScopeDto, GitToolResultSummaryDto,
        McpCapabilityKindDto, McpCapabilityToolResultSummaryDto, ToolResultSummaryDto,
        WebToolResultContentKindDto, WebToolResultSummaryDto,
    },
    db::project_store::{
        AutonomousSkillCacheStatusRecord, AutonomousSkillLifecycleCacheRecord,
        AutonomousSkillLifecycleDiagnosticRecord, AutonomousSkillLifecycleResultRecord,
        AutonomousSkillLifecycleSourceRecord, AutonomousSkillLifecycleStageRecord,
    },
    runtime::protocol::{
        BrowserComputerUseActionStatus, BrowserComputerUseSurface, GitToolResultScope,
        McpCapabilityKind, SupervisorSkillCacheStatus, SupervisorSkillDiagnostic,
        SupervisorSkillLifecycleResult, SupervisorSkillLifecycleStage,
        SupervisorSkillSourceMetadata, ToolResultSummary, WebToolResultContentKind,
    },
};

pub(crate) fn tool_result_summary_dto_from_protocol(
    summary: &ToolResultSummary,
) -> ToolResultSummaryDto {
    match summary {
        ToolResultSummary::Command(summary) => {
            ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
                exit_code: summary.exit_code,
                timed_out: summary.timed_out,
                stdout_truncated: summary.stdout_truncated,
                stderr_truncated: summary.stderr_truncated,
                stdout_redacted: summary.stdout_redacted,
                stderr_redacted: summary.stderr_redacted,
            })
        }
        ToolResultSummary::File(summary) => ToolResultSummaryDto::File(FileToolResultSummaryDto {
            path: summary.path.clone(),
            scope: summary.scope.clone(),
            line_count: summary.line_count,
            match_count: summary.match_count,
            truncated: summary.truncated,
        }),
        ToolResultSummary::Git(summary) => ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: summary.scope.clone().map(git_tool_result_scope_dto),
            changed_files: summary.changed_files,
            truncated: summary.truncated,
            base_revision: summary.base_revision.clone(),
        }),
        ToolResultSummary::Web(summary) => ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: summary.target.clone(),
            result_count: summary.result_count,
            final_url: summary.final_url.clone(),
            content_kind: summary
                .content_kind
                .clone()
                .map(web_tool_result_content_kind_dto),
            content_type: summary.content_type.clone(),
            truncated: summary.truncated,
        }),
        ToolResultSummary::BrowserComputerUse(summary) => {
            ToolResultSummaryDto::BrowserComputerUse(BrowserComputerUseToolResultSummaryDto {
                surface: browser_computer_use_surface_dto(summary.surface.clone()),
                action: summary.action.clone(),
                status: browser_computer_use_action_status_dto(summary.status.clone()),
                target: summary.target.clone(),
                outcome: summary.outcome.clone(),
            })
        }
        ToolResultSummary::McpCapability(summary) => {
            ToolResultSummaryDto::McpCapability(McpCapabilityToolResultSummaryDto {
                server_id: summary.server_id.clone(),
                capability_kind: mcp_capability_kind_dto(summary.capability_kind.clone()),
                capability_id: summary.capability_id.clone(),
                capability_name: summary.capability_name.clone(),
            })
        }
    }
}

pub(crate) fn autonomous_skill_lifecycle_stage_dto_from_protocol(
    stage: SupervisorSkillLifecycleStage,
) -> AutonomousSkillLifecycleStageDto {
    match stage {
        SupervisorSkillLifecycleStage::Discovery => AutonomousSkillLifecycleStageDto::Discovery,
        SupervisorSkillLifecycleStage::Install => AutonomousSkillLifecycleStageDto::Install,
        SupervisorSkillLifecycleStage::Invoke => AutonomousSkillLifecycleStageDto::Invoke,
    }
}

pub(crate) fn autonomous_skill_lifecycle_result_dto_from_protocol(
    result: SupervisorSkillLifecycleResult,
) -> AutonomousSkillLifecycleResultDto {
    match result {
        SupervisorSkillLifecycleResult::Succeeded => AutonomousSkillLifecycleResultDto::Succeeded,
        SupervisorSkillLifecycleResult::Failed => AutonomousSkillLifecycleResultDto::Failed,
    }
}

pub(crate) fn autonomous_skill_lifecycle_source_dto_from_protocol(
    source: &SupervisorSkillSourceMetadata,
) -> AutonomousSkillLifecycleSourceDto {
    AutonomousSkillLifecycleSourceDto {
        repo: source.repo.clone(),
        path: source.path.clone(),
        reference: source.reference.clone(),
        tree_hash: source.tree_hash.clone(),
    }
}

pub(crate) fn autonomous_skill_cache_status_dto_from_protocol(
    status: SupervisorSkillCacheStatus,
) -> AutonomousSkillCacheStatusDto {
    match status {
        SupervisorSkillCacheStatus::Miss => AutonomousSkillCacheStatusDto::Miss,
        SupervisorSkillCacheStatus::Hit => AutonomousSkillCacheStatusDto::Hit,
        SupervisorSkillCacheStatus::Refreshed => AutonomousSkillCacheStatusDto::Refreshed,
    }
}

pub(crate) fn autonomous_skill_lifecycle_diagnostic_dto_from_protocol(
    diagnostic: &SupervisorSkillDiagnostic,
) -> AutonomousSkillLifecycleDiagnosticDto {
    AutonomousSkillLifecycleDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

pub(super) fn autonomous_skill_lifecycle_source_dto_from_record(
    source: &AutonomousSkillLifecycleSourceRecord,
) -> AutonomousSkillLifecycleSourceDto {
    AutonomousSkillLifecycleSourceDto {
        repo: source.repo.clone(),
        path: source.path.clone(),
        reference: source.reference.clone(),
        tree_hash: source.tree_hash.clone(),
    }
}

pub(super) fn autonomous_skill_lifecycle_cache_dto_from_record(
    cache: &AutonomousSkillLifecycleCacheRecord,
) -> AutonomousSkillLifecycleCacheDto {
    AutonomousSkillLifecycleCacheDto {
        key: cache.key.clone(),
        status: cache.status.as_ref().map(autonomous_skill_cache_status_dto),
    }
}

pub(super) fn autonomous_skill_lifecycle_diagnostic_dto_from_record(
    diagnostic: &AutonomousSkillLifecycleDiagnosticRecord,
) -> AutonomousSkillLifecycleDiagnosticDto {
    AutonomousSkillLifecycleDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

pub(super) fn autonomous_skill_lifecycle_stage_dto(
    stage: &AutonomousSkillLifecycleStageRecord,
) -> AutonomousSkillLifecycleStageDto {
    match stage {
        AutonomousSkillLifecycleStageRecord::Discovery => {
            AutonomousSkillLifecycleStageDto::Discovery
        }
        AutonomousSkillLifecycleStageRecord::Install => AutonomousSkillLifecycleStageDto::Install,
        AutonomousSkillLifecycleStageRecord::Invoke => AutonomousSkillLifecycleStageDto::Invoke,
    }
}

pub(super) fn autonomous_skill_lifecycle_result_dto(
    result: &AutonomousSkillLifecycleResultRecord,
) -> AutonomousSkillLifecycleResultDto {
    match result {
        AutonomousSkillLifecycleResultRecord::Succeeded => {
            AutonomousSkillLifecycleResultDto::Succeeded
        }
        AutonomousSkillLifecycleResultRecord::Failed => AutonomousSkillLifecycleResultDto::Failed,
    }
}

pub(super) fn autonomous_skill_cache_status_dto(
    status: &AutonomousSkillCacheStatusRecord,
) -> AutonomousSkillCacheStatusDto {
    match status {
        AutonomousSkillCacheStatusRecord::Miss => AutonomousSkillCacheStatusDto::Miss,
        AutonomousSkillCacheStatusRecord::Hit => AutonomousSkillCacheStatusDto::Hit,
        AutonomousSkillCacheStatusRecord::Refreshed => AutonomousSkillCacheStatusDto::Refreshed,
    }
}

fn git_tool_result_scope_dto(scope: GitToolResultScope) -> GitToolResultScopeDto {
    match scope {
        GitToolResultScope::Staged => GitToolResultScopeDto::Staged,
        GitToolResultScope::Unstaged => GitToolResultScopeDto::Unstaged,
        GitToolResultScope::Worktree => GitToolResultScopeDto::Worktree,
    }
}

fn web_tool_result_content_kind_dto(kind: WebToolResultContentKind) -> WebToolResultContentKindDto {
    match kind {
        WebToolResultContentKind::Html => WebToolResultContentKindDto::Html,
        WebToolResultContentKind::PlainText => WebToolResultContentKindDto::PlainText,
    }
}

fn browser_computer_use_surface_dto(surface: BrowserComputerUseSurface) -> BrowserComputerUseSurfaceDto {
    match surface {
        BrowserComputerUseSurface::Browser => BrowserComputerUseSurfaceDto::Browser,
        BrowserComputerUseSurface::ComputerUse => BrowserComputerUseSurfaceDto::ComputerUse,
    }
}

fn browser_computer_use_action_status_dto(
    status: BrowserComputerUseActionStatus,
) -> BrowserComputerUseActionStatusDto {
    match status {
        BrowserComputerUseActionStatus::Pending => BrowserComputerUseActionStatusDto::Pending,
        BrowserComputerUseActionStatus::Running => BrowserComputerUseActionStatusDto::Running,
        BrowserComputerUseActionStatus::Succeeded => BrowserComputerUseActionStatusDto::Succeeded,
        BrowserComputerUseActionStatus::Failed => BrowserComputerUseActionStatusDto::Failed,
        BrowserComputerUseActionStatus::Blocked => BrowserComputerUseActionStatusDto::Blocked,
    }
}

fn mcp_capability_kind_dto(kind: McpCapabilityKind) -> McpCapabilityKindDto {
    match kind {
        McpCapabilityKind::Tool => McpCapabilityKindDto::Tool,
        McpCapabilityKind::Resource => McpCapabilityKindDto::Resource,
        McpCapabilityKind::Prompt => McpCapabilityKindDto::Prompt,
        McpCapabilityKind::Command => McpCapabilityKindDto::Command,
    }
}
