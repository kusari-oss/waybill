# Quickstart: verifying 1436

## Hermetic (normal lanes)

```sh
cargo test -p waybill --test nix_closure_backport_vex
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

`nix_closure_backport_vex.rs` covers these cases with a stub `nix` (FR-011):
- a patched component: its PURL is qualified in all three formats, there is one `fixed` statement per CVE, and the CDX entry is `resolved` on its bom-ref;
- an unpatched build of the same name and version: unqualified, and no statement names it;
- a component patched for one of two CVEs: one statement;
- a filename-derived association: the grade note is kept;
- one component from two variants that both resolve the CVE: both hashes in the qualifier, one statement;
- two variants where only one resolves the CVE: qualifier present, no statement, C220 = 1;
- a nixpkgs declaration for the same (component, CVE): the `fixed` statement is withheld;
- `--split` and `--tier` projections: statements stay with their components, and C193 counts the rest;
- no orphaned relationship endpoint anywhere;
- a scan without `--nix-closure`: byte-identical.

## Scanner gate (SC-001, SC-002, SC-004)

The grype and database pins are the ones in `.github/workflows/recall-truth.yml`:

```sh
export GRYPE_DB_CACHE_DIR=$(mktemp -d) GRYPE_DB_AUTO_UPDATE=false GRYPE_DB_VALIDATE_AGE=false
G=$(nix build --no-link --print-out-paths github:NixOS/nixpkgs/0bb2eedc20e4739f29e8045034f2828ad7054f8f#grype)/bin/grype
$G db import <vulnerability-db_v6.1.10_2026-11-08…tar.zst>   # sha256 dcca2472…
```

For each regenerated closure golden, and each of the six wrapper targets:
- run grype with and without the emitted `waybill.openvex.json`;
- every finding whose (PURL without qualifiers, CVE) is in `pedigree.patches[].resolves` must disappear, and nothing else may.
- Without VEX, the findings must equal the pre-change document's, for grype and for osv-scanner.

`measurements/fixed-qualified-probe.sh` is the synthetic version of this, already measured at 17/17 on main's goldens. The implementation re-runs it with the emitted sidecar.

## Corpus

Dispatch `public-corpus.yml` with `regen_goldens=true`. Expect changes only on closure targets with CVE-resolving backports:
- those components' PURLs gain `?nix_drv=`;
- `vulnerabilities[]` pairs collapse to single `resolved` entries;
- the sidecar changes the same way.

Every other golden must stay byte-identical (SC-005).
