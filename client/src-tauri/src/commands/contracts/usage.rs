use serde::{Deserialize, Serialize};

use crate::db::project_store::{ProjectUsageModelBreakdownRecord, ProjectUsageTotalsRecord};

/// Cross-run totals for a project. Powers the footer "1.28M tok · $18.42"
/// display and the sidebar header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUsageTotalsDto {
    pub run_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<String>,
}

/// One row of the usage sidebar's per-model table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUsageModelBreakdownDto {
    pub provider_id: String,
    pub model_id: String,
    pub run_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<String>,
}

/// Top-level response for `get_project_usage_summary`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUsageSummaryDto {
    pub project_id: String,
    pub totals: ProjectUsageTotalsDto,
    pub by_model: Vec<ProjectUsageModelBreakdownDto>,
}

pub fn project_usage_totals_dto(record: ProjectUsageTotalsRecord) -> ProjectUsageTotalsDto {
    ProjectUsageTotalsDto {
        run_count: record.run_count,
        input_tokens: record.input_tokens,
        output_tokens: record.output_tokens,
        total_tokens: record.total_tokens,
        cache_read_tokens: record.cache_read_tokens,
        cache_creation_tokens: record.cache_creation_tokens,
        estimated_cost_micros: record.estimated_cost_micros,
        last_updated_at: record.last_updated_at,
    }
}

pub fn project_usage_model_breakdown_dto(
    record: ProjectUsageModelBreakdownRecord,
) -> ProjectUsageModelBreakdownDto {
    ProjectUsageModelBreakdownDto {
        provider_id: record.provider_id,
        model_id: record.model_id,
        run_count: record.run_count,
        input_tokens: record.input_tokens,
        output_tokens: record.output_tokens,
        total_tokens: record.total_tokens,
        cache_read_tokens: record.cache_read_tokens,
        cache_creation_tokens: record.cache_creation_tokens,
        estimated_cost_micros: record.estimated_cost_micros,
        last_updated_at: record.last_updated_at,
    }
}
