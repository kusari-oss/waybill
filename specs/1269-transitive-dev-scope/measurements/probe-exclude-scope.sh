#!/bin/sh
# T043 — measures what `--exclude-scope` actually drops, before and
# after the #1209 relocation of the filter below the propagation pass.
#
# Replicates the m195 corpus-harness invocation exactly
# (`waybill-cli/tests/corpus_harness_195/harness.rs`, the `--root-name`
# / `--root-version` / `--offline` / `WAYBILL_NIXPKGS_CACHE` set) and
# then adds one flag. Matching the harness is load-bearing: an
# invocation missing `--root-name` produced 62 components where the
# harness produces 251, and that discrepancy was once misread as a host
# difference (memory note `feedback_match_harness_invocation`).
#
# Usage: probe-exclude-scope.sh <target> <scope> <label>
#   target : go-opentelemetry | haskell-language-server
#   scope  : dev | build | test   (the values ExcludeScopeArg accepts)
#   label  : output directory name, e.g. `before` or `after`
#
# Requires the corpus cache to be hydrated, which the m195 harness does
# on its first run. Reads it; never writes to it.
set -e
TARGET="$1"; SCOPE="$2"; LABEL="$3"
[ -n "$TARGET" ] && [ -n "$SCOPE" ] && [ -n "$LABEL" ] || {
    echo "usage: $0 <target> <dev|build|test> <label>" >&2; exit 2; }

BIN="${WAYBILL_BIN:-./target/debug/waybill}"
CACHE_ROOT="${WAYBILL_CORPUS_CACHE_DIR:-$HOME/.cache/waybill}"
OUT="${OUT_DIR:-./target/measurements/exclude-scope}/$LABEL"

# source-id/pin pairs from waybill-cli/tests/corpus_harness_195/manifest.rs
case "$TARGET" in
  go-opentelemetry)
    SRC=792f8c5a23a5766e
    PIN=55899e389dcbc559393069a2a46dac41a88b0772
    SHORT=55899e38 ;;
  haskell-language-server)
    SRC=ac3746a76d11c911
    PIN=1b4b3c6bdd2bf8d1e1182e2e770f5dea9198db80
    SHORT=1b4b3c6b ;;
  *) echo "unknown target $TARGET" >&2; exit 2 ;;
esac

REPO="$CACHE_ROOT/corpus/$SRC/$PIN/repo"
test -d "$REPO" || { echo "corpus cache missing: $REPO" >&2; exit 1; }

rm -rf "$OUT"; mkdir -p "$OUT"
for mode in baseline excl; do
  set -- --offline sbom scan --path "$REPO" \
      --format cyclonedx-json \
      --output "cyclonedx-json=$OUT/$mode.cdx.json" \
      --root-name "$TARGET" --root-version "$SHORT"
  if [ "$mode" = excl ]; then
      WAYBILL_NIXPKGS_CACHE="$CACHE_ROOT/corpus-nixpkgs" \
          "$BIN" --exclude-scope "$SCOPE" "$@" > "$OUT/$mode.log" 2>&1
  else
      WAYBILL_NIXPKGS_CACHE="$CACHE_ROOT/corpus-nixpkgs" \
          "$BIN" "$@" > "$OUT/$mode.log" 2>&1
  fi
done
echo "wrote $OUT"
echo "now: python3 specs/1269-transitive-dev-scope/measurements/exclude-scope-survivors.py $OUT $SCOPE"
