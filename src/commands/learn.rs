use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use sha2::{Sha256, Digest};
use anyhow::{Result, bail};
use rust_stemmers::{Algorithm, Stemmer};

use crate::core::{MemoryEntry, lock_musings};

static EN_STEMMER: OnceLock<Stemmer> = OnceLock::new();

const MAX_CONTENT_LENGTH: usize = 10_000;
const MAX_TAGS: usize = 50;

pub fn run_in(root: &Path, content: &str, tags: Vec<String>, custom_id: Option<String>) -> Result<()> {
    if content.trim().is_empty() {
        bail!("Content cannot be empty");
    }
    if content.len() > MAX_CONTENT_LENGTH {
        bail!("Content exceeds maximum length of {} characters", MAX_CONTENT_LENGTH);
    }
    if tags.len() > MAX_TAGS {
        bail!("Too many tags: {} provided, maximum is {}", tags.len(), MAX_TAGS);
    }

    let musings_path = root.join(".medulla/musings.ndjson");
    let now_ms = Utc::now().timestamp_millis();

    let id = custom_id.unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    });

    let en_stemmer = EN_STEMMER.get_or_init(|| Stemmer::create(Algorithm::English));

    // Split, trim, lowercase, and STEM the tags
    let parsed_tags: Vec<String> = tags
        .iter()
        .flat_map(|t| t.split(','))
        .map(|t| {
            let clean = t.trim().to_lowercase();
            en_stemmer.stem(&clean).to_string()
        })
        .filter(|t| !t.is_empty())
        .collect();

    let entry = MemoryEntry {
        id,
        content: content.to_string(),
        timestamp: now_ms,
        associations: parsed_tags,
        access_count: 0,
        last_access: now_ms,
    };

    // Hold exclusive lock for the duration of the append
    let _lock = lock_musings(&musings_path, true)?;

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
