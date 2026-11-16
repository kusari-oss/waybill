# Quickstart: check owned-binary analysis by hand

Tests are the authority. This is the manual check used while building, on macOS with Nix.

```sh
S=$(mktemp -d)
LIB=$(nix build --no-link --print-out-paths nixpkgs#zlib)/lib/libz.1.3.2.dylib
for layout in wheel site; do
  case $layout in
    wheel) R=$S/wheel ;;
    site)  R=$S/site/usr/lib/python3.12/site-packages ;;
  esac
  mkdir -p "$R/waybill_fixture_pkg/.dylibs" "$R/waybill_fixture_pkg-1.2.3.dist-info"
  cp "$LIB" "$R/waybill_fixture_pkg/.dylibs/"
  printf 'Metadata-Version: 2.1\nName: waybill-fixture-pkg\nVersion: 1.2.3\n' \
    > "$R/waybill_fixture_pkg-1.2.3.dist-info/METADATA"
  printf 'waybill_fixture_pkg/.dylibs/libz.1.3.2.dylib,,\nwaybill_fixture_pkg-1.2.3.dist-info/METADATA,,\n' \
    > "$R/waybill_fixture_pkg-1.2.3.dist-info/RECORD"
done
target/release/waybill --offline sbom scan --path "$S/wheel" --output "$S/wheel.cdx.json"
target/release/waybill --offline sbom scan --path "$S/site"  --output "$S/site.cdx.json"
```

**Expected, in both documents:**

```sh
# The binary is nested under the package, and zlib is nested under the binary.
jq '[.. | objects | select(.purl? == "pkg:pypi/waybill-fixture-pkg@1.2.3") | .components[]?.components[]?.purl]' "$S/wheel.cdx.json"
# → ["pkg:generic/zlib@1.3.2"]

# No top-level component for the library.
jq '[.components[] | select(.purl | test("libz"))] | length' "$S/wheel.cdx.json"   # → 0

# The counts.
jq -r '.metadata.properties[] | select(.name=="waybill:python-owned-binaries") | .value' "$S/wheel.cdx.json"
# → {"analysed":1,"identified":1}
```

Diff the two documents' nested trees and edges with the root normalised. They are identical (FR-006).
