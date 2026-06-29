# Quickstart — milestone 149 preserve-manifest-main-module flag

Operator-facing walkthrough.

## Scenario 1 — Opt-in demote on a Cargo project

The motivating use case: shipped service `widget-svc` has internal Cargo crate name `foo-internal`. Operator wants the SBOM to surface `widget-svc@1.2.3` as the deployment-meaningful identity AND preserve `pkg:cargo/foo-internal@0.5.1` as manifest provenance.

```bash
# Setup: any Cargo project with [package].name + version in Cargo.toml.
mkdir /tmp/cargo-demote && cd /tmp/cargo-demote
cat > Cargo.toml <<'TOML'
[package]
name = "foo-internal"
version = "0.5.1"
edition = "2021"
TOML
mkdir -p src && echo 'fn main() {}' > src/main.rs

# Pre-149 (or post-149 without --preserve-manifest-main-module): clean-replacement.
mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 \
    --format cyclonedx-json --output /tmp/pre.cdx.json

jq '{
  root: .metadata.component | {name, version, purl, type},
  has_foo_internal: (.components | map(select(.name == "foo-internal")) | length > 0)
}' /tmp/pre.cdx.json
# Pre-149:
#   { "root": { "name": "widget-svc", "version": "1.2.3",
#               "purl": "pkg:generic/widget-svc@1.2.3", "type": "application" },
#     "has_foo_internal": false }

# Post-149 with --preserve-manifest-main-module: demote-to-library.
mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 \
    --preserve-manifest-main-module \
    --format cyclonedx-json --output /tmp/post.cdx.json

jq '{
  root: .metadata.component | {name, version, purl, type},
  foo_internal: (.components | map(select(.name == "foo-internal")) | .[0])
}' /tmp/post.cdx.json
# Post-149:
#   { "root": { "name": "widget-svc", "version": "1.2.3", ... "type": "application" },
#     "foo_internal": {
#       "name": "foo-internal", "version": "0.5.1",
#       "purl": "pkg:cargo/foo-internal@0.5.1",
#       "type": "library",
#       "properties": [
#         {"name": "mikebom:demoted-from-main-module", "value": "true"}
#       ]
#     }
#   }
```

## Scenario 2 — Cross-format invariance check

```bash
# Same fixture, three formats:
for fmt in cyclonedx-json spdx-2.3-json spdx-3-json; do
    out="/tmp/demote.${fmt}.json"
    mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 \
        --preserve-manifest-main-module \
        --format "${fmt}" --output "$out"
done

# Extract the demote annotation value from each format. Should all be "true".
echo "CDX:"
jq -r '.components[]
       | select(.name == "foo-internal")
       | .properties[]
       | select(.name == "mikebom:demoted-from-main-module")
       | .value' /tmp/demote.cyclonedx-json.json

echo "SPDX 2.3:"
jq -r '.packages[]
       | select(.name == "foo-internal")
       | .annotations[]?
       | .comment
       | fromjson
       | select(.field == "mikebom:demoted-from-main-module")
       | .value' /tmp/demote.spdx-2.3-json.json

echo "SPDX 3:"
jq -r '.["@graph"][]
       | select(.software_packageUrl? == "pkg:cargo/foo-internal@0.5.1")
       | .annotation[]?
       | .statement
       | fromjson
       | select(.field == "mikebom:demoted-from-main-module")
       | .value' /tmp/demote.spdx-3-json.json
# All three: "true"
```

## Scenario 3 — Regression check: clean-replacement default still works

```bash
# Same fixture, no --preserve-manifest-main-module flag → byte-identical to
# milestone 077's clean-replacement output.

mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 \
    --format cyclonedx-json --output /tmp/regression.cdx.json

# Assert: no foo-internal entry, no demote annotation.
jq '.components | map(select(.name == "foo-internal")) | length' /tmp/regression.cdx.json
# 0

jq '[.. | objects | select(.name? == "mikebom:demoted-from-main-module")] | length' \
   /tmp/regression.cdx.json
# 0
```

## Scenario 4 — Regression check: no override flags, no behavior change

```bash
# Same fixture, no override flags at all → pre-149 behavior intact.
mikebom sbom scan --path . \
    --format cyclonedx-json --output /tmp/no-override.cdx.json

# Assert: foo-internal IS the root (milestone-064 main-module promotion behavior).
jq '.metadata.component | {name, version, purl}' /tmp/no-override.cdx.json
# {"name": "foo-internal", "version": "0.5.1", "purl": "pkg:cargo/foo-internal@0.5.1"}

# Assert: no demote annotation anywhere.
jq '[.. | objects | select(.name? == "mikebom:demoted-from-main-module")] | length' \
   /tmp/no-override.cdx.json
# 0
```

## Scenario 5 — Verify the edge cases

### Edge Case 1: preserve flag without override (silent no-op + INFO log)

```bash
mikebom sbom scan --path . --preserve-manifest-main-module \
    --format cyclonedx-json --output /tmp/edge-1.cdx.json 2>&1 \
    | grep -i 'preserve-manifest-main-module'
# Expected INFO log: "--preserve-manifest-main-module has no effect without --root-name override"

# Output is byte-identical to no-flag-at-all (Scenario 4).
diff <(jq -S 'del(.metadata.timestamp,.serialNumber)' /tmp/no-override.cdx.json) \
     <(jq -S 'del(.metadata.timestamp,.serialNumber)' /tmp/edge-1.cdx.json)
# (no diff output)
```

### Edge Case 4: multi-main-module + preserve (silent no-op + INFO log)

```bash
# Cargo workspace with >1 main-modules; the override + preserve combo is no-op
# because milestone 127 uses a placeholder root for multi-module scans.
mikebom sbom scan --path /path/to/cargo-workspace \
    --root-name big-monorepo --root-version 2026 \
    --preserve-manifest-main-module \
    --format cyclonedx-json --output /tmp/edge-4.cdx.json 2>&1 \
    | grep -i 'multi-main-module'
# Expected INFO log: "--preserve-manifest-main-module skipped: multi-main-module scan (N modules detected)"
```

## Verification commands (in-tree, CI-binding)

```bash
# New unit tests in the helper module:
cargo test -p mikebom apply_drop_or_demote

# New integration test covering Cargo + npm + Go fixtures:
cargo test --test demote_manifest_mainmod_md149

# Parity catalog row C102 (cross-format invariance once goldens refresh):
cargo test -p mikebom parity::extractors::tests::c102_

# Pre-PR gate:
./scripts/pre-pr.sh
```

## Golden refresh (post-fix, before commit)

```bash
# Cargo + npm + Go goldens may refresh when the integration test sets the
# new flag (only those tests' goldens; existing default-mode goldens stay
# byte-identical per SC-003).
MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression

# Inspect:
git diff --stat -- mikebom-cli/tests/fixtures/golden/

# Acceptance: regression-protected goldens (default-mode scans) MUST be
# unchanged. Only goldens explicitly scanned with the new flag should
# show the demote annotation diff. Reject any unrelated drift.
#
# NOTE: if the integration test doesn't use the standard golden harness
# (because it uses --preserve-manifest-main-module which the harness
# doesn't pass), the goldens for the new tests live in a separate location
# and the standard regression goldens stay completely unchanged.
```

## Cross-tool comparison (operator-cadence per Assumption 8)

```bash
# Three remaining ecosystems not covered by the CI integration test:
# pip / gem / Maven. Operator-cadence verification:
for fixture in pip-fixture gem-fixture maven-fixture; do
    mikebom sbom scan --path "/tmp/${fixture}" \
        --root-name widget-svc --root-version 1.2.3 \
        --preserve-manifest-main-module \
        --format cyclonedx-json --output "/tmp/${fixture}.cdx.json"

    # Assert the manifest-derived identity appears as a demoted library:
    jq '.components | map(select(.properties[]?
                                | select(.name == "mikebom:demoted-from-main-module")))
                    | length' \
       "/tmp/${fixture}.cdx.json"
    # Expected: 1 (the demoted manifest main-module)
done
```

## Known deferrals (spec Out of Scope)

- Default behavior change for `--root-name` (would break milestone-077 backward compat — opt-in flag preserves stability).
- Demoting other component roles (workspace members, binaries, services) — out of scope; future milestone.
- Suppression flag for the annotation (`--no-demote-annotation`) — speculative; consumers can filter at parse time.
- Round-trip "promote-back-to-main-module" inverse flag — no use case; out of scope.
- `mikebom:override-source` annotation on the override root component — separate Principle V audit; future milestone if operator tooling needs it.
- Test coverage for pip / gem / Maven in the CI integration test — covered via operator-cadence verification per Assumption 8.
