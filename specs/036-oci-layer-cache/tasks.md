---
description: "Task list — milestone 036 OCI layer cache"
---

# Tasks: OCI layer cache

**Input**: spec.md (✅), plan.md (✅), checklists/requirements.md (✅).

**Tests**: ~12 inline tests in `cache.rs` (hit/miss/corruption/
eviction/concurrent-write); 1 mock-server integration test in
`registry.rs` (zero-network on warm cache); 1 gated network smoke
test for the warm-cache speedup.

**Organization**: Single user story (US1, P1). Three atomic commits.

## Path conventions

- Adds `mikebom-cli/src/scan_fs/oci_pull/cache.rs` (new module).
- Touches `mikebom-cli/src/scan_fs/oci_pull/{mod,registry}.rs`.
- Touches `mikebom-cli/src/cli/scan_cmd.rs` (additive — 2 new
  flags + cache construction).
- Touches `mikebom-cli/tests/oci_registry_smoke.rs` (additive).
- Touches `docs/user-guide/cli-reference.md` and `CHANGELOG.md`.
- Does NOT touch parity/, generate/, resolve/, attestation/, or
  any other CLI command.

---

## Phase 1: Setup + baseline

- [X] T001 Recon done in this session: `fetch_blob` at
      `registry.rs:91-102` is the auth/cache seam — both auth
      (already wired in 034) and cache (this milestone) wrap the
      same fetch. The cache wraps OUTSIDE the network fetch (try
      cache → fall through to network → insert).
- [ ] T002 Snapshot baseline: `./scripts/pre-pr.sh 2>&1 | tee
      /tmp/baseline-036.txt | grep -E '^test [a-z_:]+ \.\.\. ok' |
      sort -u > /tmp/baseline-036-tests.txt`.

---

## Phase 2: Commit 1 — `036/cache-module`

**Goal**: New `cache.rs` with full disk-cache semantics + inline
tests. No call sites yet.

- [ ] T003 [US1] Create `mikebom-cli/src/scan_fs/oci_pull/cache.rs`. Header doc-comment names the milestone, links the OCI distribution-spec blob endpoint, and explains the SHA-256-content-addressed safety story.
- [ ] T004 [US1] Add `pub(super) struct Cache { dir: PathBuf, size_cap: u64 }`.
- [ ] T005 [US1] Add `fn resolve_cache_dir() -> Option<PathBuf>` per FR-002. Inline test covers each env-var path.
- [ ] T006 [US1] Add `pub(super) fn open(size_cap: u64) -> Option<Cache>`. Resolves dir, `create_dir_all`, probe-writes to `.mikebom-cache-probe` to verify writability. Inline test for missing-HOME → None and read-only-dir → None.
- [ ] T007 [US1] Add `pub(super) fn get(&self, digest: &str) -> Option<Vec<u8>>`. Validates digest format (must be `sha256:<64-hex>`); reads file; verifies SHA-256; touches mtime via `filetime::set_file_mtime` — wait, that's a new dep. Use `std::fs::File::open` then immediately drop and re-open with `OpenOptions::new().write(true).open(&path)` followed by no-op write of zero bytes? No — simpler: just leave mtime untouched on read (atime carries the LRU). Actually atime is unreliable due to `noatime` mount option. Pragmatic: just call `std::fs::File::set_modified(SystemTime::now())` on the open file — stable since Rust 1.75. Fallback if that's not available: rewrite a sentinel (skip — too complex). Use `set_modified`.
- [ ] T008 [US1] Add `pub(super) fn insert(&self, digest: &str, bytes: &[u8]) -> Result<()>`. Validates digest; creates `<dir>/sha256/`; writes via `tempfile::NamedTempFile::new_in(&dir)` + `persist()`. Triggers eviction post-insert.
- [ ] T009 [US1] Add `fn evict_to_cap(&self) -> Result<()>`. Walks `<dir>/sha256/`, sums file sizes, sorts by mtime ascending, removes oldest until under `size_cap`. Uses `read_dir` + `metadata.modified()` + `metadata.len()`.
- [ ] T010 [US1] Add `fn verify_sha256_file(path: &Path, expected_hex: &str) -> bool` — read all bytes, hash, compare. (Mirrors registry.rs's `verify_sha256` but on-disk.)
- [ ] T011 [US1] Inline test: cold cache miss → returns None.
- [ ] T012 [US1] Inline test: insert + get round-trips bytes.
- [ ] T013 [US1] Inline test: get with corrupted file → returns None AND deletes the file.
- [ ] T014 [US1] Inline test: get with non-sha256 digest → returns None (no panic).
- [ ] T015 [US1] Inline test: insert with non-sha256 digest → no-op, no error.
- [ ] T016 [US1] Inline test: eviction with size_cap=1KB and three 500B entries inserted in order → after the third insert, the oldest is evicted.
- [ ] T017 [US1] Inline test: concurrent inserts (10 threads × 50 same-digest writes) → final file is correct + intact.
- [ ] T018 [US1] Inline test: probe-write fails on read-only tempdir → `open` returns None.
- [ ] T019 [US1] Inline test: missing $HOME + missing override env → `resolve_cache_dir` returns None.
- [ ] T020 [US1] Edit `mod.rs` to add `#[allow(dead_code)] mod cache;`.
- [ ] T021 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T022 [US1] Commit: `feat(036/cache-module): add SHA-256-content-addressed disk cache for OCI blobs`.

---

## Phase 3: Commit 2 — `036/wire-cache`

**Goal**: Cache is consulted by `fetch_blob`; CLI flags + env-var
fallbacks build the Cache; mock-server test verifies zero-network
on warm cache.

- [ ] T023 [US1] Edit `registry.rs::RegistryClient`: add `cache: Option<Cache>` field. Update `RegistryClient::new` signature to take `cache: Option<Cache>` and store it.
- [ ] T024 [US1] Edit `registry.rs::fetch_blob`: before the network fetch, consult `self.cache.as_ref().and_then(|c| c.get(digest))`. On hit, return immediately (verify_sha256 already ran inside cache.get). On miss, do the existing fetch, then `if let Some(c) = self.cache.as_ref() { let _ = c.insert(digest, &bytes); }` (insert errors logged but non-fatal).
- [ ] T025 [US1] Edit `mod.rs::pull_to_tarball` signature: gain `cache: Option<&Cache>` (or `Option<Cache>` — pick what's ergonomic). Pass through to `RegistryClient::new`.
- [ ] T026 [US1] Edit `scan_cmd.rs::ScanArgs`: add `pub no_oci_cache: bool` and `pub oci_cache_size: Option<u64>` with the doc-comments and clap attributes per FR-006.
- [ ] T027 [US1] Edit `scan_cmd.rs` (the OciRef branch): build the cache. Logic: if `args.no_oci_cache || env("MIKEBOM_OCI_CACHE") == Some("0")` → None. Else compute size = `args.oci_cache_size.or_else(|| env("MIKEBOM_OCI_CACHE_SIZE").parse().ok()).unwrap_or(10 GB)`; call `cache::open(size)`. Pass into `pull_to_tarball`.
- [ ] T028 [US1] Remove `#[allow(dead_code)]` from `mod.rs`'s `mod cache;`.
- [ ] T029 [US1] Add inline integration test in `registry.rs`: tokio TCP listener with a connection-counter; first `fetch_blob` round-trip uses it; second `fetch_blob` for same digest uses 0 connections (cache hit). Construct `RegistryClient` via field-init with explicit `cache: Some(Cache { dir: tempdir, size_cap: 1<<30 })`.
- [ ] T030 [US1] `cargo +stable test -p mikebom --bin mikebom scan_fs::oci_pull` green; binary test count went up by inline-test count.
- [ ] T031 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T032 [US1] Commit: `feat(036/wire-cache): consult disk cache in fetch_blob; --no-oci-cache + --oci-cache-size flags`.

---

## Phase 4: Commit 3 — `036/docs-and-smoke`

**Goal**: User-facing docs, CHANGELOG, gated smoke test.

- [ ] T033 [US1] Edit `docs/user-guide/cli-reference.md`: add `--no-oci-cache` and `--oci-cache-size` flag rows; add an "OCI layer caching" subsection covering: cache location precedence (MIKEBOM_OCI_CACHE_DIR > XDG_CACHE_HOME > macOS Caches > ~/.cache), default size cap, how to clear (rm -rf), env-var equivalents.
- [ ] T034 [US1] Edit `CHANGELOG.md`: unreleased entry — disk cache for OCI blobs (closes #68).
- [ ] T035 [US1] Edit `mikebom-cli/Cargo.toml`'s `oci-registry` feature comment to mention caching.
- [ ] T036 [US1] Edit `mikebom-cli/tests/oci_registry_smoke.rs`: add `repeat_pull_uses_cache_and_skips_network` gated test. Pre-clears `MIKEBOM_OCI_CACHE_DIR=<tempdir>`; pulls alpine:3.19 twice; asserts second pull's tracing log contains `cache hit` lines (or asserts wall-clock duration on second is <50% of first).
- [ ] T037 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T038 [US1] Commit: `feat(036/docs-and-smoke): document --no-oci-cache + --oci-cache-size + gated warm-cache smoke test`.

---

## Phase 5: Verification + PR

- [ ] T039 SC-001: `./scripts/pre-pr.sh` clean.
- [ ] T040 SC-002: `git diff main..HEAD -- mikebom-cli/src/parity/ mikebom-cli/src/generate/ mikebom-cli/src/resolve/` empty.
- [ ] T041 SC-003: `MIKEBOM_UPDATE_*_GOLDENS=1 ./scripts/pre-pr.sh` produces zero diff.
- [ ] T042 SC-005: `wc -l mikebom-cli/src/scan_fs/oci_pull/cache.rs` ≤ 500.
- [ ] T043 SC-006: `git diff main..HEAD -- mikebom-cli/Cargo.toml ... | grep -E '^\+[a-z]'` empty.
- [ ] T044 Push branch; observe all 3 CI lanes green (SC-007).
- [ ] T045 Open PR closing #68.
