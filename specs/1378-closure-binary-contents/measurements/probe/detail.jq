def all: [.. | objects | select(has("bom-ref") and has("type") and has("name"))];
{ self: [all[]|select((.purl//"")|test("cli/cli|/ripgrep@|/rg@")) | {name,version,purl}],
  sample: [all[]|select((.purl//"")|test("^pkg:(golang|cargo)/"))][0:2] | map({name,version,purl,scope,cpe,props:[.properties[]?|{(.name):(.value|.[0:100])}]}) }
