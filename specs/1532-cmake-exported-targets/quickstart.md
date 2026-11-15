# Quickstart

```sh
P=$(mktemp -d)/sdk
mkdir -p "$P/lib/cmake/WaybillFixtureP" "$P/bin"
printf 'set(PACKAGE_VERSION "4.5.6")\n' > "$P/lib/cmake/WaybillFixtureP/WaybillFixturePConfigVersion.cmake"
cat > "$P/lib/cmake/WaybillFixtureP/WaybillFixturePTargets.cmake" <<'X'
get_filename_component(_IMPORT_PREFIX "${CMAKE_CURRENT_LIST_FILE}" PATH)
get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)
get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)
get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)
X
cat > "$P/lib/cmake/WaybillFixtureP/WaybillFixturePTargets-release.cmake" <<'X'
set_target_properties(wfp::ops PROPERTIES
  IMPORTED_LOCATION_RELEASE "${_IMPORT_PREFIX}/lib/libwaybill-fixture-ops.a")
set_target_properties(wfp::tool PROPERTIES
  IMPORTED_LOCATION_RELEASE "${_IMPORT_PREFIX}/bin/waybill-fixture-tool")
X
printf '!<arch>\n' > "$P/lib/libwaybill-fixture-ops.a"; printf 'x' > "$P/bin/waybill-fixture-tool"

waybill --offline sbom scan --path "$P" --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/q.cdx.json
jq -r '.components[] | select(.purl=="pkg:generic/waybillfixturep@4.5.6") | .evidence.occurrences[].location' /tmp/q.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:installed-library-coverage") | .value | fromjson | ."exported-targets"' /tmp/q.cdx.json
```

**Expected:**
- **Locations:** `bin/waybill-fixture-tool`,
  `lib/cmake/WaybillFixtureP/WaybillFixturePConfigVersion.cmake` and
  `lib/libwaybill-fixture-ops.a`.
- **Coverage:** `{"attached":2,"locations":2,…,"read":2,…}`.
