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
//!   ├── 1. 🌱 SEMEADURA (Intent::Narrating)
//!   │   └── NLU processa → extrai entidades → cria/reforça conceitos
//!   │
//!   ├── 2. ☀️ FOTOSSÍNTESE (Inferência)
//!   │   └── InferenceEngine deduz/induz novos links a partir dos existentes
//!   │
//!   ├── 3. 🌱 GERMINAÇÃO (Perguntas reflexivas a cada ~2 turnos)
//!   │   └── QuestionGenerator cria perguntas para conceitos incertos
//!   │
//!   ├── 4. ✅/❌ CONFIRMAÇÃO/NEGAÇÃO (Intent::Confirming/Denying)
//!   │   └── Ajusta TruthValues dos conceitos recentes via revision
//!   │
//!   └── 5. 🍂 PODA (Decay a cada ~10 turnos)
//!       └── Conceitos inativos perdem energia → esmaescem → arquivam
//! ```
//!
//! ## Roles das Mensagens
//!
//! O orquestrador produz mensagens com diferentes [`MessageRole`]:
//!
//! | Role | Significado | Exemplo |
//! |------|-------------|---------|
//! | `System` | Atualizações da KB | "Cristalizando: X" |
//! | `Inference` | Inferências deduzidas | "🧪 Se A→B e B→C..." |
//! | `Question` | Perguntas reflexivas | "Pode contar mais sobre X?" |
//! | `Alert` | Alertas de poda | "⚠️ X está esmaecendo" |
//!
//! ## Estado Interno
//!
//! O orquestrador mantém estado conversacional:
//! - `last_discussed` — IDs dos conceitos discutidos no turno anterior
//! - `pending_questions` — fila de perguntas pendentes (FIFO)
//! - `turns_since_question` — contador para espaçar perguntas (~2 turnos)
//! - `turns_since_decay` — contador para ciclos de poda (~10 turnos)

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
///
/// A role indica a **origem semântica** da mensagem, não seu remetente literal.
/// O frontend usa a role para estilizar cada tipo de mensagem diferenciando-as.
pub struct ChatMessage {
    /// Role semântica da mensagem (System, Inference, Question, Alert).
    pub role: MessageRole,
    /// Conteúdo textual em PT-BR, pronto para exibição.
    pub content: String,
}

/// Role semântica das mensagens do sistema.
///
/// Determina como a mensagem é estilizada no frontend:
/// - `User` — mensagem do próprio usuário (exibida à direita)
/// - `System` — atualizações operacionais da KB (ex: conceitos criados)
/// - `Inference` — resultados da inferência NARS (com ícone 🧪)
/// - `Question` — perguntas reflexivas do sistema (com ícone 🌱)
/// - `Alert` — alertas de poda/degradação (com ícone ⚠️)
#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    /// Mensagem do usuário.
    User,
    /// Mensagem do sistema reportando operações na KB.
    System,
    /// Resultado de uma inferência NARS (dedução/indução).
    Inference,
    /// Pergunta reflexiva gerada pelo sistema (germinação).
    Question,
    /// Alerta sobre conceitos em degradação (poda).
    Alert,
}

/// Orquestrador do ciclo de cultivo epistêmico.
///
/// Coordena NLU, inferência, geração de perguntas, e decaimento.
/// Mantém estado conversacional para gerenciar o fluxo de interação.
///
/// ## Concorrência
///
/// O orquestrador não é `Send + Sync` — ele é "possuído" pela task
/// do handler web que processa mensagens. A KB e a NLU são acessadas
/// via `Arc`, permitindo compartilhamento com outros componentes.
pub struct Orchestrator {
    /// Pipeline NLU para processamento de linguagem natural.
    nlu: Arc<NluPipeline>,
    /// Base de conhecimento compartilhada (protegida por RwLock).
    kb: Arc<RwLock<KnowledgeBase>>,
    /// IDs dos conceitos discutidos no último turno (para confirm/deny).
    last_discussed: Vec<ConceptId>,
    /// Fila FIFO de perguntas pendentes (ainda não apresentadas).
    pending_questions: VecDeque<String>,
    /// Turnos desde a última pergunta reflexiva (germinação a cada ~2).
    turns_since_question: u32,
    /// Total de turnos na conversa atual.
    total_turns: u32,
    /// Turnos desde o último ciclo de poda (decay a cada ~10).
    turns_since_decay: u32,
}

impl Orchestrator {
    /// Cria um novo orquestrador com estado zerado.
    ///
    /// O orquestrador começa com contadores zerados e nenhum conceito
    /// discutido anteriormente. Os contadores permitem espaçar as
    /// perguntas reflexivas e ciclos de poda adequadamente.
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
    ///
    /// Este é o **método principal** — orquestra todo o ciclo de cultivo
    /// em uma única chamada. As respostas são retornadas em ordem
    /// cronológica para exibição sequencial no chat.
    ///
    /// ## Fluxo de Processamento
    ///
    /// ```text
    /// 1. Incrementa contadores de turno
    /// 2. Classifica intent da mensagem
    /// 3. Despacha para handler específico:
    ///    - Confirming → handle_confirmation(true)
    ///    - Denying → handle_confirmation(false)
    ///    - Querying → handle_query()
    ///    - Narrating → handle_narration()
    /// 4. Se narração: roda inferência (fotossíntese)
    /// 5. A cada ~2 turnos: gera pergunta reflexiva (germinação)
    /// 6. A cada ~10 turnos: ciclo de poda (decay)
    /// ```
    ///
    /// # Erros
    ///
    /// Propaga erros do NLU (tokenização, embedding).
    pub fn process_message(&mut self, user_text: &str) -> Result<Vec<ChatMessage>> {
        let mut responses = Vec::new();
        self.total_turns += 1;
        self.turns_since_question += 1;
        self.turns_since_decay += 1;

        // Classifica a intenção do usuário
        let intent = self.nlu.classify_intent(user_text)?;

        // Despacha para o handler apropriado baseado no intent
        match intent {
            Intent::Confirming => {
                responses.extend(self.handle_confirmation(true));
            }
            Intent::Denying => {
                responses.extend(self.handle_confirmation(false));
            }
            Intent::Querying => {
                responses.extend(self.handle_query(user_text)?);
            }
            Intent::Narrating => {
                responses.extend(self.handle_narration(user_text)?);
            }
        }

        // ☀️ Fotossíntese — inferência após narração (desde o primeiro turno)
        if intent == Intent::Narrating {
            responses.extend(self.run_inference());
        }

        // 🌱 Germinação — perguntas reflexivas a cada ~2 turnos
        if self.turns_since_question >= 2 {
            if let Some(question) = self.generate_question() {
                responses.push(ChatMessage {
                    role: MessageRole::Question,
                    content: question,
                });
                self.turns_since_question = 0;
            }
        }

        // 🍂 Poda — decay a cada ~10 turnos
        if self.turns_since_decay >= 10 {
            responses.extend(self.run_decay());
            self.turns_since_decay = 0;
        }

        Ok(responses)
    }

    /// Processa uma mensagem narrativa (informativa).
    ///
    /// Este é o handler mais complexo — aciona o pipeline NLU completo
    /// para extrair entidades, criar/reforçar conceitos, e criar links.
    ///
    /// ## Etapas
    ///
    /// 1. Processa mensagem via NLU (extração + embedding + KB update)
    /// 2. Gera mensagens sobre conceitos cristalizados/reforçados
    /// 3. Gera mensagens sobre novos links criados
    /// 4. Atualiza `last_discussed` (para confirmação/negação futura)
    /// 5. Retorna sumário da KB (total de conceitos e links)
    fn handle_narration(&mut self, text: &str) -> Result<Vec<ChatMessage>> {
        let mut messages = Vec::new();

        // Processa via NLU — cria/reforça conceitos, cria links
        let result = self.nlu.process_message(text, &self.kb)?;

        // Reporta conceitos cristalizados (novos)
        for msg in &result.messages {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: msg.clone(),
            });
        }

        // Reporta conceitos reforçados (já existentes)
        for concept_name in &result.reinforced_concepts {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!("Reforçando: {}", concept_name),
            });
        }

        // Reporta novos links criados
        for link_desc in &result.new_links {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!("Novo Link: {}", link_desc),
            });
        }

        // Atualiza last_discussed — mapeia labels de volta para IDs
        let kb_read = self.kb.read();
        let mut discussed = Vec::new();
        for name in result.new_concepts.iter().chain(result.reinforced_concepts.iter()) {
            // Extrai o nome base removendo sufixos como "(sim=0.85)" ou "→ reforçado"
            let base_name = name.split(" (").next().unwrap_or(name).split(" →").next().unwrap_or(name);
            if let Some(concept) = kb_read.find_concept_by_label(base_name) {
                discussed.push(concept.id);
            }
        }
        drop(kb_read);
        self.last_discussed = discussed;

        // Sumário da KB
        let kb_read = self.kb.read();
        messages.push(ChatMessage {
            role: MessageRole::System,
            content: format!(
                "📊 KB: {} Concepts, {} Links",
                kb_read.concept_count(),
                kb_read.link_count()
            ),
        });

        Ok(messages)
    }

    /// Processa confirmação ou negação do usuário.
    ///
    /// Ajusta o [`TruthValue`] dos conceitos discutidos recentemente
    /// usando a regra de **revisão** do NARS — mescla a observação
    /// (positiva ou negativa) com a evidência existente.
    ///
    /// ## Lógica
    ///
    /// ```text
    /// Para cada conceito em last_discussed:
    ///   concept.truth = concept.truth.revision(observation)
    ///
    /// Para cada link envolvendo esses conceitos:
    ///   link.truth = link.truth.revision(observation)
    /// ```
    ///
    /// Onde `observation` é:
    /// - `positive=true` → `TruthValue::observed(true)` → freq=0.9, conf=0.8
    /// - `positive=false` → `TruthValue::observed(false)` → freq=0.1, conf=0.8
    fn handle_confirmation(&mut self, positive: bool) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        let observation = TruthValue::observed(positive);
        let word = if positive { "Confirmação" } else { "Negação" };

        // Fase 1: Atualiza TruthValues dos conceitos recentes
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

        // Fase 2: Também atualiza links envolvendo esses conceitos
        // Isso garante que a confirmação/negação propague para as relações também
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

        // Se nenhum conceito recente para atualizar, informa o usuário
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
    ///
    /// Busca conceitos similares à query na KB usando embedding similarity.
    /// Retorna até 5 conceitos mais relevantes com suas verdades, links,
    /// e similaridade com a query.
    ///
    /// ## Algoritmo
    ///
    /// ```text
    /// 1. Gera embedding da query
    /// 2. Compara com todos os conceitos (cosine sim > 0.5)
    /// 3. Ordena por similaridade (descendente)
    /// 4. Retorna top-5 com links associados
    /// ```
    fn handle_query(&self, text: &str) -> Result<Vec<ChatMessage>> {
        let mut messages = Vec::new();

        // Gera embedding da query (modo "search_query:")
        let embedding = self.nlu.embed_query(text)?;
        let kb = self.kb.read();

        // Busca conceitos similares (threshold 0.5 — mais permissivo que semeadura)
        let mut matches: Vec<(&crate::core::Concept, f32)> = Vec::new();
        for concept in kb.concepts.values() {
            if let Some(ref emb) = concept.embedding {
                let sim = crate::core::knowledge_base::cosine_similarity(&embedding, emb);
                if sim > 0.5 {
                    matches.push((concept, sim));
                }
            }
        }
        // Ordena por similaridade descendente
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if matches.is_empty() {
            messages.push(ChatMessage {
                role: MessageRole::System,
                content: "Não encontrei conceitos relacionados na base de conhecimento.".into(),
            });
        } else {
            // Retorna os 5 conceitos mais similares
            for (concept, sim) in matches.iter().take(5) {
                let links = kb.links_for_concept(concept.id);
                let link_desc: Vec<String> = links.iter().take(3).map(|l| kb.describe_link(l)).collect();
                let link_text = if link_desc.is_empty() {
                    String::new()
                } else {
                    format!("\n  Links: {}", link_desc.join("; "))
                };
                messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "🔍 {} {} (sim={:.2}, energia={:.2}){}",
                        concept.label, concept.truth, sim, concept.energy, link_text
                    ),
                });
            }
        }

        Ok(messages)
    }

    /// Executa um ciclo de inferência (fotossíntese).
    ///
    /// Chama o [`InferenceEngine`] para derivar novos links a partir
    /// dos existentes. Limita a **5 inferências por turno** para não
    /// inundar o chat com informações.
    ///
    /// Os novos links são adicionados à KB e reportados ao usuário
    /// com o ícone 🧪.
    fn run_inference(&self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        let kb = self.kb.read();
        let inferences = InferenceEngine::infer(&kb);
        drop(kb); // libera lock de leitura antes de escrever

        // Limita a 5 inferências por turno (evita spam)
        for result in inferences.into_iter().take(5) {
            let explanation = result.explanation.clone();
            let mut kb = self.kb.write();
            kb.add_link(result.link);
            messages.push(ChatMessage {
                role: MessageRole::Inference,
                content: format!("🧪 Inferência: {}", explanation),
            });
        }

        messages
    }

    /// Gera uma pergunta reflexiva (germinação).
    ///
    /// Primeiro verifica a fila de perguntas pendentes. Se vazia,
    /// busca conceitos candidatos na KB (alta energia + baixa confiança)
    /// e gera uma pergunta usando o [`QuestionGenerator`].
    ///
    /// ## Retorno
    ///
    /// `Some(pergunta)` se há algo para perguntar; `None` se a KB
    /// não tem conceitos candidatos e a fila está vazia.
    fn generate_question(&mut self) -> Option<String> {
        // Primeiro: perguntas pendentes (prioridade)
        if let Some(q) = self.pending_questions.pop_front() {
            return Some(q);
        }

        // Segundo: gera pergunta para conceito candidato
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
    ///
    /// Reduz a energia de todos os conceitos e identifica os que
    /// transitaram para o estado [`Fading`](crate::core::concept::ConceptState::Fading).
    ///
    /// Conceitos em Fading são reportados ao usuário com um alerta,
    /// dando a oportunidade de reforçá-los antes que sejam arquivados.
    fn run_decay(&mut self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        let mut kb = self.kb.write();
        // decay_cycle() retorna IDs dos conceitos que acabaram de entrar em Fading
        let newly_fading = kb.decay_cycle();

        // Alerta individual para cada conceito esmaecendo
        for id in &newly_fading {
            if let Some(concept) = kb.concepts.get(id) {
                messages.push(ChatMessage {
                    role: MessageRole::Alert,
                    content: format!(
                        "⚠️ '{}' está esmaecendo (energia: {:.2}). Deseja reforçar?",
                        concept.label, concept.energy
                    ),
                });
            }
        }

        // Sumário de poda
        if !newly_fading.is_empty() {
            messages.push(ChatMessage {
                role: MessageRole::Alert,
                content: format!(
                    "🍂 Poda: {} conceitos entrando em Fading.",
                    newly_fading.len()
                ),
            });
        }

        messages
    }

    /// Reset completo do estado do orquestrador.
    ///
    /// Usado quando a KB é resetada via interface web (botão "Limpar KB").
    /// Zera todos os contadores e limpa as filas para acompanhar o reset da KB.
    pub fn reset(&mut self) {
        self.last_discussed.clear();
        self.pending_questions.clear();
        self.turns_since_question = 0;
        self.total_turns = 0;
        self.turns_since_decay = 0;
    }

    /// Reforça um conceito manualmente (acionado pela sidebar).
    ///
    /// Permite ao usuário clicar em um conceito na sidebar e reforçá-lo
    /// diretamente, sem precisar mencioná-lo no chat. Útil para conceitos
    /// que estão em Fading e o usuário quer manter ativos.
    ///
    /// # Retorno
    ///
    /// - `Some(mensagem)` — mensagem de confirmação com novo nível de energia
    /// - `None` — conceito não encontrado na KB
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
