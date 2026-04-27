//! Static per-model pricing for token usage cost estimation.
//!
//! Costs are stored in micros (1 micro = $0.000001) so we can keep everything
//! as `u64` and add up across many runs without floating-point drift.

/// Per-million-token prices in **micros**.
///
/// Why per-million: provider docs publish prices per 1M tokens, so storing the
/// rate in that unit means the table can be edited by copy-pasting from a
/// changelog without scaling. `cost_micros = tokens * rate_per_1m / 1_000_000`.
#[derive(Debug, Clone, Copy)]
pub struct ModelRate {
    pub input_per_1m_micros: u64,
    pub output_per_1m_micros: u64,
    pub cache_read_per_1m_micros: u64,
    pub cache_write_per_1m_micros: u64,
}

/// Token usage we price against. Mirrors `ProviderUsage` but kept independent
/// so the pricing module has no dependency on the agent runtime types.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsageForPricing {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

/// Pricing lookup table.
///
/// Entries are `(provider_id, model_id, rate)`. `provider_id` matches the
/// constants in `runtime::provider`. The model id is matched case-insensitively
/// after stripping any `provider/` prefix that some clients send.
///
/// Rates as of 2026-04-27 from public provider pricing pages. Update alongside
/// model launches; unknown models silently price at 0.
const MODEL_RATES: &[(&str, &str, ModelRate)] = &[
    // ---------------- Anthropic ----------------
    // Claude Opus 4.x
    (
        "anthropic",
        "claude-opus-4-7",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 25_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 6_250_000,
        },
    ),
    (
        "anthropic",
        "claude-opus-4-6",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 25_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 6_250_000,
        },
    ),
    (
        "anthropic",
        "claude-opus-4",
        ModelRate {
            input_per_1m_micros: 15_000_000,
            output_per_1m_micros: 75_000_000,
            cache_read_per_1m_micros: 1_500_000,
            cache_write_per_1m_micros: 18_750_000,
        },
    ),
    // Claude Sonnet 4.x
    (
        "anthropic",
        "claude-sonnet-4-6",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    (
        "anthropic",
        "claude-sonnet-4-5",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    (
        "anthropic",
        "claude-sonnet-4",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    // Claude Haiku 4.x
    (
        "anthropic",
        "claude-haiku-4-5",
        ModelRate {
            input_per_1m_micros: 800_000,
            output_per_1m_micros: 4_000_000,
            cache_read_per_1m_micros: 80_000,
            cache_write_per_1m_micros: 1_000_000,
        },
    ),
    // Latest aliases
    (
        "anthropic",
        "claude-3-5-sonnet-latest",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    (
        "anthropic",
        "claude-3-5-haiku-latest",
        ModelRate {
            input_per_1m_micros: 800_000,
            output_per_1m_micros: 4_000_000,
            cache_read_per_1m_micros: 80_000,
            cache_write_per_1m_micros: 1_000_000,
        },
    ),
    (
        "anthropic",
        "claude-3-opus-latest",
        ModelRate {
            input_per_1m_micros: 15_000_000,
            output_per_1m_micros: 75_000_000,
            cache_read_per_1m_micros: 1_500_000,
            cache_write_per_1m_micros: 18_750_000,
        },
    ),
    // ---------------- OpenAI (direct API) ----------------
    (
        "openai_api",
        "gpt-4o",
        ModelRate {
            input_per_1m_micros: 2_500_000,
            output_per_1m_micros: 10_000_000,
            cache_read_per_1m_micros: 1_250_000,
            cache_write_per_1m_micros: 2_500_000,
        },
    ),
    (
        "openai_api",
        "gpt-4o-mini",
        ModelRate {
            input_per_1m_micros: 150_000,
            output_per_1m_micros: 600_000,
            cache_read_per_1m_micros: 75_000,
            cache_write_per_1m_micros: 150_000,
        },
    ),
    (
        "openai_api",
        "gpt-4.1",
        ModelRate {
            input_per_1m_micros: 2_000_000,
            output_per_1m_micros: 8_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 2_000_000,
        },
    ),
    (
        "openai_api",
        "gpt-5",
        ModelRate {
            input_per_1m_micros: 10_000_000,
            output_per_1m_micros: 40_000_000,
            cache_read_per_1m_micros: 2_500_000,
            cache_write_per_1m_micros: 10_000_000,
        },
    ),
    (
        "openai_api",
        "o1",
        ModelRate {
            input_per_1m_micros: 15_000_000,
            output_per_1m_micros: 60_000_000,
            cache_read_per_1m_micros: 7_500_000,
            cache_write_per_1m_micros: 15_000_000,
        },
    ),
    (
        "openai_api",
        "o3",
        ModelRate {
            input_per_1m_micros: 15_000_000,
            output_per_1m_micros: 60_000_000,
            cache_read_per_1m_micros: 7_500_000,
            cache_write_per_1m_micros: 15_000_000,
        },
    ),
    // ---------------- OpenAI Codex ----------------
    // Code-specialized GPT-5.x family used by the Codex provider.
    (
        "openai_codex",
        "gpt-5.1",
        ModelRate {
            input_per_1m_micros: 300_000,
            output_per_1m_micros: 1_200_000,
            cache_read_per_1m_micros: 75_000,
            cache_write_per_1m_micros: 300_000,
        },
    ),
    (
        "openai_codex",
        "gpt-5.2",
        ModelRate {
            input_per_1m_micros: 1_000_000,
            output_per_1m_micros: 4_000_000,
            cache_read_per_1m_micros: 250_000,
            cache_write_per_1m_micros: 1_000_000,
        },
    ),
    (
        "openai_codex",
        "gpt-5.3",
        ModelRate {
            input_per_1m_micros: 2_500_000,
            output_per_1m_micros: 10_000_000,
            cache_read_per_1m_micros: 625_000,
            cache_write_per_1m_micros: 2_500_000,
        },
    ),
    (
        "openai_codex",
        "gpt-5.4",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 20_000_000,
            cache_read_per_1m_micros: 1_250_000,
            cache_write_per_1m_micros: 5_000_000,
        },
    ),
    // ---------------- Azure OpenAI ----------------
    // Azure mirrors OpenAI direct-API list price for the same SKU.
    (
        "azure_openai",
        "gpt-4o",
        ModelRate {
            input_per_1m_micros: 2_500_000,
            output_per_1m_micros: 10_000_000,
            cache_read_per_1m_micros: 1_250_000,
            cache_write_per_1m_micros: 2_500_000,
        },
    ),
    (
        "azure_openai",
        "gpt-4o-mini",
        ModelRate {
            input_per_1m_micros: 150_000,
            output_per_1m_micros: 600_000,
            cache_read_per_1m_micros: 75_000,
            cache_write_per_1m_micros: 150_000,
        },
    ),
    (
        "azure_openai",
        "gpt-5",
        ModelRate {
            input_per_1m_micros: 10_000_000,
            output_per_1m_micros: 40_000_000,
            cache_read_per_1m_micros: 2_500_000,
            cache_write_per_1m_micros: 10_000_000,
        },
    ),
    // ---------------- Google Gemini (AI Studio) ----------------
    (
        "gemini_ai_studio",
        "gemini-2.0-flash",
        ModelRate {
            input_per_1m_micros: 100_000,
            output_per_1m_micros: 400_000,
            cache_read_per_1m_micros: 25_000,
            cache_write_per_1m_micros: 100_000,
        },
    ),
    (
        "gemini_ai_studio",
        "gemini-2.5-pro",
        ModelRate {
            input_per_1m_micros: 1_250_000,
            output_per_1m_micros: 5_000_000,
            cache_read_per_1m_micros: 312_500,
            cache_write_per_1m_micros: 1_250_000,
        },
    ),
    (
        "gemini_ai_studio",
        "gemini-2.5-flash",
        ModelRate {
            input_per_1m_micros: 300_000,
            output_per_1m_micros: 1_200_000,
            cache_read_per_1m_micros: 75_000,
            cache_write_per_1m_micros: 300_000,
        },
    ),
    // ---------------- Google Vertex (same SKUs, same prices) ----------------
    (
        "vertex",
        "gemini-2.0-flash",
        ModelRate {
            input_per_1m_micros: 100_000,
            output_per_1m_micros: 400_000,
            cache_read_per_1m_micros: 25_000,
            cache_write_per_1m_micros: 100_000,
        },
    ),
    (
        "vertex",
        "gemini-2.5-pro",
        ModelRate {
            input_per_1m_micros: 1_250_000,
            output_per_1m_micros: 5_000_000,
            cache_read_per_1m_micros: 312_500,
            cache_write_per_1m_micros: 1_250_000,
        },
    ),
    (
        "vertex",
        "claude-opus-4-7",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 25_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 6_250_000,
        },
    ),
    (
        "vertex",
        "claude-sonnet-4-6",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    // ---------------- AWS Bedrock ----------------
    (
        "bedrock",
        "claude-opus-4-7",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 25_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 6_250_000,
        },
    ),
    (
        "bedrock",
        "claude-sonnet-4-6",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    (
        "bedrock",
        "claude-haiku-4-5",
        ModelRate {
            input_per_1m_micros: 800_000,
            output_per_1m_micros: 4_000_000,
            cache_read_per_1m_micros: 80_000,
            cache_write_per_1m_micros: 1_000_000,
        },
    ),
    // ---------------- OpenRouter ----------------
    // OpenRouter prefixes ids with vendor; lookup_rate strips it.
    (
        "openrouter",
        "claude-opus-4-7",
        ModelRate {
            input_per_1m_micros: 5_000_000,
            output_per_1m_micros: 25_000_000,
            cache_read_per_1m_micros: 500_000,
            cache_write_per_1m_micros: 6_250_000,
        },
    ),
    (
        "openrouter",
        "claude-sonnet-4-6",
        ModelRate {
            input_per_1m_micros: 3_000_000,
            output_per_1m_micros: 15_000_000,
            cache_read_per_1m_micros: 300_000,
            cache_write_per_1m_micros: 3_750_000,
        },
    ),
    (
        "openrouter",
        "gpt-4o",
        ModelRate {
            input_per_1m_micros: 2_500_000,
            output_per_1m_micros: 10_000_000,
            cache_read_per_1m_micros: 1_250_000,
            cache_write_per_1m_micros: 2_500_000,
        },
    ),
    (
        "openrouter",
        "gpt-5",
        ModelRate {
            input_per_1m_micros: 10_000_000,
            output_per_1m_micros: 40_000_000,
            cache_read_per_1m_micros: 2_500_000,
            cache_write_per_1m_micros: 10_000_000,
        },
    ),
    (
        "openrouter",
        "deepseek-chat",
        ModelRate {
            input_per_1m_micros: 140_000,
            output_per_1m_micros: 280_000,
            cache_read_per_1m_micros: 35_000,
            cache_write_per_1m_micros: 140_000,
        },
    ),
    // ---------------- GitHub Models ----------------
    // GitHub Models is a free preview surface that bills via GitHub allotment;
    // keep entries at zero so we don't double-bill users.
    (
        "github_models",
        "gpt-4o",
        ModelRate {
            input_per_1m_micros: 0,
            output_per_1m_micros: 0,
            cache_read_per_1m_micros: 0,
            cache_write_per_1m_micros: 0,
        },
    ),
    (
        "github_models",
        "gpt-4o-mini",
        ModelRate {
            input_per_1m_micros: 0,
            output_per_1m_micros: 0,
            cache_read_per_1m_micros: 0,
            cache_write_per_1m_micros: 0,
        },
    ),
    // ---------------- Ollama ----------------
    // Local inference: free at the wallet, but we still record token totals.
    // Single zero-rate entry covers everything via the wildcard match below.
];

/// Strip provider prefixes like `"openai/gpt-4o"` -> `"gpt-4o"` and trim
/// optional version pin suffixes after `@`.
fn normalize_model_id(model_id: &str) -> &str {
    let after_slash = model_id.rsplit('/').next().unwrap_or(model_id);
    after_slash.split('@').next().unwrap_or(after_slash)
}

/// Look up the rate for a given provider+model. Case-insensitive on model id.
pub fn lookup_rate(provider_id: &str, model_id: &str) -> Option<ModelRate> {
    let needle = normalize_model_id(model_id);
    MODEL_RATES.iter().find_map(|(p, m, rate)| {
        if *p == provider_id && m.eq_ignore_ascii_case(needle) {
            Some(*rate)
        } else {
            None
        }
    })
}

/// Compute estimated cost in micros for a given usage record.
/// Returns 0 for unknown models so logging stays best-effort.
pub fn estimate_cost_micros(provider_id: &str, model_id: &str, usage: UsageForPricing) -> u64 {
    // Ollama is local: always free regardless of model.
    if provider_id == "ollama" {
        return 0;
    }

    let Some(rate) = lookup_rate(provider_id, model_id) else {
        return 0;
    };

    cost_for_bucket(usage.input_tokens, rate.input_per_1m_micros)
        .saturating_add(cost_for_bucket(
            usage.output_tokens,
            rate.output_per_1m_micros,
        ))
        .saturating_add(cost_for_bucket(
            usage.cache_read_tokens,
            rate.cache_read_per_1m_micros,
        ))
        .saturating_add(cost_for_bucket(
            usage.cache_creation_tokens,
            rate.cache_write_per_1m_micros,
        ))
}

/// One-time repair: recompute and persist `estimated_cost_micros` for any
/// usage rows that were written before Phase 3 wired pricing into the persist
/// path. Best-effort — errors are swallowed (logged via eprintln) so a bad
/// project DB doesn't block app boot. Returns the number of rows updated.
pub fn backfill_agent_usage_costs(repo_root: &std::path::Path) -> usize {
    use crate::db::project_store::{
        list_unpriced_agent_usage_rows, update_agent_usage_cost, AgentUsageCostBackfillRow,
    };

    let rows = match list_unpriced_agent_usage_rows(repo_root) {
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
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
    } in rows
    {
        let cost = estimate_cost_micros(
            &provider_id,
            &model_id,
            UsageForPricing {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            },
        );
        if cost == 0 {
            // Either ollama or an unknown model — leave the row alone so the
            // next backfill (after a pricing-table update) can pick it up.
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

/// `tokens * rate_per_1m / 1_000_000` with overflow-safe arithmetic. Uses u128
/// internally so a single 1B-token bucket at $1k/1M can't overflow.
fn cost_for_bucket(tokens: u64, rate_per_1m_micros: u64) -> u64 {
    if tokens == 0 || rate_per_1m_micros == 0 {
        return 0;
    }
    let product = (tokens as u128).saturating_mul(rate_per_1m_micros as u128);
    let micros = product / 1_000_000u128;
    u64::try_from(micros).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_anthropic_model_prices_input_and_output() {
        // claude-sonnet-4-6: $3/1M input, $15/1M output
        let cost = estimate_cost_micros(
            "anthropic",
            "claude-sonnet-4-6",
            UsageForPricing {
                input_tokens: 1_000_000,
                output_tokens: 100_000,
                ..Default::default()
            },
        );
        // 1M input @ $3/1M = $3 = 3_000_000 micros
        // 100k output @ $15/1M = $1.50 = 1_500_000 micros
        assert_eq!(cost, 4_500_000);
    }

    #[test]
    fn cache_read_priced_at_discount() {
        // claude-sonnet-4-6 cache-read is $0.30/1M (10% of input)
        let cost = estimate_cost_micros(
            "anthropic",
            "claude-sonnet-4-6",
            UsageForPricing {
                cache_read_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 300_000); // $0.30
    }

    #[test]
    fn cache_creation_priced_at_premium() {
        // claude-sonnet-4-6 cache-write is $3.75/1M (125% of input)
        let cost = estimate_cost_micros(
            "anthropic",
            "claude-sonnet-4-6",
            UsageForPricing {
                cache_creation_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 3_750_000);
    }

    #[test]
    fn unknown_model_prices_zero() {
        let cost = estimate_cost_micros(
            "anthropic",
            "claude-future-model-9000",
            UsageForPricing {
                input_tokens: 1_000_000,
                output_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 0);
    }

    #[test]
    fn ollama_always_free_even_for_known_model_names() {
        let cost = estimate_cost_micros(
            "ollama",
            "gpt-4o",
            UsageForPricing {
                input_tokens: 999_999_999,
                output_tokens: 999_999_999,
                ..Default::default()
            },
        );
        assert_eq!(cost, 0);
    }

    #[test]
    fn vendor_prefix_stripped_for_lookup() {
        let cost = estimate_cost_micros(
            "openrouter",
            "anthropic/claude-sonnet-4-6",
            UsageForPricing {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 3_000_000);
    }

    #[test]
    fn case_insensitive_model_match() {
        let cost = estimate_cost_micros(
            "anthropic",
            "Claude-Sonnet-4-6",
            UsageForPricing {
                input_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 3_000_000);
    }

    #[test]
    fn billion_token_bucket_does_not_overflow() {
        let cost = estimate_cost_micros(
            "openai_api",
            "gpt-5",
            UsageForPricing {
                input_tokens: 1_000_000_000,
                output_tokens: 1_000_000_000,
                ..Default::default()
            },
        );
        // 1B input @ $10/1M = $10_000 = 10_000_000_000 micros
        // 1B output @ $40/1M = $40_000 = 40_000_000_000 micros
        assert_eq!(cost, 50_000_000_000);
    }

    #[test]
    fn empty_usage_returns_zero() {
        let cost =
            estimate_cost_micros("anthropic", "claude-sonnet-4-6", UsageForPricing::default());
        assert_eq!(cost, 0);
    }

    #[test]
    fn github_models_priced_at_zero_even_for_real_models() {
        let cost = estimate_cost_micros(
            "github_models",
            "gpt-4o",
            UsageForPricing {
                input_tokens: 1_000_000,
                output_tokens: 1_000_000,
                ..Default::default()
            },
        );
        assert_eq!(cost, 0);
    }
}
