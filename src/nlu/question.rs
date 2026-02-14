//! # Gerador de Perguntas Reflexivas — O Ciclo da Germinação
//!
//! O [`QuestionGenerator`] cria perguntas reflexivas que incentivam o
//! usuário a aprofundar conceitos com alta energia mas baixa confiança.
//!
//! ## Analogia: Germinação
//!
//! No jardim epistêmico, a germinação é o momento em que uma semente
//! (conceito inicial) começa a **brotar** — o sistema "pergunta de volta"
//! para que o conceito se desenvolva com mais detalhes e confiança.
//!
//! ## Quando Perguntas São Geradas?
//!
//! O [`Orchestrator`](crate::orchestrator::Orchestrator) seleciona conceitos
//! candidatos para perguntas usando estes critérios:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ Conceito candidato para Germinação:     │
//! │ • Energia > 0.5 (mencionado recentemente) │
//! │ • Confiança < 0.6 (ainda incerto)       │
//! │ • Estado == Active                      │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Tipos de Perguntas
//!
//! | Método | Alvo | Objetivo |
//! |--------|------|----------|
//! | `for_concept` | Conceito individual | Aprofundar entendimento |
//! | `for_relation` | Par de conceitos | Explorar conexões |
//! | `for_causal_link` | Relação causal | Validar causalidade |
//!
//! ## Seleção Determinística
//!
//! A pergunta é selecionada de forma **determinística** usando o UUID
//! do conceito (modulo número de templates). Isso garante que o mesmo
//! conceito sempre receba perguntas variadas a cada interação.

use crate::core::Concept;

/// Gerador de perguntas reflexivas para o ciclo de germinação.
///
/// Struct sem estado (unit struct) — todos os templates são definidos
/// inline nos métodos. Isso mantém a simplicidade ao mesmo tempo que
/// permite fácil extensão com novos templates.
pub struct QuestionGenerator;

impl QuestionGenerator {
    /// Cria um novo gerador de perguntas.
    pub fn new() -> Self {
        Self
    }

    /// Gera uma pergunta reflexiva para um conceito individual.
    ///
    /// Seleciona templates diferentes baseando-se no `mention_count`:
    ///
    /// ## Conceito frequente (≥ 3 menções)
    ///
    /// Perguntas mais profundas, assumindo familiaridade:
    /// - "Você mencionou X **N vezes**. Ainda relevante?"
    /// - "X aparece frequentemente. Pode elaborar?"
    /// - "X parece importante. O que aconteceria sem ele?"
    ///
    /// ## Conceito novo (< 3 menções)
    ///
    /// Perguntas exploratórias, buscando mais informação:
    /// - "Pode contar mais sobre X?"
    /// - "O que exatamente você quer dizer com X?"
    /// - "Qual a importância de X nesse contexto?"
    ///
    /// # Seleção Determinística
    ///
    /// O template é escolhido usando `concept.id.as_bytes()[0] % len`,
    /// garantindo que o mesmo conceito gere a mesma pergunta (estabilidade)
    /// mas conceitos diferentes gerem perguntas variadas.
    pub fn for_concept(&self, concept: &Concept) -> String {
        let templates = if concept.mention_count >= 3 {
            vec![
                format!(
                    "Você mencionou '{}' {} vezes. Isso ainda é relevante para você?",
                    concept.label, concept.mention_count
                ),
                format!(
                    "'{}' aparece frequentemente. Pode elaborar mais sobre o papel dele?",
                    concept.label
                ),
                format!(
                    "Parece que '{}' é importante. O que aconteceria sem ele?",
                    concept.label
                ),
            ]
        } else {
            vec![
                format!(
                    "Você mencionou '{}'. Pode contar mais sobre isso?",
                    concept.label
                ),
                format!(
                    "O que exatamente você quer dizer com '{}'?",
                    concept.label
                ),
                format!(
                    "Qual a importância de '{}' nesse contexto?",
                    concept.label
                ),
            ]
        };

        // Seleção determinística baseada no UUID do conceito
        let idx = concept.id.as_bytes()[0] as usize % templates.len();
        templates.into_iter().nth(idx).unwrap()
    }

    /// Gera uma pergunta sobre a relação entre dois conceitos.
    ///
    /// Usado quando o sistema detecta que dois conceitos aparecem
    /// frequentemente juntos ou têm embeddings relativamente próximos,
    /// mas ainda não possuem uma relação explícita forte.
    ///
    /// Templates exploram:
    /// - Existência de conexão direta
    /// - Influência de um sobre o outro
    /// - Existência de exceções
    ///
    /// # Seleção
    ///
    /// Usa soma dos UUIDs de ambos conceitos para variar a pergunta
    /// conforme o par de conceitos.
    pub fn for_relation(&self, source: &Concept, target: &Concept) -> String {
        let templates = vec![
            format!(
                "'{}' e '{}' parecem relacionados. Há uma conexão direta?",
                source.label, target.label
            ),
            format!(
                "Como '{}' influencia '{}'?",
                source.label, target.label
            ),
            format!(
                "Existem exceções para a relação entre '{}' e '{}'?",
                source.label, target.label
            ),
        ];

        // Combina UUIDs de ambos conceitos para seleção determinística do par
        let idx = (source.id.as_bytes()[0] as usize + target.id.as_bytes()[0] as usize)
            % templates.len();
        templates.into_iter().nth(idx).unwrap()
    }

    /// Gera uma pergunta sobre um link causal (implicação) entre dois conceitos.
    ///
    /// Usado quando existe um link do tipo [`Implication`](crate::core::LinkKind::Implication)
    /// entre dois conceitos, mas sua confiança ainda é baixa.
    ///
    /// Templates exploram a **robustez** da causalidade:
    /// - Existem exceções?
    /// - É sempre verdade ou condicional?
    /// - Existem outras causas possíveis?
    ///
    /// Essas perguntas ajudam o sistema a calibrar o [`TruthValue`](crate::core::TruthValue)
    /// do link — confirmações aumentam confiança, exceções a diminuem.
    pub fn for_causal_link(&self, cause: &Concept, effect: &Concept) -> String {
        let templates = vec![
            format!(
                "Existem exceções para '{}' causar '{}'?",
                cause.label, effect.label
            ),
            format!(
                "'{} → {}' é sempre verdade ou há condições específicas?",
                cause.label, effect.label
            ),
            format!(
                "O que mais pode causar '{}' além de '{}'?",
                effect.label, cause.label
            ),
        ];

        // Combina UUIDs do par causal para seleção determinística
        let idx = (cause.id.as_bytes()[0] as usize + effect.id.as_bytes()[0] as usize)
            % templates.len();
        templates.into_iter().nth(idx).unwrap()
    }
}
