# Quickstart — milestone 145 annotation-emission parity fixes

Operator-facing walkthrough.

## Scenario 1 — Verify the `mikebom:file-paths` shape fix (US1)

After this milestone, every file-tier component's `mikebom:file-paths` annotation in SPDX 2.3 / SPDX 3 carries a native JSON array. Pre-145 carried a JSON-string-encoded array.

```bash
# Scan any image-like fixture (file-tier emission required — the default
# --file-inventory=orphan since milestone 133).
mikebom sbom scan --path <some-rootfs-or-image-extract> \
    --format spdx-2.3-json --output /tmp/check.spdx.json

# Pre-145: returns "string"
# Post-145: returns "array" for every file-tier component
jq -r '.packages[]
       | .annotations[]?
       | (.comment | fromjson)
       | select(.field == "mikebom:file-paths")
       | .value | type' /tmp/check.spdx.json \
    | sort -u
```

Same check for SPDX 3:

```bash
mikebom sbom scan --path <...> --format spdx-3-json --output /tmp/check.spdx3.json
jq -r '.["@graph"][]
       | .annotations[]?
       | .software_statement
       | select(. != null)
       | fromjson
       | select(.field == "mikebom:file-paths")
       | .value | type' /tmp/check.spdx3.json \
    | sort -u
# Post-145: only "array" appears.
```

## Scenario 2 — Verify `mikebom:lifecycle-scope` in SPDX 3 (US2)

After this milestone, SPDX 3 output carries `mikebom:lifecycle-scope` on dev/build/test-scoped components, matching the existing CDX + SPDX 2.3 behavior.

```bash
# Use an npm project that has dev dependencies — the node-dev-vs-prod
# fixture is the audit's source.
mikebom sbom scan --path <path-to-npm-fixture> \
    --format cyclonedx-json --format spdx-3-json \
    --output cyclonedx-json=/tmp/n.cdx.json \
    --output spdx-3-json=/tmp/n.spdx3.json

# Count dev-scoped components per format — they should match.
jq -r '[.components[]
        | .properties[]?
        | select(.name == "mikebom:lifecycle-scope" and .value == "development")]
       | length' /tmp/n.cdx.json
# (some count > 0)

jq -r '[.["@graph"][]
        | .annotations[]?
        | .software_statement
        | select(. != null)
        | fromjson
        | select(.field == "mikebom:lifecycle-scope" and .value == "development")]
       | length' /tmp/n.spdx3.json
# (same count > 0 — post-145; pre-145 was 0)
```

## Scenario 3 — Verify `mikebom:source-files` parity on Maven deps (US3)

After this milestone, scanning the `polyglot-builder-image` fixture (or equivalent) produces byte-equivalent `mikebom:source-files` values across CDX and SPDX 3 for Maven dep components.

```bash
mikebom sbom scan --path <path-to-polyglot-builder-image-fixture> \
    --format cyclonedx-json --format spdx-3-json \
    --output cyclonedx-json=/tmp/m.cdx.json \
    --output spdx-3-json=/tmp/m.spdx3.json

# Extract Maven dep source-files from each format, sort, diff.
jq -r '.components[]
       | select(.purl | startswith("pkg:maven/"))
       | {purl, src: [.properties[]? | select(.name == "mikebom:source-files") | .value]}' \
    /tmp/m.cdx.json | jq -s 'sort_by(.purl)' > /tmp/cdx-maven.json

jq -r '.["@graph"][]
       | select(.software_packageUrl? | tostring | startswith("pkg:maven/"))
       | {purl: .software_packageUrl,
          src: [.annotations[]? | .software_statement | select(. != null) | fromjson
                | select(.field == "mikebom:source-files") | .value]}' \
    /tmp/m.spdx3.json | jq -s 'sort_by(.purl)' > /tmp/spdx3-maven.json

diff /tmp/cdx-maven.json /tmp/spdx3-maven.json
# Post-145: empty diff (zero drift).
```

## Scenario 4 — Re-run the sbom-conformance harness

The harness-reported CFI finding counts (SC-001 / SC-004 / SC-008) are the operator-facing measurement of success. Re-run the harness against a post-145 build:

```bash
# Harness execution (assumes the operator has it installed).
sbom-conformance audit --target mikebom --fixture-set canonical \
    --pre-rev <pre-145-commit> --post-rev <post-145-commit>

# Expected: ≥3,424 CFI finding reduction across the file-paths /
# lifecycle-scope / source-files clusters.
```

## Verification commands (in-tree, CI-binding)

```bash
# All US1 + US2 + US3 unit tests pass:
cargo test -p mikebom --bin mikebom file_tier::tests::
cargo test -p mikebom --bin mikebom spdx::v3_annotations::tests::
cargo test -p mikebom --bin mikebom source_files_byte_equivalent

# Pre-PR gate:
cargo +stable clippy --workspace --all-targets -- -D warnings
cargo +stable test --workspace
# Or simply:
./scripts/pre-pr.sh
```

## Golden-fixture refresh (post-fix, before commit)

```bash
# US1 — file-paths shape change affects SPDX 2.3 + SPDX 3 goldens
# containing file-tier components (CDX unchanged):
MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression

# US2 — lifecycle-scope addition affects SPDX 3 goldens only:
# (same MIKEBOM_UPDATE_SPDX3_GOLDENS=1 invocation, covered by the above)

# US3 — source-files dedup MAY affect goldens containing Maven nested-JAR
# components on image-extract fixtures. Re-run BOTH SPDX update vars and
# the CDX update var depending on which fix path is taken:
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression
```

Confirm refresh scope by inspecting `git diff --stat tests/fixtures/golden/` — diffs MUST be limited to the documented per-US wire-shape changes; reject any unrelated drift.

## Known deferrals (spec Out of Scope)

- Image-pseudo-component absent-from-spdx-json gap (2 cases on `go-vcs-buildinfo`).
- `mikebom:sbom-tier` per-component disagreement on Maven shaded/transitive deps.
- New `mikebom:*` annotations.
- Restructuring of `extra_annotations` BTreeMap representation.
- `<component-presence>` cluster from prior audit run — collapsed to 2 by harness patch (not a mikebom issue).
- Performance optimization of file-tier emission.
- Changes to the Maven reader's `entry.source_path` POPULATION (only the per-emitter dedup behavior changes; underlying field is unchanged).
