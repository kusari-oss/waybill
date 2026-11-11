#!/usr/bin/env bash
# Q11 (#1458): which outputs does an attribute install, and how big is each
# output's runtime closure?
#
# US1 queried the head of the evaluated derivation's `outputs` (q9 d). For a
# package whose `outputs` starts with `out` but which installs `bin`, that is
# the library output, not the shipped executable. This records, per attribute:
#   - `outputs`, `outputName`, `outputSpecified`, `outPath`;
#   - `meta.outputsToInstall`, whether `nix eval --json` can read it, and
#     whether its strings carry context (q9 b: the installable form of curl
#     fails with "is not a list of strings");
#   - `nix path-info -r` against cache.nixos.org for each output, and for the
#     union of the outputs `meta.outputsToInstall` names.
# Plus the cost of the extra pure eval the fix adds, on the recall-corpus
# postgrest wrapper flake (argv 1, optional).
#
# `--max-jobs 0` on everything: nothing here may build.
# Each case writes <case>.out / .err / .exit / .secs under q11-out/.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
C=https://cache.nixos.org/
o=q11-out; mkdir -p "$o"
N=(--max-jobs 0)
run() { local name=$1; shift; echo "$*" > "$o/$name.cmd"; python3 -I -B timed.py "$o/$name" "$@"; }

for a in postgrest cachix shellcheck curl; do
  run "$a.outputs"           nix eval "${N[@]}" --json "$R#$a.outputs"
  run "$a.outputname"        nix eval "${N[@]}" --raw  "$R#$a.outputName"
  run "$a.outputspecified"   nix eval "${N[@]}" --json "$R#$a" --apply 'd: d.outputSpecified or false'
  run "$a.outpath"           nix eval "${N[@]}" --raw  "$R#$a.outPath"
  run "$a.to-install"        nix eval "${N[@]}" --json "$R#$a.meta.outputsToInstall"
  run "$a.to-install-ctx"    nix eval "${N[@]}" --json "$R#$a.meta.outputsToInstall" --apply 'map builtins.hasContext'
  # The installable form, which is what `nix build` / `nix derivation show`
  # resolve outputs through (q9 b).
  run "$a.drv-show-installable" nix derivation show "${N[@]}" "$R#$a"

  per=()
  for out in $(python3 -I -B -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))))' "$o/$a.outputs.out"); do
    p=$(nix eval "${N[@]}" --raw "$R#$a.$out.outPath")
    echo "$p" > "$o/$a.path.$out"
    run "$a.closure.$out" nix path-info "${N[@]}" -r --store "$C" "$p"
    per+=("$a.closure.$out")
  done

  if [ "$(cat "$o/$a.to-install.exit")" = 0 ]; then
    paths=()
    for out in $(python3 -I -B -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))))' "$o/$a.to-install.out"); do
      paths+=("$(cat "$o/$a.path.$out")")
    done
    run "$a.closure.to-install" nix path-info "${N[@]}" -r --store "$C" "${paths[@]}"
  fi
done

# An attribute that selects one output (`outputSpecified`): what do its
# `outputName` and `meta.outputsToInstall` say? `meta` is shared by all
# outputs, so the two can disagree.
run "curl.out.selection" nix eval "${N[@]}" --json "$R#curl.out" \
  --apply 'd: { outputSpecified = d.outputSpecified or false; outputName = d.outputName; outputsToInstall = d.meta.outputsToInstall; }'
# Is curl's installable failure (q9 b) the evaluation cache rather than the
# value? `nix eval --json` above reads the list without error.
run "curl.drv-show-installable-no-eval-cache" nix derivation show "${N[@]}" --option eval-cache false "$R#curl"
# The selection expression the fix evaluates (runtime.rs `SELECTION_EXPR`),
# against nixpkgs directly.
SEL='d: if d.outputSpecified or false then { source = "output-specified"; outputs = [ d.outputName ]; } else { source = "outputs-to-install"; outputs = d.meta.outputsToInstall or null; }'
for a in postgrest cachix shellcheck curl curl.out; do
  run "$a.selection-expr" nix eval "${N[@]}" --json --option allow-import-from-derivation false "$R#$a" --apply "$SEL"
done

# The extra evaluation, on a wrapper flake shaped like the corpus fixtures.
if [ -n "${1:-}" ] && [ -f "$1/flake.nix" ]; then
  ref="path:$(cd "$1" && pwd)#packages.aarch64-darwin.default"
  for i in 1 2 3; do
    run "wrapper.to-install.$i" nix eval "${N[@]}" --json \
      --option allow-import-from-derivation false "$ref.meta.outputsToInstall"
    run "wrapper.selection-expr.$i" nix eval "${N[@]}" --json \
      --option allow-import-from-derivation false "$ref" --apply "$SEL"
  done
fi

# Summary: one row per (attribute, output), and the outputsToInstall union.
{
  printf 'attr\toutputs\toutputName\toutPath-output\tmeta.outputsToInstall\tto-install-has-context\tinstallable-drv-show-exit\n'
  for a in postgrest cachix shellcheck curl; do
    op=$(cat "$o/$a.outpath.out"); head_out=""
    for f in "$o/$a".path.*; do [ "$(cat "$f")" = "$op" ] && head_out=${f##*.}; done
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$a" "$(cat "$o/$a.outputs.out")" "$(cat "$o/$a.outputname.out")" \
      "$head_out" "$(cat "$o/$a.to-install.out")$(tr -d '\n' < "$o/$a.to-install.err")" \
      "$(cat "$o/$a.to-install-ctx.out")" "$(cat "$o/$a.drv-show-installable.exit")"
  done
  printf '\nattr\toutput\tpath-info -r exit\tpaths\n'
  for a in postgrest cachix shellcheck curl; do
    for f in "$o/$a".closure.*.exit; do
      b=${f%.exit}; printf '%s\t%s\t%s\t%s\n' "$a" "${b##*.closure.}" "$(cat "$f")" "$(grep -c . "$b.out")"
    done
  done
  printf '\ncurl.out selection: %s\n' "$(cat "$o/curl.out.selection.out")"
  printf 'curl installable, eval-cache off: exit %s %s\n' \
    "$(cat "$o/curl.drv-show-installable-no-eval-cache.exit")" "$(tr -d '\n' < "$o/curl.drv-show-installable-no-eval-cache.err")"
  printf '\nselection expression\n'
  for a in postgrest cachix shellcheck curl curl.out; do
    printf '%s\texit %s\t%s\n' "$a" "$(cat "$o/$a.selection-expr.exit")" "$(cat "$o/$a.selection-expr.out")"
  done
  if ls "$o"/wrapper.to-install.*.secs >/dev/null 2>&1; then
    printf '\nextra eval on the postgrest wrapper, .meta.outputsToInstall (s): %s\n' "$(cat "$o"/wrapper.to-install.*.secs | tr '\n' ' ')"
    printf 'result: %s\n' "$(cat "$o/wrapper.to-install.1.out")"
    printf 'extra eval on the postgrest wrapper, selection expression (s): %s\n' "$(cat "$o"/wrapper.selection-expr.*.secs | tr '\n' ' ')"
    printf 'result: %s\n' "$(cat "$o/wrapper.selection-expr.1.out")"
  fi
} > "$o/summary.tsv"
cat "$o/summary.tsv"
