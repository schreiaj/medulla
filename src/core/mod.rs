use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub timestamp: i64,
    pub associations: Vec<String>,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub last_access: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoActivation {
    pub tag_a: String,
    pub tag_b: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Synapse {
    pub tag_a: String,
    pub tag_b: String,
    pub weight_log: f64,
    pub last_seen: i64,
}

/// Acquires an advisory lock on the musings file via a sidecar `.lock` file.
/// The returned `File` holds the lock until it is dropped — callers must
/// keep it alive for the duration of the critical section.
/// Pass `exclusive = true` for write operations, `false` for reads.
pub fn lock_musings(musings_path: &Path, exclusive: bool) -> Result<fs::File> {
    let lock_path = musings_path.with_extension("lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;
    if exclusive {
        lock_file
            .lock_exclusive()
            .context("Failed to acquire exclusive lock on musings")?;
    } else {
        lock_file
            .lock_shared()
            .context("Failed to acquire shared lock on musings")?;
    }
    Ok(lock_file)
}
