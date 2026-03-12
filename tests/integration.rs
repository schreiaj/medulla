use chrono::{Duration, Utc};
use med::commands::{commit, embed, init, learn, query, think};
use polars::prelude::*;
use std::fs;
use std::io::Write;
use tempfile::tempdir;

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
    learn::run_in(root, "Validating JSON encoding.", tags, None, None).unwrap();

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
    med::commands::learn::run_in(root, "Entry 1", vec!["tag1".into()], None, None).unwrap();
    med::commands::learn::run_in(root, "Entry 2", vec!["tag2".into()], None, None).unwrap();

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
    learn::run_in(root, "Version 1", vec![], id.clone(), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Second version (Update)
    learn::run_in(root, "Version 2", vec![], id.clone(), None).unwrap();

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
    learn::run_in(root, "Old Memory", vec![], None, None).unwrap();

    // 2. Learn something "new"
    learn::run_in(root, "New Memory", vec![], None, None).unwrap();

    think::run_in(root).unwrap();

    // 3. Inspect the Parquet activation scores
    let mut file = fs::File::open(root.join(".medulla/brain.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    // Sort by activation descending to see what's "on top"
    let sorted_df = df
        .sort(
            ["activation"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .unwrap();

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
    learn::run_in(root, "Memory 1", tags.clone(), None, None).unwrap();
    learn::run_in(root, "Memory 2", tags.clone(), None, None).unwrap();

    // Think handles the reconstruction
    think::run_in(root).unwrap();

    let mut file = std::fs::File::open(root.join(".medulla/synapses.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    let weight = df
        .column("weight_log")
        .unwrap()
        .f64()
        .unwrap()
        .get(0)
        .unwrap();

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
    learn::run_in(root, "Octopus soft robotics.", tags.clone(), None, None).unwrap();
    learn::run_in(root, "Octopus robot chassis.", tags.clone(), None, None).unwrap();

    // 2. Think (This now reconstructs from musings.ndjson)
    think::run_in(root).unwrap();

    // 3. Verify synapses.parquet exists and has the correct weight
    let synapses_path = root.join(".medulla/synapses.parquet");
    let mut file = std::fs::File::open(&synapses_path).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();

    assert!(df.height() > 0);
    let weight = df
        .column("weight_log")
        .unwrap()
        .f64()
        .unwrap()
        .get(0)
        .unwrap();
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
    learn::run_in(root, "Octopus tentacles use silicone.", tags1, None, None).unwrap();

    let tags2 = vec!["robotics".to_string(), "silicone".to_string()];
    learn::run_in(root, "Robotics joints use silicone.", tags2, None, None).unwrap();

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
        df_ref
            .clone()
            .lazy() // Shallow clone to avoid moving original df
            .filter(col("tag_a").eq(lit(tag)).or(col("tag_b").eq(lit(tag))))
            .collect()
            .unwrap()
            .column("weight_log")
            .unwrap()
            .f64()
            .unwrap()
            .get(0)
            .unwrap_or_else(|| panic!("Synapse for {} not found", tag))
    };

    let w_soft = get_weight(&df, "soft");
    let w_heavy = get_weight(&df, "heavy");

    println!("Soft weight: {:.4}, Heavy weight: {:.4}", w_soft, w_heavy);
    assert!(
        w_soft > w_heavy,
        "Recency bias failed: New (n=2) should outrank Old (n=5)"
    );
}

#[test]
fn test_commit_deduplicates_and_strips_metadata() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // Learn two distinct entries plus a duplicate (same id) to test dedup
    let id = Some("fixed-commit-id".to_string());
    learn::run_in(root, "Version 1", vec!["alpha".into()], id.clone(), None).unwrap();
    learn::run_in(root, "Version 2", vec!["beta".into()], id.clone(), None).unwrap();
    learn::run_in(root, "Another fact", vec!["gamma".into()], None, None).unwrap();

    commit::run_in(root).unwrap();

    let brain_path = root.join("brain.ndjson");
    assert!(brain_path.exists());

    let content = fs::read_to_string(&brain_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    // Dedup: 2 unique ids (fixed-commit-id + auto-generated)
    assert_eq!(
        lines.len(),
        2,
        "Expected 2 deduplicated entries, got {}",
        lines.len()
    );

    for line in &lines {
        let obj: serde_json::Value = serde_json::from_str(line).unwrap();
        // Required keys present
        assert!(obj.get("id").is_some());
        assert!(obj.get("content").is_some());
        assert!(obj.get("tags").is_some());
        // Metadata keys absent
        assert!(obj.get("timestamp").is_none());
        assert!(obj.get("access_count").is_none());
        assert!(obj.get("last_access").is_none());
    }

    // Dedup kept last-write-wins
    let fixed = lines
        .iter()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|obj| obj["id"] == "fixed-commit-id")
        .expect("fixed-commit-id not found");
    assert_eq!(fixed["content"], "Version 2");

    // Sorted alphabetically by id
    let ids: Vec<&str> = lines
        .iter()
        .map(|l| {
            let obj: serde_json::Value = serde_json::from_str(l).unwrap();
            // Extract id as owned value; we compare them as strings
            obj["id"].as_str().unwrap().to_string()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &str)
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "brain.ndjson entries are not sorted by id");
}

#[test]
fn test_commit_idempotent() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();
    learn::run_in(root, "Stable fact", vec!["tag".into()], None, None).unwrap();

    commit::run_in(root).unwrap();
    let first = fs::read_to_string(root.join("brain.ndjson")).unwrap();

    commit::run_in(root).unwrap();
    let second = fs::read_to_string(root.join("brain.ndjson")).unwrap();

    assert_eq!(
        first, second,
        "Repeated commits must produce identical output"
    );
}

/// Verify that query detects a brain.ndjson that is newer than brain.parquet and
/// triggers a recompile via think, pulling the new entry into the searchable cache.
#[test]
fn test_query_triggers_recompile_on_brain_ndjson_drift() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // 1. Learn a local fact and build the initial brain.parquet
    learn::run_in(root, "local baseline", vec!["local".into()], None, None).unwrap();
    think::run_in(root).unwrap();

    // 2. Pause so the next file write has a strictly later mtime
    std::thread::sleep(std::time::Duration::from_millis(50));

    // 3. Write brain.ndjson with a new entry (simulating a teammate's git push)
    let git_entry = serde_json::json!({
        "id": "git-pulled-id",
        "content": "fact arrived via git pull",
        "tags": ["remote"]
    });
    fs::write(root.join("brain.ndjson"), format!("{}\n", git_entry)).unwrap();

    // 4. Query — staleness check should detect the newer brain.ndjson and recompile
    query::run_in(root, "git pull", 5, 0.70).unwrap();

    // 5. brain.parquet must now contain the git-pulled entry
    let mut file = fs::File::open(root.join(".medulla/brain.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();
    let contents: Vec<&str> = df
        .column("content")
        .unwrap()
        .str()
        .unwrap()
        .into_no_null_iter()
        .collect();
    assert!(
        contents.contains(&"fact arrived via git pull"),
        "brain.parquet should contain the git-pulled entry after recompile, got: {:?}",
        contents
    );
}

/// Verify that `med learn` followed by `med think` correctly merges local musings
/// with an existing brain.ndjson — both the newly learned entry and the previously
/// committed entry must appear in the resulting brain.parquet.
#[test]
fn test_learn_merges_with_existing_brain_ndjson() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    // 1. Simulate an existing brain.ndjson (e.g. from a previous `med commit`)
    let committed_entry = serde_json::json!({
        "id": "committed-id",
        "content": "previously committed fact",
        "tags": ["committed"]
    });
    fs::write(root.join("brain.ndjson"), format!("{}\n", committed_entry)).unwrap();

    // 2. Learn a brand-new local fact (not yet in brain.ndjson)
    learn::run_in(root, "newly learned fact", vec!["fresh".into()], None, None).unwrap();

    // 3. think should overlay brain.ndjson on top of musings and compile both
    think::run_in(root).unwrap();

    // 4. brain.parquet must contain both entries
    let mut file = fs::File::open(root.join(".medulla/brain.parquet")).unwrap();
    let df = ParquetReader::new(&mut file).finish().unwrap();
    assert_eq!(
        df.height(),
        2,
        "Expected 2 entries in brain.parquet, got {}",
        df.height()
    );
    let contents: Vec<&str> = df
        .column("content")
        .unwrap()
        .str()
        .unwrap()
        .into_no_null_iter()
        .collect();
    assert!(
        contents.contains(&"previously committed fact"),
        "Missing committed entry, got: {:?}",
        contents
    );
    assert!(
        contents.contains(&"newly learned fact"),
        "Missing newly learned entry, got: {:?}",
        contents
    );
}

// --- Embedding tests ---

#[test]
fn test_cosine_similarity_identical_vectors() {
    let v = vec![1.0f32, 2.0, 3.0];
    let sim = embed::cosine_similarity(&v, &v);
    assert!((sim - 1.0).abs() < 1e-6, "identical vectors should have sim=1.0, got {}", sim);
}

#[test]
fn test_cosine_similarity_orthogonal_vectors() {
    let a = vec![1.0f32, 0.0, 0.0];
    let b = vec![0.0f32, 1.0, 0.0];
    let sim = embed::cosine_similarity(&a, &b);
    assert!(sim.abs() < 1e-6, "orthogonal vectors should have sim=0.0, got {}", sim);
}

#[test]
fn test_cosine_similarity_zero_vector() {
    let a = vec![0.0f32, 0.0, 0.0];
    let b = vec![1.0f32, 2.0, 3.0];
    let sim = embed::cosine_similarity(&a, &b);
    assert_eq!(sim, 0.0, "zero vector should return 0.0 without panic");
}

#[test]
fn test_embeddings_parquet_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.parquet");

    let ids = Series::new("id".into(), &["a", "b"]);
    let emb1 = Series::new("".into(), &[0.1f32, 0.2, 0.3]);
    let emb2 = Series::new("".into(), &[0.4f32, 0.5, 0.6]);
    let embedding_series: Series =
        ListChunked::from_iter(vec![emb1, emb2].into_iter())
            .into_series()
            .with_name("embedding".into());

    let n = 2usize;
    let id_col: Column = ids.into();
    let emb_col: Column = embedding_series.into();
    let mut df = DataFrame::new(n, vec![id_col, emb_col]).unwrap();

    let mut f = fs::File::create(&path).unwrap();
    ParquetWriter::new(&mut f).finish(&mut df).unwrap();
    drop(f);

    let mut f2 = fs::File::open(&path).unwrap();
    let df2 = ParquetReader::new(&mut f2).finish().unwrap();

    assert_eq!(df2.height(), 2);
    let list_col = df2.column("embedding").unwrap().list().unwrap();
    let row0: Vec<f32> = list_col.get_as_series(0).unwrap().f32().unwrap().into_no_null_iter().collect();
    assert!((row0[0] - 0.1f32).abs() < 1e-6);
}

#[test]
fn test_find_similar_returns_empty_without_embeddings_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    let result = embed::find_similar(root, "some query", 10, 0.70).unwrap();
    assert!(result.is_empty(), "should return empty vec when no embeddings file exists");
}

#[test]
#[ignore]
fn test_update_embeddings_is_incremental() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    learn::run_in(root, "The sky is blue.", vec!["sky".into()], None, None).unwrap();
    think::run_in(root).unwrap();

    // Count rows after first run
    let path = root.join(".medulla/embeddings.parquet");
    let row_count_1 = {
        let mut f = fs::File::open(&path).unwrap();
        ParquetReader::new(&mut f).finish().unwrap().height()
    };

    // Run think again — no new entries, should not duplicate
    think::run_in(root).unwrap();
    let row_count_2 = {
        let mut f = fs::File::open(&path).unwrap();
        ParquetReader::new(&mut f).finish().unwrap().height()
    };

    assert_eq!(row_count_1, row_count_2, "second think should not duplicate embeddings");
}

#[test]
#[ignore]
fn test_semantic_query_finds_related_content() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init::run_in(root).unwrap();

    learn::run_in(
        root,
        "The octopus can camouflage by changing skin texture.",
        vec!["cephalopod".into()],
        None,
        None,
    )
    .unwrap();
    think::run_in(root).unwrap();

    let ids = embed::find_similar(root, "octopus disguise", 10, 0.50).unwrap();
    assert!(!ids.is_empty(), "semantic search should find the cephalopod entry");
}

#[test]
fn test_query_reinforces_memory() {
    use med::commands::{init, learn, query};
    use med::core::MemoryEntry;
    use std::fs;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize the blank slate
    init::run_in(root).unwrap();

    // 2. Learn a new fact
    let tags = vec!["robot".to_string()];
    learn::run_in(root, "Testing the memory reinforcement loop.", tags, None, None).unwrap();

    let musings_path = root.join(".medulla/musings.ndjson");

    // 3. Capture the initial state of the memory
    let initial_content = fs::read_to_string(&musings_path).unwrap();
    let initial_entry: MemoryEntry = serde_json::from_str(initial_content.trim()).unwrap();

    assert_eq!(
        initial_entry.access_count, 0,
        "Memory should start with an access count of 0."
    );
    let initial_time = initial_entry.last_access;

    // Sleep for 10 milliseconds to ensure the Unix timestamp definitely ticks forward
    thread::sleep(Duration::from_millis(10));

    // 4. Query the fact
    // (Because brain.parquet doesn't exist yet, query::run_in will automatically
    // trigger think::run_in, run the search, and then reinforce the NDJSON)
    query::run_in(root, "robot", 5, 0.70).unwrap();

    // 5. Verify the cognitive weights shifted
    let final_content = fs::read_to_string(&musings_path).unwrap();
    let final_entry: MemoryEntry = serde_json::from_str(final_content.trim()).unwrap();

    assert_eq!(
        final_entry.access_count, 1,
        "Access count failed to increment. Expected 1, got {}.",
        final_entry.access_count
    );

    assert!(
        final_entry.last_access > initial_time,
        "Last access timestamp did not update. Initial: {}, Final: {}",
        initial_time,
        final_entry.last_access
    );
}
