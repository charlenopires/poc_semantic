//! # Embedder via LM Studio — Gerador de Representações Vetoriais
//!
//! O [`Embedder`] usa o LM Studio como servidor externo de modelos,
//! acessando sua API OpenAI-compatible (`/v1/embeddings` e `/v1/chat/completions`).
//!
//! ## O que são Embeddings?
//!
//! Embeddings são vetores numéricos de 768 dimensões que representam
//! o **significado semântico** de um texto. Textos com significados
//! similares terão vetores próximos no espaço vetorial.
//!
//! ## Modelos
//!
//! | Finalidade | Modelo | Dimensão |
//! |------------|--------|----------|
//! | Embeddings | `nomic-embed-text` | 768-dim |
//! | Chat/LLM | Configurável via env | — |
//!
//! ## Configuração
//!
//! | Variável | Default | Descrição |
//! |----------|---------|-----------|
//! | `LM_STUDIO_URL` | `http://localhost:1234/v1` | URL base do LM Studio |
//! | `LM_STUDIO_EMBED_MODEL` | `nomic-embed-text` | Modelo de embeddings |
//! | `LM_STUDIO_CHAT_MODEL` | `default` | Modelo de chat |

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Configuração do Embedder para conexão com LM Studio.
pub struct EmbedderConfig {
    /// URL base da API do LM Studio (ex: `http://localhost:1234/v1`).
    pub base_url: String,
    /// Nome do modelo de embeddings carregado no LM Studio.
    pub embed_model: String,
    /// Nome do modelo de chat carregado no LM Studio.
    pub chat_model: String,
}

impl EmbedderConfig {
    /// Cria configuração a partir de variáveis de ambiente.
    ///
    /// | Variável | Default |
    /// |----------|---------|
    /// | `LM_STUDIO_URL` | `http://localhost:1234/v1` |
    /// | `LM_STUDIO_EMBED_MODEL` | `nomic-embed-text` |
    /// | `LM_STUDIO_CHAT_MODEL` | `default` |
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("LM_STUDIO_URL")
                .unwrap_or_else(|_| "http://localhost:1234/v1".to_string()),
            embed_model: std::env::var("LM_STUDIO_EMBED_MODEL")
                .unwrap_or_else(|_| "nomic-embed-text".to_string()),
            chat_model: std::env::var("LM_STUDIO_CHAT_MODEL")
                .unwrap_or_else(|_| "default".to_string()),
        }
    }
}

/// Embedder via LM Studio — gera representações vetoriais e respostas de chat.
///
/// Encapsula um HTTP client para comunicação com o LM Studio.
/// A criação é instantânea (sem download de modelo).
pub struct Embedder {
    /// HTTP client com timeout configurado.
    client: reqwest::Client,
    /// Configuração de conexão (URL, modelos).
    config: EmbedderConfig,
}

impl Embedder {
    /// Cria um novo Embedder com HTTP client configurado.
    ///
    /// A criação é instantânea — não faz download de modelo.
    /// O timeout de 60s acomoda batches grandes de embeddings.
    pub fn new(config: EmbedderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        tracing::info!(
            url = %config.base_url,
            embed_model = %config.embed_model,
            chat_model = %config.chat_model,
            "Embedder configurado para LM Studio"
        );

        Self { client, config }
    }

    /// Acessor para a configuração do embedder.
    pub fn config(&self) -> &EmbedderConfig {
        &self.config
    }

    /// Verifica se o LM Studio está acessível.
    ///
    /// Faz GET `/models` e verifica se retorna HTTP 200.
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/models", self.config.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Falha ao conectar ao LM Studio")?;

        if resp.status().is_success() {
            tracing::info!("LM Studio acessivel em {}", self.config.base_url);
            Ok(())
        } else {
            anyhow::bail!(
                "LM Studio retornou status {} em {}",
                resp.status(),
                url
            )
        }
    }

    /// Gera embedding de um texto único → `Vec<f32>` (768 dims).
    ///
    /// Faz POST `/embeddings` com o texto como input.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.config.base_url);
        let request = EmbeddingRequest {
            input: serde_json::Value::String(text.to_string()),
            model: self.config.embed_model.clone(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Falha ao enviar request de embedding")?;

        let response: EmbeddingResponse = resp
            .json()
            .await
            .context("Falha ao decodificar response de embedding")?;

        response
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("Response de embedding vazio")
    }

    /// Gera embeddings de múltiplos textos em uma única chamada HTTP.
    ///
    /// Mais eficiente que chamar [`embed()`](Embedder::embed) repetidamente
    /// pois usa uma única request com array de inputs.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if texts.len() == 1 {
            return Ok(vec![self.embed(&texts[0]).await?]);
        }

        let url = format!("{}/embeddings", self.config.base_url);
        let request = EmbeddingRequest {
            input: serde_json::Value::Array(
                texts
                    .iter()
                    .map(|t| serde_json::Value::String(t.clone()))
                    .collect(),
            ),
            model: self.config.embed_model.clone(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Falha ao enviar request de embedding batch")?;

        let mut response: EmbeddingResponse = resp
            .json()
            .await
            .context("Falha ao decodificar response de embedding batch")?;

        // Ordena por index para garantir a ordem correta
        response.data.sort_by_key(|d| d.index);

        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }

    /// Envia mensagem para o LLM e retorna a resposta gerada.
    ///
    /// Faz POST `/chat/completions` com system prompt embutido na mensagem do usuário.
    /// O system prompt é mesclado no conteúdo da mensagem `user` para compatibilidade
    /// com modelos que não suportam a role `system`.
    pub async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let request = ChatRequest {
            model: self.config.chat_model.clone(),
            messages: vec![ChatMsg {
                role: "user".to_string(),
                content: format!("{}\n\n{}", system_prompt, user_message),
            }],
            temperature: 0.7,
            max_tokens: 512,
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Falha ao enviar request de chat")?;

        let response: ChatResponse = resp
            .json()
            .await
            .context("Falha ao decodificar response de chat")?;

        response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .context("Response de chat vazio")
    }
}

// ─── Tipos de Request/Response para a API LM Studio ──────────────

#[derive(Serialize)]
struct EmbeddingRequest {
    input: serde_json::Value,
    model: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    #[serde(default)]
    index: usize,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMsg>,
    temperature: f32,
    max_tokens: i32,
}

#[derive(Serialize)]
struct ChatMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMsg,
}

#[derive(Deserialize)]
struct ChatResponseMsg {
    content: String,
}
