// Embeddings: turn text into a vector through the `embedding` model role, and the
// small amount of maths the retrieval layer needs (store as a BLOB, compare with
// cosine). No native SQLite extension — vectors live in a plain BLOB column and
// are ranked in Rust, which is plenty at personal scale and adds no build burden.
//
// Embedding is optional at every call site: if no model fills the role, or the
// provider can't serve embeddings, callers degrade to keyword search.

use serde_json::json;
use tauri::AppHandle;

use crate::provider::Dialect;

/// f32 little-endian bytes, the storage form for `embeddings.vector`.
pub(crate) fn to_blob(v: &[f32]) -> Vec<u8> {
  let mut out = Vec::with_capacity(v.len() * 4);
  for f in v { out.extend_from_slice(&f.to_le_bytes()); }
  out
}

pub(crate) fn from_blob(b: &[u8]) -> Vec<f32> {
  b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Cosine similarity in [-1, 1]. Zero when either vector is empty or mismatched,
/// so a bad stored vector never ranks above a real match.
pub(crate) fn cosine(a: &[f32], b: &[f32]) -> f32 {
  if a.len() != b.len() || a.is_empty() { return 0.0; }
  let mut dot = 0f32;
  let mut na = 0f32;
  let mut nb = 0f32;
  for i in 0..a.len() {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  if na == 0.0 || nb == 0.0 { return 0.0; }
  dot / (na.sqrt() * nb.sqrt())
}

/// A cheap content hash so an unchanged node isn't re-embedded on every pass.
pub(crate) fn content_hash(text: &str) -> String {
  use std::hash::{Hash, Hasher};
  let mut h = std::collections::hash_map::DefaultHasher::new();
  text.hash(&mut h);
  format!("{:x}", h.finish())
}

fn embedding_of(dialect: Dialect, body: &serde_json::Value) -> Option<Vec<f32>> {
  let arr = match dialect {
    Dialect::Ollama => body.get("embedding")?.clone(),
    _ => body.get("data")?.get(0)?.get("embedding")?.clone(),
  };
  let v: Vec<f32> = arr.as_array()?.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
  if v.is_empty() { None } else { Some(v) }
}

/// Embeds one piece of text through the configured `embedding` role. Returns Err
/// (never panics) when no role is set, the provider can't, or the call fails.
pub(crate) async fn embed_text(app: &AppHandle, text: &str) -> Result<Vec<f32>, String> {
  let text = text.trim();
  if text.is_empty() { return Err("empty text".into()); }
  // Bound the input — embedding large transcripts is wasteful and some providers cap it.
  let text: String = text.chars().take(4000).collect();

  let resolved = {
    let conn = crate::db::db(app)?;
    let model_id = crate::provider::resolve_role(&conn, "embedding").ok_or("No embedding model configured.")?;
    crate::provider::resolve(&conn, &model_id, None)?
  };
  if !resolved.caps().embeddings {
    return Err("Provider does not support embeddings.".into());
  }
  let body = match resolved.dialect() {
    Dialect::Ollama => json!({ "model": resolved.model, "prompt": text }),
    _ => json!({ "model": resolved.model, "input": text }),
  };
  let url = crate::provider::embeddings_url(&resolved);
  let client = crate::http::client();
  let resp = crate::http::send_model_request(&client, &resolved, &url, &body, 2).await?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  embedding_of(resolved.dialect(), &json).ok_or_else(|| "Embedding response malformed.".into())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn blob_round_trips() {
    let v = vec![0.5f32, -1.25, 3.0, 0.0];
    assert_eq!(from_blob(&to_blob(&v)), v);
  }

  #[test]
  fn cosine_orders_related_above_orthogonal() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.9, 0.1, 0.0];
    let c = vec![0.0, 1.0, 0.0];
    assert!(cosine(&a, &b) > cosine(&a, &c));
    assert_eq!(cosine(&a, &c), 0.0);
  }

  #[test]
  fn cosine_guards_mismatched_and_empty() {
    assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0);
    assert_eq!(cosine(&[], &[]), 0.0);
  }
}
