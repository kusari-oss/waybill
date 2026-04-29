---
description: "Disk cache for pulled OCI image blobs (layers + config) keyed on SHA-256 digest, with LRU eviction"
status: spec
milestone: 036
closes: "#68"
---

# Spec: OCI layer cache (031.z)

## Background

Milestones 031-035 ship a complete registry-pull pipeline: parse-ref →
fetch-manifest → resolve-platform → fetch-blobs → assemble-tarball.
Every `mikebom sbom scan --image <ref>` re-pulls every blob from the
registry, even when the user just ran the same scan 30 seconds ago.
For `alpine:3.19` (~3 MB) that's negligible; for `ubuntu:24.04`
(~28 MB) it's annoying; for an 800 MB ML/CUDA image it's a real
workflow tax.

OCI blobs are content-addressed by their SHA-256 digest, so caching
them is straightforward: blob digest matches → safe to read from
disk without re-verifying upstream. The manifest itself is NOT
cached — a tag like `:latest` floats over time, and re-fetching the
manifest is what detects the float (after which the new layer
digests would naturally cache-miss).

`mikebom-cli/src/scan_fs/oci_pull/registry.rs::fetch_blob` is the
seam. It already does `verify_sha256` on the network bytes; the same
verification on cached bytes (with corruption fallback to network)
gives us safety-by-construction.

## User story (US1, P1)

**As a developer iteratively scanning the same container image** —
e.g. while debugging an SBOM-format issue — **I want repeated
`mikebom sbom scan --image <ref>` invocations to skip the network
fetch for already-pulled blobs**, so iteration is fast.

**Why P1**: this is workflow-critical for any non-trivial image.
The 031-035 work made registry pulls a one-step UX; without caching,
the cost of using that UX iteratively is prohibitive on bigger
images.

### Independent test

After implementation:
- Run `mikebom sbom scan --image ubuntu:24.04 --output ubuntu1.cdx.json`.
- Time the second invocation: `mikebom sbom scan --image ubuntu:24.04 --output ubuntu2.cdx.json`.
- Second invocation completes in ~1-3 s (vs ~10-20 s for first), with
  `RUST_LOG=debug` showing `cache hit` lines for each blob.
- `--no-oci-cache` makes the second invocation as slow as the first.
- The two output SBOMs are byte-identical (same image bytes ⇒ same
  scan output).

## Acceptance scenarios

**Scenario 1: Cold cache → warm cache speedup**
```
Given: ~/.cache/mikebom/oci-layers/ does not exist
When:  mikebom sbom scan --image alpine:3.19 (twice in a row)
Then:  the first invocation fetches from network and writes the cache;
       the second invocation reads from cache (`cache hit` debug logs
       on every blob) and produces a byte-identical SBOM.
```

**Scenario 2: Cache-hit corruption recovery**
```
Given: a cache file at <cache>/sha256/<digest> whose bytes have been
       corrupted (truncated, bit-flipped)
When:  mikebom scans an image that references <digest>
Then:  mikebom detects the SHA-256 mismatch, drops the cache entry,
       falls through to a network fetch, and produces a correct SBOM.
       A `tracing::warn!` records the corruption.
```

**Scenario 3: `--no-oci-cache` opts out**
```
Given: a populated cache
When:  mikebom sbom scan --image alpine:3.19 --no-oci-cache
Then:  every blob is fetched from network (no cache reads, no cache
       writes). The cache files on disk are untouched.
```

**Scenario 4: Eviction on size-cap**
```
Given: the cache has accumulated 11 GB of blobs and the cap is 10 GB
When:  any new blob is written to the cache
Then:  the oldest-mtime files are deleted until the total drops below
       the cap. Files in active use by the current scan are not
       evicted.
```

**Scenario 5: `MIKEBOM_OCI_CACHE_DIR` override**
```
Given: MIKEBOM_OCI_CACHE_DIR=/tmp/mikebom-test-cache is exported
When:  mikebom sbom scan --image alpine:3.19
Then:  cache files land at /tmp/mikebom-test-cache/sha256/<digest>,
       not under $XDG_CACHE_HOME or $HOME/.cache.
```

**Scenario 6: Concurrent scans → no corruption**
```
Given: two `mikebom sbom scan --image alpine:3.19` processes start at
       the same moment with an empty cache
When:  both complete
Then:  both produce correct SBOMs; the cache files on disk are
       intact (no zero-byte / truncated files left by interleaved
       writes). Scenario verifies via stress-test (loop in inline
       test).
```

## Edge cases

- **Cache dir on a read-only filesystem.** Don't crash. Fall through
  to network fetch on every blob; log a single `tracing::warn!` at
  startup naming the dir + IO error.
- **Cache dir creation fails (e.g. permission denied).** Same as
  above — log + fall through.
- **Insufficient disk space mid-write.** Tempfile-with-rename: the
  partial write goes to a `.tmp.<pid>` file in the cache dir; on
  write failure we delete the tempfile and fall through. The
  in-place final-name file is never touched mid-write, so a
  concurrent reader never sees a partial blob.
- **SHA-256 of a non-`sha256:` digest.** `verify_sha256` already
  rejects non-sha256 algorithms; the cache reuses that.
  Future-proofing for sha512 is out of scope (no registry uses it
  in practice today).
- **Eviction on an actively-being-read file.** Unix's `rename`/
  `unlink` semantics mean an open fd survives the directory-entry
  removal — readers complete safely. Eviction does NOT need a
  process-wide lock.
- **Symlink in cache dir.** Cache file paths are
  `<cache>/sha256/<hex>`; `<hex>` is sanitized (only `[0-9a-f]+`
  passes verify_sha256's check). No path-traversal surface.

## Functional requirements

- **FR-001**: New module `mikebom-cli/src/scan_fs/oci_pull/cache.rs`
  exports (crate-private):
  - `pub(super) struct Cache { dir: PathBuf, size_cap: u64 }`.
  - `pub(super) fn open(size_cap: u64) -> Result<Option<Cache>>` —
    resolves the cache dir, creates it if absent, returns `None` on
    any IO failure (cache becomes inert; non-fatal).
  - `pub(super) fn get(&self, digest: &str) -> Option<Vec<u8>>` —
    reads `<dir>/sha256/<hex>`, verifies SHA-256, returns bytes on
    success. Updates the file's mtime for LRU. On corruption,
    deletes the entry and returns `None`.
  - `pub(super) fn insert(&self, digest: &str, bytes: &[u8]) -> Result<()>` —
    writes via tempfile + atomic rename. Triggers eviction if total
    cache size exceeds `size_cap`.

- **FR-002**: `cache.rs::resolve_cache_dir() -> Option<PathBuf>` —
  resolves the cache directory in priority order:
  1. `$MIKEBOM_OCI_CACHE_DIR` (if set non-empty).
  2. `$XDG_CACHE_HOME/mikebom/oci-layers` (Linux convention).
  3. macOS: `$HOME/Library/Caches/mikebom/oci-layers`.
  4. fallback: `$HOME/.cache/mikebom/oci-layers`.
  Returns `None` if `$HOME` is also unset and no override env var.

- **FR-003**: Digest validation in `cache.rs`. Only `sha256:<64-hex>`
  digests are cacheable. Anything else (`sha512:`, malformed) →
  `get` returns `None`, `insert` returns `Ok(())` no-op (caller
  already ran `verify_sha256` on the network bytes, so a
  non-cacheable digest just falls through to anonymous network).

- **FR-004**: LRU eviction. After every successful `insert`, if the
  total size of all `<dir>/sha256/*` files exceeds `size_cap`, walk
  the directory entries sorted by mtime ascending, `remove_file`
  until the total drops below the cap. Log evictions at
  `tracing::debug!`.

- **FR-005**: `mikebom-cli/src/scan_fs/oci_pull/registry.rs::RegistryClient`
  gains `cache: Option<Cache>` (parallel to the existing
  `credentials: Option<Credential>`). `RegistryClient::new` accepts
  a `cache: Option<Cache>` parameter (constructed by the caller).
  `fetch_blob` consults the cache: hit → return cached bytes (still
  verified); miss → network fetch → cache insert (if cache is
  Some) → return.

- **FR-006**: `mikebom-cli/src/cli/scan_cmd.rs::ScanArgs` gains:
  - `pub no_oci_cache: bool` with `#[arg(long)]`. Env-var fallback:
    `MIKEBOM_OCI_CACHE=0` is equivalent to `--no-oci-cache`.
  - `pub oci_cache_size: Option<u64>` with `#[arg(long, value_name =
    "BYTES")]`. Default when unset: 10 GB
    (`10 * 1024 * 1024 * 1024`). Env-var fallback:
    `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.

- **FR-007**: `mikebom-cli/src/scan_fs/oci_pull/mod.rs::pull_to_tarball`
  signature gains a `cache: Option<&Cache>` parameter (or threads
  via a builder). The `RegistryClient::new` call passes it through.

- **FR-008**: No new top-level deps. Uses `tempfile` (already a dep),
  `std::fs`, `std::time::SystemTime` for mtime, `std::env::var`.
  `dirs` is intentionally NOT added — env-var fallback chain is ~30
  LOC and avoids the new crate.

- **FR-009**: All existing oci_pull tests pass unchanged. New inline
  tests in `cache.rs` cover: cold-cache hit/miss, corruption
  recovery, eviction order, digest-format rejection, dir-creation
  failure (read-only fs simulation via tempdir), concurrent-write
  stress (10 threads × 100 inserts).

## Success criteria

- **SC-001**: `./scripts/pre-pr.sh` clean.
- **SC-002**: `git diff main..HEAD -- mikebom-cli/src/parity/
  mikebom-cli/src/generate/ mikebom-cli/src/resolve/` empty.
- **SC-003**: 27-golden regen produces zero diff.
- **SC-004**: `MIKEBOM_OCI_NETWORK_TESTS=1 cargo +stable test -p
  mikebom --test oci_registry_smoke pulls_alpine_3_19_and_emits_apk_components`
  on a freshly-cleared cache: first run does N network blob
  fetches; second run does 0 (asserted via a counter in the
  hyper-tiny test server, not the real registry — separate inline
  test).
- **SC-005**: `wc -l mikebom-cli/src/scan_fs/oci_pull/cache.rs` ≤
  500 (production + tests).
- **SC-006**: `git diff main..HEAD -- mikebom-cli/Cargo.toml
  mikebom-common/Cargo.toml mikebom-ebpf/Cargo.toml Cargo.toml |
  grep -E '^\+[a-z][a-z0-9_-]+ = '` empty (no new top-level deps).
- **SC-007**: All 3 CI lanes green.

## Clarifications

- **Why cache all blobs, not just layers?** Config blobs are tiny
  (a few KB each) but cost zero to cache. Treating "blob" uniformly
  in the cache simplifies the seam. The issue text says "layer
  cache" but in OCI distribution-spec parlance, both config and
  layers are blobs fetched via the same `/v2/<repo>/blobs/<digest>`
  endpoint.
- **Why no `dirs` crate?** Constitution Principle VI prefers reusing
  existing deps over adding new ones. The cache-dir resolution is
  ~30 LOC of env-var chain + `cfg!(target_os)` switching — well
  within "build it ourselves" range. Same posture as the milestone
  034 `auth.rs` rejection of `dirs` for `~/.docker/config.json`.
- **Why mtime-based LRU instead of an index file?** Index files are
  a synchronization headache (lock-free concurrent updates require
  an extra abstraction). Filesystem mtime is universal, atomic on
  every POSIX-ish system, and updated for free on read by
  `File::open`-with-`O_NOATIME`-not-set. Eviction walks the
  directory once at end-of-insert; for a cache with hundreds of
  entries this is microseconds.
- **Why not cache the manifest?** A floating tag (`:latest`,
  `:edge`) MUST be re-fetched to detect updates. Caching the
  manifest would mask updates. The manifest fetch is also tiny
  (~few KB) so cache-savings would be negligible.
- **Why default 10 GB?** Big enough to fit a few mid-sized images
  (Ubuntu, Debian, Node.js) without rewriting; small enough that a
  laptop user's `~/.cache` doesn't balloon unexpectedly. Override
  via `--oci-cache-size` or `MIKEBOM_OCI_CACHE_SIZE`.

## Out of scope

- **Manifest caching.** See Clarifications.
- **Distributed cache** (multiple machines sharing a directory) —
  the current design uses local rename atomicity which doesn't
  generalize across NFS / Amazon EFS in all cases. Real-world
  shared caches are an integration concern, not a mikebom feature.
- **Compression in the cache.** Layers are already gzipped on the
  wire; we cache them as fetched. No re-compression.
- **`--clear-oci-cache` CLI command.** Users can `rm -rf` the
  directory; adding a CLI knob for it inflates surface area for
  marginal benefit.
- **Pre-warm / cache-fill commands.** Defer if real demand surfaces.
- **`#64 dpkg status.d/`** is the next non-OCI-queue item.
