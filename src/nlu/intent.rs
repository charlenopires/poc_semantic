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

use anyhow::Result;

use super::embedder::Embedder;
use crate::core::knowledge_base::cosine_similarity;

/// Intenção classificada a partir da mensagem do usuário.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Usuário está confirmando ou concordando com algo.
    Confirming,
    /// Usuário está negando ou discordando de algo.
    Denying,
    /// Usuário está fazendo uma pergunta.
    Querying,
    /// Usuário está narrando/informando algo (o caso mais comum).
    Narrating,
}

/// Template interno de intent com embedding pré-computado.
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
/// Computa embeddings para ~15 templates via uma única chamada `embed_batch`
/// (1 HTTP request vs 15 individuais).
pub struct IntentClassifier {
    /// Templates com embeddings pré-computados para matching por similaridade.
    templates: Vec<IntentTemplate>,
}

impl IntentClassifier {
    /// Cria um novo classificador com templates pré-embeddados.
    ///
    /// Usa `embed_batch` para computar todos os templates em uma única
    /// chamada HTTP ao LM Studio (muito mais eficiente).
    pub async fn new(embedder: &Embedder) -> Result<Self> {
        let template_defs = vec![
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

        // Coleta todos os textos e intents para batch
        let mut all_texts: Vec<String> = Vec::new();
        let mut all_intents: Vec<Intent> = Vec::new();

        for (intent, texts) in &template_defs {
            for text in texts {
                all_texts.push(format!("search_query: {}", text));
                all_intents.push(intent.clone());
            }
        }

        // Uma única chamada HTTP para todos os templates
        let all_embeddings = embedder.embed_batch(&all_texts).await?;

        let templates: Vec<IntentTemplate> = all_intents
            .into_iter()
            .zip(all_embeddings)
            .map(|(intent, embedding)| IntentTemplate { intent, embedding })
            .collect();

        Ok(Self { templates })
    }

    /// Classifica o intent de uma mensagem do usuário.
    ///
    /// ## Estratégia (2 fases)
    ///
    /// ### Fase 1: Heurísticas Rápidas (~0ms)
    /// Verifica padrões simples no texto.
    ///
    /// ### Fase 2: Fallback por Embedding
    /// Compara o embedding da mensagem com os templates pré-computados.
    /// Só aceita se similaridade > 0.65, senão retorna `Narrating`.
    pub async fn classify(&self, text: &str, embedder: &Embedder) -> Result<Intent> {
        let text_lower = text.to_lowercase().trim().to_string();

        // ─── Fase 1: Heurísticas rápidas ─────────────────────────
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
        let embedding = embedder.embed(&format!("search_query: {}", text)).await?;
        let mut best_intent = Intent::Narrating;
        let mut best_score = 0.0f32;

        for template in &self.templates {
            let score = cosine_similarity(&embedding, &template.embedding);
            if score > best_score {
                best_score = score;
                best_intent = template.intent.clone();
            }
        }

        if best_score > 0.65 {
            Ok(best_intent)
        } else {
            Ok(Intent::Narrating)
        }
    }
}
