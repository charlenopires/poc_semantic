//! # Regras de Inferência NARS
//!
//! Implementação das regras de inferência do NARS (Non-Axiomatic Reasoning System)
//! aplicadas sobre links causais ativos na [KnowledgeBase].
//!
//! ## Como Funciona
//!
//! O [`InferenceEngine`] examina todos os links causais ativos (energia > 0.3)
//! e tenta combinar pares de links para derivar novas relações:
//!
//! ### Dedução: S→M + M→P ⊢ S→P
//!
//! Se existem dois links onde o **objeto** do primeiro é o **sujeito** do segundo,
//! podemos deduzir uma relação direta do sujeito do primeiro ao objeto do segundo.
//!
//! ```text
//! Link 1: "Chuva" →[⇒] "Enchente"     (S→M)
//! Link 2: "Enchente" →[⇒] "Dano"       (M→P)
//! ─────────────────────────────────────
//! Dedução: "Chuva" →[⇒] "Dano"         (S→P)
//! ```
//!
//! ### Indução: M→P + M→S ⊢ S ≈ P
//!
//! Se dois links compartilham o **mesmo sujeito** (M), podemos induzir que
//! seus objetos (S e P) são **similares** — já que ambos derivam de M.
//!
//! ```text
//! Link 1: "Motor" →[⇒] "Velocidade"    (M→P)
//! Link 2: "Motor" →[⇒] "Consumo"       (M→S)
//! ─────────────────────────────────────
//! Indução: "Consumo" ≈ "Velocidade"     (S ≈ P)
//! ```
//!
//! ## Filtros de Qualidade
//!
//! - Só processa links com **energia > 0.3** (links relevantes)
//! - Só cria links que **não existem** ainda na KB (evita duplicação)
//! - Só cria links com **confiança > 0.05** (evita ruído)

use crate::core::{KnowledgeBase, Link, LinkKind, Participant, Role};

/// Resultado de uma inferência — contém o novo link e uma explicação legível.
///
/// A explicação é apresentada ao usuário na interface para que ele
/// entenda o raciocínio do sistema.
///
/// ## Exemplo de Explicação
///
/// ```text
/// Dedução: Se Chuva → Enchente e Enchente → Dano, então Chuva pode → Dano ⟨0.72, 0.45⟩
/// ```
pub struct InferenceResult {
    /// O novo link inferido, pronto para ser adicionado à KB.
    pub link: Link,
    /// Explicação legível do raciocínio em PT-BR.
    pub explanation: String,
}

/// Motor de inferência NARS — struct sem estado, totalmente funcional.
///
/// O motor não armazena estado — recebe a KB por referência e retorna
/// os novos links inferidos. Isso permite uso simples e thread-safe.
///
/// ## Uso
///
/// ```rust
/// let resultados = InferenceEngine::infer(&kb);
/// for resultado in resultados {
///     kb.add_link(resultado.link);
///     println!("{}", resultado.explanation);
/// }
/// ```
pub struct InferenceEngine;

impl InferenceEngine {
    /// Roda um ciclo completo de inferência sobre a KB.
    ///
    /// Examina todos os pares de links causais ativos e aplica as regras
    /// de dedução e indução. Retorna novos links que devem ser adicionados à KB.
    ///
    /// ## Algoritmo (O(n²) sobre links causais ativos)
    ///
    /// ```text
    /// para cada par (link_i, link_j) onde i ≠ j:
    ///   // Dedução: objeto de i == sujeito de j?
    ///   se link_i.object == link_j.subject E link não existe:
    ///     deduzir: link_i.subject → link_j.object
    ///
    ///   // Indução: sujeito de i == sujeito de j?
    ///   se link_i.subject == link_j.subject E link não existe:
    ///     induzir: link_j.object ≈ link_i.object
    /// ```
    ///
    /// ## Performance
    ///
    /// A complexidade é O(n²) no número de links causais ativos.
    /// Para uma KB típica com ~100 links ativos, isso é instantâneo.
    /// Para KBs muito grandes (>1000 links), considerar otimização.
    ///
    /// ## Retorno
    ///
    /// `Vec<InferenceResult>` — links inferidos prontos para serem
    /// adicionados à KB pelo [`Orchestrator`](crate::orchestrator::Orchestrator).
    pub fn infer(kb: &KnowledgeBase) -> Vec<InferenceResult> {
        let mut results = Vec::new();
        let energy_threshold = 0.3;

        // Busca links causais (Implication, Inheritance, Catalyzes) com energia suficiente
        let active_links = kb.causal_links(energy_threshold);

        // Examina todos os pares (i, j) com i ≠ j
        for i in 0..active_links.len() {
            for j in 0..active_links.len() {
                if i == j {
                    continue;
                }

                let link_sm = active_links[i];
                let link_mp = active_links[j];

                // ════════════════════════════════════════════════════════
                // DEDUÇÃO: S→M + M→P ⊢ S→P
                // O objeto do link_sm deve ser o sujeito do link_mp (M intermediário)
                // ════════════════════════════════════════════════════════
                if let (Some(s), Some(m1)) = (link_sm.subject(), link_sm.object()) {
                    if let (Some(m2), Some(p)) = (link_mp.subject(), link_mp.object()) {
                        // m1 == m2: o intermediário M conecta os dois links
                        // s != p: evita links triviais (A→A)
                        if m1 == m2 && s != p {
                            // Verifica se o link S→P já existe (evita duplicação)
                            if !kb.link_exists(&link_sm.kind, s, p) {
                                // Aplica a regra de dedução do TruthValue
                                let truth = link_sm.truth.deduction(&link_mp.truth);
                                // Só cria se a confiança for minimamente significativa
                                if truth.confidence() > 0.05 {
                                    let link = Link::new(
                                        link_sm.kind.clone(),
                                        vec![
                                            Participant {
                                                concept_id: s,
                                                role: Role::Subject,
                                            },
                                            Participant {
                                                concept_id: p,
                                                role: Role::Object,
                                            },
                                        ],
                                        truth,
                                    );
                                    // Constrói explicação legível usando labels dos conceitos
                                    let s_label = kb
                                        .concepts
                                        .get(&s)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let m_label = kb
                                        .concepts
                                        .get(&m1)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let p_label = kb
                                        .concepts
                                        .get(&p)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let explanation = format!(
                                        "Dedução: Se {} → {} e {} → {}, então {} pode → {} {}",
                                        s_label,
                                        m_label,
                                        m_label,
                                        p_label,
                                        s_label,
                                        p_label,
                                        link.truth
                                    );
                                    results.push(InferenceResult { link, explanation });
                                }
                            }
                        }
                    }
                }

                // ════════════════════════════════════════════════════════
                // INDUÇÃO: M→P + M→S ⊢ S ≈ P
                // Dois links compartilham o mesmo sujeito M, logo seus
                // objetos P e S provavelmente são similares
                // ════════════════════════════════════════════════════════
                if let (Some(m1), Some(p)) = (link_sm.subject(), link_sm.object()) {
                    if let (Some(m2), Some(s)) = (link_mp.subject(), link_mp.object()) {
                        // m1 == m2: compartilham o sujeito M
                        // s != p: evita links triviais
                        if m1 == m2 && s != p {
                            if !kb.link_exists(&link_sm.kind, s, p) {
                                // Aplica a regra de indução do TruthValue
                                let truth = link_sm.truth.induction(&link_mp.truth);
                                if truth.confidence() > 0.05 {
                                    // Indução gera link de Similaridade (≈)
                                    let link = Link::new(
                                        LinkKind::Similarity,
                                        vec![
                                            Participant {
                                                concept_id: s,
                                                role: Role::Subject,
                                            },
                                            Participant {
                                                concept_id: p,
                                                role: Role::Object,
                                            },
                                        ],
                                        truth,
                                    );
                                    let s_label = kb
                                        .concepts
                                        .get(&s)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let p_label = kb
                                        .concepts
                                        .get(&p)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let m_label = kb
                                        .concepts
                                        .get(&m1)
                                        .map(|c| c.label.as_str())
                                        .unwrap_or("?");
                                    let explanation = format!(
                                        "Indução: {} e {} compartilham {}, então {} ≈ {} {}",
                                        s_label,
                                        p_label,
                                        m_label,
                                        s_label,
                                        p_label,
                                        link.truth
                                    );
                                    results.push(InferenceResult { link, explanation });
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }
}
