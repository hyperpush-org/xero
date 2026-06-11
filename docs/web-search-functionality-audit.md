# Web Search Functionality Audit

Issue: <https://github.com/hyperpush-org/xero/issues/37>

Date: 2026-05-31

## Status

Implemented.

`web_fetch` remains an independent direct HTTP/HTTPS text fetch and works without any search-provider configuration. `web_search` is now configured from OS app-data-backed Settings, not process environment variables. The default mode is `Auto`: provider-managed LLM search is attempted first when the active provider/model can support it, then Xero falls back to the user's active configured web-search provider profile. Users can also force `Provider-managed only`, force `Configured provider only`, or disable `web_search`.

Configured provider API keys are stored through the global `provider_credentials` path under web-search-scoped credential ids and are filtered out of the normal LLM provider credential UI. Settings rows expose only readiness metadata such as `hasApiKey`, `apiKeyUpdatedAt`, and last-check status.

## Current Surfaces

Runtime tools live in `client/src-tauri/src/runtime/autonomous_web_runtime/`:

- `mod.rs` defines `web_search`, `web_fetch`, request/output DTOs, limits, Settings-backed configured-provider config, provider-managed config, and search mode.
- `managed.rs` constructs bounded provider-managed search requests for Anthropic, OpenAI Responses, Gemini grounding, xAI Responses, and OpenRouter server web search, then normalizes cited URLs into the same agent-visible result shape.
- `search.rs` validates queries, applies source precedence, calls configured provider adapters, normalizes common provider JSON result shapes, rejects invalid or oversized payloads, normalizes HTTP/HTTPS result URLs, decodes HTML entities, and caps result counts/snippets.
- `fetch.rs` validates absolute HTTP/HTTPS URLs, fetches text/html, application/xhtml+xml, or text/plain, extracts readable HTML text/title, and enforces character/byte limits.
- `transport.rs` uses blocking reqwest with timeouts, redirect limits, GET/POST support, auth headers, redacted transport errors, and response-size caps.

Agent exposure is wired through these paths:

- Tool descriptors: `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs` exposes `web_search` and `web_fetch` schemas.
- Tool discovery/catalog: `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs` exposes web catalog entries and the `web_search_only`, `web_fetch`, and `web` tool-access groups.
- Planner activation: prompts mentioning docs, documentation, internet, latest, current, web search, or web fetch activate search/fetch without browser-control tools.
- Dispatch: `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs` maps `AutonomousToolRequest::WebSearch` and `AutonomousToolRequest::WebFetch` to runtime execution.
- Model-visible results: `client/src-tauri/src/runtime/agent_core/provider_loop.rs` compacts web output and marks web content as untrusted lower-priority data.
- Stream summaries: `client/src-tauri/src/commands/subscribe_runtime_stream.rs` maps web calls into `ToolResultSummaryDto::Web`.

Permissions and policy:

- `web_search` and `web_fetch` are `external_service` tools with network risk metadata.
- Computer Use can use external-service tools. Plan and Crawl policies do not expose them.
- Custom agents need an effective policy/base capability that allows `external_service`.
- Project capability revocation can block external integrations via `external_integration:external_service` or the exact tool name.
- Stage gates still apply at runtime through the normal tool enforcement path.

UI and operator affordances:

- Tool categories include a `Web` category for agent authoring and runtime presentation.
- Tool call summaries render as web summaries in transcript/runtime streams.
- Settings includes a first-party `Web Search` section that lets users choose mode, inspect provider-managed readiness, add/edit/delete/enable/test/select configured providers, and save provider API keys without returning secrets after save.
- Tauri commands are exposed through the desktop adapter for loading/updating settings, upserting/deleting/selecting configured providers, and checking providers through the same runtime adapter path agents use.
- Doctor diagnostics report mode/source readiness, provider-managed status, active configured-provider readiness, and last-check status without exposing secrets.
- The README documents Settings-only configuration and the custom endpoint contract. Environment variables no longer configure or override web search.

## Verified Behavior

Focused Rust tests cover:

- Settings defaults, validation, credential-backed readiness, Google CSE `cx` requirements, and custom endpoint requirements.
- Search calls configured providers, normalizes returned titles/snippets/URLs, marks provider-overflow results as truncated, and includes source labels.
- Search without a configured source returns a clear user-fixable unavailable error.
- All planned configured provider kinds construct requests successfully against mock transport: `custom_endpoint`, `brave_search`, `tavily_search`, `exa_search`, `firecrawl_search`, `you_search`, `linkup_search`, `kagi_search`, `searxng_json`, `serpapi_google`, `searchapi_google`, and `google_cse`.
- `Auto` falls back from a retryably failed provider-managed request to the configured provider.
- Timeout, retryable status, malformed provider payload, oversized payload, and unsupported fetch payload paths fail closed.
- Fetch works without a search provider, extracts HTML title/text, normalizes content type, and uses the direct HTTP transport.
- Tool-search catalog fields now match the real schemas: `resultCount` and `maxChars`, not stale `limit` and `maxBytes` names.
- Frontend TypeScript, lint, and focused Settings Dialog tests cover the Web Search section load/render path, mode selection dispatch, and configured-provider test dispatch.

## Gaps

No known implementation gaps from this audit remain. The explicit non-goals below remain out of scope: unofficial scraping of consumer search-result pages, retired Bing Search API support, and first-class Serper support until its current official API contract is verified.

Live account/provider behavior can still vary by plan, model, admin toggle, region, and provider rollout. Xero handles those as runtime readiness/check failures and keeps the configured-provider fallback available according to mode.

## Implementation Plan

Target outcome: an end user can enable web search from Settings, and agents can use `web_search` without any process environment variables. If the active LLM provider/model has LLM-provider-managed web search enabled and supported, Xero should use that first. If provider-managed search is unavailable or insufficient, Xero should fall back to the user's active in-app web-search provider profile. Users can add a fallback provider in Settings, save its API key with the same storage/redaction guarantees used for LLM provider keys, test it, and choose it as the active fallback provider.

Search-source precedence:

1. `disabled`: if the user or policy disables web search, agents do not search.
2. `provider_managed`: if enabled, supported by the active LLM provider/model, and allowed by policy/stage gates, use the LLM provider's own web-search path first. This includes true native model-provider tools and provider-managed server tools such as OpenRouter's web-search tool.
3. `configured_provider`: if provider-managed search is disabled, unsupported, unavailable for the user's account/plan, rate-limited, returns no usable cited sources/URLs, or fails with a retryable provider error, use the active in-app web-search provider profile.
4. `unavailable`: if neither source is ready, agents get a clear unavailable state and may only use `web_fetch` for exact HTTP/HTTPS URLs already known from the user or other context.

Settings should expose this as a web-search mode with `Auto` as the default: LLM-provider-managed search first, active configured provider second. Additional modes should be `Provider-managed only`, `Configured provider only`, and `Disabled`. The runtime should avoid running both search sources by default because that can double-bill users and produce confusing duplicate evidence.

LLM-provider-managed search candidates checked against current provider docs on 2026-05-31:

| Managed search source | Xero provider ids | Notes |
| --- | --- | --- |
| `anthropic_native_web_search` | `anthropic`; `vertex` for Anthropic-on-Vertex only when the selected model/API supports it | Anthropic exposes web search as a server tool for Claude. Direct Anthropic should be the first target. Anthropic-on-Bedrock should not be assumed because Anthropic documents Bedrock as unsupported for this tool; Vertex support must be capability-gated because support level varies by platform/model. |
| `openai_native_web_search` | `openai_api`; `azure_openai` and `openai_compatible` only after capability probing | OpenAI exposes web search through the Responses API and search-capable Chat Completions models. Xero should prefer the Responses tool path where the adapter already uses Responses-style requests. |
| `gemini_grounding_google_search` | `gemini_ai_studio`; future Vertex-Gemini providers if added separately | Gemini exposes Google Search grounding through a model tool and returns grounding metadata. Use only for Gemini models whose API docs/capability probe report grounding support. |
| `xai_native_web_search` | `xai` | xAI exposes web search as a Responses API tool. Use it when the selected Grok model supports the text runtime and native web search capability. |
| `openrouter_server_web_search` | `openrouter` | OpenRouter exposes a beta server-side web search tool. Treat it as LLM-provider-managed search for Xero's precedence rules, but do not describe it as always native to the underlying model. Its engine behavior should be recorded in diagnostics so users can see whether OpenRouter used a native provider path or an OpenRouter-selected engine such as Exa, Parallel, or Firecrawl. |
| `perplexity_native_web_grounding` | OpenRouter-routed Perplexity models today; future direct Perplexity provider only if Xero adds one | Perplexity-style models are search-grounded LLM providers rather than normalized SERP APIs. Do not add this as a direct source until Xero has a first-class Perplexity provider or a capability-probed OpenAI-compatible profile that returns cited source URLs. |

LLM-provider-managed search must still use in-app configuration only. It inherits the active LLM provider credential and model settings; it must not introduce web-search environment variables or a second copy of the LLM API key. Native or provider-managed search citations/results should be normalized into the same agent-visible search evidence shape used by configured providers, tagged with a source such as `provider_managed:<provider_id>`, and treated as untrusted web content.

Reference docs checked on 2026-05-31:

- Anthropic web search tool: <https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool>
- OpenAI web search tool: <https://developers.openai.com/api/docs/guides/tools-web-search>
- Gemini Grounding with Google Search: <https://ai.google.dev/gemini-api/docs/google-search>
- xAI Web Search tool: <https://docs.x.ai/developers/tools/web-search>
- OpenRouter web-search server tool: <https://openrouter.ai/docs/guides/features/server-tools/web-search>

Planned provider adapter set:

| Provider kind | Type | Auth/storage | Notes |
| --- | --- | --- | --- |
| `custom_endpoint` | Normalized endpoint | Optional bearer API key in provider credentials | Preserves the current Xero contract: `GET <endpoint>?q=<query>&limit=<count>` returning `{ "results": [{ "title", "url", "snippet" }] }`. |
| `brave_search` | Direct web index | API key in provider credentials, sent as Brave's subscription-token header | General web results from Brave Search. Initial adapter should normalize organic web results only; images/news/suggest can be later feature flags. |
| `tavily_search` | Agent/RAG search API | API key in provider credentials, sent as bearer auth | Good default for agents that need LLM-oriented snippets, domain filters, freshness, and optional raw content. Initial adapter should request result snippets only, not Tavily-generated answers. |
| `exa_search` | Neural/keyword web search API | API key in provider credentials, sent as Exa API-key header | Good for semantic discovery and source finding. Initial adapter should expose search type and optional highlights, while keeping result output normalized. |
| `firecrawl_search` | Search plus optional scrape | API key in provider credentials, sent as bearer auth | Useful when the user wants search results and optional page markdown. Initial adapter should default to URL/title/description only; scrape options should be explicit because they increase latency/cost. |
| `you_search` | Web/news search API | API key in provider credentials, sent as You.com API-key header | Good for AI-oriented web/news results. Initial adapter should normalize web results first and optionally include news when the query asks for news/current events. |
| `linkup_search` | AI-oriented web search API | API key in provider credentials, sent as bearer auth | Good for source-grounded search with shallow/deep modes. Initial adapter should use search-results output, not synthesized answer output. |
| `kagi_search` | Premium search API | API key in provider credentials, sent using Kagi API auth | Good for users who already pay for Kagi and want personal/premium search results. Settings should note account/API availability requirements. |
| `searxng_json` | Self-hosted or trusted metasearch | No key by default; optional bearer/basic credential if an instance requires it | Lets users point Xero at their own SearXNG `/search?format=json` instance. Settings must warn that many public instances disable JSON output or rate-limit automation. |
| `serpapi_google` | Google SERP compatibility API | API key in provider credentials, sent as SerpApi requires | Useful for Google-like SERP fields, location, and verticals. Initial adapter should normalize organic results and avoid returning ads/knowledge panels as ordinary sources. |
| `searchapi_google` | Google SERP compatibility API | API key in provider credentials | Similar Google SERP compatibility path. Initial adapter should normalize organic results and preserve provider diagnostics for rate-limit/parameter errors. |
| `google_cse` | Google Programmable Search / Custom Search JSON API | API key in provider credentials plus user-provided `cx` in provider settings | Compatibility adapter for users with an existing Programmable Search Engine. It should not be the default recommendation because setup requires a configured search engine id. |

Backlog / explicit non-goals:

- `serper_google` should be treated as a candidate adapter only after implementation work verifies current official API documentation, response schema, auth, and terms. Users can still use Serper through `custom_endpoint` or a small proxy if they need it before a first-class adapter exists.
- Legacy `bing_web_search` is not planned as a direct adapter because Microsoft retired the Bing Search APIs on August 11, 2025. Microsoft's Grounding with Bing Search is an Azure AI Agents/Foundry feature, not a direct normalized SERP API; evaluate it separately if Xero adds Azure-agent-specific integrations later.
- Unofficial scraping of consumer search-result pages is out of scope. Built-in providers must use documented APIs or user-owned/self-hosted endpoints.

1. Add app-data-backed autonomous web settings and provider profiles.
   - Add global OS app-data state for web search, not repo-local `.xero/` state. Store non-secret settings such as search mode, provider-managed enablement, active fallback provider id, fallback provider kind, display name, enabled state, endpoint/base URL where applicable, result-limit defaults, last-check status, and update timestamps.
   - Support multiple fallback provider profiles with exactly one active enabled fallback profile. A disabled fallback profile can remain saved, but it must not be selected for runtime fallback search.
   - Add provider-managed search capability metadata for LLM provider/model pairs. The metadata must distinguish documented support, unsupported providers/models, user/account enablement requirements, and "unknown, probe before use" states.
   - Include all provider kinds in the planned provider adapter set above. Each adapter owns request construction, auth header/query placement, response decoding, provider-specific errors, and normalization into `AutonomousWebSearchResult`.
   - Store provider-specific non-secret settings: search region/language where supported, freshness defaults where supported, result limit, safe-search preference where supported, and provider-specific fields such as Google CSE `cx` or SearXNG instance URL.
   - Remove env-backed search-provider configuration from runtime resolution. `DesktopState::autonomous_web_config` should resolve from saved app-data Settings only; if there is no active ready provider, the runtime has no configured search provider.
   - Environment variables must not enable, override, or bypass in-app Web Search settings. Diagnostics should report the active source as `settings` or `unconfigured`.

2. Store API keys like LLM provider keys.
   - Reuse the global `provider_credentials` pattern for web-search API keys instead of introducing a parallel secret store. Extend the credential provider catalog/validation so web-search provider ids are accepted without making them LLM runtime providers.
   - Store secrets only in credential rows/fields intended for provider secrets. Web-search settings rows must reference credential/provider ids and expose only readiness fields such as `hasApiKey`, `updatedAt`, and readiness proof.
   - Tauri commands and DTOs must never return raw API keys after save, matching LLM provider credential behavior.
   - Redaction must cover web-search provider ids, endpoint URLs, auth headers, query parameters, support bundles, diagnostics, development storage views, runtime stream summaries, logs, and failed provider-check payloads. Tokens must not appear in model-visible tool output.
   - Deleting a web-search provider profile must either delete its linked credential or clearly detach it according to the same UX rules used for LLM provider credentials.

3. Add Tauri commands and model contracts.
   - Add commands such as `autonomous_web_search_settings`, `autonomous_web_search_upsert_provider`, `autonomous_web_search_delete_provider`, `autonomous_web_search_set_active_provider`, and `autonomous_web_search_check_provider`.
   - Validate provider kind, HTTP/HTTPS endpoints, result limits, timeout limits, credential readiness, enabled/active invariants, and provider-specific required fields.
   - Expose configured-provider capability metadata to the UI: whether the fallback provider supports freshness, domains, locale/region, news, content extraction, safe search, self-hosting, or answer synthesis. The agent-facing `web_search` schema should stay provider-neutral unless a later design adds explicit advanced options.
   - Expose provider-managed search capability metadata to the UI and runtime: provider id, model id, native tool type/name where applicable, source/citation format, known unsupported deployment paths, and whether an account/admin toggle may be required.
   - The fallback provider check must use the same runtime adapter and transport path agents use, with a harmless query and bounded response. It should return a redacted status, normalized sample result count, latency, and actionable error code.
   - Add a provider-managed search readiness check where feasible. It should be bounded and redacted, should not leak prompts or API keys, and should classify failures such as unsupported model, admin/account disabled, rate-limited, unavailable, no cited sources, or provider returned answer-only output.
   - Keep `web_fetch` independent of search-provider settings.

4. Add a Settings UI.
   - Add a user-facing Web Search section in the existing agent/tooling settings area using ShadCN components only.
   - Let users choose the web-search mode: `Auto`, `Provider-managed only`, `Configured provider only`, or `Disabled`.
   - Show provider-managed search availability for the selected LLM provider/model when known, including unsupported/unknown/account-toggle-required states. The UI should explain fallback status through concise labels, not raw provider errors.
   - Let users add, edit, delete, enable/disable, test, and select the active fallback web-search provider. Show provider kind, endpoint/base URL when relevant, masked credential state, readiness, and last check result.
   - Saving a fallback provider with an API key should behave like LLM provider key entry: accept the secret once, persist it through the provider credential path, and then show only masked/readiness state.
   - Do not add temporary debug UI.

5. Wire agents to configured provider availability.
   - Keep `web_search` as the logical agent tool. Runtime dispatch chooses the actual source from the search-source precedence above.
   - In `Auto`, use provider-managed search first when the active LLM provider/model supports it and the user has not disabled it. Fall back to the active configured provider when provider-managed search is unsupported, unavailable, account-disabled, rate-limited, retryably failed, or did not produce usable cited source URLs.
   - In `Provider-managed only`, do not fall back to configured providers. If provider-managed search cannot run, return a clear unavailable state.
   - In `Configured provider only`, skip provider-managed LLM search and use the active fallback provider profile.
   - When neither source is ready, agents must receive a clear unavailable state. They should not retry blind searches or imply current web access. They may still use `web_fetch` for exact HTTP/HTTPS URLs supplied by the user or found elsewhere.
   - Tool access/catalog diagnostics should show whether `web_search` is ready through provider-managed search, ready through fallback provider, disabled, unsupported for the selected model, missing credentials, account-disabled, or unconfigured.
   - Stage gates, external-service policy checks, project capability revocations, and model-visible untrusted-content boundaries continue to apply exactly as they do today.

6. Tighten diagnostics and docs.
   - Add doctor/support-bundle output that reports web-search mode, selected runtime source, native provider/model capability status, active fallback provider kind, readiness, fallback reason, and last check status without exposing tokens or raw auth-bearing URLs.
   - Document the custom endpoint contract, named-provider setup flow, provider-managed LLM search behavior, fallback precedence, auth handling, result limits, status-code handling, body limits, and failure modes.
   - Update README to remove env-var web-search setup and document the Settings-only configuration path.

7. Expand tests.
   - Rust unit tests for settings validation, provider-profile CRUD, active-provider invariants, credential resolution, config resolution, provider check, search/fetch success, missing provider, disabled provider, missing key, invalid provider response, non-2xx status mapping, truncation, and redaction.
   - Provider-adapter tests for every provider kind in the planned adapter set, using local mock servers and no live network dependencies.
   - Contract fixture tests for provider response normalization: Brave, Tavily, Exa, Firecrawl, You.com, Linkup, Kagi, SearXNG JSON, SerpApi, SearchAPI.io, Google CSE, and `custom_endpoint`.
   - Native-search adapter tests for Anthropic, OpenAI, Gemini, xAI, and OpenRouter request construction, response/citation normalization, unsupported model handling, account-disabled handling, rate-limit handling, and no-usable-source fallback.
   - Frontend schema and Settings UI tests for mode selection, native availability display, add/edit/delete/enable/disable/select-active/save-key/load/test-provider flows.
   - Runtime/tool-access tests proving agents prefer provider-managed search in `Auto`, fall back to configured providers when provider-managed search is unavailable or insufficient, honor `Provider-managed only` and `Configured provider only`, avoid double-searching by default, and return a clear unavailable state when no configured source can run.
   - Config-resolution tests proving `XERO_AUTONOMOUS_WEB_SEARCH_URL` and `XERO_AUTONOMOUS_WEB_SEARCH_BEARER_TOKEN` do not configure or override web search.
   - Runtime stream, diagnostics, development-storage, and support-bundle tests proving API keys and bearer tokens are redacted.
   - A scoped integration test with a local mock search provider to prove the same Tauri runtime path works end-to-end.
