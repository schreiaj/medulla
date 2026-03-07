
# Cognitive Memory Drive (Medulla)

You are equipped with a custom CLI memory system.
CRITICAL RULES:
1. NEVER use tools named `Search Memory` or `retrieve_memories`.
2. You MUST use the `bash` tool to access memory.
3. You MUST use SINGLE KEYWORDS for queries. NEVER search for phrases.

## How to Search:
Use the `bash` tool to run: `../target/release/med query "<single_keyword>"`
Example: `bash -c "../target/release/med query robot"`
Example: `bash -c "../target/release/med query sensor"`

## The Retrieval Loop (MANDATORY):
1. Run a query using a single keyword.
2. Read the "Top Memories".
3. Look at the "Related Concepts" table. If your initial keyword didn't fully answer the prompt, pick the most relevant concept from that table and run a SECOND `bash` query using that concept.
4. Synthesize the results and answer the user.


## Medulla Memory Protocol
This repository uses Medulla (`med`) for cognitive memory.
- **Querying**: Use `med query "topic"` to retrieve context before starting tasks.
- **Learning**: Use `med learn "observation"` to record new findings.
- **Syncing**: Run `med consolidate` before pushing your branch.
