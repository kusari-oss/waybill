# Measurements — 1281 (licenses for nix closure components)

Phase-0, run before the spec's requirements were written. Every figure is from
a command executed here; none is scaled from another system.

## The gap, measured on one closure

`nix-closure-moat` at its pin, **aarch64-darwin**, both tools on the same
project:

| | sbomnix 1.9.0 | waybill (post-m1280) |
|---|---|---|
| components | 1,274 | 413 |
| licensed | **301** | **137** |
| licensed rate | 23% | **33%** |
| content hash | 0 | 136 |
| no PURL at all | 106 | 0 |

**96 components waybill already emits are licensed by sbomnix and not by
waybill**, and all 96 are closure-tier. The licenses they would gain:

```
MIT 23 · GPL-3.0-or-later 17 · BSD-3-Clause 15 · GPL-2.0-or-later 10
BSD-2-Clause 10 · publicDomain 3 · GPL-2.0-only 3 · GPL-3.0-only 3 · …
```

`publicDomain` is **not** a valid SPDX identifier. Milestone 1273's two-state
license type already models exactly this case.

## The data is not in anything waybill reads

Scanning every attribute of all 1,275 derivations in the darwin closure JSON
for anything license-bearing — `licen`, `meta`, `spdx`, `homepage`, `descri`:

```
license-ish attributes in the closure JSON: NONE
```

`meta` is evaluation-time metadata and is not part of a derivation. No
re-reading of the closure recovers it; a new source is required.

## How the other tool does it

Read from `package_meta.nix` in sbomnix 1.9.0, not inferred: a **second Nix
evaluation** over the flake's nixpkgs input, which walks package roots and
returns metadata together with each package's `drvPath` and `outPath`. Its
caller then accepts only rows whose store path matches a component it already
has — the file's own comment says *"Python accepts only rows whose
drvPath/outPath matches an SBOM component exactly."*

Both join keys are already in waybill's hand: the closure JSON carries every
`.drv` path and every output path, and milestone 1280 already indexes by output
basename.

## Is such an evaluation available to waybill? **Yes, and purely.**

The blocking question was whether it needs `--impure`, which `argv_is_safe`
refuses. It does not, provided the flakeref is **locked**:

```
nix eval --json --option allow-import-from-derivation false --file meta-lookup-probe.nix
```

```json
[{"drv":"…-bash-interactive-5.3p9.drv","licenses":["GPL-3.0-or-later"],"name":"bash","out":"…"},
 {"drv":"…-autoconf-2.73.drv","licenses":["GPL-3.0-or-later"],"name":"autoconf","out":"…"},
 {"drv":"…-bison-3.8.2.drv","licenses":["GPL-3.0-or-later"],"name":"bison","out":"…"},
 {"drv":"…-brotli-1.2.0.drv","licenses":["MIT"],"name":"brotli","out":"…"}]
```

**0.47s** for four packages against a warm evaluation cache.

Three things this settles:

1. **No `--impure`.** `builtins.getFlake` on a *locked* `github:` flakeref is
   pure, unlike the local-path case milestone 1035 measured as needing it.
2. **nixpkgs carries `spdxId` itself.** `lib.licenses.<attr>.spdxId` is the
   SPDX identifier, so this path may not need milestone 1273's mapping table at
   all — only its *two-state* type, for the attributes that have no `spdxId`.
3. **Both join keys come back** from the same evaluation that produces the
   license, so the join needs nothing inferred.

### A gotcha, hit on the first enumeration attempt

Enumerating all of `legacyPackages` rather than named attributes cost **7.5s**
and then failed:

```
c) For `nix-env`, `nix-build`, `nix-shell` … add { allowUnsupportedSystem = true; }
```

A `tryEval` around the attribute access did **not** contain it. That is one of
the "two `tryEval` traps" milestone 1050 recorded for nixpkgs evaluation, and
any enumeration approach has to handle it. Cost is seconds, not minutes, which
is the useful half of the finding.

## Reproducing

```sh
nix eval --json --option allow-import-from-derivation false \
    --file specs/1281-closure-license-meta/measurements/meta-lookup-probe.nix
```

**Do not** add `--impure`; the point of the probe is that it is not needed.
Note the nix gotcha baked into the probe: an attrset containing a key named
`outPath` is string-coerced by `nix eval --json`, so the probe names it `out`.
The first version returned three bare store paths and no licenses, which looks
exactly like "the data is not there".
