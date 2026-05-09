# Contract — milestone 090 fixture-path helper + build.rs + cache + CI gate

The milestone's contracts: (1) the new fixture-path helper API, (2) the build.rs fetch contract, (3) the cache layout contract, (4) the `tests/fixtures.rev` pin contract, (5) the CI cache integration contract.

## CLI surface

**No new operator-facing CLI flags.** This is a test-infrastructure refactor. `mikebom sbom scan`, `mikebom attestation sign|verify`, etc. keep their existing flag sets.

## Library surface (`mikebom-cli` crate)

**No new public Rust API in the production code.** All changes are scoped to:
- `mikebom-cli/build.rs` (NEW): build-time fixture clone.
- `mikebom-cli/tests/common/fixtures.rs` or equivalent (NEW): the `fixture_path` helper.
- `mikebom-cli/tests/transitive_parity_common/mod.rs` (MODIFIED): delegate `fixture_path(subpath)` to the new common helper.
- `mikebom-cli/tests/*.rs` (MODIFIED): mechanical path rewrites at ~70 call sites.
- `mikebom-cli/src/scan_fs/*` test modules (MODIFIED): same pattern at ~6 call sites.

## `fixture_path` helper API contract

```rust
/// Resolves a fixture-relative path against the MIKEBOM_FIXTURES_DIR
/// env var set by build.rs. The fixture cache layout mirrors the
/// pre-090 mikebom main repo layout exactly, so existing relative
/// paths like "transitive_parity/cargo" or "cargo/lockfile-v3"
/// resolve without rewriting the relative portion.
///
/// # Panics
///
/// Does not panic at runtime; `env!("MIKEBOM_FIXTURES_DIR")` is a
/// compile-time check that fails to compile if the env var is unset
/// (catches build.rs misconfiguration at compile time).
pub fn fixture_path(rel: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("MIKEBOM_FIXTURES_DIR")).join(rel)
}
```

**Caller contract**:
- `rel` MUST be a relative path; absolute paths are silently absolutized against the cache (likely a bug — caller responsibility).
- Caller MUST NOT mutate the returned path's contents (the fixture cache is shared across builds; mutations bleed to subsequent runs).

This contract is enforced by VR-090-013 + VR-090-014.

## build.rs fetch contract

`mikebom-cli/build.rs` MUST:

1. Read `tests/fixtures.rev` (path resolved relative to `CARGO_MANIFEST_DIR/..`, since build.rs's CWD is the crate dir).
2. Resolve the cache parent: `$MIKEBOM_FIXTURE_CACHE` if set, else `$HOME/.cache/mikebom/fixtures`.
3. Compute the cache target: `<cache_parent>/<sha>/`.
4. **Cache hit**: if `<cache_parent>/<sha>/` exists and contains ≥1 file, emit `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<absolute-path>` and exit.
5. **Cache miss**: shell out to `git clone --depth 1 https://github.com/kusari-sandbox/mikebom-test-fixtures.git <cache_parent>/<sha>/`, followed by `git -C <cache_parent>/<sha> fetch origin <sha>` and `git -C <cache_parent>/<sha> reset --hard <sha>` to pin to the exact SHA.
6. **Fetch failure**: panic with a structured message:
   ```
   Failed to fetch mikebom-test-fixtures revision <sha>:
       URL:   https://github.com/kusari-sandbox/mikebom-test-fixtures.git
       Cache: <cache_parent>/<sha>/
       Cause: <git stderr>

   Workaround:
       1. Verify network access to github.com.
       2. Manually clone: git clone https://github.com/kusari-sandbox/mikebom-test-fixtures.git <cache_parent>/<sha>/
                          git -C <cache_parent>/<sha> reset --hard <sha>
       3. Re-run cargo build.
   ```
7. After successful fetch, emit `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<absolute-path>`.
8. Emit `cargo:rerun-if-changed=../tests/fixtures.rev` so build.rs re-runs only when the pin changes.

This contract is enforced by VR-090-006 + VR-090-008 + VR-090-009 + VR-090-010 + VR-090-011 + FR-007.

## Cache layout contract

```
$HOME/.cache/mikebom/fixtures/
├── <sha-A>/
│   ├── README.md
│   ├── transitive_parity/
│   │   ├── cargo/
│   │   ├── ...
│   ├── cargo/
│   │   ├── lockfile-v3/
│   │   └── ...
│   └── ... (mirrors mikebom-test-fixtures repo layout)
├── <sha-B>/                # different pin (e.g., older mikebom commit during git-bisect)
│   └── ...
└── <sha-C>/
```

`<cache>/<sha>/` IS the fixture-repo root; no nested `mikebom-test-fixtures/` subdirectory.

This contract is enforced by VR-090-008.

## `tests/fixtures.rev` pin contract

Single-line text file at the mikebom main repo root:

```text
<40-char-lowercase-hex-sha>\n
```

- One trailing newline.
- No leading whitespace, no trailing comment, no other lines.
- The SHA MUST resolve to a commit in the `mikebom-test-fixtures` repo.
- Pin bumps are 1-line PR diffs.

This contract is enforced by VR-090-004 + VR-090-005 + VR-090-006.

## CI cache integration contract

`.github/workflows/ci.yml` MUST add a step BEFORE `Tests` (and ideally before `Clippy` since clippy runs with `--all-targets` which compiles tests):

```yaml
- name: Cache fixture repo
  uses: actions/cache@v4
  with:
    path: ~/.cache/mikebom/fixtures
    key: mikebom-fixtures-${{ hashFiles('tests/fixtures.rev') }}
```

Applies to all 3 lanes: `lint-and-test` (Linux), `lint-and-test-macos`, `lint-and-test-ebpf`.

This contract is enforced by VR-090-019 + VR-090-020 + FR-010.

## Per-format scope contract

| Format | Affected? | Verification |
|---|---|---|
| **CDX 1.6** (all 9 ecosystems) | NO — fixture content unchanged; only its location changes | All 9 cdx goldens byte-identical |
| **SPDX 2.3** (all 9 ecosystems) | NO — same | All 9 spdx goldens byte-identical |
| **SPDX 3** (all 9 ecosystems) | NO — same | All 9 spdx3 goldens byte-identical |

This is a test-infra refactor. ZERO golden regenerations. If goldens regenerate, scope has crept.

## Test invocation contract

```bash
# Confirm fixtures fetch + tests pass on a fresh clone:
git clone https://github.com/kusari-sandbox/mikebom.git mikebom-fresh
cd mikebom-fresh
cargo +stable test --workspace
# Expected: build.rs fetches mikebom-test-fixtures into ~/.cache/mikebom/fixtures/<sha>/
#           every test suite reports `0 failed`.

# Confirm cache-warm path skips network:
# (after the above) disconnect network, then:
cargo +stable test --workspace
# Expected: zero network calls; tests pass.

# Confirm post-migration trivy scan is clean:
cd mikebom-fresh
trivy --quiet fs --scanners vuln --skip-dirs target --format json --output /tmp/post-090.json .
jq '[.Results[]?.Vulnerabilities[]?] | length' /tmp/post-090.json
# Expected: only the 4 milestone-089 known-acceptances (rustls-webpki residuals).
# NOT 38+ fixture-vuln noise.

# Confirm goldens unchanged:
git status --short mikebom-cli/tests/fixtures/golden/
# Expected: empty output.

# Confirm pin file format:
cat tests/fixtures.rev | head -1 | grep -E "^[0-9a-f]{40}$"
# Expected: matches.

# Standard pre-PR gate:
./scripts/pre-pr.sh
# Expected: zero clippy warnings; every test suite `0 failed`.
```

## Performance contract

- First-fetch wall-time: ≤30 s on standard developer hardware over residential broadband (SC-003).
- Cache-warm subsequent builds: ≤100 ms cache-existence check + zero network (SC-004).
- Total `./scripts/pre-pr.sh` wall-time post-090: same as pre-090 (the cache-existence check is sub-100ms; first-fetch happens once per pin update).
- CI cache hit rate: ≥99% across same-pin runs (only fixture-pin bumps invalidate).

## Backward-compatibility contract

- Operators of `mikebom sbom scan`, `mikebom attestation sign|verify` see ZERO behavior change. SBOM emission code paths unchanged.
- Existing test files using `workspace_root().join("tests/fixtures/...")` for the move-set paths MUST be updated (FR-009). Tests that use `workspace_root()` for goldens / schemas / binary fixtures / OS-package synthetic / `reference/` / `polyglot-rpm-binary/` / `gem-source-project/` / `polyglot-five/` / `sample-attestation.json` see NO change.
- The `tests/fixtures.rev` file is a NEW required file in the mikebom main repo. Cloning a pre-090 mikebom commit (e.g., for git-bisect) won't have this file and build.rs will need a graceful fallback. **Decision**: build.rs SHOULD detect the missing-pin-file case and emit a structured error pointing to "this commit predates the milestone-090 fixture split; checkout post-090 OR regenerate fixtures from `<old-path>` in this commit's tree". Plan-level — minor implementation detail.
