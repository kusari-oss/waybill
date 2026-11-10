#!/usr/bin/env bash
# Q7: why hlint's runtime closure holds -doc outputs, the C toolchain and test frameworks.
# `nix why-depends --precise --store https://cache.nixos.org` reads the referrer's NAR from the
# cache (nothing is added to the local store) and prints the file holding the reference.
# Chosen hops have small referrers; ghc (1.4 GB NAR) is inspected via one file instead.
set -u
cd "$(dirname "$0")"
o=q7-out
p() { grep -- "-$1\$" "$o/hlint.rt"; }
C=https://cache.nixos.org
hop() { local tag=$1 a=$2 b=$3; python3 -I -B timed.py "$o/why.$tag" nix why-depends --precise --store "$C" "$(p "$a")" "$(p "$b")"; }
hop case-insensitive--test-framework case-insensitive-1.2.1.0 test-framework-0.8.2.3
hop yaml--hspec                      yaml-0.11.11.2           hspec-2.11.17
hop hlint-doc--aeson-doc             hlint-3.10-doc           aeson-2.2.4.1-doc
hop unordered-containers--ChasingBottoms unordered-containers-0.2.20.1 ChasingBottoms-1.3.1.17
hop hlint--tasty                     hlint-3.10               tasty-1.5.4
hop hlint--ghc                       hlint-3.10               ghc-9.10.3
hop hlint--hlint-doc                 hlint-3.10               hlint-3.10-doc
# ghc -> C toolchain: ghc's settings file names the C compiler / linker / ar it calls at runtime
G=$(p ghc-9.10.3)
nix store ls --store "$C" -R "$G" > "$o/ghc.ls" 2>&1
S=$(grep -E '/lib/settings$' "$o/ghc.ls" | sed 's#^\./##' | head -n 1)
echo "$S" > "$o/ghc.settings.path"
nix store cat --store "$C" "$G/$S" > "$o/ghc.settings" 2>&1
