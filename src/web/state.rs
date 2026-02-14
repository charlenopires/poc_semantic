//! # Estado da Aplicação Web
//!
//! Define as structs de estado compartilhado entre todos os handlers Axum.
//!
//! ## Padrão de Inicialização em Duas Fases
//!
//! ```text
//! Fase 1 (imediata):     Fase 2 (background):
//! ┌────────────────┐     ┌─────────────────┐
//! │ AppState       │     │ ModelReady       │
//! │  ├── kb ✓      │     │  ├── orchestrator│
//! │  ├── events_tx ✓│    │  └── nlu         │
//! │  └── model: ∅  │←────│  (set via OnceLock)
//! └────────────────┘     └─────────────────┘
//!       ↓ Web server                ↓ ~10s depois
//!    disponível                  modelo pronto
//! ```
//!
//! O `AppState` é criado na `main()` com `model = OnceLock::new()` (vazio).
//! A task em background carrega BERTimbau e preenche o `OnceLock` com
//! `ModelReady` quando pronto. Handlers verificam `model.get().is_some()`
//! para saber se podem processar mensagens.

use std::sync::{Arc, OnceLock};

use parking_lot::{Mutex, RwLock};
use tokio::sync::broadcast;

use crate::core::KnowledgeBase;
use crate::nlu::NluPipeline;
use crate::orchestrator::Orchestrator;
use crate::web::events::IngestionEvent;

/// Modelo ML + NLU, inicializado em background (~10s).
///
/// Contém o orquestrador (protegido por Mutex para acesso exclusivo)
/// e a pipeline NLU (Arc para compartilhamento com PDF ingestion).
///
/// ## Por que Mutex no Orchestrator?
///
/// O [`Orchestrator`] mantém estado mutável (contadores, last_discussed),
/// portanto precisa de acesso exclusivo. `Mutex` (não `RwLock`) porque
/// `process_message()` sempre precisa de `&mut self`.
///
/// ## Por que Arc no NluPipeline?
///
/// A [`NluPipeline`] é compartilhada entre o chat handler (via Orchestrator)
/// e o PDF ingestion handler (direto). O `Arc` permite esse compartilhamento
/// sem cópia do modelo ML.
pub struct ModelReady {
    /// Orquestrador do ciclo de cultivo (acesso exclusivo via Mutex).
    pub orchestrator: Mutex<Orchestrator>,
    /// Pipeline NLU compartilhada (imutável após criação).
    pub nlu: Arc<NluPipeline>,
}

/// Estado compartilhado da aplicação Axum.
///
/// Implementa `Clone` (via `Arc`) para ser passado a cada handler.
/// Todos os campos são thread-safe (`Arc`-wrapped).
///
/// ## Acesso pelos Handlers
///
/// ```rust,ignore
/// async fn handler(State(state): State<AppState>) -> impl IntoResponse {
///     let kb = state.kb.read();           // leitura da KB
///     let model = state.model.get();       // modelo (se pronto)
///     let _ = state.events_tx.send(event); // emitir SSE
/// }
/// ```
#[derive(Clone)]
pub struct AppState {
    /// Modelo ML, preenchido em background via `OnceLock::set()`.
    /// `None` (get() retorna None) durante os ~10s de carregamento.
    pub model: Arc<OnceLock<ModelReady>>,
    /// Base de conhecimento compartilhada, protegida por `RwLock`.
    /// Permite múltiplas leituras simultâneas (sidebar, graph, queries).
    pub kb: Arc<RwLock<KnowledgeBase>>,
    /// Canal broadcast para eventos SSE de ingestão de PDF.
    /// Múltiplos subscribers (browsers) recebem todos os eventos.
    pub events_tx: Arc<broadcast::Sender<IngestionEvent>>,
}
