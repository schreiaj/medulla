use std::fs;
use std::io::Write;
use std::path::Path;
use anyhow::Result;

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
    ];

    let mut gitignore_file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&gitignore_path)?;

    let current_gitignore = fs::read_to_string(&gitignore_path).unwrap_or_default();

    for entry in ignore_entries {
        if !current_gitignore.contains(entry.trim()) {
            writeln!(gitignore_file, "{}", entry)?;
        }
    }

    // 3. Initialize AGENTS.md
    let agents_md_path = root.join("AGENTS.md");
    let protocol = r#"
## Medulla Memory Protocol
This repository uses Medulla (`med`) for cognitive memory.
- **Querying**: Use `med query "topic"` to retrieve context before starting tasks.
- **Learning**: Use `med learn "observation"` to record new findings.
- **Syncing**: Run `med consolidate` before pushing your branch.
"#;

    let mut agents_file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&agents_md_path)?;

    let current_agents_md = fs::read_to_string(&agents_md_path).unwrap_or_default();
    if !current_agents_md.contains("Medulla Memory Protocol") {
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
