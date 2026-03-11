# Sentence Embedding Support for Medulla

## Context
The existing query system matches memories by stemmed keyword against content/tags, then ranks by ACT-R activation. This misses semantically related content that doesn't share exact words (e.g., querying "octopus disguise" won't find "cephalopod camouflage"). Adding sentence embeddings allows semantic expansion of the candidate set before the existing ACT-R ranking takes over.

## Approach
- Use `fastembed-rs` (all-MiniLM-L6-v2, 384-dim, ~23MB download on first use)
- Store embeddings in a new cache: `.medulla/embeddings.parquet` (`id: Utf8, embedding: List(Float32)`)
- Compute embeddings incrementally in `think` (only new IDs, skip existing cache)
- At query time: embed the raw query, cosine-sim scan, use top-k IDs above a threshold as an OR expansion to the existing keyword filter
- Graceful degradation: if embeddings.parquet is missing or model fails, keyword-only path continues unchanged

## Files to Create
- `src/commands/embed.rs` — new module with all embedding logic

## Files to Modify
- `Cargo.toml` — add `fastembed = "4"`
- `src/commands/mod.rs` — add `pub mod embed;`
- `src/commands/think.rs:31` — call `embed::update_embeddings(root)?` after `update_synapses`
- `src/commands/query.rs:75-81` — insert `find_similar` call and expand filter with OR clause
- `src/commands/init.rs:17-21` — add `.medulla/embeddings.parquet` to gitignore entries
- `tests/integration.rs` — add cosine_similarity unit tests + Parquet roundtrip test + `#[ignore]` model tests

---

## Step 1: `Cargo.toml`

```toml
fastembed = { version = "4", default-features = false, features = ["ort-download-binaries"] }
```

## Step 2: `src/commands/embed.rs`

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use polars::prelude::*;
use std::path::Path;
use std::fs::File;
use std::sync::OnceLock;
use anyhow::{Result, Context};
use crate::core::MemoryEntry;

static EMBEDDER: OnceLock<TextEmbedding> = OnceLock::new();

const EMBEDDING_K: usize = 10;
const EMBEDDING_THRESHOLD: f32 = 0.70;

fn get_or_init_embedder(print_hint: bool) -> Result<&'static TextEmbedding> {
    if EMBEDDER.get().is_none() && print_hint {
        println!("[MED] Initializing embedding model (first run downloads ~23MB)...");
    }
    EMBEDDER.get_or_try_init(|| {
        TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
    }).context("Failed to initialize embedding model")
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}

pub fn update_embeddings(root: &Path) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let embeddings_path = root.join(".medulla/embeddings.parquet");

    if !musings_path.exists() { return Ok(()); }

    // Read and deduplicate musings (latest timestamp wins, matching think.rs behavior)
    let all_entries: Vec<MemoryEntry> = {
        use std::io::{BufRead, BufReader};
        BufReader::new(File::open(&musings_path)?)
            .lines()
            .filter_map(|l| l.ok())
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<MemoryEntry>(&l).ok())
            .collect()
    };

    // Deduplicate: keep newest entry per ID (same rule as think::consolidate_entries)
    let mut deduped: std::collections::HashMap<String, MemoryEntry> = std::collections::HashMap::new();
    for entry in all_entries {
        let e = deduped.entry(entry.id.clone()).or_insert_with(|| entry.clone());
        if entry.timestamp > e.timestamp { *e = entry; }
    }
    let canonical: Vec<MemoryEntry> = deduped.into_values().collect();

    if canonical.is_empty() { return Ok(()); }

    // Load existing cache IDs
    let existing_ids: std::collections::HashSet<String> = if embeddings_path.exists() {
        let mut f = File::open(&embeddings_path)?;
        ParquetReader::new(&mut f).finish()?
            .column("id")?.str()?.into_no_null_iter()
            .map(|s| s.to_string()).collect()
    } else {
        std::collections::HashSet::new()
    };

    let new_entries: Vec<&MemoryEntry> = canonical.iter()
        .filter(|e| !existing_ids.contains(&e.id)).collect();

    if new_entries.is_empty() { return Ok(()); }

    let embedder = get_or_init_embedder(!embeddings_path.exists())?;
    let texts: Vec<&str> = new_entries.iter().map(|e| e.content.as_str()).collect();
    let new_embeddings = embedder.embed(texts, None)?;

    let id_series = Series::new("id".into(),
        new_entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>());
    let embedding_series: Series = ListChunked::from_iter(
        new_embeddings.iter().map(|e| Series::new("".into(), e.as_slice()))
    ).into_series().with_name("embedding".into());

    let mut new_df = DataFrame::new(vec![id_series, embedding_series])?;

    let mut merged = if embeddings_path.exists() {
        let mut f = File::open(&embeddings_path)?;
        ParquetReader::new(&mut f).finish()?.vstack(&new_df)?
    } else {
        new_df
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
    if !embeddings_path.exists() { return Ok(Vec::new()); }

    let embedder = get_or_init_embedder(false)?;
    let query_vec = embedder.embed(vec![query], None)?;
    let q = &query_vec[0];

    let mut f = File::open(&embeddings_path)?;
    let df = ParquetReader::new(&mut f).finish()?;
    let ids = df.column("id")?.str()?;
    let list_col = df.column("embedding")?.list()?;

    let mut scored: Vec<(f32, String)> = Vec::new();
    for i in 0..df.height() {
        let Some(series) = list_col.get_as_series(i) else { continue };
        let floats: Vec<f32> = series.f32()?.into_no_null_iter().collect();
        if floats.len() != q.len() { continue; }
        let sim = cosine_similarity(q, &floats);
        if sim >= threshold {
            if let Some(id) = ids.get(i) {
                scored.push((sim, id.to_string()));
            }
        }
    }

    scored.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().take(k).map(|(_, id)| id).collect())
}
```

## Step 3: `src/commands/mod.rs`

Add `pub mod embed;`.

## Step 4: `src/commands/think.rs` — integration

After line 30 (`update_synapses(root, now_ms)?`), add:
```rust
crate::commands::embed::update_embeddings(root)?;
```

Update the success message at line 13 to mention embeddings.

**Note on offline/CI:** Consider wrapping with a warning-only path (`let _ = embed::update_embeddings(root)`) if offline model download is a concern.

## Step 5: `src/commands/query.rs` — integration

Replace the `all_results` block (lines 75-81) with:

```rust
// Semantic expansion — degrade gracefully if embeddings unavailable
let semantic_ids: Vec<String> = crate::commands::embed::find_similar(
    root, pattern, 10, 0.70,
).unwrap_or_default();

let base_filter =
    col("content").str().to_lowercase().str().contains(lit(pattern_stemmed.clone()), false)
    .or(col("associations").list().contains(lit(pattern_stemmed.clone()), false));

let combined_filter = if semantic_ids.is_empty() {
    base_filter
} else {
    let id_series = Series::new("semantic_ids".into(), semantic_ids);
    base_filter.or(col("id").is_in(lit(id_series), false))
};

let all_results = df_brain.lazy()
    .filter(combined_filter)
    .sort(["activation"], SortMultipleOptions::default().with_order_descending(true))
    .collect()?;
```

Use `pattern` (raw, not stemmed) for `find_similar` — embedding models work better with natural language than stemmed tokens.

## Step 6: `src/commands/init.rs`

Add `".medulla/embeddings.parquet"` to the `ignore_entries` array.

## Step 7: Tests in `tests/integration.rs`

**Fast tests (no model download):**
- `test_cosine_similarity_identical_vectors` — sim = 1.0
- `test_cosine_similarity_orthogonal_vectors` — sim = 0.0
- `test_cosine_similarity_zero_vector` — returns 0.0 without panic
- `test_embeddings_parquet_roundtrip` — `List(Float32)` write/read survives intact
- `test_find_similar_returns_empty_without_embeddings_file` — `Ok(vec![])` when no cache

**`#[ignore]` tests (require model download, run with `-- --include-ignored`):**
- `test_update_embeddings_is_incremental` — second run does not duplicate rows
- `test_semantic_query_finds_related_content` — "octopus disguise" finds "cephalopod camouflage" entry

---

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Embed content, not tags | Full sentences give the model sufficient context; 1-2 word tags are too short |
| Compute in `think`, not `learn` | Batch is cheaper; `think` already runs before every query via auto-consolidation |
| `unwrap_or_default()` in query | Semantic search should degrade, not block, on model failure |
| `?` in think | Embedding failures during consolidation are surfaced explicitly |
| Raw `pattern` not `pattern_stemmed` for embedding | Stemmed strings confuse tokenizers |
| Threshold 0.70, k=10 | Conservative — avoids noise while expanding meaningfully |
| Deduplicate musings before embedding | Matches `think.rs` "latest timestamp wins" rule; avoids stale embeddings |

## Verification

```bash
# Build
cargo build

# Fast tests (no download)
cargo test

# Full suite including model download
cargo test -- --include-ignored

# Manual end-to-end
med init
med learn "The octopus can camouflage by changing skin texture." --tags cephalopod
med think
med query "octopus disguise"   # should surface the cephalopod entry via semantic similarity
```
