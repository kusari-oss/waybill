# Quickstart: verifying the flake.lock reader

## The one-minute check

```sh
waybill sbom scan --path <repo-with-flake.lock> --offline \
  --format cyclonedx-json --output out.cdx.json

# every pinned input, with its revision
jq -r '.components[] | select(.purl | test("^pkg:(github|gitlab|sourcehut|generic)/"))
       | "\(.name)  \(.version)"' out.cdx.json
```

On the reference repository this goes from **zero** rows to one
(`nixpkgs  a799d3e3886da994fa307f817a6bc705ae538eeb`) — SC-001.

## The checks that matter

**No native checksum from a NAR hash** (C-4). This should return nothing:

```sh
jq -r '.components[] | select(.purl | test("^pkg:github/")) | .hashes // empty' out.cdx.json
```

and the value should instead appear, SRI prefix intact, in the component's
annotations.

**Every input reachable from the root** (C-5, SC-003):

```sh
python3 - <<'PY'
import json; d=json.load(open("out.cdx.json"))
refs={c["bom-ref"] for c in d["components"]}
root=d["metadata"]["component"]["bom-ref"]
reached={t for x in d.get("dependencies",[]) for t in x.get("dependsOn",[])}
print("unreachable:", sorted(r for r in refs if r!=root and r not in reached))
PY
```

**Offline and online agree** (SC-002) — the scan takes no network:

```sh
diff <(waybill sbom scan --path <repo> --offline --format cyclonedx-json --output /dev/stdout) \
     <(waybill sbom scan --path <repo>           --format cyclonedx-json --output /dev/stdout)
```

**Two runs agree byte-for-byte** (SC-006). This is the check #948 existed to
make possible; `nodes` is a JSON object and its iteration order is not a
guarantee:

```sh
waybill sbom scan --path <repo> --offline --format cyclonedx-json --output a.json
waybill sbom scan --path <repo> --offline --format cyclonedx-json --output b.json
cmp a.json b.json && echo "deterministic"
```

**Adding a flake.lock never removes components** (SC-005) — the property #937
and #938 were both violations of:

```sh
mv flake.lock /tmp/ && waybill sbom scan --path . --offline --format cyclonedx-json --output without.json
mv /tmp/flake.lock . && waybill sbom scan --path . --offline --format cyclonedx-json --output with.json
comm -23 <(jq -r '.components[].name' without.json | sort -u) \
         <(jq -r '.components[].name' with.json    | sort -u)
# MUST be empty
```

**Nix files stop reading as unrecognised** (SC-004):

```sh
waybill repo report --path <repo> --output report.json
jq '.totals' report.json    # files_unclaimed should drop by the Nix files
```

## Fixture shapes worth covering

Drawn from the Phase 0 measurements, not invented:

| shape | why | source |
|---|---|---|
| single `github` input | the base case | reference repository |
| `tarball` input with `rev` but no owner/repo | FR-013b; found on 2 of 5 samples | public flakes |
| `follows` alias (**array-valued** `inputs` entry) | FR-004; the structural finding of R3 | public flake |
| a non-root node declaring its own inputs | FR-008 | public flake |
| `original` naming a branch, `locked` naming a rev | FR-006, US3 | reference repository |
| `original` already exact | US3 scenario 2 — no spurious diff | constructed |
| malformed JSON | FR-010, SC-007 | constructed |
| `version` other than 7 | FR-010 | constructed |
| two independent lockfiles in one tree | FR-011 | constructed |
| lockfile with no inputs | edge case — truthfully empty, not an error | constructed |

Fixtures use synthetic names where they are constructed; the measured shapes are
copied from real lockfiles verbatim, because a parser fixture that did not come
from the tool it parses is testing the fixture. That is the lesson #937 taught:
its unit tests and its integration fixture both used a constraint format `cabal`
never emits, so both layers passed against output that does not exist.
