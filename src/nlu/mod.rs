//! # Pipeline NLU — Compreensão de Linguagem Natural
//!
//! Este módulo orquestra todo o processamento de linguagem natural do sistema.
//! O [`NluPipeline`] é o componente central que:
//!
//! 1. **Classifica** a intenção do usuário (Confirmar, Negar, Perguntar, Narrar)
//! 2. **Extrai** entidades candidatas do texto
//! 3. **Gera embeddings** para cada entidade via BERTimbau
//! 4. **Atualiza a KB** — cria ou reforça conceitos e cria links
//!
//! ## Analogia: A Raiz que Alimenta o Jardim
//!
//! O pipeline NLU é como o **sistema radicular** — absorve nutrientes
//! (informação) do solo (linguagem do usuário) e os transforma em
//! alimento (conceitos e links) para as plantas (base de conhecimento).
//!
//! ## Fluxo de Processamento
//!
//! ```text
//! Mensagem do usuário
//!   ├── 1. NFC normalize (Unicode)
//!   ├── 2. Classificar intent (IntentClassifier)
//!   ├── 3. Extrair entidades (EntityExtractor)
//!   ├── 4. Gerar embeddings em batch (Embedder)
//!   ├── 5. Para cada entidade:
//!   │   ├── Buscar conceito similar (cosine sim > 0.80) → reforçar
//!   │   ├── Buscar conceito por label exato → reforçar
//!   │   └── Se não encontrou → criar novo conceito
//!   ├── 6. Criar links de Implication entre entidades (se ≥ 2)
//!   └── 7. Auto-links de Similarity (0.70 < sim < 0.80)
//! ```
//!
//! ## Sub-módulos
//!
//! | Módulo | Responsabilidade |
//! |--------|-----------------|
//! | [`embedder`] | Gera embeddings 768-dim via BERTimbau |
//! | [`extractor`] | Extrai entidades candidatas por heurísticas |
//! | [`intent`] | Classifica intenção (Confirming/Denying/Querying/Narrating) |
//! | [`question`] | Gera perguntas reflexivas para conceitos incertos |

/// Sub-módulo do embedder BERTimbau via candle.
pub mod embedder;

/// Sub-módulo do extrator de entidades por heurísticas.
pub mod extractor;

/// Sub-módulo do classificador de intenção do usuário.
pub mod intent;

/// Sub-módulo do gerador de perguntas reflexivas.
pub mod question;

use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

use crate::core::{Concept, KnowledgeBase, Link, LinkKind, Participant, Role, TruthValue};
use crate::core::knowledge_base::cosine_similarity;

use embedder::Embedder;
use extractor::EntityExtractor;
use intent::{Intent, IntentClassifier};
use question::QuestionGenerator;

/// Informação estruturada sobre um conceito processado pelo NLU.
///
/// Usado para reportar ao frontend quais conceitos foram criados ou reforçados,
/// permitindo atualização em tempo real da sidebar e do grafo.
pub struct NluConceptInfo {
    /// UUID do conceito (como string para serialização JSON).
    pub id: String,
    /// Label legível do conceito.
    pub label: String,
    /// `true` se o conceito foi criado agora; `false` se já existia e foi reforçado.
    pub is_new: bool,
    /// Similaridade cosseno com o conceito existente (se foi reforçado por embedding).
    pub similarity: Option<f32>,
    /// Nível de energia atual do conceito após processamento.
    pub energy: f64,
}

/// Informação estruturada sobre um link criado pelo NLU.
///
/// Usado para reportar ao frontend as novas relações detectadas.
pub struct NluLinkInfo {
    /// Label do conceito de origem (Subject).
    pub source_label: String,
    /// Label do conceito de destino (Object).
    pub target_label: String,
    /// Tipo de relação (ex: "Implication", "Similarity").
    pub kind: String,
}

/// Resultado completo do processamento NLU de uma mensagem.
///
/// Agrega todas as informações sobre o que aconteceu durante o processamento:
/// conceitos criados, reforçados, links criados, e mensagens informativas.
pub struct NluResult {
    /// Intenção classificada do usuário.
    pub intent: Intent,
    /// Labels dos conceitos novos criados nesta mensagem.
    pub new_concepts: Vec<String>,
    /// Descrições dos conceitos reforçados (existentes que foram mencionados novamente).
    pub reinforced_concepts: Vec<String>,
    /// Descrições dos links novos criados nesta mensagem.
    pub new_links: Vec<String>,
    /// Mensagens informativas para o chat (ex: "Cristalizando... Novo Concept: X").
    pub messages: Vec<String>,
    /// Detalhes estruturados dos conceitos processados (para a sidebar).
    pub concept_details: Vec<NluConceptInfo>,
    /// Detalhes estruturados dos links criados (para o grafo).
    pub link_details: Vec<NluLinkInfo>,
}

/// Pipeline NLU completo — orquestra todos os componentes de processamento.
///
/// Combina:
/// - [`Embedder`] — gera embeddings via BERTimbau
/// - [`IntentClassifier`] — classifica a intenção do usuário
/// - [`EntityExtractor`] — extrai entidades candidatas do texto
/// - [`QuestionGenerator`] — gera perguntas reflexivas
///
/// ## Concorrência
///
/// O pipeline é imutável (`&self`) após criação — thread-safe para uso
/// concorrente em múltiplas requisições. A KB é acessada via `Arc<RwLock<>>`.
pub struct NluPipeline {
    /// Modelo BERTimbau para geração de embeddings.
    embedder: Embedder,
    /// Classificador de intenção baseado em templates + heurísticas.
    intent_classifier: IntentClassifier,
    /// Extrator de entidades por regex + heurísticas linguísticas.
    extractor: EntityExtractor,
    /// Gerador de perguntas reflexivas para o ciclo de germinação.
    pub question_generator: QuestionGenerator,
}

impl NluPipeline {
    /// Cria um novo pipeline NLU a partir de um embedder carregado.
    ///
    /// Durante a criação, computa os embeddings dos templates de intent —
    /// isso significa que o pipeline está pronto para classificar intents
    /// imediatamente após a criação.
    ///
    /// # Erros
    ///
    /// Retorna erro se a classificação dos templates de intent falhar.
    pub fn new(embedder: Embedder) -> Result<Self> {
        // Cria o classificador de intent com templates pré-computados
        let intent_classifier = IntentClassifier::new(&embedder)?;
        let extractor = EntityExtractor::new();
        let question_generator = QuestionGenerator::new();

        Ok(Self {
            embedder,
            intent_classifier,
            extractor,
            question_generator,
        })
    }

    /// Acessor público para o extrator de entidades.
    ///
    /// Usado pelo módulo de PDF para extração paralela de entidades
    /// em chunks de texto sem precisar do pipeline completo.
    pub fn extractor(&self) -> &EntityExtractor {
        &self.extractor
    }

    /// Acessor público para embedding em batch.
    ///
    /// Usado pelo módulo de PDF para embeddar múltiplos chunks
    /// em uma única forward pass (mais eficiente).
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embedder.embed_batch(texts)
    }

    /// Processa uma mensagem do usuário, atualizando a KB.
    ///
    /// Este é o **método principal** do pipeline — recebe texto bruto do usuário
    /// e retorna um [`NluResult`] com todas as mudanças feitas na KB.
    ///
    /// ## Passos
    ///
    /// 1. **NFC Normalize** — normaliza Unicode para forma canônica
    /// 2. **Classificar intent** — Confirming, Denying, Querying, ou Narrating
    /// 3. **Extrair entidades** — identifica conceitos candidatos no texto
    /// 4. **Embeddar em batch** — gera vetores de 768 dim para cada entidade
    /// 5. **Atualizar KB** — cria/reforça conceitos e cria links
    ///
    /// # Parâmetros
    ///
    /// - `text` — mensagem do usuário em linguagem natural
    /// - `kb` — referência à base de conhecimento compartilhada
    ///
    /// # Retorno
    ///
    /// [`NluResult`] com intent, conceitos novos/reforçados, links criados, e mensagens
    pub fn process_message(&self, text: &str, kb: &Arc<RwLock<KnowledgeBase>>) -> Result<NluResult> {
        // Normalização Unicode NFC — garante que caracteres acentuados
        // como "ã" sejam representados de forma consistente
        let text: String = text.nfc().collect();

        // Classifica a intenção do usuário
        let intent = self.intent_classifier.classify(&text, &self.embedder)?;
        tracing::debug!(intent = ?intent, "Intent classificado");

        // Extrai entidades candidatas do texto
        let entities = self.extractor.extract(&text);

        // Se nenhuma entidade foi encontrada, retorna resultado vazio
        if entities.is_empty() {
            tracing::debug!("Nenhuma entidade extraída");
            return Ok(NluResult {
                intent,
                new_concepts: Vec::new(),
                reinforced_concepts: Vec::new(),
                new_links: Vec::new(),
                messages: Vec::new(),
                concept_details: Vec::new(),
                link_details: Vec::new(),
            });
        }

        tracing::info!(count = entities.len(), entities = ?entities, "Entidades extraídas");

        // Gera embeddings em batch (uma única forward pass no modelo)
        // Prefixo "search_document:" é convenção do BERTimbau para indexação
        let embed_texts: Vec<String> = entities
            .iter()
            .map(|e| format!("search_document: {}", e))
            .collect();
        let embeddings = self.embedder.embed_batch(&embed_texts)?;

        // Aplica entidades + embeddings à KB (cria/reforça conceitos, cria links)
        let mut result = self.apply_entities_to_kb(&entities, &embeddings, kb);
        result.intent = intent;

        Ok(result)
    }

    /// Aplica entidades pré-extraídas e seus embeddings à KB.
    ///
    /// Este método é separado de `process_message` para permitir reuso
    /// pelo módulo de PDF, que extrai entidades e gera embeddings de forma
    /// independente (usando rayon para paralelismo).
    ///
    /// ## Lógica de Matching (para cada entidade)
    ///
    /// ```text
    /// 1. Busca por EMBEDDING (cosine sim ≥ 0.80)
    ///    → Se encontrou: REFORÇA conceito existente
    ///
    /// 2. Se não encontrou por embedding, busca por LABEL (case-insensitive)
    ///    → Se encontrou: REFORÇA conceito existente
    ///
    /// 3. Se não encontrou nenhum:
    ///    → CRIA novo conceito com TruthValue::proto()
    /// ```
    ///
    /// ## Criação de Links
    ///
    /// - **Implication links**: Se ≥ 2 entidades foram extraídas, cria links
    ///   de implicação do primeiro conceito para todos os outros
    /// - **Similarity links**: Para cada conceito NOVO, compara com TODOS os
    ///   existentes. Se 0.70 < sim < 0.80, cria link de similaridade
    ///   (a faixa evita duplicar com o matching por embedding ≥ 0.80)
    pub fn apply_entities_to_kb(
        &self,
        entities: &[String],
        embeddings: &[Vec<f32>],
        kb: &Arc<RwLock<KnowledgeBase>>,
    ) -> NluResult {
        let mut new_concepts = Vec::new();
        let mut reinforced_concepts = Vec::new();
        let mut new_links = Vec::new();
        let mut messages = Vec::new();
        let mut concept_details = Vec::new();
        let mut link_details = Vec::new();

        // IDs dos conceitos correspondentes a cada entidade (para criação de links)
        let mut entity_concept_ids = Vec::new();
        // IDs e embeddings de conceitos NOVOS (para auto-links de similaridade)
        let mut new_concept_ids_and_embeddings: Vec<(uuid::Uuid, Vec<f32>)> = Vec::new();

        // ─── Fase 1: Para cada entidade, encontrar ou criar conceito ───
        for (entity, embedding) in entities.iter().zip(embeddings.iter()) {
            let mut kb_write = kb.write();

            // Tentativa 1: Busca por similaridade de embedding (threshold 0.80)
            if let Some((existing_id, similarity)) = kb_write.find_similar_concept(embedding, 0.80)
            {
                // Conceito existente encontrado por embedding — reforçar
                if let Some(concept) = kb_write.concepts.get_mut(&existing_id) {
                    concept.reinforce();
                    tracing::info!(label = %concept.label, similarity = %format!("{:.2}", similarity), "Conceito reforçado (embedding)");
                    reinforced_concepts.push(format!(
                        "{} (sim={:.2}) → energia {:.2}",
                        concept.label, similarity, concept.energy
                    ));
                    concept_details.push(NluConceptInfo {
                        id: existing_id.to_string(),
                        label: concept.label.clone(),
                        is_new: false,
                        similarity: Some(similarity),
                        energy: concept.energy,
                    });
                    entity_concept_ids.push(existing_id);
                }
            } else if let Some(existing) = kb_write.find_concept_by_label(entity).map(|c| c.id) {
                // Tentativa 2: Match exato por label (case-insensitive)
                if let Some(concept) = kb_write.concepts.get_mut(&existing) {
                    concept.reinforce();
                    tracing::info!(label = %concept.label, "Conceito reforçado (label)");
                    reinforced_concepts.push(format!("{} → reforçado", concept.label));
                    concept_details.push(NluConceptInfo {
                        id: existing.to_string(),
                        label: concept.label.clone(),
                        is_new: false,
                        similarity: None,
                        energy: concept.energy,
                    });
                    entity_concept_ids.push(existing);
                }
            } else {
                // Não encontrou — criar novo conceito
                tracing::info!(label = %entity, "Novo conceito criado");
                let mut concept = Concept::new(entity.clone(), TruthValue::proto());
                concept.embedding = Some(embedding.clone());
                let id = concept.id;
                messages.push(format!(
                    "Cristalizando... Novo Concept: {} {}",
                    entity,
                    concept.truth
                ));
                concept_details.push(NluConceptInfo {
                    id: id.to_string(),
                    label: entity.clone(),
                    is_new: true,
                    similarity: None,
                    energy: concept.energy,
                });
                new_concepts.push(entity.clone());
                kb_write.add_concept(concept);
                entity_concept_ids.push(id);
                new_concept_ids_and_embeddings.push((id, embedding.clone()));
            }
        }

        // ─── Fase 2: Criar links de Implication entre entidades (se ≥ 2) ───
        // O primeiro conceito mencionado torna-se o Subject, os demais são Objects.
        // Isso captura a estrutura narrativa: "A causa B e C"
        if entity_concept_ids.len() >= 2 {
            let mut kb_write = kb.write();
            let subject_id = entity_concept_ids[0];
            for &other_id in &entity_concept_ids[1..] {
                // Evita duplicar links existentes
                if !kb_write.link_exists(&LinkKind::Implication, subject_id, other_id) {
                    let link = Link::new(
                        LinkKind::Implication,
                        vec![
                            Participant {
                                concept_id: subject_id,
                                role: Role::Subject,
                            },
                            Participant {
                                concept_id: other_id,
                                role: Role::Object,
                            },
                        ],
                        TruthValue::proto(),
                    );
                    let desc = kb_write.describe_link(&link);

                    let source_label = kb_write
                        .concepts
                        .get(&subject_id)
                        .map(|c| c.label.clone())
                        .unwrap_or_default();
                    let target_label = kb_write
                        .concepts
                        .get(&other_id)
                        .map(|c| c.label.clone())
                        .unwrap_or_default();

                    kb_write.add_link(link);
                    tracing::info!(link = %desc, "Novo link criado");
                    link_details.push(NluLinkInfo {
                        source_label,
                        target_label,
                        kind: "Implication".to_string(),
                    });
                    new_links.push(desc);
                }
            }
        }

        // ─── Fase 3: Auto-links de Similaridade para conceitos novos ───
        // Para cada conceito NOVO, compara com todos os existentes.
        // Se 0.70 < cosine_sim < 0.80, cria link de Similarity.
        // (A faixa 0.70-0.80 evita conflito com o matching por embedding ≥ 0.80)
        if !new_concept_ids_and_embeddings.is_empty() {
            // Fase 3a: Coleta candidatos (leitura imutável da KB)
            let mut sim_candidates: Vec<(uuid::Uuid, uuid::Uuid, f32, String, String)> = Vec::new();
            {
                let kb_read = kb.read();
                for (new_id, new_emb) in &new_concept_ids_and_embeddings {
                    for (existing_id, existing_concept) in kb_read.concepts.iter() {
                        if *existing_id == *new_id {
                            continue;
                        }
                        if let Some(ref existing_emb) = existing_concept.embedding {
                            let sim = cosine_similarity(new_emb, existing_emb);
                            // Faixa de similaridade moderada: similar mas não idêntico
                            if sim > 0.70 && sim < 0.80 {
                                if !kb_read.link_exists(&LinkKind::Similarity, *new_id, *existing_id)
                                    && !kb_read.link_exists(&LinkKind::Similarity, *existing_id, *new_id)
                                {
                                    let new_label = kb_read
                                        .concepts
                                        .get(new_id)
                                        .map(|c| c.label.clone())
                                        .unwrap_or_default();
                                    let existing_label = existing_concept.label.clone();
                                    sim_candidates.push((*new_id, *existing_id, sim, new_label, existing_label));
                                }
                            }
                        }
                    }
                }
            }
            // Fase 3b: Adiciona links (escrita mutável na KB)
            if !sim_candidates.is_empty() {
                let mut kb_write = kb.write();
                for (new_id, existing_id, sim, new_label, existing_label) in sim_candidates {
                    let link = Link::new(
                        LinkKind::Similarity,
                        vec![
                            Participant {
                                concept_id: new_id,
                                role: Role::Subject,
                            },
                            Participant {
                                concept_id: existing_id,
                                role: Role::Object,
                            },
                        ],
                        TruthValue::new(sim as f64, 0.6),
                    );
                    let desc = format!("{} ≈ {} (sim={:.2})", new_label, existing_label, sim);

                    kb_write.add_link(link);
                    tracing::info!(link = %desc, "Auto-link de similaridade criado");
                    link_details.push(NluLinkInfo {
                        source_label: new_label,
                        target_label: existing_label,
                        kind: "Similarity".to_string(),
                    });
                    new_links.push(desc);
                }
            }
        }

        NluResult {
            intent: Intent::Narrating, // default — caller pode sobrescrever
            new_concepts,
            reinforced_concepts,
            new_links,
            messages,
            concept_details,
            link_details,
        }
    }

    /// Gera embedding para busca por similaridade (modo query).
    ///
    /// Usa o prefixo "search_query:" para indicar ao modelo que
    /// o texto é uma query de busca (vs. "search_document:" para indexação).
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embedder.embed(&format!("search_query: {}", text))
    }

    /// Classifica a intenção de um texto.
    ///
    /// Atalho para `self.intent_classifier.classify()`.
    pub fn classify_intent(&self, text: &str) -> Result<Intent> {
        self.intent_classifier.classify(text, &self.embedder)
    }
}
