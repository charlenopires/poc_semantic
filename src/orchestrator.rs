//! # Orquestrador — O Jardineiro Epistêmico
//!
//! O [`Orchestrator`] é o **coração do sistema** — rege o ciclo completo
//! de cultivo epistêmico, coordenando todos os subsistemas em resposta
//! às mensagens do usuário.
//!
//! ## O Ciclo de Cultivo Epistêmico
//!
//! ```text
//! Mensagem do Usuário
//!   │
//!   ├── 1. SEMEADURA (Intent::Narrating)
//!   │   └── NLU processa → extrai entidades → cria/reforça conceitos
//!   │   └── LLM gera resposta natural baseada nos conceitos
//!   │
//!   ├── 2. FOTOSSINTESE (Inferência)
//!   │   └── InferenceEngine deduz/induz novos links
//!   │
//!   ├── 3. GERMINACAO (Perguntas reflexivas a cada ~2 turnos)
//!   │   └── QuestionGenerator cria perguntas para conceitos incertos
//!   │
//!   ├── 4. CONFIRMACAO/NEGACAO (Intent::Confirming/Denying)
//!   │   └── Ajusta TruthValues dos conceitos recentes
//!   │
//!   └── 5. PODA (Decay a cada ~10 turnos)
//!       └── Conceitos inativos perdem energia
//! ```

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use parking_lot::RwLock;

use crate::core::concept::ConceptId;
use crate::core::{KnowledgeBase, TruthValue};
use crate::inference::InferenceEngine;
use crate::nlu::intent::Intent;
use crate::nlu::NluPipeline;

/// Mensagem no chat — o resultado de cada processamento pelo orquestrador.
pub struct ChatMessage {
    /// Role semântica da mensagem.
    pub role: MessageRole,
    /// Conteúdo textual em PT-BR, pronto para exibição.
    pub content: String,
}

/// Role semântica das mensagens do sistema.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    /// Mensagem do usuário.
    User,
    /// Mensagem do sistema reportando operações na KB.
    System,
    /// Resultado de uma inferência NARS.
    Inference,
    /// Pergunta reflexiva gerada pelo sistema.
    Question,
    /// Alerta sobre conceitos em degradação.
    Alert,
    /// Resposta gerada pelo LLM.
    Assistant,
}

/// Orquestrador do ciclo de cultivo epistêmico.
pub struct Orchestrator {
    /// Pipeline NLU para processamento de linguagem natural.
    nlu: Arc<NluPipeline>,
    /// Base de conhecimento compartilhada.
    kb: Arc<RwLock<KnowledgeBase>>,
    /// IDs dos conceitos discutidos no último turno.
    last_discussed: Vec<ConceptId>,
    /// Fila FIFO de perguntas pendentes.
    pending_questions: VecDeque<String>,
    /// Turnos desde a última pergunta reflexiva.
    turns_since_question: u32,
    /// Total de turnos na conversa atual.
    total_turns: u32,
    /// Turnos desde o último ciclo de poda.
    turns_since_decay: u32,
}

impl Orchestrator {
    /// Cria um novo orquestrador com estado zerado.
    pub fn new(nlu: Arc<NluPipeline>, kb: Arc<RwLock<KnowledgeBase>>) -> Self {
        Self {
            nlu,
            kb,
            last_discussed: Vec::new(),
            pending_questions: VecDeque::new(),
            turns_since_question: 0,
            total_turns: 0,
            turns_since_decay: 0,
        }
    }

    /// Processa uma mensagem do usuário e retorna as respostas do sistema.
    pub async fn process_message(&mut self, user_text: &str) -> Result<Vec<ChatMessage>> {
        let mut responses = Vec::new();
        self.total_turns += 1;
        self.turns_since_question += 1;
        self.turns_since_decay += 1;

        let intent = self.nlu.classify_intent(user_text).await?;

        match intent {
            Intent::Confirming => {
                responses.extend(self.handle_confirmation(true));
            }
            Intent::Denying => {
                responses.extend(self.handle_confirmation(false));
            }
            Intent::Querying => {
                responses.extend(self.handle_query(user_text).await?);
            }
            Intent::Narrating => {
                responses.extend(self.handle_narration(user_text).await?);
            }
        }

        // Fotossintese — inferência após narração
        if intent == Intent::Narrating {
            responses.extend(self.run_inference());
        }

        // Germinacao — perguntas reflexivas a cada ~2 turnos
        if self.turns_since_question >= 2 {
            if let Some(question) = self.generate_question() {
                responses.push(ChatMessage {
                    role: MessageRole::Question,
                    content: question,
                });
                self.turns_since_question = 0;
            }
        }

        // Poda — decay a cada ~10 turnos
        if self.turns_since_decay >= 10 {
            responses.extend(self.run_decay());
            self.turns_since_decay = 0;
        }

        Ok(responses)
    }

    /// Processa uma mensagem narrativa (informativa).
    async fn handle_narration(&mut self, text: &str) -> Result<Vec<ChatMessage>> {
        let mut messages = Vec::new();

        // Processa via NLU — cria/reforça conceitos, cria links
        let result = self.nlu.process_message(text, &self.kb).await?;

        // Reporta conceitos cristalizados (novos)
        for msg in &result.messages {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: msg.clone(),
            });
        }

        // Reporta conceitos reforçados
        for concept_name in &result.reinforced_concepts {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!("Reforçando: {}", concept_name),
            });
        }

        // Reporta novos links
        for link_desc in &result.new_links {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!("Novo Link: {}", link_desc),
            });
        }

        // Atualiza last_discussed (scoped lock — guard dropped before await)
        {
            let kb_read = self.kb.read();
            let mut discussed = Vec::new();
            for name in result.new_concepts.iter().chain(result.reinforced_concepts.iter()) {
                let base_name = name.split(" (").next().unwrap_or(name).split(" →").next().unwrap_or(name);
                if let Some(concept) = kb_read.find_concept_by_label(base_name) {
                    discussed.push(concept.id);
                }
            }
            self.last_discussed = discussed;
        }

        // Gera resposta natural via LLM
        let concept_list: Vec<String> = result
            .new_concepts
            .iter()
            .chain(result.reinforced_concepts.iter())
            .cloned()
            .collect();

        if !concept_list.is_empty() {
            let system_prompt = format!(
                "Você é um assistente de cultivo epistêmico. O usuário acabou de dizer algo e o sistema extraiu os seguintes conceitos: [{}].\n\
                Gere uma resposta curta (1-2 frases) reconhecendo o que foi dito e conectando com conceitos existentes na base de conhecimento.\n\
                Responda em português brasileiro.",
                concept_list.join(", ")
            );

            match self.nlu.chat(&system_prompt, text).await {
                Ok(llm_response) => {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: llm_response,
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Falha ao gerar resposta LLM para narração");
                }
            }
        }

        // Sumário da KB (scoped lock)
        {
            let kb_read = self.kb.read();
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "KB: {} Concepts, {} Links",
                    kb_read.concept_count(),
                    kb_read.link_count()
                ),
            });
        }

        Ok(messages)
    }

    /// Processa confirmação ou negação do usuário.
    fn handle_confirmation(&mut self, positive: bool) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        let observation = TruthValue::observed(positive);
        let word = if positive { "Confirmação" } else { "Negação" };

        let mut kb = self.kb.write();
        for &concept_id in &self.last_discussed {
            if let Some(concept) = kb.concepts.get_mut(&concept_id) {
                let old_truth = concept.truth.clone();
                concept.truth = concept.truth.revision(&observation);
                messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "{}: {} {} → {}",
                        word, concept.label, old_truth, concept.truth
                    ),
                });
            }
        }

        let concept_ids = self.last_discussed.clone();
        for &cid in &concept_ids {
            let link_ids: Vec<_> = kb
                .links_for_concept(cid)
                .iter()
                .map(|l| l.id)
                .collect();
            for lid in link_ids {
                if let Some(link) = kb.links.get_mut(&lid) {
                    link.truth = link.truth.revision(&observation);
                }
            }
        }

        if messages.is_empty() {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "{}. Nenhum conceito recente para atualizar.",
                    word
                ),
            });
        }

        messages
    }

    /// Processa uma query/pergunta do usuário.
    async fn handle_query(&self, text: &str) -> Result<Vec<ChatMessage>> {
        let mut messages = Vec::new();

        let embedding = self.nlu.embed_query(text).await?;

        // Coleta dados da KB em escopo fechado (sem manter guard across await)
        let concept_descriptions = {
            let kb = self.kb.read();

            let mut matches: Vec<(String, String, f32, f64, Vec<String>)> = Vec::new();
            for concept in kb.concepts.values() {
                if let Some(ref emb) = concept.embedding {
                    let sim = crate::core::knowledge_base::cosine_similarity(&embedding, emb);
                    if sim > 0.5 {
                        let links = kb.links_for_concept(concept.id);
                        let link_desc: Vec<String> = links.iter().take(3).map(|l| kb.describe_link(l)).collect();
                        matches.push((
                            concept.label.clone(),
                            format!("{}", concept.truth),
                            sim,
                            concept.energy,
                            link_desc,
                        ));
                    }
                }
            }
            matches.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

            matches
                .into_iter()
                .take(5)
                .map(|(label, truth, sim, energy, link_desc)| {
                    let link_text = if link_desc.is_empty() {
                        String::new()
                    } else {
                        format!(" | Links: {}", link_desc.join("; "))
                    };
                    format!("- {} {} (sim={:.2}, energia={:.2}){}", label, truth, sim, energy, link_text)
                })
                .collect::<Vec<String>>()
        }; // kb guard dropped here

        if concept_descriptions.is_empty() {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: "Não encontrei conceitos relacionados na base de conhecimento.".into(),
            });
        } else {
            // Gera resposta natural via LLM
            let system_prompt = format!(
                "Você é um assistente de cultivo epistêmico. O usuário perguntou: \"{}\".\n\
                Os conceitos mais relevantes na base de conhecimento são:\n{}\n\
                Gere uma resposta informativa em português brasileiro baseada nesses conceitos.",
                text,
                concept_descriptions.join("\n")
            );

            match self.nlu.chat(&system_prompt, text).await {
                Ok(llm_response) => {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: llm_response,
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Falha ao gerar resposta LLM para query");
                    for desc in &concept_descriptions {
                        messages.push(ChatMessage {
                            role: MessageRole::System,
                            content: desc.clone(),
                        });
                    }
                }
            }
        }

        Ok(messages)
    }

    /// Executa um ciclo de inferência (fotossíntese).
    fn run_inference(&self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        let kb = self.kb.read();
        let inferences = InferenceEngine::infer(&kb);
        drop(kb);

        for result in inferences.into_iter().take(5) {
            let explanation = result.explanation.clone();
            let mut kb = self.kb.write();
            kb.add_link(result.link);
            messages.push(ChatMessage {
                role: MessageRole::Inference,
                content: format!("Inferência: {}", explanation),
            });
        }

        messages
    }

    /// Gera uma pergunta reflexiva (germinação).
    fn generate_question(&mut self) -> Option<String> {
        if let Some(q) = self.pending_questions.pop_front() {
            return Some(q);
        }

        let kb = self.kb.read();
        let candidates = kb.question_candidates();

        if let Some(concept) = candidates.first() {
            Some(
                self.nlu
                    .question_generator
                    .for_concept(concept),
            )
        } else {
            None
        }
    }

    /// Executa um ciclo de poda (decay).
    fn run_decay(&mut self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        let mut kb = self.kb.write();
        let newly_fading = kb.decay_cycle();

        for id in &newly_fading {
            if let Some(concept) = kb.concepts.get(id) {
                messages.push(ChatMessage {
                    role: MessageRole::Alert,
                    content: format!(
                        "'{}' está esmaecendo (energia: {:.2}). Deseja reforçar?",
                        concept.label, concept.energy
                    ),
                });
            }
        }

        if !newly_fading.is_empty() {
            messages.push(ChatMessage {
                role: MessageRole::Alert,
                content: format!(
                    "Poda: {} conceitos entrando em Fading.",
                    newly_fading.len()
                ),
            });
        }

        messages
    }

    /// Reset completo do estado do orquestrador.
    pub fn reset(&mut self) {
        self.last_discussed.clear();
        self.pending_questions.clear();
        self.turns_since_question = 0;
        self.total_turns = 0;
        self.turns_since_decay = 0;
    }

    /// Reforça um conceito manualmente (acionado pela sidebar).
    pub fn reinforce_concept(&self, concept_id: ConceptId) -> Option<String> {
        let mut kb = self.kb.write();
        if let Some(concept) = kb.concepts.get_mut(&concept_id) {
            concept.reinforce();
            Some(format!(
                "Reforçado: {} → energia {:.2}",
                concept.label, concept.energy
            ))
        } else {
            None
        }
    }
}
