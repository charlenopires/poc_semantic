#![allow(dead_code, unused_imports)]
#![allow(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags)]
//! # Cultivo Epistêmico — Semantic Chat
//!
//! **Ponto de entrada principal** da aplicação Cultivo Epistêmico.
//!
//! Este arquivo inicializa todos os componentes do sistema e inicia o servidor web.
//! A arquitetura segue um padrão de inicialização em duas fases:
//!
//! 1. **Fase imediata**: O servidor web (axum) é iniciado instantaneamente
//! 2. **Fase background**: A NLU é inicializada via LM Studio (async, sem download)
//!
//! ## Fluxo de Inicialização
//!
//! ```text
//! main()
//!   ├── Configura tracing/logging
//!   ├── Cria KB vazia (modo LM Studio)
//!   ├── Cria broadcast channel para SSE
//!   ├── Monta AppState e Router
//!   ├── Inicia servidor TCP (porta 3000)
//!   └── Spawn async task:
//!       ├── Cria Embedder (HTTP client, instantâneo)
//!       ├── Health check no LM Studio
//!       ├── Cria NluPipeline (embeda templates de intent via LM Studio)
//!       ├── Cria Orchestrator
//!       └── Publica em OnceLock (ModelReady)
//! ```

// Declaração dos módulos da aplicação.

/// Módulo `core` — tipos fundamentais: Concept, Link, TruthValue, KnowledgeBase.
mod core;

/// Módulo `inference` — motor de inferência NARS (dedução, indução).
mod inference;

/// Módulo `metrics` — coleta de métricas de sistema (CPU, RAM, GPU).
mod metrics;

/// Módulo `nlu` — pipeline de compreensão de linguagem natural (via LM Studio).
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
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

use crate::core::KnowledgeBase;
use crate::nlu::embedder::{Embedder, EmbedderConfig};
use crate::nlu::NluPipeline;
use crate::orchestrator::Orchestrator;
use crate::web::events::IngestionEvent;
use crate::web::state::{AppState, ModelReady};

/// Função principal assíncrona do Cultivo Epistêmico.
#[tokio::main]
async fn main() -> Result<()> {
    // Configura o sistema de logging/tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Cultivo Epistêmico — Starting...");

    // Sempre iniciar com KB vazia (modo LM Studio — troca de modelo de embeddings)
    let kb = Arc::new(RwLock::new(KnowledgeBase::new()));
    tracing::info!("KB inicializada vazia (modo LM Studio)");

    // OnceLock para o modelo — será preenchido quando a NLU estiver pronta.
    let model = Arc::new(OnceLock::new());

    // Canal broadcast para eventos SSE.
    let (events_tx, _) = broadcast::channel::<IngestionEvent>(2048);
    let events_tx = Arc::new(events_tx);

    // Estado compartilhado da aplicação.
    let state = AppState {
        model: model.clone(),
        kb: kb.clone(),
        events_tx,
    };

    // Cria o router com todas as rotas.
    let app = web::create_router(state);

    // Inicia o servidor TCP — acessível imediatamente.
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Server running at http://localhost:3000");

    // Inicializa NLU via LM Studio em background (async, não blocking).
    tokio::spawn(async move {
        let config = EmbedderConfig::from_env();
        let embedder = Embedder::new(config);

        // Health check — verifica se o LM Studio está acessível
        if let Err(e) = embedder.health_check().await {
            tracing::warn!(error = %e, "LM Studio nao acessivel — NLU aguardando servidor");
        }

        // Cria pipeline NLU (embeda templates de intent via LM Studio)
        match NluPipeline::new(embedder).await {
            Ok(nlu) => {
                let nlu = Arc::new(nlu);
                let orchestrator = tokio::sync::Mutex::new(Orchestrator::new(nlu.clone(), kb.clone()));
                let _ = model.set(ModelReady { orchestrator, nlu });
                tracing::info!("NLU pipeline pronta via LM Studio!");
            }
            Err(e) => {
                tracing::error!(error = %e, "Falha ao inicializar NLU pipeline");
            }
        }
    });

    // Inicia o servidor axum.
    axum::serve(listener, app).await?;

    Ok(())
}
