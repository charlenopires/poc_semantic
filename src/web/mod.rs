//! # Módulo Web — A Interface do Jardim Epistêmico
//!
//! Este módulo organiza toda a camada web da aplicação, construída
//! com **Axum** + **HTMX** + **Maud** + **SSE**.
//!
//! ## Arquitetura Web
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ Browser (HTMX + SSE + Canvas 3D)                        │
//! ├─────────────────────────────────────────────────────────┤
//! │ Axum Router (este módulo)                               │
//! │  ├── GET  /                    → index (chat principal) │
//! │  ├── GET  /metodologia         → artigo HTML estático   │
//! │  ├── GET  /visualizador        → grafo 3D + SSE feed   │
//! │  ├── GET  /status              → JSON: modelo pronto?   │
//! │  ├── GET  /events              → SSE stream (ingestão)  │
//! │  ├── POST /chat                → HTMX fragment          │
//! │  ├── POST /upload              → PDF multipart (50MB)   │
//! │  ├── GET  /knowledge/sidebar   → HTMX fragment          │
//! │  ├── GET  /knowledge/graph     → JSON (3D graph data)   │
//! │  ├── POST /knowledge/reinforce → HTMX fragment          │
//! │  └── POST /knowledge/reset     → HTMX fragment          │
//! ├─────────────────────────────────────────────────────────┤
//! │ Static Assets (tower_http::ServeDir → /assets/)         │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Submódulos
//!
//! | Módulo | Responsabilidade |
//! |--------|------------------|
//! | [`state`] | Estado compartilhado (`AppState`, `ModelReady`) |
//! | [`events`] | Enum de eventos SSE para ingestão de PDF |
//! | [`handlers`] | Handlers Axum para cada rota |
//! | [`templates`] | Templates Maud (HTML server-side) |

pub mod events;
pub mod handlers;
pub mod state;
pub mod templates;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::services::ServeDir;

use state::AppState;

/// Cria o router Axum com todas as rotas da aplicação.
///
/// ## Rotas Registradas
///
/// - **Páginas HTML**: `/`, `/metodologia`, `/visualizador`
/// - **API JSON**: `/status`, `/knowledge/graph`
/// - **HTMX fragments**: `/chat`, `/knowledge/sidebar`, `/knowledge/reinforce/{id}`, `/knowledge/reset`
/// - **SSE stream**: `/events`
/// - **Upload**: `/upload` (limite de 50MB para PDFs)
/// - **Estáticos**: `/assets/*` → diretório `assets/`
///
/// O estado `AppState` é compartilhado entre todos os handlers via
/// extrator `State<AppState>` do Axum.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // ── Páginas HTML ──────────────────────────────────────
        .route("/", get(handlers::index))
        .route("/metodologia", get(handlers::metodologia))
        .route("/visualizador", get(handlers::visualizador))
        // ── API JSON ──────────────────────────────────────────
        .route("/status", get(handlers::model_status))
        .route("/events", get(handlers::sse_events))
        // ── HTMX fragments ───────────────────────────────────
        .route("/chat", post(handlers::chat))
        .route(
            "/upload",
            post(handlers::upload_pdf).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/knowledge/sidebar", get(handlers::knowledge_sidebar))
        .route("/knowledge/graph", get(handlers::graph_data))
        .route("/knowledge/reinforce/{id}", post(handlers::reinforce_concept))
        .route("/knowledge/reset", post(handlers::reset_knowledge))
        // ── Arquivos estáticos ────────────────────────────────
        .nest_service("/assets", ServeDir::new("assets"))
        .with_state(state)
}
