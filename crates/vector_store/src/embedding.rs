use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::serde_json;
use isahc::prelude::Configurable;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use util::http::{HttpClient, Request};

lazy_static! {
    static ref OPENAI_API_KEY: Option<String> = env::var("OPENAI_API_KEY").ok();
}

#[derive(Clone)]
pub struct OpenAIEmbeddings {
    pub client: Arc<dyn HttpClient>,
}

#[derive(Serialize)]
struct OpenAIEmbeddingRequest<'a> {
    model: &'static str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbedding>,
    usage: OpenAIEmbeddingUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbedding {
    embedding: Vec<f32>,
    index: usize,
    object: String,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingUsage {
    prompt_tokens: usize,
    total_tokens: usize,
}

#[async_trait]
pub trait EmbeddingProvider: Sync {
    async fn embed_batch(&self, spans: Vec<&str>) -> Result<Vec<Vec<f32>>>;
}

pub struct DummyEmbeddings {}

#[async_trait]
impl EmbeddingProvider for DummyEmbeddings {
    async fn embed_batch(&self, spans: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        // 1024 is the OpenAI Embeddings size for ada models.
        // the model we will likely be starting with.
        let dummy_vec = vec![0.32 as f32; 1536];
        return Ok(vec![dummy_vec; spans.len()]);
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddings {
    async fn embed_batch(&self, spans: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let api_key = OPENAI_API_KEY
            .as_ref()
            .ok_or_else(|| anyhow!("no api key"))?;

        let request = Request::post("https://api.openai.com/v1/embeddings")
            .redirect_policy(isahc::config::RedirectPolicy::Follow)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .body(
                serde_json::to_string(&OpenAIEmbeddingRequest {
                    input: spans,
                    model: "text-embedding-ada-002",
                })
                .unwrap()
                .into(),
            )?;

        let mut response = self.client.send(request).await?;
        if !response.status().is_success() {
            return Err(anyhow!("openai embedding failed {}", response.status()));
        }

        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        let response: OpenAIEmbeddingResponse = serde_json::from_str(&body)?;

        log::info!(
            "openai embedding completed. tokens: {:?}",
            response.usage.total_tokens
        );

        // do we need to re-order these based on the `index` field?
        eprintln!(
            "indices: {:?}",
            response
                .data
                .iter()
                .map(|embedding| embedding.index)
                .collect::<Vec<_>>()
        );

        Ok(response
            .data
            .into_iter()
            .map(|embedding| embedding.embedding)
            .collect())
    }
}
