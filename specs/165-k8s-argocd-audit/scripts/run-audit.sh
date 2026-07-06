#!/usr/bin/env bash
# Milestone 165 audit runner — clone a target + run 5 scans against it.
# Does NOT run analyze.py or SPDX validation (those are separate tasks
# T012/T013/T019/T020 in tasks.md).
#
# Usage:
#   ./run-audit.sh --target <kubernetes|argocd> --workdir <path>
#
# Emits progress + wall-clock timings via `time` per scan.
# Assumes:
#   - trivy 0.71.1 at ~/.local/bin/trivy (or on PATH)
#   - syft 1.44.0 on PATH
#   - mikebom release build at ./target/release/mikebom
#
# The wall-clock timings are captured in `<workdir>/timing.txt` so
# analyze.py + report writer can pick them up.
set -euo pipefail

TARGET=""
WORKDIR=""

usage() {
    cat <<'EOF' >&2
Usage: run-audit.sh --target <kubernetes|argocd> --workdir <path>
EOF
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            TARGET="$2"; shift 2 ;;
        --workdir)
            WORKDIR="$2"; shift 2 ;;
        --help|-h)
            usage ;;
        *)
            echo "unknown arg: $1" >&2; usage ;;
    esac
done

if [ -z "$TARGET" ] || [ -z "$WORKDIR" ]; then
    usage
fi

case "$TARGET" in
    kubernetes)
        UPSTREAM="https://github.com/kubernetes/kubernetes.git" ;;
    argocd)
        UPSTREAM="https://github.com/argoproj/argo-cd.git" ;;
    *)
        echo "invalid --target: $TARGET (must be kubernetes|argocd)" >&2
        exit 1 ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MIKEBOM_BIN="$REPO_ROOT/target/release/mikebom"
ARTIFACTS_DIR="$REPO_ROOT/specs/165-k8s-argocd-audit/artifacts/$TARGET"
TIMING_FILE="$WORKDIR/timing.txt"

if [ ! -x "$MIKEBOM_BIN" ]; then
    echo "mikebom release build not found at $MIKEBOM_BIN" >&2
    echo "Run: cargo +stable build --release -p mikebom" >&2
    exit 1
fi

# Choose trivy: prefer ~/.local/bin/trivy at 0.71.1 pin, then system PATH
if [ -x "$HOME/.local/bin/trivy" ]; then
    TRIVY="$HOME/.local/bin/trivy"
elif command -v trivy > /dev/null; then
    TRIVY="$(command -v trivy)"
else
    echo "trivy not found (needs 0.71.1 at ~/.local/bin/trivy or on PATH)" >&2
    exit 1
fi

if ! command -v syft > /dev/null; then
    echo "syft not found (needs 1.44.0 on PATH)" >&2
    exit 1
fi

mkdir -p "$WORKDIR" "$ARTIFACTS_DIR"
echo "target=$TARGET" > "$TIMING_FILE"
echo "audit_date=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 1: clone (or reuse if already present in workdir)
# ---------------------------------------------------------------
CLONE_DIR="$WORKDIR/$TARGET"
if [ -d "$CLONE_DIR/.git" ]; then
    echo "→ clone reuse: $CLONE_DIR" >&2
else
    echo "→ cloning $UPSTREAM (depth=1) into $CLONE_DIR" >&2
    START=$(date +%s)
    git clone --depth 1 "$UPSTREAM" "$CLONE_DIR"
    END=$(date +%s)
    echo "clone_seconds=$((END - START))" >> "$TIMING_FILE"
fi

cd "$CLONE_DIR"
COMMIT_SHA=$(git rev-parse HEAD)
CLONE_SIZE_BYTES=$(du -sk . | awk '{print $1 * 1024}')
echo "commit_sha=$COMMIT_SHA" >> "$TIMING_FILE"
echo "clone_size_bytes=$CLONE_SIZE_BYTES" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 2: mikebom CDX
# ---------------------------------------------------------------
echo "→ mikebom CDX scan" >&2
START=$(date +%s)
"$MIKEBOM_BIN" --offline sbom scan \
    --path "$CLONE_DIR" \
    --output "$ARTIFACTS_DIR/mikebom.cdx.json" \
    --no-deep-hash \
    2> "$ARTIFACTS_DIR/mikebom.cdx.log" || {
    echo "  ⚠️  mikebom CDX scan FAILED — see $ARTIFACTS_DIR/mikebom.cdx.log" >&2
    exit 2
}
END=$(date +%s)
echo "mikebom_cdx_seconds=$((END - START))" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 3: mikebom SPDX 2.3
# ---------------------------------------------------------------
echo "→ mikebom SPDX 2.3 scan" >&2
START=$(date +%s)
"$MIKEBOM_BIN" --offline sbom scan \
    --path "$CLONE_DIR" \
    --format spdx-2.3-json \
    --output "$ARTIFACTS_DIR/mikebom.spdx23.json" \
    --no-deep-hash \
    2> "$ARTIFACTS_DIR/mikebom.spdx23.log" || {
    echo "  ⚠️  mikebom SPDX 2.3 scan FAILED — see $ARTIFACTS_DIR/mikebom.spdx23.log" >&2
    exit 2
}
END=$(date +%s)
echo "mikebom_spdx23_seconds=$((END - START))" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 4: mikebom SPDX 3
# ---------------------------------------------------------------
echo "→ mikebom SPDX 3 scan" >&2
START=$(date +%s)
"$MIKEBOM_BIN" --offline sbom scan \
    --path "$CLONE_DIR" \
    --format spdx-3-json \
    --output "$ARTIFACTS_DIR/mikebom.spdx3.json" \
    --no-deep-hash \
    2> "$ARTIFACTS_DIR/mikebom.spdx3.log" || {
    echo "  ⚠️  mikebom SPDX 3 scan FAILED — see $ARTIFACTS_DIR/mikebom.spdx3.log" >&2
    exit 2
}
END=$(date +%s)
echo "mikebom_spdx3_seconds=$((END - START))" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 5: trivy CDX
# ---------------------------------------------------------------
echo "→ trivy CDX scan" >&2
START=$(date +%s)
"$TRIVY" fs \
    --format cyclonedx \
    --output "$ARTIFACTS_DIR/trivy.cdx.json" \
    "$CLONE_DIR" \
    2> "$ARTIFACTS_DIR/trivy.cdx.log" || {
    echo "  ⚠️  trivy scan FAILED — see $ARTIFACTS_DIR/trivy.cdx.log" >&2
    # non-fatal: continue with what we have; report will flag it
}
END=$(date +%s)
echo "trivy_cdx_seconds=$((END - START))" >> "$TIMING_FILE"

# ---------------------------------------------------------------
# Step 6: syft CDX
# ---------------------------------------------------------------
echo "→ syft CDX scan" >&2
START=$(date +%s)
syft "$CLONE_DIR" --output cyclonedx-json="$ARTIFACTS_DIR/syft.cdx.json" \
    2> "$ARTIFACTS_DIR/syft.cdx.log" || {
    echo "  ⚠️  syft scan FAILED — see $ARTIFACTS_DIR/syft.cdx.log" >&2
    # non-fatal: continue
}
END=$(date +%s)
echo "syft_cdx_seconds=$((END - START))" >> "$TIMING_FILE"

echo "" >&2
echo "✓ Audit runner complete for target=$TARGET" >&2
echo "  Artifacts: $ARTIFACTS_DIR" >&2
echo "  Timings:   $TIMING_FILE" >&2
echo "" >&2
cat "$TIMING_FILE" >&2
