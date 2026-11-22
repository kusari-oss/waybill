# Quickstart: spec 1644

## Run the tests

```sh
cargo test -p waybill --test version_string_anchored
cargo test -p waybill --lib scan_fs::binary::version_strings
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## See a verdict

```sh
RUST_LOG=waybill::version_census=debug \
  waybill --offline sbom scan --path <tree> --format cyclonedx-json --output out.cdx.json 2>census.log
grep -a 'anchored=' census.log | wc -l
```

Each `anchored=` line holds one binary × library verdict (`contracts/wire.md`). A tree with no anchor prints none. The positive control (fixture A-2) must print one before a zero elsewhere means anything.

## Census on TheRock and the quality corpus (CI only)

```sh
gh workflow run census-versions.yml -R kusari-sandbox/waybill-fixture-therock-sdk \
  --ref main -f waybill_ref=<FULL sha>
```

Verify every job's `refs.txt` names the dispatched SHA before reading any count.

## Re-measure the CPE evidence

```sh
scripts/probe-cpe-dictionary.sh gmplib:gmp tukaani:xz xz_project:xz
python3 -I -B specs/1375-generic-cpe-table/measurements/cpe-titles.py gmp xz
```
