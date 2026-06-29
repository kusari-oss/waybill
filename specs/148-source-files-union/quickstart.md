# Quickstart — milestone 148 source-files cross-emitter union

Operator-facing walkthrough.

## Scenario 1 — Reproduce the 51 → 0 audit drop on polyglot-builder-image

The motivating use case: the polyglot-builder-image fixture surfaced 51 cross-format `mikebom:source-files` divergence findings in the 2026-06-28 audit harness run (post-145).

```bash
# Pre-148: 51 Maven PURLs with divergent mikebom:source-files across CDX vs SPDX 3
mikebom sbom scan --path /path/to/polyglot-builder-image \
    --format cyclonedx-json --output /tmp/mb-pre.cdx.json
mikebom sbom scan --path /path/to/polyglot-builder-image \
    --format spdx-3-json --output /tmp/mb-pre.spdx3.json

# Compare mikebom:source-files values for every shared Maven PURL.
# Pre-148: ~51 PURLs with divergent values; post-148: 0 (or near-zero).
python3 - <<'PY'
import json, sys

cdx = json.load(open("/tmp/mb-pre.cdx.json"))
spdx = json.load(open("/tmp/mb-pre.spdx3.json"))

def cdx_source_files(comp):
    for p in comp.get("properties", []):
        if p.get("name") == "mikebom:source-files":
            return frozenset(json.loads(p["value"]) if isinstance(p["value"], str) else p["value"])
    return None

def spdx_source_files(elem):
    for a in elem.get("annotation", []):
        try:
            env = json.loads(a.get("statement", "{}"))
            if env.get("field") == "mikebom:source-files":
                return frozenset(env.get("value", []))
        except (ValueError, AttributeError):
            continue
    return None

cdx_by_purl = {c["purl"]: cdx_source_files(c) for c in cdx.get("components", []) if c.get("purl")}
spdx_by_purl = {e["software_packageUrl"]: spdx_source_files(e)
                for e in spdx.get("@graph", [])
                if e.get("type") == "software_Package" and e.get("software_packageUrl")}

shared = set(cdx_by_purl) & set(spdx_by_purl)
diverge = [p for p in shared
           if cdx_by_purl[p] is not None
           and spdx_by_purl[p] is not None
           and cdx_by_purl[p] != spdx_by_purl[p]
           and "pkg:maven/" in p]
print(f"Divergent Maven PURLs: {len(diverge)}")
# Pre-148: ~51
# Post-148: 0
PY
```

## Scenario 2 — Verify single-entry PURLs are unchanged (FR-007)

```bash
# Scan a non-Maven fixture (npm, Cargo, etc.) where no same-PURL multi-entry
# shape exists. Compare pre/post-148 mikebom:source-files values.

mikebom sbom scan --path /path/to/npm-fixture \
    --format cyclonedx-json --output /tmp/mb.cdx.json

# Every component's mikebom:source-files value MUST be byte-identical to its
# pre-148 emission. The union-with-self degenerates to identity (FR-007).

# Confirm via golden diff (see Verification Commands below for the trifecta).
```

## Scenario 3 — Verify the synthetic Maven nested-coord fixture

```bash
# The SC-003 in-tree integration test exercises a minimal synthetic fixture:
mikebom sbom scan --path mikebom-cli/tests/fixtures/source_files_union/ \
    --format cyclonedx-json --output /tmp/syn.cdx.json

# Inspect the foo coord: pre-148 it had ONE path per entry; post-148 it has
# BOTH paths (alphabetically sorted) on every entry.
jq '
  .components[]
  | select(.purl == "pkg:maven/com.example/foo@1.0")
  | .properties[]
  | select(.name == "mikebom:source-files")
' /tmp/syn.cdx.json
# Post-148: {"name": "mikebom:source-files",
#            "value": "[\"target/fat-bundle.jar!com/example/foo-1.0.jar\",
#                      \"target/primary.jar\"]"}
# (Both paths, alphabetically sorted, on the entry's mikebom:source-files
# annotation. Same value appears on the nested-under-fat-jar entry too.)

# Same query against SPDX 3:
mikebom sbom scan --path mikebom-cli/tests/fixtures/source_files_union/ \
    --format spdx-3-json --output /tmp/syn.spdx3.json

jq '
  .["@graph"][]
  | select(.software_packageUrl == "pkg:maven/com.example/foo@1.0")
  | .annotation[]?
  | .statement
  | fromjson
  | select(.field == "mikebom:source-files")
' /tmp/syn.spdx3.json
# Post-148: identical {"field":"mikebom:source-files",
#                      "value":["target/fat-bundle.jar!.../foo-1.0.jar",
#                               "target/primary.jar"]}
# byte-equal to the CDX value above.
```

## Verification commands (in-tree, CI-binding)

```bash
# New unit tests in deduplicator.rs:
cargo test -p mikebom canonicalize_source_files_by_purl

# New in-tree integration test:
cargo test --test source_files_purl_union_md148

# Parity catalog row C18 (mikebom:source-files SymmetricEqual):
cargo test -p mikebom parity::extractors::tests::c18_

# Pre-PR gate:
./scripts/pre-pr.sh
```

## Golden refresh (post-fix, before commit)

```bash
# Maven-bearing goldens may be affected:
MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression cdx_regression_maven
MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression maven_byte_identity
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression maven_byte_identity

# Inspect:
git diff --stat -- mikebom-cli/tests/fixtures/golden/cyclonedx/maven.cdx.json \
                   mikebom-cli/tests/fixtures/golden/spdx-2.3/maven.spdx.json \
                   mikebom-cli/tests/fixtures/golden/spdx-3/maven.spdx3.json

# Acceptance: any diff line MUST be either:
#   (a) a mikebom:source-files value change on a Maven component that previously
#       carried a non-canonical single-path Vec, OR
#   (b) the new alphabetically-sorted union content
# NO unrelated drift permitted (FR-010).
#
# NOTE: if the pom-three-deps fixture doesn't exercise the multi-entry shape
# (likely — it's a simple POM fixture), the diffs will be empty and the
# synthetic fixture from SC-003 is the only exercise of the new code path.
```

## Cross-tool comparison (operator-cadence per SC-007)

```bash
# Re-run the sbom-conformance audit harness on polyglot-builder-image post-merge:
# (operator-specific tooling; not in mikebom's CI)
sbom-conformance-audit \
    --formats cdx,spdx-2.3,spdx-3 \
    --target polyglot-builder-image \
    --filter mikebom:source-files

# Expected: pre-148 51 findings → post-148 0 findings (or near-zero residuals
# from a different bug class, documented in the audit report).
```

## Known deferrals (spec Out of Scope)

- CDX `bom-ref` uniqueness enforcement (two same-PURL `parent_purl=None` components both get bom-ref = plain PURL string, a CDX 1.6 spec violation that's pre-existing; not addressed by this milestone).
- Per-emitter component-instance dedup (some emitters might benefit from collapsing two same-PURL components into one wire entry; out of scope — this milestone only canonicalizes the `evidence.source_file_paths` Vec).
- New `mikebom:source-files-*` annotations — none introduced (FR-008).
- Investigation of the JVM symlink paths issue (#1 from the 2026-06-28 triage) — needs operator-cadence raw-output diagnostic first.
- The SPDX 3 lifecycle-scope harness finding (#2 from the 2026-06-28 triage) — deliberate Principle V decision; not a mikebom bug.
- File-tier components — file_tier walker already aggregates per-hash; the union pass is idempotent for them.
- Cross-ecosystem same-PURL multi-entry cases beyond Maven (Cargo workspace vendor, Go vendor) — fix applies generically, but SC-001 metric is Maven-specific.
