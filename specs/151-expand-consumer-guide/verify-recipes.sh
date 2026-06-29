#!/usr/bin/env bash
# Authoring artifact for milestone 151 — verifies jq recipes from the 6 newly-
# depth-covered signal sections in docs/reference/reading-a-mikebom-sbom.md
# against real mikebom-emitted SBOMs.
#
# Per spec FR-012 + SC-003: at least 6 new jq recipes verified runnable at doc-
# authoring time. This script makes the verification re-runnable post-merge.
#
# Mirrors the milestone-150 harness pattern (specs/150-sbom-consumer-guide/
# verify-recipes.sh) — same run_recipe helper, same fixtures-dir lookup, same
# scratch-dir + cleanup pattern, same per-recipe expectation modes ("nonempty"
# vs "present").
#
# Usage:
#   ./specs/151-expand-consumer-guide/verify-recipes.sh
#
# Requires:
#   - mikebom binary built (cargo build --release OR cargo run will trigger)
#   - jq installed
#   - the milestone-090 sibling-fixture repo cached at $MIKEBOM_FIXTURES_DIR
#     (or available at $HOME/.cache/mikebom/fixtures/<sha>/)
#
# Exit code 0 = all recipes produced expected output shape.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

FIXTURES_DIR="${MIKEBOM_FIXTURES_DIR:-$HOME/.cache/mikebom/fixtures/$(ls $HOME/.cache/mikebom/fixtures 2>/dev/null | head -1)}"
if [[ ! -d "$FIXTURES_DIR" ]]; then
    echo "ERROR: fixtures dir not found at $FIXTURES_DIR. Set MIKEBOM_FIXTURES_DIR." >&2
    exit 2
fi
echo "Using fixtures from: $FIXTURES_DIR"

TMP="$(mktemp -d)"
trap "rm -rf $TMP" EXIT

# Build mikebom once (release for speed).
echo "Building mikebom (release) ..."
cargo +stable build --release -p mikebom 2>&1 | tail -3
MIKEBOM="$REPO_ROOT/target/release/mikebom"

PASS=0
FAIL=0

run_recipe() {
    local recipe_name="$1"
    local format="$2"
    local fixture="$3"
    local extra_flags="$4"
    local jq_recipe="$5"
    local expectation="$6"  # 'nonempty' or 'present'

    echo
    echo "=== Recipe $recipe_name ($format on $fixture) ==="
    local out="$TMP/$recipe_name.$format.json"
    # shellcheck disable=SC2086
    $MIKEBOM sbom scan --offline --path "$FIXTURES_DIR/$fixture" \
        --format "$format" --output "$out" --no-deep-hash $extra_flags \
        > /dev/null 2>&1

    local result
    result=$(jq "$jq_recipe" "$out" 2>/dev/null || echo "JQ_ERROR")

    if [[ "$expectation" == "nonempty" && -n "$result" && "$result" != "null" && "$result" != "JQ_ERROR" ]]; then
        echo "✓ PASS — recipe produced non-empty output:"
        echo "$result" | head -5
        PASS=$((PASS + 1))
    elif [[ "$expectation" == "present" && "$result" != "JQ_ERROR" ]]; then
        echo "✓ PASS — recipe ran without error (output may be empty if signal not in fixture):"
        echo "$result" | head -3
        PASS=$((PASS + 1))
    else
        echo "✗ FAIL — recipe error or unexpected output:"
        echo "$result" | head -10
        FAIL=$((FAIL + 1))
    fi
}

# ============================================================================
# US1 — Trust trio (mikebom:evidence-kind, mikebom:confidence)
# ============================================================================
# Recipes for the trust-trio depth coverage added to §3.3 build provenance.
# Use the cargo transitive-parity fixture (rich source-type variety per
# milestone-150 precedent).

# Recipe US1.1: mikebom:evidence-kind filter in CDX
run_recipe "evidence-kind-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:evidence-kind")
     | {purl, evidence_kind: (.properties[] | select(.name == "mikebom:evidence-kind") | .value)}' \
    "present"

# Recipe US1.2: mikebom:confidence filter in CDX
run_recipe "confidence-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:confidence")
     | {purl, confidence: (.properties[] | select(.name == "mikebom:confidence") | .value)}' \
    "present"

# Recipe US1.3: trust-trio composing recipe (the workflow that drove this milestone)
run_recipe "trust-trio-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | {
         purl,
         source_type:   (.properties[]? | select(.name == "mikebom:source-type")   | .value),
         evidence_kind: (.properties[]? | select(.name == "mikebom:evidence-kind") | .value),
         confidence:    (.properties[]? | select(.name == "mikebom:confidence")    | .value)
       }
     | select(.source_type != null)' \
    "present"

# ============================================================================
# US2 — Binary linkage (mikebom:linkage-kind, mikebom:not-linked)
# ============================================================================
# These signals are emitted on binary-tier scans (linkage-kind) and Go-source
# scans with a binary present (not-linked). Fixture availability for binary-
# tier scans in the milestone-090 sibling repo is limited; use "present"
# expectation per research.md §R8 skip-with-note pattern.

# Recipe US2.1: mikebom:linkage-kind filter in CDX (binary-tier components)
run_recipe "linkage-kind-cdx" "cyclonedx-json" "transitive_parity/go" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:linkage-kind")
     | {name, linkage_kind: (.properties[] | select(.name == "mikebom:linkage-kind") | .value)}' \
    "present"

# Recipe US2.2: mikebom:not-linked suppression filter (Go-only)
run_recipe "not-linked-cdx" "cyclonedx-json" "transitive_parity/go" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:not-linked" and .value == "true")
     | .purl' \
    "present"

# ============================================================================
# US3 — Unresolved deps + assertion conflict
# ============================================================================
# depends-unresolved is currently Yocto-only (milestone 128); assertion-conflict
# requires a supplement file (milestone 119). The milestone-090 sibling-fixture
# repo does not currently carry Yocto or supplement-file fixtures; use "present"
# expectation per research.md §R8 + the fixture-gap note. The recipes are
# documented so consumers running mikebom against THEIR own Yocto/supplement
# scans can verify the doc's claims against real data.

# Recipe US3.1: mikebom:depends-unresolved closure-gap scan
run_recipe "depends-unresolved-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:depends-unresolved")
     | {purl, unresolved: (.properties[] | select(.name == "mikebom:depends-unresolved") | .value | fromjson)}' \
    "present"

# Recipe US3.2: mikebom:assertion-conflict supplement-override audit
run_recipe "assertion-conflict-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:assertion-conflict")
     | {
         purl,
         conflicts: (.properties[]
                     | select(.name == "mikebom:assertion-conflict")
                     | .value | fromjson)
       }' \
    "present"

echo
echo "============================================================"
echo "Verification summary: $PASS passed, $FAIL failed"
echo "============================================================"

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
