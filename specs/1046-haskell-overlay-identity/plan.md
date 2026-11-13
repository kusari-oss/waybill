# Implementation Plan: One identity for a Nix-built Haskell dependency the package set does not carry

**Branch**: `1046-haskell-overlay-identity` | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1046-haskell-overlay-identity/spec.md`

## Summary

Three changes, all under `--nix-closure`:

1. **The twin merge.** It runs right after the m1275 identity pass. A versionless design-tier `pkg:hackage/<n>` folds into the build-tier closure member of the same Haskell package, whether or not the build's version satisfies the declared range.
   - **The survivor** keeps the build's identity, and gains the declared ranges.
   - **A build outside the declared range** (the hspec-discover case) records the new C221.
   - **Edges move** with `apply_renames`.
2. **The Hackage-published identity.** A GHC-built member whose version the pinned package set does not carry is checked against Hackage with one GET per member (measured in research R3: 200 means published, 404 means not).
   - **Published:** it becomes `pkg:hackage/<n>@<v>`, with `vcs_url` when it was built from a git checkout.
   - **Otherwise:** it stays `pkg:generic`. C201 records which of five outcomes applied.
   - **Gating:** on by default, and refused under `--offline` or `--no-hackage-lookup`.
3. **The VCS source.** `source_url` reads structured attributes too: at nixpkgs `a799d3e3`, `fetchgit` keeps `url` and `rev` there (R5). A `fetchgit` source member, and each package built from it, carry a native A10 `vcs` reference `git+<url>@<rev>`.

Expected on the corpus: slacklinker's `slack-web` becomes one `pkg:hackage/slack-web@2.3.0.0?vcs_url=…` component, and `tmp-postgres@1.35.0.0`, a fork version Hackage does not publish, stays `pkg:generic` with `hackage-unpublished`. slack-web's `hspec-discover` merges and carries C221.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.

**Primary Dependencies**: Existing only.
- `reqwest` (blocking, on a dedicated thread, as `haskell_packages/fetch.rs::HttpSource` does);
- `serde_json`, `tracing`;
- `version_range::{admits, contradicts}`;
- `closure_identity::{decide, built_by_ghc}`;
- `RawDerivation::attr_str`;
- `apply_renames`;
- `waybill_common::types::purl::Purl`.

**Zero new Cargo dependencies.**

**Storage**: N/A. A per-scan memo of Hackage answers; no disk cache (R3).

**Testing**: `cargo test`.
- **Unit tests:** twin selection (FR-004), the merge's annotation rules (R6), C221, the `vcs_url` encoding (R4), and the `source_url`/`vcs` extraction from both carriers.
- **Integration (`tests/closure_haskell_overlay.rs`):** a stub `nix` and an injected Hackage source, covering every outcome in the data model.
- **Parity:** C221, and the new C201 values.
- **Corpus:** regenerated goldens, reviewed line by line.

**Target Platform**: wherever `--nix-closure` runs.

**Project Type**: CLI.

**Performance Goals**: about one 200–400 ms request per Haskell overlay (R3; 2 on slacklinker), each bounded by a timeout. Nothing for any other member.

**Constraints**:
- Byte-identical output without `--nix-closure`.
- No request for a member no GHC built.
- Degradation never fails the scan (Principle XI, XII.3).

**Scale/Scope**:
- One new pass and one new client.
- One emitter fix.
- One new row (C221), and new C201 values.
- One new flag.
- All five closure goldens regenerate.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, statically linked | Pass | No new crates or subprocesses. |
| IV Type-driven correctness | Pass | The outcomes are an exhaustive enum (C201's existing `HackageIdentity`, extended). PURLs are built through `Purl::new`. No new `.unwrap()` outside tests. |
| V Native first | Pass | The VCS source rides A10 (CycloneDX `vcs`, SPDX `externalRefs`/`software_sourceInfo`) and the PURL's own `vcs_url` qualifier. C221 has a KEEP-NO-NATIVE audit (R7). The check's outcome extends C201 rather than adding a row (R8). |
| VII Test isolation | Pass | The Hackage source is a trait with a stub. No test touches the network. |
| IX Accuracy | Pass | No Hackage identity without both the GHC signal and Hackage's 200 (R2, R3). Unpublished and unchecked versions stay `pkg:generic`. C221 is claimed only when every range parses and rejects the version. |
| X Transparency | Pass | Every candidate records its C201 outcome. C221 makes a relaxed bound visible. The merge keeps the declared ranges. |
| XI Enrichment / XII External sources | Pass | Hackage is an external identity source. `hackage-published` is its provenance (XII.2). Unavailability degrades with a recorded reason (XII.3). The scanned evidence, the closure, remains the authority: Hackage only confirms a version the build already recorded (XII.4). |

No violations. **Re-check after Phase 1:** still passes. The data model adds no field outside the rows above.

## Project Structure

### Documentation (this feature)

```text
specs/1046-haskell-overlay-identity/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/merge-and-identity.md
├── measurements/hackage-probe.sh, hackage-probe-2026-11-13.txt
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/hackage.rs           # new: HackageSource trait, HTTP impl (dedicated thread, timeout), stub for tests
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs  # HackageIdentity gains the five outcomes; reclassify consults HackageSource for GHC-built version-disagrees members; vcs_url when src is VCS
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/twin_merge.rs        # new: select (FR-004), merge (R6), C221 (R7), rename edges
waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs                       # source_url via attr_str (both carriers, urls list); vcs reference for url+rev members and for packages whose src is one
waybill-cli/src/cli/scan_cmd.rs                                              # --no-hackage-lookup; call twin_merge after reclassify, before qualify_backport_identities
waybill-cli/src/generate/{cyclonedx/metadata,spdx/annotations,spdx/v3_annotations}.rs  # C221 (per-component emitters)
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs                   # C221
docs/reference/sbom-format-mapping.md                                        # C201 values, C221
docs/reference/nix-evaluation.md                                             # overlays: the merge, the Hackage check, --no-hackage-lookup
waybill-cli/tests/closure_haskell_overlay.rs                                 # new
waybill-cli/tests/fixtures/closure_haskell_overlay/                          # new, synthetic names (waybill-fixture-*)
waybill-cli/tests/fixtures/public_corpus/nix-closure-*/                      # regenerated
```

**Structure Decision**: the existing single crate, `waybill-cli`. The new logic sits beside the m1275 pass it extends.

## Complexity Tracking

No constitution violations to justify.
