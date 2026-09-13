# Quickstart — verifying the multi-main-module override policy

## Reproduce the defect (before the change)

```bash
# a corpus checkout with more than one main module
GUICE=~/.cache/waybill/corpus/*/b0e1d0fab0167cd555ab8d262333c1a32db7d492/repo

# without an override: modules present
waybill --offline sbom scan --path $GUICE \
  --format cyclonedx-json --output cyclonedx-json=/tmp/no-override.json

# with one: modules gone
waybill --offline sbom scan --path $GUICE \
  --format cyclonedx-json --output cyclonedx-json=/tmp/override.json \
  --root-name maven-guice --root-version b0e1d0f

jq '[.components[]?]|length' /tmp/no-override.json   # 61
jq '[.components[]?]|length' /tmp/override.json      # 45
```

Dangling references — the symptom a consumer sees:

```bash
jq -r '[.components[]?|."bom-ref"] as $have
       | [.dependencies[]?|.dependsOn[]?] as $want
       | ($want - $have) | unique | .[]' /tmp/override.json
```

Five on maven-guice, nine on rust-ripgrep, zero on python-flask — flask
loses four components with no dangling edge to signal it, which is why
counting dangling references alone under-reports the problem.

## The three checks that matter after the change

```bash
# I2 — nothing dangles, in all three formats
# I3 — every retained module reachable from the subject
# C-2.3 — the override does not shrink the component set
jq '[.components[]?]|length' /tmp/override.json   # must equal 61
```

## Targets that exercise this

| target | main modules | why it matters |
|--------|-------------:|----------------|
| maven-guice | 16 | largest N; inter-module edges; 5 dangling refs |
| rust-ripgrep | 10 | cargo workspace; 9 dangling refs |
| python-flask | 4 | **zero** dangling refs — silent component loss, including `pkg:pypi/flask@3.1.2` |
| other eight | 0 | must stay byte-identical |

## What the corpus cannot cover

Two required behaviours have no corpus target:

- **N=1** — no target has exactly one main module.
- **FR-011 identity collision** — no target names a root matching a
  module's PURL.

Both need synthetic tests. Write them against the real emitter path, not
a hand-assembled component vector: milestone 856 shipped a bug for
months underneath a unit test that hand-built its input and therefore
tested a function that was never broken.

## Regenerating goldens

Three targets will move. Follow
`docs/development/refreshing-corpus-goldens.md` — regenerate through CI
dispatch, never locally, and read every diff before accepting.
