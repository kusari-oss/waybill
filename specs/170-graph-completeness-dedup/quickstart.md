# Quickstart: m170 Manual Verification

**Feature**: 170-graph-completeness-dedup
**Date**: 2026-07-06

Three-step manual verification for post-m170 correctness. Each step is executable without external test infrastructure — good for reviewer sanity-checks and post-merge smoke tests.

## Path A — Reproduce the pre-m170 duplicate (before merging)

Use the local Go-ecosystem golden as the reproduction target. The golden `mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json` is the emitted output of running mikebom against a real Go fixture at scan time.

```bash
cd /path/to/mikebom
# Confirm two duplicate mikebom:graph-completeness entries in the pre-m170 golden.
jq '[.metadata.properties[] | select(.name == "mikebom:graph-completeness")] | length' \
    mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json
# Expected on main (pre-m170): 2
# Expected on 170-graph-completeness-dedup (post-m170): 1
```

## Path B — Verify single emission on a fresh Go scan (after applying m170)

Regenerate a golden or emit fresh SBOM against a Go fixture:

```bash
cd /path/to/mikebom
# Use any Go fixture from the milestone-090 sibling repo; the transitive-parity
# case is a compact, well-formed target that exercises the C104 emission
# without needing network access.
FIXTURE="$(cat mikebom-cli/tests/fixtures/.fixture-repo-sha | xargs -I{} echo ~/.cache/mikebom/fixtures/{}/transitive_parity/go)"

# Emit CDX with the m170 branch checked out.
mikebom --offline sbom scan --path "$FIXTURE" \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/mikebom-m170-verify.cdx.json \
    --no-deep-hash

# Verify single emission.
jq '[.metadata.properties[] | select(.name == "mikebom:graph-completeness")] | length' \
    /tmp/mikebom-m170-verify.cdx.json
# Expected: 1

# Repeat for SPDX 2.3.
mikebom --offline sbom scan --path "$FIXTURE" \
    --format spdx-2.3-json \
    --output spdx-2.3-json=/tmp/mikebom-m170-verify.spdx.json \
    --no-deep-hash
jq '[.annotations[]? | .comment | fromjson? | select(.field == "mikebom:graph-completeness")] | length' \
    /tmp/mikebom-m170-verify.spdx.json
# Expected: 1

# Repeat for SPDX 3.0.1.
mikebom --offline sbom scan --path "$FIXTURE" \
    --format spdx-3-json \
    --output spdx-3-json=/tmp/mikebom-m170-verify.spdx3.json \
    --no-deep-hash
jq '[.["@graph"][]? | select(.type == "Annotation" and (.statement | fromjson? | select(.field == "mikebom:graph-completeness")))] | length' \
    /tmp/mikebom-m170-verify.spdx3.json
# Expected: 1
```

Post-m170: every format returns `1`. Pre-m170 (on main), the same commands returned `2` on the CDX side and (likely) `2` on the SPDX sides — verified by inspecting the corresponding sibling-repo goldens on main.

## Path C — Verify the parity-extractor integrity gate fails on a synthesized collision

The new `extractors_have_unique_labels` test defends against future regressions. Manually exercise it:

```bash
cd /path/to/mikebom
# Baseline: test passes on the m170 branch.
cargo test -p mikebom --lib 'parity::extractors::tests::extractors_have_unique_labels' 2>&1 | tail -5
# Expected: test result: ok. 1 passed; 0 failed
```

To synthesize a collision (do NOT commit this — for local demonstration only):

```rust
// In mikebom-cli/src/parity/extractors/mod.rs, TEMPORARILY add a duplicate:
ParityExtractor {
    row_id: "C999",
    label: "mikebom:graph-completeness",  // DUPLICATE of C104
    cdx: c104_cdx, spdx23: c104_spdx23, spdx3: c104_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

Then:

```bash
cargo test -p mikebom --lib 'parity::extractors::tests::extractors_have_unique_labels' 2>&1 | tail -8
# Expected: test result: FAILED with a message like:
#   panicked at ...: duplicate label "mikebom:graph-completeness" in rows: ["C104", "C999"]
```

Revert the change; verify the test passes again.

## Path D — Verify pre-PR gate stays green

```bash
cd /path/to/mikebom
./scripts/pre-pr.sh
# Expected: >>> all pre-PR checks passed.
```

This exercises:
- `cargo +stable clippy --workspace --all-targets -- -D warnings` (Constitution: zero warnings)
- `cargo +stable test --workspace` (all unit + integration tests, including the new m170 duplicate-label gate + the golden-diff checks)

## Path E — Verify SC-005 golden byte-identity restriction

```bash
cd /path/to/mikebom
git diff main -- 'mikebom-cli/tests/fixtures/golden/**' 2>&1 | grep -E "^[-+]" | head -30
# Expected on m170 branch: shows ONLY the removal of duplicate mikebom:graph-completeness entries.
# No unrelated deltas — no timestamp changes, no reordering, no other property edits.
```

If additional deltas appear, investigate before merging: they may indicate an accidental behavior change on an unrelated code path.
