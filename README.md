# medulla

A cognitive memory layer for local LLM agents. `med` gives agents persistent, ranked memory using ACT-R activation decay and Hebbian association — without a database or network dependency.

## How it works

Facts are stored as NDJSON and compiled into Parquet caches. When you query:

1. Recent memories score higher (ACT-R recency decay)
2. Frequently accessed memories are boosted (access count reinforcement)
3. Co-occurring tags build a semantic graph (Hebbian wiring)
4. Related concepts are surfaced alongside results

## Install

**Nix:**
```sh
nix shell github:schreiaj/medulla#med
# or permanently:
nix profile install github:schreiaj/medulla#med
```

**Cargo:**
```sh
cargo install --git https://github.com/schreiaj/medulla
```

## Usage

```sh
# Set up a project
med init

# Record a fact
med learn "The robot arm uses silicone grippers for better grip" --tags robotics,materials

# Record with a stable ID (for updating a fact later)
med learn "Silicone degrades under UV exposure" --tags materials,durability --id silicone-uv

# Consolidate into ranked cache, this will also happen automatically if new facts have been learned since last cache
med think

# Query (auto-consolidates if cache is stale)
med query "silicone"
med query "robotics" --limit 10
```

## Agent integration

`med init` writes an `AGENTS.md` into `.medulla/` with the memory protocol. Add it to your agent's context or system prompt.

Key protocol rules for agents:
- Query with **single keywords** for best results
- Use the **Related Concepts** table to chain a second query
- Record new findings with `med learn` during a session
- Run `med think` to consolidate before ending a session

See [`src/templates/AGENTS.md`](src/templates/AGENTS.md) for the full agent protocol.

## Data files

All data lives in `.medulla/` in your project directory. `med init` adds the runtime files to `.gitignore` automatically — only `AGENTS.md` is tracked.

| File | Purpose | Tracked? |
|------|---------|---------|
| `.medulla/AGENTS.md` | Agent protocol instructions | Yes |
| `.medulla/musings.ndjson` | Raw fact log (source of truth) | No |
| `.medulla/brain.parquet` | Ranked fact cache | No |
| `.medulla/synapses.parquet` | Tag association graph | No |

## License

MIT
