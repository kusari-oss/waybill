---
description: "Implementation plan — milestone 036 OCI layer cache"
status: plan
milestone: 036
---

# Plan: OCI layer cache

## Architecture

```
┌─────────────────┐
│ scan_cmd::scan  │ resolves no_oci_cache + oci_cache_size from
└────────┬────────┘ args/env, calls cache::open(...)
         │
         ▼
┌─────────────────┐
│ pull_to_tarball │ takes Option<&Cache> arg, threads to
└────────┬────────┘ RegistryClient::new
         │
         ▼
┌─────────────────────┐
│ RegistryClient.cache│ Option<Cache> field, parallel to
└────────┬────────────┘ existing `credentials: Option<Credential>`
         │
         ▼
┌─────────────────────┐
│ fetch_blob          │ try cache.get(digest) → return cached
│                     │ on cache miss → network fetch → cache.insert
└─────────────────────┘ (if cache is Some)
```

A new `cache.rs` sibling to `auth.rs` and `registry.rs` owns the
disk surface: dir resolution, atomic-rename writes, SHA-256 verify
on read with corruption fallback, mtime-based LRU eviction.

No public API change. No new top-level dep. Lives entirely behind
the existing `oci-registry` Cargo feature.

## Reuse inventory

- **`tempfile::NamedTempFile`** — workspace dep. Atomic rename via
  `persist()`.
- **`sha2::Sha256` + the existing `verify_sha256` shape in
  registry.rs** — the same verification primitive already in use;
  cache reuses it.
- **`std::env::var`, `std::fs`, `std::time::SystemTime`,
  `std::path::PathBuf`** — stdlib; no `dirs` crate.
- **Existing `tracing::warn!` posture from auth.rs** — same
  redaction discipline (cache paths and IO errors are not
  secrets, but the warning style is the same).
- **`#[cfg(target_os = "macos")]` switching** — already used
  elsewhere; cache uses it for `~/Library/Caches` vs `~/.cache`.

## Touched files

| File | Change | LOC |
|---|---|---|
| `mikebom-cli/src/scan_fs/oci_pull/cache.rs` | NEW — Cache struct, dir resolution, get/insert, eviction | +400 |
| `mikebom-cli/src/scan_fs/oci_pull/mod.rs` | declare module; thread cache through pull_to_tarball | +25 |
| `mikebom-cli/src/scan_fs/oci_pull/registry.rs` | RegistryClient.cache field + fetch_blob lookup/insert | +40 |
| `mikebom-cli/src/cli/scan_cmd.rs` | --no-oci-cache + --oci-cache-size; resolve env vars; build Cache | +60 |
| `mikebom-cli/tests/oci_registry_smoke.rs` | gated repeat-scan smoke test (cache-hit verification via fixture mock) | +60 |
| `docs/user-guide/cli-reference.md` | flag rows + cache section | +40 |
| `CHANGELOG.md` | unreleased entry | +5 |

Total ~630 LOC across 7 files. Bulk in `cache.rs` (with inline
tests).

## Phasing

Three atomic commits, each `./scripts/pre-pr.sh`-clean.

### Commit 1: `036/cache-module`
- New `cache.rs` with Cache + open + get + insert + eviction.
- Inline tests (10+) covering: cold/warm get, mtime-LRU eviction
  order, corruption recovery, sha512 rejection, missing-dir
  handling, tempfile cleanup on insert failure.
- Concurrent-write stress test using `std::thread::scope` (10
  threads × 100 inserts to same digest set; verify final files
  intact).
- mod.rs declares `mod cache;` with `#[allow(dead_code)]`
  (lifted in commit 2).

### Commit 2: `036/wire-cache`
- `RegistryClient` accepts `cache: Option<Cache>`.
- `fetch_blob` consults cache first, inserts on miss.
- `pull_to_tarball` takes `cache: Option<&Cache>` and forwards.
- ScanArgs gains `--no-oci-cache` + `--oci-cache-size` (with env
  fallbacks); scan_cmd builds the Cache.
- Inline integration test in registry.rs uses tokio TCP listener
  to count blob fetches, asserts second-fetch is zero-network.
- Remove `#[allow(dead_code)]`.

### Commit 3: `036/docs-and-smoke`
- cli-reference.md: new flag rows + "OCI layer caching" subsection.
- CHANGELOG entry.
- Cargo.toml feature comment refresh.
- Gated network smoke test for the warm-cache speedup.

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Commit 1 | 6 hr | Eviction + concurrent-write tests are the careful parts |
| Commit 2 | 4 hr | Mock-server test for hit-counting |
| Commit 3 | 2 hr | Mostly text |
| Verification + PR | 1 hr | CI fast (<5 min) |
| **Total** | **~13 hr** | Slightly under the 2-day issue estimate. |

## Risks

- **R1: mtime resolution on filesystems.** ext4 / APFS update mtime
  per second by default; that's coarse but adequate for LRU
  ordering of cache reads. tmpfs and some network filesystems may
  have lower resolution. If two reads happen in the same second,
  eviction order is filesystem-dependent — acceptable for an LRU
  approximation.
- **R2: NamedTempFile + persist on cross-FS rename.** `persist()`
  uses `rename(2)` which is atomic only within the same
  filesystem. If `$HOME` is one mount and `/tmp` is another (the
  default tempfile dir), the rename could fail with EXDEV.
  Mitigation: create the tempfile inside the cache dir itself
  (`NamedTempFile::new_in(&cache_dir)`) so the rename is always
  intra-fs.
- **R3: Concurrent eviction races.** Two processes inserting blobs
  simultaneously, each running eviction at end-of-insert, could
  both choose the same victim and one would unlink-fail. Mitigate
  by treating "file not found" on unlink as success during
  eviction. The eventual-correct-size invariant is what matters.
- **R4: Cache-dir resolution falsely returns Some on a path that
  doesn't exist.** Mitigated by `open()` calling `create_dir_all`
  and verifying writability via a probe-write to a `.mikebom-cache-probe`
  file (deleted immediately after the probe). Failure → return
  None; cache is inert.
- **R5: SHA-256 verify on every read.** ~30 ms per 30 MB blob on
  modern hardware. Doubles a warm-cache pull's cost vs the
  zero-verify alternative. Justified: silent corruption is the
  opposite of the safety property a content-addressed cache
  should provide. (Future: opt-in `--oci-cache-skip-verify` if
  benchmarks show it matters.)

## Constitution alignment

- **Principle I (zero C):** No new deps. ✓
- **Principle IV (no `.unwrap()` in production):** `cache.rs`
  returns `Option`/`Result` throughout. Tests use the standard
  `cfg_attr(test, allow(clippy::unwrap_used))` envelope. ✓
- **Principle VI (three-crate architecture):** Untouched. ✓
- **Per-commit verification:** FR-009.

## What this milestone does NOT do

- Does not introduce a `--clear-oci-cache` CLI command.
- Does not cache the manifest.
- Does not implement a distributed / shared-volume cache.
- Does not change SBOM output bytes — the same image bytes produce
  the same SBOM regardless of cache hit/miss.
