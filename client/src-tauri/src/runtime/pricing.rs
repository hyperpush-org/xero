//! Dynamic per-model token pricing for usage cost accounting.
//!
//! Pricing is intentionally sourced from provider-published endpoints/docs at
//! runtime and cached in OS app-data. Provider-reported cost remains the source
//! of truth when a provider supplies it with usage metadata.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::Duration as StdDuration,
};

use regex::Regex;
use reqwest::blocking::Client;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const PRICING_CACHE_KEY: &str = "official-model-pricing-v2";
const PRICING_CACHE_TTL_SECONDS: i64 = 12 * 60 * 60;
const PRICING_HTTP_TIMEOUT_SECONDS: u64 = 15;
const PRICING_USER_AGENT: &str = "Xero/0.1 model-pricing";

const OPENAI_PRICING_URL: &str = "https://developers.openai.com/api/docs/pricing.md";
const XAI_MODELS_URL: &str = "https://docs.x.ai/developers/models.md";
const XAI_MODEL_DETAIL_BASE_URL: &str = "https://docs.x.ai/developers/models";
const ANTHROPIC_PRICING_URL: &str = "https://docs.anthropic.com/en/docs/about-claude/pricing.md";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Per-million-token prices in micros (1 micro = $0.000001).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRate {
    pub input_per_1m_micros: u64,
    pub output_per_1m_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_1m_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_1m_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context_threshold_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context_input_per_1m_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context_output_per_1m_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context_cache_read_per_1m_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context_cache_write_per_1m_micros: Option<u64>,
}

/// Token usage we price against. Mirrors `ProviderUsage` but keeps the pricing
/// module independent from the agent runtime types.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsageForPricing {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PricingKey {
    provider_id: String,
    model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedModelRate {
    provider_id: String,
    model_id: String,
    source_url: String,
    rate: ModelRate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PricingCatalogSnapshot {
    fetched_at: String,
    rates: Vec<CachedModelRate>,
}

#[derive(Debug, Clone, Default)]
struct PricingCatalog {
    rates: BTreeMap<PricingKey, ModelRate>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
    #[serde(default)]
    input_cache_read: Option<String>,
    #[serde(default)]
    input_cache_write: Option<String>,
}

static PRICING_MEMORY_CACHE: LazyLock<Mutex<Option<PricingCatalogSnapshot>>> =
    LazyLock::new(|| Mutex::new(None));

/// Compute cost in micros for a usage record from published token prices.
///
/// Unknown or incompletely priced models return zero rather than inventing a
/// rate. Runtime provider code uses provider-reported cost before calling this
/// function.
pub fn estimate_cost_micros(provider_id: &str, model_id: &str, usage: UsageForPricing) -> u64 {
    if provider_is_zero_wallet_cost(provider_id) {
        return 0;
    }

    let Some(catalog) = load_pricing_catalog() else {
        return 0;
    };
    estimate_cost_micros_from_catalog(&catalog, provider_id, model_id, usage)
}

/// Recompute and persist the legacy `estimated_cost_micros` column for usage
/// rows with token activity. Direct-provider rows are recalculated from the
/// newest published catalog; OpenRouter rows are only filled when currently
/// zero because non-zero OpenRouter costs are usually provider-reported exact
/// costs.
pub fn backfill_agent_usage_costs(repo_root: &std::path::Path) -> usize {
    use crate::db::project_store::{
        list_agent_usage_cost_rows, update_agent_usage_cost, AgentUsageCostBackfillRow,
    };

    let Some(catalog) = load_pricing_catalog() else {
        return 0;
    };
    let rows = match list_agent_usage_cost_rows(repo_root) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!(
                "[pricing] backfill skipped for {}: {error}",
                repo_root.display()
            );
            return 0;
        }
    };

    let mut updated = 0usize;
    for AgentUsageCostBackfillRow {
        project_id,
        run_id,
        provider_id,
        model_id,
        billable_input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        estimated_cost_micros,
    } in rows
    {
        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            &provider_id,
            &model_id,
            UsageForPricing {
                input_tokens: billable_input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            },
        );
        if cost == 0 {
            continue;
        }
        if provider_id == "openrouter" && estimated_cost_micros > 0 {
            continue;
        }
        if cost == estimated_cost_micros {
            continue;
        }
        if let Err(error) = update_agent_usage_cost(repo_root, &project_id, &run_id, cost) {
            eprintln!("[pricing] failed to update cost for {project_id}/{run_id}: {error}");
            continue;
        }
        updated += 1;
    }
    updated
}

fn estimate_cost_micros_from_catalog(
    catalog: &PricingCatalog,
    provider_id: &str,
    model_id: &str,
    usage: UsageForPricing,
) -> u64 {
    if provider_is_zero_wallet_cost(provider_id) {
        return 0;
    }

    if let Some(cost) = lookup_catalog_rate(catalog, provider_id, model_id)
        .and_then(|rate| cost_for_usage_at_rate(rate, usage))
    {
        return cost;
    }

    if let Some(cost) = lookup_openrouter_fallback_rate(catalog, provider_id, model_id)
        .and_then(|rate| cost_for_usage_at_rate(rate, usage))
    {
        return cost;
    }

    0
}

fn provider_is_zero_wallet_cost(provider_id: &str) -> bool {
    matches!(provider_id, "ollama" | "github_models")
}

fn load_pricing_catalog() -> Option<PricingCatalog> {
    if let Some(snapshot) = load_memory_snapshot().filter(snapshot_is_fresh) {
        return Some(catalog_from_snapshot(snapshot));
    }

    let cached_snapshot = load_cached_snapshot();
    if let Some(snapshot) = cached_snapshot
        .as_ref()
        .filter(|snapshot| snapshot_is_fresh(snapshot))
    {
        store_memory_snapshot(snapshot.clone());
        return Some(catalog_from_snapshot(snapshot.clone()));
    }

    match fetch_pricing_snapshot() {
        Ok(snapshot) => {
            persist_cached_snapshot(&snapshot);
            store_memory_snapshot(snapshot.clone());
            Some(catalog_from_snapshot(snapshot))
        }
        Err(error) => {
            eprintln!("[pricing] official pricing refresh failed: {error}");
            cached_snapshot.map(catalog_from_snapshot)
        }
    }
}

fn load_memory_snapshot() -> Option<PricingCatalogSnapshot> {
    PRICING_MEMORY_CACHE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn store_memory_snapshot(snapshot: PricingCatalogSnapshot) {
    if let Ok(mut guard) = PRICING_MEMORY_CACHE.lock() {
        *guard = Some(snapshot);
    }
}

fn configured_global_database_path() -> Option<PathBuf> {
    crate::db::configured_app_data_dir().map(|dir| crate::global_db::global_database_path(&dir))
}

fn load_cached_snapshot() -> Option<PricingCatalogSnapshot> {
    let path = configured_global_database_path()?;
    let connection = crate::global_db::open_global_database(&path).ok()?;
    let payload = connection
        .query_row(
            "SELECT payload FROM model_pricing_catalog_cache WHERE cache_key = ?1",
            params![PRICING_CACHE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&payload).ok()
}

fn persist_cached_snapshot(snapshot: &PricingCatalogSnapshot) {
    let Some(path) = configured_global_database_path() else {
        return;
    };
    let Ok(payload) = serde_json::to_string(snapshot) else {
        return;
    };
    let Ok(connection) = crate::global_db::open_global_database(&path) else {
        return;
    };
    if let Err(error) = connection.execute(
        r#"
        INSERT INTO model_pricing_catalog_cache (cache_key, payload, fetched_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(cache_key) DO UPDATE SET
            payload = excluded.payload,
            fetched_at = excluded.fetched_at
        "#,
        params![PRICING_CACHE_KEY, payload, snapshot.fetched_at],
    ) {
        eprintln!("[pricing] failed to cache official pricing catalog: {error}");
    }
}

fn snapshot_is_fresh(snapshot: &PricingCatalogSnapshot) -> bool {
    snapshot_age_seconds(snapshot)
        .map(|seconds| seconds <= PRICING_CACHE_TTL_SECONDS)
        .unwrap_or(false)
}

fn snapshot_age_seconds(snapshot: &PricingCatalogSnapshot) -> Option<i64> {
    let fetched_at = OffsetDateTime::parse(&snapshot.fetched_at, &Rfc3339).ok()?;
    Some((OffsetDateTime::now_utc() - fetched_at).whole_seconds())
}

fn catalog_from_snapshot(snapshot: PricingCatalogSnapshot) -> PricingCatalog {
    let rates = snapshot
        .rates
        .into_iter()
        .map(|rate| {
            (
                PricingKey {
                    provider_id: rate.provider_id,
                    model_id: rate.model_id,
                },
                rate.rate,
            )
        })
        .collect();
    PricingCatalog { rates }
}

fn snapshot_from_rates(rates: BTreeMap<PricingKey, CachedModelRate>) -> PricingCatalogSnapshot {
    PricingCatalogSnapshot {
        fetched_at: crate::auth::now_timestamp(),
        rates: rates.into_values().collect(),
    }
}

fn fetch_pricing_snapshot() -> Result<PricingCatalogSnapshot, String> {
    let client = Client::builder()
        .timeout(StdDuration::from_secs(PRICING_HTTP_TIMEOUT_SECONDS))
        .user_agent(PRICING_USER_AGENT)
        .build()
        .map_err(|error| format!("could not build pricing HTTP client: {error}"))?;

    let mut rates = BTreeMap::new();
    let mut errors = Vec::new();

    for fetcher in [
        fetch_openai_rates
            as fn(&Client, &mut BTreeMap<PricingKey, CachedModelRate>) -> Result<usize, String>,
        fetch_xai_rates,
        fetch_anthropic_rates,
        fetch_openrouter_rates,
    ] {
        if let Err(error) = fetcher(&client, &mut rates) {
            errors.push(error);
        }
    }

    if rates.is_empty() {
        return Err(if errors.is_empty() {
            "no official pricing sources returned token rates".into()
        } else {
            errors.join("; ")
        });
    }

    Ok(snapshot_from_rates(rates))
}

fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("{url} unreachable: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url} returned HTTP {}", status.as_u16()));
    }
    response
        .text()
        .map_err(|error| format!("{url} response body unreadable: {error}"))
}

fn fetch_openai_rates(
    client: &Client,
    rates: &mut BTreeMap<PricingKey, CachedModelRate>,
) -> Result<usize, String> {
    let text = fetch_text(client, OPENAI_PRICING_URL)?;
    let rows = parse_openai_standard_rows(&text);
    let mut inserted = 0usize;
    for (model_id, rate) in rows {
        for provider_id in ["openai_api", "openai_codex"] {
            set_rate(rates, provider_id, &model_id, OPENAI_PRICING_URL, rate);
            inserted += 1;
        }
    }
    if inserted == 0 {
        return Err("OpenAI pricing docs did not contain standard token rows".into());
    }
    Ok(inserted)
}

fn parse_openai_standard_rows(text: &str) -> Vec<(String, ModelRate)> {
    let table_re = Regex::new(
        r#"(?s)<TextTokenPricingTables\s+client:load\s+tier="standard".*?rows=\{\[(?P<rows>.*?)\]\}"#,
    )
    .expect("valid OpenAI pricing table regex");
    let row_re = Regex::new(
        r#"\[\s*"(?P<model>[^"]+)"\s*,\s*(?P<input>[^,\]]+)\s*,\s*(?P<cached>[^,\]]+)\s*,\s*(?P<output>[^,\]]+)\s*\]"#,
    )
    .expect("valid OpenAI row regex");

    table_re
        .captures_iter(text)
        .flat_map(|table| {
            let rows = table
                .name("rows")
                .map(|rows| rows.as_str())
                .unwrap_or_default();
            row_re.captures_iter(rows).filter_map(|row| {
                let model_id = normalize_direct_model_key(row.name("model")?.as_str());
                if model_id.is_empty() {
                    return None;
                }
                let input = parse_js_usd_per_1m(row.name("input")?.as_str())?;
                let cached = parse_js_usd_per_1m(row.name("cached")?.as_str());
                let output = parse_js_usd_per_1m(row.name("output")?.as_str())?;
                Some((
                    model_id,
                    ModelRate {
                        input_per_1m_micros: input,
                        output_per_1m_micros: output,
                        cache_read_per_1m_micros: cached,
                        cache_write_per_1m_micros: None,
                        long_context_threshold_tokens: None,
                        long_context_input_per_1m_micros: None,
                        long_context_output_per_1m_micros: None,
                        long_context_cache_read_per_1m_micros: None,
                        long_context_cache_write_per_1m_micros: None,
                    },
                ))
            })
        })
        .collect()
}

fn fetch_xai_rates(
    client: &Client,
    rates: &mut BTreeMap<PricingKey, CachedModelRate>,
) -> Result<usize, String> {
    let models_text = fetch_text(client, XAI_MODELS_URL)?;
    let model_ids = parse_xai_model_ids(&models_text);
    if model_ids.is_empty() {
        return Err("xAI model docs did not contain language model ids".into());
    }

    let mut inserted = 0usize;
    let mut errors = Vec::new();
    for model_id in model_ids {
        let markdown_url = format!("{XAI_MODEL_DETAIL_BASE_URL}/{model_id}.md");
        let html_url = format!("{XAI_MODEL_DETAIL_BASE_URL}/{model_id}");
        let markdown = match fetch_text(client, &markdown_url) {
            Ok(markdown) => markdown,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        let Some(mut rate) = parse_xai_model_detail_rate(&markdown) else {
            errors.push(format!(
                "{markdown_url} did not contain a token pricing table"
            ));
            continue;
        };
        if let Ok(html) = fetch_text(client, &html_url) {
            if let Some(long_context_rate) = parse_xai_long_context_rate(&html, &model_id) {
                rate.long_context_threshold_tokens = Some(long_context_rate.threshold_tokens);
                rate.long_context_input_per_1m_micros = Some(long_context_rate.input);
                rate.long_context_output_per_1m_micros = Some(long_context_rate.output);
                rate.long_context_cache_read_per_1m_micros = Some(long_context_rate.cache_read);
                rate.long_context_cache_write_per_1m_micros = None;
            }
        }

        let aliases = parse_xai_model_aliases(&markdown);
        for alias in std::iter::once(model_id.as_str()).chain(aliases.iter().map(String::as_str)) {
            set_rate(rates, "xai", alias, &markdown_url, rate);
            inserted += 1;
        }
    }

    if inserted == 0 {
        return Err(if errors.is_empty() {
            "xAI pricing docs did not yield token rates".into()
        } else {
            errors.join("; ")
        });
    }
    Ok(inserted)
}

fn parse_xai_model_ids(text: &str) -> BTreeSet<String> {
    markdown_table_rows(text)
        .filter_map(|cells| {
            if cells.len() < 4 || cells.first().is_some_and(|cell| cell == "Model") {
                return None;
            }
            let model_id = cells[0].trim();
            if model_id.starts_with("grok-") && !model_id.contains("imagine") {
                Some(normalize_direct_model_key(model_id))
            } else {
                None
            }
        })
        .collect()
}

fn parse_xai_model_detail_rate(markdown: &str) -> Option<ModelRate> {
    let mut input = None;
    let mut cached = None;
    let mut output = None;

    for cells in markdown_table_rows(markdown) {
        if cells.len() < 2 {
            continue;
        }
        match cells[0].trim().to_ascii_lowercase().as_str() {
            "input" => input = parse_usd_per_1m_to_micros(&cells[1]),
            "cached input" => cached = parse_usd_per_1m_to_micros(&cells[1]),
            "output" => output = parse_usd_per_1m_to_micros(&cells[1]),
            _ => {}
        }
    }

    let input = input?;
    Some(ModelRate {
        input_per_1m_micros: input,
        output_per_1m_micros: output?,
        cache_read_per_1m_micros: cached,
        cache_write_per_1m_micros: None,
        long_context_threshold_tokens: None,
        long_context_input_per_1m_micros: None,
        long_context_output_per_1m_micros: None,
        long_context_cache_read_per_1m_micros: None,
        long_context_cache_write_per_1m_micros: None,
    })
}

fn parse_xai_model_aliases(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("- **Aliases:**")
                .map(|aliases| aliases.to_owned())
        })
        .map(|aliases| {
            aliases
                .split(',')
                .map(|alias| alias.trim().trim_matches('`'))
                .map(normalize_direct_model_key)
                .filter(|alias| !alias.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct XaiLongContextRate {
    threshold_tokens: u64,
    input: u64,
    output: u64,
    cache_read: u64,
}

fn parse_xai_long_context_rate(html: &str, model_id: &str) -> Option<XaiLongContextRate> {
    let decoded = html.replace("\\\"", "\"");
    let needle = format!("\"name\":\"{model_id}\"");
    let start = decoded.find(&needle)?;
    let end = decoded.len().min(start.saturating_add(6000));
    let section = &decoded[start..end];

    let threshold = parse_xai_numeric_field(section, "longContextThreshold")?;
    Some(XaiLongContextRate {
        threshold_tokens: threshold,
        input: parse_xai_price_field(section, "promptTextTokenPriceLongContext")?,
        output: parse_xai_price_field(section, "completionTokenPriceLongContext")?,
        cache_read: parse_xai_price_field(section, "cachedPromptTokenPriceLongContext")?,
    })
}

fn parse_xai_price_field(section: &str, field: &str) -> Option<u64> {
    parse_xai_numeric_field(section, field).map(|value| value.saturating_mul(100))
}

fn parse_xai_numeric_field(section: &str, field: &str) -> Option<u64> {
    let pattern = format!(r#""{}":"\$n(?P<value>\d+)""#, regex::escape(field));
    Regex::new(&pattern)
        .ok()?
        .captures(section)?
        .name("value")?
        .as_str()
        .parse()
        .ok()
}

fn fetch_anthropic_rates(
    client: &Client,
    rates: &mut BTreeMap<PricingKey, CachedModelRate>,
) -> Result<usize, String> {
    let text = fetch_text(client, ANTHROPIC_PRICING_URL)?;
    let parsed = parse_anthropic_rates(&text);
    let mut inserted = 0usize;
    for (model_id, rate) in parsed {
        for provider_id in ["anthropic", "bedrock", "vertex"] {
            set_rate(rates, provider_id, &model_id, ANTHROPIC_PRICING_URL, rate);
            inserted += 1;
        }
    }
    if inserted == 0 {
        return Err("Anthropic pricing docs did not contain model token rows".into());
    }
    Ok(inserted)
}

fn parse_anthropic_rates(markdown: &str) -> Vec<(String, ModelRate)> {
    markdown_table_rows(markdown)
        .filter_map(|cells| {
            if cells.len() < 6 || cells.first().is_some_and(|cell| cell.starts_with("Model")) {
                return None;
            }
            let model_id = anthropic_model_id_from_display_name(&cells[0])?;
            let input = parse_usd_per_1m_to_micros(&cells[1])?;
            let cache_write = parse_usd_per_1m_to_micros(&cells[2])?;
            let cache_read = parse_usd_per_1m_to_micros(&cells[4])?;
            let output = parse_usd_per_1m_to_micros(&cells[5])?;
            Some((
                model_id,
                ModelRate {
                    input_per_1m_micros: input,
                    output_per_1m_micros: output,
                    cache_read_per_1m_micros: Some(cache_read),
                    cache_write_per_1m_micros: Some(cache_write),
                    long_context_threshold_tokens: None,
                    long_context_input_per_1m_micros: None,
                    long_context_output_per_1m_micros: None,
                    long_context_cache_read_per_1m_micros: None,
                    long_context_cache_write_per_1m_micros: None,
                },
            ))
        })
        .collect()
}

fn anthropic_model_id_from_display_name(value: &str) -> Option<String> {
    let cleaned = strip_markdown_links(value)
        .replace("[deprecated]", "")
        .replace("deprecated", "");
    let mut parts = cleaned
        .split_whitespace()
        .map(|part| {
            part.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
                .to_ascii_lowercase()
        })
        .filter(|part| !part.is_empty() && part != "claude")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    for part in &mut parts {
        *part = part.replace('.', "-");
    }
    Some(format!("claude-{}", parts.join("-")))
}

fn fetch_openrouter_rates(
    client: &Client,
    rates: &mut BTreeMap<PricingKey, CachedModelRate>,
) -> Result<usize, String> {
    let response = client
        .get(OPENROUTER_MODELS_URL)
        .send()
        .map_err(|error| format!("{OPENROUTER_MODELS_URL} unreachable: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "{OPENROUTER_MODELS_URL} returned HTTP {}",
            status.as_u16()
        ));
    }
    let payload: OpenRouterModelsResponse = response
        .json()
        .map_err(|error| format!("OpenRouter models JSON was unreadable: {error}"))?;
    let mut inserted = 0usize;
    for model in payload.data {
        let Some(pricing) = model.pricing else {
            continue;
        };
        let Some(input) = pricing
            .prompt
            .as_deref()
            .and_then(parse_usd_per_token_to_micros_per_1m)
        else {
            continue;
        };
        let Some(output) = pricing
            .completion
            .as_deref()
            .and_then(parse_usd_per_token_to_micros_per_1m)
        else {
            continue;
        };
        let cache_read = pricing
            .input_cache_read
            .as_deref()
            .and_then(parse_usd_per_token_to_micros_per_1m);
        let cache_write = pricing
            .input_cache_write
            .as_deref()
            .and_then(parse_usd_per_token_to_micros_per_1m);
        set_rate(
            rates,
            "openrouter",
            &model.id,
            OPENROUTER_MODELS_URL,
            ModelRate {
                input_per_1m_micros: input,
                output_per_1m_micros: output,
                cache_read_per_1m_micros: cache_read,
                cache_write_per_1m_micros: cache_write,
                long_context_threshold_tokens: None,
                long_context_input_per_1m_micros: None,
                long_context_output_per_1m_micros: None,
                long_context_cache_read_per_1m_micros: None,
                long_context_cache_write_per_1m_micros: None,
            },
        );
        inserted += 1;
    }
    if inserted == 0 {
        return Err("OpenRouter models JSON did not contain token pricing".into());
    }
    Ok(inserted)
}

fn set_rate(
    rates: &mut BTreeMap<PricingKey, CachedModelRate>,
    provider_id: &str,
    model_id: &str,
    source_url: &str,
    rate: ModelRate,
) {
    let normalized_model_id = normalize_model_key_for_provider(provider_id, model_id);
    if normalized_model_id.is_empty() {
        return;
    }
    let key = PricingKey {
        provider_id: provider_id.into(),
        model_id: normalized_model_id,
    };
    rates.insert(
        key.clone(),
        CachedModelRate {
            provider_id: key.provider_id,
            model_id: key.model_id,
            source_url: source_url.into(),
            rate,
        },
    );
}

fn lookup_catalog_rate<'a>(
    catalog: &'a PricingCatalog,
    provider_id: &str,
    model_id: &str,
) -> Option<&'a ModelRate> {
    lookup_model_candidates(provider_id, model_id)
        .into_iter()
        .find_map(|candidate| {
            catalog.rates.get(&PricingKey {
                provider_id: provider_id.into(),
                model_id: candidate,
            })
        })
}

fn lookup_openrouter_fallback_rate<'a>(
    catalog: &'a PricingCatalog,
    provider_id: &str,
    model_id: &str,
) -> Option<&'a ModelRate> {
    for candidate in openrouter_fallback_model_candidates(provider_id, model_id) {
        if let Some(rate) = catalog.rates.get(&PricingKey {
            provider_id: "openrouter".into(),
            model_id: candidate,
        }) {
            return Some(rate);
        }
    }

    lookup_unique_openrouter_short_name_rate(catalog, model_id)
}

fn lookup_unique_openrouter_short_name_rate<'a>(
    catalog: &'a PricingCatalog,
    model_id: &str,
) -> Option<&'a ModelRate> {
    let target_variants = model_key_variants(&normalize_direct_model_key(model_id));
    let mut matched: Option<(&str, &ModelRate)> = None;

    for (key, rate) in catalog
        .rates
        .iter()
        .filter(|(key, _)| key.provider_id == "openrouter")
    {
        let candidate_variants = model_key_variants(&normalize_direct_model_key(&key.model_id));
        if !candidate_variants
            .iter()
            .any(|candidate| target_variants.iter().any(|target| target == candidate))
        {
            continue;
        }

        if let Some((existing_model_id, _)) = matched {
            if existing_model_id != key.model_id {
                return None;
            }
        } else {
            matched = Some((&key.model_id, rate));
        }
    }

    matched.map(|(_, rate)| rate)
}

fn openrouter_fallback_model_candidates(provider_id: &str, model_id: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let normalized_openrouter_id = normalize_openrouter_model_key(model_id);
    if normalized_openrouter_id.contains('/') {
        for variant in model_key_variants(&normalized_openrouter_id) {
            push_unique(&mut candidates, variant);
        }
    }

    let direct_model_id = normalize_direct_model_key(model_id);
    for prefix in openrouter_prefixes_for_provider(provider_id) {
        for variant in model_key_variants(&direct_model_id) {
            push_unique(&mut candidates, format!("{prefix}/{variant}"));
        }
    }

    candidates
}

fn openrouter_prefixes_for_provider(provider_id: &str) -> &'static [&'static str] {
    match provider_id {
        "openai_api" | "openai_codex" | "azure_openai" => &["openai"],
        "xai" => &["x-ai"],
        "anthropic" | "bedrock" | "vertex" => &["anthropic"],
        "gemini_ai_studio" => &["google"],
        "deepseek" => &["deepseek"],
        _ => &[],
    }
}

fn lookup_model_candidates(provider_id: &str, model_id: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let primary = normalize_model_key_for_provider(provider_id, model_id);
    for variant in model_key_variants(&primary) {
        push_unique(&mut candidates, variant);
    }
    if provider_id == "openrouter" {
        let stripped = normalize_direct_model_key(model_id);
        for variant in model_key_variants(&stripped) {
            push_unique(&mut candidates, variant);
        }
    }
    candidates
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn normalize_model_key_for_provider(provider_id: &str, model_id: &str) -> String {
    if provider_id == "openrouter" {
        normalize_openrouter_model_key(model_id)
    } else {
        normalize_direct_model_key(model_id)
    }
}

fn normalize_openrouter_model_key(model_id: &str) -> String {
    normalize_model_label(model_id.split('@').next().unwrap_or(model_id))
}

fn normalize_direct_model_key(model_id: &str) -> String {
    let without_version = model_id.split('@').next().unwrap_or(model_id);
    let without_provider = without_version
        .rsplit('/')
        .next()
        .unwrap_or(without_version);
    normalize_model_label(without_provider)
}

fn normalize_model_label(value: &str) -> String {
    value
        .split('(')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('`')
        .to_ascii_lowercase()
}

fn model_key_variants(model_id: &str) -> Vec<String> {
    let mut variants = Vec::new();
    push_unique(&mut variants, model_id.to_owned());
    if let Some(stripped_latest) = model_id.strip_suffix("-latest") {
        push_unique(&mut variants, stripped_latest.to_owned());
    }
    push_unique(&mut variants, model_id.replace('.', "-"));
    if let Some(numeric_dot_variant) = numeric_hyphen_to_dot_variant(model_id) {
        push_unique(&mut variants, numeric_dot_variant);
    }
    variants
}

fn numeric_hyphen_to_dot_variant(model_id: &str) -> Option<String> {
    let re = Regex::new(r"(?P<left>\d)-(?P<right>\d)").expect("valid numeric hyphen regex");
    let replaced = re.replace_all(model_id, "$left.$right").into_owned();
    (replaced != model_id).then_some(replaced)
}

#[derive(Debug, Clone, Copy)]
struct ActiveModelRate {
    input_per_1m_micros: u64,
    output_per_1m_micros: u64,
    cache_read_per_1m_micros: Option<u64>,
    cache_write_per_1m_micros: Option<u64>,
}

fn active_rate_for_usage(rate: &ModelRate, usage: UsageForPricing) -> Option<ActiveModelRate> {
    let prompt_tokens = usage
        .input_tokens
        .saturating_add(usage.cache_read_tokens)
        .saturating_add(usage.cache_creation_tokens);
    if rate
        .long_context_threshold_tokens
        .is_some_and(|threshold| prompt_tokens > threshold)
    {
        return Some(ActiveModelRate {
            input_per_1m_micros: rate.long_context_input_per_1m_micros?,
            output_per_1m_micros: rate.long_context_output_per_1m_micros?,
            cache_read_per_1m_micros: rate.long_context_cache_read_per_1m_micros,
            cache_write_per_1m_micros: rate.long_context_cache_write_per_1m_micros,
        });
    }

    Some(ActiveModelRate {
        input_per_1m_micros: rate.input_per_1m_micros,
        output_per_1m_micros: rate.output_per_1m_micros,
        cache_read_per_1m_micros: rate.cache_read_per_1m_micros,
        cache_write_per_1m_micros: rate.cache_write_per_1m_micros,
    })
}

fn cost_for_usage_at_rate(rate: &ModelRate, usage: UsageForPricing) -> Option<u64> {
    let active = active_rate_for_usage(rate, usage)?;
    Some(
        cost_for_bucket(usage.input_tokens, active.input_per_1m_micros)
            .saturating_add(cost_for_bucket(
                usage.output_tokens,
                active.output_per_1m_micros,
            ))
            .saturating_add(cost_for_optional_bucket(
                usage.cache_read_tokens,
                active.cache_read_per_1m_micros,
            )?)
            .saturating_add(cost_for_optional_bucket(
                usage.cache_creation_tokens,
                active.cache_write_per_1m_micros,
            )?),
    )
}

fn cost_for_optional_bucket(tokens: u64, rate_per_1m_micros: Option<u64>) -> Option<u64> {
    if tokens == 0 {
        Some(0)
    } else {
        rate_per_1m_micros.map(|rate| cost_for_bucket(tokens, rate))
    }
}

/// `tokens * rate_per_1m / 1_000_000` with overflow-safe arithmetic.
fn cost_for_bucket(tokens: u64, rate_per_1m_micros: u64) -> u64 {
    if tokens == 0 || rate_per_1m_micros == 0 {
        return 0;
    }
    let product = (tokens as u128).saturating_mul(rate_per_1m_micros as u128);
    let micros = product / 1_000_000u128;
    u64::try_from(micros).unwrap_or(u64::MAX)
}

fn markdown_table_rows(markdown: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    markdown.lines().filter_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            return None;
        }
        let cells = trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_owned())
            .collect::<Vec<_>>();
        if cells
            .iter()
            .all(|cell| cell.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
        {
            return None;
        }
        Some(cells)
    })
}

fn strip_markdown_links(value: &str) -> String {
    Regex::new(r#"\[([^\]]+)\]\([^)]+\)"#)
        .expect("valid markdown link regex")
        .replace_all(value, "$1")
        .into_owned()
}

fn parse_js_usd_per_1m(value: &str) -> Option<u64> {
    let trimmed = value.trim().trim_matches('"').trim();
    if matches!(trimmed, "" | "-" | "null") {
        return None;
    }
    parse_usd_per_1m_to_micros(trimmed)
}

fn parse_usd_per_1m_to_micros(value: &str) -> Option<u64> {
    parse_decimal_scaled(&extract_decimal(value)?, 1_000_000)
}

fn parse_usd_per_token_to_micros_per_1m(value: &str) -> Option<u64> {
    parse_decimal_scaled(&extract_decimal(value)?, 1_000_000_000_000)
}

fn extract_decimal(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut started = false;
    let mut seen_dot = false;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            started = true;
            out.push(ch);
        } else if ch == '.' && started && !seen_dot {
            seen_dot = true;
            out.push(ch);
        } else if started {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn parse_decimal_scaled(value: &str, scale: u128) -> Option<u64> {
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return None;
    }
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let whole_value = if whole.is_empty() {
        0u128
    } else {
        whole.parse::<u128>().ok()?
    };
    let fraction_value = if fraction.is_empty() {
        0u128
    } else {
        fraction.parse::<u128>().ok()?
    };
    let denominator = 10u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let numerator = whole_value
        .checked_mul(denominator)?
        .checked_add(fraction_value)?;
    let scaled = numerator.checked_mul(scale)?.checked_add(denominator / 2)? / denominator;
    u64::try_from(scaled).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_with(provider_id: &str, model_id: &str, rate: ModelRate) -> PricingCatalog {
        let mut rates = BTreeMap::new();
        rates.insert(
            PricingKey {
                provider_id: provider_id.into(),
                model_id: normalize_model_key_for_provider(provider_id, model_id),
            },
            rate,
        );
        PricingCatalog { rates }
    }

    fn rate(input: u64, cache_read: u64, output: u64) -> ModelRate {
        ModelRate {
            input_per_1m_micros: input,
            output_per_1m_micros: output,
            cache_read_per_1m_micros: Some(cache_read),
            cache_write_per_1m_micros: Some(input),
            long_context_threshold_tokens: None,
            long_context_input_per_1m_micros: None,
            long_context_output_per_1m_micros: None,
            long_context_cache_read_per_1m_micros: None,
            long_context_cache_write_per_1m_micros: None,
        }
    }

    #[test]
    fn openai_pricing_rows_parse_standard_tier_and_codex_model_names() {
        let rows = parse_openai_standard_rows(
            r#"
            <TextTokenPricingTables
              client:load
              tier="standard"
              rows={[
                ["gpt-5.5 (<272K context length)", 5, 0.5, 30],
                ["gpt-5.4-mini", 0.75, 0.075, 4.5],
              ]}
            />
            "#,
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "gpt-5.5");
        assert_eq!(rows[0].1.input_per_1m_micros, 5_000_000);
        assert_eq!(rows[0].1.cache_read_per_1m_micros, Some(500_000));
        assert_eq!(rows[0].1.cache_write_per_1m_micros, None);
        assert_eq!(rows[0].1.output_per_1m_micros, 30_000_000);
    }

    #[test]
    fn xai_model_detail_parses_cached_input_and_aliases() {
        let markdown = r#"
        # Grok 4.3
        - **Aliases:** `grok-4.3-latest`, `grok-latest`

        ## Pricing

        | Type | Price per 1M tokens |
        | --- | --- |
        | Input | $1.25 |
        | Cached input | $0.20 |
        | Output | $2.50 |
        "#;

        let parsed = parse_xai_model_detail_rate(markdown).expect("xai rate");
        assert_eq!(parsed.input_per_1m_micros, 1_250_000);
        assert_eq!(parsed.cache_read_per_1m_micros, Some(200_000));
        assert_eq!(parsed.cache_write_per_1m_micros, None);
        assert_eq!(parsed.output_per_1m_micros, 2_500_000);
        assert_eq!(
            parse_xai_model_aliases(markdown),
            vec!["grok-4.3-latest", "grok-latest"]
        );
    }

    #[test]
    fn xai_embedded_model_data_parses_long_context_prices() {
        let html = r#"
        {\"name\":\"grok-4.3\",\"promptTextTokenPrice\":\"$n12500\",\"promptTextTokenPriceLongContext\":\"$n25000\",\"cachedPromptTokenPrice\":\"$n2000\",\"cachedPromptTokenPriceLongContext\":\"$n4000\",\"completionTextTokenPrice\":\"$n25000\",\"completionTokenPriceLongContext\":\"$n50000\",\"longContextThreshold\":\"$n200000\"}
        "#;

        let parsed = parse_xai_long_context_rate(html, "grok-4.3").expect("long context");
        assert_eq!(parsed.threshold_tokens, 200_000);
        assert_eq!(parsed.input, 2_500_000);
        assert_eq!(parsed.cache_read, 400_000);
        assert_eq!(parsed.output, 5_000_000);
    }

    #[test]
    fn anthropic_pricing_rows_parse_cache_write_and_read() {
        let rows = parse_anthropic_rates(
            r#"
            | Model | Base Input Tokens | 5m Cache Writes | 1h Cache Writes | Cache Hits & Refreshes | Output Tokens |
            | --- | --- | --- | --- | --- | --- |
            | Claude Sonnet 4.6 | $3 / MTok | $3.75 / MTok | $6 / MTok | $0.30 / MTok | $15 / MTok |
            "#,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "claude-sonnet-4-6");
        assert_eq!(rows[0].1.input_per_1m_micros, 3_000_000);
        assert_eq!(rows[0].1.cache_write_per_1m_micros, Some(3_750_000));
        assert_eq!(rows[0].1.cache_read_per_1m_micros, Some(300_000));
        assert_eq!(rows[0].1.output_per_1m_micros, 15_000_000);
    }

    #[test]
    fn openrouter_per_token_decimal_converts_to_per_million_micros() {
        assert_eq!(
            parse_usd_per_token_to_micros_per_1m("0.0000003"),
            Some(300_000)
        );
        assert_eq!(
            parse_usd_per_token_to_micros_per_1m("0.0000012"),
            Some(1_200_000)
        );
    }

    #[test]
    fn estimate_uses_long_context_rate_when_prompt_crosses_threshold() {
        let mut long_rate = rate(1_250_000, 200_000, 2_500_000);
        long_rate.long_context_threshold_tokens = Some(200_000);
        long_rate.long_context_input_per_1m_micros = Some(2_500_000);
        long_rate.long_context_cache_read_per_1m_micros = Some(400_000);
        long_rate.long_context_cache_write_per_1m_micros = Some(2_500_000);
        long_rate.long_context_output_per_1m_micros = Some(5_000_000);
        let catalog = catalog_with("xai", "grok-4.3", long_rate);

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "xai",
            "grok-4.3-latest",
            UsageForPricing {
                input_tokens: 200_001,
                output_tokens: 10_000,
                cache_read_tokens: 100_000,
                cache_creation_tokens: 0,
            },
        );

        assert_eq!(cost, 590_002);
    }

    #[test]
    fn vendor_prefix_and_latest_aliases_resolve_for_direct_providers() {
        let catalog = catalog_with(
            "openai_codex",
            "gpt-5.5",
            rate(5_000_000, 500_000, 30_000_000),
        );
        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "openai_codex",
            "openai/gpt-5.5-latest",
            UsageForPricing {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 5_000_000);
    }

    #[test]
    fn openrouter_keeps_vendor_prefix_for_lookup() {
        let catalog = catalog_with(
            "openrouter",
            "anthropic/claude-sonnet-4.6",
            rate(3_000_000, 300_000, 15_000_000),
        );
        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "openrouter",
            "anthropic/claude-sonnet-4.6",
            UsageForPricing {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 3_000_000);
    }

    #[test]
    fn openrouter_short_model_id_uses_unique_catalog_match() {
        let catalog = catalog_with(
            "openrouter",
            "x-ai/grok-4.3",
            rate(1_250_000, 200_000, 2_500_000),
        );

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "openrouter",
            "grok-4.3",
            UsageForPricing {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        );

        assert_eq!(cost, 1_250_000);
    }

    #[test]
    fn direct_provider_falls_back_to_openrouter_when_direct_rate_missing() {
        let catalog = catalog_with(
            "openrouter",
            "x-ai/grok-4.3",
            rate(1_250_000, 200_000, 2_500_000),
        );

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "xai",
            "grok-4.3",
            UsageForPricing {
                input_tokens: 1_000_000,
                output_tokens: 100_000,
                ..Default::default()
            },
        );

        assert_eq!(cost, 1_500_000);
    }

    #[test]
    fn direct_provider_falls_back_to_openrouter_for_unpublished_used_bucket() {
        let mut rates = BTreeMap::new();
        rates.insert(
            PricingKey {
                provider_id: "xai".into(),
                model_id: "grok-4.3".into(),
            },
            ModelRate {
                input_per_1m_micros: 1_250_000,
                output_per_1m_micros: 2_500_000,
                cache_read_per_1m_micros: Some(200_000),
                cache_write_per_1m_micros: None,
                long_context_threshold_tokens: None,
                long_context_input_per_1m_micros: None,
                long_context_output_per_1m_micros: None,
                long_context_cache_read_per_1m_micros: None,
                long_context_cache_write_per_1m_micros: None,
            },
        );
        rates.insert(
            PricingKey {
                provider_id: "openrouter".into(),
                model_id: "x-ai/grok-4.3".into(),
            },
            ModelRate {
                input_per_1m_micros: 1_250_000,
                output_per_1m_micros: 2_500_000,
                cache_read_per_1m_micros: Some(200_000),
                cache_write_per_1m_micros: Some(1_100_000),
                long_context_threshold_tokens: None,
                long_context_input_per_1m_micros: None,
                long_context_output_per_1m_micros: None,
                long_context_cache_read_per_1m_micros: None,
                long_context_cache_write_per_1m_micros: None,
            },
        );
        let catalog = PricingCatalog { rates };

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "xai",
            "grok-4.3",
            UsageForPricing {
                cache_creation_tokens: 1_000_000,
                ..Default::default()
            },
        );

        assert_eq!(cost, 1_100_000);
    }

    #[test]
    fn unpublished_used_bucket_without_openrouter_rate_prices_zero() {
        let mut direct_rate = rate(5_000_000, 500_000, 30_000_000);
        direct_rate.cache_write_per_1m_micros = None;
        let catalog = catalog_with("openai_api", "gpt-5.5", direct_rate);

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "openai_api",
            "gpt-5.5",
            UsageForPricing {
                cache_creation_tokens: 1_000_000,
                ..Default::default()
            },
        );

        assert_eq!(cost, 0);
    }

    #[test]
    fn openrouter_fallback_matches_provider_prefix_and_decimal_variant() {
        let catalog = catalog_with(
            "openrouter",
            "anthropic/claude-sonnet-4.6",
            rate(3_000_000, 300_000, 15_000_000),
        );

        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "anthropic",
            "claude-sonnet-4-6",
            UsageForPricing {
                output_tokens: 100_000,
                ..Default::default()
            },
        );

        assert_eq!(cost, 1_500_000);
    }

    #[test]
    fn billion_token_bucket_does_not_overflow() {
        let catalog = catalog_with(
            "openai_api",
            "gpt-5.5",
            rate(5_000_000, 500_000, 30_000_000),
        );
        let cost = estimate_cost_micros_from_catalog(
            &catalog,
            "openai_api",
            "gpt-5.5",
            UsageForPricing {
                input_tokens: 1_000_000_000,
                output_tokens: 1_000_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 35_000_000_000);
    }
}
