# Implementation Plan: Identifiers — built-in + user-defined

> **Post-implementation rename note (2026-05-03, before merge)**: this milestone shipped under the renamed concept "identifiers" (not "source identifiers") with a dedicated-flag CLI (`--repo` / `--git-ref` / `--image-id` / `--attestation` / `--id <scheme>=<value>`) instead of the originally-drafted `--with-source <scheme>:<value>`. See spec.md's prepended note for the full rename scope. The plan below uses the original draft's terminology ("source identifiers", `--with-source`, `mikebom:source-identifiers`) — treat those as historical references to what was renamed pre-merge.

# Implementation Plan: Source identifiers — built-in + user-defined

**Branch**: `073-source-identifiers` | **Date**: 2026-05-05 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/073-source-identifiers/spec.md`

## Summary

Add `--with-source <scheme>:<value>` (repeatable) to `mikebom sbom scan` (path + image modes) and `mikebom trace`. Auto-detect `repo:` from git origin remote (with `upstream` + first-listed fallbacks) on `--path`; auto-detect `image:<registry>/<name>:<tag>@sha256:<digest>` from the resolved image reference + digest on `--image`. Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) get value-validated and ride standards-native carriers per format (CDX `metadata.component.externalReferences[]`, SPDX 2.3 dual-carrier on main-module `Package.externalRefs[PERSISTENT-ID]` + `creationInfo.creators` text line, SPDX 3 `Element.externalIdentifier[]`). User-defined schemes ride a new `mikebom:source-identifiers` document-level annotation per Constitution Principle V. Add a parity catalog row (likely C47) registering the annotation as `Directionality::SymmetricEqual`. Publish `docs/reference/source-identifiers.md` so external SBOM consumers can decode mikebom-emitted identifiers without source access.

**Approach**: Layer purely additive emission code on top of milestone-072's source-document-binding infrastructure. The git-remote shell-out reuses milestone 053's `Command::new("git")` pattern. The per-format carriers reuse the milestone-072 emit sites (CDX `metadata.component`, SPDX 2.3 `creationInfo` + main-module Package, SPDX 3 `SpdxDocument`-element). The `mikebom:source-identifiers` annotation reuses milestone-071's `MikebomAnnotationCommentV1` envelope + cross-format-parity test infrastructure. No new crates, no new toolchains, no new external dependencies.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–072; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (envelope decode), `tracing` (info/warn logs), `anyhow` (CLI error propagation), `clap` (`ValueEnum` not needed — `Vec<String>` with manual validation is fine since the scheme syntax is regex-bounded; alternatively a custom `Identifier` newtype that implements `FromStr` and clap's `Args`-derive picks it up). `Command::new("git")` for the auto-detection shell-out (same pattern as milestone 053's `git describe`). No new `Cargo.toml` deps.
**Storage**: N/A — identifier metadata lives in emitted SBOMs only.
**Testing**: Existing — `cargo +stable test --workspace`. New: (a) `binding/identifiers.rs` unit tests for the `Identifier` newtype + scheme validation; (b) `mikebom-cli/tests/source_identifiers_emission.rs` integration test exercising auto-detect on a `tempfile::tempdir() + git init + git remote add origin ...` fixture, plus manual `--with-source` flags on path / image / trace; (c) extend `holistic_parity.rs` with the new C47 catalog row.
**Target Platform**: Same as alpha.15 — Linux + macOS user-space.
**Project Type**: Existing three-crate workspace per Constitution VI; the milestone is `mikebom-cli`-only.
**Performance Goals**: Auto-detection is one git subprocess (~5–20 ms on a normal repo). The full scan path stays performance-equivalent to alpha.15 — no new walk passes.
**Constraints**: MUST NOT regress alpha.15's 27 byte-identity goldens for non-git fixtures (no auto-detection fires when no git remote present). The git-tracked fixtures (cargo-workspace, maven-multi-module-reactor) WILL get one additive identifier slot per their format's golden — that's the FR-012 expected regen. Source-tier scans on non-git inputs are byte-identical.
**Scale/Scope**: 4 built-in schemes × 3 formats = 12 carrier-mapping rules. Plus the user-defined-passthrough path (1 mechanism). ~5 new files in `mikebom-cli/src/binding/identifiers/` (it's a new submodule under the milestone-072 binding/ tree), ~2 modifications to `cli/scan_cmd.rs`, ~2 to `cli/trace_cmd.rs`, ~3 to `generate/{cyclonedx,spdx,openvex}/`, ~2 new integration tests, 1 new doc, 1 catalog row.

## Constitution Check

Running through v1.4.0 principles before Phase 0:

- **I. Pure Rust, Zero C** — ✅ pure Rust + an existing `Command::new("git")` subprocess (already a project dependency).
- **II. eBPF-Only Observation** — ✅ orthogonal: identifiers are operator-attached metadata, not new discovery. Auto-detection from git remote is a "metadata enrichment" path (Constitution XII), and the git origin URL is operator-controlled local config — not externally fetched.
- **III. Fail Closed** — ✅ auto-detection NEVER fails the scan; it logs and continues. This is correct per the spec — operators can manually attach identifiers if auto-detection didn't fire. Manual flag failures (malformed scheme) ARE fail-closed at clap parse time.
- **IV. Type-Driven Correctness** — ✅ new newtypes: `Identifier { scheme: SchemeName, value: IdentifierValue }`, `SchemeName(String)` (regex-validated), `IdentifierKind { Builtin(BuiltinScheme), UserDefined }`, enum `BuiltinScheme { Repo, Git, Image, Attestation }`. No `.unwrap()` in production. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **V. Specification Compliance — standards-native fields take precedence** — ✅ this is the central design. FR-005 explicitly maps each built-in scheme to a standards-native carrier per format (`externalReferences[type:vcs]` for CDX, `Package.externalRefs[PERSISTENT-ID]` for SPDX 2.3, `Element.externalIdentifier[]` for SPDX 3). User-defined schemes ride a `mikebom:source-identifiers` annotation ONLY because no native carrier accepts arbitrary opaque namespaces — this is the documented Principle V exception path. Justification clause goes in `docs/reference/source-identifiers.md`.
- **VI. Three-Crate Architecture** — ✅ all changes confined to `mikebom-cli`.
- **VII. Test Isolation** — ✅ no eBPF; new tests run unprivileged. The `git init` inside `tempdir` for integration tests is hermetic.
- **VIII. Completeness** / **IX. Accuracy** — ✅ orthogonal: identifiers are metadata, not components.
- **X. Transparency** — ✅ FR-001 / FR-006 specify info-level logging when auto-detection skips or when manual override wins. The chosen git remote name is recorded in the emitted carrier's comment field (FR-007).
- **XI. Enrichment** — ✅ identifiers gracefully degrade: missing git → no auto-detected `repo:`, scan still emits with whatever manual flags were supplied.
- **XII. External Data Source Enrichment** — ✅ the git origin URL is local config, not a fetch — but the principle's "annotated with provenance" requirement is met by the comment field naming the source remote.

**Gates: PASS.** No deviations to record. The user-defined-namespace `mikebom:source-identifiers` annotation IS justified per Principle V's documented-exception clause; the justification will be in `docs/reference/source-identifiers.md` per FR-010.

## Project Structure

### Documentation (this feature)

```text
specs/073-source-identifiers/
├── spec.md                                          # complete (with 3 clarifications)
├── plan.md                                          # this file
├── research.md                                      # Phase 0 output (next)
├── data-model.md                                    # Phase 1 output
├── quickstart.md                                    # Phase 1 output
├── contracts/
│   ├── identifier-shape.md                          # Identifier syntax + scheme registry
│   └── source-identifiers-annotation.md             # mikebom:source-identifiers carrier shape
├── checklists/
│   └── requirements.md                              # complete
└── tasks.md                                         # Phase 2 output (later — /speckit.tasks)
```

### Source Code (repository root)

The milestone touches only `mikebom-cli`. Concrete paths:

```text
mikebom-cli/
├── src/
│   ├── binding/
│   │   ├── identifiers/                  # NEW SUBMODULE under milestone-072 binding/
│   │   │   ├── mod.rs                    # public API: Identifier newtype, BuiltinScheme enum,
│   │   │   │                              #   IdentifierKind, parse + validate.
│   │   │   ├── auto_detect.rs            # git_remote_origin_url(), with the 3-step fallback;
│   │   │   │                              #   image_reference_to_identifier() for image-tier.
│   │   │   └── validators.rs             # per-built-in-scheme validators (repo: / git: /
│   │   │                                  #   image: / attestation: parse+validate).
│   │   └── mod.rs                        # add `pub mod identifiers;` declaration.
│   ├── cli/
│   │   ├── scan_cmd.rs                   # ADD --with-source <Vec<String>> flag on ScanArgs;
│   │   │                                  #   wire auto-detection at scan-start; thread the
│   │   │                                  #   resolved Vec<Identifier> through to the emitters.
│   │   └── trace_cmd.rs                  # SAME flag on TraceArgs (no auto-detection).
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   └── metadata.rs               # EMIT identifiers as metadata.component.externalReferences[]
│   │   │                                  #   with per-scheme `type` mapping (vcs / distribution /
│   │   │                                  #   attestation / other).
│   │   ├── spdx/
│   │   │   ├── document.rs               # EMIT creationInfo.creators "Tool: ... source: <id>" lines
│   │   │   │                              #   for each built-in identifier (the dual-carrier
│   │   │   │                              #   redundant text path).
│   │   │   ├── packages.rs               # EMIT main-module Package's externalRefs[PERSISTENT-ID]
│   │   │   │                              #   for each built-in identifier (the typed primary).
│   │   │   ├── annotations.rs            # EMIT user-defined identifiers under
│   │   │   │                              #   mikebom:source-identifiers document-level annotation.
│   │   │   ├── v3_annotations.rs         # SAME for SPDX 3 user-defined namespace.
│   │   │   └── v3_document.rs            # EMIT every identifier (built-in + user-defined) into
│   │   │                                  #   Element.externalIdentifier[] on the SpdxDocument
│   │   │                                  #   (SPDX 3 multi-identifier model is the perfect fit).
│   │   └── mod.rs                        # ADD source_identifiers: Vec<Identifier> field on
│   │                                      #   ScanArtifacts<'_> so all 3 emitters see the same
│   │                                      #   resolved identifier list (built-in + user-defined,
│   │                                      #   with auto-detected entries first).
│   └── parity/
│       └── extractors/mod.rs             # ADD catalog row C47 for mikebom:source-identifiers,
│                                          #   Directionality::SymmetricEqual.
├── tests/
│   ├── source_identifiers_emission.rs    # NEW — happy path: tempdir + git init + scan
│   │                                      #   emits repo: identifier in all 3 formats.
│   └── source_identifiers_manual.rs      # NEW — manual --with-source flags including
│                                          #   user-defined namespaces; override behavior;
│                                          #   per-tier (path / image / trace).

docs/
└── reference/
    ├── source-identifiers.md             # NEW — FR-010 published reference for external
    │                                      #   verifiers. Per-scheme syntax, per-format
    │                                      #   carrier table, decode recipes.
    └── sbom-format-mapping.md            # ADD row for mikebom:source-identifiers to the
                                           #   parity-catalog table.
```

**Structure Decision**: New `mikebom-cli/src/binding/identifiers/` submodule keeps source-identifier logic colocated with the milestone-072 binding/ tree. Rationale: identifiers are functionally analogous to milestone-072's `SourceDocumentId` (both are document-level identity attachments) and milestone 074 will resolve identifiers against `SourceDocumentBinding` annotations — keeping them in the same module-tree makes the cross-reference natural. CLI surface stays thin (one new flag per command); per-format emission lives in the existing `generate/{cyclonedx,spdx,openvex}/` tree alongside the milestone-072 emit sites.

## Phase 0: Outline & Research

**Output**: [research.md](research.md) — full content authored alongside this plan.

The 3 spec clarifications resolved the highest-impact unknowns. Research focuses on three concrete operational unknowns:

1. **Per-built-in-scheme validators**. Exact regex / parse rules for `repo:`, `git:`, `image:`, `attestation:`. Resolved in research.md §1 with concrete validator behavior + the malformed-input fallback (warn + emit-as-opaque).

2. **CDX `externalReferences[].type` mapping per scheme**. CDX 1.6 enumerates a fixed list of valid `type` values. Confirm the per-scheme map: `repo:` → `vcs`, `git:` → `vcs`, `image:` → `distribution`, `attestation:` → `attestation`. Resolved in research.md §2.

3. **Image-reference → `image:` identifier extraction**. Where in mikebom-cli is the resolved image reference + digest already known? It's the output of `mikebom sbom scan --image <ref>` after the OCI pull / docker save / tarball-load step. Resolved in research.md §3 by pointing at the existing `scan_fs/oci_pull/` + `scan_fs/docker_image.rs` resolution sites.

## Phase 1: Design & Contracts

**Outputs**: [data-model.md](data-model.md), [contracts/identifier-shape.md](contracts/identifier-shape.md), [contracts/source-identifiers-annotation.md](contracts/source-identifiers-annotation.md), [quickstart.md](quickstart.md), and an agent-context update.

### 1. Data model (`data-model.md`)

The 5 spec entities (Source identifier, Built-in scheme, User-defined scheme, Auto-detected identifier, Manual identifier) plus the concrete Rust shapes:

- `Identifier { scheme: SchemeName, value: IdentifierValue, kind: IdentifierKind, source_label: Option<String> }` — the canonical type. `kind` is `Builtin(BuiltinScheme)` or `UserDefined` (decided at parse time per the FR-004 regex + the BuiltinScheme registry). `source_label` records "auto-detected from git remote `origin`" or similar when auto-detection fired.
- `SchemeName(String)` — newtype, regex-validated `^[a-z][a-z0-9_-]*$` per FR-004. Construction returns `Result<SchemeName, IdentifierError>`.
- `IdentifierValue(String)` — newtype, opaque post-parse (everything after the first `:`). Validation depends on `kind`: built-in schemes run their per-scheme validator (research.md §1); user-defined are unvalidated.
- `IdentifierKind { Builtin(BuiltinScheme), UserDefined }`.
- `BuiltinScheme { Repo, Git, Image, Attestation }` — `clap::ValueEnum` not needed since `Identifier` parses the scheme prefix itself; the enum is a closed registry the parser maps `SchemeName` against.
- `IdentifierError` — `thiserror`-based enum: malformed scheme prefix, unknown scheme (parses successfully but no validator), built-in-scheme value validation failure.

### 2. Contracts (`contracts/`)

Two contract documents:

- **`identifier-shape.md`** — the wire-format contract: `<scheme>:<value>` with first-`:`-only split, scheme regex `^[a-z][a-z0-9_-]*$`, the 4 built-in schemes' value-syntax rules, the user-defined-passthrough rule, the `image:` canonical shape per the Q3 clarification (`image:<registry>/<name>:<tag>@sha256:<digest>`), the deterministic ordering contract per FR-009.
- **`source-identifiers-annotation.md`** — the per-format carrier shapes. CDX `metadata.component.externalReferences[]` per-scheme `type` mapping, SPDX 2.3 dual-carrier (main-module `Package.externalRefs[PERSISTENT-ID]` + `creationInfo.creators` redundant text), SPDX 3 `Element.externalIdentifier[]` (perfect fit), `mikebom:source-identifiers` annotation envelope shape for user-defined-namespace fallback.

### 3. Quickstart (`quickstart.md`)

Five operator-facing recipes:

- **Recipe 1**: scan a git checkout, observe auto-detected `repo:` identifier in the CDX `externalReferences[]`.
- **Recipe 2**: scan a non-git directory, supply manual `--with-source repo:git@github.com:...`, observe identifier emitted.
- **Recipe 3**: attach a corporate user-defined identifier `--with-source acme_corp_id:abc123`, observe it emitted under `mikebom:source-identifiers`.
- **Recipe 4**: scan an image, observe auto-detected `image:` identifier with full `<registry>/<name>:<tag>@sha256:<digest>` form.
- **Recipe 5**: validate cross-tier consumption — emit a source SBOM with auto-detected `repo:` identifier; later, milestone 074's `--bind-to-source repo:git@...` will resolve to this SBOM (forward-looking handshake).

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after writing artifacts.

## Re-evaluate Constitution Check

Post-design review: still ✅ on all 12 principles. The plan does not introduce a new format, does not extend any existing schema beyond what milestone-072 already established (the new `mikebom:source-identifiers` annotation reuses the milestone-071 envelope contract), and Principle V is materially strengthened by FR-010's published reference doc.

The user-defined-namespace `mikebom:source-identifiers` annotation IS the `mikebom:*` exception case — but it's the documented exception (no native carrier accepts arbitrary operator-defined opaque schemes), and `docs/reference/source-identifiers.md` will carry the justification clause per Principle V's procedure.

**Gates: PASS post-design.** No new deviations.

## Complexity Tracking

*(empty — no constitution gate violations)*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| *(none)* | | |
