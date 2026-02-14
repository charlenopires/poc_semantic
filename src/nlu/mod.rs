//! # Pipeline NLU — Compreensão de Linguagem Natural
//!
//! Este módulo orquestra todo o processamento de linguagem natural do sistema.
//! O [`NluPipeline`] é o componente central que:
//!
//! 1. **Classifica** a intenção do usuário (Confirmar, Negar, Perguntar, Narrar)
//! 2. **Extrai** entidades candidatas do texto
//! 3. **Gera embeddings** para cada entidade via LM Studio
//! 4. **Atualiza a KB** — cria ou reforça conceitos e cria links
//!
//! ## Sub-módulos
//!
//! | Módulo | Responsabilidade |
//! |--------|-----------------|
//! | [`embedder`] | Gera embeddings e chat via LM Studio API |
//! | [`extractor`] | Extrai entidades candidatas por heurísticas |
//! | [`intent`] | Classifica intenção (Confirming/Denying/Querying/Narrating) |
//! | [`question`] | Gera perguntas reflexivas para conceitos incertos |

/// Sub-módulo do embedder via LM Studio.
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
pub struct NluLinkInfo {
    /// Label do conceito de origem (Subject).
    pub source_label: String,
    /// Label do conceito de destino (Object).
    pub target_label: String,
    /// Tipo de relação (ex: "Implication", "Similarity").
    pub kind: String,
}

/// Resultado completo do processamento NLU de uma mensagem.
pub struct NluResult {
    /// Intenção classificada do usuário.
    pub intent: Intent,
    /// Labels dos conceitos novos criados nesta mensagem.
    pub new_concepts: Vec<String>,
    /// Descrições dos conceitos reforçados.
    pub reinforced_concepts: Vec<String>,
    /// Descrições dos links novos criados.
    pub new_links: Vec<String>,
    /// Mensagens informativas para o chat.
    pub messages: Vec<String>,
    /// Detalhes estruturados dos conceitos processados (para a sidebar).
    pub concept_details: Vec<NluConceptInfo>,
    /// Detalhes estruturados dos links criados (para o grafo).
    pub link_details: Vec<NluLinkInfo>,
}

/// Pipeline NLU completo — orquestra todos os componentes de processamento.
///
/// Combina:
/// - [`Embedder`] — gera embeddings e chat via LM Studio
/// - [`IntentClassifier`] — classifica a intenção do usuário
/// - [`EntityExtractor`] — extrai entidades candidatas do texto
/// - [`QuestionGenerator`] — gera perguntas reflexivas
pub struct NluPipeline {
    /// Cliente HTTP para LM Studio (embeddings + chat).
    embedder: Embedder,
    /// Classificador de intenção baseado em templates + heurísticas.
    intent_classifier: IntentClassifier,
    /// Extrator de entidades por regex + heurísticas linguísticas.
    extractor: EntityExtractor,
    /// Gerador de perguntas reflexivas para o ciclo de germinação.
    pub question_generator: QuestionGenerator,
}

impl NluPipeline {
    /// Cria um novo pipeline NLU a partir de um embedder configurado.
    ///
    /// Durante a criação, computa os embeddings dos templates de intent
    /// via uma chamada HTTP batch ao LM Studio.
    pub async fn new(embedder: Embedder) -> Result<Self> {
        let intent_classifier = IntentClassifier::new(&embedder).await?;
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
    pub fn extractor(&self) -> &EntityExtractor {
        &self.extractor
    }

    /// Gera embeddings em batch via LM Studio.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embedder.embed_batch(texts).await
    }

    /// Processa uma mensagem do usuário, atualizando a KB.
    ///
    /// ## Passos
    ///
    /// 1. **NFC Normalize** — normaliza Unicode para forma canônica
    /// 2. **Classificar intent** — Confirming, Denying, Querying, ou Narrating
    /// 3. **Extrair entidades** — identifica conceitos candidatos no texto
    /// 4. **Embeddar em batch** — gera vetores via LM Studio
    /// 5. **Atualizar KB** — cria/reforça conceitos e cria links
    pub async fn process_message(&self, text: &str, kb: &Arc<RwLock<KnowledgeBase>>) -> Result<NluResult> {
        let text: String = text.nfc().collect();

        let intent = self.intent_classifier.classify(&text, &self.embedder).await?;
        tracing::debug!(intent = ?intent, "Intent classificado");

        let entities = self.extractor.extract(&text);

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

        let embed_texts: Vec<String> = entities
            .iter()
            .map(|e| format!("search_document: {}", e))
            .collect();
        let embeddings = self.embedder.embed_batch(&embed_texts).await?;

        let mut result = self.apply_entities_to_kb(&entities, &embeddings, kb);
        result.intent = intent;

        Ok(result)
    }

    /// Aplica entidades pré-extraídas e seus embeddings à KB.
    ///
    /// Este método é separado de `process_message` para permitir reuso
    /// pelo módulo de PDF.
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

        let mut entity_concept_ids = Vec::new();
        let mut new_concept_ids_and_embeddings: Vec<(uuid::Uuid, Vec<f32>)> = Vec::new();

        // ─── Fase 1: Para cada entidade, encontrar ou criar conceito ───
        for (entity, embedding) in entities.iter().zip(embeddings.iter()) {
            let mut kb_write = kb.write();

            if let Some((existing_id, similarity)) = kb_write.find_similar_concept(embedding, 0.80)
            {
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

        // ─── Fase 2: Criar links de Implication entre entidades ───
        if entity_concept_ids.len() >= 2 {
            let mut kb_write = kb.write();
            let subject_id = entity_concept_ids[0];
            for &other_id in &entity_concept_ids[1..] {
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
        if !new_concept_ids_and_embeddings.is_empty() {
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
            intent: Intent::Narrating,
            new_concepts,
            reinforced_concepts,
            new_links,
            messages,
            concept_details,
            link_details,
        }
    }

    /// Gera embedding para busca por similaridade (modo query).
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embedder.embed(&format!("search_query: {}", text)).await
    }

    /// Classifica a intenção de um texto.
    pub async fn classify_intent(&self, text: &str) -> Result<Intent> {
        self.intent_classifier.classify(text, &self.embedder).await
    }

    /// Envia mensagem para o LLM via LM Studio e retorna a resposta.
    pub async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        self.embedder.chat(system_prompt, user_message).await
    }
}
