# Quickstart: 670-pip-under-detection-fix

**Feature**: Fix critical Python under-detection
**Branch**: `670-pip-under-detection-fix`

## Prerequisites

- Repo checked out at branch `670-pip-under-detection-fix`
- Rust stable toolchain installed
- `git` available on PATH (for fixture fetch via m090 pattern)
- Optional (for cross-tool ground-truthing): `syft`, `cdxgen` on PATH

## Fresh-tree fixture verification (mirrors the sweep that surfaced the bug)

Clone the three failing fixtures locally and confirm the current under-detection numbers:

```sh
for repo in test-markitdown test-OctoPrint test-cpython; do
    workdir=$(mktemp -d)
    git clone --depth 1 --single-branch --no-tags \
        "https://github.com/kusari-sandbox/$repo" "$workdir" 2>/dev/null
    printf "=== %s ===\n" "$repo"
    cargo run --release -p waybill -- sbom scan \
        --path "$workdir" --offline --no-deep-hash \
        --format cyclonedx-json --output /tmp/scan-$repo.cdx.json
    jq '[.components[] | .purl // "none"] |
        group_by(if . == "none" then "no-purl"
                 else (split(":") | .[1] // "none" | split("/")[0])
                 end) |
        map({eco: .[0] | (if . == "none" then "no-purl"
                          elif type == "string" then .
                          else (split(":") | .[1] // "none" | split("/")[0])
                          end), count: length}) |
        sort_by(.eco)' /tmp/scan-$repo.cdx.json
    rm -rf "$workdir"
done
```

**Baseline (pre-milestone)**: markitdown=4 pypi, OctoPrint=3 pypi, cpython=16 pypi.
**Success target**: markitdown ≥ 30, OctoPrint ≥ 30, cpython ≥ 50.

## Local dev loop

### 1. Wire the new module

Extend `waybill-cli/src/scan_fs/package_db/pip/mod.rs` to dispatch to the new readers:

```rust
// pip/mod.rs (existing)
pub(crate) mod dist_info;

// NEW additions:
pub(crate) mod pyproject_toml;
pub(crate) mod requirements_txt;
pub(crate) mod setup_py;
pub(crate) mod setup_cfg;
pub(crate) mod uv_lock;
pub(crate) mod poetry_lock;
pub(crate) mod pdm_lock;
pub(crate) mod pipfile_lock;
pub(crate) mod venv_prune;
pub(crate) mod req_scope_heuristic;
pub(crate) mod direct_url;
```

Register each with the m664 walker registry — see `waybill-cli/src/scan_fs/walk_registry/mod.rs` for the `ReaderRegistration` pattern.

### 2. Iterate a single reader

Fastest inner loop for developing one reader in isolation:

```sh
# Focus on pyproject reader (US1)
cargo test -p waybill --lib scan_fs::package_db::pip::pyproject_toml -- --nocapture

# Add clippy check before pushing
cargo +stable clippy -p waybill --lib -- -D warnings
```

### 3. Full pre-PR gate (MANDATORY per constitution)

```sh
./scripts/pre-pr.sh
```

This runs both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace`. Both MUST pass green (per Constitution v2.1.0 §Development Workflow).

### 4. Fixture-integration test

The new `transitive_parity_python.rs` test at `waybill-cli/tests/transitive_parity_python.rs`:

```sh
# Run the fixture tests
cargo test -p waybill --test transitive_parity_python -- --nocapture

# Regenerate goldens after intentional emission-shape changes
MIKEBOM_UPDATE_GOLDENS=1 cargo test -p waybill --test transitive_parity_python
```

**Cross-host stability**: per memory `feedback_cross_host_goldens`, the test harness rewrites workspace paths, strips SHA-256 hashes to `<hash-64>`, isolates HOME, and masks serial-number + timestamp all-at-once. Verify byte-identical output on Linux + macOS before pushing.

## Sweep-regression check

Before merge, re-run the full sweep from issue #743 and confirm no non-Python fixture regressed:

```sh
# Assumes waybill-cli is built at target/release/waybill
bash /tmp/waybill-sweep.sh
# Compare component counts against sweep-2026-08-31.tsv checked into
# specs/670-pip-under-detection-fix/artifacts/ once the milestone lands
```

**Pass criteria**:
- All 21 previously-succeeding fixtures still succeed (exit 0)
- Non-Python fixtures within ± 5% component count
- Python-containing fixtures (markitdown, OctoPrint, cpython, others) increase in count
- No scan wall-clock regression > 20% on any non-Python fixture

## Cross-tool ground-truth verification (optional)

For SC-005 verification (every emitted component has evidence):

```sh
# Manual ground-truth for markitdown
uv --directory /tmp/test-markitdown pip compile pyproject.toml --output-file /tmp/expected.txt
comm -12 \
    <(grep '^[a-zA-Z]' /tmp/expected.txt | cut -d'=' -f1 | sort -u) \
    <(jq -r '.components[] | select(.purl | startswith("pkg:pypi/")) | .name' /tmp/scan-test-markitdown.cdx.json | sort -u) \
    | wc -l
# Expected: covers ≥ 80% of the uv-resolved set
```

For cross-tool comparison against syft:

```sh
syft dir:/tmp/test-markitdown --output cyclonedx-json > /tmp/syft-markitdown.cdx.json
diff \
    <(jq -r '.components[].purl // "none"' /tmp/scan-test-markitdown.cdx.json | sort -u) \
    <(jq -r '.components[].purl // "none"' /tmp/syft-markitdown.cdx.json | sort -u)
```

## Manual acceptance checks (from spec)

| Acceptance | Command | Expected |
|------------|---------|----------|
| US1 AS1 | scan markitdown; jq `.components[].purl \| test("pkg:pypi/")` count | ≥ 30 |
| US1 AS3 | scan pyproject-only fixture; jq `.components[] \| select(.version == "unresolved")` | ≥ 1 with `waybill:unresolved-reason` |
| US1 AS4 | scan pyproject-with-optional-deps; jq `.components[] \| .scope` | `"optional"` for optional-dependencies entries |
| US2 AS3 | fixture with `-r ../other.txt`; check other's deps present | present |
| US2 AS4 | unpinned entry; jq `.components[] \| .version` | `"unresolved"` + `waybill:unresolved-reason` |
| US3 AS1 | OctoPrint scan; count pypi | ≥ 30 |
| US3 AS2 | fixture with `install_requires = get_deps()` | main-module only; no fabrication |
| US3 AS3 | fixture with `setup.cfg [options]`; count pypi | > 0 |

## Troubleshooting

### Fixture cache stale

If `~/.cache/waybill/fixtures/<sha>/kusari-sandbox/test-{markitdown,OctoPrint,cpython}/` is stale:

```sh
rm -rf ~/.cache/waybill/fixtures/*/kusari-sandbox/
cargo test -p waybill --test transitive_parity_python  # re-fetches
```

### Golden diff on a different host

Symptoms: test fails on macOS but passes on Linux CI, or vice versa. See memory `feedback_cross_host_goldens`:
1. Confirm workspace-path rewrite fires
2. Confirm SHA-256 masking fires
3. Confirm HOME isolation fires
4. Confirm timestamp masking fires (m669 baseline mtime doesn't leak)

### Scan time regression

If test-cpython wall-clock exceeds 5.5 s:
1. `WAYBILL_LOG=debug cargo test ... -- --nocapture` to see per-reader timings
2. Check if `--include-python-vendored` is accidentally enabled
3. Check the `venv_prune.rs` allowlist is being consulted (miss = walking `.venv/` = catastrophic)

### Constitution deviation flagged in review

Per plan.md `Complexity Tracking`, this milestone documents divergence from Principle II and SB#1 as standard sbom-scan practice. If a reviewer questions this, point at:
1. Every `scan_fs/package_db/*` reader since m002 (200+ precedent milestones)
2. The plan.md `Complexity Tracking` table
3. Constitution rationale (Principle II covers `waybill trace`, not `waybill sbom scan`)
