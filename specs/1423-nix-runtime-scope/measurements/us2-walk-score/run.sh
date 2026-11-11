#!/usr/bin/env bash
# T026 (SC-003): scan the five wrapper flakes with the runtime-closure query
# disabled, so every scan takes the reachability walk (US2).
#
# A wrapper `nix` first on PATH answers every `path-info` the way nix answers
# for an output no store holds, and passes everything else to the real nix.
# Nothing else about the scan changes: the flags are gen-sboms.sh's
# (specs/1377-nix-subject-identity/measurements/gen-sboms.sh), deps.dev on.
#
# usage: run.sh   (env: WAYBILL, TARGETS_DIR, WALK_DIR; REAL_NIX optional)
set -uo pipefail
WAYBILL="${WAYBILL:?set WAYBILL to the waybill binary under test}"
TARGETS_DIR="${TARGETS_DIR:?set TARGETS_DIR to the wrapper flakes}"
WALK_DIR="${WALK_DIR:?set WALK_DIR for the walk-derived documents}"
REAL_NIX="${REAL_NIX:-$(command -v nix)}"

wrap="$WALK_DIR/bin"
mkdir -p "$wrap"
cat >"$wrap/nix" <<EOF
#!/bin/sh
# T026: no store holds the output.
if [ "\$1" = "path-info" ]; then
  eval "last=\\\${\$#}"
  echo "don't know how to build these paths:" >&2
  echo "error: path '\$last' is not valid" >&2
  exit 1
fi
exec "$REAL_NIX" "\$@"
EOF
chmod 755 "$wrap/nix"

for t in curl openssh httpie gh ripgrep; do
  o="$WALK_DIR/$t"; mkdir -p "$o"
  s=$(date +%s)
  ( cd "$TARGETS_DIR/$t" && PATH="$wrap:$PATH" "$WAYBILL" sbom scan --no-deep-hash \
      --nix-closure --no-clearly-defined --path . --format cyclonedx-json \
      --output "cyclonedx-json=$o/waybill.cdx.json" >"$o/waybill.log" 2>&1 )
  echo "$t exit=$? $(( $(date +%s)-s ))s"
done
