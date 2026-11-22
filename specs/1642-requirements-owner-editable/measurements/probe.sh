#!/usr/bin/env bash
# Spec 1642 research probe (R1, R4). Builds #1640's tree and the propagation
# fixture, scans each with the given waybill binary, and prints the edges into
# waybill-fixture-shared, its scope, and the dev package's source-files.
#
# usage: probe.sh WAYBILL_BIN WORKDIR
set -euo pipefail
B=$1; W=$2
rm -rf "$W"; mkdir -p "$W"

edges() { jq -r '([.components[]|select(.purl|startswith("pkg:pypi/waybill-fixture-shared"))][0]["bom-ref"]) as $b
  | [.dependencies[] | select(.dependsOn|index($b)) | .ref] | sort | join(", ")' "$1"; }
scope() { jq -r '[.components[]|select(.purl|startswith("pkg:pypi/waybill-fixture-shared"))][0]
  | "\(.scope // "required") \([.properties[]?|select(.name|test("lifecycle-scope"))|"\(.name)=\(.value)"]|join(" "))"' "$1"; }

# --- R1: #1640's tree, in both name orders, with and without tool/uv.lock ---
tree() { # $1 dir, $2 dev package name
  local d=$1 dev=$2
  mkdir -p "$d/tool/devtool"
  echo 'waybill-fixture-shared>=1.0' > "$d/requirements.txt"
  echo 'waybill-fixture-shared>=1.0' > "$d/tool/requirements.txt"
  printf '[project]\nname = "waybill-fixture-tool"\nversion = "1.0.0"\ndependencies = []\n[dependency-groups]\ndev = ["%s"]\n[tool.uv.sources]\n%s = { path = "devtool", editable = true }\n' "$dev" "$dev" > "$d/tool/pyproject.toml"
  printf '[project]\nname = "%s"\nversion = "0.1.0"\n' "$dev" > "$d/tool/devtool/pyproject.toml"
  cat > "$d/tool/uv.lock" <<E
version = 1
requires-python = ">=3.10"

[[package]]
name = "$dev"
version = "0.1.0"
source = { editable = "devtool" }

[[package]]
name = "waybill-fixture-tool"
version = "1.0.0"
source = { editable = "." }

[package.dev-dependencies]
dev = [
    { name = "$dev" },
]
E
}
for dev in waybill-fixture-devtool waybill-fixture-zdevtool; do
  for lock in with without; do
    d="$W/r1-$dev-$lock"; tree "$d" "$dev"
    [ "$lock" = without ] && rm "$d/tool/uv.lock"
    "$B" --offline sbom scan --path "$d" --format cyclonedx-json --output "$d.cdx.json" 2>/dev/null
    sf=$(jq -r --arg p "pkg:pypi/$dev@0.1.0" '[.components[]|select(.purl==$p)][0] | [.properties[]?|select(.name=="waybill:source-files" or .name=="waybill:workspace-member")|"\(.name)=\(.value)"]|join(" ")' "$d.cdx.json")
    echo "R1 $dev uv.lock:$lock | edges into shared: $(edges "$d.cdx.json") | $dev: $sf"
  done
done

# --- R4: a root-declared package also reached through a dev-group package ---
d="$W/r4"; mkdir -p "$d/app"
echo 'waybill-fixture-shared==2.0' > "$d/requirements.txt"
printf '[project]\nname = "waybill-fixture-app"\nversion = "1.0.0"\ndependencies = []\n[dependency-groups]\ndev = ["waybill-fixture-devlib"]\n' > "$d/app/pyproject.toml"
cat > "$d/app/uv.lock" <<'E'
version = 1
requires-python = ">=3.10"

[[package]]
name = "waybill-fixture-app"
version = "1.0.0"
source = { editable = "." }

[package.dev-dependencies]
dev = [
    { name = "waybill-fixture-devlib" },
]

[[package]]
name = "waybill-fixture-devlib"
version = "1.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "waybill-fixture-shared" },
]

[[package]]
name = "waybill-fixture-shared"
version = "2.0"
source = { registry = "https://pypi.org/simple" }
E
cp -R "$d" "$W/r4-control"; mv "$W/r4-control/requirements.txt" "$W/r4-control/app/requirements.txt"
for v in r4 r4-control; do
  for i in 1 2 3; do
    "$B" --offline sbom scan --path "$W/$v" --format cyclonedx-json --output "$W/$v.$i.cdx.json" 2>/dev/null
    echo "$v run $i | root: $(jq -r '.metadata.component["bom-ref"]' "$W/$v.$i.cdx.json") | edges into shared: $(edges "$W/$v.$i.cdx.json") | shared: $(scope "$W/$v.$i.cdx.json")"
  done
done
