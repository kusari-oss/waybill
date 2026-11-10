#!/usr/bin/env bash
# Q6 ground truth from the binaries themselves: which GHC units have code in each static
# executable. Binaries are read from cache.nixos.org with `nix store cat` (not added to the
# local store) into a scratch directory OUTSIDE the repository; only the derived unit lists
# are kept here. Requires q6-static-link.sh to have run first (.rt, drv-closure, cabal-closure).
set -u
cd "$(dirname "$0")"
o=q6-out
SCRATCH=${SCRATCH:-$(mktemp -d)}
# target : file inside its runtime output : runtime path suffix : cabal-closure name
for spec in "shellcheck:bin/shellcheck:-ShellCheck-0.11.0:ShellCheck" \
            "pandoc:bin/pandoc:-pandoc-cli-3.7.0.2:pandoc-cli" \
            "ormolu:bin/ormolu:-ormolu-0.8.0.2-bin:ormolu"; do
  IFS=: read -r t file suffix hp <<< "$spec"
  p=$(grep -- "$suffix\$" "$o/$t.rt")
  nix store cat --store https://cache.nixos.org "$p/$file" > "$SCRATCH/$t.bin"
  nm "$SCRATCH/$t.bin" | python3 -I -B q6-linked-units.py > "$o/$t.linked-units.tsv"
  python3 -I -B q6-binary-vs-rule.py "$o/$t.drv-closure.json" "$o/$t.rt" "$o/cabal-closure.$hp.json" "$o/$t.linked-units.tsv" > "$o/$t.binary-vs-rule.txt"
done
echo "binaries left in $SCRATCH"
