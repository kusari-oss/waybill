# Quickstart: verifying the Go per-main-module scope fix

**Feature**: 233-go-per-mainmod-scope
**Phase**: 1

Executes SC-001..SC-006 predicates against the finished implementation. Assumes repo root at `/Users/mlieberman/Projects/mikebom` and a `cargo build --release -p waybill` binary at `target/release/waybill`.

## Setup

```bash
cargo build --release -p waybill
```

## Walkthrough — SC-001 + SC-005 (4-module fixture)

Build the reporter's minimal repro layout as a shell fixture:

```bash
R=$(mktemp -d)/root
mkdir -p "$R"

sum() {
  printf 'example.com/mikebomfixture/text %s h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\nexample.com/mikebomfixture/text %s/go.mod h1:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=\n' "$1" "$1"
}
mod() {
  mkdir -p "$R/$1"
  printf 'module %s\n\ngo 1.24.0\n\nrequire example.com/mikebomfixture/text %s\n' "$2" "$3" > "$R/$1/go.mod"
  sum "$3" > "$R/$1/go.sum"
}
mod .              example.com/root      v0.40.0
mod hack           example.com/hack      v0.37.0
mod tools          example.com/tools     v0.29.0
mod deep/src/thing example.com/deepthing v0.25.0
printf 'package main\nimport _ "example.com/mikebomfixture/text/language"\nfunc main() {}\n' > "$R/main.go"

./target/release/waybill --offline sbom scan --path "$R" \
  --project-discovery=all --format cyclonedx-json --no-deep-hash \
  --output /tmp/233-all.cdx.json 2>/dev/null
```

**SC-001 assertion** — per-main-module `x/text` version matches its own declaration:

```bash
jq -r '
  .dependencies[]
  | select(.ref | startswith("pkg:golang/example.com/"))
  | { module: .ref, text_deps: [.dependsOn[]? | select(contains("mikebomfixture/text"))] }
' /tmp/233-all.cdx.json
# Expect:
#   root      → [pkg:golang/example.com/mikebomfixture/text@v0.40.0]
#   hack      → [pkg:golang/example.com/mikebomfixture/text@v0.37.0]
#   tools     → [pkg:golang/example.com/mikebomfixture/text@v0.29.0]
#   deepthing → [pkg:golang/example.com/mikebomfixture/text@v0.25.0]
```

**SC-005 assertion** — no main-module points at another main-module:

```bash
jq -r '
  ([.components[] | select((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module")) | ."bom-ref"]) as $mainmods
  | .dependencies[]
  | select(.ref as $r | $mainmods | index($r))
  | { ref, mm_deps: [.dependsOn[]? | select(. as $d | $mainmods | index($d))] }
' /tmp/233-all.cdx.json
# Expect: every main-module entry has mm_deps == []
```

## Walkthrough — SC-002 (project-discovery=root-only)

```bash
./target/release/waybill --offline sbom scan --path "$R" \
  --project-discovery=root-only --format cyclonedx-json --no-deep-hash \
  --output /tmp/233-root-only.cdx.json 2>/dev/null

# Extract all x/text versions in the filtered SBOM.
jq -r '.components[] | select(.purl // "" | test("mikebomfixture/text")) | .purl' \
  /tmp/233-root-only.cdx.json | sort -u
# Expect: pkg:golang/example.com/mikebomfixture/text@v0.40.0
# (only the root's declared version — no v0.25.0/v0.29.0/v0.37.0 leaks)
```

Pre-233 baseline (from 2026-08-11 measurement on `main` post-m232): this same query returned BOTH `v0.25.0` and `v0.40.0`. Post-233 target: only `v0.40.0`.

## Walkthrough — SC-003 + SC-004 (Grafana manual verification)

Requires a local clone of `github.com/grafana/grafana`. One-shot manual step:

```bash
GRAFANA_PATH="$HOME/Projects/grafana"  # adjust to your clone

./target/release/waybill --offline sbom scan --path "$GRAFANA_PATH" \
  --project-discovery=root-only --format cyclonedx-json --no-deep-hash \
  --output /tmp/233-grafana-root.cdx.json 2>/dev/null

# SC-003: x/text versions in the root unit's SBOM
jq -r '.components[] | select(.purl // "" | test("golang.org/x/text")) | .purl' \
  /tmp/233-grafana-root.cdx.json | sort -u
# Pre-233 baseline: at least v0.37.0 bleeds in from hack/.
# Post-233 target: only the version root's go.mod declares.

# SC-004: klauspost/compress versions in the root unit's SBOM
jq -r '.components[] | select(.purl // "" | test("klauspost/compress")) | .purl' \
  /tmp/233-grafana-root.cdx.json | sort -u
# Pre-233 baseline: v1.18.5 bleeds in from devenv/docker/blocks/prometheus_high_card/.
# Post-233 target: only root-declared versions.
```

## Walkthrough — FR-008 (mixed Go versions)

Build a fixture with 2 modules on different Go versions:

```bash
R2=$(mktemp -d)/mixed-go
mod2() {
  mkdir -p "$R2/$1"
  printf 'module %s\n\ngo %s\n' "$2" "$3" > "$R2/$1/go.mod"
}
mkdir -p "$R2"
mod2 . example.com/root v1.24.0
mod2 legacy example.com/legacy v1.22.5

./target/release/waybill --offline sbom scan --path "$R2" \
  --project-discovery=all --format cyclonedx-json --no-deep-hash \
  --output /tmp/233-mixed.cdx.json 2>/dev/null

# Two distinct stdlib components emitted?
jq -r '.components[] | select(.purl // "" | startswith("pkg:golang/stdlib@")) | .purl' \
  /tmp/233-mixed.cdx.json | sort -u
# Expect:
#   pkg:golang/stdlib@v1.22.5
#   pkg:golang/stdlib@v1.24.0

# Each main-module dependsOn its own stdlib version?
jq -r '.dependencies[] | select(.ref | startswith("pkg:golang/example.com/")) | { ref, stdlib_deps: [.dependsOn[]? | select(contains("stdlib@"))] }' \
  /tmp/233-mixed.cdx.json
# Expect:
#   root   → [pkg:golang/stdlib@v1.24.0]
#   legacy → [pkg:golang/stdlib@v1.22.5]
```

## Walkthrough — FR-004 (workspace-member union)

Build a fixture with 2 modules requiring the same package + version:

```bash
R3=$(mktemp -d)/shared-ver
sum() { printf 'example.com/mikebomfixture/text %s h1:xxx\nexample.com/mikebomfixture/text %s/go.mod h1:yyy\n' "$1" "$1"; }
mod3() {
  mkdir -p "$R3/$1"
  printf 'module %s\n\ngo 1.24.0\n\nrequire example.com/mikebomfixture/text v0.29.0\n' "$2" > "$R3/$1/go.mod"
  sum v0.29.0 > "$R3/$1/go.sum"
}
mkdir -p "$R3"
printf 'module example.com/root\ngo 1.24.0\n' > "$R3/go.mod"
mod3 hack example.com/hack
mod3 tools example.com/tools

./target/release/waybill --offline sbom scan --path "$R3" \
  --project-discovery=all --format cyclonedx-json --no-deep-hash \
  --output /tmp/233-shared.cdx.json 2>/dev/null

# One component with union workspace-member?
jq -r '.components[]
  | select(.purl // "" | test("mikebomfixture/text"))
  | { purl, ws_member: ((.properties // []) | map(select(.name == "waybill:workspace-member")) | .[0].value) }' \
  /tmp/233-shared.cdx.json
# Expect: one entry with ws_member == "[\"hack\",\"tools\"]" (sorted, deduped)
```

## Post-implementation checklist

- [ ] SC-001: 4-module fixture — each main-module's x/text dep matches its own declaration
- [ ] SC-002: root-only scan of 4-module fixture emits only root's declared version
- [ ] SC-003: Grafana root-unit scan emits only root's declared x/text version
- [ ] SC-004: Grafana root-unit scan emits only root's declared klauspost/compress version
- [ ] SC-005: no main-module `dependsOn` list contains any other main-module PURL
- [ ] SC-006: Grafana root-unit orphan-reason inventory drops leak-attributable classes
- [ ] FR-004: shared-version fixture → single component with union workspace-member
- [ ] FR-008: mixed-Go-version fixture → distinct stdlib components per version
- [ ] Pre-PR gate: `./scripts/pre-pr.sh` exits 0
