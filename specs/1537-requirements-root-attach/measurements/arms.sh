#!/usr/bin/env bash
# Build the spec-1537 repro arms in $2 (a fresh directory) and scan each
# with waybill $1 in all three formats. Prints, per arm and format, the
# root's direct targets (PURLs without qualifiers), the completeness
# verdict, and every waybill:orphan-reason. Usage: arms.sh <waybill> <dir>
set -euo pipefail
W=$1; D=$2
mkdir -p "$D"; cd "$D"
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
git init -q dep && git -C dep commit -q --allow-empty -m init
mk() { rm -rf "$1"; mkdir -p "$1"; git init -q "$1"; }
req2='waybill-fixture-a==1.0.0\nwaybill-fixture-b==2.0.0\n'
mk c1;  printf "$req2" > c1/requirements.txt
mk c2;  printf "$req2" > c2/requirements.txt
git -C c2 -c protocol.file.allow=always submodule -q add ../dep third_party/waybill-fixture-dep
mk c3;  printf "$req2" > c3/requirements.txt
printf '[project]\nname = "waybill-fixture-app"\nversion = "1.0.0"\ndependencies = ["waybill-fixture-c==3.0.0"]\n' > c3/pyproject.toml
mk c4;  mkdir -p c4/tools; printf 'waybill-fixture-a==1.0.0\n' > c4/requirements.txt; printf 'waybill-fixture-t==9.0.0\n' > c4/tools/requirements.txt
mk r12; printf '[project]\nname = "waybill-fixture-app"\nversion = "1.0.0"\ndependencies = ["waybill-fixture-c==3.0.0"]\n' > r12/pyproject.toml
printf '{"name":"waybill-fixture-native","version":"1.0.0","dependencies":["waybill-fixture-port"]}\n' > r12/vcpkg.json
mk c11; mkdir -p c11/vendor/x; printf 'waybill-fixture-a==1.0.0\n' > c11/requirements.txt; printf 'waybill-fixture-v==4.0.0\n' > c11/vendor/x/requirements.txt
mk c12; printf 'waybill-fixture-a==1.0.0\n' > c12/requirements.txt; printf 'waybill-fixture-k==5.0.0\n' > c12/requirements-constraints.txt
for a in c1 c2 c3 c4 r12 c11 c12; do git -C $a add -A && git -C $a commit -q -m add; done
for a in c1 c2 c3 c4 r12 c11 c12; do
  "$W" --offline sbom scan --path $a \
    --format cyclonedx-json --output cyclonedx-json=$a.cdx.json \
    --format spdx-2.3-json --output spdx-2.3-json=$a.spdx23.json \
    --format spdx-3-json --output spdx-3-json=$a.spdx3.json >/dev/null 2>$a.err
  cdx=$(jq -r '.metadata.component["bom-ref"] as $r | ([.components[]|..|objects|select(has("bom-ref") and has("purl"))|{key:.["bom-ref"],value:.purl}]|from_entries) as $p | [.dependencies[]|select(.ref==$r)|.dependsOn[]|($p[.]//.)|split("?")[0]]|sort|join(" ")' $a.cdx.json)
  s23=$(jq -r '(.relationships[]|select(.relationshipType=="DESCRIBES" and .spdxElementId=="SPDXRef-DOCUMENT")|.relatedSpdxElement) as $r | ([.packages[]|{key:.SPDXID,value:([.externalRefs[]?|select(.referenceType=="purl")|.referenceLocator][0]//.SPDXID)}]|from_entries) as $p | [(.relationships[]|select(.spdxElementId==$r and .relationshipType=="DEPENDS_ON")|.relatedSpdxElement),(.relationships[]|select(.relatedSpdxElement==$r and (.relationshipType|test("_DEPENDENCY_OF$")))|.spdxElementId)]|map($p[.]//.|split("?")[0])|unique|join(" ")' $a.spdx23.json)
  s3=$(jq -r '(.["@graph"]) as $g | ($g[]|select(.type=="SpdxDocument")|.rootElement[0]) as $r | ([$g[]|select(.spdxId and .software_packageUrl)|{key:.spdxId,value:.software_packageUrl}]|from_entries) as $p | [$g[]|select(.type=="Relationship" and .relationshipType=="dependsOn" and .from==$r)|.to[]|($p[.]//.)|split("?")[0]]|unique|join(" ")' $a.spdx3.json)
  verdict=$(jq -r '[.metadata.properties[]?|select(.name=="waybill:graph-completeness")|.value][0]' $a.cdx.json)
  reasons=$(jq -r '[.components[]|..|objects|select(has("purl"))|select((.properties//[])|any(.name=="waybill:orphan-reason"))|"\(.purl|split("?")[0])=\([.properties[]|select(.name=="waybill:orphan-reason")|.value][0])"]|join(" ")' $a.cdx.json)
  comps=$(jq -r '[.components[]|..|objects|select(has("purl") and .type!="file")|.purl|split("?")[0]]|sort|join(" ")' $a.cdx.json)
  printf '%s\n  root      %s\n  cdx       %s\n  spdx-2.3  %s\n  spdx-3    %s\n  verdict   %s\n  reasons   %s\n  components %s\n' "$a" "$(jq -r '.metadata.component.purl' $a.cdx.json)" "$cdx" "$s23" "$s3" "$verdict" "${reasons:-none}" "$comps"
done
