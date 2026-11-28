#!/usr/bin/env bash
# Spec 1668 T001: build the #1744 fixtures (a)-(g), scan each with <waybill>,
# and summarise the frozen/unused/loose components into <out>/summary.tsv.
set -euo pipefail
wb=$1; out=$2; here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"; work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
cabal() { # dir name deps...
  local d=$1 n=$2; shift 2; mkdir -p "$d"
  { printf 'cabal-version: 2.4\nname: %s\nversion: 0.1.0\nlibrary\n  build-depends: base' "$n"
    for x in "$@"; do printf ', %s' "$x"; done; printf '\n'; } > "$d/$n.cabal"
}
freeze() { printf 'constraints: waybill-fixture-frozen ==4.0.0,\n             waybill-fixture-unused ==1.0.0\n' > "$1/cabal.project.freeze"; }
deps="waybill-fixture-frozen waybill-fixture-loose"
for f in a b c d e f g; do
  r=$work/$f/waybill-fixture-tree; mkdir -p "$r"; freeze "$r"
  case $f in
    a) printf 'packages: app\n' > "$r/cabal.project"; cabal "$r/app" waybill-fixture-app $deps ;;
    b) printf 'packages: libs/*\n' > "$r/cabal.project"; cabal "$r/libs/x" waybill-fixture-x; cabal "$r/libs/y" waybill-fixture-y $deps ;;
    c) printf 'packages: ./app/\n' > "$r/cabal.project"; cabal "$r/app" waybill-fixture-app $deps ;;
    d) printf 'packages: app/waybill-fixture-app.cabal\n' > "$r/cabal.project"; cabal "$r/app" waybill-fixture-app $deps ;;
    e) printf 'packages: app\n' > "$r/cabal.project"; mkdir -p "$r/app"
       printf 'name: waybill-fixture-app\nversion: 0.1.0\nlibrary:\n  dependencies: [base, waybill-fixture-frozen, waybill-fixture-loose]\n' > "$r/app/package.yaml" ;;
    f) printf 'packages: .\n' > "$r/cabal.project"; cabal "$r" waybill-fixture-app $deps ;;
    g) printf 'packages: app ../outside\n' > "$r/cabal.project"; cabal "$r/app" waybill-fixture-app $deps
       cabal "$r/other" waybill-fixture-other waybill-fixture-frozen; cabal "$work/$f/outside" waybill-fixture-outside waybill-fixture-frozen ;;
  esac
  HOME=$work/home WAYBILL_FIXED_TIMESTAMP=2026-01-01T00:00:00Z "$wb" --offline sbom scan --path "$r" --no-deep-hash \
    --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
    --output "cyclonedx-json=$out/$f.cdx.json" --output "spdx-2.3-json=$out/$f.spdx.json" --output "spdx-3-json=$out/$f.spdx3.json" \
    > "$out/$f.log" 2>&1
done
python3 -I -B "$here/summarise.py" "$out" > "$out/summary.tsv" 2> "$out/mains.tsv"
