use polars::prelude::*;
use std::path::Path;
use anyhow::Result;
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct QueryResult {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Memory Content")]
    content: String,
    #[tabled(rename = "Activation")]
    activation: f64,
    #[tabled(rename = "Tags")]
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

    let mut file = std::fs::File::open(&brain_path)?;
    let df_brain = ParquetReader::new(&mut file).finish()?;

    // Search and Rank by ACT-R Activation
    let filtered_lf = df_brain.lazy()
        .filter(
            col("content").str().contains_literal(lit(pattern.to_lowercase()))
        )
        .sort(["activation"], SortMultipleOptions::default().with_order_descending(true))
        .limit(limit as u32);

    let results_df = filtered_lf.collect()?;

    let ids = results_df.column("id")?.str()?;
    let contents = results_df.column("content")?.str()?;
    let activations = results_df.column("activation")?.f64()?;
    let associations = results_df.column("associations")?;

    let mut display_rows = Vec::new();
    let mut all_found_tags = Vec::new();

    for i in 0..results_df.height() {
        let val = associations.get(i)?;
        let tags_series = Series::from_any_values("tags".into(), &[val], false)?;

        // FIX: Use the fields specified by the 0.53 compiler error
        let tags_ca = tags_series
            .explode(ExplodeOptions {
                empty_as_null: true,
                keep_nulls: true
            })?
            .str()?
            .clone();

        let tags_vec: Vec<String> = tags_ca.into_no_null_iter()
            .map(|s| s.to_string())
            .collect();

        let tags_str = tags_vec.join(", ");
        all_found_tags.extend(tags_vec);

        display_rows.push(QueryResult {
            id: ids.get(i).unwrap_or("").to_string(),
            content: contents.get(i).unwrap_or("").to_string(),
            activation: activations.get(i).unwrap_or(0.0),
            tags: tags_str,
        });
    }

    println!("\n--- Top Memories (Ranked by ACT-R Activation) ---");
    if display_rows.is_empty() {
        println!("No memories found matching '{}'.", pattern);
    } else {
        println!("{}", Table::new(display_rows).to_string());
    }

    if synapses_path.exists() && !all_found_tags.is_empty() {
        render_suggestions(synapses_path, all_found_tags)?;
    }

    Ok(())
}

fn render_suggestions(path: std::path::PathBuf, tags: Vec<String>) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let df_syn = ParquetReader::new(&mut file).finish()?;

    let mut unique_tags = tags;
    unique_tags.sort();
    unique_tags.dedup();
    let tag_series = Series::new("search_tags".into(), unique_tags);

    let suggestions = df_syn.lazy()
        .filter(
            col("tag_a").is_in(lit(tag_series.clone()), false)
            .or(col("tag_b").is_in(lit(tag_series), false))
        )
        .sort(["weight_log"], SortMultipleOptions::default().with_order_descending(true))
        .limit(5)
        .collect()?;

    if suggestions.height() > 0 {
        println!("\n--- Related Concepts (Hebbian Associations) ---");
        let a = suggestions.column("tag_a")?.str()?;
        let b = suggestions.column("tag_b")?.str()?;
        let weight = suggestions.column("weight_log")?.f64()?;

        for i in 0..suggestions.height() {
            println!(
                "  • {:<15} ←→ {:<15} (strength: {:.2})",
                a.get(i).unwrap(),
                b.get(i).unwrap(),
                weight.get(i).unwrap()
            );
        }
        println!("");
    }
    Ok(())
}
