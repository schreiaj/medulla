use anyhow::Result;
use chrono::Utc;
use polars::prelude::*;
use rust_stemmers::{Algorithm, Stemmer};
use serde_json::Value;
use std::collections::HashMap;
use std::f64::consts::E;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::Path;
use std::sync::OnceLock;

use crate::core::MemoryEntry;

static EN_STEMMER: OnceLock<Stemmer> = OnceLock::new();

const DECAY_RATE: f64 = 0.5;
const LEARNING_RATE: f64 = 0.1;

pub fn run() -> Result<()> {
    run_in(Path::new("."))?;
    println!("✔ Memory consolidated.");
    Ok(())
}

pub fn run_in(root: &Path) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    let musings_path = root.join(".medulla/musings.ndjson");

    // Hold a shared lock for the full consolidation to prevent concurrent writes
    // from learn/reinforce_memories corrupting the snapshot we're reading.
    let _lock = if musings_path.exists() {
        Some(crate::core::lock_musings(&musings_path, true)?)
    } else {
        None
    };

    let t = std::time::Instant::now();
    println!("[MED] Loading memories...");
    let canonical = build_canonical_entries(root, now_ms)?;
    println!(
        "[MED] Compacting memory store ({} entries)... ({:.1}s)",
        canonical.len(),
        t.elapsed().as_secs_f32()
    );
    compact_musings(root, &canonical)?;
    println!(
        "[MED] Consolidating ACT-R model... ({:.1}s)",
        t.elapsed().as_secs_f32()
    );
    consolidate_entries(root, now_ms, &canonical)?;
    println!(
        "[MED] Updating Hebbian synapses... ({:.1}s)",
        t.elapsed().as_secs_f32()
    );
    update_synapses(root, now_ms, &canonical)?;
    println!(
        "[MED] Updating embeddings... ({:.1}s)",
        t.elapsed().as_secs_f32()
    );
    if let Err(e) = crate::commands::embed::update_embeddings(root, &canonical) {
        eprintln!("[MED] Warning: embedding update skipped: {}", e);
    }
    println!("[MED] Done. ({:.1}s total)", t.elapsed().as_secs_f32());
    Ok(())
}

/// Write the deduplicated canonical entries back to musings.ndjson atomically,
/// replacing the potentially unbounded append-only file with a compact snapshot.
fn compact_musings(root: &Path, canonical: &[MemoryEntry]) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let tmp_path = musings_path.with_extension("tmp");
    {
        let mut file = File::create(&tmp_path)?;
        for entry in canonical {
            let line = serde_json::to_string(entry)?;
            writeln!(file, "{}", line)?;
        }
    }
    fs::rename(&tmp_path, &musings_path)?;
    Ok(())
}

/// Merge local musings (ACT-R metadata) with the Git-tracked brain.ndjson (authoritative text).
///
/// Local musings are read first as the stateful baseline. brain.ndjson is then overlaid:
/// for entries that exist locally, only content and tags are updated (preserving access_count
/// and timestamps). Entries present only in brain.ndjson — e.g. learned by a teammate and
/// merged via `git pull` — are rehydrated with default metadata so the graph can process them.
fn build_canonical_entries(root: &Path, now_ms: i64) -> Result<Vec<MemoryEntry>> {
    let local_ndjson = root.join(".medulla/musings.ndjson");
    let public_ndjson = root.join("brain.ndjson");
    let mut map: HashMap<String, MemoryEntry> = HashMap::new();

    // 1. Read local musings (stateful baseline)
    if local_ndjson.exists() {
        let file = File::open(&local_ndjson)?;
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<MemoryEntry>(&line) {
                let e = map.entry(entry.id.clone()).or_insert_with(|| entry.clone());
                if entry.timestamp >= e.timestamp {
                    *e = entry;
                }
            }
        }
    }

    // 2. Overlay brain.ndjson (Git-tracked, authoritative for text and tags)
    if public_ndjson.exists() {
        let file = File::open(&public_ndjson)?;
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(stateless) = serde_json::from_str::<Value>(&line) {
                let id = stateless["id"].as_str().unwrap_or("").to_string();
                let content = stateless["content"].as_str().unwrap_or("").to_string();
                let tags: Vec<String> = stateless["tags"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                if id.is_empty() {
                    continue;
                }

                let stemmer = EN_STEMMER.get_or_init(|| Stemmer::create(Algorithm::English));
                let stems: Vec<String> = tags.iter().map(|t| stemmer.stem(t).to_string()).collect();
                let source = stateless["source"].as_str().map(String::from);

                if let Some(existing) = map.get_mut(&id) {
                    // Git is the source of truth for facts — update text and tags
                    // while preserving the agent's local access_count and timestamps.
                    existing.content = content;
                    existing.tags = tags;
                    existing.associations = stems;
                    existing.source = source;
                } else {
                    // New entry from a teammate via git pull — rehydrate with default metadata.
                    map.insert(
                        id.clone(),
                        MemoryEntry {
                            id,
                            content,
                            tags,
                            associations: stems,
                            source,
                            timestamp: now_ms,
                            access_count: 1,
                            last_access: now_ms,
                        },
                    );
                }
            }
        }
    }

    Ok(map.into_values().collect())
}

/// Serialize canonical entries to an in-memory NDJSON buffer for Polars.
fn entries_to_df(canonical: &[MemoryEntry]) -> Result<DataFrame> {
    let ndjson = canonical
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    let df = JsonReader::new(Cursor::new(ndjson.into_bytes()))
        .with_json_format(JsonFormat::JsonLines)
        .finish()?;
    // When all entries have empty `tags`, polars infers List(Null). Cast to List(String)
    // so downstream code can always call .str()? on the inner series.
    // For entries that predate dual-store (tags is empty), fall back to associations.
    Ok(df
        .lazy()
        .with_column(col("tags").cast(DataType::List(Box::new(DataType::String))))
        .with_column(
            when(col("tags").list().len().eq(lit(0)))
                .then(col("associations"))
                .otherwise(col("tags"))
                .alias("tags"),
        )
        .collect()?)
}

fn consolidate_entries(root: &Path, now_ms: i64, canonical: &[MemoryEntry]) -> Result<()> {
    let brain_path = root.join(".medulla/brain.parquet");
    if canonical.is_empty() {
        return Ok(());
    }

    let processed_lf = entries_to_df(canonical)?
        .lazy()
        .with_column(
            (((lit(now_ms) - col("timestamp")).cast(DataType::Float64) / lit(1000.0) + lit(1.0))
                .pow(lit(-DECAY_RATE))
                .log(lit(E)))
            .alias("activation"),
        )
        .sort(
            ["id", "timestamp"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .unique(
            Some(Selector::ByName {
                names: vec!["id".into()].into(),
                strict: true,
            }),
            UniqueKeepStrategy::First,
        );

    let mut df = processed_lf.collect()?;
    let brain_tmp = brain_path.with_extension("tmp");
    let mut file = File::create(&brain_tmp)?;
    ParquetWriter::new(&mut file).finish(&mut df)?;
    drop(file);
    fs::rename(&brain_tmp, &brain_path)?;
    Ok(())
}

pub fn update_synapses(root: &Path, now_ms: i64, canonical: &[MemoryEntry]) -> Result<()> {
    let synapses_path = root.join(".medulla/synapses.parquet");
    if canonical.is_empty() {
        return Ok(());
    }

    // Accumulate (tag_a, tag_b) co-occurrence pairs directly in Rust — avoids the
    // O(rows²) intermediate that the Polars self-join produced for entries with many tags.
    // Key: (tag_a, tag_b) with tag_a < tag_b lexicographically.
    // Value: (signal_count, last_seen_ms).
    let mut pairs: HashMap<(String, String), (u64, i64)> = HashMap::new();

    for entry in canonical {
        let tags = &entry.associations;
        if tags.len() < 2 {
            continue;
        }
        for i in 0..tags.len() {
            for j in (i + 1)..tags.len() {
                let (a, b) = if tags[i] <= tags[j] {
                    (tags[i].clone(), tags[j].clone())
                } else {
                    (tags[j].clone(), tags[i].clone())
                };
                let e = pairs.entry((a, b)).or_insert((0, i64::MIN));
                e.0 += 1;
                if entry.timestamp > e.1 {
                    e.1 = entry.timestamp;
                }
            }
        }
    }

    if pairs.is_empty() {
        return Ok(());
    }

    let log_inc = (1.0 + LEARNING_RATE).ln();

    let mut tag_a_col: Vec<&str> = Vec::with_capacity(pairs.len());
    let mut tag_b_col: Vec<&str> = Vec::with_capacity(pairs.len());
    let mut weight_col: Vec<f64> = Vec::with_capacity(pairs.len());
    let mut last_seen_col: Vec<i64> = Vec::with_capacity(pairs.len());

    for ((a, b), (count, last_seen)) in &pairs {
        let dt = (now_ms - last_seen) as f64 / 1000.0 + 1.0;
        let weight = (*count as f64 * log_inc) - (dt.ln() * DECAY_RATE);
        tag_a_col.push(a.as_str());
        tag_b_col.push(b.as_str());
        weight_col.push(weight);
        last_seen_col.push(*last_seen);
    }

    let n = pairs.len();
    let mut final_synapses = DataFrame::new(
        n,
        vec![
            Series::new("tag_a".into(), tag_a_col).into(),
            Series::new("tag_b".into(), tag_b_col).into(),
            Series::new("weight_log".into(), weight_col).into(),
            Series::new("last_seen".into(), last_seen_col).into(),
        ],
    )?;

    let syn_tmp = synapses_path.with_extension("tmp");
    let mut file = File::create(&syn_tmp)?;
    ParquetWriter::new(&mut file).finish(&mut final_synapses)?;
    drop(file);
    fs::rename(&syn_tmp, &synapses_path)?;

    Ok(())
}
