use std::sync::LazyLock;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::commands::CommandError;

pub const AGENT_RETRIEVAL_EMBEDDING_DIM: i32 = 768;
pub const DEFAULT_AGENT_EMBEDDING_PROVIDER: &str = "openai";
pub const DEFAULT_AGENT_EMBEDDING_MODEL: &str = "openai:text-embedding-3-small";
pub const DEFAULT_AGENT_EMBEDDING_VERSION: &str = "openai:text-embedding-3-small:dimensions-768:v1";
pub const LOCAL_HASH_AGENT_EMBEDDING_PROVIDER: &str = "local_hash";
pub const LOCAL_HASH_AGENT_EMBEDDING_MODEL: &str = "xero-local-hash-embedding";
pub const LOCAL_HASH_AGENT_EMBEDDING_VERSION: &str = "xero-local-hash-embedding.v1";

#[cfg(not(test))]
const DEFAULT_OPENAI_EMBEDDING_BASE_URL: &str = "https://api.openai.com/v1";
#[cfg(not(test))]
const DEFAULT_OPENAI_EMBEDDING_REQUEST_MODEL: &str = "text-embedding-3-small";
const OPENAI_EMBEDDING_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentEmbedding {
    pub vector: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub dimension: i32,
    pub version: String,
    pub migration_state: String,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEmbeddingProviderConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub request_model: String,
    pub storage_model: String,
    pub dimension: i32,
    pub version: String,
}

pub trait AgentEmbeddingService {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn dimension(&self) -> i32;
    fn version(&self) -> &str;
    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError>;

    fn embed_with_metadata(&self, text: &str) -> Result<AgentEmbedding, CommandError> {
        let vector = self.embed(text)?;
        Ok(AgentEmbedding {
            vector,
            provider: self.provider().to_string(),
            model: self.model().to_string(),
            dimension: self.dimension(),
            version: self.version().to_string(),
            migration_state: "current".to_string(),
            fallback_reason: None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalHashEmbeddingService;

impl AgentEmbeddingService for LocalHashEmbeddingService {
    fn provider(&self) -> &str {
        LOCAL_HASH_AGENT_EMBEDDING_PROVIDER
    }

    fn model(&self) -> &str {
        LOCAL_HASH_AGENT_EMBEDDING_MODEL
    }

    fn dimension(&self) -> i32 {
        AGENT_RETRIEVAL_EMBEDDING_DIM
    }

    fn version(&self) -> &str {
        LOCAL_HASH_AGENT_EMBEDDING_VERSION
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError> {
        hash_embedding(text, AGENT_RETRIEVAL_EMBEDDING_DIM)
    }
}

#[derive(Debug)]
pub struct OpenAiCompatibleEmbeddingService {
    config: AgentEmbeddingProviderConfig,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatibleEmbeddingService {
    pub fn new(config: AgentEmbeddingProviderConfig) -> Result<Self, CommandError> {
        validate_embedding_dimension(config.dimension)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(OPENAI_EMBEDDING_TIMEOUT_SECONDS))
            .build()
            .map_err(|error| {
                CommandError::retryable(
                    "agent_embedding_provider_client_failed",
                    format!("Xero could not prepare the embedding provider client: {error}"),
                )
            })?;
        Ok(Self { config, client })
    }

    fn endpoint(&self) -> String {
        format!("{}/embeddings", self.config.base_url.trim_end_matches('/'))
    }
}

impl AgentEmbeddingService for OpenAiCompatibleEmbeddingService {
    fn provider(&self) -> &str {
        &self.config.provider
    }

    fn model(&self) -> &str {
        &self.config.storage_model
    }

    fn dimension(&self) -> i32 {
        self.config.dimension
    }

    fn version(&self) -> &str {
        &self.config.version
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError> {
        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&serde_json::json!({
                "model": self.config.request_model,
                "input": text,
                "dimensions": self.config.dimension,
                "encoding_format": "float",
            }))
            .send()
            .map_err(|error| {
                CommandError::retryable(
                    "agent_embedding_provider_request_failed",
                    format!(
                        "Xero could not request embeddings from provider `{}` model `{}`: {error}",
                        self.config.provider, self.config.request_model
                    ),
                )
            })?;
        let status = response.status();
        let body = response.text().map_err(|error| {
            CommandError::retryable(
                "agent_embedding_provider_response_read_failed",
                format!(
                    "Xero could not read the embedding response from provider `{}`: {error}",
                    self.config.provider
                ),
            )
        })?;
        if !status.is_success() {
            return Err(CommandError::retryable(
                "agent_embedding_provider_request_failed",
                format!(
                    "Xero embedding provider `{}` rejected model `{}` with HTTP status {}.",
                    self.config.provider, self.config.request_model, status
                ),
            ));
        }
        parse_openai_embedding_response(&body, self.config.dimension)
    }
}

#[derive(Debug)]
pub struct DefaultAgentEmbeddingService {
    primary: Option<OpenAiCompatibleEmbeddingService>,
    fallback: LocalHashEmbeddingService,
    unavailable_reason: Option<String>,
}

impl DefaultAgentEmbeddingService {
    fn from_environment() -> Self {
        #[cfg(test)]
        {
            Self {
                primary: None,
                fallback: LocalHashEmbeddingService,
                unavailable_reason: None,
            }
        }
        #[cfg(not(test))]
        match AgentEmbeddingProviderConfig::from_environment() {
            Ok(Some(config)) => match OpenAiCompatibleEmbeddingService::new(config) {
                Ok(primary) => Self {
                    primary: Some(primary),
                    fallback: LocalHashEmbeddingService,
                    unavailable_reason: None,
                },
                Err(error) => Self::fallback_with_reason(error.code),
            },
            Ok(None) if embedding_provider_env_requested() => Self::fallback_with_reason(
                "agent_embedding_provider_credentials_missing".to_string(),
            ),
            Ok(None) => Self {
                primary: None,
                fallback: LocalHashEmbeddingService,
                unavailable_reason: None,
            },
            Err(error) => Self::fallback_with_reason(error.code),
        }
    }

    fn fallback_with_reason(reason: String) -> Self {
        Self {
            primary: None,
            fallback: LocalHashEmbeddingService,
            unavailable_reason: Some(reason),
        }
    }

    fn fallback_embedding(
        &self,
        text: &str,
        reason: Option<String>,
    ) -> Result<AgentEmbedding, CommandError> {
        let mut embedding = self.fallback.embed_with_metadata(text)?;
        if let Some(reason) = reason {
            embedding.migration_state = "fallback".to_string();
            embedding.fallback_reason = Some(reason);
        }
        Ok(embedding)
    }
}

impl AgentEmbeddingService for DefaultAgentEmbeddingService {
    fn provider(&self) -> &str {
        self.primary
            .as_ref()
            .map(AgentEmbeddingService::provider)
            .unwrap_or_else(|| self.fallback.provider())
    }

    fn model(&self) -> &str {
        self.primary
            .as_ref()
            .map(AgentEmbeddingService::model)
            .unwrap_or_else(|| self.fallback.model())
    }

    fn dimension(&self) -> i32 {
        self.primary
            .as_ref()
            .map(AgentEmbeddingService::dimension)
            .unwrap_or_else(|| self.fallback.dimension())
    }

    fn version(&self) -> &str {
        self.primary
            .as_ref()
            .map(AgentEmbeddingService::version)
            .unwrap_or_else(|| self.fallback.version())
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError> {
        self.embed_with_metadata(text)
            .map(|embedding| embedding.vector)
    }

    fn embed_with_metadata(&self, text: &str) -> Result<AgentEmbedding, CommandError> {
        if let Some(primary) = self.primary.as_ref() {
            match primary.embed_with_metadata(text) {
                Ok(embedding) => return Ok(embedding),
                Err(error) => return self.fallback_embedding(text, Some(error.code)),
            }
        }
        self.fallback_embedding(text, self.unavailable_reason.clone())
    }
}

impl AgentEmbeddingProviderConfig {
    #[cfg(not(test))]
    fn from_environment() -> Result<Option<Self>, CommandError> {
        let api_key = std::env::var("XERO_AGENT_EMBEDDING_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(api_key) = api_key else {
            return Ok(None);
        };
        let provider = std::env::var("XERO_AGENT_EMBEDDING_PROVIDER")
            .unwrap_or_else(|_| DEFAULT_AGENT_EMBEDDING_PROVIDER.to_string())
            .trim()
            .to_string();
        let request_model = std::env::var("XERO_AGENT_EMBEDDING_MODEL")
            .unwrap_or_else(|_| DEFAULT_OPENAI_EMBEDDING_REQUEST_MODEL.to_string())
            .trim()
            .to_string();
        let base_url = std::env::var("XERO_AGENT_EMBEDDING_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| DEFAULT_OPENAI_EMBEDDING_BASE_URL.to_string())
            .trim()
            .trim_end_matches('/')
            .to_string();
        let dimension = std::env::var("XERO_AGENT_EMBEDDING_DIMENSION")
            .ok()
            .map(|value| {
                value.parse::<i32>().map_err(|error| {
                    CommandError::user_fixable(
                        "agent_embedding_provider_dimension_invalid",
                        format!(
                            "Xero could not parse XERO_AGENT_EMBEDDING_DIMENSION `{value}` as a positive integer: {error}"
                        ),
                    )
                })
            })
            .transpose()?
            .unwrap_or(AGENT_RETRIEVAL_EMBEDDING_DIM);
        validate_embedding_dimension(dimension)?;
        if provider.is_empty() || request_model.is_empty() || base_url.is_empty() {
            return Err(CommandError::user_fixable(
                "agent_embedding_provider_config_invalid",
                "Xero embedding provider, model, and base URL must be non-empty when an embedding API key is configured.",
            ));
        }
        Ok(Some(Self::openai_compatible(
            provider,
            base_url,
            api_key,
            request_model,
            dimension,
        )))
    }

    pub fn openai_compatible(
        provider: String,
        base_url: String,
        api_key: String,
        request_model: String,
        dimension: i32,
    ) -> Self {
        let storage_model = format!("{provider}:{request_model}");
        let version = format!("{storage_model}:dimensions-{dimension}:v1");
        Self {
            provider,
            base_url,
            api_key,
            request_model,
            storage_model,
            dimension,
            version,
        }
    }
}

#[cfg(not(test))]
fn embedding_provider_env_requested() -> bool {
    [
        "XERO_AGENT_EMBEDDING_PROVIDER",
        "XERO_AGENT_EMBEDDING_MODEL",
        "XERO_AGENT_EMBEDDING_BASE_URL",
        "XERO_AGENT_EMBEDDING_DIMENSION",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some())
}

pub fn default_embedding_service() -> &'static DefaultAgentEmbeddingService {
    static SERVICE: LazyLock<DefaultAgentEmbeddingService> =
        LazyLock::new(DefaultAgentEmbeddingService::from_environment);
    &SERVICE
}

pub fn embedding_for_storage(text: &str) -> Result<AgentEmbedding, CommandError> {
    embedding_with_service(default_embedding_service(), text)
}

pub fn embedding_with_service(
    service: &dyn AgentEmbeddingService,
    text: &str,
) -> Result<AgentEmbedding, CommandError> {
    validate_embedding_dimension(service.dimension())?;
    let embedding = service.embed_with_metadata(text)?;
    if embedding.vector.len() != embedding.dimension as usize {
        return Err(CommandError::system_fault(
            "agent_embedding_vector_dimension_mismatch",
            format!(
                "Xero embedding service `{}` returned {} dimensions, but declared {}.",
                service.model(),
                embedding.vector.len(),
                embedding.dimension
            ),
        ));
    }
    validate_embedding_dimension(embedding.dimension)?;
    Ok(embedding)
}

pub fn validate_embedding_dimension(dimension: i32) -> Result<(), CommandError> {
    if dimension == AGENT_RETRIEVAL_EMBEDDING_DIM {
        Ok(())
    } else {
        Err(CommandError::system_fault(
            "agent_retrieval_embedding_dimension_mismatch",
            format!(
                "Xero configured embedding dimension {dimension}, but the Lance retrieval tables require {AGENT_RETRIEVAL_EMBEDDING_DIM}."
            ),
        ))
    }
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_value = f64::from(*left_value);
        let right_value = f64::from(*right_value);
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        (dot / (left_norm.sqrt() * right_norm.sqrt())).max(0.0)
    }
}

pub fn embedding_provider_for_model(model: Option<&str>) -> &'static str {
    match model {
        Some(model) if model == LOCAL_HASH_AGENT_EMBEDDING_MODEL => {
            LOCAL_HASH_AGENT_EMBEDDING_PROVIDER
        }
        Some(model) if model.starts_with("openai:") => DEFAULT_AGENT_EMBEDDING_PROVIDER,
        Some(_) => "custom",
        None => "unavailable",
    }
}

fn parse_openai_embedding_response(
    body: &str,
    expected_dimension: i32,
) -> Result<Vec<f32>, CommandError> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        CommandError::retryable(
            "agent_embedding_provider_response_decode_failed",
            format!("Xero could not decode the embedding provider response as JSON: {error}"),
        )
    })?;
    let embedding = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|data| data.first())
        .and_then(|item| item.get("embedding"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CommandError::retryable(
                "agent_embedding_provider_response_missing_embedding",
                "Xero embedding provider response did not include data[0].embedding.",
            )
        })?;
    let mut vector = Vec::with_capacity(embedding.len());
    for value in embedding {
        let Some(value) = value.as_f64() else {
            return Err(CommandError::retryable(
                "agent_embedding_provider_response_invalid_vector",
                "Xero embedding provider returned a non-numeric embedding value.",
            ));
        };
        vector.push(value as f32);
    }
    if vector.len() != expected_dimension as usize {
        return Err(CommandError::retryable(
            "agent_embedding_provider_dimension_mismatch",
            format!(
                "Xero embedding provider returned {} dimensions, but the configured model declares {expected_dimension}.",
                vector.len()
            ),
        ));
    }
    Ok(vector)
}

fn hash_embedding(text: &str, dimension: i32) -> Result<Vec<f32>, CommandError> {
    if dimension <= 0 {
        return Err(CommandError::system_fault(
            "agent_embedding_dimension_invalid",
            "Xero embedding dimensions must be positive.",
        ));
    }
    let dimension = dimension as usize;
    let mut vector = vec![0.0_f32; dimension];
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Ok(vector);
    }

    for token in tokens {
        let digest = Sha256::digest(token.as_bytes());
        let index = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]) as usize
            % dimension;
        let sign = if digest[8] & 1 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 + ((digest[9] % 7) as f32 / 16.0);
        vector[index] += sign * weight;
    }

    let norm = vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value = (f64::from(*value) / norm) as f32;
        }
    }
    Ok(vector)
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            current.push(character);
        } else if !current.is_empty() {
            push_token_variants(&mut tokens, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_token_variants(&mut tokens, &current);
    }
    tokens
}

fn push_token_variants(tokens: &mut Vec<String>, token: &str) {
    tokens.push(format!("tok:{token}"));
    let chars = token.chars().collect::<Vec<_>>();
    if chars.len() >= 4 {
        for window in chars.windows(4) {
            tokens.push(format!("gram:{}", window.iter().collect::<String>()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BadDimensionService;

    impl AgentEmbeddingService for BadDimensionService {
        fn provider(&self) -> &str {
            "bad-provider"
        }

        fn model(&self) -> &str {
            "bad"
        }

        fn dimension(&self) -> i32 {
            32
        }

        fn version(&self) -> &str {
            "bad.v1"
        }

        fn embed(&self, _text: &str) -> Result<Vec<f32>, CommandError> {
            Ok(vec![0.0; 32])
        }
    }

    #[test]
    fn local_hash_embeddings_are_deterministic_and_normalized() {
        let service = LocalHashEmbeddingService;
        let first = service
            .embed("Project records use LanceDB retrieval.")
            .unwrap();
        let second = service
            .embed("Project records use LanceDB retrieval.")
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), AGENT_RETRIEVAL_EMBEDDING_DIM as usize);
        assert!(cosine_similarity(&first, &second) > 0.99);
    }

    #[test]
    fn embedding_dimension_mismatch_is_refused() {
        let err = embedding_with_service(&BadDimensionService, "text").unwrap_err();
        assert_eq!(err.code, "agent_retrieval_embedding_dimension_mismatch");
    }

    #[test]
    fn s32_openai_compatible_config_declares_real_embedding_identity() {
        let config = AgentEmbeddingProviderConfig::openai_compatible(
            "openai".into(),
            "https://api.openai.com/v1".into(),
            "test-key".into(),
            "text-embedding-3-small".into(),
            AGENT_RETRIEVAL_EMBEDDING_DIM,
        );
        let service = OpenAiCompatibleEmbeddingService::new(config).unwrap();

        assert_eq!(service.provider(), DEFAULT_AGENT_EMBEDDING_PROVIDER);
        assert_eq!(service.model(), DEFAULT_AGENT_EMBEDDING_MODEL);
        assert_eq!(service.dimension(), AGENT_RETRIEVAL_EMBEDDING_DIM);
        assert_eq!(service.version(), DEFAULT_AGENT_EMBEDDING_VERSION);
    }

    #[test]
    fn s32_openai_embedding_response_parser_validates_dimensions() {
        let body = serde_json::json!({
            "data": [{
                "embedding": vec![0.125_f32; AGENT_RETRIEVAL_EMBEDDING_DIM as usize]
            }]
        })
        .to_string();

        let vector = parse_openai_embedding_response(&body, AGENT_RETRIEVAL_EMBEDDING_DIM)
            .expect("embedding response parses");

        assert_eq!(vector.len(), AGENT_RETRIEVAL_EMBEDDING_DIM as usize);
        assert_eq!(vector[0], 0.125);
    }

    #[test]
    fn s32_default_service_marks_local_hash_as_explicit_fallback() {
        let service = DefaultAgentEmbeddingService::fallback_with_reason(
            "agent_embedding_provider_credentials_missing".into(),
        );
        let embedding = embedding_with_service(&service, "fallback text").unwrap();

        assert_eq!(embedding.provider, LOCAL_HASH_AGENT_EMBEDDING_PROVIDER);
        assert_eq!(embedding.model, LOCAL_HASH_AGENT_EMBEDDING_MODEL);
        assert_eq!(embedding.migration_state, "fallback");
        assert_eq!(
            embedding.fallback_reason.as_deref(),
            Some("agent_embedding_provider_credentials_missing")
        );
    }
}
