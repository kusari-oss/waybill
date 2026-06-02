# Phase 0 Research — External symbol-fingerprint corpus

**Feature**: 108-fingerprint-corpus
**Status**: Complete — no `NEEDS CLARIFICATION` items remain. This document captures the validation of the four design assumptions backing the plan.

---

## R1: GitHub archive-download API as the fetch transport

**Decision**: Fetch the corpus tarball via `https://github.com/kusari-sandbox/mikebom-fingerprints/archive/<sha>.tar.gz` using the workspace `reqwest = "0.12"` (`rustls-tls` + `blocking` features already enabled). Decompress with `flate2` (workspace) into `tar = "0.4"` (workspace) for extraction.

**Rationale**: GitHub's archive endpoint resolves to a stable tarball-per-commit URL with no auth required for public repos. The URL pattern is documented + stable since 2009. Reuses existing workspace dependencies; no new transitive C deps (`reqwest`'s `rustls-tls` is pure Rust). Same fetch pattern as milestone 090's `mikebom-test-fixtures` setup.

**Alternatives considered**:

- **`git2` library** (pure-Rust libgit2 binding): rejected — `git2`'s `libgit2-sys` brings in C dependencies (`libgit2`, `libssh2`, optionally `openssl`-sys). Violates Constitution Principle I + Strict Boundary 3. Same rationale as milestone 090's rejection.
- **Shell out to `git clone` / `git fetch`**: `git` is already in mikebom's dependency closure (milestone 053 + 090 use it), so this is technically permissible. Rejected because: (a) `git clone --depth 1` requires more bandwidth than a single tarball fetch; (b) per-SHA fetches require `git clone --depth 1 --branch <sha>` syntax which doesn't work — git only accepts branches/tags as `--branch`. Tarball is simpler and faster for SHA-pinned snapshots.
- **GitHub API (`/repos/<owner>/<repo>/tarball/<sha>`)**: works identically to the archive endpoint, but requires an auth header for some rate-limit tiers + redirects to the archive endpoint anyway. Direct archive is simpler.
- **`oras` / OCI registry**: out-of-scope — we'd need an OCI registry to host the corpus, doubling the infrastructure surface. GitHub archives are zero-infra.

**Validation**: Tested locally — `curl -fsSL https://github.com/kusari-sandbox/mikebom/archive/main.tar.gz | tar -xz` resolves cleanly with no auth. Tarball top-level directory is `<repo>-<short-sha>/` (or `<repo>-<branch>/` for branch-name fetches), which mikebom's extractor will strip the one-level prefix from.

---

## R2: Cache-directory layout consistency with milestone 090

**Decision**: Cache at `~/.cache/mikebom/fingerprints/<full-40-hex-sha>/corpus/` — mirrors milestone 090's `mikebom-test-fixtures` cache at `~/.cache/mikebom/fixtures/<sha>/`. Full 40-hex SHA in the directory name (NOT the 12-hex truncation used in the SBOM annotation) to eliminate cache-key collisions.

**Rationale**: Consistency with the established pattern lets operators reason about both caches uniformly. The full 40-hex collision resistance is overkill for any realistic repo size, but costs nothing. The 12-hex truncation is reserved for the human-readable SBOM annotation per FR-005.

**Cache invariants**:

- **Atomic writes**: extract the tarball into `<cache-root>/.tmp-<random>/` then `std::fs::rename` to `<cache-root>/<sha>/`. Concurrent scans don't see partial cache state.
- **No auto-eviction**: per FR-009, explicit `mikebom fingerprints cache-clear` only.
- **Validation at load time**: the cache loader verifies `<cache-root>/<sha>/corpus/index.json` exists + parses; on validation failure, the entire `<sha>` directory is treated as corrupt + the next fetch overwrites it.

**Alternatives considered**:

- **Per-record cache files** (`~/.cache/mikebom/fingerprints/<library>.json` flat): rejected — loses SHA-pinning. Two operators with different SHAs would clobber each other.
- **XDG_CACHE_HOME respect**: `~/.cache/mikebom/` IS the XDG default (`$XDG_CACHE_HOME` falls back to `$HOME/.cache`). The `dirs` crate (already a transitive dep) handles this on macOS / Windows too.

**Validation**: Confirmed against the existing milestone 090 cache machinery — same `dirs::cache_dir()` resolution + same `<root>/<sha>/` content addressing.

---

## R3: Build-time `env!()` embed via `build.rs` + `Cargo.toml [package.metadata]`

**Decision**: Pin the build-time corpus SHA in `mikebom-cli/Cargo.toml`:

```toml
[package.metadata.fingerprints]
corpus_sha = "<40-hex-sha>"
```

`mikebom-cli/build.rs` parses this at build time and emits `cargo:rustc-env=MIKEBOM_FINGERPRINTS_CORPUS_SHA=<sha>`. At runtime, the loader resolves the SHA via `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")`.

**Rationale**:

- **Cargo-native**: the SHA pin lives next to the version + edition + dep pins it accompanies. PR diffs that bump the corpus version show up in `Cargo.toml` review like any other dep update.
- **No new build dependency**: `build.rs` parsing the crate's own `Cargo.toml` via `std::fs::read_to_string` + a small TOML parser is trivial. We already use `toml = "0.8"` (workspace) elsewhere — `build.rs` can use it.
- **Compile-time enforcement**: if the SHA pin is malformed or missing, the build fails. The reproducibility contract is unforgeable at runtime.

**Alternatives considered**:

- **Runtime config file** (`mikebom.toml` or similar): rejected — operator-side config defeats the reproducibility purpose. Two operators on the same mikebom binary could pin different SHAs.
- **Compile-time literal in source code** (e.g., `const FINGERPRINTS_CORPUS_SHA: &str = "abc..."`): functionally equivalent to the metadata approach but PRs to bump the SHA would touch code, not metadata. Code-vs-metadata is a stylistic call; metadata is closer to how `version =` and `repository =` are handled.
- **Git-based auto-detection at build time** (`build.rs` shells out to `git ls-remote` against the sibling repo): rejected — couples the build environment to a network call. Hermetic builders couldn't reproduce the build offline.

**Validation**: TOML metadata parsing in `build.rs` is well-trodden across the Rust ecosystem. The `cargo:rustc-env` directive is documented (https://doc.rust-lang.org/cargo/reference/build-scripts.html#cargo-warning).

---

## R4: JSON Schema validation in sibling-repo CI

**Decision**: The sibling-repo `mikebom-fingerprints` ships its own CI workflow that validates every `corpus/<library>.json` file against `schema/fingerprint-record.v1.json` at PR time. Records that fail validation block the PR. mikebom-cli at scan time treats records as TRUSTED (no re-validation) — they passed the upstream review per FR-010's "PRs to the sibling repo are the review point" semantics.

The CI also enforces semantic invariants beyond pure schema:

- **`min_symbols ≥ 5`** — refuses records with thresholds tighter than 5 to prevent extreme false-positive rates. Q3 clarification recommended per-record values; this is the floor.
- **`symbols.len() ≥ 2 × min_symbols`** — refuses records whose symbol list is too short relative to the threshold (otherwise the threshold is meaningless).
- **Prefix distinctiveness** — refuses records whose top-3 symbols are all common-prefix terms like `init`, `start`, `open` that any binary might export.
- **PURL form validity** — refuses records whose `target_purl` field doesn't parse as a valid PURL.

**Rationale**: Validation upstream (at PR time, where humans review the change anyway) is more efficient than re-validating at scan time on every operator's machine. mikebom-cli runs against trusted snapshots; corpus authors are accountable for the contents of merged PRs. Same trust model as milestone 090's fixture repo.

**Alternatives considered**:

- **mikebom-cli-side validation at scan time**: rejected — every operator's first scan would re-run the validation, wasting CPU + producing noisy warnings for records that the sibling-repo CI already proved good. mikebom-cli still has a minimal defensive parse (per FR-010: records that fail to deserialize are skipped + warned) but doesn't re-run the upstream semantic checks.
- **No CI validation; trust contributors**: rejected — schema drift is too easy. The CI is one workflow YAML + a tiny script; the maintenance cost is minimal.
- **TypeScript-style runtime types**: out of scope — JSON Schema is the standard, well-tooled choice (e.g., `ajv` for the CI checker).

**Validation**: This matches the milestone 090 sibling-repo CI pattern. JSON Schema is a stable RFC standard with mature tooling in every language; the sibling repo's CI can use any off-the-shelf validator.

---

## Summary

| Item | Decision | Confidence |
|---|---|---|
| R1: Fetch transport | GitHub archive download via `reqwest` + `tar` + `flate2` | High (no new deps; tested URL pattern) |
| R2: Cache layout | `~/.cache/mikebom/fingerprints/<full-40-hex-sha>/corpus/` with atomic writes | High (mirrors milestone 090) |
| R3: SHA pin | `Cargo.toml [package.metadata.fingerprints]` + `build.rs`-emitted env var | High (Cargo-native) |
| R4: Validation | Sibling-repo CI at PR time; mikebom-cli trusts SHA-pinned snapshots | High (matches milestone 090) |

No follow-up research blocks remain. Proceed to Phase 1.
