use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use ndarray::{Array1, Array2, Axis};

const DEFAULT_MODEL_NAME: &str = "minishlab/potion-code-16M";

pub struct StaticEncoder {
    tokenizer: tokenizers::Tokenizer,
    embeddings: Array2<f32>,
    vocab_size: usize,
    dim: usize,
}

impl StaticEncoder {
    pub fn load(model_name: Option<&str>) -> Result<Self> {
        let name = model_name.unwrap_or(DEFAULT_MODEL_NAME);
        let api = hf_hub::api::sync::Api::new().context("Failed to create HuggingFace Hub API")?;
        let repo = api.model(name.to_string());
        let tokenizer_path = repo.get("tokenizer.json").context("Failed to download tokenizer.json")?;
        let model_path = repo
            .get("model.safetensors")
            .context("Failed to download model.safetensors")?;
        Self::from_files(&tokenizer_path, &model_path)
    }

    pub fn from_files(tokenizer_path: &PathBuf, model_path: &PathBuf) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        let model_data = std::fs::read(model_path).context("Failed to read model file")?;
        let tensors = safetensors::SafeTensors::deserialize(&model_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize safetensors: {e}"))?;

        let tensor_names: Vec<String> = tensors.names().into_iter().map(String::from).collect();
        let emb = ["embeddings", "static_embeddings", "embedding.weight"]
            .iter()
            .find_map(|name| tensors.tensor(name).ok())
            .or_else(|| {
                tensor_names
                    .iter()
                    .find_map(|name| {
                        let t = tensors.tensor(name).ok()?;
                        if t.shape().len() == 2 {
                            Some(t)
                        } else {
                            None
                        }
                    })
            })
            .ok_or_else(|| anyhow::anyhow!("No embedding tensor found in model. Tensors: {tensor_names:?}"))?;

        let shape = emb.shape();
        if shape.len() != 2 {
            bail!("Expected 2D embedding tensor, got {}D", shape.len());
        }
        let vocab_size = shape[0];
        let dim = shape[1];

        let embedding_data: Vec<f32> = match emb.dtype() {
            safetensors::Dtype::F32 => emb
                .data()
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
            safetensors::Dtype::F16 => emb
                .data()
                .chunks_exact(2)
                .map(|b| half::f16::from_bits(u16::from_le_bytes([b[0], b[1]])).to_f32())
                .collect(),
            safetensors::Dtype::BF16 => emb
                .data()
                .chunks_exact(2)
                .map(|b| half::bf16::from_bits(u16::from_le_bytes([b[0], b[1]])).to_f32())
                .collect(),
            dt => bail!("Unsupported embedding dtype: {dt:?}"),
        };

        let embeddings =
            Array2::from_shape_vec((vocab_size, dim), embedding_data).context("Failed to reshape embedding tensor")?;

        Ok(Self {
            tokenizer,
            embeddings,
            vocab_size,
            dim,
        })
    }

    pub fn embedding_dim(&self) -> usize {
        self.dim
    }

    pub fn encode_single(&self, text: &str) -> Result<Array1<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?;
        let ids = encoding.get_ids();

        let mut sum = Array1::zeros(self.dim);
        let mut count = 0usize;

        for &id in ids {
            let id = id as usize;
            if id < self.vocab_size {
                sum += &self.embeddings.row(id);
                count += 1;
            }
        }

        if count > 0 {
            sum /= count as f32;
        }

        let norm = sum.dot(&sum).sqrt();
        if norm > 1e-12 {
            sum /= norm;
        }

        Ok(sum)
    }

    pub fn encode_batch(&self, texts: &[String]) -> Result<Array2<f32>> {
        if texts.is_empty() {
            return Ok(Array2::zeros((0, self.dim)));
        }
        let mut result = Array2::zeros((texts.len(), self.dim));
        for (i, text) in texts.iter().enumerate() {
            let embedding = self.encode_single(text)?;
            result.row_mut(i).assign(&embedding);
        }
        Ok(result)
    }
}

pub struct SemanticIndex {
    embeddings: Array2<f32>,
}

impl SemanticIndex {
    pub fn new(mut embeddings: Array2<f32>) -> Self {
        for mut row in embeddings.axis_iter_mut(Axis(0)) {
            let norm: f32 = row.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-12 {
                row.mapv_inplace(|x| x / norm);
            }
        }
        Self { embeddings }
    }

    pub fn query(
        &self,
        query_embedding: &Array1<f32>,
        k: usize,
        selector: Option<&[usize]>,
    ) -> Vec<(usize, f32)> {
        let norm: f32 = query_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let query_norm = if norm > 1e-12 {
            query_embedding.mapv(|x| x / norm)
        } else {
            query_embedding.clone()
        };

        if let Some(selector) = selector {
            let mut dists: Vec<(usize, f32)> = selector
                .iter()
                .filter(|&&idx| idx < self.embeddings.nrows())
                .map(|&idx| {
                    let sim: f32 = self.embeddings.row(idx).dot(&query_norm);
                    (idx, 1.0 - sim)
                })
                .collect();
            dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            dists.truncate(k);
            dists
        } else {
            let similarities = self.embeddings.dot(&query_norm);
            let mut dists: Vec<(usize, f32)> = similarities
                .iter()
                .enumerate()
                .map(|(idx, &sim)| (idx, 1.0 - sim))
                .collect();
            if k < dists.len() {
                dists.select_nth_unstable_by(k, |a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                dists.truncate(k);
            }
            dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            dists
        }
    }
}
