#!/usr/bin/env bash
# Milestone 168 — Round-4 audit harness. Reproduces every measurement
# in the audit report at `docs/audits/2026-07-06-tauri-airflow.md`.
#
# Pinned target commit SHAs are the truth — this script clones fresh
# if the source trees are absent, otherwise fast-forwards. To pin a
# specific SHA per SC-010, override MIKEBOM_TAURI_SHA / MIKEBOM_AIRFLOW_SHA
# in the environment before running.
#
# Idempotent: safe to re-run. Emits per-tool wall-clock times per FR-003.
#
# Usage:
#     bash specs/168-rust-python-audit/scripts/run-audit.sh
#
# Optional environment overrides:
#     MIKEBOM_BIN=/path/to/mikebom     # default: $PWD/target/release/mikebom
#     MIKEBOM_TAURI_SHA=<sha>           # pin Tauri to a specific commit
#     MIKEBOM_AIRFLOW_SHA=<sha>         # pin Airflow to a specific commit
#     MIKEBOM_SKIP_SPDX3=1              # skip SPDX 3 emission (faster iteration)

set -euo pipefail

# ---------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
ARTIFACTS_ROOT="$REPO_ROOT/specs/168-rust-python-audit/artifacts"
SCRIPTS_DIR="$REPO_ROOT/specs/168-rust-python-audit/scripts"

MIKEBOM_BIN="${MIKEBOM_BIN:-$REPO_ROOT/target/release/mikebom}"
SPDX3_VALIDATE="$REPO_ROOT/.venv/spdx3-validate/bin/spdx3-validate"

TAURI_URL="https://github.com/tauri-apps/tauri.git"
AIRFLOW_URL="https://github.com/apache/airflow.git"

# ---------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------
if [[ ! -x "$MIKEBOM_BIN" ]]; then
    echo "error: mikebom binary not found at $MIKEBOM_BIN" >&2
    echo "  build with: cargo +stable build --release -p mikebom" >&2
    exit 2
fi

command -v trivy >/dev/null || { echo "error: trivy not found on PATH" >&2; exit 2; }
command -v syft  >/dev/null || { echo "error: syft not found on PATH"  >&2; exit 2; }
command -v jq    >/dev/null || { echo "error: jq not found on PATH"    >&2; exit 2; }

# ---------------------------------------------------------------------
# Clone or refresh a target repo. Records HEAD SHA for report header.
# ---------------------------------------------------------------------
clone_or_refresh() {
    local url="$1" dest="$2" pinned_sha="${3:-}"
    if [[ -d "$dest/.git" ]]; then
        echo "==> $dest exists — leaving as-is"
    else
        echo "==> cloning $url → $dest"
        git clone --depth 1 "$url" "$dest"
    fi
    if [[ -n "$pinned_sha" ]]; then
        (cd "$dest" && git fetch --depth 1 origin "$pinned_sha" && git checkout "$pinned_sha")
    fi
    (cd "$dest" && git rev-parse HEAD)
}

# ---------------------------------------------------------------------
# Run mikebom on a target, all 3 formats. Filenames per data-model.md E7.
# ---------------------------------------------------------------------
run_mikebom() {
    local src="$1" out="$2"
    mkdir -p "$out"

    echo "==> mikebom cyclonedx-json on $src"
    { time "$MIKEBOM_BIN" --offline sbom scan \
        --path "$src" \
        --format cyclonedx-json \
        --output "$out/mikebom.cdx.json" \
        --no-deep-hash 2>&1; } 2>&1 | tee "$out/mikebom.cdx.log"

    echo "==> mikebom spdx-2.3-json on $src"
    { time "$MIKEBOM_BIN" --offline sbom scan \
        --path "$src" \
        --format spdx-2.3-json \
        --output "$out/mikebom.spdx23.json" \
        --no-deep-hash 2>&1; } 2>&1 | tee "$out/mikebom.spdx23.log"

    if [[ -z "${MIKEBOM_SKIP_SPDX3:-}" ]]; then
        echo "==> mikebom spdx-3-json on $src"
        { time "$MIKEBOM_BIN" --offline sbom scan \
            --path "$src" \
            --format spdx-3-json \
            --output "$out/mikebom.spdx3.json" \
            --no-deep-hash 2>&1; } 2>&1 | tee "$out/mikebom.spdx3.log"
    fi
}

# ---------------------------------------------------------------------
# Run Trivy on a target, CDX only.
# ---------------------------------------------------------------------
run_trivy() {
    local src="$1" out="$2"
    echo "==> trivy fs (cyclonedx) on $src"
    { time trivy fs --format cyclonedx --output "$out/trivy.cdx.json" "$src" 2>&1; } 2>&1 | tee "$out/trivy.log"
}

# ---------------------------------------------------------------------
# Run Syft on a target, CDX only.
# ---------------------------------------------------------------------
run_syft() {
    local src="$1" out="$2"
    echo "==> syft (cyclonedx-json) on $src"
    { time syft "$src" -o "cyclonedx-json=$out/syft.cdx.json" 2>&1; } 2>&1 | tee "$out/syft.log"
}

# ---------------------------------------------------------------------
# SPDX validation on mikebom's output.
# ---------------------------------------------------------------------
run_spdx_validation() {
    local out="$1"
    echo "==> SPDX 2.3 jsonschema validation"
    # Reuse mikebom's vendored SPDX 2.3 schema.
    local schema="$REPO_ROOT/mikebom-cli/tests/fixtures/schemas/spdx-2.3.schema.json"
    if [[ -f "$schema" ]] && [[ -f "$out/mikebom.spdx23.json" ]]; then
        python3 -c "
import json, sys, jsonschema
schema = json.load(open('$schema'))
doc = json.load(open('$out/mikebom.spdx23.json'))
try:
    jsonschema.validate(doc, schema)
    print('SPDX 2.3 PASS')
except jsonschema.ValidationError as e:
    print(f'SPDX 2.3 FAIL: {e.message[:200]}')
    sys.exit(1)
" 2>&1 | tee "$out/spdx23-validate.log"
    else
        echo "SPDX 2.3 schema or SBOM missing — skipping" | tee "$out/spdx23-validate.log"
    fi

    if [[ -z "${MIKEBOM_SKIP_SPDX3:-}" && -x "$SPDX3_VALIDATE" && -f "$out/mikebom.spdx3.json" ]]; then
        echo "==> SPDX 3 spdx3-validate"
        "$SPDX3_VALIDATE" --json "$out/mikebom.spdx3.json" --quiet 2>&1 \
            && echo "SPDX 3 PASS" || echo "SPDX 3 FAIL"
    fi 2>&1 | tee -a "$out/spdx3-validate.log"
}

# ---------------------------------------------------------------------
# Run analyze.py on a target's SBOMs directory. Writes analysis.json.
# ---------------------------------------------------------------------
run_analyze() {
    local target_name="$1" out="$2" sha="$3"
    echo "==> analyze.py --target-name $target_name"
    python3 "$SCRIPTS_DIR/analyze.py" \
        --target-name "$target_name" \
        --sboms-dir "$out" \
        --commit-sha "$sha" \
        > "$out/analysis.json"
}

# ---------------------------------------------------------------------
# Per-target orchestration.
# ---------------------------------------------------------------------
audit_target() {
    local target_name="$1" url="$2" pinned_sha="$3"
    local src="$ARTIFACTS_ROOT/${target_name}-src"
    local out="$ARTIFACTS_ROOT/$target_name"

    echo ""
    echo "########################################################"
    echo "# ${target_name^^}"
    echo "########################################################"

    local sha
    sha=$(clone_or_refresh "$url" "$src" "$pinned_sha")
    echo "==> $target_name at SHA $sha"

    mkdir -p "$out"
    run_mikebom "$src" "$out"
    run_trivy   "$src" "$out"
    run_syft    "$src" "$out"
    run_spdx_validation "$out"
    run_analyze "$target_name" "$out" "$sha"

    echo "==> $target_name analysis:"
    jq '.tools | map_values({component_count, edge_count, bfs_reachable_pct, wall_clock_seconds})' \
        "$out/analysis.json" 2>/dev/null || cat "$out/analysis.json"
}

# ---------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------
mkdir -p "$ARTIFACTS_ROOT"

audit_target "tauri"   "$TAURI_URL"   "${MIKEBOM_TAURI_SHA:-}"
audit_target "airflow" "$AIRFLOW_URL" "${MIKEBOM_AIRFLOW_SHA:-}"

echo ""
echo ">>> Audit complete. Analysis JSONs at:"
echo "    $ARTIFACTS_ROOT/tauri/analysis.json"
echo "    $ARTIFACTS_ROOT/airflow/analysis.json"
