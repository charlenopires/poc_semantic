//! # Concept — Unidade Atômica de Conhecimento
//!
//! Um [`Concept`] é a menor unidade de conhecimento no Cultivo Epistêmico.
//! Cada conceito representa **uma ideia, entidade ou fato** extraído da
//! linguagem natural do usuário.
//!
//! ## Analogia: A Semente no Jardim
//!
//! Pense em cada conceito como uma **planta no jardim epistêmico**:
//! - Nasce quando o usuário menciona algo pela primeira vez ("semeadura")
//! - Recebe **energia** cada vez que é mencionado ("reforço / fotossíntese")
//! - A energia **decai** naturalmente ao longo do tempo ("poda natural")
//! - Passa por ciclos de vida: **Ativo → Dormente → Esmaecendo → Arquivado**
//!
//! ## Campos Principais
//!
//! | Campo | Tipo | Descrição |
//! |-------|------|-----------|
//! | `id` | UUID | Identificador único gerado automaticamente |
//! | `label` | String | Nome legível do conceito (ex: "Fotossíntese") |
//! | `truth` | [TruthValue] | Grau de verdade NARS (frequency + confidence) |
//! | `energy` | f64 | Nível de energia (0.0 a 1.0) — determina o estado |
//! | `state` | [ConceptState] | Ciclo de vida atual |
//! | `embedding` | Option<Vec<f32>> | Vetor de embeddings BERTimbau (768 dimensões) |
//! | `mention_count` | u32 | Quantas vezes foi mencionado pelo usuário |
//!
//! ## Exemplo de Uso
//!
//! ```rust
//! use crate::core::{Concept, TruthValue, ConceptState};
//!
//! // Criar novo conceito
//! let mut conceito = Concept::new("Fotossíntese".to_string(), TruthValue::proto());
//! assert_eq!(conceito.state, ConceptState::Active);
//! assert!(conceito.energy > 0.5);
//!
//! // Reforçar (quando o usuário menciona novamente)
//! conceito.reinforce();
//! assert_eq!(conceito.mention_count, 2);
//!
//! // Decair (passagem do tempo)
//! for _ in 0..50 {
//!     conceito.decay(0.95);
//! }
//! // Após muitos ciclos de decaimento, o conceito fica Dormente ou Esmaecendo
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::TruthValue;

/// Alias de tipo para o identificador de um [Concept].
///
/// Utiliza UUID v4 para garantir unicidade sem coordenação central.
pub type ConceptId = Uuid;

/// Ciclo de vida de um conceito no jardim epistêmico.
///
/// Os estados formam uma progressão natural, como uma planta:
///
/// ```text
/// 🌿 Active (e > 0.5)  →  💤 Dormant (0.2 < e ≤ 0.5)  →  🍂 Fading (e ≤ 0.2)  →  📦 Archived
/// ```
///
/// - **Active**: O conceito está "vivo" — foi mencionado recentemente, tem alta energia
/// - **Dormant**: O conceito está "dormindo" — não foi mencionado há algum tempo
/// - **Fading**: O conceito está "murchando" — energia muito baixa, candidato a ser esquecido
/// - **Archived**: O conceito foi arquivado permanentemente (não retorna mais)
///
/// A transição entre estados é automática, baseada no nível de energia do conceito.
/// O estado `Archived` é terminal — uma vez arquivado, não volta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConceptState {
    /// 🌿 Conceito ativo — energia > 0.5 — mencionado recentemente
    Active,
    /// 💤 Conceito dormente — 0.2 < energia ≤ 0.5 — não mencionado há algum tempo
    Dormant,
    /// 🍂 Conceito esmaecendo — energia ≤ 0.2 — quase esquecido
    Fading,
    /// 📦 Conceito arquivado — terminal, não retorna
    Archived,
}

impl ConceptState {
    /// Retorna a classe CSS correspondente ao estado.
    ///
    /// Usada nos templates HTML maud para estilizar conceitos
    /// com cores de acordo com seu ciclo de vida.
    pub fn css_class(&self) -> &'static str {
        match self {
            ConceptState::Active => "active",
            ConceptState::Dormant => "dormant",
            ConceptState::Fading => "fading",
            ConceptState::Archived => "archived",
        }
    }

    /// Retorna o label legível em PT-BR do estado.
    ///
    /// Usado na interface do usuário para exibir o estado atual
    /// de cada conceito de forma amigável.
    pub fn label(&self) -> &'static str {
        match self {
            ConceptState::Active => "Ativo",
            ConceptState::Dormant => "Dormente",
            ConceptState::Fading => "Esmaecendo",
            ConceptState::Archived => "Arquivado",
        }
    }
}

/// Unidade atômica de conhecimento no Cultivo Epistêmico.
///
/// Cada conceito é uma "semente plantada" no jardim de conhecimento.
/// Ele carrega informações sobre:
///
/// - **Identidade**: `id` (UUID) e `label` (nome legível)
/// - **Crença**: `truth` ([TruthValue]) — o quanto o sistema "acredita" neste conceito
/// - **Vitalidade**: `energy` (0.0-1.0) — quão "vivo" o conceito está
/// - **Semântica**: `embedding` — vetor de 768 dimensões (BERTimbau) para busca por similaridade
/// - **Histórico**: `mention_count`, `created_at`, `last_mentioned`
///
/// ## Ciclo de Vida Típico
///
/// 1. Usuário menciona "Fotossíntese" → `Concept::new("Fotossíntese", TruthValue::proto())`
/// 2. Conceito nasce com `energy = 0.8` e `state = Active`
/// 3. Cada menção subsequente → `reinforce()` — energia sobe, count incrementa
/// 4. A cada ciclo do orquestrador → `decay(0.95)` — energia diminui 5%
/// 5. Quando energia cai abaixo de 0.5 → estado muda para `Dormant`
/// 6. Quando energia cai abaixo de 0.2 → estado muda para `Fading`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Concept {
    /// Identificador único (UUID v4) — gerado automaticamente na criação.
    pub id: ConceptId,

    /// Nome legível do conceito (ex: "Fotossíntese", "Rust", "Inteligência Artificial").
    /// Preserva a capitalização original fornecida pelo usuário.
    pub label: String,

    /// Grau de verdade NARS — combina frequency (proporção positiva)
    /// e confidence (estabilidade da avaliação). Veja [`TruthValue`].
    pub truth: TruthValue,

    /// Nível de energia do conceito (0.0 a 1.0).
    ///
    /// Determina o estado de vida do conceito:
    /// - `> 0.5` → Active (vivo, mencionado recentemente)
    /// - `0.2 < e ≤ 0.5` → Dormant (dormindo)
    /// - `≤ 0.2` → Fading (esmaecendo)
    ///
    /// Inicia em `0.8` ao ser criado. Aumenta com `reinforce()`,
    /// diminui com `decay()`.
    pub energy: f64,

    /// Estado atual do ciclo de vida. Veja [`ConceptState`].
    pub state: ConceptState,

    /// Vetor de embeddings BERTimbau (768 dimensões).
    ///
    /// Usado para busca semântica por similaridade via cosine similarity.
    /// É `None` quando o conceito foi criado antes do modelo terminar de carregar,
    /// ou quando o texto não pôde ser embeddado.
    pub embedding: Option<Vec<f32>>,

    /// Contagem de menções — quantas vezes o usuário mencionou este conceito.
    /// Inicia em 1 (criação conta como primeira menção).
    pub mention_count: u32,

    /// Timestamp de quando o conceito foi criado.
    pub created_at: DateTime<Utc>,

    /// Timestamp da última vez que o conceito foi mencionado pelo usuário.
    /// Atualizado por `reinforce()`.
    pub last_mentioned: DateTime<Utc>,
}

impl Concept {
    /// Cria um novo conceito com os valores iniciais padrão.
    ///
    /// Um conceito recém-criado:
    /// - Tem `energy = 0.8` (alto, próximo ao máximo de 1.0)
    /// - Está no estado `Active`
    /// - Não tem embedding (será preenchido pelo NLU pipeline)
    /// - Tem `mention_count = 1` (a criação conta como primeira menção)
    /// - Timestamps de criação e última menção são `now()`
    ///
    /// # Parâmetros
    ///
    /// - `label` — nome legível do conceito (ex: "Fotossíntese")
    /// - `truth` — grau de verdade inicial (normalmente `TruthValue::proto()`)
    pub fn new(label: String, truth: TruthValue) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            label,
            truth,
            energy: 0.8,
            state: ConceptState::Active,
            embedding: None,
            mention_count: 1,
            created_at: now,
            last_mentioned: now,
        }
    }

    /// Reforça o conceito — chamado cada vez que o usuário menciona este conceito.
    ///
    /// Efeitos:
    /// - **Energia** aumenta em +0.3, limitada a 1.0
    /// - **Mention count** incrementa em 1
    /// - **Timestamp** de última menção é atualizado para agora
    /// - **Estado** é recalculado (pode voltar de Dormant para Active)
    ///
    /// O reforço é a "fotossíntese" do conceito — cada menção
    /// é como receber luz solar, dando mais energia à planta.
    pub fn reinforce(&mut self) {
        // Energia nunca ultrapassa 1.0 (clamped pelo .min())
        self.energy = (self.energy + 0.3).min(1.0);
        self.mention_count += 1;
        self.last_mentioned = Utc::now();
        // Recalcula estado baseado na nova energia
        self.update_state();
    }

    /// Decai a energia do conceito ao longo do tempo.
    ///
    /// Chamado periodicamente pelo [`Orchestrator`] — representa a
    /// "poda natural" do jardim epistêmico. Conceitos não mencionados
    /// perdem energia gradualmente.
    ///
    /// # Parâmetros
    ///
    /// - `factor` — fator multiplicativo (ex: 0.95 = perda de 5% por ciclo)
    ///
    /// # Exemplo
    ///
    /// Após 10 ciclos com fator 0.95:
    /// `energy = 0.8 × 0.95^10 ≈ 0.48` — conceito vai de Active para Dormant
    pub fn decay(&mut self, factor: f64) {
        // Energia nunca fica negativa (clamped pelo .max())
        self.energy = (self.energy * factor).max(0.0);
        // Recalcula estado baseado na nova energia
        self.update_state();
    }

    /// Atualiza o estado do conceito baseado no nível de energia atual.
    ///
    /// Regras de transição:
    /// - Se `Archived` → **não muda** (estado terminal)
    /// - Se `energy > 0.5` → `Active`
    /// - Se `0.2 < energy ≤ 0.5` → `Dormant`
    /// - Se `energy ≤ 0.2` → `Fading`
    ///
    /// Esta função é chamada automaticamente por `reinforce()` e `decay()`.
    pub fn update_state(&mut self) {
        // Archived é terminal — nunca volta
        if self.state == ConceptState::Archived {
            return;
        }
        self.state = if self.energy > 0.5 {
            ConceptState::Active
        } else if self.energy > 0.2 {
            ConceptState::Dormant
        } else {
            ConceptState::Fading
        };
    }
}
