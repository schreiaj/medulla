use anyhow::{Result, bail};
use chrono::Utc;
use rust_stemmers::{Algorithm, Stemmer};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

use crate::core::{MemoryEntry, lock_musings};

static EN_STEMMER: OnceLock<Stemmer> = OnceLock::new();

const MAX_CONTENT_LENGTH: usize = 10_000;
const MAX_TAGS: usize = 50;

pub fn run_in(
    root: &Path,
    content: &str,
    tags: Vec<String>,
    custom_id: Option<String>,
    source: Option<String>,
) -> Result<()> {
    if content.trim().is_empty() {
        bail!("Content cannot be empty");
    }
    if content.len() > MAX_CONTENT_LENGTH {
        bail!(
            "Content exceeds maximum length of {} characters",
            MAX_CONTENT_LENGTH
        );
    }
    if tags.len() > MAX_TAGS {
        bail!(
            "Too many tags: {} provided, maximum is {}",
            tags.len(),
            MAX_TAGS
        );
    }

    let musings_path = root.join(".medulla/musings.ndjson");
    let now_ms = Utc::now().timestamp_millis();

    let id = custom_id.unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    });

    let en_stemmer = EN_STEMMER.get_or_init(|| Stemmer::create(Algorithm::English));

    // Split, trim, and lowercase the tags — originals for display.
    let original_tags: Vec<String> = tags
        .iter()
        .flat_map(|t| t.split(','))
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    // Stemmed forms for Hebbian graph indexing.
    let stemmed_tags: Vec<String> = original_tags
        .iter()
        .map(|t| en_stemmer.stem(t).to_string())
        .collect();

    let entry = MemoryEntry {
        id,
        content: content.to_string(),
        timestamp: now_ms,
        tags: original_tags,
        associations: stemmed_tags,
        source,
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

pub fn run(content: &str, tags: Vec<String>, id: Option<String>, source: Option<String>) -> Result<()> {
    run_in(Path::new("."), content, tags, id, source)?;
    println!("✔ Memory encoded.");
    Ok(())
}
