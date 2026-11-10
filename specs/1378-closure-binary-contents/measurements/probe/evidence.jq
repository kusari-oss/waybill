# Per ecosystem component: which evidence kind and which source file put it there.
def all: [.. | objects | select(has("bom-ref") and has("type") and has("name"))];
def prop($n): ([.properties[]? | select(.name==$n) | .value] | first) // "NONE";
[all[] | select((.purl//"")|test("^pkg:(golang|cargo)/"))
 | {kind: prop("waybill:evidence-kind"), tier: prop("waybill:sbom-tier"), src: prop("waybill:source-files")}]
| group_by([.kind,.tier,.src]) | map({kind:.[0].kind, tier:.[0].tier, src:(.[0].src|.[0:110]), n:length}) | sort_by(-.n)
