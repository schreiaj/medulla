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
}

pub fn run(pattern: &str, limit: usize) -> Result<()> {
    run_in(Path::new("."), pattern, limit)
}

pub fn run_in(root: &Path, pattern: &str, limit: usize) -> Result<()> {
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

    let needs_update = !brain_path.exists()
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
    let all_results = df_brain
        .lazy()
        .filter(
            col("content")
                .str()
                .to_lowercase()
                .str()
                .contains(lit(pattern_stemmed.clone()), false)
                .or(col("associations")
                    .list()
                    .contains(lit(pattern_stemmed.clone()), false)),
        )
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
    render_memories(&display_results)?;

    // 3. Reinforce the accessed memories in the source of truth
    reinforce_memories(root, &display_results)?;

    // 4. Render Hebbian Suggestions
    if synapses_path.exists() {
        let mut syn_file = File::open(&synapses_path)?;
        let df_syn = ParquetReader::new(&mut syn_file).finish()?;

        let explode_opts = ExplodeOptions {
            empty_as_null: true,
            keep_nulls: false,
        };
        let found_tags_col = all_results.column("associations")?.explode(explode_opts)?;
        let found_tags = found_tags_col.as_materialized_series().clone();

        render_suggestions(df_syn, found_tags, &pattern_stemmed)?;
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

fn render_memories(df: &DataFrame) -> Result<()> {
    let contents = df.column("content")?.str()?;
    let associations = df.column("associations")?.list()?;

    let mut rows = Vec::new();
    for i in 0..df.height() {
        let tags_series = associations
            .get_as_series(i)
            .context("Missing associations series")?;
        let tags_str = tags_series
            .str()?
            .into_no_null_iter()
            .collect::<Vec<_>>()
            .join(", ");

        rows.push(MemoryRow {
            content: contents
                .get(i)
                .context("Missing content value")?
                .to_string(),
            tags: tags_str,
        });
    }

    println!("\nMemories:");
    let mut table = Table::new(rows);
    table.with(Style::rounded());
    println!("{}", table);
    Ok(())
}

fn render_suggestions(df_syn: DataFrame, found_tags: Series, pattern_stemmed: &str) -> Result<()> {
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

    if suggestions.height() > 0 {
        let tag_a = suggestions.column("tag_a")?.str()?;
        let tag_b = suggestions.column("tag_b")?.str()?;
        let weights = suggestions.column("weight_log")?.f64()?;

        let mut concept_map: HashMap<String, f64> = HashMap::new();

        for i in 0..suggestions.height() {
            let a = tag_a.get(i).unwrap_or_default();
            let b = tag_b.get(i).unwrap_or_default();
            let w = weights.get(i).unwrap_or(0.0);

            if a != pattern_stemmed {
                let entry = concept_map.entry(a.to_string()).or_insert(w);
                if w > *entry {
                    *entry = w;
                }
            }
            if b != pattern_stemmed {
                let entry = concept_map.entry(b.to_string()).or_insert(w);
                if w > *entry {
                    *entry = w;
                }
            }
        }

        let mut sorted_concepts: Vec<_> = concept_map.into_iter().collect();
        sorted_concepts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if !sorted_concepts.is_empty() {
            let concepts: Vec<String> = sorted_concepts
                .into_iter()
                .take(5)
                .map(|(tag, _)| tag)
                .collect();
            println!("\nRelated: {}", concepts.join(", "));
        }
    }

    Ok(())
}
