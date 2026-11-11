#!/usr/bin/env bash
# Q2: querying a substituter for a closure without building.
# Each case writes <case>.out / <case>.err / <case>.exit under q2-out/.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q2-out; mkdir -p "$o"
run() { local name=$1; shift; echo "$*" > "$o/$name.cmd"; python3 -I -B timed.py "$o/$name" "$@"; }

SC=$(nix eval --raw "$R#shellcheck.outPath")    # bin output; absent from this host's store
CURL=$(nix eval --raw "$R#curl.outPath")        # bin output; present in this host's store
echo "$SC" > "$o/shellcheck.outpath"; echo "$CURL" > "$o/curl.outpath"

# (a) no --store: does path-info consult substituters for a path not in the local store?
run a-default-store-absent      nix path-info -r "$SC"
run a-default-store-absent-json nix path-info --json -r "$SC"
run a-default-store-present     nix path-info -r "$CURL"

# (b) --store <url> for each configured substituter (as `nix config show substituters` prints them)
i=0; for s in $(nix config show substituters); do i=$((i+1))
  run "b-store-$i" nix path-info -r --store "$s" "$SC"; echo "$s" > "$o/b-store-$i.url"
done
run b-store-nixos-noslash nix path-info -r --store https://cache.nixos.org "$SC"

# (c) a single invocation over all substituters?
#   - two --store flags: which one wins?
run c-two-stores-det-last nix path-info -r --store https://cache.nixos.org --store https://install.determinate.systems "$SC"
run c-two-stores-nixos-last nix path-info -r --store https://install.determinate.systems --store https://cache.nixos.org "$SC"
#   - nix build --dry-run on the store path: consults every configured substituter, builds nothing
run c-build-dry-run nix build --dry-run "$SC"
run c-build-dry-run-json nix build --dry-run --json "$SC"
run c-build-dry-run-present nix build --dry-run "$CURL"
run c-nix-store-realise-dry-run nix-store --realise --dry-run "$SC"

# (d) signatures
run d-json-sigs nix path-info --json --json-format 1 -r --sigs --store https://cache.nixos.org "$SC"
run d-json-nosigs-flag nix path-info --json --json-format 1 -r --store https://cache.nixos.org "$SC"
run d-json-v2 nix path-info --json --json-format 2 -r --store https://cache.nixos.org "$SC"
#   path-info with an empty / bogus trusted key list: does the query verify signatures?
run d-bogus-keys-pathinfo nix path-info -r --store https://cache.nixos.org --option trusted-public-keys 'bogus-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=' "$SC"
run d-verify-trusted nix store verify --no-contents -r --store https://cache.nixos.org "$SC"
run d-verify-bogus-keys nix store verify --no-contents -r --store https://cache.nixos.org --option trusted-public-keys 'bogus-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=' "$SC"

# (e) a path that is not in any cache: a trivially modified package
UNC=$(nix eval --raw --impure --expr "((builtins.getFlake \"$R\").legacyPackages.aarch64-darwin.hello.overrideAttrs (_: { WAYBILL_PROBE = \"1423\"; })).outPath")
echo "$UNC" > "$o/uncached.outpath"
run e-uncached-default-store nix path-info -r "$UNC"
run e-uncached-store-nixos   nix path-info -r --store https://cache.nixos.org "$UNC"
run e-uncached-store-nixos-json nix path-info --json -r --store https://cache.nixos.org "$UNC"
run e-uncached-store-nixos-repeat nix path-info -r --store https://cache.nixos.org "$UNC"
run e-uncached-store-det     nix path-info -r --store https://install.determinate.systems "$UNC"
run e-uncached-build-dry-run nix build --dry-run "$UNC"
