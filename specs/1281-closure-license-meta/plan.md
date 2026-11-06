# Implementation Plan: licenses for nix closure components

**Branch**: `1281-closure-license-meta` | **Date**: 2026-11-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification at `specs/1281-closure-license-meta/spec.md`

## Summary

The closure JSON carries no license data at all, so closure components reach a
compliance reviewer unlicensed — 96 of them on the measured target are licensed
by sbomnix and silent in waybill. This milestone adds a second, pure `nix eval`
that returns `(drvPath, outPath, licenses)` for a scoped set of package names
and joins the result back to closure components by **exact store path**. The
name scopes the evaluation; the store path is the join, which is what keeps the
accuracy bar in Principle IX intact. Measured: 1.05 s and 70% of the gap for
the default shape, against 165.6 s and 85% for exhaustive enumeration — so
enumeration is therefore rejected, not deferred behind a flag.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.

**Primary Dependencies**: Existing only — `serde`/`serde_json` (the eval's JSON
rows), `spdx` via `SpdxExpression::try_canonical` (license canonicalisation,
already used by the rpm and ipk readers), `tracing`, `anyhow`. **Zero new
Cargo dependencies.** No new CLI flag — see the note under Performance
Goals.

**External tools**: the host's `nix`, already a hard requirement of this tier
since milestone 1034. The evaluation is pure — no `--impure`, so it passes the
existing `argv_is_safe` guard — and reuses `eval::invoke`'s `run_bounded`,
`argv_is_safe` and `is_safe_attribute_name` helpers verbatim (m1034 / m1035 /
m1066).

**Storage**: N/A — in-process per scan. The existing per-revision nixpkgs cache
at `~/.cache/waybill/nixpkgs/<rev>/` is reused unchanged; this milestone adds
no new cache and no new persistence.

**Testing**: `./scripts/pre-pr.sh` (no `+toolchain`, per #1189), plus a new
integration suite over committed closure fixtures. Corpus goldens refresh
through `public-corpus.yml` dispatch, never locally.

**Target Platform**: wherever the closure tier already runs — Linux and macOS
hosts with `nix` present.

**Project Type**: single Rust workspace.

**Performance Goals**: the name-scoped shape adds ~1 s to a scan of the
measured target.

An earlier draft of this plan exposed the 165.6 s enumeration shape behind an
opt-in flag. That is **out of scope**: no requirement in the spec authorises a
new flag, and a plan that promises a deliverable no task builds is how scope
arrives uncosted. The enumeration shape stays where it belongs — a measured
alternative in research R4, available to a future milestone that specifies
it.

**Constraints**: the join must be by exact store path (FR-002); a scan whose
license lookup fails must still emit its document (FR-006); the Nix expression
must survive a population containing `null` aliases, missing attributes
and throwing derivations without aborting the scan (R2).

**Scale/Scope**: the measured target carries 1,749 closure derivations, 604 of
them `pname`-bearing, 478 distinct names; the evaluation returns 747 rows.

## Constitution Check

| Principle | Gate | Verdict |
| --- | --- | --- |
| I. Pure Rust, statically linked | No new crates; no C; `nix` is a subprocess already required by this tier | **PASS** |
| III. Fail closed | A license gap is not a correctness failure. FR-006 degrades and records why, rather than failing the scan — consistent with how the tier already treats an unavailable `nix` | **PASS** |
| IV. Type-driven correctness | No `.unwrap()` in production; licenses canonicalise through the existing `SpdxExpression` newtype rather than raw strings | **PASS** |
| V. Specification compliance | Licenses land in the native `licenses[]` slot in all three formats, not a `waybill:` property. Only the *provenance and coverage* metadata becomes an annotation | **PASS** |
| IX. Accuracy | The join is an exact store-path match, not a name heuristic. R5 shows why that matters: `autoconf269` carries `pname = autoconf`, so a name join would attach 2.73's license to 2.69 | **PASS** |
| X. Transparency | FR-007 records how many components wanted a license and how many received one, so a reviewer can distinguish a closure with few licenses from a lookup that silently did nothing | **PASS** |
| XI. Enrichment | License data is a named enrichment target; this is that, from the authoritative upstream rather than a third-party index | **PASS** |
| Boundary 1 (no lockfile discovery in trace mode) | Scan-mode only; introduces no components, attaches metadata to components the closure already produced | **PASS** |

No violations. Complexity Tracking is therefore empty.

One gate deserves naming rather than a tick: R5 establishes that coverage can
never reach 100%, because a `drvPath` encodes the whole transitive input
closure and the same package built in a different context is a different store
path. Principle X is what carries that — the coverage annotation must make an
incomplete join legible instead of leaving the reviewer to infer it from
silence.

## Project Structure

### Documentation (this feature)

```text
specs/1281-closure-license-meta/
├── spec.md
├── plan.md                 # this file
├── research.md             # Phase 0 — R1..R7, all measured
├── data-model.md           # Phase 1
├── quickstart.md           # Phase 1
├── contracts/
│   └── license-lookup.md   # Phase 1 — the eval contract and its guards
├── checklists/requirements.md
└── measurements/
    ├── README.md
    ├── name-scoped-probe.nix          # the default shape (1.05 s)
    ├── name-scoped-multiroot-probe.nix# root-set sensitivity (R6)
    ├── enumerate-probe.nix            # the opt-in shape (165.6 s)
    ├── meta-lookup-probe.nix
    └── meta-enumerate-probe.nix
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/
├── eval/
│   ├── invoke.rs        # reused: run_bounded, argv_is_safe, is_safe_attribute_name
│   └── licenses.rs      # NEW — build the expression, run it, parse rows
├── closure/
│   ├── mod.rs           # wire the lookup after identity reclassification
│   ├── emit.rs          # licenses[] into the native slot; coverage annotation
│   └── summary.rs       # coverage counters
└── identity.rs          # unchanged

waybill-cli/tests/
└── closure_licenses.rs  # NEW — fixtures for join, multi-license, degradation
```

**Structure Decision**: single project. The license lookup is a second
evaluation alongside the existing closure evaluation, so it belongs beside it
in `nix/eval/` rather than in a new module tree; emission changes stay in the
closure emitter that already owns the component shape.

The wiring point is **after** `closure_identity::reclassify`, matching how
milestone 1280 ordered its edge construction — renames move endpoints and
identities in the same step, so anything keyed on component identity has to run
downstream of it.

## Complexity Tracking

No constitutional violations; this section is intentionally empty.
