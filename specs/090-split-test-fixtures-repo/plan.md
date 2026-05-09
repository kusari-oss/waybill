# Implementation Plan: Split test fixtures into separate repo

**Branch**: `090-split-test-fixtures-repo` | **Date**: 2026-05-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/090-split-test-fixtures-repo/spec.md`

## Summary

Move 44 manifest-bearing fixture directories (across `mikebom-cli/tests/fixtures/` + `tests/fixtures/`) into a separate Git repository at `kusari-sandbox/mikebom-test-fixtures`. mikebom main repo retains goldens (regression contract), schemas (upstream JSON for offline validation), binary fixtures (no source-language manifests, no scanner trigger), OS-package synthetic fixtures (no source-language manifests), `bdb-rpmdb`, `gem-source-project`, `polyglot-rpm-binary`, `reference/`, and `sample-attestation.json`.

Tests fetch the fixture repo at build time via a `build.rs` hook in `mikebom-cli/` that clones the pinned revision into `~/.cache/mikebom/fixtures/<pinned-rev>/` (or `$MIKEBOM_FIXTURE_CACHE` if overridden). The path is exposed to test code via the `MIKEBOM_FIXTURES_DIR` compile-time env var. Cache-warm subsequent builds skip the network round-trip. Cache-miss + no-network fails fast with an actionable error per Constitution Principle III.

The fixture repo's revision pin lives in `tests/fixtures.rev` at the mikebom main repo root (a single-line file containing a Git SHA). Bumping the pin = a one-line diff visible in PR review; historical reproducibility holds across mikebom commits per US3.

Existing `tests/fixtures/` and `mikebom-cli/tests/fixtures/<manifest-bearing-subdir>/` paths in 70 test files + 6 source files get rewritten to use a new `fixture_path(rel: &str) -> PathBuf` helper that resolves relative paths against `MIKEBOM_FIXTURES_DIR`. Goldens stay accessed via the existing `workspace_root()`-based helpers (no API change for the regression-test side).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–089; no nightly required for this user-space-only test-infra refactor).
**Primary Dependencies**: ONE new direct dep on `mikebom-cli` — `git2 = "0.19"` for pure-Rust Git clone in `build.rs`. Alternative: shell out to `git` via `std::process::Command` (mikebom already does this in golang reader + cargo reader). **Decision**: shell out to `git` — same pattern as existing readers (Constitution-friendly: zero new transitive crates, no `git2`'s `libgit2-sys` C dependency). The `git` binary is already a hard prereq for any mikebom dev setup.
**Storage**: Per-host fixture cache directory at `$MIKEBOM_FIXTURE_CACHE` (default: `~/.cache/mikebom/fixtures/<pinned-rev>/`). Cache-key is the pinned Git SHA so multiple revisions can co-exist (useful during git-bisect through mikebom history). The cache layout mirrors the fixture repo's internal directory structure exactly so `fixture_path("transitive_parity/cargo")` resolves to `<cache>/transitive_parity/cargo`.
**Testing**: existing `cargo +stable test --workspace` flow. The build.rs runs once per build target (per crate), and `cargo test` triggers it for the integration-test target.
**Target Platform**: Linux + macOS (matches existing CI lanes). Windows not supported (matches existing mikebom posture).
**Project Type**: Single project — Rust workspace test-infra refactor + a new sibling Git repository.
**Performance Goals**: First-fetch ≤30 s on standard developer hardware (SC-003). Cache-warm fetch is a no-op (zero network, ≤100 ms wall-time for the cache-existence check). CI lanes pay the first-fetch cost once per fresh runner; with the standard `actions/cache` GitHub setup, the fixture cache survives across CI runs.
**Constraints**: Constitution Principle I (Pure Rust, Zero C) — non-negotiable. The shell-out-to-git approach has zero new C deps. Constitution Principle III (Fail Closed) — fixture-fetch failures emit explicit errors, no silent test skips. Constitution Principle XII allows external data sources for enrichment but this isn't enrichment — it's test setup; doesn't apply.
**Scale/Scope**: 44 manifest-bearing fixture directories migrate (~17 MB of content, mostly the milestone-083 audit fixtures + polyglot-monorepo). 70 test files + 6 source files get path-rewrite. ~20 LOC new build.rs. ~30 LOC new `fixture_path()` helper. New `tests/fixtures.rev` file (1 line). Updated `.github/workflows/ci.yml` to cache the fixture-clone (~10 LOC). New `mikebom-test-fixtures` repo seeded with the moved content. Total mikebom main repo diff: ~700+ deletions (the moved directories) + ~200 additions (build.rs, helper, path rewrites, CI cache).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ PASS | Shell-out to `git` (already a hard prereq) — zero new C-linking crates. Rejected `git2 = "0.19"` because `libgit2-sys` would violate. |
| II. eBPF-Only Observation | ✅ PASS | Not applicable — test infrastructure, no discovery code path. |
| III. Fail Closed | ✅ PASS | Fixture-fetch failure (network unavailable, repo unreachable, revision not found) emits structured error in build.rs panic message naming the URL + cache path + workaround. No silent test skip. Aligns with FR-007 + edge case 1. |
| IV. Type-Driven Correctness | ✅ PASS | New `fixture_path(&str) -> PathBuf` helper uses `PathBuf` (Rust newtype), not `String`. Existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` pattern preserved. No new `.unwrap()` in production code. |
| V. Specification Compliance | ✅ PASS | No SBOM-emission code path changes. CycloneDX/SPDX 2.3/SPDX 3 outputs unaffected. |
| V — Standards-native precedence | ✅ PASS | No new `mikebom:*` properties / annotations / relationships. |
| VI. Three-Crate Architecture | ✅ PASS | No new crates. mikebom-cli, mikebom-common, xtask remain the workspace members; mikebom-ebpf untouched. The build.rs lives in `mikebom-cli/build.rs` (build scripts don't count as crates). |
| VII. Test Isolation | ✅ PASS | All tests still run unprivileged. The fixture-fetch is a network call but not a privilege escalation. |
| VIII. Completeness | ✅ PASS | Test-infra change; no impact on dependency-discovery completeness. |
| IX. Accuracy | ✅ PASS | No phantom-component risk. |
| X. Transparency | ✅ PASS | Build.rs emits `cargo:warning=` lines on fixture-fetch (visible in `cargo build` output) so the network call is not hidden. |
| XI. Enrichment | ✅ PASS | No enrichment-source changes. |
| XII. External Data Source Enrichment | ✅ PASS | The fixture clone is test-input fetch, NOT enrichment of mikebom-emitted SBOMs. Principle XII's "external sources MUST NOT introduce new components" doesn't apply (no SBOM components introduced; this is test scaffolding). |

**Strict Boundaries**:
- ✅ No lockfile-based dependency discovery — unchanged.
- ✅ No MITM proxy — unchanged.
- ✅ No C code — REINFORCED. Shell-out to `git` (already a system tool) avoids `libgit2-sys`.
- ✅ No `.unwrap()` in production — unchanged. Build.rs is dev-time only; build.rs uses `expect()` with structured error messages per FR-007 (build.rs is not "production code").

**Pre-PR Verification (mandatory)**: standard `./scripts/pre-pr.sh` gate, post-fixture-fetch. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` must report clean.

**Gate verdict**: ✅ all gates pass. No constitution amendments required.

## Project Structure

### Documentation (this feature)

```text
specs/090-split-test-fixtures-repo/
├── plan.md              # This file
├── research.md          # Phase 0 output (fetch-mechanism + pin-mechanism + cache + migration-scope decisions)
├── data-model.md        # Phase 1 output (entities + validation rules + cache layout)
├── quickstart.md        # Phase 1 output (maintainer recipes: bootstrap fixture repo, migrate, smoke-test, scan-cleanliness check)
├── contracts/
│   └── fixture-path-helper.md  # The new fixture_path() helper API + env var contract + cache layout contract
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already complete)
└── tasks.md             # Phase 2 output (/speckit.tasks command — NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── build.rs                          # NEW: clones fixture repo at build time + sets MIKEBOM_FIXTURES_DIR env var
├── tests/
│   ├── transitive_parity_common/
│   │   └── mod.rs                    # MODIFIED: fixture_path() resolves against MIKEBOM_FIXTURES_DIR instead of workspace_root
│   ├── common/
│   │   └── mod.rs                    # MODIFIED: same pattern for the cross-format fixture matrix
│   ├── *.rs                          # MODIFIED: ~70 integration-test files updated to use the new resolver
│   └── fixtures/
│       ├── golden/                   # STAYS — regression contract
│       ├── schemas/                  # STAYS — upstream JSON schemas
│       └── (manifest-bearing dirs)   # REMOVED — moved to mikebom-test-fixtures repo
└── src/
    └── (test modules referencing tests/fixtures/) # MODIFIED: 6 files updated to use the new resolver

tests/
└── fixtures/
    ├── binaries/                     # STAYS — no source-language manifests
    ├── bdb-rpmdb/                    # STAYS — no source-language manifests
    ├── apk/synthetic/                # STAYS — synthetic OS-package fixture
    ├── deb/synthetic/                # STAYS — synthetic OS-package fixture
    ├── rpm-files/                    # STAYS — synthetic OS-package fixture
    ├── gem-source-project/           # STAYS — no Gemfile/Gemfile.lock (just a *.gemspec)
    ├── polyglot-rpm-binary/          # STAYS — binary RPM, no source-language manifests
    ├── polyglot-five/                # STAYS — placeholder
    ├── reference/                    # STAYS — non-fixture reference data
    ├── sample-attestation.json       # STAYS — single test attestation
    └── (44 manifest-bearing dirs)    # REMOVED — moved

tests/fixtures.rev                    # NEW: single-line file with the pinned mikebom-test-fixtures Git SHA

.github/workflows/
└── ci.yml                            # MODIFIED: add a fixture-cache step using actions/cache keyed by tests/fixtures.rev

specs/083-transitive-correctness/
└── research.md                       # MODIFIED: §8 audit references update fixture paths from
                                      # "mikebom-cli/tests/fixtures/transitive_parity/<eco>" to the new repo location
```

**Structure Decision**: Existing single-project Rust workspace, plus a new sibling Git repository under the same org. The new repo (`kusari-sandbox/mikebom-test-fixtures`) seeds with the moved content; mikebom main repo loses ~17 MB of fixture content + adds the build-time fetch infrastructure.

PR diff target on the mikebom main repo:
- ~700+ deletions (manifest-bearing fixture directories).
- ~50 LOC new build.rs.
- ~30 LOC new fixture_path helper.
- ~70 test files + 6 source files: path-rewrite (mechanical, often single-line replacements like `workspace_root().join("tests/fixtures/foo")` → `fixture_path("foo")`).
- 1 new `tests/fixtures.rev` file.
- ~10 LOC `.github/workflows/ci.yml` change.
- ~20 LOC `specs/083-transitive-correctness/research.md` audit-row path updates.

The new `mikebom-test-fixtures` repo seeds with the moved content + a `README.md` documenting:
- "These are intentionally vulnerable test fixtures for mikebom. DO NOT use as a reference."
- The fixture-repo layout (mirrors mikebom's pre-090 paths).
- A pointer to mikebom main repo + the `tests/fixtures.rev` mechanism.
- An advisory list (not a CVE-acceptance list — fixtures are intentionally vulnerable; the README documents this design intent).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitution violations. Complexity tracking N/A.

The migration's complexity is in the SCOPE (44 directories + ~76 files touched), not in any architectural novelty. The build.rs fetch pattern is well-trodden in the Rust ecosystem (rust-rocksdb, libsqlite3-sys, lots of FFI crates use it).
