//! # Eventos SSE de Ingestão de PDF
//!
//! Define o enum [`IngestionEvent`] — todos os eventos emitidos durante
//! o processamento de um PDF, enviados em tempo real ao frontend via
//! Server-Sent Events (SSE).
//!
//! ## Ciclo de Vida dos Eventos
//!
//! ```text
//! Started → [ChunkStarted → ConceptCreated* → ConceptReinforced*
//!            → LinkCreated* → ChunkCompleted]×N → Completed
//!                                          ou → Error
//! ```
//!
//! ## Serialização
//!
//! Usa `#[serde(tag = "type")]` para produzir JSON com discriminador:
//!
//! ```json
//! { "type": "ConceptCreated", "id": "uuid", "label": "IA" }
//! ```
//!
//! O frontend (JavaScript) faz `JSON.parse(e.data)` e usa `ev.type`
//! para decidir como renderizar cada evento.

use serde::Serialize;

/// Evento emitido durante ingestão de PDF, enviado via SSE ao frontend.
///
/// Cada variante corresponde a um momento específico do processamento.
/// O frontend usa esses eventos para atualizar a barra de progresso,
/// o feed de atividade, e o grafo 3D em tempo real.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum IngestionEvent {
    /// Ingestão iniciada — texto extraído e chunks calculados.
    ///
    /// É o primeiro evento emitido. O frontend usa para inicializar
    /// a barra de progresso com o total de chunks.
    Started {
        /// Comprimento total do texto extraído (em caracteres).
        text_len: usize,
        /// Número total de chunks após divisão.
        total_chunks: usize,
    },

    /// Início do processamento de um chunk individual.
    ///
    /// Emitido antes de processar cada chunk. O frontend atualiza
    /// a barra de progresso com `chunk/total`.
    ChunkStarted {
        /// Número do chunk atual (1-indexed).
        chunk: usize,
        /// Total de chunks.
        total: usize,
        /// Número de caracteres neste chunk.
        chars: usize,
    },

    /// Novo conceito cristalizado na KB.
    ///
    /// Emitido quando uma entidade extraída não corresponde a nenhum
    /// conceito existente (novo conceito criado). O frontend adiciona
    /// um nó ao grafo 3D em tempo real.
    ConceptCreated {
        /// UUID do novo conceito.
        id: String,
        /// Label do conceito (ex: "Inteligência Artificial").
        label: String,
        /// Frequência NARS (0.0-1.0).
        frequency: f64,
        /// Confiança NARS (0.0-1.0).
        confidence: f64,
        /// Energia do conceito (0.0-1.0).
        energy: f64,
        /// Estado CSS ("active", "dormant", "fading", "archived").
        state: String,
    },

    /// Conceito existente reforçado (entidade remapeada por similaridade).
    ///
    /// Emitido quando a entidade extraída corresponde a um conceito
    /// existente (cosine sim > threshold). O frontend pode destacar
    /// o nó já existente.
    ConceptReinforced {
        /// UUID do conceito existente.
        id: String,
        /// Label do conceito.
        label: String,
        /// Similaridade coseno com a entidade extraída (0.0-1.0).
        similarity: f32,
        /// Nova energia do conceito após reforço.
        energy: f64,
    },

    /// Novo link criado entre dois conceitos.
    ///
    /// Emitido quando uma relação (Implication, Similarity) é
    /// estabelecida. O frontend adiciona uma aresta ao grafo 3D
    /// em tempo real.
    LinkCreated {
        /// UUID do link.
        link_id: String,
        /// UUID do conceito-fonte.
        source_id: String,
        /// Label do conceito-fonte.
        source_label: String,
        /// UUID do conceito-alvo.
        target_id: String,
        /// Label do conceito-alvo.
        target_label: String,
        /// Tipo do link ("Implication", "Similarity", etc.).
        kind: String,
        /// Frequência NARS do link.
        frequency: f64,
        /// Confiança NARS do link.
        confidence: f64,
        /// Energia do link.
        energy: f64,
    },

    /// Chunk processado completamente.
    ///
    /// Emitido após todas as entidades de um chunk serem processadas.
    /// O frontend pode atualizar o contador de progresso.
    ChunkCompleted {
        /// Número do chunk concluído.
        chunk: usize,
        /// Total de chunks.
        total: usize,
        /// Conceitos novos neste chunk.
        new_concepts: usize,
        /// Links novos neste chunk.
        new_links: usize,
    },

    /// Ingestão completa — sumário final com métricas do sistema.
    ///
    /// É o evento final (exceto Error). Contém estatísticas completas
    /// da ingestão e métricas de hardware para exibição no frontend.
    Completed {
        /// Total de chunks processados.
        total_chunks: usize,
        /// Total de novos conceitos criados.
        new_concepts: usize,
        /// Total de novos links criados.
        new_links: usize,
        /// Total de conceitos na KB após ingestão.
        kb_concepts: usize,
        /// Total de links na KB após ingestão.
        kb_links: usize,
        /// Tempo de extração de texto do PDF (ms).
        extract_ms: u64,
        /// Tempo de processamento NLU + KB (ms).
        ingestion_ms: u64,
        /// Tempo total de ingestão (ms).
        total_ms: u64,
        // ── Métricas do sistema ──
        /// RAM usada pelo processo (MB).
        memory_used_mb: f64,
        /// RAM total do sistema (MB).
        memory_total_mb: f64,
        /// Cores CPU ativos (uso > 1%).
        cpu_active_cores: usize,
        /// Pico de uso de CPU por core (%).
        cpu_max_core_percent: f32,
        /// Total de cores lógicos.
        cpu_total_cores: usize,
        /// Tamanho do arquivo `data/kb.json` em bytes.
        kb_file_size_bytes: u64,
        /// Nome da GPU (ex: "Apple M1 Pro").
        gpu_name: String,
        /// Cores GPU.
        gpu_cores: u32,
        /// Utilização GPU (%).
        gpu_utilization_pct: u32,
        /// Memória GPU em uso (MB).
        gpu_memory_mb: f64,
        /// Throughput do processamento (ex: "1500 chars/s").
        throughput: String,
    },

    /// Erro durante a ingestão.
    ///
    /// Pode ocorrer em qualquer fase. O frontend exibe como alerta
    /// e encerra o rastreamento de progresso.
    Error {
        /// Mensagem de erro legível (ex: "PDF vazio ou sem texto").
        message: String,
    },
}
