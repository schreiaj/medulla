use crate::core::MemoryEntry;
use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use polars::prelude::*;
use std::fs::File;
use std::path::Path;
use std::sync::Mutex;

static EMBEDDER: Mutex<Option<TextEmbedding>> = Mutex::new(None);

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn embed_texts(root: &Path, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
    let mut guard = EMBEDDER.lock().unwrap();
    if guard.is_none() {
        println!("[MED] Initializing embedding model (first run downloads ~23MB)...");
        let cache_dir = root.join(".medulla/.cache");
        std::fs::create_dir_all(&cache_dir)?;
        *guard = Some(
            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                    .with_cache_dir(cache_dir)
                    .with_show_download_progress(true),
            )
            .context("Failed to initialize embedding model")?,
        );
    }
    guard
        .as_mut()
        .unwrap()
        .embed(texts, None)
        .context("Embedding failed")
}

/// Update the embeddings cache for a set of canonical entries (already merged from
/// musings.ndjson + brain.ndjson by think.rs). Only new IDs not yet in the cache are embedded.
pub fn update_embeddings(root: &Path, canonical: &[MemoryEntry]) -> Result<()> {
    let embeddings_path = root.join(".medulla/embeddings.parquet");

    if canonical.is_empty() {
        return Ok(());
    }

    // Read the existing cache once — use it for both ID dedup and the final merge.
    let existing_df: Option<DataFrame> = if embeddings_path.exists() {
        let mut f = File::open(&embeddings_path)?;
        Some(ParquetReader::new(&mut f).finish()?)
    } else {
        None
    };

    let existing_ids: std::collections::HashSet<String> = match &existing_df {
        Some(df) => df
            .column("id")?
            .str()?
            .into_no_null_iter()
            .map(|s| s.to_string())
            .collect(),
        None => std::collections::HashSet::new(),
    };

    let new_entries: Vec<&MemoryEntry> = canonical
        .iter()
        .filter(|e| !existing_ids.contains(&e.id))
        .collect();

    if new_entries.is_empty() {
        return Ok(());
    }

    let texts: Vec<&str> = new_entries.iter().map(|e| e.content.as_str()).collect();
    let new_embeddings = embed_texts(root, texts)?;

    let n = new_entries.len();
    let id_col: Column = Series::new(
        "id".into(),
        new_entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
    )
    .into();
    let embedding_col: Column = ListChunked::from_iter(
        new_embeddings
            .iter()
            .map(|e| Series::new("".into(), e.as_slice())),
    )
    .into_series()
    .with_name("embedding".into())
    .into();

    let new_df = DataFrame::new(n, vec![id_col, embedding_col])?;

    let mut merged = match existing_df {
        Some(df) => df.vstack(&new_df)?,
        None => new_df,
    };

    let tmp = embeddings_path.with_extension("tmp");
    let mut out = File::create(&tmp)?;
    ParquetWriter::new(&mut out).finish(&mut merged)?;
    drop(out);
    std::fs::rename(&tmp, &embeddings_path)?;
    Ok(())
}

pub fn find_similar(root: &Path, query: &str, k: usize, threshold: f32) -> Result<Vec<String>> {
    let embeddings_path = root.join(".medulla/embeddings.parquet");
    if !embeddings_path.exists() {
        return Ok(Vec::new());
    }

    let query_embeddings = embed_texts(root, vec![query])?;
    let q = &query_embeddings[0];

    let mut f = File::open(&embeddings_path)?;
    let df = ParquetReader::new(&mut f).finish()?;
    let ids = df.column("id")?.str()?;
    let list_col = df.column("embedding")?.list()?;

    let mut scored: Vec<(f32, String)> = Vec::new();
    for i in 0..df.height() {
        let Some(series) = list_col.get_as_series(i) else {
            continue;
        };
        let floats: Vec<f32> = series.f32()?.into_no_null_iter().collect();
        if floats.len() != q.len() {
            continue;
        }
        // fastembed outputs L2-normalised vectors, so cosine similarity == dot product
        let sim: f32 = q.iter().zip(floats.iter()).map(|(x, y)| x * y).sum();
        if sim >= threshold
            && let Some(id) = ids.get(i)
        {
            scored.push((sim, id.to_string()));
        }
    }

    scored.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().take(k).map(|(_, id)| id).collect())
}
