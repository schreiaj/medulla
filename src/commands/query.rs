use anyhow::{Context, Result};
use chrono::Utc;
use polars::prelude::*;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::SystemTime;
use tabled::{Table, Tabled, settings::Style};

use crate::core::{MemoryEntry, lock_musings};

static EN_STEMMER: OnceLock<Stemmer> = OnceLock::new();

#[derive(Tabled)]
struct MemoryRow {
    #[tabled(rename = "Memory")]
    content: String,
    #[tabled(rename = "Tags")]
    tags: String,
    #[tabled(rename = "Source")]
    source: String,
}

pub fn run(pattern: &str, limit: usize, threshold: f32) -> Result<()> {
    run_in(Path::new("."), pattern, limit, threshold)
}

pub fn run_in(root: &Path, pattern: &str, limit: usize, threshold: f32) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let brain_path = root.join(".medulla/brain.parquet");
    let synapses_path = root.join(".medulla/synapses.parquet");

    let public_ndjson = root.join("brain.ndjson");

    if !musings_path.exists() && !public_ndjson.exists() {
        println!("The mind is blank. Run 'med learn' to add memories.");
        return Ok(());
    }

    // 1. Auto-Consolidation Check — trigger if brain.parquet is missing or stale
    // relative to either the local musings or the Git-tracked brain.ndjson.
    let get_mtime = |p: &Path| -> SystemTime {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    };

    let embeddings_path = root.join(".medulla/embeddings.parquet");

    let needs_update = !brain_path.exists()
        || !embeddings_path.exists()
        || get_mtime(&musings_path) > get_mtime(&brain_path)
        || get_mtime(&public_ndjson) > get_mtime(&brain_path);

    if needs_update {
        println!("[MED] Data drift detected. Recompiling cognitive graph...");
        crate::commands::think::run_in(root)?;
    }

    let mut file = File::open(&brain_path)?;
    let df_brain = ParquetReader::new(&mut file).finish()?;

    let pattern_lowered = pattern.to_lowercase();
    let pattern_stemmed = EN_STEMMER
        .get_or_init(|| Stemmer::create(Algorithm::English))
        .stem(&pattern_lowered)
        .to_string();

    // 2. Search Logic — collect once, slice for display
    // Semantic expansion — degrade gracefully if embeddings unavailable
    let semantic_ids: Vec<String> =
        crate::commands::embed::find_similar(root, pattern, 10, threshold).unwrap_or_default();

    let base_filter = col("content")
        .str()
        .to_lowercase()
        .str()
        .contains(lit(pattern_stemmed.clone()), false)
        .or(col("associations")
            .list()
            .contains(lit(pattern_stemmed.clone()), false));

    let combined_filter = if semantic_ids.is_empty() {
        base_filter
    } else {
        let id_series = Series::new("semantic_ids".into(), semantic_ids)
            .implode()?
            .into_series();
        base_filter.or(col("id").is_in(lit(id_series), false))
    };

    let all_results = df_brain
        .clone()
        .lazy()
        .filter(combined_filter)
        .sort(
            ["activation"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .collect()?;

    if all_results.height() == 0 {
        println!(
            "No memories found matching '{}' (stemmed as '{}').",
            pattern, pattern_stemmed
        );
        return Ok(());
    }

    // Limit the results actually shown (and therefore "accessed")
    let display_results = all_results.head(Some(limit));
    render_memories(&display_results, "Memories")?;

    // 3. Reinforce the accessed memories in the source of truth
    reinforce_memories(root, &display_results)?;

    // 4. Walk the Hebbian graph from the displayed results and show related memories
    if synapses_path.exists() {
        let mut syn_file = File::open(&synapses_path)?;
        let df_syn = ParquetReader::new(&mut syn_file).finish()?;

        let shown_ids: HashSet<String> = display_results
            .column("id")?
            .str()?
            .into_no_null_iter()
            .map(|s| s.to_string())
            .collect();

        let explode_opts = ExplodeOptions {
            empty_as_null: true,
            keep_nulls: false,
        };
        let found_tags_col = all_results.column("associations")?.explode(explode_opts)?;
        let found_tags = found_tags_col.as_materialized_series().clone();

        render_suggestions(df_syn, &df_brain, found_tags, &shown_ids, &pattern_stemmed)?;
    }

    Ok(())
}

// Write-back function to increment access frequency and reset recency
fn reinforce_memories(root: &Path, displayed_df: &DataFrame) -> Result<()> {
    let ids_col = displayed_df.column("id")?.str()?;
    let accessed_ids: HashSet<&str> = ids_col.into_no_null_iter().collect();

    if accessed_ids.is_empty() {
        return Ok(());
    }

    let musings_path = root.join(".medulla/musings.ndjson");

    // Exclusive lock: prevent concurrent learn/think from racing this read-modify-write
    let _lock = lock_musings(&musings_path, true)?;

    let file = File::open(&musings_path)?;
    let reader = BufReader::new(file);

    let mut lines_out: Vec<String> = Vec::new();
    let mut updated = false;
    let now_ms = Utc::now().timestamp_millis();

    for line in reader.lines() {
        let line_str = line?;
        if line_str.trim().is_empty() {
            continue;
        }

        let new_line = match serde_json::from_str::<MemoryEntry>(&line_str) {
            Ok(mut entry) => {
                if accessed_ids.contains(entry.id.as_str()) {
                    entry.access_count += 1;
                    entry.last_access = now_ms;
                    updated = true;
                    serde_json::to_string(&entry)?
                } else {
                    line_str
                }
            }
            // Preserve lines we can't parse (e.g., newer schema versions) rather
            // than aborting or silently dropping data.
            Err(_) => line_str,
        };
        lines_out.push(new_line);
    }

    if updated {
        // Atomic write: write to a temp file then rename so a crash mid-write
        // never leaves musings.ndjson in a partial/corrupt state.
        let tmp_path = musings_path.with_extension("tmp");
        let mut out_file = File::create(&tmp_path)?;
        for line in &lines_out {
            writeln!(out_file, "{}", line)?;
        }
        drop(out_file);
        fs::rename(&tmp_path, &musings_path)?;
    }

    Ok(())
}

fn render_memories(df: &DataFrame, header: &str) -> Result<()> {
    let contents = df.column("content")?.str()?;
    // Use original tags for display; fall back to associations for old entries
    let tag_col = df
        .column("tags")
        .or_else(|_| df.column("associations"))?
        .list()?;
    let source_col = df.column("source").ok();

    let mut rows = Vec::new();
    for i in 0..df.height() {
        let tag_series = tag_col.get_as_series(i).context("Missing tags series")?;
        let tags_str = tag_series
            .str()?
            .into_no_null_iter()
            .collect::<Vec<_>>()
            .join(", ");

        let source = source_col
            .and_then(|col| col.str().ok())
            .and_then(|ca| ca.get(i))
            .unwrap_or("—")
            .to_string();

        rows.push(MemoryRow {
            content: contents
                .get(i)
                .context("Missing content value")?
                .to_string(),
            tags: tags_str,
            source,
        });
    }

    println!("\n{}:", header);
    let mut table = Table::new(rows);
    table.with(Style::rounded());
    println!("{}", table);
    Ok(())
}

fn render_suggestions(
    df_syn: DataFrame,
    df_brain: &DataFrame,
    found_tags: Series,
    shown_ids: &HashSet<String>,
    pattern_stemmed: &str,
) -> Result<()> {
    let tag_series = found_tags.unique()?;
    let tag_list_series = tag_series.implode()?.into_series();

    let suggestions = df_syn
        .lazy()
        .filter(
            col("tag_a")
                .is_in(lit(tag_list_series.clone()), false)
                .or(col("tag_b").is_in(lit(tag_list_series), false)),
        )
        .collect()?;

    if suggestions.height() == 0 {
        return Ok(());
    }

    let tag_a = suggestions.column("tag_a")?.str()?;
    let tag_b = suggestions.column("tag_b")?.str()?;
    let weights = suggestions.column("weight_log")?.f64()?;

    let mut concept_map: HashMap<String, f64> = HashMap::new();
    for i in 0..suggestions.height() {
        let a = tag_a.get(i).unwrap_or_default();
        let b = tag_b.get(i).unwrap_or_default();
        let w = weights.get(i).unwrap_or(0.0);
        if a != pattern_stemmed {
            let e = concept_map.entry(a.to_string()).or_insert(w);
            if w > *e {
                *e = w;
            }
        }
        if b != pattern_stemmed {
            let e = concept_map.entry(b.to_string()).or_insert(w);
            if w > *e {
                *e = w;
            }
        }
    }

    let mut sorted_concepts: Vec<_> = concept_map.into_iter().collect();
    sorted_concepts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_concepts: Vec<String> = sorted_concepts
        .into_iter()
        .take(3)
        .map(|(t, _)| t)
        .collect();

    if top_concepts.is_empty() {
        return Ok(());
    }

    // For each top concept, fetch related memories not already shown
    let mut related_rows: Vec<MemoryRow> = Vec::new();
    let mut seen: HashSet<String> = shown_ids.clone();

    for concept in &top_concepts {
        let candidates = df_brain
            .clone()
            .lazy()
            .filter(
                col("associations")
                    .list()
                    .contains(lit(concept.as_str()), false),
            )
            .sort(
                ["activation"],
                SortMultipleOptions::default().with_order_descending(true),
            )
            .limit(2)
            .collect()?;

        let contents = candidates.column("content")?.str()?;
        let ids = candidates.column("id")?.str()?;
        let tag_col = candidates
            .column("tags")
            .or_else(|_| candidates.column("associations"))?
            .list()?;
        let source_col = candidates.column("source").ok();

        for i in 0..candidates.height() {
            let id = ids.get(i).unwrap_or_default().to_string();
            if seen.contains(&id) {
                continue;
            }
            seen.insert(id);

            let content = contents.get(i).unwrap_or_default().to_string();
            let tags_str = tag_col
                .get_as_series(i)
                .map(|s| {
                    s.str()
                        .map(|ca| ca.into_no_null_iter().collect::<Vec<_>>().join(", "))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let source = source_col
                .and_then(|col| col.str().ok())
                .and_then(|ca| ca.get(i))
                .unwrap_or("—")
                .to_string();

            related_rows.push(MemoryRow {
                content,
                tags: tags_str,
                source,
            });
        }
    }

    if !related_rows.is_empty() {
        println!("\nRelated Memories:");
        let mut table = Table::new(related_rows);
        table.with(Style::rounded());
        println!("{}", table);
    }

    Ok(())
}
