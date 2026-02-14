//! # Módulo Inference — Motor de Inferência Lógica NARS
//!
//! Este módulo contém o **motor de inferência** do Cultivo Epistêmico,
//! responsável por derivar **novas relações** a partir das existentes
//! usando regras da lógica NARS.
//!
//! ## Analogia: A Fotossíntese do Jardim
//!
//! Se a semeadura adiciona conceitos, a inferência é a **fotossíntese** —
//! o processo que transforma matéria-prima (links existentes) em novos
//! conhecimentos (links inferidos), fazendo o jardim crescer organicamente.
//!
//! ## Regras Implementadas
//!
//! | Regra | Padrão | Resultado | Confiança |
//! |-------|--------|-----------|-----------|
//! | **Dedução** | S→M + M→P | S→P | Moderada |
//! | **Indução** | M→P + M→S | S ≈ P | Baixa |
//!
//! ## Exemplo
//!
//! ```text
//! KB contém: "Chuva" → "Enchente", "Enchente" → "Dano"
//! Inferência deduz: "Chuva" → "Dano" (com confiança menor)
//! ```
//!
//! Veja [`InferenceEngine`] para detalhes.

/// Sub-módulo com as regras de inferência NARS.
pub mod rules;

/// Re-export do motor de inferência para acesso via `crate::inference::InferenceEngine`.
pub use rules::InferenceEngine;
