#!/usr/bin/env bash
# Milestone 1274 — reproduce #1283's "the reason does not say so" half.
#
# #1283 was measured at 0.13.1 (`96e7a545`). Part of it has since been fixed,
# and this script exists to separate what is still true from what is not --
# a spec built on a stale defect report describes a defect that may no longer
# exist.
#
# Two otherwise-identical flakes for the same `.cabal`:
#
#   handwritten/  haskellPackages.mkDerivation { ... }     -- evaluates
#   cabal2nix/    haskellPackages.callCabal2nix "..." ./.  -- needs IFD
#
# `callCabal2nix` must BUILD cabal2nix during evaluation to produce the
# derivation, which is import-from-derivation. waybill refuses IFD on purpose
# (#1034, #1114), so the second flake degrades. The question this probe
# answers is not whether it degrades but **whether the document and the log
# say why**.
#
# The handwritten flake is the control. Without it, a degradation on the
# cabal2nix flake is indistinguishable from a broken fixture.
#
# Needs `nix` and network on first run (it fetches the pinned nixpkgs).
# Real coordinates are deliberate and documented: the subject is nix's own
# behaviour.
set -uo pipefail

WAYBILL="${WAYBILL:-./target/debug/waybill}"
[ -x "$WAYBILL" ] || { echo "no waybill at '$WAYBILL'" >&2; exit 2; }
WAYBILL="$(cd "$(dirname "$WAYBILL")" && pwd)/$(basename "$WAYBILL")"
REV="${NIXPKGS_REV:-a799d3e3886da994fa307f817a6bc705ae538eeb}"
SYS="$(nix --extra-experimental-features 'nix-command flakes' \
         eval --raw --impure --expr 'builtins.currentSystem')"
echo "waybill: $("$WAYBILL" --version)   system: $SYS   nixpkgs: ${REV:0:8}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for d in handwritten cabal2nix; do
  mkdir -p "$work/$d/src"
  printf 'module Lib where\n' > "$work/$d/src/Lib.hs"
  cat > "$work/$d/waybill-fixture-ifd.cabal" <<'CABAL'
cabal-version: 2.4
name:          waybill-fixture-ifd
version:       0.1.0
license:       MIT

library
  build-depends: base
  exposed-modules: Lib
  hs-source-dirs: src
CABAL
done

cat > "$work/handwritten/flake.nix" <<NIX
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/$REV";
  outputs = { self, nixpkgs }:
    let p = nixpkgs.legacyPackages.$SYS;
    in {
      packages.$SYS.default = p.haskell.packages.ghc96.mkDerivation {
        pname = "waybill-fixture-ifd";
        version = "0.1.0";
        src = ./.;
        libraryHaskellDepends = [ p.haskell.packages.ghc96.base ];
        license = p.lib.licenses.mit;
      };
    };
}
NIX
cat > "$work/cabal2nix/flake.nix" <<NIX
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/$REV";
  outputs = { self, nixpkgs }:
    let p = nixpkgs.legacyPackages.$SYS;
    in {
      packages.$SYS.default =
        p.haskell.packages.ghc96.callCabal2nix "waybill-fixture-ifd" ./. { };
    };
}
NIX
# A flake must be a git tree for `nix eval` to see its files.
for d in handwritten cabal2nix; do
  (cd "$work/$d" && git init -q && git add -A) 2>/dev/null
done

echo
echo "════ what nix itself says about the cabal2nix flake ════"
# Captured into a variable rather than piped into `grep ... | head -1 | sed
# ... || echo`. In that form `head -1` closes the pipe, `grep` takes SIGPIPE,
# the pipeline's status is non-zero despite having produced the match, and the
# `||` branch fires -- so the first version of this probe printed nix's
# refusal and then "nix did NOT refuse" on the next line. A harness that
# contradicts itself one line apart is worse than one that says nothing.
ifd_line="$( (cd "$work/cabal2nix" && nix --extra-experimental-features 'nix-command flakes' \
  --option allow-import-from-derivation false eval --raw '.#default.drvPath' 2>&1) \
  | grep -oE "because the option 'allow-import-from-derivation' is disabled" | head -1 )"
if [ -n "$ifd_line" ]; then
  echo "  nix: ...cannot build during evaluation $ifd_line"
else
  echo "  nix did NOT refuse import-from-derivation."
  echo "  The control assumption of this probe is wrong -- every finding below"
  echo "  would be about something other than the refusal. Stop here."
  exit 1
fi

report() { # <label> <dir>
  local out="$work/$2.json"
  echo
  echo "════ $1 ════"
  RUST_LOG=info "$WAYBILL" sbom scan --path "$work/$2" --no-deep-hash \
    --nix-closure --format cyclonedx-json --output "cyclonedx-json=$out" \
    2>&1 | sed 's/\x1b\[[0-9;]*m//g' > "$work/$2.log" || true
  grep -oE "nix-closure: (classified|degrading)[^\"]*" "$work/$2.log" | head -1 \
    | cut -c1-110 | sed 's/^/  log: /' || true
  echo -n "  log detail: "
  grep -oE 'detail=.*' "$work/$2.log" | head -1 | cut -c1-120 || echo "(none)"
  python3 - "$out" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
props = {p["name"]: p["value"] for p in (d.get("metadata", {}).get("properties") or [])}
print(f'  document: waybill:nix-closure-degraded = '
      f'{props.get("waybill:nix-closure-degraded", "<absent>")}')
print(f'  components: {len(d.get("components", []))}')
PY
}

report "control: a handwritten flake (must NOT degrade)" handwritten
report "subject: a callCabal2nix flake (degrades by policy)" cabal2nix

echo
echo "════ the finding ════"
echo "  The refusal is deliberate (#1034, #1114). What an operator can read is"
echo "  only \`evaluation-failed\`, and the log's detail is whichever line came"
echo "  first -- a git warning -- rather than the line that explains it. A"
echo "  policy refusal is therefore indistinguishable from a broken flake."
