# Quickstart — milestone 168 (Rust + Python monorepos audit)

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Data model**: [data-model.md](./data-model.md)

How to run the m168 audit end-to-end.

## Prerequisites

- macOS or Linux dev host
- `git`, `jq`, `python3` ≥ 3.10, `time`, POSIX shell
- Rust stable toolchain (for building post-m167 mikebom if not already built)
- Trivy 0.71.1 (or later; recorded in report). If brew tap serves stale version, direct-binary-download from `github.com/aquasecurity/trivy/releases` per m165 §Install-Friction reproduction appendix.
- Syft 1.44.0 (or later; recorded in report). `brew install syft` typically current.
- spdx3-validate 0.0.5 at `.venv/spdx3-validate/bin/spdx3-validate` (per memory `reference_spdx3_validator`; already present from milestone 078).
- ~40 GB free disk for clones + intermediate SBOMs.
- Working internet connection for clones (Tauri ~50 MB, Airflow ~200 MB).

## Step 1 — Build a post-m167 mikebom binary

From the mikebom workspace root:

```bash
cargo +stable build --release -p mikebom
export MIKEBOM_BIN="$PWD/target/release/mikebom"
"$MIKEBOM_BIN" --version   # should show a post-m167 build
git log --oneline -1        # confirm HEAD includes commit ccde910 or later
```

Expected: `mikebom 0.1.0-alpha.52` (or later; whichever alpha post-m167 corresponds to the audit's mikebom pin recorded in the report header).

## Step 2 — Clone target repos

```bash
mkdir -p specs/168-rust-python-audit/artifacts/{tauri,airflow}
cd specs/168-rust-python-audit/artifacts

git clone --depth 1 https://github.com/tauri-apps/tauri.git tauri-src
git -C tauri-src rev-parse HEAD   # record this SHA in report header

git clone --depth 1 https://github.com/apache/airflow.git airflow-src
git -C airflow-src rev-parse HEAD # record this SHA in report header
```

## Step 3 — Run mikebom on both targets, all 3 formats

Per FR-001 + FR-002 + clarifications Q1/Q2 (repo root scope for both, no exclusions):

```bash
for target in tauri airflow; do
    src="specs/168-rust-python-audit/artifacts/${target}-src"
    out="specs/168-rust-python-audit/artifacts/${target}"

    for fmt in cyclonedx-json spdx-2.3-json spdx-3-json; do
        ext=${fmt%%-*}   # cyclonedx / spdx / spdx  (approximate; report uses full names)
        time "$MIKEBOM_BIN" --offline sbom scan \
            --path "$src" \
            --format "$fmt" \
            --output "$out/mikebom.${fmt}.json" \
            --no-deep-hash 2>&1 | tee "$out/mikebom.${fmt}.log"
    done
done
```

Wall-clock times captured via `time` (recorded in report per FR-003).

## Step 4 — Run Trivy + Syft on both targets (CDX only — external tools' SPDX support is patchy per m165 R2)

```bash
for target in tauri airflow; do
    src="specs/168-rust-python-audit/artifacts/${target}-src"
    out="specs/168-rust-python-audit/artifacts/${target}"

    time trivy fs --format cyclonedx --output "$out/trivy.cdx.json" "$src" 2>&1 | tee "$out/trivy.log"
    time syft "$src" -o cyclonedx-json="$out/syft.cdx.json" 2>&1 | tee "$out/syft.log"
done
```

## Step 5 — SPDX validation on mikebom output only

```bash
for target in tauri airflow; do
    out="specs/168-rust-python-audit/artifacts/${target}"

    # SPDX 2.3 — existing jsonschema gate at mikebom-cli/tests/fixtures/schemas/
    python3 -c "import json, jsonschema; \
        schema = json.load(open('mikebom-cli/tests/fixtures/schemas/spdx-2.3.schema.json')); \
        doc = json.load(open('$out/mikebom.spdx-2.3-json.json')); \
        jsonschema.validate(doc, schema); print('SPDX 2.3 PASS')" \
        || echo "SPDX 2.3 FAIL"

    # SPDX 3.0.1 — memory reference_spdx3_validator
    .venv/spdx3-validate/bin/spdx3-validate --json "$out/mikebom.spdx-3-json.json" --quiet \
        && echo "SPDX 3 PASS" || echo "SPDX 3 FAIL"
done
```

## Step 6 — Run analyze.py (reused from m165 per research §R5)

```bash
cp specs/165-k8s-argocd-audit/scripts/analyze.py specs/168-rust-python-audit/scripts/analyze.py

for target in tauri airflow; do
    python3 specs/168-rust-python-audit/scripts/analyze.py \
        --target-dir "specs/168-rust-python-audit/artifacts/${target}" \
        --target-name "$target" \
        --output "specs/168-rust-python-audit/artifacts/${target}/analysis.json"
done
```

The `analysis.json` files (per E7 in data-model.md) contain per-tool metrics, orphan classifications, and tool comparison delta.

## Step 7 — Author the report

Using the `analysis.json` files + qualitative observations from log inspection, author `docs/audits/2026-07-06-tauri-airflow.md` per data-model.md E1 structure. Sections in order:

1. Header (SHAs, versions)
2. Per-Target Tauri
3. Per-Target Airflow
4. Recommended Follow-On Milestones (with m167 Vocab Applicability sub-section)
5. Cross-Round Trend Analysis (with freshness caveats per Q3)
6. Backlog Observations
7. Executive Summary
8. Reproduction Appendix

## Step 8 — Verify SC-007 + SC-008 (pre-PR gate + golden byte-identity)

```bash
./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.` Zero clippy warnings, all workspace tests pass. Since m168 touches only `docs/`, `specs/168-rust-python-audit/`, and `scripts/` (analyzer copy), no production code paths are affected and all goldens must be byte-identical to pre-m168.

## Rollback / bail-out path

Milestone 168 is documentation-only. If review surfaces objections:

1. Revert the merge commit — restores pre-168 state. No wire-format impact, no consumer-visible change.
2. Intermediate artifacts under `specs/168-rust-python-audit/artifacts/` are gitignored — reverting has no filesystem cleanup burden.
3. The `docs/audits/2026-07-06-tauri-airflow.md` file is the ONLY user-visible artifact; reverting removes it.
