//! # Extrator de Entidades — Heurísticas Linguísticas para PT-BR
//!
//! O [`EntityExtractor`] identifica **entidades candidatas** em texto livre
//! usando uma combinação de expressões regulares e heurísticas linguísticas
//! adaptadas para Português Brasileiro.
//!
//! ## O que é uma "Entidade"?
//!
//! No contexto deste sistema, uma entidade é qualquer trecho de texto que
//! pode representar um **conceito** na base de conhecimento. Exemplos:
//!
//! - Nomes próprios: "Carlos", "São Paulo"
//! - Termos técnicos: "inteligência artificial", "sustentabilidade"
//! - Conceitos abstratos: "urgência", "problema", "solução"
//!
//! ## Estratégia de Extração (4 etapas)
//!
//! O extrator aplica regras em **ordem de prioridade**, usando deduplicação
//! por lowercase para evitar repetições:
//!
//! | Prioridade | Estratégia | Exemplo |
//! |-----------|------------|---------|
//! | 1 | Texto entre aspas | `"inteligência artificial"` |
//! | 2 | Palavras capitalizadas | `Carlos`, `São Paulo` |
//! | 3 | N-grams (bigrams/trigrams) | `base conhecimento` |
//! | 4 | Palavras individuais ≥ 4 chars | `urgência`, `problema` |
//!
//! ## Filtros de Qualidade
//!
//! - **Stopwords**: Lista extensa de stopwords PT-BR (artigos, preposições, pronomes...)
//! - **Verbos**: Heurística por sufixo verbal (ando, endo, indo, ado, ido)
//! - **Comprimento mínimo**: 2+ chars para aspas, 3+ para capitalizadas, 4+ para individuais
//! - **Deduplicação**: Case-insensitive via HashSet

use regex::Regex;

/// Stopwords em Português Brasileiro para filtragem de entidades.
///
/// Lista curada de palavras funcionais que **não** representam conceitos:
/// artigos, preposições, pronomes, advérbios comuns, e verbos auxiliares.
///
/// ## Nota sobre a Lista
///
/// A lista inclui também algumas palavras de conteúdo que tendem a ser
/// ruído em extração de entidades, como "coisa", "vez", "dia".
/// Isso é uma decisão de design para reduzir falsos positivos.
const STOPWORDS: &[&str] = &[
    "o", "a", "os", "as", "um", "uma", "uns", "umas", "de", "do", "da", "dos", "das", "em", "no",
    "na", "nos", "nas", "por", "pelo", "pela", "pelos", "pelas", "para", "com", "sem", "sob",
    "sobre", "entre", "que", "se", "não", "sim", "mas", "ou", "e", "é", "são", "foi", "era",
    "ser", "ter", "há", "está", "eu", "ele", "ela", "nós", "eles", "elas", "me", "te", "lhe",
    "isso", "isto", "esse", "esta", "esse", "essa", "aquele", "aquela", "meu", "minha", "seu",
    "sua", "nosso", "nossa", "muito", "mais", "menos", "bem", "mal", "já", "ainda", "também",
    "então", "quando", "como", "onde", "porque", "porquê", "depois", "antes", "agora", "sempre",
    "nunca", "todo", "toda", "cada", "outro", "outra", "mesmo", "mesma", "próprio", "própria",
    "ao", "à", "aos", "às", "num", "numa", "dum", "duma", "qual", "quais", "quem",
    "até", "pode", "vai", "vou", "tem", "tinha", "acho", "aqui", "ali", "lá", "cá",
    "faz", "coisa", "vez", "vezes", "dia", "dias", "ligou", "disse", "falou",
    "causa", "principal", "atrasou", "atrasado", "atraso",
];

/// Sufixos verbais comuns em Português.
///
/// Usados pela heurística [`looks_like_verb()`] para filtrar
/// palavras que provavelmente são formas verbais (gerúndios e particípios).
///
/// - `ando` → gerúndio 1ª conjugação (falando, cantando)
/// - `endo` → gerúndio 2ª conjugação (correndo, fazendo)
/// - `indo` → gerúndio 3ª conjugação (partindo, saindo)
/// - `ado` → particípio 1ª conjugação (falado, cantado)
/// - `ido` → particípio 2ª/3ª conjugação (corrido, partido)
const VERB_SUFFIXES: &[&str] = &["ando", "endo", "indo", "ado", "ido"];

/// Palavras reais PT-BR de 2-4 chars que NÃO devem ser tratadas como fragmentos.
///
/// A heurística de normalização de palavras quebradas detecta tokens curtos (2-4 chars)
/// como possíveis fragmentos de palavras maiores. Esta lista protege palavras reais
/// comuns para evitar junções indevidas (ex: "caso alto" ficaria "casoalto" sem isso).
const SHORT_CONTENT_WORDS: &[&str] = &[
    // 2 chars
    "ar", "ir", "vi",
    // 3 chars
    "sol", "luz", "mar", "rio", "lei", "ato", "fim", "mal", "bem", "pai", "mãe",
    "céu", "dor", "paz", "voz", "uso", "boa", "bom", "cem",
    // 4 chars
    "caso", "alto", "base", "dado", "dose", "fase", "fato", "foco", "grau", "guia",
    "hora", "item", "lado", "lago", "lote", "mata", "meta", "modo", "muro", "nome",
    "nota", "obra", "pena", "peso", "piso", "polo", "rede", "rota", "sala", "taxa",
    "tema", "tipo", "topo", "vaga", "vida", "zona", "área", "água", "fogo", "eixo",
    "risco", "plano",
];

/// Extrator de entidades baseado em heurísticas linguísticas.
///
/// Usa duas expressões regulares compiladas uma única vez e reutilizadas:
///
/// - `quoted_re` — captura texto entre aspas (retas e curvas)
/// - `capitalized_re` — captura palavras capitalizadas (nomes próprios)
///
/// ## Exemplo de Uso
///
/// ```rust
/// let extractor = EntityExtractor::new();
/// let entities = extractor.extract("Carlos falou sobre inteligência artificial");
/// // entities: ["Carlos", "inteligência artificial"]
/// ```
pub struct EntityExtractor {
    /// Regex para capturar texto entre aspas (retas " e curvas " ").
    quoted_re: Regex,
    /// Regex para capturar palavras capitalizadas (possíveis nomes próprios),
    /// incluindo compostos com preposições (ex: "São Paulo do Brasil").
    capitalized_re: Regex,
}

impl EntityExtractor {
    /// Cria um novo extrator de entidades com regexes compiladas.
    ///
    /// As regexes são compiladas uma única vez e reutilizadas em todas
    /// as chamadas a [`extract()`](EntityExtractor::extract).
    pub fn new() -> Self {
        Self {
            // Captura texto entre aspas: "texto", "texto", ou 'texto'
            quoted_re: Regex::new(r#"["""]([^"""]+)["""]|'([^']+)'"#).unwrap(),
            // Captura palavras capitalizadas, incluindo compostos com preposições:
            // "Carlos", "São Paulo", "Universidade de São Paulo"
            capitalized_re: Regex::new(r"\b([A-ZÁÀÂÃÉÈÊÍÏÓÔÕÖÚÇ][a-záàâãéèêíïóôõöúçüñ]{2,})(?:\s+(?:de|do|da|dos|das)\s+[A-ZÁÀÂÃÉÈÊÍÏÓÔÕÖÚÇ][a-záàâãéèêíïóôõöúçüñ]+)*\b").unwrap(),
        }
    }

    /// Extrai entidades candidatas de um texto em Português.
    ///
    /// Aplica 4 estratégias em sequência, com deduplicação incremental:
    ///
    /// ## 1. Texto entre aspas (maior prioridade)
    ///
    /// Captura qualquer texto entre aspas — o usuário indicou explicitamente
    /// que é um termo importante.
    ///
    /// ## 2. Palavras capitalizadas
    ///
    /// Identifica possíveis nomes próprios. Inclui compostos com preposições
    /// como "São Paulo" ou "Universidade de São Paulo".
    ///
    /// ## 3. N-grams de content words (bigrams e trigrams)
    ///
    /// Janelas deslizantes de 2-3 palavras onde TODAS são content words
    /// (não stopwords, não verbos, >2 chars). Captura frases nominais como
    /// "base conhecimento" ou "inteligência artificial".
    ///
    /// ## 4. Palavras individuais ≥ 4 caracteres
    ///
    /// Fallback para termos simples como "urgência", "problema".
    /// O mínimo de 4 chars evita artigos e preposições residuais.
    ///
    /// # Retorno
    ///
    /// `Vec<String>` — entidades extraídas, sem duplicação, na ordem de prioridade
    pub fn extract(&self, text: &str) -> Vec<String> {
        let text = normalize_broken_words(text);
        let mut entities = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // ─── 1. Texto entre aspas ────────────────────────────────
        for cap in self.quoted_re.captures_iter(&text) {
            let entity = cap.get(1).or(cap.get(2)).map(|m| m.as_str().to_string());
            if let Some(e) = entity {
                let lower = e.to_lowercase();
                if !seen.contains(&lower) && e.len() > 1 {
                    seen.insert(lower);
                    entities.push(e);
                }
            }
        }

        // ─── 2. Palavras capitalizadas (nomes próprios) ──────────
        for cap in self.capitalized_re.captures_iter(&text) {
            let entity = cap.get(0).unwrap().as_str().to_string();
            let lower = entity.to_lowercase();
            if !seen.contains(&lower) && !is_stopword(&lower) && entity.len() > 2 {
                seen.insert(lower);
                entities.push(entity);
            }
        }

        // ─── 3. N-grams compostos (bigrams e trigrams) ──────────
        // Captura frases nominais compostas de content words
        let words: Vec<&str> = text.split_whitespace().collect();
        // Nota: text agora é String (normalizada), sem fragmentos quebrados
        for window_size in [2, 3] {
            if words.len() >= window_size {
                for window in words.windows(window_size) {
                    // Remove pontuação das bordas de cada palavra
                    let cleaned: Vec<&str> = window
                        .iter()
                        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
                        .collect();
                    // Todas as palavras devem ser "significativas"
                    let all_meaningful = cleaned.iter().all(|w| {
                        let wl = w.to_lowercase();
                        !is_stopword(&wl) && wl.len() >= 5 && !looks_like_verb(&wl)
                    });
                    if all_meaningful {
                        let phrase: String = cleaned.join(" ");
                        let lower = phrase.to_lowercase();
                        if !seen.contains(&lower) {
                            seen.insert(lower);
                            entities.push(phrase);
                        }
                    }
                }
            }
        }

        // ─── 4. Palavras individuais ≥ 4 caracteres ─────────────
        // Último recurso — captura substantivos comuns não capitalizados
        for word in &words {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
            let lower = clean.to_lowercase();
            if clean.len() >= 4
                && !is_stopword(&lower)
                && !looks_like_verb(&lower)
                && !seen.contains(&lower)
            {
                seen.insert(lower);
                entities.push(clean.to_string());
            }
        }

        entities
    }
}

/// Verifica se uma palavra (em lowercase) é uma stopword PT-BR.
///
/// Faz busca linear na lista [`STOPWORDS`]. Para uma lista de ~100 itens,
/// a busca linear é mais eficiente que um HashMap.
fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

/// Verifica se uma palavra (em lowercase) parece ser uma forma verbal.
///
/// Usa heurística simples: se a palavra termina com um dos sufixos verbais
/// comuns (`ando`, `endo`, `indo`, `ado`, `ido`), provavelmente é um verbo.
///
/// ## Falsos Positivos Possíveis
///
/// - "conteúdo" termina em "do" mas não é verbo
/// - A heurística aceita esse trade-off em favor da simplicidade
fn looks_like_verb(word: &str) -> bool {
    VERB_SUFFIXES.iter().any(|s| word.ends_with(s))
}

/// Verifica se uma palavra curta (2-4 chars) é uma palavra real conhecida.
fn is_known_short_word(word: &str) -> bool {
    SHORT_CONTENT_WORDS.contains(&word)
}

/// Determina se um token é provavelmente um fragmento de palavra quebrada.
///
/// Um token é considerado fragmento quando:
/// - Tem 2-4 caracteres
/// - Não é stopword, nem palavra curta conhecida, nem verbo
/// - É composto apenas por letras (sem números/pontuação)
fn is_probable_fragment(token: &str) -> bool {
    let lower = token.to_lowercase();
    let len = lower.chars().count();
    len >= 2
        && len <= 4
        && lower.chars().all(|c| c.is_alphabetic())
        && !is_stopword(&lower)
        && !is_known_short_word(&lower)
        && !looks_like_verb(&lower)
}

/// Normaliza palavras quebradas por espaços em texto extraído de PDF.
///
/// O `pdf_extract` frequentemente quebra palavras portuguesas em posições
/// arbitrárias: "arm azenagem", "Oper acio nal". Esta função detecta
/// fragmentos curtos e os junta com tokens adjacentes.
///
/// ## Algoritmo
///
/// Percorre tokens da esquerda para a direita. Ao detectar um fragmento provável
/// (2-4 chars, não é palavra real conhecida), acumula tokens subsequentes que
/// iniciam com minúscula até que:
/// - O acumulado tenha >= 6 chars E o próximo token não seja fragmento, OU
/// - O próximo token seja stopword, OU
/// - O próximo token inicie com maiúscula e não seja fragmento
///
/// ## Exemplos
///
/// ```text
/// "arm azenagem"              → "armazenagem"
/// "Oper acio nal"             → "Operacional"
/// "Excelência Oper acio nal"  → "Excelência Operacional"
/// "caso alto base"            → "caso alto base" (preservado)
/// ```
fn normalize_broken_words(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    let mut result: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];

        if is_probable_fragment(token) {
            // Começa a acumular fragmentos
            let mut accumulated = token.to_string();
            i += 1;

            while i < tokens.len() {
                let next = tokens[i];
                let next_first_char = next.chars().next().unwrap_or('A');
                let accumulated_len = accumulated.chars().count();

                // Para se próximo é stopword
                if is_stopword(&next.to_lowercase()) {
                    break;
                }

                // Para se acumulado já tem tamanho razoável e próximo não é fragmento
                if accumulated_len >= 6 && !is_probable_fragment(next) {
                    break;
                }

                // Para se próximo inicia com maiúscula e não é fragmento
                if next_first_char.is_uppercase() && !is_probable_fragment(next) {
                    break;
                }

                // Junta o próximo token (sem espaço)
                accumulated.push_str(next);
                i += 1;
            }

            result.push(accumulated);
        } else {
            result.push(token.to_string());
            i += 1;
        }
    }

    result.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── normalize_broken_words ────────────────────────────────

    #[test]
    fn join_simple_fragment() {
        assert_eq!(normalize_broken_words("arm azenagem"), "armazenagem");
    }

    #[test]
    fn join_multi_fragment() {
        assert_eq!(normalize_broken_words("Oper acio nal"), "Operacional");
    }

    #[test]
    fn join_mixed_with_real_word() {
        assert_eq!(
            normalize_broken_words("Excelência Oper acio nal"),
            "Excelência Operacional"
        );
    }

    #[test]
    fn preserve_known_short_words() {
        assert_eq!(
            normalize_broken_words("caso alto base"),
            "caso alto base"
        );
    }

    #[test]
    fn preserve_stopwords() {
        assert_eq!(
            normalize_broken_words("controle de qualidade"),
            "controle de qualidade"
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(normalize_broken_words(""), "");
    }

    #[test]
    fn single_word() {
        assert_eq!(normalize_broken_words("sustentabilidade"), "sustentabilidade");
    }

    #[test]
    fn fragment_before_stopword() {
        // "ges" is fragment, "de" is stopword → should stop accumulation
        assert_eq!(
            normalize_broken_words("ges de risco"),
            "ges de risco"
        );
    }

    #[test]
    fn fragment_followed_by_capitalized_real_word() {
        // "acio" and "nal" are fragments, "Carlos" starts uppercase and is not fragment → stop
        assert_eq!(
            normalize_broken_words("Oper acio nal Carlos chegou"),
            "Operacional Carlos chegou"
        );
    }

    // ─── is_probable_fragment ──────────────────────────────────

    #[test]
    fn fragment_detection() {
        assert!(is_probable_fragment("nal"));
        assert!(is_probable_fragment("gio"));
        assert!(is_probable_fragment("aci"));
    }

    #[test]
    fn non_fragment_short_words() {
        assert!(!is_probable_fragment("caso"));
        assert!(!is_probable_fragment("base"));
        assert!(!is_probable_fragment("sol"));
    }

    #[test]
    fn non_fragment_stopwords() {
        assert!(!is_probable_fragment("de"));
        assert!(!is_probable_fragment("que"));
    }

    #[test]
    fn non_fragment_long_words() {
        assert!(!is_probable_fragment("sustentabilidade"));
    }

    #[test]
    fn non_fragment_with_numbers() {
        assert!(!is_probable_fragment("a2b"));
    }

    // ─── extract integration ───────────────────────────────────

    #[test]
    fn extract_normalizes_before_extracting() {
        let ext = EntityExtractor::new();
        let entities = ext.extract("Oper acio nal Excelência");
        // "Operacional" should appear as a merged word, not fragments
        let has_operacional = entities.iter().any(|e| e.to_lowercase().contains("operacional"));
        assert!(has_operacional, "Expected 'Operacional' in {:?}", entities);
        let has_fragment = entities.iter().any(|e| e == "acio" || e == "nal" || e == "Oper");
        assert!(!has_fragment, "Unexpected fragments in {:?}", entities);
    }

    #[test]
    fn extract_ngram_min_length_filters_short() {
        let ext = EntityExtractor::new();
        // "arm" is 3 chars — should NOT appear in n-grams (min >= 5)
        let entities = ext.extract("arm teste");
        let has_ngram_with_arm = entities.iter().any(|e| e.contains("arm teste"));
        assert!(!has_ngram_with_arm, "Short words should not form n-grams: {:?}", entities);
    }
}
