# Quickstart — milestone 087 maintainer recipes

Four maintainer-facing recipes for verifying the fix, regenerating goldens, bumping the milestone-083 baseline, and reproducing the issue locally.

## Recipe 1 — Reproduce the issue (pre-fix baseline)

```bash
# Build alpha.25 mikebom (latest pre-087):
cargo +stable build --release -p mikebom

# Scan the milestone-083 cargo audit fixture:
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/transitive_parity/cargo \
    --format spdx-2.3-json \
    --output /tmp/repro-172.spdx.json \
    --no-deep-hash

# Find the wrong-version edge:
jq -r '
  ([.packages[] | {(.SPDXID): (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator)}] | add) as $purl |
  [.relationships[]
   | select(.relationshipType == "DEPENDS_ON")
   | {from: $purl[.spdxElementId], to: $purl[.relatedSpdxElement]}
   | select(.from != null and .to != null)
   | select(.from == "pkg:cargo/clap@4.5.21" and (.to | contains("clap_builder")))
  ]
' /tmp/repro-172.spdx.json
```

Expected (pre-fix): one entry, `from: pkg:cargo/clap@4.5.21, to: pkg:cargo/clap_builder@4.5.9` (wrong version).

## Recipe 2 — Verify the fix (post-implementation)

After implementing the fix per `plan.md`:

```bash
cargo +stable build --release -p mikebom

target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/transitive_parity/cargo \
    --format spdx-2.3-json \
    --output /tmp/post-087.spdx.json \
    --no-deep-hash

# Same jq query as Recipe 1 — the fix flips the version:
jq -r '
  ([.packages[] | {(.SPDXID): (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator)}] | add) as $purl |
  [.relationships[]
   | select(.relationshipType == "DEPENDS_ON")
   | {from: $purl[.spdxElementId], to: $purl[.relatedSpdxElement]}
   | select(.from != null and .to != null)
   | select(.from == "pkg:cargo/clap@4.5.21" and (.to | contains("clap_builder")))
  ]
' /tmp/post-087.spdx.json
```

Expected (post-fix): `from: pkg:cargo/clap@4.5.21, to: pkg:cargo/clap_builder@4.5.21` (correct version).

## Recipe 3 — Bump the milestone-083 cargo regression baseline

The fix changes mikebom's cargo edge emission, so milestone 083's `transitive_parity_cargo.rs` regression test will fail with edge-count drift. Standard maintainer workflow:

```bash
# Step 1: run the test, observe the count drift:
cargo +stable test -p mikebom --test transitive_parity_cargo
# Failure: "left: <new count>, right: 319"

# Step 2: re-derive EXPECTED_REPRESENTATIVE_EDGES post-fix:
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/transitive_parity/cargo \
    --format spdx-2.3-json --output /tmp/post-087.spdx.json --no-deep-hash
# Pick 2-3 edges that are now correct and use them as new representatives.

# Step 3: edit mikebom-cli/tests/transitive_parity_cargo.rs:
# - Update EXPECTED_MIKEBOM_EDGE_COUNT from 319 to the new count
# - Update EXPECTED_REPRESENTATIVE_EDGES to point at the now-correct
#   workspace-internal edges (e.g., clap → clap_builder is now valid)

# Step 4: re-run, confirm passes:
cargo +stable test -p mikebom --test transitive_parity_cargo
```

Then update `specs/083-transitive-correctness/research.md §8 — Ecosystem: cargo` to remove gap #1 from the "Specific gaps surfaced (mikebom-side)" list. Gap #2 (clap_derive zero outgoing edges, issue #173) remains.

## Recipe 4 — Regenerate the 3 cargo goldens

```bash
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression

# Confirm ONLY the 3 cargo goldens regenerated:
git status --short mikebom-cli/tests/fixtures/golden/
```

Expected: 3 modified files (`cyclonedx/cargo.cdx.json`, `spdx-2.3/cargo.spdx.json`, `spdx-3/cargo.spdx3.json`). Other 24 goldens byte-identical.

Then audit the diff scope — only dep-edge version strings should change:

```bash
git diff -- mikebom-cli/tests/fixtures/golden/cyclonedx/cargo.cdx.json | grep -E "^[-+].*\"@" | head -20
```

Expected: pairs of `-pkg:cargo/foo@<wrong>` / `+pkg:cargo/foo@<right>` lines. No other field changes.

## Recipe 5 — Final pre-PR gate

```bash
./scripts/pre-pr.sh
```

Expected output: zero clippy warnings, every test suite reports `0 failed`. Standard CLAUDE.md mandatory gate.

## When in doubt

- **Reproducer doesn't match Recipe 1's output**: confirm the cargo fixture is the alpha.25 baseline at `mikebom-cli/tests/fixtures/transitive_parity/cargo/Cargo.lock` (clap-rs/clap @ v4.5.21). If your local copy was modified, restore from git.
- **More than 3 goldens regenerate**: scope creep. Inspect `git diff --stat mikebom-cli/tests/fixtures/golden/`. Only cargo's 3 should change. If others changed, the fix has unintended impact — narrow.
- **The closure-invariant test fails post-fix**: shouldn't — the version-disambiguation doesn't change closure-set membership. If it does fail, the fix has introduced a new orphan ref. Investigate `cdx_ref_closure_invariant` failure output.
