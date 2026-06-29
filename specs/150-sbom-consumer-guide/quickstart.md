# Quickstart — milestone 150 consumer reading guide

Operator-facing walkthrough for VALIDATING the new doc post-merge. This milestone's quality is fundamentally an operator-cadence read-through assessment (per spec SC-001) — there's no automated test that can verify "is this doc useful to a consumer?".

## Scenario 1 — SC-001 5-question read-through audit

After the doc ships, an operator (or external reviewer simulating a first-time mikebom-SBOM consumer) reads `docs/reference/reading-a-mikebom-sbom.md` end-to-end and must be able to answer all 5 questions WITHOUT consulting `sbom-format-mapping.md` or any other doc:

1. **"What does `mikebom:lifecycle-scope` mean?"**
   Expected answer: a parity-bridging annotation carrying the finer dev/build/test/runtime distinction beyond CDX 1.6's 3-value `scope` enum. The annotation lives on the dep target's `properties[]` in CDX, on the target Package's `annotations[]` envelope in SPDX 2.3 + SPDX 3.

2. **"How do I find dev-only dependencies in a mikebom SPDX 2.3 SBOM?"**
   Expected answer: use SPDX 2.3 typed relationships (`DEV_DEPENDENCY_OF`) for the spec-native expression, OR walk the target Package's `annotations[]` for `mikebom:lifecycle-scope = "development"`. The doc provides a `jq` recipe.

3. **"Where do I find which OCI layer a binary came from?"**
   Expected answer: the per-component `mikebom:layer-digest` annotation (CDX `properties[]` / SPDX 2.3 + SPDX 3 envelope). Emitted only for image scans.

4. **"What's the difference between `mikebom:source-type = trace-observed` and `mikebom:source-type = declared-not-cached`?"**
   Expected answer: trace-observed means mikebom's eBPF (or filesystem scan) saw the component as actually fetched/used; declared-not-cached means a lockfile or manifest declared it but mikebom couldn't verify its presence. Strong vs weak provenance.

5. **"What's the `mikebom-annotation/v1` envelope shape?"**
   Expected answer: a JSON object with 3 fields — `schema` (the literal string `"mikebom-annotation/v1"`), `field` (the `mikebom:*` key), `value` (the payload). Used in SPDX 2.3 + SPDX 3 to wrap annotation values; CDX uses a flat `properties[].value` string carrier instead.

If all 5 answers come from the doc alone: ✅ SC-001 passes. If any answer requires reading `sbom-format-mapping.md` or another doc: the doc needs strengthening on that signal.

## Scenario 2 — SC-002 appendix-coverage audit

```bash
# Extract all mikebom: keys present in the catalog at doc-ship time:
grep -E "^\| C[0-9]+\b" /Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md \
  | grep -oE "mikebom:[a-z0-9-]+" | sort -u > /tmp/catalog-keys.txt

# Extract all mikebom: keys present in the new doc's Appendix A:
grep -oE "mikebom:[a-z0-9-]+" /Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md \
  | sort -u > /tmp/guide-keys.txt

# Asymmetric diff: keys in the catalog but missing from the guide.
diff /tmp/catalog-keys.txt /tmp/guide-keys.txt
# Expected: empty (all 102 catalog keys are in the guide's appendix).
# If ANY key is missing: the appendix has a coverage gap.
```

Per spec FR-006 + SC-002: every `mikebom:*` key in the catalog at milestone-150 ship time MUST be in Appendix A. The audit is mechanical.

## Scenario 3 — SC-003 index.md linkback verification

```bash
# Confirm the new doc is linked from docs/index.md's Reference material section.
grep -A 3 "Reference material" /Users/mlieberman/Projects/mikebom/docs/index.md \
  | grep "reading-a-mikebom-sbom"
# Expected: 1 match.
```

## Scenario 4 — SC-004 `jq` recipe verification (≥5 recipes)

Run each `jq` recipe in the doc against a real mikebom-emitted SBOM and confirm the documented output matches. Authoring artifact at `specs/150-sbom-consumer-guide/verify-recipes.sh` (or inline in the doc's authoring notes) lists each scan-then-jq invocation.

Example for the `mikebom:lifecycle-scope` recipe:

```bash
# Scan a fixture with dev deps included:
cargo +stable run -q -p mikebom --bin mikebom -- sbom scan --offline \
    --path /Users/mlieberman/.cache/mikebom/fixtures/<sha>/cargo/lockfile-v3 \
    --include-dev \
    --format spdx-2.3-json --output /tmp/spdx.json

# Run the doc's recipe:
jq -r '.packages[]
       | select(.annotations[]?
                | .comment
                | fromjson?
                | select(.field == "mikebom:lifecycle-scope" and .value == "development"))
       | .name' /tmp/spdx.json
# Expected output: dev-scoped package names, one per line.
```

If the recipe produces the documented output: ✅ SC-004 increments by 1 verified recipe. Repeat for ≥5 recipes total.

## Scenario 5 — SC-005 cluster coverage audit

```bash
# Verify the doc has 4 thematic-cluster section headings:
grep -E "^### 3\.[1-4]" /Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md
# Expected: 4 matches (3.1 / 3.2 / 3.3 / 3.4).
```

Then visually inspect each cluster section to confirm ≥2 documented signals (per spec FR-003 + SC-005). The 12-signal target from research §C ensures ≥3 per cluster — 1.5× the minimum.

## Scenario 6 — SC-006 depth-covered signal count

```bash
# Each depth-covered signal renders per the data-model §2 invariant — count
# section headings at depth 4 (####) inside section 3:
awk '/^### 3\./,/^### 4 /' /Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md \
  | grep -cE "^#### "
# Expected: ≥8 (target 12 per research §C).
```

## Scenario 7 — SC-007 pre-PR gate

```bash
./scripts/pre-pr.sh
# Expected: green except documented pre-existing sbomqs_parity env failure.
# This milestone is docs-only; the gate's clippy + test outcomes match pre-150 main.
```

## Scenario 8 — SC-008 reverse-link audit

```bash
# Verify the catalog is linked from the new doc at least once:
grep -c "sbom-format-mapping.md" /Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md
# Expected: ≥1 (likely much more — every depth-covered signal links to its C-row).
```

## Post-merge — operator-cadence external review

Per spec Assumption 9, the doc's quality is assessed via an operator-cadence read-through (the 5-question SC-001 audit above), not via automated tests. After merge:

1. The maintainer or an external reviewer sits down with a real mikebom-emitted SBOM (any format, any ecosystem) AND the new doc.
2. They formulate a question relevant to their workflow ("how do I tell whether this package was actually built vs just declared in a lockfile?").
3. They search the new doc for the answer.
4. They report success / failure in a follow-up issue if the doc didn't help.

This feedback loop drives future milestone updates to the doc — single-file deliverable means edits are surgical.

## Known deferrals (spec Out of Scope)

- No competitor comparison (per Q1 Option D — focus is consumer-centric on mikebom's emitted data).
- No auto-generated appendix (manual maintenance at milestone-150 ship; future milestone could automate via a build-time script that diffs catalog ↔ guide).
- No JSON Schema artifact for `mikebom-annotation/v1` (citing existing Rust source as canonical).
- No per-annotation deep-dive subdocs (single-file deliverable).
- No translations (English-only).
- No interactive consumer tooling (static Markdown only).
