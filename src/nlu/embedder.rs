//! # Embedder BERTimbau — Gerador de Representações Vetoriais
//!
//! O [`Embedder`] encapsula o modelo **BERTimbau**
//! (`neuralmind/bert-base-portuguese-cased`), um BERT pré-treinado
//! especificamente para Português Brasileiro.
//!
//! ## O que são Embeddings?
//!
//! Embeddings são vetores numéricos de 768 dimensões que representam
//! o **significado semântico** de um texto. Textos com significados
//! similares terão vetores próximos no espaço vetorial.
//!
//! ```text
//! "gato"   → [0.12, -0.45, 0.78, ...]  (768 dimensões)
//! "felino" → [0.11, -0.43, 0.76, ...]  (vetor próximo!)
//! "carro"  → [-0.56, 0.23, -0.09, ...] (vetor distante)
//! ```
//!
//! ## Pipeline de Embedding
//!
//! ```text
//! Texto → Tokenizer → Token IDs → BERT Forward Pass → Pooling → L2 Normalize
//!                                                        ↓
//!                                                  Vec<f32> (768-dim)
//! ```
//!
//! ### Detalhes Técnicos
//!
//! 1. **Tokenização**: WordPiece divide texto em sub-palavras
//!    - "conhecimento" → ["conhec", "##imento"]
//! 2. **Forward Pass**: BERT processa tokens com self-attention
//! 3. **Mean Pooling**: Média ponderada dos tokens (attention mask)
//! 4. **L2 Normalize**: Normalização para cosine similarity eficiente
//!
//! ## Carregamento do Modelo
//!
//! O modelo é baixado do HuggingFace Hub na primeira execução (~400 MB)
//! e cacheado em `~/.cache/huggingface/`. O carregamento segue uma
//! estratégia de fallback:
//!
//! | Componente | Preferido | Fallback |
//! |-----------|-----------|----------|
//! | Tokenizer | `tokenizer.json` | `vocab.txt` (WordPiece) |
//! | Pesos | `model.safetensors` | `pytorch_model.bin` |
//! | Device | CPU | — (Metal não suporta layer-norm do BERT) |

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert;
use hf_hub::api::sync::Api;
use tokenizers::Tokenizer;

/// Embedder BERTimbau — gera representações vetoriais de texto em PT-BR.
///
/// Encapsula o modelo BERT, tokenizer, e device (CPU).
/// Após carregamento via [`Embedder::load()`], expõe dois métodos:
///
/// - [`embed()`](Embedder::embed) — embedding de texto único
/// - [`embed_batch()`](Embedder::embed_batch) — embedding de múltiplos textos em uma forward pass
///
/// ## Performance
///
/// | Operação | Tempo típico (CPU, M1) |
/// |----------|----------------------|
/// | `embed()` (1 texto) | ~15ms |
/// | `embed_batch()` (10 textos) | ~40ms |
/// | `embed_batch()` (50 textos) | ~150ms |
///
/// O batch é significativamente mais eficiente que chamadas individuais.
pub struct Embedder {
    /// Modelo BERT carregado — Candle `BertModel`.
    model: bert::BertModel,
    /// Tokenizer WordPiece para Português.
    tokenizer: Tokenizer,
    /// Device de execução (atualmente sempre CPU).
    device: Device,
}

impl Embedder {
    /// Carrega o modelo BERTimbau do HuggingFace Hub.
    ///
    /// Este método executa operações de I/O pesadas:
    /// - Download do modelo (~400 MB na primeira vez)
    /// - Leitura de arquivos do disco (config, pesos, tokenizer)
    /// - Alocação de memória para o modelo
    ///
    /// Por isso é chamado em `spawn_blocking` no `main.rs` para não
    /// bloquear o runtime do Tokio.
    ///
    /// ## Estratégia de Fallback
    ///
    /// ```text
    /// Tokenizer: tokenizer.json → vocab.txt (WordPiece manual)
    /// Pesos:     model.safetensors → pytorch_model.bin
    /// ```
    ///
    /// ## Device
    ///
    /// Usa CPU porque o Candle 0.8 Metal não possui suporte completo
    /// para layer-norm (necessário pelo BERT). CPU é suficiente para
    /// inferência de BERT-base com ~110M parâmetros.
    ///
    /// # Erros
    ///
    /// Retorna erro se:
    /// - Não conseguir acessar o HuggingFace Hub (rede)
    /// - Os arquivos do modelo estiverem corrompidos
    /// - Não houver memória suficiente (~500 MB RAM)
    pub fn load() -> Result<Self> {
        // Candle 0.8 Metal carece do suporte a layer-norm exigido pelo BERT;
        // CPU é rápido o suficiente para inferência de um BERT-base.
        let device = Device::Cpu;
        tracing::info!("Device: CPU");

        let repo_id = "neuralmind/bert-base-portuguese-cased";

        tracing::info!("Loading BERTimbau ({}) from HuggingFace Hub...", repo_id);
        let api = Api::new().context("Failed to create HF Hub API")?;
        let repo = api.model(repo_id.to_string());

        // ─── Tokenizer ────────────────────────────────────────────
        let config_path = repo
            .get("config.json")
            .context("Failed to download config.json")?;
        // Tenta tokenizer.json primeiro (sem configuração manual);
        // caso não exista, constrói um tokenizer WordPiece a partir de vocab.txt
        let tokenizer = match repo.get("tokenizer.json") {
            Ok(tokenizer_path) => {
                tracing::info!("Loading tokenizer from tokenizer.json...");
                Tokenizer::from_file(&tokenizer_path)
                    .map_err(|e| anyhow::anyhow!("{}", e))?
            }
            Err(_) => {
                tracing::info!(
                    "tokenizer.json not available, building WordPiece from vocab.txt..."
                );
                let vocab_path = repo
                    .get("vocab.txt")
                    .context("Failed to download vocab.txt")?;
                Self::build_bert_tokenizer(
                    vocab_path
                        .to_str()
                        .context("Invalid vocab.txt path encoding")?,
                )?
            }
        };

        // ─── Config do modelo ─────────────────────────────────────
        tracing::info!("Loading model config...");
        let config_str = std::fs::read_to_string(&config_path)?;
        let config: bert::Config =
            serde_json::from_str(&config_str).context("Failed to parse model config")?;

        // ─── Pesos do modelo ──────────────────────────────────────
        // Prefere safetensors (rápido, seguro) sobre pytorch_model.bin (pickle)
        tracing::info!("Loading model weights...");
        let vb = match repo.get("model.safetensors") {
            Ok(safetensors_path) => {
                tracing::info!("Loading from model.safetensors...");
                unsafe {
                    VarBuilder::from_mmaped_safetensors(
                        &[safetensors_path],
                        DType::F32,
                        &device,
                    )
                    .context("Failed to load safetensors weights")?
                }
            }
            Err(_) => {
                tracing::info!("Falling back to pytorch_model.bin...");
                let weights_path = repo
                    .get("pytorch_model.bin")
                    .context("Failed to download pytorch_model.bin")?;
                VarBuilder::from_pth(&weights_path, DType::F32, &device)
                    .context("Failed to load pytorch weights")?
            }
        };

        // ─── Instanciação do modelo ──────────────────────────────
        let model =
            bert::BertModel::load(vb, &config).context("Failed to load BERTimbau model")?;

        tracing::info!("BERTimbau model loaded successfully on {:?}!", device);
        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    /// Constrói um tokenizer WordPiece BERT a partir de `vocab.txt`.
    ///
    /// Usado como fallback quando o repositório não possui `tokenizer.json`.
    /// Configura:
    /// - **BertNormalizer**: clean_text + handle_chinese_chars (sem lowercase)
    /// - **BertPreTokenizer**: split por whitespace e pontuação
    /// - **BertProcessing**: adiciona tokens [CLS] e [SEP]
    ///
    /// ## BERTimbau é Cased!
    ///
    /// O BERTimbau mantém capitalização (`lowercase = false`), o que é
    /// importante para Português onde nomes próprios são capitalizados.
    fn build_bert_tokenizer(vocab_path: &str) -> Result<Tokenizer> {
        use tokenizers::models::wordpiece::WordPiece;
        use tokenizers::normalizers::BertNormalizer;
        use tokenizers::pre_tokenizers::bert::BertPreTokenizer;
        use tokenizers::processors::bert::BertProcessing;

        let wordpiece = WordPiece::from_file(vocab_path)
            .unk_token("[UNK]".to_string())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut tokenizer = Tokenizer::new(wordpiece);
        // BERTimbau é cased (sensível a maiúsculas) — lowercase = false
        tokenizer.with_normalizer(Some(BertNormalizer::new(
            true,  // clean_text: remove caracteres de controle
            true,  // handle_chinese_chars: adiciona espaços ao redor
            None,  // strip_accents: comportamento padrão (não remove)
            false, // lowercase: MANTER capitalização original
        )));
        tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
        tokenizer.with_post_processor(Some(BertProcessing::new(
            ("[SEP]".to_string(), 102), // Token separador (ID 102)
            ("[CLS]".to_string(), 101), // Token classificador (ID 101)
        )));

        Ok(tokenizer)
    }

    /// Gera embedding de um texto único → `Vec<f32>` (768 dims, L2 normalizado).
    ///
    /// ## Pipeline
    ///
    /// ```text
    /// texto → tokenize → [CLS] tok1 tok2 ... [SEP]
    ///                          ↓
    ///       BERT Forward (12 camadas de self-attention)
    ///                          ↓
    ///       Mean Pooling (média ponderada por attention mask)
    ///                          ↓
    ///       L2 Normalize (||v|| = 1, para cosine sim eficiente)
    ///                          ↓
    ///                   Vec<f32> [768-dim]
    /// ```
    ///
    /// ## Mean Pooling vs CLS
    ///
    /// Usamos **mean pooling** (média de todos os tokens) em vez do token CLS
    /// porque produz embeddings de melhor qualidade para similaridade semântica.
    /// O token CLS é otimizado para classificação, não para representação.
    ///
    /// # Erros
    ///
    /// Retorna erro se a tokenização ou o forward pass falhar.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Tokeniza o texto com truncamento automático
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenizer error: {}", e))?;

        let ids = encoding.get_ids();
        let attention_mask_vec: Vec<u32> = encoding.get_attention_mask().to_vec();
        // Token type IDs = 0 para todos (single segment, sem sentence pairs)
        let token_type_ids_vec: Vec<u32> = vec![0u32; ids.len()];

        // Constrói tensores com batch_size = 1
        let input_ids = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::new(&token_type_ids_vec[..], &self.device)?.unsqueeze(0)?;
        let attention_mask = Tensor::new(&attention_mask_vec[..], &self.device)?.unsqueeze(0)?;

        // Forward pass — BertModel retorna tensor [1, seq_len, 768]
        let output = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // ─── Mean Pooling ─────────────────────────────────────────
        // Expande mask para [1, seq_len, 768] para multiplicação element-wise
        let mask_expanded = attention_mask
            .unsqueeze(2)?           // [1, seq_len, 1]
            .to_dtype(DType::F32)?
            .broadcast_as(output.shape())?; // [1, seq_len, 768]

        // Multiplica output pelo mask (zerando tokens de padding)
        let masked = (output * mask_expanded.clone())?;
        // Soma ao longo da dimensão seq_len → [1, 768]
        let summed = masked.sum(1)?;
        // Divide pela soma do mask (número de tokens reais)
        let mask_sum = mask_expanded.sum(1)?.clamp(1e-9, f64::MAX)?;
        let pooled = (summed / mask_sum)?;

        // ─── L2 Normalize ─────────────────────────────────────────
        // Normaliza para ||v|| = 1, assim cosine_similarity(a, b) = dot(a, b)
        let norm = pooled.sqr()?.sum(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm.unsqueeze(1)?)?;

        // Extrai o vetor final como Vec<f32>
        let embedding: Vec<f32> = normalized.squeeze(0)?.to_vec1()?;
        Ok(embedding)
    }

    /// Gera embeddings de múltiplos textos em uma única forward pass.
    ///
    /// Significativamente mais eficiente que chamar [`embed()`](Embedder::embed)
    /// repetidamente, pois utiliza uma única forward pass do modelo para todos os textos.
    ///
    /// ## Pipeline
    ///
    /// ```text
    /// ["texto1", "texto2", "texto3"]
    ///           ↓
    /// Tokenize cada → pad para max_len → concatenar em batch
    ///           ↓
    /// BERT Forward [batch_size, max_len] → [batch_size, max_len, 768]
    ///           ↓
    /// Mean Pool + L2 Norm para cada item → [batch_size, 768]
    ///           ↓
    /// Vec<Vec<f32>>: um vetor 768-dim por texto
    /// ```
    ///
    /// ## Padding
    ///
    /// Textos mais curtos são padded com zeros até o comprimento do texto
    /// mais longo do batch. O attention mask garante que esses tokens
    /// padding não influenciem o resultado.
    ///
    /// # Parâmetros
    ///
    /// - `texts` — slice de textos para embeddar (vazio retorna `Vec::new()`)
    ///
    /// # Retorno
    ///
    /// `Vec<Vec<f32>>` — um embedding 768-dim normalizado para cada texto
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // Otimização: se só há 1 texto, delegate para embed() (sem padding)
        if texts.len() == 1 {
            return Ok(vec![self.embed(&texts[0])?]);
        }

        // Tokeniza todos os textos
        let encodings: Vec<_> = texts
            .iter()
            .map(|t| {
                self.tokenizer
                    .encode(t.as_str(), true)
                    .map_err(|e| anyhow::anyhow!("Tokenizer error: {}", e))
            })
            .collect::<Result<Vec<_>>>()?;

        // Determina o comprimento máximo para padding
        let max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);

        // Constrói tensores padded para o batch inteiro
        let batch_size = encodings.len();
        let mut all_ids = vec![0u32; batch_size * max_len];     // Token IDs (0 = pad)
        let all_type_ids = vec![0u32; batch_size * max_len];    // Tipo de segmento (sempre 0)
        let mut all_mask = vec![0u32; batch_size * max_len];    // Attention mask (0 = ignorar)

        // Preenche tensores com dados reais (tokens padding ficam como 0)
        for (i, enc) in encodings.iter().enumerate() {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let offset = i * max_len;
            for (j, &id) in ids.iter().enumerate() {
                all_ids[offset + j] = id;
                all_mask[offset + j] = mask[j];
            }
        }

        // Converte para tensores Candle [batch_size, max_len]
        let input_ids =
            Tensor::from_vec(all_ids, (batch_size, max_len), &self.device)?;
        let token_type_ids =
            Tensor::from_vec(all_type_ids, (batch_size, max_len), &self.device)?;
        let attention_mask =
            Tensor::from_vec(all_mask, (batch_size, max_len), &self.device)?;

        // Forward pass único para todo o batch → [batch_size, max_len, 768]
        let output = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // ─── Mean Pooling (batch) ─────────────────────────────────
        let mask_expanded = attention_mask
            .unsqueeze(2)?
            .to_dtype(DType::F32)?
            .broadcast_as(output.shape())?;

        let masked = (output * mask_expanded.clone())?;
        let summed = masked.sum(1)?;
        let mask_sum = mask_expanded.sum(1)?.clamp(1e-9, f64::MAX)?;
        let pooled = (summed / mask_sum)?;

        // ─── L2 Normalize (batch) ─────────────────────────────────
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm)?;

        // Extrai embeddings individuais do tensor batch
        let mut results = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let emb: Vec<f32> = normalized.get(i)?.to_vec1()?;
            results.push(emb);
        }

        Ok(results)
    }
}
