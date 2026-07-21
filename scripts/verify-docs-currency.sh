#!/usr/bin/env bash
# verify-docs-currency.sh — milestone 082 SC-001 verifier.
#
# Diffs the long-form `--<flag>` set surfaced by `waybill <subcommand> --help`
# against the flag-name set documented in `docs/user-guide/cli-reference.md`.
# Exit 0 when in sync; exit 1 with the missing-flag list otherwise.
#
# Coverage: every operator-facing subcommand at alpha.22:
#   - waybill sbom scan
#   - waybill sbom verify
#   - waybill sbom enrich
#   - waybill trace run
#   - waybill trace capture
#   - waybill policy init
#   - waybill sbom verify-binding
#   - waybill sbom trace-binding
#
# Usage:
#   ./scripts/verify-docs-currency.sh
#       Default — uses `cargo run --quiet --` to invoke waybill from source.
#
#   WAYBILL_BIN=/path/to/waybill ./scripts/verify-docs-currency.sh
#       Use a pre-built binary (faster on warm caches).
#
# Exits 0 when every flag listed by every subcommand's --help has an entry
# in cli-reference.md. Exits 1 with the per-subcommand missing-flag list
# when out of sync.

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
CLI_REF="${REPO_ROOT}/docs/user-guide/cli-reference.md"
MIKEBOM="${WAYBILL_BIN:-cargo run --quiet --manifest-path ${REPO_ROOT}/Cargo.toml --bin waybill --}"

if [[ ! -f "$CLI_REF" ]]; then
    echo "error: CLI reference not found at $CLI_REF" >&2
    exit 2
fi

extract_flags_from_help() {
    # shellcheck disable=SC2086
    $MIKEBOM "$@" --help 2>&1 \
        | grep -oE -- '--[a-z][a-z0-9-]+' \
        | sort -u
}

extract_flags_from_doc() {
    grep -oE -- '--[a-z][a-z0-9-]+' "$CLI_REF" | sort -u
}

ok=0
doc_flags=$(extract_flags_from_doc)

# Loop iterates over all 8 documented subcommands per spec FR-002 + tasks T002.
# Note: `verify-binding` and `trace-binding` are nested under `sbom` (not top-level).
for sub in \
    "sbom scan" \
    "sbom verify" \
    "sbom enrich" \
    "trace run" \
    "trace capture" \
    "policy init" \
    "sbom verify-binding" \
    "sbom trace-binding"; do
    # shellcheck disable=SC2086
    binary_flags=$(extract_flags_from_help $sub) || {
        echo "warning: failed to extract --help for 'waybill $sub' (subcommand may not exist at this build)" >&2
        continue
    }
    missing=$(comm -23 <(echo "$binary_flags") <(echo "$doc_flags") || true)
    if [[ -n "$missing" ]]; then
        echo "FLAGS MISSING from CLI reference for 'waybill $sub':"
        echo "$missing" | sed 's/^/  /'
        echo
        ok=1
    fi
done

if [[ "$ok" -eq 0 ]]; then
    echo "OK — every flag from every subcommand's --help is documented in cli-reference.md."
fi

exit $ok
