#!/usr/bin/env bash
# Spec 1380 (#1374), research R1–R4: what derivation evidence separates the
# census groups, and whether the name rules match anything they should not.
#
# Evaluates nixpkgs only (trusted), at the corpus closures' revisions, for
# x86_64-linux (the goldens' system). Nothing is built. No corpus flake is
# evaluated. Usage: member-evidence-probe.sh <scratch-dir>
set -euo pipefail
out="${1:?scratch dir}"; mkdir -p "$out"; cd "$out"
show() { nix derivation show --recursive "github:NixOS/nixpkgs/$1#legacyPackages.x86_64-linux.$2" > "$3"; }
show a799d3e3886da994fa307f817a6bc705ae538eeb haskellPackages.hspec-discover moat-rev.json
show 5b2c2d84341b2afb5647081c1386a80d7a8d8605 llvmPackages_21.clang-unwrapped slacklinker-rev-llvm.json
show 5b2c2d84341b2afb5647081c1386a80d7a8d8605 cargo-auditable slacklinker-rev-cargo.json
all=(moat-rev.json slacklinker-rev-llvm.json slacklinker-rev-cargo.json)

echo "## R1 builder shape: checks vs genuine packages (env keys minus common ones)"
jq -s -r '[.[].derivations|to_entries[]]|unique_by(.key)[]|.value as $d|($d.name//"")
  | select(test("-get-version-|-simple-program-|^mes-libc|^tinycc-.*-(libs|chain)-|^glibc-iconv-[0-9]"))
  | select(($d.env.text//null)==null and ($d.outputs.out.hash//null)==null)
  | [., ((($d.env.builder//"")|split("/")|last)), (($d.env|keys)-["out","name","system","builder","outputs"]|map(select(startswith("deps")|not))|join(",")|.[0:80])]|@tsv' "${all[@]}" | sort -u

echo "## R2 E1: non-fixed-output members whose output another derivation lists in patches"
for f in "${all[@]}"; do jq -r '
 [.derivations[] | ((.env.patches // ""), ((.structuredAttrs.patches // []) | if type=="array" then join(" ") else tostring end)) | split(" ")[] | select(startswith("/nix/store/")) | split("/")[3]] as $pp
 | ($pp|map({key:.,value:1})|from_entries) as $ps
 | .derivations[] | select(.outputs.out.hash==null) | select(any(.outputs[]; (.path//"" | split("/") | last) as $b | $ps[$b])) | .name' "$f"; done | sort | uniq -c

echo "## R3 E2: srcOnly (phases = unpackPhase patchPhase installPhase)"
jq -s -r '[.[].derivations[]]|.[]|select(((.env.phases//.structuredAttrs.phases//"")|tostring)=="unpackPhase patchPhase installPhase")|.name' "${all[@]}" | sort | uniq -c

echo "## R4 name rules over every non-fixed-output, non-text derivation"
jq -s -r '[.[].derivations|to_entries[]]|unique_by(.key)[]|.value|select(.outputs.out.hash==null and (.env.text//null)==null)|.name//""
  | if test("-(get-version|simple-program)-([0-9]|unstable-)") then "check\t\(.)" elif test("(-source|-src-[0-9][^-]*|-vendor)$") then "source-name\t\(.)" else empty end' "${all[@]}" | sort -u
