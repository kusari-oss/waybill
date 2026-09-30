# Quickstart: emitting a Nix derivation closure

**Feature**: `1035-nix-closure-sbom` (#1034, #1040)

## Prerequisites

`nix` on `PATH`, and a project whose flake exposes `packages.<system>.default`.
Both measured Haskell libraries do; haskell-language-server does not, and is
the degradation case.

## The happy path

```sh
waybill sbom scan --path <project> --nix-closure \
  --format cyclonedx-json --output cyclonedx-json=out.cdx.json

jq '.metadata.properties[] | select(.name=="waybill:nix-closure")' out.cdx.json
```

Expect an attribute name, derivation count, per-role counts, and the count of
patches carrying no CVE.

## Check the closure actually added something

```sh
jq '[.components[] | select(.properties[]?.name=="waybill:closure-role")] | length' out.cdx.json
```

Measured baselines: **216** components on moat and **218** on slack-web exist
in the closure and not in today's output. A number near zero means the closure
ran but contributed nothing — investigate rather than accept.

## Check the patch data

```sh
jq '.components[] | select(.pedigree.patches) |
    {name, patches: [.pedigree.patches[] | {type, resolves: [.resolves[]?.id]}]}' out.cdx.json
```

Expect `CVE-2019-13232` on both measured projects; `CVE-2021-4217` on
slack-web.

## Verify both VEX statements

```sh
jq '[.statements[] | select(.vulnerability.name=="CVE-2019-13232")] |
    map({status, subject: .products[0].id})' openvex.json
```

**Two** statements, not one: `affected` subject to the version and
`not_affected` subject to this build. One alone is a defect — a lone
`not_affected` from filename evidence is the overclaim FR-011 forbids.

## Reproduce the measurements

```sh
nix derivation show -r .#default > closure.json
python3 specs/1034-nix-eval-tier/measurements/classify-derivation-closure.py closure.json
python3 specs/1035-nix-closure-sbom/measurements/closure-vs-emitted.py closure.json out.cdx.json
```

The second prints both directions. Components in the closure and not the SBOM
are what this feature adds; components in the SBOM and not the closure are GHC
boot libraries and other-stanza dependencies — expected, and the reason this
supplements rather than replaces (research R1).

## Degradations

| Provoke | Expect |
|---|---|
| `nix` absent | `tool-absent`, scan succeeds, manifest set intact |
| `--offline` | `offline-requested`; no nix process starts |
| flake with attributes but no `default` | degrade naming the available ones |
| haskell-language-server | `no-evaluable-attribute` |

In every case the manifest-derived set is untouched — the closure supplements,
so losing it costs only the supplement.

## Pre-PR

```sh
./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"
```

Read the script's own status, never a pipeline's, then assert the positive
signals: `>>> all pre-PR checks passed.` and the per-target
`test result: ok. N passed; 0 failed`.
