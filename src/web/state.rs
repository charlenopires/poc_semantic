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
//!       ↓ Web server                ↓ async init
//!    disponível                  modelo pronto
//! ```

use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;
use tokio::sync::{broadcast, Mutex};

use crate::core::KnowledgeBase;
use crate::nlu::NluPipeline;
use crate::orchestrator::Orchestrator;
use crate::web::events::IngestionEvent;

/// Modelo ML + NLU, inicializado em background.
///
/// Contém o orquestrador (protegido por tokio::sync::Mutex para acesso
/// exclusivo async) e a pipeline NLU (Arc para compartilhamento).
pub struct ModelReady {
    /// Orquestrador do ciclo de cultivo (acesso exclusivo via tokio::sync::Mutex).
    pub orchestrator: Mutex<Orchestrator>,
    /// Pipeline NLU compartilhada (imutável após criação).
    pub nlu: Arc<NluPipeline>,
}

/// Estado compartilhado da aplicação Axum.
#[derive(Clone)]
pub struct AppState {
    /// Modelo ML, preenchido em background via `OnceLock::set()`.
    pub model: Arc<OnceLock<ModelReady>>,
    /// Base de conhecimento compartilhada, protegida por `RwLock`.
    pub kb: Arc<RwLock<KnowledgeBase>>,
    /// Canal broadcast para eventos SSE de ingestão de PDF.
    pub events_tx: Arc<broadcast::Sender<IngestionEvent>>,
}
