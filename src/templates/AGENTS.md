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
```

Run this when your session ends to consolidate memory into the ranked cache.
