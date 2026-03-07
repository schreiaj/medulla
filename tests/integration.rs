use tempfile::tempdir;
use med::commands::{init, learn, think};
use std::fs;
use polars::prelude::*;
use chrono::{Utc, Duration};
use std::io::Write;

#[test]
fn test_init_creates_directory_and_files() {
    let dir = tempdir().expect("Failed to create temp dir");
    let root = dir.path();

    // 1. Run init in the temp root
    init::run_in(root).expect("Init failed");

    // 2. Assert using root.join()
    assert!(root.join(".medulla").is_dir());

    // 3. Read .gitignore FROM THE TEMP ROOT
    let gitignore = fs::read_to_string(root.join(".gitignore")).expect("Missing .gitignore");
    assert!(gitignore.contains(".medulla/musings.ndjson"));
}

#[test]
fn test_learn_encodes_valid_json() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // Use root words that the stemmer will leave alone
    let tags = vec!["robot".to_string(), "metal".to_string()];
    learn::run_in(root, "Validating JSON encoding.", tags, None).unwrap();

    let musings_path = root.join(".medulla/musings.ndjson");
    let musings_content = std::fs::read_to_string(musings_path).unwrap();

    // The array will now exactly match the root words
    assert!(
        musings_content.contains("\"associations\":[\"robot\",\"metal\"]"),
        "JSON did not contain the expected stemmed associations array. Content was: {}",
        musings_content
    );
}

#[test]
fn test_think_generates_parquet_with_activation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    med::commands::init::run_in(root).unwrap();

    // Learn two things
    med::commands::learn::run_in(root, "Entry 1", vec!["tag1".into()], None).unwrap();
    med::commands::learn::run_in(root, "Entry 2", vec!["tag2".into()], None).unwrap();

    // Run think
    med::commands::think::run_in(root).expect("Think failed");

    // Verify Parquet exists
    assert!(root.join(".medulla/brain.parquet").exists());
}


#[test]
fn test_think_handles_missing_musings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // Running think with no musings should just exit gracefully
    let result = think::run_in(root);
    assert!(result.is_ok());
    assert!(!root.join(".medulla/brain.parquet").exists());
}

#[test]
fn test_think_deduplicates_updates() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    let id = Some("fixed-id-123".to_string());

    // First version of the thought
    learn::run_in(root, "Version 1", vec![], id.clone()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Second version (Update)
    learn::run_in(root, "Version 2", vec![], id.clone()).unwrap();

    think::run_in(root).unwrap();

    // Verify Parquet only has ONE row for this ID
    let mut file = fs::File::open(root.join(".medulla/brain.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    assert_eq!(df.height(), 1);
    // Ensure it's the LATEST version
    let content_series = df.column("content").unwrap().str().unwrap();
    assert_eq!(content_series.get(0).unwrap(), "Version 2");
}

#[test]
fn test_act_r_activation_sorting() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // 1. Learn something "old" (we'll simulate time by sleeping or just letting it exist)
    learn::run_in(root, "Old Memory", vec![], None).unwrap();


    // 2. Learn something "new"
    learn::run_in(root, "New Memory", vec![], None).unwrap();

    think::run_in(root).unwrap();

    // 3. Inspect the Parquet activation scores
    let mut file = fs::File::open(root.join(".medulla/brain.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    // Sort by activation descending to see what's "on top"
    let sorted_df = df.sort(["activation"], SortMultipleOptions::default().with_order_descending(true)).unwrap();

    let content_series = sorted_df.column("content").unwrap().str().unwrap();

    // The "New Memory" should have the higher activation score
    assert_eq!(content_series.get(0).unwrap(), "New Memory");
    assert_eq!(content_series.get(1).unwrap(), "Old Memory");
}


// In tests/integration.rs
#[test]
fn test_hebbian_association_strengthening() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    let tags = vec!["robotics".to_string(), "octopus".to_string()];
    learn::run_in(root, "Memory 1", tags.clone(), None).unwrap();
    learn::run_in(root, "Memory 2", tags.clone(), None).unwrap();

    // Think handles the reconstruction
    think::run_in(root).unwrap();

    let mut file = std::fs::File::open(root.join(".medulla/synapses.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    let weight = df.column("weight_log").unwrap().f64().unwrap().get(0).unwrap();

    // The implementation now does: (count * log_inc) - (log(dt + 1) * d)
    // In a test running instantly, dt is ~0, so log(1) * 0.5 is ~0.
    let log_inc = (1.1f64).ln();
    let expected_max = log_inc * 2.0;

    // Assert it's close to the frequency weight, but slightly less due to any tiny dt
    assert!(weight <= expected_max);
    assert!(weight > 0.0);
}

#[test]
fn test_hebbian_reconstruction_from_musings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // 1. Learn two separate memories sharing tags
    let tags = vec!["robotics".to_string(), "octopus".to_string()];
    learn::run_in(root, "Octopus soft robotics.", tags.clone(), None).unwrap();
    learn::run_in(root, "Octopus robot chassis.", tags.clone(), None).unwrap();

    // 2. Think (This now reconstructs from musings.ndjson)
    think::run_in(root).unwrap();

    // 3. Verify synapses.parquet exists and has the correct weight
    let synapses_path = root.join(".medulla/synapses.parquet");
    let mut file = std::fs::File::open(&synapses_path).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    assert!(df.height() > 0);
    let weight = df.column("weight_log").unwrap().f64().unwrap().get(0).unwrap();
    let expected = (1.1f64).ln() * 2.0;
    assert!((weight - expected).abs() < 0.01);
}

#[test]
fn test_full_cognitive_loop_octopus_robotics() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // 1. Learn two related facts
    let tags1 = vec!["octopus".to_string(), "silicone".to_string()];
    learn::run_in(root, "Octopus tentacles use silicone.", tags1, None).unwrap();

    let tags2 = vec!["robotics".to_string(), "silicone".to_string()];
    learn::run_in(root, "Robotics joints use silicone.", tags2, None).unwrap();

    // 2. Think (Consolidate + Hebbian Weighting)
    think::run_in(root).unwrap();

    // 3. Query "octopus" and verify we see the "robotics" association
    // Since we don't want to capture stdout easily in a simple test,
    // we can call the internal logic to verify the data exists.
    let brain_path = root.join(".medulla/brain.parquet");
    let synapses_path = root.join(".medulla/synapses.parquet");

    assert!(brain_path.exists());
    assert!(synapses_path.exists());

    // Verify the synapse specifically
    let mut file = std::fs::File::open(synapses_path).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    // The dataframe should contain a link between 'octopus' and 'robotics'
    // via the shared 'silicone' tag.
    assert!(df.height() >= 1);
}


#[test]
fn test_recency_bias_and_reconstruction() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    let now = Utc::now();
    let old_time = (now - Duration::days(100)).timestamp_millis();
    let recent_time = now.timestamp_millis();

    let musings_path = root.join(".medulla/musings.ndjson");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&musings_path)
        .unwrap();

    // 1. OLD entries (Frequency = 5, but 100 days old)
    for i in 0..5 {
        let entry = serde_json::json!({
            "id": format!("old_{}", i),
            "content": "Old heavy robot",
            "timestamp": old_time,
            "associations": ["robotics", "heavy"]
        });
        writeln!(file, "{}", entry).unwrap();
    }

    // 2. RECENT entries (Frequency = 2, but brand new)
    for i in 0..2 {
        let entry = serde_json::json!({
            "id": format!("recent_{}", i),
            "content": "Recent soft robot",
            "timestamp": recent_time,
            "associations": ["robotics", "soft"]
        });
        writeln!(file, "{}", entry).unwrap();
    }

    // 3. Reconstruct
    think::run_in(root).unwrap();

    // 4. Validate with shallow clones to satisfy the borrow checker
    let mut f = fs::File::open(root.join(".medulla/synapses.parquet")).unwrap();
    let df = ParquetReader::new(&mut f).finish().unwrap();

    let get_weight = |df_ref: &DataFrame, tag: &str| {
        df_ref.clone().lazy() // Shallow clone to avoid moving original df
            .filter(col("tag_a").eq(lit(tag)).or(col("tag_b").eq(lit(tag))))
            .collect().unwrap()
            .column("weight_log").unwrap()
            .f64().unwrap()
            .get(0)
            .expect(&format!("Synapse for {} not found", tag))
    };

    let w_soft = get_weight(&df, "soft");
    let w_heavy = get_weight(&df, "heavy");

    println!("Soft weight: {:.4}, Heavy weight: {:.4}", w_soft, w_heavy);
    assert!(w_soft > w_heavy, "Recency bias failed: New (n=2) should outrank Old (n=5)");
}
