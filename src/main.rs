#![allow(dead_code, unused_imports)]
#![allow(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags)]
//! # Cultivo Epistêmico — Semantic Chat
//!
//! **Ponto de entrada principal** da aplicação Cultivo Epistêmico.
//!
//! Este arquivo inicializa todos os componentes do sistema e inicia o servidor web.
//! A arquitetura segue um padrão de inicialização em duas fases:
//!
//! 1. **Fase imediata**: O servidor web (axum) é iniciado e começa a aceitar conexões
//!    em `http://localhost:3000` instantaneamente
//! 2. **Fase background**: O modelo BERTimbau (~400MB) é carregado em uma thread
//!    separada via `tokio::task::spawn_blocking`, sem bloquear o servidor
//!
//! ## Fluxo de Inicialização
//!
//! ```text
//! main()
//!   ├── Configura tracing/logging
//!   ├── Carrega KB do disco (ou cria vazia)
//!   ├── Cria broadcast channel para SSE
//!   ├── Monta AppState e Router
//!   ├── Inicia servidor TCP (porta 3000)
//!   └── Spawn background:
//!       ├── Carrega BERTimbau via HuggingFace Hub
//!       ├── Cria NluPipeline
//!       ├── Cria Orchestrator
//!       └── Publica em OnceLock (ModelReady)
//! ```
//!
//! ## Exemplo de Uso
//!
//! ```bash
//! # Executar com logs padrão (info)
//! cargo run
//!
//! # Executar com logs detalhados
//! RUST_LOG=debug cargo run
//!
//! # O servidor estará disponível em http://localhost:3000
//! ```
//!
//! ## Caso de Uso
//!
//! O sistema permite que um usuário converse em linguagem natural (português),
//! e automaticamente:
//! - Extrai conceitos do texto
//! - Encontra relações semânticas entre conceitos
//! - Realiza inferências lógicas (NARS)
//! - Faz perguntas reflexivas para aprofundar o conhecimento
//! - Visualiza o grafo de conhecimento em 3D

// Declaração dos módulos da aplicação.
// Cada módulo corresponde a uma camada da arquitetura:

/// Módulo `core` — tipos fundamentais: Concept, Link, TruthValue, KnowledgeBase.
mod core;

/// Módulo `inference` — motor de inferência NARS (dedução, indução).
mod inference;

/// Módulo `metrics` — coleta de métricas de sistema (CPU, RAM, GPU).
mod metrics;

/// Módulo `nlu` — pipeline de compreensão de linguagem natural (BERTimbau).
mod nlu;

/// Módulo `orchestrator` — orquestra o ciclo de cultivo epistêmico.
mod orchestrator;

/// Módulo `pdf` — ingestão e processamento de documentos PDF.
mod pdf;

/// Módulo `persistence` — serialização/desserialização da KB em JSON.
mod persistence;

/// Módulo `web` — servidor web axum, handlers HTTP, templates e SSE.
mod web;

use std::sync::{Arc, OnceLock};

use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

use crate::core::KnowledgeBase;
use crate::nlu::embedder::Embedder;
use crate::nlu::NluPipeline;
use crate::orchestrator::Orchestrator;
use crate::web::events::IngestionEvent;
use crate::web::state::{AppState, ModelReady};

/// Função principal assíncrona do Cultivo Epistêmico.
///
/// Inicializa o sistema em duas fases:
/// - **Fase 1 (síncrona)**: Carrega KB, cria estado compartilhado, inicia servidor
/// - **Fase 2 (background)**: Carrega modelo ML, cria pipeline NLU e orquestrador
///
/// O servidor fica acessível imediatamente enquanto o modelo carrega em background.
/// Quando o modelo termina de carregar, o `OnceLock` é preenchido e o sistema
/// passa a responder mensagens de chat.
///
/// # Erros
///
/// Retorna erro se:
/// - Não conseguir fazer bind na porta 3000
/// - O servidor axum falhar durante execução
#[tokio::main]
async fn main() -> Result<()> {
    // Configura o sistema de logging/tracing.
    // Aceita a variável de ambiente RUST_LOG para configurar o nível.
    // Exemplo: RUST_LOG=debug cargo run
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("🌱 Cultivo Epistêmico — Starting...");

    // Tenta carregar a base de conhecimento do disco (data/kb.json).
    // Se o arquivo não existir ou estiver corrompido, inicia com KB vazia.
    // A KB é envolta em Arc<RwLock<>> para acesso concorrente seguro.
    let kb = match persistence::load_kb() {
        Ok(loaded_kb) => {
            tracing::info!(
                concepts = loaded_kb.concept_count(),
                links = loaded_kb.link_count(),
                "KB carregada do disco"
            );
            Arc::new(RwLock::new(loaded_kb))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Falha ao carregar KB do disco, iniciando vazia");
            Arc::new(RwLock::new(KnowledgeBase::new()))
        }
    };

    // OnceLock para o modelo ML — será preenchido quando o modelo terminar de carregar.
    // Enquanto estiver vazio, o servidor responde "modelo carregando...".
    let model = Arc::new(OnceLock::new());

    // Canal broadcast para eventos SSE (Server-Sent Events).
    // Usado para streaming em tempo real durante a ingestão de PDFs.
    // Capacidade de 256 eventos — mensagens antigas são descartadas se o consumidor for lento.
    let (events_tx, _) = broadcast::channel::<IngestionEvent>(256);
    let events_tx = Arc::new(events_tx);

    // Estado compartilhado da aplicação — passado para todos os handlers via axum State.
    let state = AppState {
        model: model.clone(),
        kb: kb.clone(),
        events_tx,
    };

    // Cria o router com todas as rotas da aplicação.
    let app = web::create_router(state);

    // Inicia o servidor TCP — o servidor fica acessível IMEDIATAMENTE,
    // mesmo antes do modelo ML terminar de carregar.
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🚀 Server running at http://localhost:3000");

    // Carrega o modelo BERTimbau em uma thread de background.
    // Usa spawn_blocking porque o carregamento do modelo é uma operação
    // CPU-intensiva que bloquearia o runtime tokio se fosse feita inline.
    tokio::task::spawn_blocking(move || {
        tracing::info!("Loading BERTimbau model (first run downloads ~400MB)...");

        // Carrega o modelo de embeddings BERTimbau.
        // Na primeira execução, faz download dos pesos do HuggingFace Hub.
        let embedder = match Embedder::load() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("Failed to load embedder: {}", e);
                return;
            }
        };
        tracing::info!("Model loaded!");

        // Cria o pipeline NLU completo (embedder + classificador de intent + extrator).
        let nlu = match NluPipeline::new(embedder) {
            Ok(n) => Arc::new(n),
            Err(e) => {
                tracing::error!("Failed to create NLU pipeline: {}", e);
                return;
            }
        };
        tracing::info!("NLU pipeline initialized.");

        // Cria o orquestrador do ciclo epistêmico.
        let orchestrator = Mutex::new(Orchestrator::new(nlu.clone(), kb.clone()));

        // Publica o modelo no OnceLock — a partir deste ponto,
        // todos os handlers que verificam state.model.get() saberão que está pronto.
        let _ = model.set(ModelReady { orchestrator, nlu });
        tracing::info!("✅ System ready!");
    });

    // Inicia o servidor axum — bloqueia até que o processo seja encerrado.
    axum::serve(listener, app).await?;

    Ok(())
}
