# Quickstart: Name a CMake git fetch with an unresolved reference after its repository

## See the change on a synthetic tree

```sh
d=$(mktemp -d)
cat > "$d/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.24)
project(waybill-fixture-app)
include(FetchContent)
FetchContent_Declare(waybill-fixture-target
  GIT_REPOSITORY https://github.com/Waybill-Org/Waybill-Fixture-Dep.git
  GIT_TAG ${WAYBILL_FIXTURE_UNSET})
EOF
cargo run -q -p waybill -- sbom scan --path "$d" --offline --format cyclonedx-json --output "$d/out.cdx.json"
jq -r '.components[] | select(.purl | startswith("pkg:github/")) | [.purl, (.properties[] | select(.name=="waybill:unresolved-reason") | .value)] | @tsv' "$d/out.cdx.json"
```

Expected: `pkg:github/waybill-org/waybill-fixture-dep` with no `@`, followed by the C151 reason. On `main` the same tree gives `pkg:generic/waybill-fixture-target`.

## Run the feature's tests

```sh
cargo test -p waybill --test cmake_unresolved_github_fetch
```

## Gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

Read `target/gate/prepr.log` for `GATE_RESULT=` and the per-target `test result: ok. N passed; 0 failed` lines.

## CI measurements

Dispatch with full SHAs, and read the scanned SHA from each run's logs, not from the run's `headSha`.
- **Census:** `census-fetches` in `kusari-sandbox/waybill-fixture-therock-sdk`, with `census/unresolved_fetches.py` (T001).
- **TheRock:** `measure-source --ref main -f waybill_ref=<branch sha> -f base_ref=<main sha>`.
- **Public corpus:** `public-corpus.yml` with `regen_goldens`.
