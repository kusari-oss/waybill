#!/usr/bin/env bash
# Spec 1543 FR-001: where a synthesized workspace root ends up, per reader
# and shape. Usage: workspace-roots.sh <waybill binary> <scratch dir>
set -euo pipefail
W=$1; S=$2; rm -rf "$S"; mkdir -p "$S"
uv() { d=$1; mkdir -p $d/rocisa
  printf '[project]\nname = "waybill-fixture-tensile"\nversion = "5.0.0"\n\n[tool.uv.workspace]\nmembers = ["rocisa"]\n' > $d/pyproject.toml
  printf '[project]\nname = "waybill-fixture-rocisa"\nversion = "0.1.0"\n' > $d/rocisa/pyproject.toml
  printf 'version = 1\n\n[manifest]\nmembers = ["waybill-fixture-rocisa", "waybill-fixture-tensile"]\n\n[[package]]\nname = "waybill-fixture-rocisa"\nversion = "0.1.0"\nsource = { editable = "rocisa" }\n\n[[package]]\nname = "waybill-fixture-tensile"\nversion = "5.0.0"\nsource = { editable = "." }\ndependencies = [{ name = "waybill-fixture-rocisa" }]\n' > $d/uv.lock; }
bun() { d=$1; mkdir -p $d/packages/a
  printf '{"name":"waybill-fixture-mono","version":"1.0.0","workspaces":["packages/*"]}\n' > $d/package.json
  printf '{"name":"waybill-fixture-a","version":"1.0.0"}\n' > $d/packages/a/package.json
  printf '{\n  "lockfileVersion": 1,\n  "workspaces": {\n    "": { "name": "waybill-fixture-mono" },\n    "packages/a": { "name": "waybill-fixture-a", "version": "1.0.0" }\n  },\n  "packages": {}\n}\n' > $d/bun.lock; }
kts() { d=$1; mkdir -p $d/app
  printf 'rootProject.name = "waybill-fixture-kts"\ninclude(":app")\n' > $d/settings.gradle.kts
  printf 'plugins { kotlin("jvm") }\ndependencies {\n    implementation("org.waybill-fixture:lib:1.0.0")\n}\n' > $d/app/build.gradle.kts; }
for k in uv bun kts; do
  $k $S/$k/atroot; mkdir -p $S/$k/sub/proj; $k $S/$k/sub/proj
  printf '[package]\nname = "waybill-fixture-top"\nversion = "1.0.0"\n' > $S/$k/sub/Cargo.toml
  for t in atroot sub; do for mode in plain op; do
    o=$S/$k-$t-$mode.cdx.json; flags=(); [ $mode = op ] && flags=(--root-name x --root-version 1)
    "$W" --offline sbom scan --path $S/$k/$t --no-deep-hash --format cyclonedx-json --output cyclonedx-json=$o "${flags[@]}" >/dev/null 2>&1
    jq -r --arg cell "$k $t $mode" '
      (.components[] | select([.properties[]? | select(.name=="waybill:component-role") | .value] == ["workspace-root"])) as $w
      | [.dependencies[] | select(.dependsOn | index($w["bom-ref"])) | .ref] as $in
      | "\($cell)\tsubject=\(.metadata.component["bom-ref"])\tincoming=\($in | join(","))\torphan-reason=\([$w.properties[]? | select(.name=="waybill:orphan-reason") | .value] | join(","))"' $o
  done; done
done
