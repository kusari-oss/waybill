# Implementation Plan: Cross-tier SBOM binding

**Branch**: `072-cross-tier-sbom-binding` | **Date**: 2026-05-05 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/072-cross-tier-sbom-binding/spec.md`

## Summary

Emit a **layered binding hash** (VCS commit + lockfile-content-hash + main-module manifest hash, SHA-256 over a canonical JSON envelope) on every non-source-tier mikebom SBOM component. Pair it with standards-native cross-document references (CDX `externalReferences[type: bom]`, SPDX `externalDocumentRefs` + `BUILT_FROM`/`GENERATED_FROM` relationships). Add `mikebom sbom verify-binding` and `mikebom sbom trace-binding` consumer-side commands. Extend mikebom's OpenVEX 0.2.0 sidecar to put each component's `bom-ref`/`SPDXID` into the existing `Statement.products[].identifiers` map alongside the PURL — per-instance VEX without extending OpenVEX upstream. Wire `--vex-propagation-mode {permissive,caveated,strict}` (default `caveated`) into `mikebom sbom enrich`. Document the contract at `docs/reference/cross-tier-binding.md`.

**Approach**: The work is entirely a layer over already-discovered evidence — `mikebom-cli/src/scan_fs/` already extracts VCS commits (Go BuildInfo, git checkouts, cargo-auditable), already canonicalizes lockfiles (cargo `Cargo.lock`, npm `package-lock.json`, etc.), and the milestone-071 `canonicalize_for_compare` helper at `parity/extractors/common.rs` provides the JSON canonical-serialization primitive needed for the binding-hash envelope. The OpenVEX sidecar already exists at `generate/openvex/`; extending `OpenVexProduct.identifiers` to carry `cyclonedx-bom-ref` / `spdx-spdxid` keys is idiomatic OpenVEX 0.2.0 usage that doesn't fork the upstream schema.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–071; no nightly).
**Primary Dependencies**: Existing only — `sha2` (workspace; SHA-256 over the canonical JSON envelope), `serde`/`serde_json` (canonicalization via the milestone-071 `canonicalize_for_compare` helper at `parity/extractors/common.rs`), `data-encoding` (already used for hex-encoding hashes), `tracing`, `anyhow`, `clap` (for the new flag + new subcommands). No new crates.
**Storage**: N/A — binding metadata lives in emitted SBOMs only; no caches, no registries. The `verify-binding` / `trace-binding` commands read SBOMs from operator-supplied paths.
**Testing**: Existing — `cargo +stable test --workspace`. New integration tests under `mikebom-cli/tests/`: (a) `binding_emission.rs` — emits an image-tier SBOM with `--bind-to-source <source-sbom>` and asserts the binding annotations + cross-document references appear with correct strength; (b) `binding_verify.rs` — round-trips a (source SBOM, image SBOM) pair through `mikebom sbom verify-binding` and asserts pass/fail; (c) `binding_drift.rs` — synthesizes a binding-hash mismatch and asserts strict-mode `mikebom sbom enrich --vex-propagation-mode strict` refuses propagation; (d) `vex_per_instance.rs` — encodes the worked-example case from US2 AS#4 / SC-003 (same PURL, two instances, one bound, one unbound) and asserts the post-enrichment OpenVEX sidecar has per-instance subjects.
**Target Platform**: Same as alpha.14 — Linux + macOS user-space.
**Project Type**: Existing three-crate workspace per Constitution VI; the milestone is `mikebom-cli`-only.
**Performance Goals**: Binding hash computation O(N) in the size of the source-tier inputs (read 3 files + SHA-256 each = milliseconds per project). The new `verify-binding` command runs in <2s on the existing 9 ecosystem fixtures (it reads two SBOMs + recomputes the hashes; no scan).
**Constraints**: MUST NOT regress alpha.14's 27 byte-identity goldens for source-tier SBOMs. The binding annotation is additive on **non-source-tier** SBOMs (`mikebom:sbom-tier: build` or `deployed`); source-tier SBOMs are unchanged. MUST NOT break the cross-format-parity test suite at `tests/holistic_parity.rs` from milestone 071 — the new `mikebom:source-document-binding` annotation gets a new catalog row with the right Directionality (SymmetricEqual after canonicalization).
**Scale/Scope**: 6 known ecosystems (cargo / npm / pip / gem / maven / golang) × the binding-input plumbing per ecosystem. ~5 new source files in `mikebom-cli/src/binding/`, ~2 modifications to `generate/openvex/`, ~3 modifications to `cli/{enrich,sbom_cmd}.rs`, ~5 new integration tests, 1 new doc.

## Constitution Check

Running through the v1.4.0 principles before Phase 0:

- **I. Pure Rust, Zero C** — ✅ pure-Rust additions only.
- **II. eBPF-Only Observation** — ✅ binding metadata is derived from already-discovered components (already inside the eBPF + lockfile + manifest evidence model). No new discovery; orthogonal.
- **III. Fail Closed** — ✅ `mikebom sbom verify-binding` failure exits non-zero with a structured rationale; `--bind-to-source <path>` failure to load the source SBOM aborts the scan rather than silently producing unbound output (FR-011).
- **IV. Type-Driven Correctness** — ✅ new newtypes `BindingHash(String)`, `SourceDocumentId`, enum `BindingStrength { Verified, Weak, Unknown }`, enum `VexPropagationMode { Permissive, Caveated, Strict }`. Production code uses `anyhow::Result`; test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **V. Specification Compliance — standards-native fields take precedence** — ✅ this milestone's central design decision (FR-004) explicitly puts the cross-document reference into CDX `externalReferences[type: bom]` and SPDX `externalDocumentRefs` + `BUILT_FROM`/`GENERATED_FROM` relationships. Only the per-component binding hash + strength label (which have no native equivalent) live in a `mikebom:source-document-binding` annotation. Per Principle V's named pattern: native-first, mikebom-annotation supplementary, justification clause documented in `docs/reference/cross-tier-binding.md` (FR-010).
- **VI. Three-Crate Architecture** — ✅ all changes confined to `mikebom-cli`.
- **VII. Test Isolation** — ✅ no eBPF; tests run unprivileged.
- **VIII. Completeness** / **IX. Accuracy** — ✅ orthogonal; binding emission is additive metadata that doesn't change the component set.
- **X. Transparency** — ✅ the explicit `binding: unknown` marker with structured `reason` (FR-003 / SC-006) IS the transparency mechanism for the unbound case, exactly as Principle X mandates.
- **XI. Enrichment** — ✅ the binding annotation is enrichment that gracefully degrades (`weak`, `unknown`) when underlying evidence is missing rather than failing.
- **XII. External Data Source Enrichment** — ✅ orthogonal — binding hash is computed from on-disk evidence, not external sources.

**Gates: PASS.** No deviations to record. Milestone strengthens Principle V (mikebom-annotation only where standards-native is insufficient, with documented justification).

## Project Structure

### Documentation (this feature)

```text
specs/072-cross-tier-sbom-binding/
├── spec.md                                          # complete (with 3 clarifications)
├── plan.md                                          # this file
├── research.md                                      # Phase 0 output (next)
├── data-model.md                                    # Phase 1 output
├── quickstart.md                                    # Phase 1 output
├── contracts/
│   ├── binding-hash-v1.md                           # canonical envelope + SHA-256 contract (FR-002)
│   ├── source-document-binding-annotation.md        # mikebom:* annotation shape per format (FR-001/FR-004)
│   └── openvex-instance-identifiers.md              # OpenVEX Product.identifiers extension (FR-008)
├── checklists/
│   └── requirements.md                              # complete
└── tasks.md                                         # Phase 2 output (later — /speckit.tasks)
```

### Source Code (repository root)

The milestone touches only `mikebom-cli`. Concrete paths:

```text
mikebom-cli/
├── src/
│   ├── binding/                          # NEW MODULE
│   │   ├── mod.rs                        # public API: compute_binding_hash(), BindingStrength, etc.
│   │   ├── hash.rs                       # FR-002 algorithm — composite envelope + SHA-256
│   │   ├── source_inputs.rs              # ecosystem-specific extractors for VCS / lockfile / manifest
│   │   ├── verify.rs                     # consumer-side recompute + compare logic
│   │   └── annotation.rs                 # serialize/deserialize mikebom:source-document-binding
│   ├── cli/
│   │   ├── sbom_cmd.rs                   # ADD verify-binding + trace-binding subcommands
│   │   ├── enrich.rs                     # ADD --vex-propagation-mode flag (FR-007)
│   │   └── scan_cmd.rs                   # ADD --bind-to-source flag (FR-011)
│   ├── generate/
│   │   ├── openvex/
│   │   │   └── statements.rs             # EXTEND OpenVexProduct with identifiers map (FR-008)
│   │   ├── cyclonedx/
│   │   │   ├── builder.rs                # EMIT mikebom:source-document-binding property + externalReferences[type: bom]
│   │   │   └── metadata.rs               # SAME for metadata.component
│   │   └── spdx/
│   │       ├── annotations.rs            # EMIT envelope-wrapped binding on Package.annotations[]
│   │       ├── packages.rs               # EMIT externalDocumentRefs + BUILT_FROM relationship
│   │       ├── v3_annotations.rs         # SAME on SPDX 3 Annotation.statement
│   │       └── relationships.rs          # SPDX 3 Relationship[built_from]
│   ├── parity/
│   │   └── extractors/mod.rs             # ADD catalog row for mikebom:source-document-binding
│   ├── sbom/
│   │   └── mutator.rs                    # WIRE VEX propagation per FR-007 (currently a JSON-Patch stub)
│   └── lib.rs                            # pub mod binding (so integration tests can call compute_binding_hash directly)
└── tests/
    ├── binding_emission.rs               # NEW — round-trip image-tier scan with --bind-to-source
    ├── binding_verify.rs                 # NEW — verify-binding command end-to-end
    ├── binding_drift.rs                  # NEW — strict-mode refusal on hash mismatch
    └── vex_per_instance.rs               # NEW — US2 AS#4 / SC-003 worked-example regression

docs/
└── reference/
    ├── cross-tier-binding.md             # NEW — FR-010 published contract for external verifiers
    └── sbom-format-mapping.md            # ADD row for mikebom:source-document-binding to the parity-catalog table
```

**Structure Decision**: New `mikebom-cli/src/binding/` module owns the hash algorithm + source-input extraction + verify logic. Emission lives in the existing `generate/{cyclonedx,spdx,openvex}/` modules — no shape forks. Two new CLI subcommands (`verify-binding`, `trace-binding`) under the existing `mikebom sbom` noun-verb tree (per the user's CLI-pattern memory). The new flag (`--vex-propagation-mode`) lives on `mikebom sbom enrich`, per Q3 clarification.

## Phase 0: Outline & Research

**Output**: [research.md](research.md) — full content authored alongside this plan.

The 3 spec clarifications resolved the highest-impact unknowns. Research focuses on three concrete operational unknowns:

1. **Per-ecosystem VCS commit + lockfile + manifest extraction.** What is the canonical `(vcs, lockfile, manifest)` triple for each of mikebom's 6 source-tier ecosystems (cargo / npm / pip / gem / maven / golang)? Where in mikebom's existing scan code is each input already extracted? Resolved per-ecosystem in research.md §1.

2. **OpenVEX 0.2.0 `Product.identifiers` schema.** Confirm the existing `OpenVexProduct` struct can be extended with an `identifiers: BTreeMap<String, String>` field in a way that's both wire-compatible with OpenVEX 0.2.0's published schema AND machine-parseable by existing consumers (vexctl, etc.). Resolved in research.md §2.

3. **Cross-document-reference shapes per format.** Confirm exactly what CDX `externalReferences[type: bom]` and SPDX `externalDocumentRefs` + `BUILT_FROM`/`GENERATED_FROM` look like and where they attach (document-level vs component-level). Resolved in research.md §3.

## Phase 1: Design & Contracts

**Outputs**: [data-model.md](data-model.md), [contracts/binding-hash-v1.md](contracts/binding-hash-v1.md), [contracts/source-document-binding-annotation.md](contracts/source-document-binding-annotation.md), [contracts/openvex-instance-identifiers.md](contracts/openvex-instance-identifiers.md), [quickstart.md](quickstart.md), and an agent-context update.

### 1. Data model (`data-model.md`)

The 7 spec entities (Source-tier SBOM / Build-tier / Image-tier / Cross-tier binding / Binding hash / Binding strength / VEX propagation mode / Per-instance VEX state / Aggregate VEX state) plus the concrete Rust shapes:

- `BindingHashInputs { vcs: Option<String>, lockfile: Option<String>, manifest: Option<String> }` — the FR-002 layered triple.
- `BindingHash(String)` — newtype for the SHA-256 hex output.
- `BindingStrength { Verified, Weak, Unknown }` — enum, derived from how many `BindingHashInputs` sides are populated.
- `SourceDocumentBinding { source_doc_id: SourceDocumentId, hash: BindingHash, strength: BindingStrength, reason: Option<String> }` — the per-component annotation payload.
- `SourceDocumentId { sha256: String, iri: Option<String> }` — stable identifier for the source SBOM document (SHA-256 of the canonical JSON bytes; optional IRI for human-readable cross-reference).
- `VexPropagationMode { Permissive, Caveated, Strict }` — the `--vex-propagation-mode` enum.
- `OpenVexProduct.identifiers: BTreeMap<String, String>` — extended field per FR-008.

### 2. Contracts (`contracts/`)

Three contract documents:

- **`binding-hash-v1.md`** — the algorithm v1 specification: canonical JSON envelope shape (`{"vcs":"<commit-or-null>","lockfile":"<sha256-or-null>","manifest":"<sha256-or-null>","algo":"v1"}`), SHA-256 algorithm, hex output, ecosystem-specific input-source rules (Go BuildInfo `vcs.revision` for golang; `git rev-parse HEAD` for git checkouts; cargo-auditable embedded VCS for Rust binaries; etc.).
- **`source-document-binding-annotation.md`** — the per-format carrier shapes: CDX `properties[].name == "mikebom:source-document-binding"` with JSON-encoded value; SPDX 2.3 envelope inside `Package.annotations[].comment` per the existing `MikebomAnnotationCommentV1` shape; SPDX 3 same envelope inside `Annotation.statement`. Plus the standards-native sibling: CDX `externalReferences[]` entry, SPDX `externalDocumentRefs[]` + `Relationship.relationshipType: BUILT_FROM`.
- **`openvex-instance-identifiers.md`** — the FR-008 OpenVEX `Product.identifiers` extension contract: identifier-type keys (`purl`, `cyclonedx-bom-ref`, `spdx-spdxid`); pre-072 consumer fallback path; aggregation rule for per-PURL consumers (`affected ⊕ unbound-and-not-explicitly-vexed = affected`).

### 3. Quickstart (`quickstart.md`)

Five operator-facing recipes:

- **Recipe 1**: Generate a source-tier SBOM, generate an image-tier SBOM with `--bind-to-source <source.cdx.json>`, run `mikebom sbom verify-binding <image> <source>`, observe `verified` results.
- **Recipe 2**: Take a wrong source SBOM (different commit), run verify-binding, observe explicit failure with mismatch rationale.
- **Recipe 3**: VEX merge in `caveated` mode — propagate a source-tier `not_affected` to image-tier, observe `binding-unverified` caveat on unbound instances.
- **Recipe 4**: Strict mode — same input as Recipe 3 but `--vex-propagation-mode strict` refuses propagation.
- **Recipe 5**: The worked-example case (US2 AS#4 / SC-003) — Go networking CVE, two instances of `golang.org/x/net` (one first-party bound, one base-layer unbound), aggregate VEX state correctly reports `affected` because the unbound instance defaults to "could be affected".

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after writing artifacts.

## Re-evaluate Constitution Check

Post-design review of the artifacts above: still ✅ on all 12 principles. The plan does NOT introduce a new format (binding annotation reuses the existing `MikebomAnnotationCommentV1` envelope from milestone 071 + the existing CDX `properties[]` carrier); does NOT extend OpenVEX upstream (uses the existing `Product.identifiers` mechanism per OpenVEX 0.2.0); the cross-document references are entirely standards-native. Principle V is materially strengthened by FR-004 / FR-010's documentation requirement.

The breaking change (`--vex-propagation-mode` default flips from implicit-permissive to explicit-caveated) is documented in spec SC-008 + the milestone-072 release notes. `--vex-propagation-mode permissive` is the back-compat opt-out.

**Gates: PASS post-design.** No new deviations.

## Complexity Tracking

*(empty — no constitution gate violations)*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| *(none)* | | |
