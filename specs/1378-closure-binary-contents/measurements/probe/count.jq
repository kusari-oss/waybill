# Counts CycloneDX components at EVERY nesting depth. The first pass of this
# probe counted `.components` only and reported 0 cargo crates for ripgrep:
# the binary reader nests crates under the binary's own component.
def all: [.. | objects | select(has("bom-ref") and has("type") and has("name"))];
def ty: (.purl // "NONE") | capture("^(?<t>pkg:[^/]+|NONE)").t;
{ top: (.components|length), all: (all|length),
  golang: ([all[]|select(ty=="pkg:golang")]|length),
  cargo:  ([all[]|select(ty=="pkg:cargo")]|length),
  nested_parents: [.components[]|select((.components//[])|length>0)|{name, nested:(.components|length)}],
  dep_entries: (.dependencies|length),
  edges: ([.dependencies[]|(.dependsOn//[])|length]|add) }
