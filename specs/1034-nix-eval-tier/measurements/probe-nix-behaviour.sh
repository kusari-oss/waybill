#!/usr/bin/env bash
# Probes behind specs/1034-nix-eval-tier/research.md (R2, R3, R4).
# Re-run when the `nix` on the bench changes — an undocumented behaviour is one
# that can move without notice.
#
# Usage: bash probe-nix-behaviour.sh [scratch-dir]
set -u
SCRATCH="${1:-$(mktemp -d)}"
REV=cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73
mkdir -p "$SCRATCH"

echo "nix: $(nix --version)"
echo "host: $(uname -sm)"
echo

# ---------------------------------------------------------------- R3 (safety)
echo "## R3a — does evaluation run repository-controlled code?"
D="$SCRATCH/ifd"; rm -rf "$D"; mkdir -p "$D"; cd "$D"
cat > flake.nix <<'NIX'
{
  outputs = _: {
    packages.__SYSTEM__.default =
      let built = derivation {
            name = "ifd-marker"; system = "__SYSTEM__"; builder = "/bin/sh";
            args = [ "-c" "echo ARBITRARY-BUILD-RAN >&2; echo '\"pwned-during-eval\"' > $out" ];
          };
      in { name = "probe"; version = import built; };
  };
}
NIX
SYS=$(nix eval --raw --impure --expr 'builtins.currentSystem')
sed -i.bak "s/__SYSTEM__/$SYS/g" flake.nix && rm -f flake.nix.bak
git init -q . && git add -A
echo "-- default settings (EXPECT: builds, i.e. runs repo-supplied /bin/sh):"
nix eval --no-write-lock-file ".#packages.$SYS.default.version" 2>&1 \
  | grep -vE 'uncommitted changes' | tail -3
echo "-- allow-import-from-derivation=false (EXPECT: refused):"
nix store delete /nix/store/*-ifd-marker >/dev/null 2>&1
nix eval --no-write-lock-file --option allow-import-from-derivation false \
  ".#packages.$SYS.default.version" 2>&1 | grep -E 'cannot build|error:' | tail -2
nix store delete /nix/store/*-ifd-marker >/dev/null 2>&1
echo

echo "## R3b — is an unsupported setting a SILENT no-op?"
echo "-- EXPECT: a warning, output '1', and rc=0:"
nix eval --expr '1' --impure --option definitely-not-a-real-option true 2>&1 | head -2
nix eval --expr '1' --impure --option definitely-not-a-real-option true >/dev/null 2>&1
echo "   rc=$?"
echo "-- EXPECT: pre-flight DOES distinguish supported from unsupported:"
nix config show --option allow-import-from-derivation false 2>/dev/null \
  | grep 'allow-import-from-derivation'
nix config show --option definitely-not-a-real-option false 2>&1 \
  | grep -c 'definitely-not-a-real-option =' | sed 's/^/   times the bogus setting appears as a setting: /'
echo

# ------------------------------------------------------------------ R2 (pure)
echo "## R2 — does a pinned revision evaluate WITHOUT --impure?"
echo "-- EXPECT: a version string, no error:"
time nix eval --json --option allow-import-from-derivation false \
  --expr "let p = (builtins.getFlake \"github:NixOS/nixpkgs/$REV\").legacyPackages.$SYS; in p.haskellPackages.aeson.version" 2>&1 | tail -2
echo "-- EXPECT: currentSystem is UNAVAILABLE in pure mode:"
nix eval --raw --expr 'builtins.currentSystem' 2>&1 | head -2
echo

# ------------------------------------------------------------- R4 (unbounded)
echo "## R4a — is runaway RECURSION bounded? (EXPECT: yes, max-call-depth)"
nix eval --expr 'let f = x: f (x + 1); in f 0' --impure 2>&1 | head -1
nix config show | grep -E '^(max-call-depth|timeout) '
echo
echo "## R4b — is shallow long-running evaluation TIME-bounded? (EXPECT: no)"
EXPR='builtins.foldl'"'"' (acc: i: acc + (builtins.foldl'"'"' (a: b: a + b) 0 (builtins.genList (x: x) 40000))) 0 (builtins.genList (x: x) 40000)'
nix eval --expr "$EXPR" --impure >/dev/null 2>&1 &
pid=$!; t=0
while kill -0 $pid 2>/dev/null; do
  sleep 5; t=$((t+5))
  if [ $t -ge 45 ]; then
    kill -9 $pid 2>/dev/null
    echo "   STILL RUNNING at ${t}s -> nix imposed no time bound; killed externally"
    break
  fi
done
if kill -0 $pid 2>/dev/null; then :; elif [ $t -lt 45 ]; then
  echo "   completed in under ${t}s -- RE-SCALE THE EXPRESSION, this probe proved nothing"
fi

# ------------------------------------------------- flake-supplied nix settings
echo
echo "## R9 — can a flake re-enable IFD through its own nixConfig?"
D="$SCRATCH/nixcfg"; rm -rf "$D"; mkdir -p "$D"; cd "$D"
cat > flake.nix <<NIX
{
  nixConfig.allow-import-from-derivation = true;
  outputs = _: {
    packages.__SYS__.default = import (derivation {
      name = "waybill-nixcfg-marker"; system = "__SYS__"; builder = "/bin/sh";
      args = [ "-c" "echo '\"ifd-ran-anyway\"' > \$out" ];
    });
  };
}
NIX
sed -i.bak "s/__SYS__/$SYS/g" flake.nix && rm -f flake.nix.bak
git init -q . && git add -A
echo "-- waybill's invocation, no --accept-flake-config (EXPECT: refused):"
nix eval --no-write-lock-file --option allow-import-from-derivation false \
  ".#packages.$SYS.default" 2>&1 | grep -E 'cannot build|disabled' | tail -1
echo "-- with --accept-flake-config, which waybill never passes (EXPECT: it runs):"
nix eval --no-write-lock-file --accept-flake-config \
  --option allow-import-from-derivation false \
  ".#packages.$SYS.default" 2>&1 | grep -E 'ifd-ran-anyway|cannot build' | tail -1
nix store delete /nix/store/*waybill-nixcfg-marker* >/dev/null 2>&1
echo "   (the refusal holds only because that flag is absent)"
