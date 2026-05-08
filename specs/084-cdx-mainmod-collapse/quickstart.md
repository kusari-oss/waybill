# Quickstart — milestone 084 maintainer recipes

Five maintainer-facing recipes for verifying the closure invariant, regenerating goldens, auditing the diff shape, reproducing the slack-notifier-style ingestion failure, and running the override-path verification.

## Recipe 1 — Run the new closure-invariant test

```bash
# After implementation lands:
cargo +stable test -p mikebom --test cdx_ref_closure_invariant -- --nocapture
```

Expected output: each per-ecosystem assertion logs `ecosystem <eco>: closure invariant holds (N entries in S)`; the test result line reads `test result: ok. 1 passed; 0 failed`.

If a violation is logged: stdout shows `ecosystem <eco>: <N> closure violations: [(<field-path>, <ref-value>), ...]`. Each entry names the field path and the orphan ref; the fix is to either (a) add the missing component to `components[]`, (b) ensure `metadata.component.bom-ref` matches the orphan, or (c) more likely, fix the emission code that is emitting the dangling ref.

## Recipe 2 — Regenerate affected CDX goldens

```bash
# Discover which goldens currently contain a non-PURL ref in dependencies[]:
find mikebom-cli/tests -name "cdx_*.golden.json" \
  | xargs -I{} sh -c '
      if jq -e ".dependencies[].ref | select(test(\"^pkg:\") | not)" "{}" >/dev/null 2>&1; then
        echo "AFFECTED: {}"
      fi'

# Regenerate all CDX goldens in one pass:
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression

# Verify SPDX goldens stayed byte-identical (FR-007):
cargo +stable test -p mikebom --test spdx_regression -- --nocapture
cargo +stable test -p mikebom --test spdx3_regression -- --nocapture
```

If any SPDX golden regenerates: STOP. The implementation has touched SPDX emission and is over-scoped. Narrow the fix back to `cyclonedx/builder.rs` only.

## Recipe 3 — Audit the diff shape per affected golden

For each golden flagged by Recipe 2's discovery step:

```bash
# Inspect the diff:
git diff -- mikebom-cli/tests/fixtures/cdx_<ecosystem>_*.golden.json
```

The diff MUST consist of EXACTLY:

1. ONE `dependencies[]` entry deleted, identifiable by:
   - `"ref"` value matching `<short-name>@0.0.0` (no `pkg:` prefix; basename + hardcoded `0.0.0`).
   - `"dependsOn"` of length 1 pointing at the main-module PURL.
2. TWO `compositions[]` entries with their `assemblies[0]` or `dependencies[0]` rewritten from the short-name orphan to the main-module PURL.

NO other fields change. If the diff shows changes to `metadata.component`, `components[]`, or any other top-level array, the implementation has overcorrected — narrow the fix.

Quick verification command (per-golden):

```bash
# Count diff hunks per top-level field:
git diff -- <golden-path> | grep -E "^[+-]\s+\"(metadata|components|services|annotations|vulnerabilities|formulation)\"" | sort -u
```

Empty output = good (only `dependencies` and `compositions` changed).

## Recipe 4 — Reproduce the slack-notifier-style ingestion failure pre-fix

To confirm pre-fix behavior on a real Go project:

```bash
# Clone any Go project with a non-trivial dep graph (≥30 components in go.sum):
git clone https://github.com/<org>/<repo> /tmp/scratch-go
cd /tmp/scratch-go

# Emit CDX 1.6 with mikebom alpha.23 (or current main pre-fix):
~/Projects/mikebom/target/release/mikebom sbom scan \
    --path . \
    --format cyclonedx-1-6 \
    --output /tmp/pre-fix.cdx.json

# Verify the orphan ref exists:
jq '.dependencies[] | select(.ref | test("^pkg:") | not) | {ref, dependsOn}' /tmp/pre-fix.cdx.json
# Expected output: { ref: "<short-name>@0.0.0", dependsOn: ["pkg:golang/.../<full-path>@<version>"] }

# Verify compositions reference the same orphan:
jq '.compositions[] | select((.assemblies // []) + (.dependencies // []) | any(test("^pkg:") | not))' /tmp/pre-fix.cdx.json

# Run cyclonedx-cli validate (if available) and observe ref-resolution warnings:
cyclonedx-cli validate --input-file /tmp/pre-fix.cdx.json --fail-on-errors
```

After the fix lands and you rebuild, re-run the same commands against `/tmp/post-fix.cdx.json`. The orphan-ref jq queries should return empty; `cyclonedx-cli validate` should exit 0 with no closure warnings.

## Recipe 5 — Verify the override path closure invariant

The milestone-077 `--root-component-name` override path also gets the closure-invariant treatment per research §2 Option A. To verify:

```bash
# Run mikebom with override flags against any post-053 fixture:
~/Projects/mikebom/target/release/mikebom sbom scan \
    --path mikebom-cli/tests/fixtures/<some-go-fixture>/ \
    --format cyclonedx-1-6 \
    --output /tmp/override-test.cdx.json \
    --root-component-name "test-override" \
    --root-component-version "9.9.9"

# Verify metadata.component.bom-ref carries the override identity:
jq '.metadata.component | {"bom-ref", name, version}' /tmp/override-test.cdx.json
# Expected: bom-ref = "test-override@9.9.9", name = "test-override", version = "9.9.9"

# Verify the closure invariant under override:
S=$(jq -r '[.metadata.component["bom-ref"], .components[]["bom-ref"]] | unique[]' /tmp/override-test.cdx.json | sort -u)
ALL_REFS=$(jq -r '[
    .dependencies[].ref,
    .dependencies[].dependsOn[]?,
    .compositions[].assemblies[]?,
    .compositions[].dependencies[]?
] | unique[]' /tmp/override-test.cdx.json | sort -u)
ORPHANS=$(comm -23 <(echo "$ALL_REFS") <(echo "$S"))
if [ -z "$ORPHANS" ]; then
    echo "Closure invariant holds under override."
else
    echo "Orphan refs in override path:"
    echo "$ORPHANS"
fi
```

Pre-fix expected: orphan list contains the dropped main-module PURL.
Post-fix expected: orphan list empty.

## When in doubt

- **Test fails after a fresh `git pull` from main**: run the closure-invariant test (Recipe 1); if it fails, the failing fixture's golden was regenerated without the closure invariant being enforced — file an issue and `git bisect` to find the responsible PR.
- **Goldens drift on a milestone-unrelated commit**: SHOULD NOT happen. CDX goldens regenerate only when CDX emission code changes. If they drift, root-cause the unintended emission change.
- **`cyclonedx-cli validate` reports closure errors post-fix**: the closure invariant covers `dependencies[]` and `compositions[]`. If a different field path is flagged (e.g., `services[]`, `vulnerabilities[]`, evidence chains), it's a separate latent bug — file an issue; the milestone-084 fix is scoped to the bridges that were observed.
- **Operator-level support: cyclonedx-cli not installed**: the closure-invariant test is hermetic; it doesn't need `cyclonedx-cli`. Use it directly. The cyclonedx-cli verification is supplementary, not load-bearing.
- **An override path golden has a larger diff than expected**: research §2 Option A bounds the override-path diff scope. If the diff shows `metadata.component` or `components[]` changes under override, the implementation has overcorrected — narrow back to "filter `relationships[]` only when `override_active` AND the relationship's `from` matches the dropped main-module PURL."
