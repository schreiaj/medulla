use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use sha2::{Sha256, Digest};
use anyhow::Result;

use crate::core::MemoryEntry;

pub fn run_in(root: &Path, content: &str, tags: Vec<String>, custom_id: Option<String>) -> Result<()> {
    let musings_path = root.join(".medulla/musings.ndjson");
    let now_ms = Utc::now().timestamp_millis();

    // 1. ID Generation
    let id = custom_id.unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())[..12].to_string()
    });

    // 2. Build the Entry
    // Note: 'associations' here is the raw material for the Reconstructor
    let entry = MemoryEntry {
        id,
        content: content.to_string(),
        timestamp: now_ms,
        confidence: 0.5,
        associations: tags.iter().map(|t| t.to_lowercase()).collect(),
        access_count: 1,
        last_access: now_ms,
    };

    // 3. Atomic Append to NDJSON (The only file Git cares about)
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&musings_path)?;

    let json_line = serde_json::to_string(&entry)?;
    writeln!(file, "{}", json_line)?;

    Ok(())
}

pub fn run(content: &str, tags: Vec<String>, id: Option<String>) -> Result<()> {
    run_in(Path::new("."), content, tags, id)?;
    println!("✔ Memory encoded.");
    Ok(())
}
