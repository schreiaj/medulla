use crate::core::{MemoryEntry, lock_musings};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::{fs, fs::File};

pub fn run() -> Result<()> {
    run_in(Path::new("."))
}

pub fn run_in(root: &Path) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let output_path = root.join("brain.ndjson");
    let tmp_path = root.join("brain.ndjson.tmp");

    // 1. Acquire shared (read) lock
    let _lock = lock_musings(&musings_path, false)?;

    // 2. Read and deduplicate: keep the entry with the highest timestamp
    let mut map: HashMap<String, MemoryEntry> = HashMap::new();
    if musings_path.exists() {
        let file = File::open(&musings_path)?;
        for line in BufReader::new(file).lines() {
            let line = line?;
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

    // 3. Collect and sort alphabetically by id
    let mut entries: Vec<&MemoryEntry> = map.values().collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));

    // 4. Stream directly to temp file, then atomic rename
    let count = entries.len();
    {
        let mut tmp = File::create(&tmp_path)?;
        for e in &entries {
            writeln!(
                tmp,
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "id": e.id,
                    "content": e.content,
                    "tags": if e.tags.is_empty() { &e.associations } else { &e.tags }
                }))?
            )?;
        }
        tmp.flush()?;
    }
    fs::rename(&tmp_path, &output_path)?;

    println!("Exported {} memories to brain.ndjson", count);
    Ok(())
}
