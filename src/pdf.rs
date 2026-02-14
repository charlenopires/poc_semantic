//! # Ingestão de PDF — Da Página à Base de Conhecimento
//!
//! Este módulo processa documentos PDF e alimenta a KB com o conhecimento
//! extraído. É usado pela interface web para upload de documentos.
//!
//! ## Pipeline de Ingestão
//!
//! ```text
//! Upload PDF (bytes)
//!   ├── 1. Extrair texto → pdf_extract (spawn_blocking, CPU-bound)
//!   ├── 2. Normalizar texto PT-BR → NFC + regex cleanup
//!   ├── 3. Chunkar texto (~500 chars por chunk)
//!   ├── 4. Extrair entidades de todos os chunks → EntityExtractor
//!   ├── 5. Embeddar TODAS as entidades em batch → LM Studio API (async)
//!   ├── 6. Aplicar na KB chunk por chunk → NluPipeline
//!   └── 7. Salvar KB em disco → persistence::save_kb()
//! ```

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use parking_lot::RwLock;
use regex::Regex;
use tokio::sync::broadcast;
use unicode_normalization::UnicodeNormalization;

use crate::core::KnowledgeBase;
use crate::nlu::NluPipeline;
use crate::web::events::IngestionEvent;

/// Normaliza texto extraído de PDF para Português Brasileiro.
pub fn normalize_pdf_text(text: &str) -> String {
    let normalized: String = text.nfc().collect();
    let re = Regex::new(r"(\w+)\s+(ção|ções|cia|ência|ância|mente|dade|ável|ível)")
        .expect("invalid regex");
    re.replace_all(&normalized, "$1$2").into_owned()
}

/// Processa bytes de um PDF: extrai texto, chunka, e alimenta a KB via NLU.
///
/// Emite eventos SSE via broadcast channel durante todo o processamento.
///
/// ## Fases de Processamento
///
/// | Fase | Operação | Custo |
/// |------|----------|-------|
/// | 1 | Extração de texto (spawn_blocking) | ~100ms |
/// | 2 | Extração de entidades (regex) | ~10ms |
/// | 3 | Batch embedding (LM Studio API) | ~500ms |
/// | 4 | Aplicação na KB | ~100ms |
pub async fn ingest_pdf(
    bytes: &[u8],
    nlu: &NluPipeline,
    kb: &Arc<RwLock<KnowledgeBase>>,
    tx: &broadcast::Sender<IngestionEvent>,
) -> Result<String> {
    let span = tracing::info_span!("pdf_ingestion");
    let _guard = span.enter();

    let t_total = Instant::now();

    // ─── Fase 1: Extração de texto (CPU-bound, em spawn_blocking) ──
    let t_extract = Instant::now();
    let bytes_owned = bytes.to_vec();
    let raw_text = tokio::task::spawn_blocking(move || {
        pdf_extract::extract_text_from_mem(&bytes_owned)
    })
    .await
    .context("spawn_blocking panicked")?
    .context("Failed to extract text from PDF")?;
    let text = normalize_pdf_text(&raw_text);
    let extract_ms = t_extract.elapsed().as_millis() as u64;

    tracing::info!(text_len = text.len(), extract_ms, "Texto extraído e normalizado do PDF");

    if text.trim().is_empty() {
        tracing::warn!("PDF sem texto extraível");
        let _ = tx.send(IngestionEvent::Error {
            message: "PDF vazio ou sem texto extraível.".into(),
        });
        return Ok("PDF vazio ou sem texto extraível.".into());
    }

    let chunks = chunk_text(&text, 500);
    let total_chunks = chunks.len();
    tracing::info!(total_chunks, "Texto dividido em chunks");

    let _ = tx.send(IngestionEvent::Started {
        text_len: text.len(),
        total_chunks,
    });

    // ─── Fase 2: Extração de entidades (rápido, só regex) ────────
    let t_ingestion = Instant::now();

    let chunk_entities: Vec<(usize, usize, Vec<String>)> = chunks
        .iter()
        .enumerate()
        .filter(|(_, chunk)| !chunk.trim().is_empty())
        .map(|(i, chunk)| {
            let entities = nlu.extractor().extract(chunk);
            (i, chunk.len(), entities)
        })
        .collect();

    // ─── Fase 3: Batch embedding de TODAS as entidades via LM Studio ──
    let all_entity_texts: Vec<String> = chunk_entities
        .iter()
        .flat_map(|(_, _, entities)| entities.iter().map(|e| format!("search_document: {}", e)))
        .collect();

    let total_entities = all_entity_texts.len();
    tracing::info!(total_entities, "Embedding de todas as entidades em batch único...");

    let all_embeddings = nlu.embed_batch(&all_entity_texts).await?;

    tracing::info!(total_entities, "Embeddings computados");

    // ─── Fase 4: Aplicação na KB chunk por chunk ─────────────────
    let mut total_new_concepts = 0usize;
    let mut total_new_links = 0usize;
    let mut chunks_processed = 0usize;
    let mut embedding_offset = 0usize;

    for (i, chunk_len, entities) in &chunk_entities {
        let chunk_num = i + 1;
        let count = entities.len();
        let embeddings = &all_embeddings[embedding_offset..embedding_offset + count];
        embedding_offset += count;

        tracing::info!(chunk = chunk_num, total = total_chunks, chars = chunk_len, entities = count, "Processando chunk");

        let _ = tx.send(IngestionEvent::ChunkStarted {
            chunk: chunk_num,
            total: total_chunks,
            chars: *chunk_len,
        });

        let result = nlu.apply_entities_to_kb(entities, embeddings, kb);

        tracing::info!(
            novos = result.new_concepts.len(),
            reforçados = result.reinforced_concepts.len(),
            links = result.new_links.len(),
            "Chunk processado"
        );

        for info in &result.concept_details {
            if info.is_new {
                let _ = tx.send(IngestionEvent::ConceptCreated {
                    id: info.id.clone(),
                    label: info.label.clone(),
                });
            } else {
                let _ = tx.send(IngestionEvent::ConceptReinforced {
                    id: info.id.clone(),
                    label: info.label.clone(),
                    similarity: info.similarity.unwrap_or(1.0),
                    energy: info.energy,
                });
            }
        }

        for info in &result.link_details {
            let _ = tx.send(IngestionEvent::LinkCreated {
                source_label: info.source_label.clone(),
                target_label: info.target_label.clone(),
                kind: info.kind.clone(),
            });
        }

        let chunk_new_concepts = result.new_concepts.len();
        let chunk_new_links = result.new_links.len();
        total_new_concepts += chunk_new_concepts;
        total_new_links += chunk_new_links;
        chunks_processed += 1;

        let _ = tx.send(IngestionEvent::ChunkCompleted {
            chunk: chunk_num,
            total: total_chunks,
            new_concepts: chunk_new_concepts,
            new_links: chunk_new_links,
        });

        // Cede controle ao runtime tokio entre chunks para que o consumidor SSE
        // possa drenar e enviar eventos ao cliente HTTP em tempo real.
        tokio::task::yield_now().await;
    }

    let ingestion_ms = t_ingestion.elapsed().as_millis() as u64;
    let total_ms = t_total.elapsed().as_millis() as u64;

    let kb_read = kb.read();
    let kb_concepts = kb_read.concept_count();
    let kb_links = kb_read.link_count();

    tracing::info!(
        chunks_processed,
        new_concepts = total_new_concepts,
        new_links = total_new_links,
        kb_concepts,
        kb_links,
        extract_ms,
        ingestion_ms,
        total_ms,
        "Ingestão PDF completa"
    );

    // ─── Persistência em disco ───────────────────────────────────
    match crate::persistence::save_kb(kb) {
        Ok(()) => tracing::info!("KB salva em disco após ingestão PDF"),
        Err(e) => tracing::error!(error = %e, "Falha ao salvar KB após ingestão PDF"),
    }

    // ─── Métricas do sistema ─────────────────────────────────────
    let throughput_str = if total_ms > 0 {
        format!("{:.0} chars/s", text.len() as f64 / (total_ms as f64 / 1000.0))
    } else {
        "N/A".into()
    };
    let pm = crate::metrics::collect_metrics(Some(throughput_str.clone()));

    let _ = tx.send(IngestionEvent::Completed {
        total_chunks,
        new_concepts: total_new_concepts,
        new_links: total_new_links,
        kb_concepts,
        kb_links,
        extract_ms,
        ingestion_ms,
        total_ms,
        memory_used_mb: pm.memory_used_mb,
        memory_total_mb: pm.memory_total_mb,
        cpu_active_cores: pm.cpu_active_cores,
        cpu_max_core_percent: pm.cpu_max_core_percent,
        cpu_total_cores: pm.cpu_total_cores,
        kb_file_size_bytes: pm.kb_file_size_bytes,
        gpu_name: pm.gpu_name.clone(),
        gpu_cores: pm.gpu_cores,
        gpu_utilization_pct: pm.gpu_utilization_pct,
        gpu_memory_mb: pm.gpu_memory_mb,
        throughput: throughput_str.clone(),
    });

    Ok(format!(
        "PDF processado: {} chunks analisados. {} concepts e {} links criados. KB total: {} concepts, {} links. Tempo: leitura {}ms, ingestão {}ms, total {}ms. | {}",
        total_chunks,
        total_new_concepts,
        total_new_links,
        kb_concepts,
        kb_links,
        extract_ms,
        ingestion_ms,
        total_ms,
        pm.summary_line(total_ms),
    ))
}

/// Divide texto em chunks de ~`max_chars` caracteres, respeitando parágrafos e sentenças.
fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        if current.len() + paragraph.len() + 1 > max_chars && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(paragraph);

        if current.len() > max_chars {
            let sentences: Vec<&str> = current.split(". ").collect();
            let mut buf = String::new();
            for sentence in sentences {
                if buf.len() + sentence.len() + 2 > max_chars && !buf.is_empty() {
                    chunks.push(buf.clone());
                    buf.clear();
                }
                if !buf.is_empty() {
                    buf.push_str(". ");
                }
                buf.push_str(sentence);
            }
            current = buf;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    tracing::debug!(chunks = chunks.len(), "Chunking concluído");
    chunks
}
