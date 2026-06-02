# Contract: Per-host cache layout

## Cache root resolution

```text
$XDG_CACHE_HOME/mikebom/fingerprints/      (Linux when XDG_CACHE_HOME is set)
~/.cache/mikebom/fingerprints/              (Linux + default macOS)
~/Library/Caches/mikebom/fingerprints/      (macOS when dirs::cache_dir() picks it)
%LOCALAPPDATA%\mikebom\fingerprints\        (Windows)
```

Resolution uses the workspace `dirs` crate (already a transitive dep via the existing milestone 090 fixture cache). The `MIKEBOM_FINGERPRINTS_CACHE_DIR` env var overrides at runtime — useful for CI sandboxes + Docker `COPY` scenarios.

## Per-SHA directory layout

```text
<cache-root>/
├── <full-40-hex-sha-A>/                   # One directory per cached corpus SHA
│   └── corpus/
│       ├── index.json                     # Required; absence = corrupt cache
│       ├── openssl.json
│       ├── zlib.json
│       └── ...
├── <full-40-hex-sha-B>/
│   └── corpus/...
└── .tmp-<uuid>/                           # In-flight extraction; rename atomic on success
```

**Why full 40-hex** (not the 12-hex SBOM-annotation truncation): collision resistance + avoids forcing the loader to perform a prefix-match lookup. Cache-key === full SHA; SBOM annotation === truncated. The two are intentionally different views of the same value.

## Atomic write protocol

The fetcher (FR-008 `fingerprints fetch`, FR-004 cache-miss auto-fetch) MUST follow this protocol:

1. Generate a fresh `<uuid>` (the workspace `uuid = "1"` crate is already in the dep closure; if not, use a timestamp + random bytes hand-rolled with `std::time` + `rand_core`).
2. `mkdir -p <cache-root>/.tmp-<uuid>/corpus/` — staging directory.
3. Stream the GitHub archive tarball into a `tempfile::tempfile()` (memory-backed if available, otherwise OS temp). Decompress via `flate2::read::GzDecoder`. Iterate `tar::Archive::entries()`.
4. For each entry whose path matches `<repo>-<short-sha-from-github>/corpus/*.json`, strip the top-level directory prefix and write to `<cache-root>/.tmp-<uuid>/corpus/<filename>`.
5. After the entire tarball is extracted: `std::fs::rename(<cache-root>/.tmp-<uuid>, <cache-root>/<full-sha>)`. POSIX rename of an empty target is atomic.
6. If ANY step 3–5 fails: `std::fs::remove_dir_all(<cache-root>/.tmp-<uuid>)` and propagate the error. The destination directory is never touched in the failure case.

## Reader validation

`load_corpus_from_cache(sha) -> Result<FingerprintCorpus, ...>` performs:

1. `<cache-root>/<sha>/corpus/index.json` exists + is a regular file. If absent: return `CacheNotFound`.
2. Parse `index.json` via `serde_json`. If parse fails: return `CacheCorrupt`.
3. Verify `index.json::version == 1`. If not: return `CacheCorrupt`.
4. For each `entries[].path`: open `<cache-root>/<sha>/corpus/<path>`, parse as a `FingerprintRecord`. Records that fail to parse are skipped with `tracing::warn!`; other entries continue (FR-010 defensive load).
5. Return the populated `FingerprintCorpus` with `source: CorpusSource::Cached { sha }`.

A `CacheNotFound` triggers the cache-miss fetch path (FR-004) when network is available + not `--offline`. `CacheCorrupt` triggers the same path PLUS a `tracing::warn!` recommending the operator inspect or `mikebom fingerprints cache-clear`. Both paths fall through to bundled defaults if network is unavailable.

## Cache cleanup

`mikebom fingerprints cache-clear [--keep-rev <sha>]` (FR-009):

- No flags: `std::fs::remove_dir_all(<cache-root>)`. Removes every cached SHA + any leftover `.tmp-<uuid>/` staging dirs.
- `--keep-rev <sha>`: iterate `<cache-root>/*` and remove every directory EXCEPT `<sha>` (after validating `<sha>` is well-formed 40-hex; mikebom exits non-zero on a malformed value).
- Output: print the absolute path of each removed directory on stdout. Exit zero on success.

## Concurrency model

Multiple `mikebom sbom scan` invocations against the same `<sha>` are safe:

- Two readers don't conflict (read-only filesystem access).
- A reader + a writer (the writer is doing the cache-miss fetch): the writer's `.tmp-<uuid>` staging is invisible to the reader; the final `rename` is atomic. The reader either sees the OLD state (cache empty → falls back to bundled defaults) or the NEW state (cache populated → reads it). No partial-cache-visible state.
- Two writers (concurrent cache-miss fetches for the same `<sha>`): both write to distinct `<uuid>` staging dirs; both succeed via `rename`. The second `rename` overwrites the first via POSIX `renameat()` semantics (overwriting a dir with another dir requires the target be empty — if the first writer already populated it, the second writer's rename will fail with `ENOTEMPTY`). On rename failure, the second writer cleans up its staging dir + logs a debug message ("another writer beat us to it") + proceeds to load from the now-populated cache. Either way, the on-disk state ends up correct.

This is the same concurrency model as milestone 090's fixture cache; the precedent is established.
