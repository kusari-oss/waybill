# Research — milestone 090 fixture-repo split

Phase 0 investigation against the spec's open questions. Six decision points; all resolved without further clarification.

## §1 — Fetch mechanism

**Decision**: `build.rs` in `mikebom-cli/` shells out to the system `git` binary at build time, clones the pinned revision into `~/.cache/mikebom/fixtures/<sha>/` (overridable via `MIKEBOM_FIXTURE_CACHE` env var), and emits a `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<path>` line so the integration-test target gets the path baked in via `env!("MIKEBOM_FIXTURES_DIR")`.

**Rationale**:
- US2 acceptance scenario 1 ("test setup automatically fetches the fixture repo") rules out manual setup-script-only approaches; build.rs is the most automatic.
- Cache-warm subsequent builds skip the network entirely (the build.rs first checks `<cache>/<sha>/` and exits early if present). Satisfies US4 + FR-006 (zero network on warm path).
- Cache-key is the pinned SHA — multiple revisions co-exist (useful during git-bisect through mikebom history). Satisfies US3.
- Shell-out to `git` (zero new C deps, Constitution Principle I compliant). The `git` binary is already a hard prereq for any mikebom dev setup.
- build.rs runs on every `cargo build` / `cargo test`, but the cache check is sub-100 ms when populated. First-fetch is one-time per pin update.
- Errors surface as build.rs panic with a structured message (URL + cache path + workaround). FR-007 / Constitution III alignment.

**Alternatives considered**:
- **Git submodule**: rejected because the submodule's worktree appears under `mikebom-cli/tests/fixtures/external/` (or wherever it's gitlinked), and security tooling that clones with `--recurse-submodules` (the GitHub default) re-introduces the trigger surface that this milestone is trying to remove. Defeats US1.
- **`git2 = "0.19"` Rust crate**: rejected because `libgit2-sys` is a C dep, violating Constitution Principle I. Shell-out to system `git` has zero new C deps.
- **Tarball download from a tagged release**: rejected because pin-bumping requires cutting a release tag in the fixture repo for every fixture update — worse pin-bump UX than a SHA pin. Also harder to git-bisect through fixture history.
- **Separate `./scripts/setup-fixtures.sh` script that the user runs explicitly**: rejected because US2 scenario 1 requires automatic setup; the explicit-script approach adds friction for first-time contributors. (We CAN ALSO ship the script as a manual-fallback for the FR-007 error message — see §3.)

## §2 — Revision pin mechanism

**Decision**: a single-line file at `tests/fixtures.rev` (mikebom main repo root, NOT inside a crate) containing the pinned Git SHA (40-char hex) followed by a newline. build.rs reads the file at build time. PR review sees a 1-line diff when bumping the pin.

**Rationale**:
- Single source of truth for "which fixture revision is mikebom commit X tested against" — supports US3's reproducibility invariant.
- `tests/fixtures.rev` is at the repo root so it's discoverable; the path mirrors the convention `tests/` already establishes for test infrastructure.
- A single-line text file diffs cleanly in PRs (vs a YAML or TOML structure where formatting noise can obscure the actual change).
- Storing the SHA (not a tag) avoids a layer of indirection — the fixture repo's tags can move, but a SHA is immutable.

**Alternatives considered**:
- **Cargo.toml `[package.metadata]` entry**: rejected because the build.rs would need to parse Cargo.toml at build time (chicken-and-egg with cargo's own build system). Also clutters the manifest with non-Cargo metadata.
- **Build-script constant `const FIXTURE_REV: &str = "..."` in build.rs itself**: rejected because every pin-bump becomes a code change in build.rs — harder to grep/audit "what fixture rev was each mikebom release built against". The `tests/fixtures.rev` file is the canonical record.
- **Git submodule's tracked SHA** (Option A from §1): rejected with §1's Option A.

## §3 — Cache invalidation

**Decision**: the cache-key IS the pinned SHA. When `tests/fixtures.rev` changes, the build.rs cache lookup misses (`<cache>/<old-sha>/` is not the same path as `<cache>/<new-sha>/`), and a new clone happens. No explicit invalidation logic needed — Git's content-addressed pinning gives us natural cache-keying.

**Rationale**:
- Aligns with the established Rust pattern of content-addressed caches (`~/.cargo/registry/src/<hash>`, etc.).
- Old caches accumulate but don't break anything; a `mikebom-cli/build.rs --clean` mode can be added in a follow-up if disk usage becomes a concern (it won't — fixture repo is ~17 MB; 100 historical revs = 1.7 GB worst case, still acceptable for a dev cache).
- Stale-cache risk (edge case 2 in spec) is structurally impossible: if the SHA in `tests/fixtures.rev` matches the cache directory's contents, the cache IS up to date. No drift possible.

**Alternatives considered**:
- **Single cache directory at `~/.cache/mikebom/fixtures/` + a "current sha" marker**: rejected because git-bisecting through mikebom history would force re-clones at every commit boundary. Multi-rev cache supports bisect cheaply.
- **Verify cache integrity via `git status --porcelain` against the cache**: rejected as overkill — if a developer manually edits the cache directory, it's their problem; the cache is in `~/.cache/`, a clearly-cache-typed location.

## §4 — What stays vs what moves

**Decision**: 44 manifest-bearing directories move; the rest stay. Concrete inventory captured at plan time:

**Move set** (manifest-bearing — the trigger surface):
- `mikebom-cli/tests/fixtures/`: `cargo-workspace/`, `maven-multi-module-reactor/`, `npm-scoped-package/`, `npm-workspace/`, `pip-pyproject-pep621/`, `pip-pyproject-poetry-only/`, `transitive_parity/{cargo, gem, go, maven, npm, pip_plain, pip_poetry}/` (= 13 dirs).
- `tests/fixtures/`: `cargo/{lockfile-v1-refused, lockfile-v2-refused, lockfile-v3, lockfile-v4}/`, `gem/simple-bundle/`, `go/argo-style-no-cache/`, `go/simple-module/`, `maven/{pom-three-deps, pom-with-property-ref}/`, `npm/{lockfile-v1-refused, lockfile-v3, lockfile-v3-transitive, node-modules-walk, package-json-only, pnpm-v8, scoped-package}/`, `polyglot-monorepo/`, `python/{pipfile-project, poetry-project, pyproject-only, requirements-only, simple-venv}/` (= 31 dirs).

**Stay set** (no source-language manifests, not the trigger surface):
- `mikebom-cli/tests/fixtures/golden/` — REGRESSION CONTRACT (FR-008).
- `mikebom-cli/tests/fixtures/schemas/` — upstream CycloneDX/SPDX JSON schemas (FR-008).
- `tests/fixtures/binaries/{elf, macho, pe}/` — opaque binary fixtures, no scanner trigger.
- `tests/fixtures/bdb-rpmdb/` — binary RPM database, no source-language manifest.
- `tests/fixtures/apk/synthetic/`, `tests/fixtures/deb/synthetic/`, `tests/fixtures/rpm-files/` — synthetic OS-package fixtures (no source-language manifests; OS-package metadata is in package-control format, not what SBOM scanners typically trigger on).
- `tests/fixtures/gem-source-project/` — has a `*.gemspec` but no `Gemfile.lock` (so unlikely to register as a "vulnerable Ruby project" to vuln scanners).
- `tests/fixtures/polyglot-rpm-binary/` — binary RPM, no source-language manifest.
- `tests/fixtures/polyglot-five/` — placeholder (just a `README.md`).
- `tests/fixtures/reference/` — non-fixture reference data.
- `tests/fixtures/sample-attestation.json` — single in-toto attestation file (not a project).
- `tests/fixtures/go/binaries/` — opaque Go binary fixtures.

**Rationale**:
- The user's request explicitly framed scope as "tests that include manifest files" — the move set strictly corresponds to that criterion.
- Goldens MUST stay (FR-008) — they're EXPECTED-OUTPUT regression-contract artifacts, not test inputs.
- Moving binary fixtures + OS-package synthetic fixtures has marginal scanner-trigger benefit (these don't typically register as SBOM-scannable projects) at the cost of slowing dev iteration on binary-related tests (extra clone wait time on first-build). Trade-off favors keeping them in main repo.

**Alternatives considered**:
- **Move everything (including binaries + OS-package synthetic)**: maximizes main-repo size shrinkage but adds first-fetch latency for tests that don't otherwise need network. Rejected — the marginal SC-005 (size shrinkage) gain isn't worth the dev-ergonomics cost.
- **Move only the milestone-083 audit fixtures (transitive_parity/*)**: too narrow — the polyglot-monorepo + the cargo-workspace fixtures + various other manifest-bearing dirs ALSO trigger scanners. Half-measure; doesn't fully solve US1.

## §5 — Path-resolver API

**Decision**: introduce a new helper `mikebom-cli/tests/common/fixtures.rs::fixture_path(rel: &str) -> PathBuf` that:

```rust
/// Resolves a fixture-relative path against the MIKEBOM_FIXTURES_DIR
/// env var set by build.rs. The fixture cache layout mirrors the
/// pre-090 directory structure exactly, so existing relative paths
/// like "transitive_parity/cargo" or "cargo/lockfile-v3" resolve
/// without rewriting the relative portion.
pub fn fixture_path(rel: &str) -> std::path::PathBuf {
    let base = env!("MIKEBOM_FIXTURES_DIR");
    std::path::PathBuf::from(base).join(rel)
}
```

Existing test code transforms from:

```rust
let fixture = workspace_root().join("tests/fixtures/cargo/lockfile-v3");
// or
let fixture = workspace_root().join("mikebom-cli/tests/fixtures/transitive_parity/cargo");
```

to:

```rust
let fixture = fixture_path("cargo/lockfile-v3");
// or
let fixture = fixture_path("transitive_parity/cargo");
```

**Cache layout convention**: the fixture repo's internal directory structure flattens the pre-090 split between `tests/fixtures/` and `mikebom-cli/tests/fixtures/`. Both subtrees merge under the new repo's root. So `fixture_path("transitive_parity/cargo")` resolves the same relative path regardless of where the fixture lived in the pre-090 main-repo layout.

**Rationale**:
- Single function with a clear contract — no per-test boilerplate.
- The cache-layout-mirrors-source convention means the migration's path-rewrite work is mechanical (often a single-line `sed`-friendly replacement).
- `env!("MIKEBOM_FIXTURES_DIR")` is a compile-time check — the helper FAILS TO COMPILE if build.rs didn't set the env var. Catches misconfiguration at compile time, not test runtime.
- The existing `transitive_parity_common::fixture_path(subpath: &str)` helper at `mikebom-cli/tests/transitive_parity_common/mod.rs:50` is updated in-place to delegate to the new common helper — minimizes API churn.

**Alternatives considered**:
- **Runtime `MIKEBOM_FIXTURES_DIR` env-var lookup via `std::env::var()`**: rejected because runtime lookup loses the compile-time misconfiguration check; tests would fail at runtime instead of build time.
- **Per-fixture-type helpers (`cargo_fixture()`, `npm_fixture()`, etc.)**: rejected as over-engineering — `fixture_path("cargo/lockfile-v3")` with a single helper is just as clear, with less API surface.

## §6 — CI integration

**Decision**: `.github/workflows/ci.yml` gets a new step before `Tests` that uses `actions/cache@v4` to persist `~/.cache/mikebom/fixtures/<sha>/` across CI runs, keyed by the SHA in `tests/fixtures.rev`. Cache miss = clone happens once; cache hit = clone skipped.

```yaml
- name: Cache fixture repo
  uses: actions/cache@v4
  with:
    path: ~/.cache/mikebom/fixtures
    key: mikebom-fixtures-${{ hashFiles('tests/fixtures.rev') }}
```

This step runs on all 3 CI lanes (Linux, macOS, eBPF). Cache survives across CI runs at the SHA level, so a fixture-pin-bump invalidates the cache cleanly.

**Rationale**:
- Aligns with the existing `Swatinem/rust-cache@v2` pattern already in CI for cargo build artifacts.
- `actions/cache` is a GitHub Actions standard; no new tooling.
- Hashing `tests/fixtures.rev` for the cache key ensures the cache is invalidated on pin bump.

**Alternatives considered**:
- **Skip CI cache; clone every run**: rejected because each CI run pays the ≤30 s first-fetch hit, multiplied across the 3 lanes × multiple PR pushes per day = wasted minutes. Cache-keyed-by-SHA is cheap.
- **Vendor the fixtures into the CI runner image**: rejected because it couples mikebom's CI to a custom runner image, which is overkill and harder for new contributors to reproduce locally.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (separate repo with manifest fixtures) | §4 → 44 directories enumerated; the new repo seeds with this exact set. |
| FR-002 (deletions visible in git status) | §4 + standard `git rm` pattern. |
| FR-003 (test runner resolves against cache) | §1 + §5 → build.rs sets `MIKEBOM_FIXTURES_DIR`; helper resolves against it. |
| FR-004 (revision pin in main repo) | §2 → `tests/fixtures.rev` single-line file. |
| FR-005 (≤30s first-fetch) | §1 → ~17 MB clone over HTTPS Git completes in <30s on broadband. Verified by smoke test during implementation. |
| FR-006 (zero network on warm path) | §1 + §3 → cache-key by SHA; build.rs early-exits on cache hit. |
| FR-007 (actionable error on fetch failure) | §1 → build.rs panics with a structured message naming URL + cache path + workaround command. |
| FR-008 (goldens + schemas stay) | §4 → explicit stay-set listed. |
| FR-009 (zero test deletions/skips) | §5 → mechanical path rewrite, no test logic changes. |
| FR-010 (CI lanes work without manual setup) | §6 → `actions/cache@v4` keyed by SHA. |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
