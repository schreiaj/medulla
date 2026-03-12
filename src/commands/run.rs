use polars::prelude::*;
use std::path::Path;
use anyhow::{Result, Context};
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct QueryResult {
    id: String,
    content: String,
    activation: f64,
    tags: String,
}

pub fn run(pattern: &str, limit: usize) -> Result<()> {
    run_in(Path::new("."), pattern, limit)
}

pub fn run_in(root: &Path, pattern: &str, limit: usize) -> Result<()> {
    let brain_path = root.join(".medulla/brain.parquet");
    let synapses_path = root.join(".medulla/synapses.parquet");

    if !brain_path.exists() {
        println!("The brain is empty. Try 'med learn' and 'med think' first.");
        return Ok(());
    }

    // 1. Search the Brain
    let mut file = std::fs::File::open(&brain_path)?;
    let df_brain = ParquetReader::new(&mut file).finish()?;

    // Case-insensitive regex search on content
    let filtered_lf = df_brain.lazy()
        .filter(col("content").str().contains_any(
            lit(vec![pattern.to_lowercase()]),
            false
        ))
        .sort(["activation"], SortMultipleOptions::default().with_order_descending(true))
        .limit(limit as u32);

    let results_df = filtered_lf.collect()?;

    // 2. Prepare Display Data
    let ids = results_df.column("id")?.str()?;
    let contents = results_df.column("content")?.str()?;
    let activations = results_df.column("activation")?.f64()?;
    let associations = results_df.column("associations")?.list()?;

    let mut display_rows = Vec::new();
    let mut all_found_tags = Vec::new();

    for i in 0..results_df.height() {
        let tags_series = associations.get(i).unwrap();
        let tags_str = tags_series.str()?.into_no_null_iter()
            .collect::<Vec<_>>()
            .join(", ");

        // Collect tags for Hebbian lookup
        for tag in tags_series.str()?.into_no_null_iter() {
            all_found_tags.push(tag.to_string());
        }

        display_rows.push(QueryResult {
            id: ids.get(i).unwrap_or("").to_string(),
            content: contents.get(i).unwrap_or("").to_string(),
            activation: activations.get(i).unwrap_or(0.0),
            tags: tags_str,
        });
    }

    // 3. Print the Memories
    println!("\n--- Top Memories (Ranked by ACT-R Activation) ---");
    println!("{}", Table::new(display_rows).to_string());

    // 4. Hebbian Suggestions
    if synapses_path.exists() && !all_found_tags.is_empty() {
        render_suggestions(synapses_path, all_found_tags)?;
    }

    Ok(())
}

fn render_suggestions(path: std::path::PathBuf, tags: Vec<String>) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let df_syn = ParquetReader::new(&mut file).finish()?;

    // Find synapses where EITHER tag_a or tag_b matches our search tags
    let tags_series = Series::new("tags".into(), &tags).implode()?.into_series();
    let suggestions = df_syn.lazy()
        .filter(
            col("tag_a").is_in(lit(tags_series.clone()))
            .or(col("tag_b").is_in(lit(tags_series)))
        )
        .sort(["weight_log"], SortMultipleOptions::default().with_order_descending(true))
        .limit(5)
        .collect()?;

    if suggestions.height() > 0 {
        println!("\n--- Related Concepts (Hebbian Associations) ---");
        let a = suggestions.column("tag_a")?.str()?;
        let b = suggestions.column("tag_b")?.str()?;

        for i in 0..suggestions.height() {
            println!("  • {} ←→ {}", a.get(i).unwrap(), b.get(i).unwrap());
        }
    }
    Ok(())
}
