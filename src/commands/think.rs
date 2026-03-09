use polars::prelude::*;
use std::path::Path;
use std::fs::{self, File};
use std::f64::consts::E;
use chrono::Utc;
use anyhow::{Result, Context};

const DECAY_RATE: f64 = 0.5;
const LEARNING_RATE: f64 = 0.1;

pub fn run() -> Result<()> {
    run_in(Path::new("."))?;
    println!("✔ Brain consolidated. Recency bias and Hebbian synapses updated.");
    Ok(())
}

pub fn run_in(root: &Path) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    let musings_path = root.join(".medulla/musings.ndjson");

    // Hold a shared lock for the full consolidation to prevent concurrent writes
    // from learn/reinforce_memories corrupting the snapshot we're reading.
    let _lock = if musings_path.exists() {
        Some(crate::core::lock_musings(&musings_path, false)?)
    } else {
        None
    };

    consolidate_entries(root, now_ms)?;
    update_synapses(root, now_ms)?;
    Ok(())
}

fn consolidate_entries(root: &Path, now_ms: i64) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let brain_path = root.join(".medulla/brain.parquet");
    if !musings_path.exists() { return Ok(()); }

    let path_str = musings_path.to_string_lossy();
    let lf = LazyJsonLineReader::new(path_str.as_ref().into()).finish()?;

    let processed_lf = lf
        .with_column(
            (
                ((lit(now_ms) - col("timestamp")).cast(DataType::Float64) / lit(1000.0) + lit(1.0))
                .pow(lit(-DECAY_RATE))
                .log(lit(E))
            ).alias("activation")
        )
        .sort(["id", "timestamp"], SortMultipleOptions::default().with_order_descending(true))
        .unique(
            Some(Selector::ByName {
                names: vec!["id".into()].into(),
                strict: true,
            }),
            UniqueKeepStrategy::First
        );

    let mut df = processed_lf.collect()?;
    let brain_tmp = brain_path.with_extension("tmp");
    let mut file = File::create(&brain_tmp)?;
    ParquetWriter::new(&mut file).finish(&mut df)?;
    drop(file);
    fs::rename(&brain_tmp, &brain_path)?;
    Ok(())
}

pub fn update_synapses(root: &Path, now_ms: i64) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let synapses_path = root.join(".medulla/synapses.parquet");
    if !musings_path.exists() { return Ok(()); }

    let path_str = musings_path.to_string_lossy();
    let lf_musings = LazyJsonLineReader::new(path_str.as_ref().into()).finish()
        .context("Failed to read musings for Hebbian reconstruction")?;

    let explode_sel = Selector::ByName {
        names: vec!["associations".into()].into(),
        strict: true,
    };
    let explode_opts = ExplodeOptions {
        empty_as_null: true,
        keep_nulls: false,
    };

    // Materialize once to avoid reading musings.ndjson twice for the self-join
    let base_df = lf_musings
        .filter(col("associations").list().len().gt(lit(1)))
        .explode(explode_sel, explode_opts)
        .collect()?;

    let left = base_df.clone().lazy()
        .rename(["associations"], ["tag_a"], true);

    let right = base_df.lazy()
        .rename(["associations"], ["tag_b"], true);

    let log_inc = (1.0 + LEARNING_RATE).ln();

    let signals = left.join(
        right,
        [col("id")],
        [col("id")],
        JoinType::Inner.into()
    )
    .filter(col("tag_a").lt(col("tag_b")))
    .group_by([col("tag_a"), col("tag_b")])
    .agg([
        len().alias("signal_count"),
        col("timestamp").max().alias("last_seen")
    ]);

    let mut final_synapses = signals
        .with_column(
            (
                (col("signal_count").cast(DataType::Float64) * lit(log_inc))
                - (
                    ((lit(now_ms) - col("last_seen")).cast(DataType::Float64) / lit(1000.0) + lit(1.0))
                    .log(lit(E)) * lit(DECAY_RATE)
                )
            ).alias("weight_log")
        )
        .select([
            col("tag_a"),
            col("tag_b"),
            col("weight_log"),
            col("last_seen")
        ])
        .collect()?;

    let syn_tmp = synapses_path.with_extension("tmp");
    let mut file = File::create(&syn_tmp)?;
    ParquetWriter::new(&mut file).finish(&mut final_synapses)?;
    drop(file);
    fs::rename(&syn_tmp, &synapses_path)?;

    Ok(())
}
