# Quickstart: test scope for installed trees

## See it on a synthetic prefix

```sh
p=$(mktemp -d)
mkdir -p "$p/lib/cmake/waybill-fixture-p" "$p/lib/pkgconfig" "$p/bin/waybill-fixture-p" "$p/share/waybill-fixture-q/tests"
# An installed library, so the --path scan is recognised as an installed tree (FR-015).
printf 'Name: waybill-fixture-p\nVersion: 1.2.3\nDescription: x\n' > "$p/lib/pkgconfig/waybill-fixture-p.pc"
# A test executable declared by an installed CTest file.
cp /bin/true "$p/bin/waybill-fixture-p-test"
printf 'add_test(quick "../waybill-fixture-p-test" --quick)\n' > "$p/bin/waybill-fixture-p/CTestTestfile.cmake"
# A test-suite data file under a conventional directory.
printf 'x' > "$p/share/waybill-fixture-q/tests/input.dat"
waybill --offline sbom scan --path "$p" --format cyclonedx-json --output /tmp/t.cdx.json
jq '[.. | objects | select(.scope? == "excluded") | .name]' /tmp/t.cdx.json
jq -r '.metadata.properties[] | select(.name == "waybill:test-scope-coverage") | .value' /tmp/t.cdx.json
```

**What to expect:**
- `waybill-fixture-p-test` carries `scope: "excluded"`, `waybill:lifecycle-scope: test` and `waybill:lifecycle-scope-derivation: ctest-declaration`.
- `input.dat` carries the same scope, with the derivation `test-directory`.
- C226 shows `installed-tree: recognised`, `ctest: 1`, and one `resolved` location.

Remove the `.pc` file and scan again. The tree is no longer recognised, so `input.dat` is unscoped, while the CTest declaration still scopes the executable.

## The ROCm measurement (CI only)

In `kusari-sandbox/waybill-fixture-therock-sdk`:

```sh
gh workflow run measure-full.yml -R kusari-sandbox/waybill-fixture-therock-sdk -f waybill_ref=1533-installed-test-scope -f base_ref=main
```

The report gives C226, the scoped bytes per rule, and the median scan time against `main`. The committed census for comparison is at `specs/1533-installed-test-scope/measurements/test-census.json`. Regenerate it with:

```sh
python3 -I -B specs/1533-installed-test-scope/measurements/test-census.py <fixture-clone>
```
