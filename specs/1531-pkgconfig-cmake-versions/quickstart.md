# Quickstart: check the feature by hand

These checks build a synthetic prefix in a scratch directory and scan it. Nothing is fetched.

```sh
P=$(mktemp -d)/sdk
mkdir -p "$P/lib/pkgconfig" "$P/lib/cmake/WaybillFixtureB"
cat > "$P/lib/pkgconfig/waybill-fixture-a.pc" <<'EOF'
prefix=/opt/sdk
version=1.2.3
Name: Waybill Fixture A
Version: ${version}
Requires: waybill-fixture-c >= 1.0
Requires.private: waybill-fixture-d
EOF
printf 'Name: c\nVersion: 2.0\n' > "$P/lib/pkgconfig/waybill-fixture-c.pc"
printf 'Name: d\nVersion: 3.0\n' > "$P/lib/pkgconfig/waybill-fixture-d.pc"
printf 'Name: t\nVersion: ${undefined}\n' > "$P/lib/pkgconfig/waybill-fixture-t.pc"
printf 'set(PACKAGE_VERSION "4.5.6")\n' > "$P/lib/cmake/WaybillFixtureB/WaybillFixtureBConfigVersion.cmake"

waybill --offline sbom scan --path "$P" --format cyclonedx-json --output cyclonedx-json=/tmp/q.cdx.json
```

Expected:

- **Components:**
  - `pkg:generic/waybill-fixture-a@1.2.3`, `…-c@2.0`, `…-d@3.0` and `pkg:generic/waybillfixtureb@4.5.6`;
  - nothing for `waybill-fixture-t`.
- **Dependencies:** `waybill-fixture-a` depends on `-c` and `-d`.
- **Private requirements:** its `waybill:pkg-config-private-requires` is `["pkg:generic/waybill-fixture-d@3.0"]`.
- **Coverage:** `metadata.properties` has `waybill:installed-library-coverage` with `unresolved-version: 1`.

```sh
jq -r '.components[] | select(.purl|startswith("pkg:generic/waybill")) | .purl' /tmp/q.cdx.json | sort
jq -r '.metadata.properties[] | select(.name=="waybill:installed-library-coverage") | .value' /tmp/q.cdx.json
```

**Control.** Removing the `.pc` and `.cmake` files gives a document with no coverage record. It is byte-identical to the base commit's output for the same tree, once the serial number and timestamp are masked.
