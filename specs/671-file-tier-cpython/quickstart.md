# Quickstart: 671-file-tier-cpython

**Feature**: File-tier surfacing for source-heavy trees (SC-003 follow-up)
**Branch**: `671-file-tier-cpython`

## Prerequisites

- Repo checked out on branch `671-file-tier-cpython` (checked out post-`speckit.specify`)
- Rust stable toolchain
- `git` on PATH
- Optional: `syft` on PATH for cross-tool spot-checks

## Fresh-tree diagnostic (reproducing the m670 SC-003 gap)

```sh
workdir=$(mktemp -d)
git clone --depth 1 --single-branch --no-tags \
    https://github.com/kusari-sandbox/test-cpython "$workdir"
# Current (v0.5.0) behavior:
cargo run --release -p waybill -- sbom scan \
    --path "$workdir" --offline --no-deep-hash \
    --format cyclonedx-json --output /tmp/cpython-default.cdx.json 2>&1 | grep file_tier
jq '.components | length, [.components[] | select(.type == "file")] | length' \
    /tmp/cpython-default.cdx.json
```

Expected on v0.5.0:
- `file_tier_components=58 mode=Orphan shape_skipped=5890`
- Total components: 187 | file-tier components: 58

## Post-m671 verification (target state)

After implementation lands:

```sh
# New mode — all 21 source-shape extensions:
cargo run --release -p waybill -- sbom scan \
    --path "$workdir" --offline --no-deep-hash \
    --file-inventory=source-tree \
    --format cyclonedx-json --output /tmp/cpython-source.cdx.json 2>&1 | grep file_tier

# Restrict to Python only:
cargo run --release -p waybill -- sbom scan \
    --path "$workdir" --offline --no-deep-hash \
    --file-inventory=source-tree \
    --file-inventory-source-shapes=py \
    --format cyclonedx-json --output /tmp/cpython-py-only.cdx.json 2>&1 | grep file_tier

# Fail-loud on unknown extension:
cargo run --release -p waybill -- sbom scan \
    --path "$workdir" --offline --no-deep-hash \
    --file-inventory=source-tree \
    --file-inventory-source-shapes=md,toml   # rejected extensions
# Expected: exit 2, diagnostic listing FR-002 allowlist
```

Expected post-m671:
- Full `source-tree` mode: ≥ 100 file-tier components (SC-001) — realistic ~3000 covering `.py`+`.c`+`.h`
- `.py`-only: ~2000 file-tier components (SC-006)
- Doc-scope `waybill:file-inventory-source-shapes-active` annotation present:

  ```json
  {"mode": "source-tree", "restriction": ["py"]}
  ```

## Backward-compat regression (SC-003 + SC-004)

```sh
# 6 golden test suites must pass WITHOUT regeneration:
cargo +stable test --workspace --no-fail-fast \
    --test cdx_regression \
    --test spdx_regression \
    --test spdx3_regression \
    --test pkg_alias_binding_us1 \
    --test oci_pull_backward_compat \
    --test optional_dep_classification

# 21-fixture sweep — default mode, no new flag:
bash /tmp/waybill-sweep.sh
# Component counts must stay within ± 1% of the v0.5.0 baseline
# (see specs/670-pip-under-detection-fix/artifacts/sweep-after-2026-09-01.tsv for baseline)
```

## Local dev loop

### 1. Iterate on the classifier

```sh
cargo +stable test -p waybill --lib \
    scan_fs::file_tier::content_shape \
    scan_fs::file_tier::source_shape \
    -- --nocapture
```

### 2. Iterate on integration behavior

```sh
cargo +stable test --test scan_file_tier_source_tree_m671 -- --nocapture
```

### 3. Full pre-PR gate (per Constitution §Development Workflow)

```sh
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Per memory `feedback_prepr_gate_bails_on_first_failure`, use `--no-fail-fast` if iterating on failures.

## Parity-catalog verification

C156 lands in `docs/reference/sbom-format-mapping.md` between C155 and existing rows. Enforce bidirectionality:

```sh
cargo +stable test --lib parity::extractors::tests::every_catalog_row_has_an_extractor
cargo +stable test --test holistic_parity
```

## Sweep-regression check post-implementation

```sh
# Regenerate the sweep tsv against the post-m671 binary
bash /tmp/waybill-sweep.sh > /tmp/sweep-after-m671.log 2>&1
bash /tmp/sweep-compare.sh \
    specs/670-pip-under-detection-fix/artifacts/sweep-after-2026-09-01.tsv \
    /tmp/waybill-sweep-results.tsv
# Must show "PASS: no regressions" with ± 1% envelope on all 21 repos
```

## Troubleshooting

### Default-mode count drift

If the sweep shows non-zero delta on default-mode fixtures:
- `git diff main -- waybill-cli/src/scan_fs/file_tier/content_shape.rs` — the mode-gated bypass should be ADDITIVE only. Verify no lines in the default path were changed.
- Re-run the 6 golden suites — if they fail, the drift is at the CDX/SPDX emission layer, not the walker. Regen goldens only after confirming the drift is intentional.

### Fail-loud not firing on unknown extension

- Check that `clap` `value_parser` is wired to `source_shape::parse_restriction`
- Check that the cross-arg validation runs BEFORE walker invocation (in `scan_cmd.rs`)
- Confirm `SourceShapeParseError::UnknownExtension` implements `Display` via `thiserror`

### C156 annotation missing under `source-tree` mode

- Check `run_shared_walker_pilot` (or wherever the mode is threaded through) — is the `FileInventoryMode::SourceTree` variant matched at emission time?
- Verify the emitter reaches into `metadata.properties[]` (CDX) / doc-scope annotation (SPDX) unconditionally when mode is `SourceTree`

## Constitution deviation notes

Same as m670 Complexity Tracking: Principle II eBPF-Only Observation is inherited-divergent for the `sbom scan` command family. No new divergences from this milestone.

## References

- `spec.md` — user stories, FRs, SCs
- `plan.md` — Technical Context, Constitution Check
- `research.md` — 10 research decisions with rationale
- `data-model.md` — `SourceShape`, `FileInventoryMode::SourceTree`, C156 shape
- `contracts/` — CLI flag surfaces
- Prior art: m133 (file-tier + `--file-inventory`), m665 (`waybill:binary-scan-suppressed` doc-scope), m670 T012+T016 (JSON-object annotation values, parity extractors)
