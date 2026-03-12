use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Internal logic that accepts a root path (essential for parallel testing)
pub fn run_in(root: &Path) -> Result<()> {
    let medulla_path = root.join(".medulla");

    // 1. Create the .medulla directory
    if !medulla_path.exists() {
        fs::create_dir_all(&medulla_path)?;
    }

    // 2. Configure .gitignore
    let gitignore_path = root.join(".gitignore");
    let ignore_entries = [
        "\n# Medulla: Local working memory and caches",
        ".medulla/musings.ndjson",
        ".medulla/brain.parquet",
        ".medulla/embeddings.parquet",
        ".medulla/.cache",
    ];

    let current_gitignore = match fs::read_to_string(&gitignore_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };

    let mut gitignore_file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&gitignore_path)?;

    for entry in ignore_entries {
        if !current_gitignore.contains(entry.trim()) {
            writeln!(gitignore_file, "{}", entry)?;
        }
    }

    // 3. Initialize AGENTS.md
    let agents_md_path = root.join("AGENTS.md");
    let protocol = include_str!("../templates/AGENTS.md");

    let current_agents_md = match fs::read_to_string(&agents_md_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };

    let mut agents_file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&agents_md_path)?;

    if !current_agents_md.contains("Cognitive Memory Drive (Medulla)") {
        writeln!(agents_file, "{}", protocol)?;
    }

    Ok(())
}

/// Standard entry point for the CLI
pub fn run() -> Result<()> {
    run_in(Path::new("."))?;
    println!("✔ Medulla initialized successfully.");
    Ok(())
}
