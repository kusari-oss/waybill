#!/usr/bin/env bash
# Authoring artifact for milestone 150 — verifies jq recipes from
# docs/reference/reading-a-mikebom-sbom.md against real mikebom-emitted SBOMs.
#
# Per spec FR-011 + SC-004: at least 5 jq recipes verified runnable at doc-
# authoring time. This script makes the verification re-runnable post-merge.
#
# Usage:
#   ./specs/150-sbom-consumer-guide/verify-recipes.sh
#
# Requires:
#   - mikebom binary built (cargo build --release OR cargo run will trigger)
#   - jq installed
#   - the milestone-090 sibling-fixture repo cached at $MIKEBOM_FIXTURES_DIR
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

# Recipe 1: mikebom:lifecycle-scope (dev-scoped) in CDX
# Requires --include-dev to see dev components
run_recipe "lifecycle-scope-cdx" "cyclonedx-json" "transitive_parity/npm" \
    "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:lifecycle-scope" and .value == "development")
     | .purl' \
    "present"

# Recipe 2: mikebom:lifecycle-scope (dev-scoped) in SPDX 2.3
run_recipe "lifecycle-scope-spdx23" "spdx-2.3-json" "transitive_parity/npm" \
    "" \
    '.packages[]
     | select(.annotations[]?
              | .comment | fromjson?
              | select(.field == "mikebom:lifecycle-scope" and .value == "development"))
     | .name + " " + .versionInfo' \
    "present"

# Recipe 3: mikebom:lifecycle-scope (dev) in SPDX 3 — native LifecycleScopedRelationship.scope
run_recipe "lifecycle-scope-spdx3" "spdx-3-json" "transitive_parity/npm" \
    "" \
    '.["@graph"][]
     | select(.type == "Relationship"
              and .relationshipType == "dependsOn"
              and .scope == "development")
     | .to[]?' \
    "present"

# Recipe 4: mikebom:source-type grouping
run_recipe "source-type-cdx" "cyclonedx-json" "cargo/lockfile-v3" "" \
    '[.components[]
      | {purl, source_type: (.properties[]? | select(.name == "mikebom:source-type") | .value)}]
     | group_by(.source_type)
     | map({source_type: .[0].source_type, count: length})' \
    "nonempty"

# Recipe 5: mikebom:generation-context document-scope
run_recipe "generation-context-cdx" "cyclonedx-json" "cargo/lockfile-v3" "" \
    '.metadata.properties[]?
     | select(.name == "mikebom:generation-context")
     | .value' \
    "present"

# Recipe 6: mikebom:demoted-from-main-module — needs override flag
run_recipe "demoted-from-main-module-cdx" "cyclonedx-json" "transitive_parity/cargo" \
    "--root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:demoted-from-main-module" and .value == "true")
     | {purl, name, version}' \
    "nonempty"

echo
echo "============================================================"
echo "Verification summary: $PASS passed, $FAIL failed"
echo "============================================================"

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
