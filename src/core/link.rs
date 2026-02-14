//! # Link — Relação N-ária Entre Conceitos
//!
//! Um [`Link`] representa uma **relação semântica** entre dois ou mais conceitos
//! na base de conhecimento. É o equivalente de uma "aresta" em um grafo de conhecimento,
//! mas com suporte para relações N-árias (mais de dois participantes).
//!
//! ## Analogia: Raízes no Jardim
//!
//! Se os [`Concept`](super::Concept)s são as plantas, os Links são as **raízes** que
//! conectam plantas — mostrando como ideias se relacionam no subsolo do conhecimento.
//!
//! ## Tipos de Relação ([`LinkKind`])
//!
//! | Tipo | Descrição | Exemplo |
//! |------|-----------|---------|
//! | `Inheritance` | "é um" | "Gato é um Animal" |
//! | `Similarity` | "≈ semelhante a" | "Gato ≈ Tigre" |
//! | `Implication` | "implica ⇒" | "Chuva ⇒ Solo Molhado" |
//! | `Equivalence` | "equivale ⇔" | "H₂O ⇔ Água" |
//! | `PartOf` | "parte de" | "Roda parte de Carro" |
//! | `HasProperty` | "tem propriedade" | "Sol tem brilho" |
//! | `InstanceOf` | "instância de" | "Rex instância de Cão" |
//! | `Catalyzes` | "catalisa" | "Enzima catalisa Reação" |
//! | `Inhibits` | "inibe" | "Veneno inibe Crescimento" |
//! | `Custom` | Personalizado | Qualquer outro tipo de relação |
//!
//! ## Papéis dos Participantes ([`Role`])
//!
//! Cada participante de um Link tem um **papel semântico** que descreve
//! como ele participa da relação:
//!
//! - **Subject** — quem pratica/origina a ação
//! - **Object** — quem recebe/sofre a ação
//! - **Cause** — a causa da relação
//! - **Effect** — o efeito da relação
//! - **Context** — contexto em que a relação ocorre
//!
//! ## Exemplo
//!
//! ```rust
//! use crate::core::{Link, LinkKind, Participant, Role, TruthValue};
//! use uuid::Uuid;
//!
//! let link = Link::new(
//!     LinkKind::Implication,
//!     vec![
//!         Participant { concept_id: Uuid::new_v4(), role: Role::Subject },
//!         Participant { concept_id: Uuid::new_v4(), role: Role::Object },
//!     ],
//!     TruthValue::new(0.9, 0.7),
//! );
//! ```

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::concept::ConceptId;
use super::TruthValue;

/// Alias de tipo para o identificador de um [Link].
///
/// Utiliza UUID v4 para garantir unicidade.
pub type LinkId = Uuid;

/// Tipo de relação semântica entre conceitos.
///
/// Baseado nos tipos de relação do NARS, com adições para relações
/// causais (Catalyzes, Inhibits) e composição (PartOf).
///
/// Cada tipo tem um [`label()`](LinkKind::label) legível em PT-BR
/// que é usado para exibição na interface.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkKind {
    /// "é um" — relação de herança/classificação.
    /// Exemplo: "Gato é um Animal"
    Inheritance,

    /// "≈" — relação de similaridade semântica.
    /// Exemplo: "Gato ≈ Tigre"
    Similarity,

    /// "⇒" — relação de implicação/causalidade.
    /// Exemplo: "Chuva ⇒ Solo Molhado"
    Implication,

    /// "⇔" — relação de equivalência bidirecional.
    /// Exemplo: "H₂O ⇔ Água"
    Equivalence,

    /// "parte de" — relação mereológica (todo/parte).
    /// Exemplo: "Roda parte de Carro"
    PartOf,

    /// "tem" — relação de propriedade/atributo.
    /// Exemplo: "Sol tem brilho"
    HasProperty,

    /// "instância de" — relação de instanciação.
    /// Exemplo: "Rex instância de Cão"
    InstanceOf,

    /// "catalisa" — relação de catálise/aceleração positiva.
    /// Exemplo: "Enzima catalisa Reação"
    Catalyzes,

    /// "inibe" — relação de inibição/bloqueio.
    /// Exemplo: "Veneno inibe Crescimento"
    Inhibits,

    /// Tipo personalizado — qualquer outra relação descrita como string.
    /// Usado para relações que não se encaixam nos tipos pré-definidos.
    Custom(String),
}

impl LinkKind {
    /// Retorna o label legível em PT-BR do tipo de relação.
    ///
    /// Usado na interface do usuário e na descrição de links
    /// para exibir relações de forma amigável.
    pub fn label(&self) -> &str {
        match self {
            LinkKind::Inheritance => "é um",
            LinkKind::Similarity => "≈",
            LinkKind::Implication => "⇒",
            LinkKind::Equivalence => "⇔",
            LinkKind::PartOf => "parte de",
            LinkKind::HasProperty => "tem",
            LinkKind::InstanceOf => "instância de",
            LinkKind::Catalyzes => "catalisa",
            LinkKind::Inhibits => "inibe",
            LinkKind::Custom(s) => s.as_str(),
        }
    }
}

/// Papel semântico de um participante em um [Link].
///
/// Define **como** um conceito participa de uma relação.
/// Inspirado nos papéis temáticos da linguística.
///
/// ## Exemplo
///
/// Na frase "Chuva causa enchente":
/// - "Chuva" → `Role::Cause`
/// - "enchente" → `Role::Effect`
///
/// Na frase "Gato é um animal":
/// - "Gato" → `Role::Subject`
/// - "animal" → `Role::Object`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Sujeito — quem pratica ou origina a ação
    Subject,
    /// Objeto — quem recebe ou sofre a ação
    Object,
    /// Causa — elemento que origina o evento
    Cause,
    /// Efeito — elemento resultante do evento
    Effect,
    /// Contexto — circunstância em que a relação ocorre
    Context,
    /// Qualificador — modificador ou atributo da relação
    Qualifier,
    /// Origem — ponto de partida de uma transformação
    Source,
    /// Destino — ponto de chegada de uma transformação
    Target,
    /// Instrumento — meio pelo qual a ação é realizada
    Instrument,
}

impl Role {
    /// Retorna o label legível em PT-BR do papel semântico.
    pub fn label(&self) -> &str {
        match self {
            Role::Subject => "Sujeito",
            Role::Object => "Objeto",
            Role::Cause => "Causa",
            Role::Effect => "Efeito",
            Role::Context => "Contexto",
            Role::Qualifier => "Qualificador",
            Role::Source => "Origem",
            Role::Target => "Destino",
            Role::Instrument => "Instrumento",
        }
    }
}

/// Um participante em um [Link] N-ário.
///
/// Associa um conceito a um papel semântico dentro da relação.
/// Cada link pode ter múltiplos participantes com papéis diferentes.
///
/// ## Exemplo
///
/// ```rust
/// let participante = Participant {
///     concept_id: algum_uuid,
///     role: Role::Subject,
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Participant {
    /// ID do conceito que participa desta relação.
    /// Referência ao [`Concept`](super::Concept) na [`KnowledgeBase`](super::KnowledgeBase).
    pub concept_id: ConceptId,

    /// Papel semântico que este conceito desempenha na relação.
    pub role: Role,
}

/// Relação N-ária entre [Concept](super::Concept)s na base de conhecimento.
///
/// Um Link conecta dois ou mais conceitos com um tipo de relação semântica
/// ([`LinkKind`]) e um grau de verdade ([`TruthValue`]).
///
/// ## Estrutura
///
/// ```text
/// Link {
///     kind: Implication ("⇒"),
///     participants: [
///         Participant { concept: "Chuva", role: Subject },
///         Participant { concept: "Enchente", role: Object },
///     ],
///     truth: ⟨0.90, 0.70⟩,
///     energy: 0.80,
/// }
/// ```
///
/// ## Energia e Decaimento
///
/// Assim como os conceitos, links têm **energia** que decai ao longo do tempo.
/// Links com baixa energia são menos relevantes para inferência e visualização.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    /// Identificador único (UUID v4).
    pub id: LinkId,

    /// Tipo de relação semântica (Inheritance, Similarity, etc.).
    pub kind: LinkKind,

    /// Lista de participantes — conceitos e seus papéis na relação.
    /// Para links binários simples (maioria dos casos): Subject + Object.
    /// Para links causais: Cause + Effect.
    pub participants: Vec<Participant>,

    /// Grau de verdade NARS — quão certo o sistema está dessa relação.
    pub truth: TruthValue,

    /// Nível de energia (0.0 a 1.0) — determina a relevância temporal.
    /// Inicia em 0.8, decai ao longo do tempo como os conceitos.
    pub energy: f64,
}

impl Link {
    /// Cria um novo Link com energia padrão de 0.8.
    ///
    /// # Parâmetros
    ///
    /// - `kind` — tipo de relação semântica
    /// - `participants` — lista de participantes com seus papéis
    /// - `truth` — grau de verdade NARS da relação
    pub fn new(kind: LinkKind, participants: Vec<Participant>, truth: TruthValue) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            participants,
            truth,
            energy: 0.8,
        }
    }

    /// Retorna o [ConceptId] do participante com papel `Subject`, se houver.
    ///
    /// Usado extensivamente no motor de inferência para acessar
    /// o sujeito de relações binárias.
    pub fn subject(&self) -> Option<ConceptId> {
        self.participants
            .iter()
            .find(|p| p.role == Role::Subject)
            .map(|p| p.concept_id)
    }

    /// Retorna o [ConceptId] do participante com papel `Object`, se houver.
    ///
    /// Usado extensivamente no motor de inferência para acessar
    /// o objeto de relações binárias.
    pub fn object(&self) -> Option<ConceptId> {
        self.participants
            .iter()
            .find(|p| p.role == Role::Object)
            .map(|p| p.concept_id)
    }

    /// Retorna o [ConceptId] do participante com papel `Cause`, se houver.
    ///
    /// Usado para links causais (Implication, Catalyzes).
    pub fn cause(&self) -> Option<ConceptId> {
        self.participants
            .iter()
            .find(|p| p.role == Role::Cause)
            .map(|p| p.concept_id)
    }

    /// Retorna o [ConceptId] do participante com papel `Effect`, se houver.
    ///
    /// Usado para links causais (Implication, Catalyzes).
    pub fn effect(&self) -> Option<ConceptId> {
        self.participants
            .iter()
            .find(|p| p.role == Role::Effect)
            .map(|p| p.concept_id)
    }

    /// Decai a energia do link ao longo do tempo.
    ///
    /// Similar ao decaimento de conceitos, mas links não têm estado —
    /// simplesmente ficam menos relevantes para inferência.
    ///
    /// # Parâmetros
    ///
    /// - `factor` — fator multiplicativo (ex: 0.95 = -5% por ciclo)
    pub fn decay(&mut self, factor: f64) {
        // Energia nunca fica negativa
        self.energy = (self.energy * factor).max(0.0);
    }
}
