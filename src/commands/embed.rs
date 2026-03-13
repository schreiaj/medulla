use crate::core::MemoryEntry;
use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use polars::prelude::*;
use std::fs::File;
use std::path::Path;
use std::sync::Mutex;

static EMBEDDER: Mutex<Option<TextEmbedding>> = Mutex::new(None);

/// Locate the ONNX Runtime shared library without touching any ort code.
///
/// Checks (in order):
///   1. ORT_DYLIB_PATH env var (set by `nix develop` shellHook or the user)
///   2. Common Homebrew / system install paths
///
/// Returns the path string so the caller can set ORT_DYLIB_PATH before the
/// first ort call, ensuring ort's LazyLock finds the library without panicking.
fn find_ort_dylib() -> Option<String> {
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH")
        && !p.is_empty()
        && std::path::Path::new(&p).exists()
    {
        return Some(p);
    }

    #[cfg(target_os = "macos")]
    let candidates: &[&str] = &[
        "/opt/homebrew/lib/libonnxruntime.dylib",
        "/usr/local/lib/libonnxruntime.dylib",
    ];
    #[cfg(not(target_os = "macos"))]
    let candidates: &[&str] = &[
        "/usr/lib/libonnxruntime.so",
        "/usr/lib/x86_64-linux-gnu/libonnxruntime.so",
        "/usr/local/lib/libonnxruntime.so",
    ];

    candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| p.to_string())
}

fn build_execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    use ort::execution_providers::CPUExecutionProvider;

    let accelerator = std::env::var("MED_ACCELERATOR")
        .unwrap_or_default()
        .to_lowercase();

    let mut eps = Vec::new();

    match accelerator.as_str() {
        #[cfg(target_os = "macos")]
        "coreml" => {
            use ort::execution_providers::CoreMLExecutionProvider;
            println!(
                "[MED] GPU acceleration: CoreML (Neural Engine + GPU). First run compiles the model and may take several minutes."
            );
            eps.push(CoreMLExecutionProvider::default().build());
        }
        #[cfg(not(target_os = "macos"))]
        "cuda" => {
            use ort::execution_providers::CUDAExecutionProvider;
            println!("[MED] GPU acceleration: CUDA.");
            eps.push(CUDAExecutionProvider::default().build());
        }
        _ => {}
    }

    eps.push(CPUExecutionProvider::default().build());
    eps
}

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
    // Recover from a poisoned mutex (e.g. a panic in a previous init attempt).
    let mut guard = EMBEDDER.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        // ort-load-dynamic: locate the dylib BEFORE calling any ort code.
        // ort's LazyLock panics (possibly abort()) if the dylib is missing, so
        // we pre-check and return a clean Err instead of crashing.
        // ort-download-binaries: ORT is statically linked — no dylib needed.
        #[cfg(feature = "ort-load-dynamic")]
        {
            let dylib = find_ort_dylib().ok_or_else(|| {
                anyhow::anyhow!(
                    "ONNX Runtime library not found (semantic search disabled).\n\
                     Install it with `brew install onnxruntime` (macOS) or your\n\
                     distro's package manager, then set:\n\
                     \n\
                     export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib\n\
                     \n\
                     Or point ORT_DYLIB_PATH at wherever libonnxruntime is installed."
                )
            })?;
            // SAFETY: called under the EMBEDDER mutex before any ort code runs.
            unsafe { std::env::set_var("ORT_DYLIB_PATH", &dylib) };
        }

        println!("[MED] Initializing embedding model (first run downloads ~23MB)...");
        let cache_dir = root.join(".medulla/.cache");
        std::fs::create_dir_all(&cache_dir)?;
        // catch_unwind as a final safety net in case ort panics for other reasons.
        let init_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                    .with_cache_dir(cache_dir)
                    .with_show_download_progress(true)
                    .with_execution_providers(build_execution_providers()),
            )
        }));
        match init_result {
            Ok(Ok(embedder)) => *guard = Some(embedder),
            Ok(Err(e)) => {
                return Err(anyhow::anyhow!(
                    "Failed to initialize embedding model: {}",
                    e
                ));
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "ONNX Runtime panicked during initialization. \
                     Check that ORT_DYLIB_PATH points to a valid libonnxruntime shared library."
                ));
            }
        }
    }
    guard
        .as_mut()
        .unwrap()
        .embed(texts, Some(32))
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

    const CHUNK_SIZE: usize = 100;
    let total_chunks = new_entries.len().div_ceil(CHUNK_SIZE);

    let mut chunk_dfs: Vec<DataFrame> = Vec::new();
    for (chunk_idx, chunk) in new_entries.chunks(CHUNK_SIZE).enumerate() {
        println!(
            "[MED] Embedding batch {}/{} ({} entries)...",
            chunk_idx + 1,
            total_chunks,
            chunk.len()
        );
        let texts: Vec<&str> = chunk.iter().map(|e| e.content.as_str()).collect();
        let embeddings = embed_texts(root, texts)?;

        let n = chunk.len();
        let id_col: Column = Series::new(
            "id".into(),
            chunk.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        )
        .into();
        let embedding_col: Column = ListChunked::from_iter(
            embeddings
                .iter()
                .map(|e| Series::new("".into(), e.as_slice())),
        )
        .into_series()
        .with_name("embedding".into())
        .into();

        chunk_dfs.push(DataFrame::new(n, vec![id_col, embedding_col])?);
        // embeddings Vec is dropped here, freeing memory between chunks
    }

    let new_df = chunk_dfs
        .into_iter()
        .reduce(|mut acc, df| {
            acc.vstack_mut(&df).expect("vstack failed");
            acc
        })
        .expect("at least one chunk");

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
