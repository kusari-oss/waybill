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
# plus a third flake that is not about Haskell at all:
#
#   nixconfig_unrelated/  nixConfig requests the IFD setting, then fails on a
#                         missing attribute
#
# That third one is the FALSE POSITIVE the classifier must not fall for. Its
# stderr names `allow-import-from-derivation` -- in a *warning* -- and then
# fails for an unrelated cause. A classifier keyed on the bare setting name
# anywhere in stderr reports a missing attribute as a policy refusal, which
# turns one uninformative answer into one misleading answer. Real flakes set
# this option (`slack-web` does), so it is on the path of ordinary use.
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

# T002 -- `--raw-stderr` prints, per fixture, every line index, which lines
# match the salient-line predicate, and the byte offset of the IFD clause.
# R1's finding ("the cause is at byte 1320 of 1382, 1,155 past the 300-char
# window") is then reproduced rather than quoted. A number in a spec that
# nobody can re-derive is a number nobody can correct.
RAW_STDERR=0
[ "${1:-}" = "--raw-stderr" ] && RAW_STDERR=1

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
# T001 / R2 -- the false positive. `nixConfig` requests the very setting the
# classifier looks for, and the evaluation then fails for an unrelated cause.
mkdir -p "$work/nixconfig_unrelated"
cat > "$work/nixconfig_unrelated/flake.nix" <<NIX
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/$REV";
  nixConfig.allow-import-from-derivation = true;
  outputs = { self, nixpkgs }:
    let p = nixpkgs.legacyPackages.$SYS;
    in {
      packages.$SYS.default = p.waybillFixtureNoSuchAttribute;
    };
}
NIX

# A flake must be a git tree for `nix eval` to see its files.
for d in handwritten cabal2nix nixconfig_unrelated; do
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

# T002 -- what the selection rule is actually choosing between.
raw_stderr() { # <dir>
  echo
  echo "════ raw nix stderr: $1 ════"
  local err
  err="$( (cd "$work/$1" && nix --extra-experimental-features 'nix-command flakes' \
    --option allow-import-from-derivation false eval --raw '.#default.drvPath' 2>&1) )"
  printf '%s' "$err" > "$work/$1.stderr"
  python3 - "$work/$1.stderr" <<'PY'
import sys, pathlib
CLAUSE = "because the option 'allow-import-from-derivation' is disabled"
raw = pathlib.Path(sys.argv[1]).read_text()
lines = raw.splitlines()
# The predicate in nix/eval/invoke.rs::salient_stderr, verbatim.
def is_err(l):
    return l.lstrip().startswith("error") or ": error:" in l
matches = [i for i, l in enumerate(lines) if is_err(l)]
print(f"  lines={len(lines)}  bytes={len(raw)}")
print(f"  error-predicate matches at line indices: {matches}")
print(f"  position()  would start at line {matches[0] if matches else 0}")
print(f"  rposition() would start at line {matches[-1] if matches else 0}")
off = raw.find(CLAUSE)
if off < 0:
    print("  the IFD clause is ABSENT from this stderr")
else:
    # Where the 300-char window ends under each rule.
    def win_end(start_line):
        return len("\n".join(lines[start_line:])[:300])
    first = matches[0] if matches else 0
    last = matches[-1] if matches else 0
    pre_first = len("\n".join(lines[:first])) + (1 if first else 0)
    pre_last = len("\n".join(lines[:last])) + (1 if last else 0)
    print(f"  the IFD clause starts at byte {off} of {len(raw)}")
    print(f"  under position():  window covers bytes {pre_first}..{pre_first + win_end(first)}"
          f"  -> clause {'INSIDE' if pre_first <= off < pre_first + win_end(first) else 'OUTSIDE'}")
    print(f"  under rposition(): window covers bytes {pre_last}..{pre_last + win_end(last)}"
          f"  -> clause {'INSIDE' if pre_last <= off < pre_last + win_end(last) else 'OUTSIDE'}")
PY
}

if [ "$RAW_STDERR" = 1 ]; then
  raw_stderr cabal2nix
  raw_stderr nixconfig_unrelated
  raw_stderr handwritten
  exit 0
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
  python3 - "$out" "$2" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
which = sys.argv[2]
props = {p["name"]: p["value"] for p in (d.get("metadata", {}).get("properties") or [])}
raw = props.get("waybill:nix-closure-degraded")
comps = len(d.get("components", []))
if raw is None:
    print("  document: waybill:nix-closure-degraded = <absent>")
else:
    v = json.loads(raw)   # m1274: an object, not a bare string
    print(f'  document: reason = {v["reason"]}')
    print(f'  document: detail = {v.get("detail", "<none>")[:110]}')
print(f"  components: {comps}")

# Asserted, not printed and left to the reader (T044 / SC-008). A probe that
# reports a finding it did not reproduce is worse than one that says nothing.
EXPECT = {
    "handwritten":         (None, 304),
    "cabal2nix":           ("import-from-derivation-required", None),
    "nixconfig_unrelated": ("evaluation-failed", None),
}
want_reason, want_comps = EXPECT[which]
got = None if raw is None else json.loads(raw)["reason"]
ok = True
if got != want_reason:
    print(f"  x FAIL expected reason {want_reason!r}, got {got!r}")
    ok = False
if want_comps is not None and comps != want_comps:
    print(f"  x FAIL expected {want_comps} components, got {comps}")
    ok = False
if want_reason == "import-from-derivation-required" and raw is not None:
    det = json.loads(raw).get("detail", "")
    if "is disabled" not in det:
        print(f"  x FAIL the detail must carry nix's clause, got {det[:80]!r}")
        ok = False
    if "uncommitted changes" in det:
        print("  x FAIL the detail is the git warning again")
        ok = False
print("  ok - as expected" if ok else "  x THIS RUN CONTRADICTS THE RECORDED FINDING")
sys.exit(0 if ok else 1)
PY
  [ $? -eq 0 ] || FAILED=1
}

FAILED=0

report "control: a handwritten flake (must NOT degrade)" handwritten
report "subject: a callCabal2nix flake (degrades by policy)" cabal2nix
report "false positive: nixConfig names the setting, fails elsewhere" nixconfig_unrelated

echo
echo "════ the finding, and which side of the fix each half describes ════"
echo "  BEFORE m1274 (see baseline-0.14.0.md): the refusal is deliberate"
echo "  (#1034, #1114), but all an operator could read was \`evaluation-failed\`,"
echo "  and the detail was whichever line came first -- a git warning -- rather"
echo "  than the line that explains it. A policy refusal was indistinguishable"
echo "  from a broken flake."
echo
echo "  AFTER m1274 this run must show, and asserts above:"
echo "    subject         -> reason import-from-derivation-required, detail"
echo "                       carrying nix's own \"is disabled\" clause"
echo "    false positive  -> reason evaluation-failed  (the guard; FR-005a)"
echo "    control         -> no degradation, 304 components  (FR-010 / SC-003)"
echo
echo "  The third fixture is the guard: its stderr names the setting in a"
echo "  WARNING and then fails on a missing attribute. It must keep the"
echo "  generic reason. A classifier that reports it as a refusal has made"
echo "  the output worse, not better."

echo
if [ "${FAILED:-0}" -ne 0 ]; then
  echo "  x at least one expectation above was NOT reproduced."
  exit 1
fi
echo "  ok - every expectation above was reproduced."
