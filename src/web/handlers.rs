//! # Handlers HTTP — Os Endpoints da Aplicação
//!
//! Cada função pública neste módulo é um handler Axum, mapeado a uma
//! rota em [`super::create_router()`]. Os handlers seguem o padrão
//! **HTMX fragment** — retornam fragmentos HTML (não páginas completas)
//! que o HTMX injeta no DOM via `hx-swap`.
//!
//! ## Padrão de Resposta
//!
//! | Handler | Método | Retorno | Uso |
//! |---------|--------|---------|-----|
//! | `index` | GET | HTML completo | Página principal (Maud) |
//! | `metodologia` | GET | HTML estático | Artigo embutido |
//! | `visualizador` | GET | HTML completo | Página visualizador |
//! | `model_status` | GET | JSON | Polling de readiness |
//! | `sse_events` | GET | SSE stream | Eventos de ingestão |
//! | `chat` | POST | HTMX fragment | Fragmento de mensagem |
//! | `upload_pdf` | POST | HTMX fragment | Confirmação de upload |
//! | `knowledge_sidebar` | GET | HTMX fragment | Conteúdo da sidebar |
//! | `graph_data` | GET | JSON | Dados do grafo 3D |
//! | `reinforce_concept` | POST | HTMX fragment | Feedback de reforço |
//! | `reset_knowledge` | POST | HTMX fragment | Confirmação de reset |
//!
//! ## Guarda de Model Ready
//!
//! Handlers que dependem do modelo ML verificam `state.model.get()`:
//! - Se `Some(model)` → processa normalmente
//! - Se `None` → retorna mensagem "⏳ Modelo carregando..."

use std::convert::Infallible;
use std::time::{Duration, Instant};

use axum::extract::{Multipart, Path, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::Html;
use axum::Json;
use futures_util::stream::StreamExt;
use maud::html;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use super::state::AppState;
use super::templates;
use crate::orchestrator::MessageRole;
use crate::pdf;
use crate::web::events::IngestionEvent;

/// Resposta do endpoint `/status` — indica se o modelo ML está pronto.
#[derive(serde::Serialize)]
pub struct StatusResponse {
    /// `true` quando BERTimbau terminou de carregar e o orquestrador está pronto.
    pub ready: bool,
}

// ─── Tipos para o endpoint /knowledge/graph ──────────────────────

/// Dados completos do grafo para renderização 3D no frontend.
///
/// Serializado como JSON contendo listas de conceitos (nós) e
/// links (arestas). O frontend (graph3d.js) usa esses dados
/// para renderizar o grafo força-direcionado em Canvas.
#[derive(serde::Serialize)]
pub struct GraphData {
    /// Conceitos como nós do grafo.
    pub concepts: Vec<GraphConcept>,
    /// Links como arestas do grafo.
    pub links: Vec<GraphLink>,
}

/// Conceito serializado para o grafo 3D.
///
/// Contém todas as informações que o frontend precisa para
/// renderizar e estilizar cada nó.
#[derive(serde::Serialize)]
pub struct GraphConcept {
    /// UUID do conceito.
    pub id: String,
    /// Label textual.
    pub label: String,
    /// Frequência NARS (0.0-1.0) — mapeia para cor do nó.
    pub frequency: f64,
    /// Confiança NARS (0.0-1.0) — mapeia para opacidade.
    pub confidence: f64,
    /// Energia (0.0-1.0) — mapeia para tamanho do nó.
    pub energy: f64,
    /// Estado CSS ("active", "fading", "archived").
    pub state: String,
    /// Número de menções (exibido como tooltip).
    pub mention_count: u32,
}

/// Link serializado para o grafo 3D.
///
/// As arestas conectam conceitos por seus UUIDs.
#[derive(serde::Serialize)]
pub struct GraphLink {
    /// UUID do link.
    pub id: String,
    /// UUID do conceito-fonte.
    pub source: String,
    /// UUID do conceito-alvo.
    pub target: String,
    /// Tipo do link ("Inheritance", "Implication", "Similarity", etc.).
    pub kind: String,
    /// Frequência NARS do link — mapeia para cor da aresta.
    pub frequency: f64,
    /// Confiança NARS do link — mapeia para opacidade da aresta.
    pub confidence: f64,
    /// Energia do link — mapeia para espessura da aresta.
    pub energy: f64,
}

/// Converte Maud Markup em resposta Html<String> do Axum.
fn markup_to_html(m: maud::Markup) -> Html<String> {
    Html(m.into_string())
}

/// Resposta padrão quando o modelo ainda está carregando.
///
/// Retorna um HTMX fragment com mensagem de loading que o frontend
/// exibe enquanto BERTimbau é inicializado em background (~10s).
fn loading_response() -> Html<String> {
    markup_to_html(html! {
        div class="message system-message loading" {
            div class="message-role" { "Sistema" }
            div class="message-content" {
                "⏳ Modelo carregando, aguarde alguns segundos..."
            }
        }
    })
}

/// GET `/` — Página principal do chat.
///
/// Renderiza a página completa usando [`templates::full_page()`].
/// Inclui o layout com chat, sidebar, grafo 3D, e scripts.
pub async fn index() -> Html<String> {
    markup_to_html(templates::full_page())
}

/// GET `/metodologia` — Artigo sobre a metodologia epistêmica.
///
/// Serve o arquivo HTML estático `epistemic_cultivation.html`
/// embutido no binário via `include_str!()`.
pub async fn metodologia() -> Html<String> {
    let content = include_str!("../../epistemic_cultivation.html");
    Html(content.to_string())
}

/// GET `/visualizador` — Página de visualização em tempo real.
///
/// Renderiza a página do visualizador com grafo 3D full-screen
/// e feed de atividade SSE. Usa [`templates::visualizador_page()`].
pub async fn visualizador() -> Html<String> {
    markup_to_html(templates::visualizador_page())
}

/// GET `/status` — Verifica se o modelo ML está pronto.
///
/// Retorna JSON `{ "ready": true/false }`.
/// O frontend faz polling deste endpoint a cada 3s durante o loading.
pub async fn model_status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        ready: state.model.get().is_some(),
    })
}

/// GET `/events` — Stream SSE de eventos de ingestão de PDF.
///
/// Cria um subscriber no canal broadcast e converte cada
/// [`IngestionEvent`] em um `SseEvent` com JSON serializado.
///
/// ## Keep-Alive
///
/// Envia keep-alive a cada 15s para manter a conexão viva
/// (proxies HTTP frequentemente fecham conexões idle).
///
/// ## Lagged Messages
///
/// Se o subscriber ficar para trás (buffer cheio), mensagens
/// são silenciosamente descartadas (filter_map retorna None).
pub async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.events_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(event) => {
                // Serializa o evento como JSON
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok(SseEvent::default().data(data)))
            }
            Err(_) => None, // mensagens atrasadas são descartadas
        }
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// POST `/chat` — Processa mensagem de chat e retorna HTMX fragment.
///
/// ## Fluxo
///
/// ```text
/// 1. Lê o campo "message" do form
/// 2. Verifica se modelo está pronto (senão: loading response)
/// 3. Adquire lock do Orchestrator
/// 4. Chama orchestrator.process_message() → Vec<ChatMessage>
/// 5. Persiste KB em disco
/// 6. Coleta métricas do sistema
/// 7. Renderiza fragmento HTML com mensagem do usuário + respostas
/// ```
///
/// O HTMX injeta o fragment retornado antes do fim de `#chat-messages`
/// (via `hx-swap="beforeend"`).
pub async fn chat(
    State(state): State<AppState>,
    axum::Form(form): axum::Form<ChatForm>,
) -> Html<String> {
    let user_text = form.message.trim().to_string();
    if user_text.is_empty() {
        return markup_to_html(html! {});
    }

    // Guarda de model ready
    let Some(model) = state.model.get() else {
        return markup_to_html(html! {
            div class="message user-message" {
                div class="message-role" { "Você" }
                div class="message-content" { (user_text) }
            }
            div class="message system-message loading" {
                div class="message-role" { "Sistema" }
                div class="message-content" {
                    "⏳ Modelo carregando, aguarde alguns segundos..."
                }
            }
        });
    };

    // Processa mensagem via Orchestrator (adquire Mutex)
    let t0 = Instant::now();
    let mut orchestrator = model.orchestrator.lock();
    let responses = orchestrator.process_message(&user_text);
    drop(orchestrator); // libera Mutex o mais rápido possível
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    // Persiste KB em disco após cada mensagem
    if let Err(e) = crate::persistence::save_kb(&state.kb) {
        tracing::error!(error = %e, "Falha ao salvar KB após chat");
    }

    // Coleta métricas (CPU, RAM, GPU, etc.)
    let pm = crate::metrics::collect_metrics(None);
    let metrics_line = pm.summary_line(elapsed_ms);

    // Renderiza fragmento HTML
    markup_to_html(match responses {
        Ok(messages) => {
            html! {
                // Mensagem do usuário (exibida à direita)
                div class="message user-message" {
                    div class="message-role" { "Você" }
                    div class="message-content" { (user_text) }
                }
                // Respostas do sistema (cada uma com sua role/estilo)
                @for msg in &messages {
                    div class=(format!("message system-message {}", msg.role.css_class())) {
                        div class="message-role" { (msg.role.label()) }
                        div class="message-content" { (msg.content) }
                    }
                }
                // Linha de métricas do sistema
                div class="message system-message metrics" {
                    div class="message-content metrics-line" {
                        (format!("\u{26a1} {}", metrics_line))
                    }
                }
            }
        }
        Err(e) => {
            html! {
                div class="message user-message" {
                    div class="message-role" { "Você" }
                    div class="message-content" { (user_text) }
                }
                div class="message system-message error" {
                    div class="message-role" { "Erro" }
                    div class="message-content" { (format!("Erro: {}", e)) }
                }
            }
        }
    })
}

/// Dados do formulário de chat (campo `message` do form HTML).
#[derive(serde::Deserialize)]
pub struct ChatForm {
    /// Texto da mensagem do usuário.
    pub message: String,
}

/// POST `/upload` — Upload de PDF para ingestão em background.
///
/// ## Fluxo
///
/// ```text
/// 1. Lê campo "pdf" do multipart form
/// 2. Verifica se modelo está pronto
/// 3. Spawna task blocking para processar PDF em background
/// 4. Retorna IMEDIATAMENTE com confirmação de recebimento
/// 5. Background: ingest_pdf() emite SSE events durante processamento
/// ```
///
/// ## Processamento em Background
///
/// O processamento é feito em `spawn_blocking` porque:
/// - Forward pass do BERTimbau é CPU-bound
/// - Não queremos bloquear o executor async do Tokio
/// - O usuário vê o progresso via SSE no Visualizador
pub async fn upload_pdf(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Html<String> {
    let Some(model) = state.model.get() else {
        return loading_response();
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "pdf" {
            let filename = field
                .file_name()
                .unwrap_or("documento.pdf")
                .to_string();

            match field.bytes().await {
                Ok(bytes) => {
                    tracing::info!(size_bytes = bytes.len(), filename = %filename, "PDF upload recebido");

                    // Clona recursos para a task em background
                    let nlu = model.nlu.clone();
                    let kb = state.kb.clone();
                    let tx = state.events_tx.clone();

                    // Processa em background (CPU-bound: BERTimbau forward pass)
                    tokio::task::spawn_blocking(move || {
                        match pdf::ingest_pdf(&bytes, &nlu, &kb, &tx) {
                            Ok(msg) => {
                                tracing::info!(result = %msg, "PDF background ingestion complete");
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "PDF background ingestion failed");
                                let _ = tx.send(IngestionEvent::Error {
                                    message: format!("Erro ao processar PDF: {}", e),
                                });
                            }
                        }
                    });

                    // Retorna imediatamente — progresso via SSE
                    return markup_to_html(html! {
                        div class="message system-message pdf-result" {
                            div class="message-role" { "PDF Ingestão" }
                            div class="message-content" {
                                "📄 Upload de " strong { (filename) } " recebido. "
                                "Processamento iniciado em background. "
                                a href="/visualizador" target="_blank" {
                                    "Acompanhe em tempo real no Visualizador →"
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Falha ao ler bytes do PDF");
                    return markup_to_html(html! {
                        div class="message system-message error" {
                            div class="message-role" { "Erro" }
                            div class="message-content" { (format!("Erro no upload: {}", e)) }
                        }
                    });
                }
            }
        }
    }

    tracing::warn!("Nenhum campo PDF encontrado no upload multipart");
    markup_to_html(html! {
        div class="message system-message error" {
            div class="message-role" { "Erro" }
            div class="message-content" { "Nenhum arquivo PDF encontrado no upload." }
        }
    })
}

/// POST `/knowledge/reset` — Limpa toda a KB e reseta o orquestrador.
///
/// Executa reset completo:
/// 1. Limpa todos os conceitos e links da KB
/// 2. Persiste KB vazia em disco
/// 3. Reseta estado do orquestrador (contadores, filas)
pub async fn reset_knowledge(State(state): State<AppState>) -> Html<String> {
    // Limpa KB completamente
    state.kb.write().clear();

    // Persiste KB vazia em disco
    if let Err(e) = crate::persistence::save_kb(&state.kb) {
        tracing::error!(error = %e, "Falha ao salvar KB vazia após reset");
    }

    // Reseta estado do orquestrador
    if let Some(model) = state.model.get() {
        model.orchestrator.lock().reset();
    }

    tracing::info!("KB resetada pelo usuário");

    markup_to_html(html! {
        div class="message system-message" {
            div class="message-role" { "Sistema" }
            div class="message-content" {
                "Base de conhecimento resetada. Todos os conceitos e links foram removidos."
            }
        }
    })
}

/// GET `/knowledge/sidebar` — Fragment HTMX da sidebar de conhecimento.
///
/// Atualizada via polling a cada 10s (definido no template via
/// `hx-trigger="load, every 10s"`). Retorna a lista de conceitos
/// ativos e esmaecendo, renderizada por [`templates::sidebar_content()`].
pub async fn knowledge_sidebar(State(state): State<AppState>) -> Html<String> {
    let kb = state.kb.read();
    markup_to_html(templates::sidebar_content(&kb))
}

/// POST `/knowledge/reinforce/{id}` — Reforça um conceito via sidebar.
///
/// Recebe o UUID do conceito na URL path e delega ao orquestrador.
/// Persiste KB após reforço.
pub async fn reinforce_concept(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Html<String> {
    // Valida UUID
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return markup_to_html(html! {
                div class="message system-message error" {
                    "ID inválido"
                }
            });
        }
    };

    let Some(model) = state.model.get() else {
        return loading_response();
    };

    // Reforça via orquestrador
    let orchestrator = model.orchestrator.lock();
    let result = orchestrator.reinforce_concept(uuid);
    drop(orchestrator);

    // Persiste KB após reforço
    if let Err(e) = crate::persistence::save_kb(&state.kb) {
        tracing::error!(error = %e, "Falha ao salvar KB após reinforce");
    }

    markup_to_html(match result {
        Some(msg) => html! {
            div class="message system-message reinforced" {
                div class="message-role" { "Reforço" }
                div class="message-content" { (msg) }
            }
        },
        None => html! {
            div class="message system-message error" {
                "Conceito não encontrado"
            }
        },
    })
}

/// GET `/knowledge/graph` — Dados JSON do grafo para visualização 3D.
///
/// Retorna todos os conceitos (nós) e links (arestas) da KB
/// como JSON para o renderer Canvas (graph3d.js).
///
/// Cada conceito inclui frequency, confidence, energy para que
/// o frontend mapeie propriedades visuais (cor, tamanho, opacidade).
pub async fn graph_data(State(state): State<AppState>) -> Json<GraphData> {
    let kb = state.kb.read();

    let concepts: Vec<GraphConcept> = kb
        .concepts
        .values()
        .map(|c| GraphConcept {
            id: c.id.to_string(),
            label: c.label.clone(),
            frequency: c.truth.frequency(),
            confidence: c.truth.confidence(),
            energy: c.energy,
            state: c.state.css_class().to_string(),
            mention_count: c.mention_count,
        })
        .collect();

    let links: Vec<GraphLink> = kb
        .links
        .values()
        .filter_map(|l| {
            let source = l.subject()?;
            let target = l.object()?;
            Some(GraphLink {
                id: l.id.to_string(),
                source: source.to_string(),
                target: target.to_string(),
                kind: l.kind.label().to_string(),
                frequency: l.truth.frequency(),
                confidence: l.truth.confidence(),
                energy: l.energy,
            })
        })
        .collect();

    Json(GraphData { concepts, links })
}

// ─── Extensões de MessageRole para HTML ──────────────────────────

impl MessageRole {
    /// Classe CSS para estilização da mensagem no frontend.
    ///
    /// Mapeia cada role a uma classe CSS que define cor, ícone, e estilo:
    /// - `user` → fundo claro, alinhado à direita
    /// - `system` → fundo escuro, ícone de engrenagem
    /// - `inference` → fundo azulado, ícone 🧪
    /// - `question` → fundo verde, ícone 🌱
    /// - `alert` → fundo alaranjado, ícone ⚠️
    pub fn css_class(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::System => "system",
            MessageRole::Inference => "inference",
            MessageRole::Question => "question",
            MessageRole::Alert => "alert",
        }
    }

    /// Label textual da role para exibição no chat.
    pub fn label(&self) -> &'static str {
        match self {
            MessageRole::User => "Você",
            MessageRole::System => "Sistema",
            MessageRole::Inference => "Inferência",
            MessageRole::Question => "Germinação",
            MessageRole::Alert => "Alerta",
        }
    }
}
