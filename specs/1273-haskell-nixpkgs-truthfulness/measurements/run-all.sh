#!/usr/bin/env bash
# Milestone 1273 — run every committed probe, non-zero if any fails (SC-008).
#
# SC-008 requires every probe under this directory to re-run and reproduce its
# recorded finding. One command rather than five, so "did they all still pass"
# is answerable without remembering the list.
#
# `hpack-semantics.sh` needs the real hpack and is SKIPPED unless $HPACK is
# set, because it is the only probe that cannot run offline from the repo
# alone. A skip is reported as a skip; it never counts as a pass.
set -uo pipefail
cd "$(dirname "$0")"

# Absolutised, because this script cd's into its own directory and the
# probes it calls do not. A caller who passes a repo-root-relative path --
# `WAYBILL=./target/debug/waybill`, the natural thing to type -- would
# otherwise have every probe fail to find the binary and report the FINDING
# as unreproducible. That is a harness error wearing a measurement's clothes,
# and it is what happened on the first run of this script.
# Computed AFTER the `cd` above, from the new working directory. Deriving
# it from `$0` here instead fails, because `$0` is relative to the
# caller's cwd and that is no longer where we are.
REPO_ROOT="$(cd ../../.. && pwd)"
WAYBILL="${WAYBILL:-$REPO_ROOT/target/debug/waybill}"
case "$WAYBILL" in
  /*) ;;
  # A relative value was typed relative to wherever the caller stood.
  # `$OLDPWD` is that directory, because the `cd` above set it.
  *)  WAYBILL="${OLDPWD:-$REPO_ROOT}/${WAYBILL#./}" ;;
esac
if [ ! -x "$WAYBILL" ]; then
  echo "no executable waybill at '$WAYBILL'" >&2
  echo "build one with: cargo build -p waybill --bin waybill" >&2
  exit 2
fi
export WAYBILL

fail=0
run() { # <label> <script...>
  local label="$1"; shift
  printf '\n════ %s ════\n' "$label"
  if "$@"; then
    printf '──── %s: PASS\n' "$label"
  else
    printf '──── %s: FAIL (exit %d)\n' "$label" "$?"
    fail=1
  fi
}

run "Probe B — license shapes"        ./license-shapes.sh
run "Probe C — the four issues"       ./reproduce-1273.sh
run "Probe D — SC-003 public library" ./sc003-public-library.sh

if [ -n "${HPACK:-}" ]; then
  run "Probe A — hpack semantics" ./hpack-semantics.sh
else
  printf '\n════ Probe A — hpack semantics ════\n'
  printf '──── SKIPPED: set HPACK=<path> to run. Obtain one with\n'
  printf '       nix build --no-link --print-out-paths nixpkgs#haskellPackages.hpack\n'
fi

printf '\n════ summary ════\n'
[ "$fail" -eq 0 ] && printf 'all probes that ran reproduced their finding\n' \
                  || printf 'at least one probe FAILED\n'
exit "$fail"
