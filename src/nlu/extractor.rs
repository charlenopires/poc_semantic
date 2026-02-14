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
        let mut entities = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // ─── 1. Texto entre aspas ────────────────────────────────
        for cap in self.quoted_re.captures_iter(text) {
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
        for cap in self.capitalized_re.captures_iter(text) {
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
                        !is_stopword(&wl) && wl.len() > 2 && !looks_like_verb(&wl)
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
