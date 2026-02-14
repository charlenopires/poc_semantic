//! # Módulo Core — Tipos Fundamentais do Domínio
//!
//! Este módulo agrupa os **tipos fundamentais** que formam a base do sistema
//! de conhecimento semântico. Tudo no Cultivo Epistêmico gira em torno destes tipos:
//!
//! - [`TruthValue`] — Grau de verdade baseado na lógica NARS
//! - [`Concept`] — Unidade atômica de conhecimento (ex: "fotossíntese", "Rust")
//! - [`ConceptState`] — Ciclo de vida de um conceito (Ativo → Dormente → Esmaecendo → Arquivado)
//! - [`Link`] — Relação N-ária entre conceitos (ex: "Rust" → "linguagem de programação")
//! - [`LinkKind`] — Tipo de relação semântica (Herança, Similaridade, Implicação, etc.)
//! - [`KnowledgeBase`] — Contêiner central que armazena todos os conceitos e links
//!
//! ## Analogia com o Mundo Real
//!
//! Pense na [`KnowledgeBase`] como um **jardim**:
//! - Cada [`Concept`] é uma **planta** — nasce, cresce com energia, e pode murchar
//! - Cada [`Link`] é uma **raiz** conectando plantas — mostra como ideias se relacionam
//! - O [`TruthValue`] é a **saúde** da planta — quanto mais evidência, mais confiante
//!
//! ## Exemplo de Uso
//!
//! ```rust
//! use crate::core::{Concept, TruthValue, KnowledgeBase, Link, LinkKind, Participant, Role};
//!
//! let mut kb = KnowledgeBase::new();
//!
//! // Criar um conceito
//! let conceito = Concept::new("Fotossíntese".to_string(), TruthValue::proto());
//! let id = kb.add_concept(conceito);
//!
//! // Buscar por label
//! let encontrado = kb.find_concept_by_label("fotossíntese");
//! ```

/// Sub-módulo com a implementação de [`TruthValue`] — grau de verdade NARS.
pub mod truth_value;

/// Sub-módulo com a implementação de [`Concept`] e [`ConceptState`].
pub mod concept;

/// Sub-módulo com a implementação de [`Link`], [`LinkKind`], [`Participant`] e [`Role`].
pub mod link;

/// Sub-módulo com a implementação de [`KnowledgeBase`] — contêiner central.
pub mod knowledge_base;

// Re-exports para conveniência — permite usar `crate::core::TruthValue` diretamente.
pub use truth_value::TruthValue;
pub use concept::{Concept, ConceptState};
pub use link::{Link, LinkKind, Participant, Role};
pub use knowledge_base::KnowledgeBase;
