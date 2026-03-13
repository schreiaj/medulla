# medulla

A cognitive memory layer for local LLM agents. `med` gives agents persistent, ranked memory using ACT-R activation decay and Hebbian association — without a database or network dependency.

## How it works

Facts are stored as NDJSON and compiled into Parquet caches. When you query:

1. Recent memories score higher (ACT-R recency decay)
2. Frequently accessed memories are boosted (access count reinforcement)
3. Co-occurring tags build a semantic graph (Hebbian wiring)
4. Semantically similar memories are found via sentence embeddings (e.g. "octopus disguise" finds "cephalopod camouflage")
5. Related concepts are surfaced alongside results

## Install

**Pre-built binaries (recommended):**

Download the latest release for your platform from [GitHub Releases](https://github.com/schreiaj/medulla/releases/latest):

| Platform | Archive |
|----------|---------|
| macOS (Apple Silicon) | `med-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `med-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `med-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `med-aarch64-unknown-linux-gnu.tar.gz` |

```sh
tar -xzf med-<platform>.tar.gz
mv med /usr/local/bin/
```

Pre-built binaries include the ONNX Runtime statically linked — no extra dependencies required.

**Nix:**
```sh
nix shell github:schreiaj/medulla#med
# or permanently:
nix profile install github:schreiaj/medulla#med
```

The Nix devShell sets up ORT automatically. For semantic search outside a Nix shell,
set `ORT_DYLIB_PATH` to your `libonnxruntime` install (e.g. `brew install onnxruntime`
on macOS). Keyword search works without it.

**Cargo:**
```sh
cargo install --git https://github.com/schreiaj/medulla
```

> Cargo builds use `ort-load-dynamic` by default and require `libonnxruntime` at runtime.
> To build a self-contained binary (like the pre-built releases):
> ```sh
> cargo install --git https://github.com/schreiaj/medulla \
>   --no-default-features --features ort-download-binaries
> ```

## Usage

```sh
# Set up a project
med init

# Record a fact
med learn "The robot arm uses silicone grippers for better grip" --tags robotics,materials

# Record with a stable ID (for updating a fact later)
med learn "Silicone degrades under UV exposure" --tags materials,durability --id silicone-uv

# Consolidate into ranked cache (also happens automatically when cache is stale)
med think

# Query (auto-consolidates if cache is stale)
med query "silicone"
med query "robotics" --limit 10

# Export a clean, git-trackable snapshot of the brain
med commit
git add brain.ndjson && git commit -m "chore: update brain snapshot"
```

## GitOps workflow

`med commit` exports a deterministic `brain.ndjson` to your project root. It strips
ephemeral metadata (access counts, timestamps) and sorts entries by ID, so `git diff`
shows only genuine knowledge changes — not cognitive noise.

When a teammate merges their own `brain.ndjson`, `med query` detects the drift and
automatically recompiles, overlaying their facts onto your local ACT-R state. You get
their knowledge without losing your access history.

```
agent A learns → med commit → git push
agent B git pull → med query (auto-recompile) → unified brain
```

## Agent integration

`med init` writes an `AGENTS.md` into `.medulla/` with the memory protocol. Add it to your agent's context or system prompt.

Key protocol rules for agents:
- Query with natural language phrases or single keywords — semantic search handles both
- Use the **Related Concepts** table to chain a second query for deeper recall
- Record new findings with `med learn` during a session
- Run `med think` then `med commit` at the end of a session to persist and share knowledge

See [`src/templates/AGENTS.md`](src/templates/AGENTS.md) for the full agent protocol.

## Data files

`med init` adds runtime files to `.gitignore` automatically. `brain.ndjson` is the only
data file intended for git — it is the stable, shareable export of the brain.

| File | Purpose | Tracked? |
|------|---------|---------|
| `.medulla/AGENTS.md` | Agent protocol instructions | Yes |
| `brain.ndjson` | Shareable brain snapshot (git-stable) | Yes |
| `.medulla/musings.ndjson` | Raw fact log with ACT-R metadata | No |
| `.medulla/brain.parquet` | Ranked fact cache | No |
| `.medulla/synapses.parquet` | Tag association graph | No |
| `.medulla/embeddings.parquet` | Sentence embedding cache | No |
| `.medulla/.cache` | Downloaded embedding model | No |

## License

MIT
