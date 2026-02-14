//! # Persistência — Salvamento e Carregamento da KB em Disco
//!
//! Módulo responsável por serializar/desserializar a [`KnowledgeBase`]
//! como JSON em `data/kb.json`.
//!
//! ## Formato de Armazenamento
//!
//! A KB é salva como JSON "pretty-printed" para facilitar inspeção manual.
//! O índice `concept_links` é marcado `#[serde(skip)]` e reconstruído
//! após carregamento via [`KnowledgeBase::rebuild_index()`].
//!
//! ## Quando a KB é Salva?
//!
//! - Após cada mensagem processada pelo orquestrador
//! - Após ingestão completa de um PDF
//!
//! ## ⚠️ Atomicidade
//!
//! A escrita **não é atômica** — crash durante escrita pode corromper
//! o arquivo. Aceitável para PoC; produção usaria write-rename pattern.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::RwLock;

use crate::core::KnowledgeBase;

/// Caminho do arquivo de persistência da KB (relativo à raiz do projeto).
const KB_PATH: &str = "data/kb.json";

/// Salva a KnowledgeBase em disco como JSON pretty-printed.
///
/// Cria o diretório `data/` se não existir. Adquire um read lock
/// na KB — múltiplas leituras simultâneas são permitidas.
///
/// # Erros
///
/// Retorna erro se não conseguir criar o diretório, serializar,
/// ou escrever no arquivo.
pub fn save_kb(kb: &Arc<RwLock<KnowledgeBase>>) -> Result<()> {
    let path = Path::new(KB_PATH);
    // Garante que o diretório data/ existe
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Falha ao criar diretório data/")?;
    }
    // Adquire read lock — múltiplas leituras simultâneas são OK
    let kb_read = kb.read();
    let json = serde_json::to_string_pretty(&*kb_read)
        .context("Falha ao serializar KnowledgeBase")?;
    std::fs::write(path, json)
        .context("Falha ao escrever data/kb.json")?;
    Ok(())
}

/// Carrega a KnowledgeBase do disco, ou cria uma vazia se não existir.
///
/// Após desserializar, chama [`KnowledgeBase::rebuild_index()`]
/// para repovoar o HashMap `concept_links` (não serializado).
///
/// # Erros
///
/// Retorna erro se o arquivo existir mas estiver corrompido
/// ou incompatível com a struct atual.
pub fn load_kb() -> Result<KnowledgeBase> {
    let path = Path::new(KB_PATH);
    if !path.exists() {
        tracing::info!("Nenhum {} encontrado, iniciando KB vazia", KB_PATH);
        return Ok(KnowledgeBase::new());
    }
    let json = std::fs::read_to_string(path)
        .context("Falha ao ler data/kb.json")?;
    let mut kb: KnowledgeBase = serde_json::from_str(&json)
        .context("Falha ao desserializar data/kb.json")?;
    // Reconstrói o índice concept_links (não serializado)
    kb.rebuild_index();
    Ok(kb)
}
