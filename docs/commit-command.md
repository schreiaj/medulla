### Plan A: The `med commit` Command

**The Goal:** Isolate the LLM's cognitive metadata (which changes constantly) from the actual facts (which only change when the legislation changes), producing a perfectly stable, stateless file optimized for `git diff`.

#### 1. The Mechanism

1. **Read Local State:** Open `.medulla/musings.ndjson` and deserialize the lines into standard Rust `MemoryEntry` structs.
2. **Deduplicate:** Read them into a `HashMap<String, MemoryEntry>` keyed by the `id` (your `chunk_id`) to ensure only the latest version of any given fact survives.
3. **Strip Metadata & Sort:** Pour the map's values into a `Vec`, map them into a new, simplified dynamic JSON object that *only* contains `id`, `content`, and `tags` (explicitly dropping `timestamp`, `last_access`, and `access_count`), and sort the vector alphabetically by `id`.
4. **Export:** Write the sorted objects to `./brain.ndjson` in the project root.

#### 2. Architectural Decision: Why Serde & Native Collections over Polars?

While Polars is the analytical backbone for `med think` and `med query`, it is explicitly avoided for the `med commit` command. This pipeline relies entirely on `serde_json` and native Rust collections (`HashMap` / `Vec`) for three critical reasons:

* **The Schema Inference Trap (Strict vs. Guessed Typing):**
When Polars reads an NDJSON file using `LazyJsonLineReader`, it has to guess the schema by scanning the first chunk of rows. If the first 100 legislative chunks processed didn't trigger any tags, the NDJSON will have `tags: []`. Polars will infer the column type as `Null`. When it hits row 101 and sees `tags: ["budget"]`, the reader will panic and crash because a `Null` column cannot suddenly hold a `List(Utf8)`. Using `serde_json::from_str::<MemoryEntry>(&line)` relies on Rust's compiler. It knows unconditionally that `tags` is a `Vec<String>`, regardless of whether the arrays are empty, preventing fatal schema drift errors.
* **Byte-Perfect Determinism for Git:**
The entire purpose of `commit` is to create a file that behaves perfectly under `git diff`. The keys inside the JSON objects must be in the exact same order every single time. Polars' `JsonWriter` is designed for speed, giving you very little granular control over the exact byte-formatting. With `serde_json`, we construct the exact stateless object manually (`serde_json::json!({"id": ..., "content": ..., "tags": ...})`). This guarantees that if the underlying facts haven't changed, the resulting `brain.ndjson` will have the exact same file hash as the previous run.
* **The "Arrow Translation" Overhead:**
Polars is built on Apache Arrow, a **columnar** memory format. NDJSON is a **row-based** text format. Using Polars to deduplicate and sort requires the CPU to parse row-based text, pivot the data into contiguous columnar Arrow arrays (allocating large heap chunks), perform the sort, pivot the columns back into rows, and serialize them. For a Last-Write-Wins deduplication and ID sort, reading the lines directly into a native Rust `HashMap` and then a `Vec` is a straight line. It avoids the column-to-row pivot entirely, making the execution instantaneous and highly memory efficient.

#### 3. The Edge Cases Handled

* **Immutability for Git:** Because it drops the ACT-R access counts and sorts by ID, running `med commit` ten times in a row without learning anything new will result in the exact same file. Git will report a clean working directory.
* **Orphaned Memories:** If an agent updates its understanding of a chunk and overwrites an older memory locally, the deduplication ensures only the final, cleanest version makes it into the Git export.
