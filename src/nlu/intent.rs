//! # Classificador de Intenção (Intent) do Usuário
//!
//! O [`IntentClassifier`] determina a **intenção** do usuário a partir
//! da sua mensagem. A intenção influencia como o sistema responde:
//!
//! | Intent | Significado | Exemplo |
//! |--------|-------------|---------|
//! | [`Confirming`](Intent::Confirming) | Usuário confirma/concorda | "sim, exatamente" |
//! | [`Denying`](Intent::Denying) | Usuário nega/discorda | "não, está errado" |
//! | [`Querying`](Intent::Querying) | Usuário pergunta algo | "como funciona?" |
//! | [`Narrating`](Intent::Narrating) | Usuário narra/informa | "o motor queimou" |
//!
//! ## Estratégia Híbrida (Heurística + Embedding)
//!
//! ```text
//! Mensagem do usuário
//!   ├── 1. Heurísticas rápidas (keywords + patterns)
//!   │   → Se match: retorna imediatamente
//!   └── 2. Embedding similarity (fallback)
//!       → Compara com templates pré-computados
//!       → Se melhor score > 0.65: retorna intent do template
//!       → Senão: retorna Narrating (default)
//! ```
//!
//! As heurísticas são verificadas primeiro por desempenho — não precisam
//! de forward pass no modelo. O fallback por embedding captura variações
//! que as heurísticas não cobrem.

use anyhow::Result;

use super::embedder::Embedder;
use crate::core::knowledge_base::cosine_similarity;

/// Intenção classificada a partir da mensagem do usuário.
///
/// ## Uso na Pipeline
///
/// O intent é incluído no [`NluResult`](super::NluResult) e influencia:
/// - Como o [`Orchestrator`](crate::orchestrator::Orchestrator) processa a mensagem
/// - Se o sistema gera perguntas reflexivas (só para `Narrating`)
/// - Se o sistema confirma/nega insights anteriores
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Usuário está confirmando ou concordando com algo.
    ///
    /// Exemplos: "sim", "correto", "exatamente", "faz sentido", "concordo"
    Confirming,

    /// Usuário está negando ou discordando de algo.
    ///
    /// Exemplos: "não", "errado", "discordo", "incorreto", "na verdade é diferente"
    Denying,

    /// Usuário está fazendo uma pergunta.
    ///
    /// Exemplos: "o que é X?", "como funciona?", "por que isso acontece?"
    Querying,

    /// Usuário está narrando/informando algo (o caso mais comum).
    ///
    /// Este é o intent **padrão** quando nenhum outro se aplica.
    /// A maioria das mensagens são narrações que adicionam conhecimento.
    Narrating,
}

/// Template interno de intent com embedding pré-computado.
///
/// Na inicialização, cada combinação (intent, frase-template) é
/// embeddada e armazenada. Na classificação, o embedding da mensagem
/// é comparado com todos os templates por cosine similarity.
struct IntentTemplate {
    /// O intent que este template representa.
    intent: Intent,
    /// Embedding pré-computado da frase-template (768-dim).
    embedding: Vec<f32>,
}

/// Classificador de intenção baseado em heurísticas + embedding similarity.
///
/// ## Inicialização
///
/// Computa embeddings para ~15 templates (5 por intent × 3 intents).
/// Apenas `Narrating` não tem templates — é o fallback default.
///
/// ## Custo
///
/// - Inicialização: ~15 × 15ms ≈ 225ms (forward pass para cada template)
/// - Classificação com heurística: ~0ms
/// - Classificação com embedding: ~15ms (1 forward pass + 15 comparações cosine)
pub struct IntentClassifier {
    /// Templates com embeddings pré-computados para matching por similaridade.
    templates: Vec<IntentTemplate>,
}

impl IntentClassifier {
    /// Cria um novo classificador com templates pré-embeddados.
    ///
    /// Computa embeddings para as frases-template de cada intent:
    /// - **5 templates** para `Confirming`
    /// - **5 templates** para `Denying`
    /// - **5 templates** para `Querying`
    /// - `Narrating` não tem templates (é o default)
    ///
    /// # Erros
    ///
    /// Retorna erro se o embedder falhar ao processar os templates.
    pub fn new(embedder: &Embedder) -> Result<Self> {
        let template_texts = vec![
            (Intent::Confirming, vec![
                "sim, correto, exatamente",
                "concordo, faz sentido",
                "é isso mesmo, verdade",
                "sim faz total sentido",
                "correto exato preciso",
            ]),
            (Intent::Denying, vec![
                "não, errado, incorreto",
                "discordo, não é assim",
                "na verdade é diferente",
                "não concordo está errado",
                "isso não está certo",
            ]),
            (Intent::Querying, vec![
                "o que é, como funciona",
                "por que, qual a razão",
                "me explique, o que significa",
                "como assim, pode explicar",
                "qual o motivo, por quê",
            ]),
        ];

        // Computa embedding para cada template
        let mut templates = Vec::new();
        for (intent, texts) in template_texts {
            for text in texts {
                // Prefixo "search_query:" indica ao modelo que é uma query
                let embedding = embedder.embed(&format!("search_query: {}", text))?;
                templates.push(IntentTemplate {
                    intent: intent.clone(),
                    embedding,
                });
            }
        }

        Ok(Self { templates })
    }

    /// Classifica o intent de uma mensagem do usuário.
    ///
    /// ## Estratégia (2 fases)
    ///
    /// ### Fase 1: Heurísticas Rápidas (~0ms)
    ///
    /// Verifica padrões simples no texto:
    /// - Começa com "sim"/"concordo" → `Confirming`
    /// - Começa com "não"/"discordo" → `Denying`
    /// - Começa com "o que"/"como"/"por que" ou contém "?" → `Querying`
    ///
    /// ### Fase 2: Fallback por Embedding (~15ms)
    ///
    /// Se nenhuma heurística acertou, compara o embedding da mensagem
    /// com os templates pré-computados. O template mais similar determina
    /// o intent, mas **só se a similaridade > 0.65**.
    ///
    /// Se nenhum template é suficientemente similar, retorna `Narrating`.
    ///
    /// # Parâmetros
    ///
    /// - `text` — mensagem do usuário
    /// - `embedder` — referência ao embedder para gerar embedding da mensagem
    ///
    /// # Retorno
    ///
    /// O [`Intent`] classificado — sempre retorna um valor (sem `None`).
    pub fn classify(&self, text: &str, embedder: &Embedder) -> Result<Intent> {
        let text_lower = text.to_lowercase().trim().to_string();

        // ─── Fase 1: Heurísticas rápidas ─────────────────────────
        // Verifica padrões conhecidos por substring matching (instantâneo)
        if text_lower.starts_with("sim")
            || text_lower == "correto"
            || text_lower == "exato"
            || text_lower.starts_with("faz sentido")
            || text_lower.starts_with("concordo")
        {
            return Ok(Intent::Confirming);
        }

        if text_lower.starts_with("não")
            || text_lower.starts_with("errado")
            || text_lower.starts_with("discordo")
            || text_lower.starts_with("incorreto")
        {
            return Ok(Intent::Denying);
        }

        if text_lower.starts_with("o que")
            || text_lower.starts_with("como")
            || text_lower.starts_with("por que")
            || text_lower.starts_with("qual")
            || text_lower.contains('?')
        {
            return Ok(Intent::Querying);
        }

        // ─── Fase 2: Fallback por embedding similarity ───────────
        // Gera embedding da mensagem e compara com todos os templates
        let embedding = embedder.embed(&format!("search_query: {}", text))?;
        let mut best_intent = Intent::Narrating;
        let mut best_score = 0.0f32;

        for template in &self.templates {
            let score = cosine_similarity(&embedding, &template.embedding);
            if score > best_score {
                best_score = score;
                best_intent = template.intent.clone();
            }
        }

        // Threshold 0.65: abaixo disso, não confiamos na classificação
        // e retornamos Narrating (o default seguro)
        if best_score > 0.65 {
            Ok(best_intent)
        } else {
            Ok(Intent::Narrating)
        }
    }
}
