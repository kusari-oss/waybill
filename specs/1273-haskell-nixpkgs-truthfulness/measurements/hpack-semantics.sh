#!/usr/bin/env bash
# Milestone 1273 Phase 0 — measure hpack's rendering rules for the FR-001a
# subset, rather than read them from its documentation.
#
# FR-001a names four constructs as modelled: top-level `dependencies`,
# per-component `dependencies`, `source-dirs`, and `when:` conditional
# blocks. It then requires that "the exact semantics of each ... MUST be
# measured before a requirement depends on them, not read from its
# documentation alone."
#
# This is that measurement. It runs the real hpack over synthetic manifests
# and prints the rendered stanza VERBATIM. hpack is the oracle: the question
# is not what waybill should do but what the file means, and hpack's output
# is the definition of that.
#
# # A mistake this script was rewritten to stop making
#
# The first version filtered the rendered `.cabal` through awk, matching
# `^  build-depends:` and its `^    ` continuation lines. A cabal `if` block
# nests its own build-depends one level deeper, so every conditional
# dependency was invisible — and cases C5 and C7, whose entire subject is
# the conditional, printed as though hpack had dropped the dependency. Three
# cases that must differ printed identically, which is the signature of a
# broken harness rather than a uniform system. The filter is gone: this
# prints the stanza as hpack wrote it, and the reader does the reading.
#
# Manifest names are synthetic (`waybill-fixture-*`) per the project's
# fixture rule. Real package names appear only as DEPENDENCY names, because
# name propagation is the thing measured and a dependency's name is the
# observable.
#
# Usage:
#   nix build --no-link --print-out-paths nixpkgs#haskellPackages.hpack
#   HPACK=<that>/bin/hpack ./hpack-semantics.sh
set -euo pipefail

HPACK="${HPACK:-hpack}"
command -v "$HPACK" >/dev/null 2>&1 || { echo "no hpack at '$HPACK'" >&2; exit 1; }
echo "hpack: $("$HPACK" --version)"
echo

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Render one manifest; print every stanza of the result verbatim.
# `default-language` is dropped: it is in every stanza and carries nothing.
case_() { # <name> <<heredoc
  local name="$1"
  local d="$work/$name"
  mkdir -p "$d/src" "$d/app" "$d/other"
  cat > "$d/package.yaml"
  : > "$d/src/Lib.hs"; : > "$d/app/Main.hs"; : > "$d/other/Other.hs"
  echo "──── $name ────────────────────────────────────────────"
  if ! (cd "$d" && "$HPACK" --force >/dev/null 2>"$d/err"); then
    echo "  hpack REFUSED the manifest:"
    sed 's/^/    /' "$d/err"
    echo
    return 0
  fi
  [ -s "$d/err" ] && { echo "  hpack warned:"; sed 's/^/    /' "$d/err"; }
  sed -nE '/^(library|executable |test-suite |benchmark |foreign-library )/,$p' "$d"/*.cabal \
    | grep -v '^  default-language:' | sed 's/^/  /'
  echo
}

# ═══ Inheritance ═══════════════════════════════════════════════════
# C1 — does a top-level `dependencies` list reach every component kind,
#      including tests and benchmarks?
case_ c1-toplevel-inheritance <<'YAML'
name: waybill-fixture-c1
version: 0.1.0
dependencies:
  - base
  - text
library:
  source-dirs: src
executables:
  cli:
    main: Main.hs
    source-dirs: app
tests:
  spec:
    main: Main.hs
    source-dirs: app
benchmarks:
  bench:
    main: Main.hs
    source-dirs: app
YAML

# C2 — is a per-component list a REPLACEMENT of the top-level one, or an
#      ADDITION to it?
case_ c2-percomponent-merge <<'YAML'
name: waybill-fixture-c2
version: 0.1.0
dependencies:
  - base
library:
  source-dirs: src
  dependencies:
    - containers
executables:
  cli:
    main: Main.hs
    source-dirs: app
    dependencies:
      - aeson
YAML

# ═══ Constraints on a repeated name ════════════════════════════════
# C3 — constrained at top level, bare per component.
case_ c3-constrained-then-bare <<'YAML'
name: waybill-fixture-c3
version: 0.1.0
dependencies:
  - base >= 4 && < 5
library:
  source-dirs: src
  dependencies:
    - base
YAML

# C4 — bare at top level, constrained per component.
case_ c4-bare-then-constrained <<'YAML'
name: waybill-fixture-c4
version: 0.1.0
dependencies:
  - base
library:
  source-dirs: src
  dependencies:
    - base >= 4 && < 5
YAML

# C5 — two different constraints: do they intersect, or does one win?
case_ c5-two-constraints <<'YAML'
name: waybill-fixture-c5
version: 0.1.0
dependencies:
  - text >= 1.0
library:
  source-dirs: src
  dependencies:
    - text < 3
YAML

# C6 — the same question inside ONE list, with no inheritance involved.
case_ c6-duplicate-in-one-list <<'YAML'
name: waybill-fixture-c6
version: 0.1.0
dependencies:
  - text >= 1.0
  - text < 3
library:
  source-dirs: src
YAML

# ═══ when: ═════════════════════════════════════════════════════════
# C7 — condition is the literal `true`.
case_ c7-when-literal-true <<'YAML'
name: waybill-fixture-c7
version: 0.1.0
dependencies:
  - base
when:
  - condition: true
    dependencies:
      - unix
library:
  source-dirs: src
YAML

# C8 — condition is the literal `false`.
case_ c8-when-literal-false <<'YAML'
name: waybill-fixture-c8
version: 0.1.0
dependencies:
  - base
when:
  - condition: false
    dependencies:
      - unix
library:
  source-dirs: src
YAML

# C9 — conditions that CANNOT be decided from the manifest alone: one on
#      the build host, one on a declared flag with a default.
case_ c9-when-undecidable <<'YAML'
name: waybill-fixture-c9
version: 0.1.0
dependencies:
  - base
flags:
  fast:
    description: go faster
    manual: true
    default: false
library:
  source-dirs: src
  when:
    - condition: os(darwin)
      dependencies:
        - unix
    - condition: flag(fast)
      dependencies:
        - vector
YAML

# ═══ source-dirs ═══════════════════════════════════════════════════
# C10 — can `source-dirs` change the dependency set?
case_ c10-source-dirs <<'YAML'
name: waybill-fixture-c10
version: 0.1.0
library:
  source-dirs:
    - src
    - other
  dependencies:
    - base
YAML

# ═══ The two unmodelled constructs (FR-001b / FR-001c) ═════════════
# C11 — `defaults:`. FR-001c forbids waybill from fetching for it. What
#       does hpack itself do? The URL is deliberately non-existent, so the
#       observable is hpack's failure mode, not someone's repository.
case_ c11-defaults <<'YAML'
name: waybill-fixture-c11
version: 0.1.0
defaults:
  github: waybill-fixture/does-not-exist
  ref: main
library:
  source-dirs: src
  dependencies:
    - base
YAML

# C12 — `!include` of a sibling file. FR-001b announces this alongside
#       `defaults:`; measure whether it is the same KIND of construct.
d="$work/c12"; mkdir -p "$d/src"; : > "$d/src/Lib.hs"
printf -- '- base\n- text\n' > "$d/common.yaml"
cat > "$d/package.yaml" <<'YAML'
name: waybill-fixture-c12
version: 0.1.0
library:
  source-dirs: src
  dependencies: !include "common.yaml"
YAML
echo "──── c12-include-sibling ────────────────────────────────"
if (cd "$d" && "$HPACK" --force >/dev/null 2>"$d/err"); then
  sed -n '/^library/,$p' "$d"/*.cabal | grep -v '^  default-language:' | sed 's/^/  /'
else
  echo "  hpack REFUSED the manifest:"; sed 's/^/    /' "$d/err"
fi
