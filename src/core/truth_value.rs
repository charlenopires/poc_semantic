//! # TruthValue — Grau de Verdade NARS
//!
//! Implementação do sistema de **grau de verdade** baseado na lógica
//! **NARS** (Non-Axiomatic Reasoning System), criada por Pei Wang.
//!
//! ## O que é NARS?
//!
//! NARS é uma lógica projetada para raciocínio sob **incerteza** e **recursos limitados**.
//! Diferente da lógica clássica (verdadeiro/falso), NARS trabalha com graus de verdade
//! que combinam duas dimensões:
//!
//! - **Frequency (f)**: "Qual proporção da evidência é positiva?"
//!   - `f = w⁺ / (w⁺ + w⁻)` — varia de 0.0 a 1.0
//!   - Exemplo: Se 8 de 10 observações são positivas, `f = 0.8`
//!
//! - **Confidence (c)**: "Quão estável é essa avaliação?"
//!   - `c = (w⁺ + w⁻) / (w⁺ + w⁻ + k)` — varia de 0.0 a ~1.0
//!   - Quanto mais evidência total, maior a confiança
//!   - `k` é o "horizonte evidencial" (padrão = 1.0 em NARS)
//!
//! ## Representação Interna
//!
//! Internamente, o [`TruthValue`] armazena **evidência** (w⁺, w⁻) em vez de (f, c).
//! Isso facilita a **revisão** (merge de evidências), que é simplesmente somar evidências.
//!
//! ## Regras de Inferência
//!
//! O TruthValue implementa as regras de inferência NARS:
//!
//! | Regra | Padrão | Resultado |
//! |-------|--------|-----------|
//! | **Revisão** | Merge de evidências | Soma w⁺ e w⁻ |
//! | **Dedução** | S→M + M→P ⊢ S→P | f = f₁×f₂, c ↓ |
//! | **Indução** | M→P + M→S ⊢ S→P | Generalização |
//! | **Abdução** | P→M + S→M ⊢ S→P | Hipótese |
//!
//! ## Exemplo
//!
//! ```rust
//! use crate::core::TruthValue;
//!
//! // Conceito recém-criado com baixa confiança
//! let proto = TruthValue::proto(); // ⟨0.50, 0.10⟩
//!
//! // Observação direta positiva (alta confiança)
//! let obs = TruthValue::observed(true); // ⟨1.00, 0.90⟩
//!
//! // Revisão: combinar evidências → confiança aumenta
//! let revisado = proto.revision(&obs);
//! assert!(revisado.confidence() > proto.confidence());
//! ```
//!
//! ## Caso de Uso Real
//!
//! Quando o usuário diz "Fotossíntese é o processo de conversão de luz em energia":
//! 1. O conceito "Fotossíntese" é criado com `TruthValue::proto()` (⟨0.50, 0.10⟩)
//! 2. Se o usuário confirma ("sim, correto"), aplica-se `TruthValue::observed(true)`
//! 3. A revisão combina as evidências, aumentando frequency e confidence
//! 4. Se o usuário nega, aplica-se `TruthValue::observed(false)`, diminuindo frequency

use std::fmt;

use serde::{Deserialize, Serialize};

/// Parâmetro de horizonte evidencial (default = 1.0 em NARS).
///
/// Este valor controla quão rápido a confiança cresce com nova evidência.
/// Valores maiores exigem mais evidência para atingir alta confiança.
/// O valor padrão de 1.0 é o mesmo usado na implementação original NARS.
const EVIDENTIAL_HORIZON: f64 = 1.0;

/// Grau de verdade baseado em NARS (Non-Axiomatic Logic).
///
/// Representa o **nível de crença** do sistema sobre uma proposição.
/// Internamente armazena evidência (w⁺, w⁻) — positiva e negativa.
/// Externamente expõe (frequency, confidence) para fácil interpretação.
///
/// ## Fórmulas
///
/// - `frequency  = w⁺ / (w⁺ + w⁻)` — proporção de evidência positiva
/// - `confidence = (w⁺ + w⁻) / (w⁺ + w⁻ + k)` — estabilidade da avaliação
///
/// ## Display
///
/// O formato de exibição é `⟨frequency, confidence⟩`, por exemplo: `⟨0.80, 0.45⟩`
///
/// ## Exemplos
///
/// ```rust
/// let tv = TruthValue::new(0.8, 0.5);
/// println!("{}", tv); // ⟨0.80, 0.50⟩
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TruthValue {
    /// Evidência positiva (w⁺) — quantidade de observações a favor.
    positive_evidence: f64,
    /// Evidência negativa (w⁻) — quantidade de observações contra.
    negative_evidence: f64,
}

impl TruthValue {
    /// Cria um novo TruthValue a partir de **frequency** e **confidence**.
    ///
    /// Os valores são automaticamente limitados (clamped):
    /// - `frequency`: entre 0.0 e 1.0
    /// - `confidence`: entre 0.0 e 0.9999 (nunca pode ser 1.0 em NARS)
    ///
    /// Internamente, converte (f, c) para (w⁺, w⁻) usando:
    /// - `w_total = k × c / (1 - c)`
    /// - `w⁺ = w_total × f`
    /// - `w⁻ = w_total × (1 - f)`
    ///
    /// # Parâmetros
    ///
    /// - `frequency` — proporção de evidência positiva (0.0 a 1.0)
    /// - `confidence` — estabilidade da avaliação (0.0 a ~1.0)
    ///
    /// # Exemplo
    ///
    /// ```rust
    /// let tv = TruthValue::new(0.9, 0.8);
    /// assert!((tv.frequency() - 0.9).abs() < 0.01);
    /// assert!((tv.confidence() - 0.8).abs() < 0.01);
    /// ```
    pub fn new(frequency: f64, confidence: f64) -> Self {
        // Clamp para garantir valores dentro dos limites NARS
        let frequency = frequency.clamp(0.0, 1.0);
        let confidence = confidence.clamp(0.0, 0.9999);

        let k = EVIDENTIAL_HORIZON;
        // Calcula o total de evidência a partir da confiança
        let w_total = k * confidence / (1.0 - confidence);
        Self {
            positive_evidence: w_total * frequency,
            negative_evidence: w_total * (1.0 - frequency),
        }
    }

    /// Cria um **proto truth value** — usado para conceitos recém-criados.
    ///
    /// Representa "sabe-se pouco" sobre o conceito: frequency neutra (0.5)
    /// e confiança muito baixa (0.1).
    ///
    /// Em termos práticos, significa: "Acabamos de ouvir sobre isso,
    /// não temos certeza se é verdade ou não."
    ///
    /// # Exemplo
    ///
    /// ```rust
    /// let tv = TruthValue::proto();
    /// assert!((tv.frequency() - 0.5).abs() < 0.01);
    /// assert!((tv.confidence() - 0.1).abs() < 0.01);
    /// ```
    pub fn proto() -> Self {
        Self::new(0.5, 0.1)
    }

    /// Cria um truth value de **observação direta** — alta confiança (0.9).
    ///
    /// Usado quando o usuário **confirma ou nega** explicitamente algo.
    /// Representa evidência forte baseada em observação direta.
    ///
    /// # Parâmetros
    ///
    /// - `positive` — `true` para confirmação (f=1.0), `false` para negação (f=0.0)
    ///
    /// # Exemplo
    ///
    /// ```rust
    /// let confirmacao = TruthValue::observed(true);  // ⟨1.00, 0.90⟩
    /// let negacao = TruthValue::observed(false);       // ⟨0.00, 0.90⟩
    /// ```
    pub fn observed(positive: bool) -> Self {
        if positive {
            Self::new(1.0, 0.9)
        } else {
            Self::new(0.0, 0.9)
        }
    }

    /// Retorna a **frequency** — proporção de evidência positiva.
    ///
    /// Varia de 0.0 (toda evidência é negativa) a 1.0 (toda evidência é positiva).
    /// Se não houver evidência, retorna 0.5 (neutro).
    ///
    /// Fórmula: `f = w⁺ / (w⁺ + w⁻)`
    pub fn frequency(&self) -> f64 {
        let total = self.positive_evidence + self.negative_evidence;
        if total == 0.0 {
            0.5 // Sem evidência → neutro
        } else {
            self.positive_evidence / total
        }
    }

    /// Retorna a **confidence** — estabilidade da avaliação.
    ///
    /// Varia de 0.0 (nenhuma evidência) até próximo de 1.0 (muita evidência).
    /// Nunca atinge 1.0 exatamente — sempre há espaço para nova evidência em NARS.
    ///
    /// Fórmula: `c = (w⁺ + w⁻) / (w⁺ + w⁻ + k)`
    pub fn confidence(&self) -> f64 {
        let total = self.positive_evidence + self.negative_evidence;
        total / (total + EVIDENTIAL_HORIZON)
    }

    /// Retorna a **expectation** — valor esperado combinando frequency e confidence.
    ///
    /// Útil para ordenar conceitos por "importância" percebida.
    ///
    /// Fórmula: `e = c × (f − 0.5) + 0.5`
    ///
    /// - Se confidence é 0, expectation é 0.5 (neutro)
    /// - Se confidence é alta e frequency é alta, expectation é alta
    pub fn expectation(&self) -> f64 {
        self.confidence() * (self.frequency() - 0.5) + 0.5
    }

    /// **Regra de Revisão NARS** — combina (merge) evidências independentes.
    ///
    /// Esta é a operação mais fundamental em NARS. Quando duas fontes independentes
    /// fornecem evidência sobre o mesmo conceito, simplesmente **somamos** as evidências.
    ///
    /// O resultado sempre tem **confiança maior** que qualquer um dos inputs individualmente,
    /// porque temos mais evidência total.
    ///
    /// # Caso de Uso
    ///
    /// Quando o usuário confirma ou nega um conceito, a observação é combinada
    /// com o truth value existente via revisão.
    ///
    /// # Exemplo
    ///
    /// ```rust
    /// let a = TruthValue::new(0.8, 0.3);
    /// let b = TruthValue::new(0.7, 0.4);
    /// let revisado = a.revision(&b);
    /// // A confiança do resultado é maior que ambas as entradas
    /// assert!(revisado.confidence() > a.confidence());
    /// assert!(revisado.confidence() > b.confidence());
    /// ```
    pub fn revision(&self, other: &TruthValue) -> TruthValue {
        TruthValue {
            positive_evidence: self.positive_evidence + other.positive_evidence,
            negative_evidence: self.negative_evidence + other.negative_evidence,
        }
    }

    /// **Regra de Dedução NARS** — `S→M + M→P ⊢ S→P`
    ///
    /// Se sabemos que "S implica M" e "M implica P", podemos deduzir que
    /// "S provavelmente implica P" — mas com confiança reduzida.
    ///
    /// - `f = f₁ × f₂` — frequency diminui (mais passos, mais incerteza)
    /// - `c = f₁ × f₂ × c₁ × c₂` — confidence diminui significativamente
    ///
    /// # Exemplo Real
    ///
    /// ```text
    /// "Chuva causa enchente" ⟨0.9, 0.8⟩ (S→M)
    /// "Enchente causa dano" ⟨0.8, 0.7⟩  (M→P)
    /// ⊢ "Chuva causa dano" ⟨0.72, ...⟩  (S→P) — confiança menor
    /// ```
    pub fn deduction(&self, other: &TruthValue) -> TruthValue {
        let f = self.frequency() * other.frequency();
        let c = self.frequency() * other.frequency() * self.confidence() * other.confidence();
        TruthValue::new(f, c.min(0.9999))
    }

    /// **Regra de Indução NARS** — `M→P + M→S ⊢ S→P`
    ///
    /// Se M leva tanto a P quanto a S, podemos **generalizar** que S e P
    /// estão relacionados. Isso é uma forma de aprendizado por observação.
    ///
    /// - `f = f₂` — frequency do segundo argumento
    /// - `c = f₁ × c₁ × c₂ / (f₁ × c₁ × c₂ + k)` — confiança moderada
    ///
    /// # Exemplo Real
    ///
    /// ```text
    /// "Motor potente" → "alta velocidade" (M→P)
    /// "Motor potente" → "alto consumo"    (M→S)
    /// ⊢ "alto consumo" ≈ "alta velocidade" — indução (correlação)
    /// ```
    pub fn induction(&self, other: &TruthValue) -> TruthValue {
        let f = other.frequency();
        let w = self.frequency() * self.confidence() * other.confidence();
        let c = w / (w + EVIDENTIAL_HORIZON);
        TruthValue::new(f, c.min(0.9999))
    }

    /// **Regra de Abdução NARS** — `P→M + S→M ⊢ S→P`
    ///
    /// Se tanto P quanto S levam a M, podemos formular a **hipótese**
    /// de que S está relacionado a P. Abdução é a forma mais fraca
    /// de inferência — gera hipóteses para investigação.
    ///
    /// - `f = f₁` — frequency do primeiro argumento
    /// - `c = f₂ × c₁ × c₂ / (f₂ × c₁ × c₂ + k)` — confiança baixa
    ///
    /// # Exemplo Real
    ///
    /// ```text
    /// "Exercício" → "saúde" (P→M)
    /// "Boa dieta" → "saúde" (S→M)
    /// ⊢ "Boa dieta" → "Exercício" — abdução (hipótese)
    /// ```
    pub fn abduction(&self, other: &TruthValue) -> TruthValue {
        let f = self.frequency();
        let w = other.frequency() * self.confidence() * other.confidence();
        let c = w / (w + EVIDENTIAL_HORIZON);
        TruthValue::new(f, c.min(0.9999))
    }
}

/// Formatação legível do TruthValue no formato `⟨frequency, confidence⟩`.
///
/// Exemplo: `⟨0.80, 0.45⟩` significa "80% positivo com 45% de confiança".
impl fmt::Display for TruthValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "⟨{:.2}, {:.2}⟩", self.frequency(), self.confidence())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifica que o proto truth value tem frequency ≈ 0.5 e confidence ≈ 0.1
    #[test]
    fn test_proto() {
        let tv = TruthValue::proto();
        assert!((tv.frequency() - 0.5).abs() < 0.01);
        assert!((tv.confidence() - 0.1).abs() < 0.01);
    }

    /// Verifica que a revisão sempre aumenta a confiança
    /// (porque estamos adicionando mais evidência)
    #[test]
    fn test_revision_increases_confidence() {
        let a = TruthValue::new(0.8, 0.3);
        let b = TruthValue::new(0.7, 0.4);
        let r = a.revision(&b);
        assert!(r.confidence() > a.confidence());
        assert!(r.confidence() > b.confidence());
    }

    /// Verifica que a dedução reduz tanto frequency quanto confidence
    /// (mais passos lógicos = mais incerteza)
    #[test]
    fn test_deduction() {
        let sm = TruthValue::new(0.9, 0.8);
        let mp = TruthValue::new(0.8, 0.7);
        let sp = sm.deduction(&mp);
        assert!(sp.frequency() < sm.frequency());
        assert!(sp.confidence() < sm.confidence());
    }
}
