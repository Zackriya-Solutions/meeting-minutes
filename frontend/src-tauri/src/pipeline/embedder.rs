//! Local sentence embedder (PLAN.md Phase 1).
//!
//! Default model: `intfloat/multilingual-e5-small` exported to ONNX (dim 384, see
//! [`crate::vector::EMBEDDING_DIM`]). Runs on the existing `ort` runtime — no Python,
//! no network (embeddings are fully local per the tech constraints).
//!
//! e5 REQUIRES the `query:` / `passage:` prefixes; they are applied here in the wrapper
//! (never at call sites) so the whole codebase gets them for free.
//!
//! SCAFFOLD STATUS: the pure math (prefixing, mean-pooling, L2-normalization) is
//! implemented and unit-tested. The ONNX session + tokenizer wiring is written against
//! the model export but is inert until the model files ship — [`Embedder::load`] returns
//! `Err` if they are absent, and the `chunk_embed` job degrades to "chunks created,
//! embeddings skipped" so FTS search keeps working (see jobs::handlers).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedderKind {
    MultilingualE5Small,
    Frida,
}

impl EmbedderKind {
    pub fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("frida") {
            Self::Frida
        } else {
            Self::MultilingualE5Small
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::MultilingualE5Small => "multilingual-e5-small",
            Self::Frida => "frida",
        }
    }

    pub fn dim(self) -> usize {
        match self {
            Self::MultilingualE5Small => crate::vector::EMBEDDING_DIM,
            Self::Frida => crate::vector::FRIDA_EMBEDDING_DIM,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Pooling {
    Mean,
    Cls,
}

/// Where to find the embedding model and how to execute it.
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub model_dir: PathBuf,
    pub dim: usize,
    /// Inference batch size (PLAN.md: 8–16).
    pub batch_size: usize,
    kind: EmbedderKind,
    pooling: Pooling,
}

impl EmbedderConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self::for_kind(model_dir, EmbedderKind::MultilingualE5Small)
    }

    pub fn for_kind(model_dir: impl Into<PathBuf>, kind: EmbedderKind) -> Self {
        Self {
            model_dir: model_dir.into(),
            dim: kind.dim(),
            batch_size: if kind == EmbedderKind::Frida { 4 } else { 16 },
            kind,
            pooling: if kind == EmbedderKind::Frida {
                Pooling::Cls
            } else {
                Pooling::Mean
            },
        }
    }
    pub fn model_path(&self) -> PathBuf {
        self.model_dir.join(match self.kind {
            EmbedderKind::MultilingualE5Small => "model.onnx",
            EmbedderKind::Frida => "FRIDA.onnx",
        })
    }
    pub fn tokenizer_path(&self) -> PathBuf {
        self.model_dir.join("tokenizer.json")
    }
    pub fn is_available(&self) -> bool {
        self.model_path().exists()
            && self.tokenizer_path().exists()
            && (self.kind != EmbedderKind::Frida || self.model_dir.join("FRIDA.onnx.data").exists())
    }
}

/// Apply the e5 instruction prefix. Queries and passages use different prefixes; getting
/// this wrong silently degrades recall, which is why it lives in one place.
pub fn build_input_text_for_kind(kind: EmbedderKind, text: &str, is_query: bool) -> String {
    match (kind, is_query) {
        (EmbedderKind::MultilingualE5Small, true) => format!("query: {text}"),
        (EmbedderKind::MultilingualE5Small, false) => format!("passage: {text}"),
        (EmbedderKind::Frida, true) => format!("search_query: {text}"),
        (EmbedderKind::Frida, false) => format!("search_document: {text}"),
    }
}

pub fn build_input_text(text: &str, is_query: bool) -> String {
    build_input_text_for_kind(EmbedderKind::MultilingualE5Small, text, is_query)
}

/// Mean-pool token embeddings using the attention mask (standard e5 pooling): the
/// average of token vectors where mask == 1. `token_embeddings[i]` is the hidden vector
/// for token i; `attention_mask[i]` is 1.0 for real tokens, 0.0 for padding.
pub fn mean_pool(token_embeddings: &[Vec<f32>], attention_mask: &[f32]) -> Vec<f32> {
    let hidden = token_embeddings.first().map(|v| v.len()).unwrap_or(0);
    let mut sum = vec![0.0f32; hidden];
    let mut count = 0.0f32;
    for (tok, &m) in token_embeddings.iter().zip(attention_mask.iter()) {
        if m <= 0.0 {
            continue;
        }
        for (s, &t) in sum.iter_mut().zip(tok.iter()) {
            *s += t * m;
        }
        count += m;
    }
    if count > 0.0 {
        for s in sum.iter_mut() {
            *s /= count;
        }
    }
    sum
}

/// L2-normalize in place so cosine similarity reduces to a dot product (and matches the
/// distances sqlite-vec computes).
pub fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Tokenized inputs for one text (padded within a batch by the caller).
pub struct TokenizedInput {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
}

/// Pluggable tokenizer so the heavy HF `tokenizers` dependency is an integration detail,
/// not a hard coupling in this module.
pub trait TextTokenizer: Send + Sync {
    fn encode(&self, text: &str) -> Result<TokenizedInput>;
}

/// The embedder: an ONNX session + tokenizer. Construct with [`Embedder::load`].
pub struct Embedder {
    config: EmbedderConfig,
    #[allow(dead_code)]
    session: ort::session::Session,
    tokenizer: Box<dyn TextTokenizer>,
}

impl Embedder {
    /// Load the model. Returns `Err` if the model/tokenizer files are missing, so the
    /// caller can degrade gracefully.
    pub fn load(config: EmbedderConfig, tokenizer: Box<dyn TextTokenizer>) -> Result<Self> {
        if !config.is_available() {
            return Err(anyhow!(
                "embedding model not found at {} (model.onnx + tokenizer.json required)",
                config.model_dir.display()
            ));
        }
        let session = build_session(&config.model_path())
            .with_context(|| format!("loading embedder from {}", config.model_path().display()))?;
        Ok(Self {
            config,
            session,
            tokenizer,
        })
    }

    pub fn dim(&self) -> usize {
        self.config.dim
    }

    /// Embed a query (adds `query:` prefix, L2-normalized). Takes `&mut self` because
    /// `ort::Session::run` requires a mutable session.
    pub fn embed_query(&mut self, text: &str) -> Result<Vec<f32>> {
        Ok(self
            .embed_batch(&[text.to_string()], true)?
            .into_iter()
            .next()
            .unwrap_or_default())
    }

    /// Embed passages (adds `passage:` prefix). Batches internally per `batch_size`.
    pub fn embed_passages(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let batch_size = self.config.batch_size.max(1);
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(batch_size) {
            out.extend(self.embed_batch(batch, false)?);
        }
        Ok(out)
    }

    /// Core inference for one batch. Tokenize → ONNX forward → mean-pool → L2-normalize.
    fn embed_batch(&mut self, texts: &[String], is_query: bool) -> Result<Vec<Vec<f32>>> {
        use ort::inputs;
        use ort::value::TensorRef;

        // Tokenize with prefixes, pad to the batch's max length.
        let encoded: Vec<TokenizedInput> = texts
            .iter()
            .map(|t| {
                self.tokenizer
                    .encode(&build_input_text_for_kind(self.config.kind, t, is_query))
            })
            .collect::<Result<_>>()?;
        let max_len = encoded.iter().map(|e| e.input_ids.len()).max().unwrap_or(0);
        let bsz = encoded.len();

        let mut ids = ndarray::Array2::<i64>::zeros((bsz, max_len));
        let mut mask = ndarray::Array2::<i64>::zeros((bsz, max_len));
        for (b, e) in encoded.iter().enumerate() {
            for (j, (&id, &m)) in e.input_ids.iter().zip(e.attention_mask.iter()).enumerate() {
                ids[[b, j]] = id;
                mask[[b, j]] = m;
            }
        }

        // ort's `Error<R>` is not Send+Sync for all R, so it can't auto-convert into
        // anyhow::Error via `?`; map to a string at each call site.
        let ids_tensor =
            TensorRef::from_array_view(ids.view()).map_err(|e| anyhow!("ort input_ids: {e}"))?;
        let mask_tensor = TensorRef::from_array_view(mask.view())
            .map_err(|e| anyhow!("ort attention_mask: {e}"))?;
        // Some multilingual-e5 ONNX exports retain BERT's token-type embedding input.
        // A sentence is one segment, so its canonical token-type value is zero. Other
        // exports omit the input; inspect the graph rather than passing an unknown name.
        let needs_token_type_ids = self
            .session
            .inputs
            .iter()
            .any(|input| input.name == "token_type_ids");
        let token_types = ndarray::Array2::<i64>::zeros((bsz, max_len));
        let outputs = if needs_token_type_ids {
            let token_types_tensor = TensorRef::from_array_view(token_types.view())
                .map_err(|e| anyhow!("ort token_type_ids: {e}"))?;
            self.session.run(inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
                "token_type_ids" => token_types_tensor,
            ])
        } else {
            self.session
                .run(inputs!["input_ids" => ids_tensor, "attention_mask" => mask_tensor])
        }
        .map_err(|e| anyhow!("ort run: {e}"))?;

        // last_hidden_state: [batch, seq, hidden]. Matches the parakeet extraction API
        // (ort rc): `.get(name).try_extract_array()` -> ArrayViewD<f32>.
        let value = outputs
            .get("last_hidden_state")
            .ok_or_else(|| anyhow!("model output 'last_hidden_state' missing"))?;
        let hidden_states = value
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("ort extract: {e}"))?;
        let shape = hidden_states.shape();
        let seq = shape[1];
        let hidden = shape[2];

        let mut result = Vec::with_capacity(bsz);
        for b in 0..bsz {
            let token_embeddings: Vec<Vec<f32>> = (0..seq)
                .map(|s| (0..hidden).map(|h| hidden_states[[b, s, h]]).collect())
                .collect();
            let mut pooled = match self.config.pooling {
                Pooling::Mean => {
                    let attn: Vec<f32> = (0..seq).map(|s| mask[[b, s]] as f32).collect();
                    mean_pool(&token_embeddings, &attn)
                }
                Pooling::Cls => token_embeddings.first().cloned().unwrap_or_default(),
            };
            if pooled.len() != self.config.dim {
                return Err(anyhow!(
                    "embedding dimension mismatch: model returned {}, expected {}",
                    pooled.len(),
                    self.config.dim
                ));
            }
            l2_normalize(&mut pooled);
            result.push(pooled);
        }
        Ok(result)
    }
}

fn build_session(model_path: &Path) -> Result<ort::session::Session> {
    use ort::session::{builder::GraphOptimizationLevel, Session};
    let session = Session::builder()
        .map_err(|e| anyhow!("ort builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("ort optimization level: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow!("ort load {}: {e}", model_path.display()))?;
    Ok(session)
}

/// Concrete tokenizer backed by a HuggingFace `tokenizer.json` (multilingual-e5 uses an
/// XLM-R unigram model). Truncates to the model's max sequence length.
pub struct HfTokenizer {
    inner: tokenizers::Tokenizer,
    max_len: usize,
}

impl HfTokenizer {
    pub fn from_file(path: &Path) -> Result<Self> {
        let inner = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| anyhow!("failed to load tokenizer {}: {e}", path.display()))?;
        Ok(Self {
            inner,
            max_len: 512,
        })
    }
}

impl TextTokenizer for HfTokenizer {
    fn encode(&self, text: &str) -> Result<TokenizedInput> {
        let enc = self
            .inner
            .encode(text, true)
            .map_err(|e| anyhow!("tokenize failed: {e}"))?;
        let mut input_ids: Vec<i64> = enc.get_ids().iter().map(|&i| i as i64).collect();
        let mut attention_mask: Vec<i64> =
            enc.get_attention_mask().iter().map(|&m| m as i64).collect();
        if input_ids.len() > self.max_len {
            input_ids.truncate(self.max_len);
            attention_mask.truncate(self.max_len);
        }
        Ok(TokenizedInput {
            input_ids,
            attention_mask,
        })
    }
}

// ---- Process-wide embedder instance ----
//
// One shared embedder for the chunk_embed job and the search/RAG query paths. A
// const-initialized `std::sync::Mutex` (Embedder is Send; ONNX inference is synchronous,
// so the lock is never held across an await — async callers go through spawn_blocking).
static EMBEDDER: Mutex<Option<Embedder>> = Mutex::new(None);
static MODEL_INDEX_LOCK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// Hold while an embedding is produced and consumed by vector search or persistence.
/// Model switches take the write side so the active model and sqlite-vec dimension
/// cannot change between inference and the corresponding database operation.
pub async fn model_index_read_guard() -> tokio::sync::RwLockReadGuard<'static, ()> {
    MODEL_INDEX_LOCK.read().await
}

pub async fn model_index_write_guard() -> tokio::sync::RwLockWriteGuard<'static, ()> {
    MODEL_INDEX_LOCK.write().await
}

/// Load the default e5-small embedder from `model_dir` (expects `model.onnx` +
/// `tokenizer.json`) into the global slot, replacing any previous model.
pub fn load_global(model_dir: impl Into<PathBuf>) -> Result<()> {
    load_global_kind(model_dir, EmbedderKind::MultilingualE5Small)
}

pub fn load_kind(model_dir: impl Into<PathBuf>, kind: EmbedderKind) -> Result<Embedder> {
    let config = EmbedderConfig::for_kind(model_dir, kind);
    let tokenizer: Box<dyn TextTokenizer> =
        Box::new(HfTokenizer::from_file(&config.tokenizer_path())?);
    Embedder::load(config, tokenizer)
}

pub fn install_global(embedder: Embedder) {
    let kind = embedder.config.kind;
    *EMBEDDER.lock().unwrap() = Some(embedder);
    log::info!("embedding model {} loaded (dim={})", kind.id(), kind.dim());
}

pub fn load_global_kind(model_dir: impl Into<PathBuf>, kind: EmbedderKind) -> Result<()> {
    let embedder = load_kind(model_dir, kind)?;
    install_global(embedder);
    Ok(())
}

pub fn is_loaded() -> bool {
    EMBEDDER.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn unload_global() {
    if let Ok(mut g) = EMBEDDER.lock() {
        *g = None;
    }
}

/// Embed a query on a blocking thread (adds `query:` prefix). `None` when no model is
/// loaded; `Some(Err)` on inference failure.
pub async fn embed_query(text: String) -> Option<Result<Vec<f32>, String>> {
    if !is_loaded() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = EMBEDDER.lock().unwrap();
        guard
            .as_mut()
            .map(|e| e.embed_query(&text).map_err(|e| e.to_string()))
    })
    .await
    .ok()
    .flatten()
}

/// Embed passages on a blocking thread (adds `passage:` prefix). `None` when no model is
/// loaded.
pub async fn embed_passages(texts: Vec<String>) -> Option<Result<Vec<Vec<f32>>, String>> {
    if !is_loaded() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = EMBEDDER.lock().unwrap();
        guard
            .as_mut()
            .map(|e| e.embed_passages(&texts).map_err(|e| e.to_string()))
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e5_prefixes() {
        assert_eq!(build_input_text("бюджет", true), "query: бюджет");
        assert_eq!(build_input_text("текст", false), "passage: текст");
    }

    #[test]
    fn frida_retrieval_prefixes() {
        assert_eq!(
            build_input_text_for_kind(EmbedderKind::Frida, "бюджет", true),
            "search_query: бюджет"
        );
        assert_eq!(
            build_input_text_for_kind(EmbedderKind::Frida, "текст", false),
            "search_document: текст"
        );
    }

    #[test]
    fn mean_pool_ignores_padding() {
        // 3 tokens, hidden=2; third token is padding (mask 0).
        let toks = vec![vec![2.0, 4.0], vec![4.0, 8.0], vec![100.0, 100.0]];
        let mask = vec![1.0, 1.0, 0.0];
        let pooled = mean_pool(&toks, &mask);
        assert_eq!(pooled, vec![3.0, 6.0]); // mean of the first two only
    }

    #[test]
    fn l2_normalize_unit_length() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        let norm = (v[0] * v[0] + v[1] * v[1]).sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_vector_normalization_is_safe() {
        let mut v = vec![0.0, 0.0];
        l2_normalize(&mut v); // must not divide by zero
        assert_eq!(v, vec![0.0, 0.0]);
    }

    #[test]
    #[ignore = "requires MEETILY_TEST_EMBEDDING_MODEL_DIR with model.onnx and tokenizer.json"]
    fn installed_embedding_model_runs_end_to_end() {
        let model_dir = std::env::var("MEETILY_TEST_EMBEDDING_MODEL_DIR")
            .expect("MEETILY_TEST_EMBEDDING_MODEL_DIR is required");
        let config = EmbedderConfig::new(model_dir);
        let tokenizer = Box::new(HfTokenizer::from_file(&config.tokenizer_path()).unwrap());
        let mut embedder = Embedder::load(config, tokenizer).unwrap();
        let embeddings = embedder
            .embed_passages(&["проверка локальных эмбеддингов".to_string()])
            .unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), crate::vector::EMBEDDING_DIM);
        let norm = embeddings[0]
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}
