//! # KnowledgeBase — Contêiner Central de Conhecimento
//!
//! A [`KnowledgeBase`] é o **coração** do Cultivo Epistêmico — o contêiner
//! que armazena todos os conceitos e links em memória, com índices para
//! busca rápida e métodos para consulta e manutenção.
//!
//! ## Analogia: O Solo do Jardim
//!
//! Se os conceitos são plantas e os links são raízes, a KnowledgeBase
//! é o **solo** que sustenta tudo — armazena, organiza e permite que
//! os componentes interajam.
//!
//! ## Armazenamento
//!
//! - **Conceitos**: `HashMap<ConceptId, Concept>` — busca O(1) por ID
//! - **Links**: `HashMap<LinkId, Link>` — busca O(1) por ID
//! - **Índice reverso**: `HashMap<ConceptId, Vec<LinkId>>` — "quais links envolvem este conceito?"
//!
//! O índice reverso é construído em memória e **não é serializado** (`#[serde(skip)]`).
//! Após desserialização, deve ser reconstruído via [`rebuild_index()`](KnowledgeBase::rebuild_index).
//!
//! ## Persistência
//!
//! A KB é serializada como JSON em `data/kb.json` via
//! [`persistence::save_kb`](crate::persistence::save_kb).
//!
//! ## Exemplo de Uso
//!
//! ```rust
//! use crate::core::{KnowledgeBase, Concept, TruthValue, Link, LinkKind, Participant, Role};
//!
//! let mut kb = KnowledgeBase::new();
//!
//! // Adicionar conceitos
//! let rust_id = kb.add_concept(Concept::new("Rust".to_string(), TruthValue::proto()));
//! let lang_id = kb.add_concept(Concept::new("Linguagem".to_string(), TruthValue::proto()));
//!
//! // Criar relação "Rust é uma Linguagem"
//! let link = Link::new(
//!     LinkKind::Inheritance,
//!     vec![
//!         Participant { concept_id: rust_id, role: Role::Subject },
//!         Participant { concept_id: lang_id, role: Role::Object },
//!     ],
//!     TruthValue::new(0.95, 0.85),
//! );
//! kb.add_link(link);
//!
//! // Buscar por label
//! assert!(kb.find_concept_by_label("rust").is_some());
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::concept::{Concept, ConceptId, ConceptState};
use super::link::{Link, LinkId, LinkKind};

/// Base de conhecimento in-memory — contêiner central de [Concept]s e [Link]s.
///
/// Todas as operações de leitura e escrita na KB passam por esta struct.
/// No servidor, ela é protegida por `Arc<RwLock<KnowledgeBase>>` para
/// acesso concorrente seguro entre múltiplas threads.
///
/// ## Índice Reverso
///
/// O campo `concept_links` mantém um índice reverso: para cada conceito,
/// sabe-se quais links o mencionam. Isso permite consultas rápidas como
/// "quais relações envolvem o conceito Fotossíntese?".
///
/// Este índice é marcado com `#[serde(skip)]` e deve ser reconstruído
/// via [`rebuild_index()`](KnowledgeBase::rebuild_index) após desserialização.
#[derive(Serialize, Deserialize)]
pub struct KnowledgeBase {
    /// Mapa de conceitos: ID → Conceito.
    pub concepts: HashMap<ConceptId, Concept>,

    /// Mapa de links: ID → Link.
    pub links: HashMap<LinkId, Link>,

    /// Índice reverso: concept_id → IDs dos links que envolvem este conceito.
    ///
    /// **Não serializado** — reconstruído em memória após load.
    /// Isso evita duplicação de dados no JSON e mantém o arquivo compacto.
    #[serde(skip, default)]
    concept_links: HashMap<ConceptId, Vec<LinkId>>,
}

impl KnowledgeBase {
    /// Cria uma KnowledgeBase vazia.
    ///
    /// Todos os HashMaps iniciam vazios. O índice reverso será
    /// populado automaticamente ao adicionar links via [`add_link()`](KnowledgeBase::add_link).
    pub fn new() -> Self {
        Self {
            concepts: HashMap::new(),
            links: HashMap::new(),
            concept_links: HashMap::new(),
        }
    }

    /// Reconstrói o índice reverso `concept_links` a partir dos links existentes.
    ///
    /// **Deve ser chamado após desserialização**, porque o campo `concept_links`
    /// é `#[serde(skip)]` e portanto estará vazio após `load_kb()`.
    ///
    /// Percorre todos os links e, para cada participante, registra o link_id
    /// no índice reverso do conceito correspondente.
    pub fn rebuild_index(&mut self) {
        self.concept_links.clear();
        for (link_id, link) in &self.links {
            for p in &link.participants {
                self.concept_links
                    .entry(p.concept_id)
                    .or_default()
                    .push(*link_id);
            }
        }
    }

    /// Limpa toda a KB — remove todos os conceitos, links e índices.
    ///
    /// Usado quando o usuário solicita "reset" da base de conhecimento.
    /// Não afeta o arquivo em disco até que `save_kb()` seja chamado.
    pub fn clear(&mut self) {
        self.concepts.clear();
        self.links.clear();
        self.concept_links.clear();
    }

    /// Adiciona um conceito à KB e retorna seu [ConceptId].
    ///
    /// Se já existir um conceito com o mesmo ID (improvável com UUID v4),
    /// ele será sobrescrito (comportamento do HashMap::insert).
    ///
    /// Emite log de nível `debug` com o ID e label do conceito armazenado.
    pub fn add_concept(&mut self, concept: Concept) -> ConceptId {
        let id = concept.id;
        tracing::debug!(id = %id, label = %concept.label, "KB: conceito armazenado");
        self.concepts.insert(id, concept);
        id
    }

    /// Adiciona um link à KB, atualiza o índice reverso, e retorna o [LinkId].
    ///
    /// Para cada participante do link, registra o link_id no índice reverso
    /// (`concept_links`) do conceito correspondente. Isso permite consultas
    /// rápidas via [`links_for_concept()`](KnowledgeBase::links_for_concept).
    pub fn add_link(&mut self, link: Link) -> LinkId {
        let id = link.id;
        tracing::debug!(id = %id, kind = %link.kind.label(), "KB: link armazenado");
        // Atualiza o índice reverso para cada participante
        for p in &link.participants {
            self.concept_links
                .entry(p.concept_id)
                .or_default()
                .push(id);
        }
        self.links.insert(id, link);
        id
    }

    /// Busca conceito por label (case-insensitive).
    ///
    /// Converte ambos os labels para lowercase antes de comparar.
    /// Retorna o primeiro conceito encontrado com label exato (após lowercase).
    ///
    /// # Exemplo
    ///
    /// ```rust
    /// let conceito = kb.find_concept_by_label("fotossíntese");
    /// // Encontra "Fotossíntese", "FOTOSSÍNTESE", "fotossíntese", etc.
    /// ```
    ///
    /// # Performance
    ///
    /// Busca linear O(n) — adequada para KBs com milhares de conceitos.
    /// Para KBs maiores, um índice adicional por label seria recomendado.
    pub fn find_concept_by_label(&self, label: &str) -> Option<&Concept> {
        let label_lower = label.to_lowercase();
        self.concepts
            .values()
            .find(|c| c.label.to_lowercase() == label_lower)
    }

    /// Busca o conceito mais similar por embedding (cosine similarity).
    ///
    /// Percorre todos os conceitos que têm embedding e calcula a
    /// similaridade cosseno com o embedding fornecido. Retorna o conceito
    /// com maior similaridade, **desde que esteja acima do threshold**.
    ///
    /// # Parâmetros
    ///
    /// - `embedding` — vetor de 768 dimensões (BERTimbau) para comparação
    /// - `threshold` — similaridade mínima (0.0 a 1.0). Recomendado: 0.75
    ///
    /// # Retorno
    ///
    /// - `Some((concept_id, similarity))` — conceito mais similar
    /// - `None` — nenhum conceito acima do threshold
    ///
    /// # Performance
    ///
    /// Busca linear O(n × d), onde n = número de conceitos e d = dimensão
    /// do embedding (768). Para KBs com ~10k conceitos, isso é rápido.
    pub fn find_similar_concept(
        &self,
        embedding: &[f32],
        threshold: f32,
    ) -> Option<(ConceptId, f32)> {
        let mut best: Option<(ConceptId, f32)> = None;
        for concept in self.concepts.values() {
            if let Some(ref emb) = concept.embedding {
                let sim = cosine_similarity(embedding, emb);
                if sim >= threshold {
                    if best.is_none() || sim > best.unwrap().1 {
                        best = Some((concept.id, sim));
                    }
                }
            }
        }
        // Log do conceito similar encontrado (para debugging)
        if let Some((id, sim)) = &best {
            if let Some(concept) = self.concepts.get(id) {
                tracing::debug!(label = %concept.label, similarity = %format!("{:.2}", sim), "KB: conceito similar encontrado");
            }
        }
        best
    }

    /// Retorna conceitos candidatos para **perguntas reflexivas**.
    ///
    /// O sistema gera perguntas para conceitos que têm:
    /// - **Alta energia** (`> 0.4`) — são relevantes na conversa atual
    /// - **Baixa confiança** (`< 0.5`) — o sistema tem pouca certeza sobre eles
    /// - **Estado ativo** — não estão dormentes ou esmaecendo
    ///
    /// Retorna ordenado por energia decrescente (mais "urgentes" primeiro).
    ///
    /// # Caso de Uso
    ///
    /// Se o conceito "Fotossíntese" tem alta energia (mencionado muitas vezes)
    /// mas baixa confiança (pouca evidência direta), o sistema pode perguntar:
    /// "O que exatamente você entende por Fotossíntese?"
    pub fn question_candidates(&self) -> Vec<&Concept> {
        let mut candidates: Vec<&Concept> = self
            .concepts
            .values()
            .filter(|c| {
                c.state == ConceptState::Active && c.energy > 0.4 && c.truth.confidence() < 0.5
            })
            .collect();
        // Ordena por energia decrescente — conceitos mais "quentes" primeiro
        candidates.sort_by(|a, b| {
            b.energy
                .partial_cmp(&a.energy)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates
    }

    /// Retorna todos os conceitos ativos, ordenados por energia decrescente.
    ///
    /// Usado na sidebar da interface para mostrar os conceitos mais
    /// relevantes no topo.
    pub fn active_concepts(&self) -> Vec<&Concept> {
        let mut active: Vec<&Concept> = self
            .concepts
            .values()
            .filter(|c| c.state == ConceptState::Active)
            .collect();
        active.sort_by(|a, b| {
            b.energy
                .partial_cmp(&a.energy)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        active
    }

    /// Retorna todos os conceitos com estado [ConceptState::Fading].
    ///
    /// Usado para exibir alertas na interface: "Estes conceitos estão
    /// sendo esquecidos — mencione-os para reforçar!"
    pub fn fading_concepts(&self) -> Vec<&Concept> {
        self.concepts
            .values()
            .filter(|c| c.state == ConceptState::Fading)
            .collect()
    }

    /// Retorna todos os links que envolvem um conceito específico.
    ///
    /// Utiliza o índice reverso `concept_links` para busca rápida O(k),
    /// onde k = número de links que envolvem o conceito.
    ///
    /// Usado na sidebar para mostrar as relações de um conceito selecionado.
    pub fn links_for_concept(&self, concept_id: ConceptId) -> Vec<&Link> {
        self.concept_links
            .get(&concept_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.links.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Retorna links binários ativos (com Subject e Object) acima de um threshold de energia.
    ///
    /// "Binário" aqui significa que o link tem tanto Subject quanto Object —
    /// requisito para as regras de inferência NARS (dedução, indução).
    ///
    /// # Parâmetros
    ///
    /// - `energy_threshold` — energia mínima do link (ex: 0.3)
    pub fn active_binary_links(&self, energy_threshold: f64) -> Vec<&Link> {
        self.links
            .values()
            .filter(|l| l.energy > energy_threshold && l.subject().is_some() && l.object().is_some())
            .collect()
    }

    /// Retorna links causais/de implicação ativos para o motor de inferência.
    ///
    /// Filtra links com tipos causais:
    /// - `Implication` (⇒)
    /// - `Inheritance` (é um)
    /// - `Catalyzes` (catalisa)
    ///
    /// Estes são os links que o motor de inferência NARS usa para
    /// deduzir novas relações (ver [`InferenceEngine`](crate::inference::InferenceEngine)).
    ///
    /// # Parâmetros
    ///
    /// - `energy_threshold` — energia mínima do link
    pub fn causal_links(&self, energy_threshold: f64) -> Vec<&Link> {
        self.links
            .values()
            .filter(|l| {
                l.energy > energy_threshold
                    && matches!(
                        l.kind,
                        LinkKind::Implication | LinkKind::Inheritance | LinkKind::Catalyzes
                    )
            })
            .collect()
    }

    /// Verifica se já existe um link com determinado tipo entre dois conceitos.
    ///
    /// Evita duplicação de links — antes de criar um novo link,
    /// o [`NluPipeline`](crate::nlu::NluPipeline) verifica se já existe.
    ///
    /// # Retorno
    ///
    /// - `true` — já existe um link com esse kind, subject e object
    /// - `false` — não existe, pode criar
    pub fn link_exists(&self, kind: &LinkKind, subject: ConceptId, object: ConceptId) -> bool {
        self.links.values().any(|l| {
            l.kind == *kind && l.subject() == Some(subject) && l.object() == Some(object)
        })
    }

    /// Executa um ciclo de decaimento ("Poda Natural") em toda a KB.
    ///
    /// Decai a energia de **todos** os conceitos e links com fator 0.95
    /// (5% de perda por ciclo). Retorna os IDs dos conceitos que
    /// **entraram no estado Fading** neste ciclo (eram Active ou Dormant antes).
    ///
    /// # Retorno
    ///
    /// `Vec<ConceptId>` — conceitos que ficaram Fading neste ciclo.
    /// Usado pelo orquestrador para emitir alertas ao usuário.
    ///
    /// # Exemplo
    ///
    /// Um conceito com energia 0.8 atinge energy < 0.2 (Fading) após:
    /// `0.8 × 0.95^n < 0.2` → `n ≈ 28 ciclos`
    pub fn decay_cycle(&mut self) -> Vec<ConceptId> {
        let decay_factor = 0.95;
        let mut newly_fading = Vec::new();

        // Decai todos os conceitos
        for concept in self.concepts.values_mut() {
            let was_fading = concept.state == ConceptState::Fading;
            concept.decay(decay_factor);
            // Detecta conceitos que ACABARAM de entrar em Fading
            if !was_fading && concept.state == ConceptState::Fading {
                newly_fading.push(concept.id);
            }
        }

        // Decai todos os links
        for link in self.links.values_mut() {
            link.decay(decay_factor);
        }

        newly_fading
    }

    /// Gera uma descrição legível de um link para exibição na interface.
    ///
    /// Formato: `[Conceito1 →Papel1, Conceito2 →Papel2] tipo_relação ⟨f, c⟩`
    ///
    /// # Exemplo de Saída
    ///
    /// ```text
    /// [Chuva →Sujeito, Enchente →Objeto] ⇒ ⟨0.90, 0.70⟩
    /// ```
    pub fn describe_link(&self, link: &Link) -> String {
        let parts: Vec<String> = link
            .participants
            .iter()
            .filter_map(|p| {
                self.concepts.get(&p.concept_id).map(|c| {
                    format!("{} →{}", c.label, p.role.label())
                })
            })
            .collect();
        format!("[{}] {} {}", parts.join(", "), link.kind.label(), link.truth)
    }

    /// Retorna o número total de conceitos na KB.
    pub fn concept_count(&self) -> usize {
        self.concepts.len()
    }

    /// Retorna o número total de links na KB.
    pub fn link_count(&self) -> usize {
        self.links.len()
    }
}

/// Calcula a **similaridade cosseno** entre dois vetores.
///
/// A similaridade cosseno mede o ângulo entre dois vetores no espaço
/// N-dimensional. É a métrica padrão para comparar embeddings de texto.
///
/// ## Fórmula
///
/// ```text
/// cos(θ) = (A · B) / (‖A‖ × ‖B‖)
/// ```
///
/// ## Interpretação
///
/// | Valor | Significado |
/// |-------|-------------|
/// | 1.0 | Vetores idênticos (mesma direção) |
/// | 0.75+ | Muito similares (recomendado como threshold) |
/// | 0.5 | Moderadamente similares |
/// | 0.0 | Sem relação (ortogonais) |
/// | -1.0 | Opostos (direções contrárias) |
///
/// ## Edge Cases
///
/// - Vetores de tamanhos diferentes → retorna 0.0
/// - Vetores vazios → retorna 0.0
/// - Vetor zero (norma 0) → retorna 0.0
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    // Vetores devem ter o mesmo tamanho e não ser vazios
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    // Calcula produto escalar e normas em uma única passada
    let mut dot = 0.0f32;    // A · B
    let mut norm_a = 0.0f32; // ‖A‖²
    let mut norm_b = 0.0f32; // ‖B‖²
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0 // Evita divisão por zero
    } else {
        dot / denom
    }
}
