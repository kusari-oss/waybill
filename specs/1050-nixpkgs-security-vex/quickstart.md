# Quickstart — nixpkgs security declarations as VEX

## What an operator does

Nothing new. The feature runs whenever `--nix-closure` does (FR-020a).

```sh
waybill sbom scan --path . --nix-closure \
  --format cyclonedx-json --output sbom.json
```

## What to look for

Coverage first, because it bounds everything else:

```sh
jq -r '.metadata.properties[]
       | select(.name=="waybill:nixpkgs-security").value' sbom.json
```

Then the declarations, if any:

```sh
jq -r '.statements[]
       | select(.impact_statement // "" | test("nixpkgs-declared"))
       | "\(.vulnerability.name)  \(.products[0].subcomponents[0]["@id"])"' \
  sbom.openvex.json
```

And the prose half, which is where the bundled-component findings live:

```sh
jq -r '.components[]
       | select(.properties[]?.name=="waybill:nixpkgs-declaration")
       | "\(.name): \(.properties[] | select(.name=="waybill:nixpkgs-declaration").value)"' \
  sbom.json
```

## Expect nothing on most projects, and know why

**A project that builds has already permitted any insecure package it
contains** — Nix refuses to *evaluate* one otherwise. Measured: moat's
1,275-derivation closure carries zero declarations.

So an empty result is the common case and means "this build accepted no policy
exceptions", not "the check did not run". The coverage annotation is what
distinguishes those two, which is why it is the first thing to read.

## Expect partial coverage, and read the number

Measured on moat: 273 of 380 members confirmed (71%), 72 unreachable (18%),
35 rejected because an attribute of that name builds something else (9%).

That last 9% is the feature working. Those are attributes whose name matches a
closure member but whose output path does not, and accepting them would attach
a security claim to the wrong component.

## Testing it

No public project scanned so far exercises the path (research R5), so the
fixture has to permit an insecure package deliberately. Package names in it
must be synthetic — `waybill-fixture-*` — because real coordinates in a
fixture trip advisory scanning, which has bitten this repository twice.

## Two traps the measurements already hit

- **Store-path prefixes differ between the two sides.** The closure JSON omits
  `/nix/store/`; evaluation includes it. Compare basenames via
  `store_basename`, or get zero matches and conclude the mechanism is broken.
- **`tryEval` alone is not enough.** A missing attribute escapes it, and its
  return value is lazy so throws escape at serialisation time. Needs `or null`
  and `deepSeq` both.
