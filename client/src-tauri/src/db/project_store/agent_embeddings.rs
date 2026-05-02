use sha2::{Digest, Sha256};

use crate::commands::CommandError;

pub const AGENT_RETRIEVAL_EMBEDDING_DIM: i32 = 768;
pub const DEFAULT_AGENT_EMBEDDING_MODEL: &str = "xero-local-hash-embedding";
pub const DEFAULT_AGENT_EMBEDDING_VERSION: &str = "xero-local-hash-embedding.v1";

#[derive(Debug, Clone, PartialEq)]
pub struct AgentEmbedding {
    pub vector: Vec<f32>,
    pub model: String,
    pub dimension: i32,
    pub version: String,
}

pub trait AgentEmbeddingService {
    fn model(&self) -> &str;
    fn dimension(&self) -> i32;
    fn version(&self) -> &str;
    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalHashEmbeddingService;

impl AgentEmbeddingService for LocalHashEmbeddingService {
    fn model(&self) -> &str {
        DEFAULT_AGENT_EMBEDDING_MODEL
    }

    fn dimension(&self) -> i32 {
        AGENT_RETRIEVAL_EMBEDDING_DIM
    }

    fn version(&self) -> &str {
        DEFAULT_AGENT_EMBEDDING_VERSION
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, CommandError> {
        hash_embedding(text, AGENT_RETRIEVAL_EMBEDDING_DIM)
    }
}

pub fn default_embedding_service() -> &'static LocalHashEmbeddingService {
    static SERVICE: LocalHashEmbeddingService = LocalHashEmbeddingService;
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
    let vector = service.embed(text)?;
    if vector.len() != service.dimension() as usize {
        return Err(CommandError::system_fault(
            "agent_embedding_vector_dimension_mismatch",
            format!(
                "Xero embedding service `{}` returned {} dimensions, but declared {}.",
                service.model(),
                vector.len(),
                service.dimension()
            ),
        ));
    }
    Ok(AgentEmbedding {
        vector,
        model: service.model().to_string(),
        dimension: service.dimension(),
        version: service.version().to_string(),
    })
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
}
