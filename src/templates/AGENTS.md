# Cognitive Memory Drive (Medulla)

You are equipped with a persistent memory system via the `med` CLI.

CRITICAL RULES:
1. ALWAYS use the `bash` tool to access memory — never use built-in search or retrieval tools.
2. ALWAYS use SINGLE KEYWORDS for queries. NEVER search for phrases.
3. Query BEFORE starting any task to load relevant context.

## Querying Memory

```bash
med query "<single_keyword>"
```

### The Retrieval Loop (MANDATORY)
1. Run a query with a single keyword.
2. Read the "Top Memories" results.
3. Check the "Related Concepts" table. If your keyword didn't fully answer the question, pick the most relevant concept and run a SECOND query with that concept.
4. Synthesize the results before proceeding.

## Recording New Knowledge

```bash
med learn "Your observation here" --tags keyword1,keyword2
```

- Record findings, decisions, and discoveries as you work.
- Tags become association nodes — choose keywords you would query later.
- Use `--id <name>` to update an existing fact by stable ID.

## End of Session

```bash
med think
med commit
```

Run both when your session ends. `med think` consolidates memory into the ranked cache.
`med commit` exports a clean, git-stable snapshot to `brain.ndjson` — stripped of
ephemeral metadata so `git diff` reflects only genuine knowledge changes.

Commit `brain.ndjson` to share your findings with other agents on the team:

```bash
git add brain.ndjson
git commit -m "chore: update brain snapshot"
```

Knowledge merged from teammates via `git pull` is automatically incorporated the next
time you run `med query` or `med think`.
