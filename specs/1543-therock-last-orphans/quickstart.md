# Quickstart

## Reproduce the trace

```sh
cargo build -p waybill
specs/1543-therock-last-orphans/measurements/workspace-roots.sh ./target/debug/waybill "$TMPDIR/ws1543"
```

On `main` this prints 7 stranded cells (`incoming=` empty). On the branch it prints:
- 0 stranded cells;
- `subject=pkg:generic/…` for the three `atroot plain` rows.

## Run the contract tests

```sh
cargo test -p waybill --test scan_therock_last_orphans
```

## Gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## TheRock (CI only)

Dispatch the jobs:

```sh
gh workflow run measure-source.yml -R kusari-sandbox/waybill-fixture-therock-sdk --ref main \
  -f waybill_ref=<branch sha> -f base_ref=main
gh workflow run measure-full.yml   -R kusari-sandbox/waybill-fixture-therock-sdk --ref main \
  -f waybill_ref=<branch sha> -f base_ref=main
```

Then probe the downloaded branch documents:

```sh
WAYBILL_I7_PROBE_DOC=<branch>.cdx.json cargo test -p waybill --test public_corpus probe_i7_on_document -- --ignored --nocapture
```

Expect 0 unexplained components, `tensile` reachable, and `wkmi` with C235 `["WIN_SDK"]`.
