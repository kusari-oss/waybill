# Implementation Plan: SPDX 3 externalIdentifierType controlled-vocabulary conformance

**Branch**: `079-spdx3-id-vocab` | **Date**: 2026-05-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/079-spdx3-id-vocab/spec.md`

## Summary

Close the second SPDX 3 conformance gap surfaced by the JPEWdev `spdx3-validate` tool: when image-tier (milestone 074), source-tier-with-git-detection (milestone 074), or build-tier (milestone 076) flows reach the SPDX 3 emitter, mikebom's identifier schemes (`image`, `repo`, `git`, `attestation`, `subject`, plus user-defined scheme names attached via milestone 073's `--component-id <PURL>=<SCHEME>:<VALUE>` flag) pass through into the `externalIdentifierType` field verbatim. The SPDX 3 SHACL constraint requires exactly one of 11 controlled-vocabulary values; mikebom's emission is non-conformant for every such SBOM.

The fix is a single mapping function in the SPDX 3 emission code path: every mikebom scheme maps to one of the 11 vocab values, and the original scheme name is preserved in the `Core/ExternalIdentifier` element's `comment` field per the 2026-05-07 clarification (Q1) — formatted as `"original-scheme: <name>"`. Per Q2, content-shape detection is `gitoid`-only: `git:` values matching `^[0-9a-f]{40}$` (a SHA-1) emit as `externalIdentifierType: "gitoid"`; everything else maps to `"other"` with the original scheme preserved.

The fix touches one helper file (a new `v3_id_type_map.rs`-style module or extension to `v3_external_ids.rs`) and two existing call sites (`v3_document.rs:309` for document-level identifiers + `v3_packages.rs:170` for per-package identifiers). CDX 1.6 + SPDX 2.3 emission paths are not modified. Reuses the milestone-078 `tests/spdx3_conformance.rs` integration test infrastructure + CI gate.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–078; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (JSON-LD round-tripping), `regex` (already in the dependency closure for the `gitoid` detection per Q2's regex `^[0-9a-f]{40}$`), `tracing`, `anyhow`. No new Cargo deps. Python 3.10+ in CI/test layer (already added in milestone 078 for `spdx3-validate==0.0.5`); reused as-is.
**Storage**: N/A — pure metadata transform on the SPDX 3 emission code path; no caches, no persistence.
**Testing**: `cargo +stable test --workspace` continues as the primary gate. Extends the existing `mikebom-cli/tests/spdx3_conformance.rs` integration test with new test cases covering: image-tier with non-empty RepoTags (the milestone-078 dodge case from issue #154's reproduction recipe), source-tier inside a git repository (so milestone-074's `repo:` and `git:` auto-detect fires), build-tier with synthetic `subject:` and `attestation:` identifiers (per milestone 076's pattern), and `--component-id <PURL>=jira:PROJ-1234` user-defined invocation. Each test calls the validator and asserts zero `externalIdentifierType` violations + recoverable original scheme name in `comment`.
**Target Platform**: Linux (CI primary; gates on the validator), macOS (developer workstations; graceful-skip when validator isn't installed locally — milestone 078 pattern preserved).
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Validator runs <30s against the extended fixture suite (per milestone 078's measured baseline; new fixtures add ~5 fresh-emission validations × <3s each). Integration test wall-time remains <60s end-to-end including the 4 new test cases. Mapping function itself is a pure function with O(1) per-identifier cost — no measurable impact on emission wall-time.
**Constraints**: Determinism per FR-005 (mapping is a pure function of `(scheme, value)`; same inputs → byte-identical output across re-runs). Backward compatibility per FR-006/FR-007 (CDX 1.6 + SPDX 2.3 byte-identity goldens stay byte-identical; SPDX 3 byte-identity goldens that don't exercise the new mapping path stay byte-identical). No regression on existing milestone-078 SPDX 3 fixtures — those are source-tier with no auto-detected identifiers, so their goldens MUST stay byte-identical post-fix.
**Scale/Scope**: Bug-fix scope. ~50–80 LOC for the mapping helper + ~30 LOC at the two call sites + ~200 LOC integration test extensions + 0 CI workflow updates (reuses milestone 078's). Likely 0–3 SPDX 3 fixture regenerations (only fixtures that exercise auto-detected or build-tier identifiers — none of the 9 source-tier ecosystem fixtures do, so likely 0 from that set; new fixture-emission tests don't have goldens). Smaller than 078 because no validator integration to build, no new helper script, no new CI gate.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All 12 principles + 4 strict boundaries reviewed:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | All Rust changes inside `mikebom-cli/src/generate/spdx/`. No C, no FFI. Python at the CI/test layer reuses milestone 078's existing integration. |
| II. eBPF-Only Observation | ✅ Pass / N/A | Identifier emission metadata only; eBPF trace path is unchanged. |
| III. Fail Closed | ✅ Pass | The mapping is total — every input has a defined output. The `gitoid` detection has a defined fallback (`other`) when the regex doesn't match. The integration test fails closed when validator reports any violation. |
| IV. Type-Driven Correctness | ✅ Pass | The mapping function's signature is `(scheme: &SchemeName, value: &str) -> SpdxIdType` (or equivalent newtype). The SPDX vocabulary set is encoded as an enum (`SpdxIdType::Other`, `SpdxIdType::Gitoid`, etc.) with `as_str()` returning the literal vocab string. No raw-`String` boundary crossings; production code uses `anyhow::Result`. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md. |
| V. Specification Compliance | ✅ Pass | Native-first audit (per Principle V's standards-native-precedence requirement, codified in the v1.4.0 amendment): the `comment` field on `Core/ExternalIdentifier` is the standards-native SPDX 3 field for free-text supplementary metadata — it's universally available on SPDX 3 Core elements and is the spec-conformant slot for "the original scheme this maps from" semantic. The 2026-05-07 Q1 clarification explicitly chose this native field over (a) `mikebom:original-scheme` annotation (the alternative ranked last per Principle V), and (b) `identifierLocator` (a more specialized SPDX 3 field that's harder to navigate). Result: zero new `mikebom:*` properties introduced; the original scheme name is preserved in standards-native metadata. The milestone IS Principle V conformance work — closes the SPDX 3 controlled-vocabulary gap milestone 078 left for issue #154 follow-up. |
| VI. Three-Crate Architecture | ✅ Pass | All Rust changes inside `mikebom-cli`. No new crates. |
| VII. Test Isolation | ✅ Pass | Conformance tests run without elevated privileges. Reuses milestone 078's graceful-skip + CI strict-mode pattern; no kernel privileges needed. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | Validator-driven fix → improves accuracy of mikebom's SPDX 3 output. The `gitoid` detection (Q2) preserves more semantic precision than uniform `other` mapping for git SHAs. The `original-scheme: <name>` preservation in `comment` ensures no information loss. |
| X. Transparency | ✅ Pass | Operators can inspect the `comment` field on any `externalIdentifier` element to recover the original scheme. The mapping function is documented in research with the per-scheme decision table. |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | The validator is a CI/test tool, not an external data source for SBOM content. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — extending production code that already complies; tests use the standard guard |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed. Principle V audit explicitly cited per the v1.4.0 amendment's spec-author requirement: native `comment` field chosen over `mikebom:original-scheme` annotation.

## Project Structure

### Documentation (this feature)

```text
specs/079-spdx3-id-vocab/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify + /speckit.clarify output (Q1 + Q2 integrated)
├── research.md                     # Phase 0 — per-scheme mapping table + content-shape regex justification
├── data-model.md                   # Phase 1 — mapping function signature, SpdxIdType enum, comment-field shape
├── quickstart.md                   # Phase 1 — operator-facing recipes for inspecting the new shape
├── contracts/
│   └── spdx3-id-vocab-mapping.md   # Phase 1 — wire-format contract per SPDX 3 model
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches one new helper module + two existing emission call sites + extends one integration test file. CI workflow is unchanged (reuses milestone 078's gate).

```text
mikebom-cli/
├── src/
│   ├── generate/
│   │   └── spdx/
│   │       ├── v3_id_type_map.rs           # NEW (~80 LOC) — pure-function mapping
│   │       │                                 # `(scheme, value) -> (SpdxIdType, original_scheme_comment)`.
│   │       │                                 # Defines SpdxIdType enum (Other/Gitoid/Cve/Cpe23/PackageUrl/...
│   │       │                                 # — all 11 vocab values). Implements:
│   │       │                                 # - map_scheme_to_vocab(scheme, value) -> (SpdxIdType, Option<String>)
│   │       │                                 # - format_original_scheme_comment(name) -> String
│   │       │                                 #   (formats as "original-scheme: <name>")
│   │       │                                 # - is_git_sha(value) -> bool (compiled regex `^[0-9a-f]{40}$`)
│   │       │                                 # Pure function; deterministic; no I/O.
│   │       ├── v3_document.rs              # MODIFY (~10 LOC) — at line 309
│   │       │                                 # (document-level externalIdentifier emission), replace
│   │       │                                 # `"externalIdentifierType": id.scheme.as_str()` with
│   │       │                                 # call to the new mapping. When mapping returns Some(comment),
│   │       │                                 # add `"comment": <value>` to the externalIdentifier JSON
│   │       │                                 # object. Update the externalIdentifier-array sort key in
│   │       │                                 # v3_external_ids.rs accordingly.
│   │       ├── v3_packages.rs              # MODIFY (~10 LOC) — at line 170
│   │       │                                 # (per-package externalIdentifier emission), same pattern
│   │       │                                 # as v3_document.rs above.
│   │       └── v3_external_ids.rs          # MODIFY (~10 LOC) — sort-key adjustment if needed.
│   │                                         # The existing dedup/sort works on (type, identifier);
│   │                                         # post-fix, multiple `image:` identifiers with different
│   │                                         # values still dedup correctly because comment is metadata,
│   │                                         # not identity. May not need touching — verified at
│   │                                         # implementation time.
│   └── binding/
│       └── identifiers/                    # NOT MODIFIED. The internal SchemeName + Identifier types
│                                             # are preserved verbatim. The mapping happens at SPDX 3
│                                             # emission time, NOT in the identifier-binding layer.
│                                             # CDX 1.6 + SPDX 2.3 emission paths read the same internal
│                                             # types and continue to use their own format-specific
│                                             # vocabulary mappings (which were never affected by this bug).
└── tests/
    └── spdx3_conformance.rs                # MODIFY (~200 LOC additions) — add test cases:
                                              # 1. image_tier_with_repo_tags_passes_validator
                                              #    (the milestone-078 dodge case; non-empty RepoTags)
                                              # 2. source_tier_in_git_repo_passes_validator
                                              #    (set up a tempdir with `.git/HEAD` + remote URL
                                              #    so milestone-074 auto-detect fires)
                                              # 3. build_tier_with_subjects_passes_validator
                                              #    (synthetic ScanArtifacts with subject + attestation
                                              #    identifiers per milestone 076 pattern)
                                              # 4. user_defined_scheme_passes_validator
                                              #    (--component-id <PURL>=jira:PROJ-1234)
                                              # 5. id_type_mapping_unit_table
                                              #    (table-driven unit test enumerating every
                                              #    (scheme, value) pair → expected (SpdxIdType, comment))
                                              # 6. git_sha_detected_as_gitoid
                                              #    (assert (git, <40-char hex>) → SpdxIdType::Gitoid;
                                              #     (git, <https://...>) → SpdxIdType::Other)
                                              # 7. original_scheme_recoverable_from_comment
                                              #    (assert all 5 schemes round-trip recoverably)

mikebom-cli/tests/fixtures/golden/spdx-3/    # MAY MODIFY 0–3 fixtures only IF any of the existing
                                              # 9 source-tier fixtures happens to emit auto-detected
                                              # identifiers (none do today, so likely 0 changes).
                                              # If new "synthetic build-tier" or "image-tier" goldens
                                              # are added as part of test infrastructure, they live
                                              # under a separate sub-directory to keep ecosystem-fixture
                                              # boundaries clean.

scripts/                                     # NOT MODIFIED. install-spdx3-validate.sh from milestone 078
                                              # is reused as-is (validator pin unchanged at 0.0.5).

.github/workflows/                           # NOT MODIFIED. The milestone-078 conformance gate already
                                              # exercises the new test cases automatically (they're added
                                              # to the same test binary that's already gated by
                                              # MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1).

docs/reference/identifiers.md                # MODIFY (small) — add a brief note in the SPDX 3 wire-mapping
                                              # section that scheme names map to the SPDX 3 controlled
                                              # vocabulary at emission time, with the original scheme
                                              # preserved in `Core/ExternalIdentifier.comment` as
                                              # `"original-scheme: <name>"`. Per-scheme mapping table
                                              # for operators who need to predict the wire shape.
```

**Structure Decision**: Single project. Extends `mikebom-cli` with one new helper module (`v3_id_type_map.rs`) + minimal touch-ups at two existing emission call sites + integration test extensions in the milestone-078 file. Smallest-possible-surface-change consistent with milestones 074/075/076/077/078. No new modules outside `generate/spdx/`; no new crates; no new dependencies; no CI workflow changes.

## Phase 0 — Research questions

Five implementation-level decisions to pin in `research.md`. The Q1 + Q2 clarifications already locked the two highest-impact decisions during /speckit.clarify; Phase 0 documents the remaining details and validates the per-scheme mapping table against the SPDX 3 model.

1. **Per-scheme mapping table — definitive** — Enumerate every mikebom scheme (`image`, `repo`, `git`, `subject`, `attestation`, plus the case where a user-defined scheme name happens to match an SPDX 3 vocab value verbatim) and document the chosen SPDX 3 vocab value + comment-field text + edge-case behavior. The Q1 + Q2 answers fix the major axes (`other` for everything except git-SHA values; `gitoid` for git-SHA values; `comment` carries `"original-scheme: <name>"`). Phase 0 ratifies the full per-scheme decision and validates against the SPDX 3 controlled-vocabulary spec. **The output of this section IS the lookup table the implementation encodes**.

2. **`gitoid` regex precision + `git:` value-shape catalog** — The Q2 detection is `^[0-9a-f]{40}$`, which matches SHA-1 git commit SHAs. Verify: (a) does mikebom's milestone 074 git-detection ever produce SHA-256 git SHAs (which would be 64-char hex)? If yes, expand the regex to `^[0-9a-f]{40}$|^[0-9a-f]{64}$`. (b) Does `git:` ever carry abbreviated SHAs (7-12 chars) from any milestone-074 code path? If yes, document that abbreviated SHAs map to `other` (the regex already excludes them; just confirm). (c) Does `git:` ever carry tag names, branch names, or git URL forms? Pin the value-shape catalog so the implementation's regex doesn't miss a case.

3. **The `Core/ExternalIdentifier` element's `comment` field native shape** — Verify against `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json` (the schema audit pattern milestone 078 used) that `comment` is in fact the canonical SPDX 3 free-text metadata field on `Core/ExternalIdentifier` (not `Core/Element.comment` higher up the inheritance chain that wouldn't apply to a non-Element subclass), and that the SHACL constraints permit free-text content. If `comment` is on the parent class only and `Core/ExternalIdentifier` doesn't inherit it, fall back to the next-best native field (likely `identifierLocator` or a top-level note). Audit per Principle V's spec-author requirement.

4. **Determinism contract for the new comment field** — The comment field is a string `format!("original-scheme: {scheme_name}")` — fully deterministic per (scheme, value) input. Phase 0 documents this as part of the determinism contract (FR-005). Multi-identifier-per-component cases (Edge Case in spec) need the externalIdentifier-array sort key extended: today the sort key is `(externalIdentifierType, identifier)`; post-fix two identifiers with the same (mapped vocab value, identifier value) but different `original-scheme:` comments would dedup incorrectly. Pin the sort key as `(externalIdentifierType, identifier, comment)` to preserve determinism + correctness.

5. **CDX 1.6 + SPDX 2.3 emission audit (negative confirmation)** — Confirm that the CDX 1.6 emission path (`mikebom-cli/src/generate/cyclonedx/`) and the SPDX 2.3 emission path (`mikebom-cli/src/generate/spdx/document.rs`) read the internal `SchemeName` types via different code paths than the SPDX 3 emission code path, so this milestone's changes can't accidentally regress them. The CDX 1.6 `externalReferences[].type` vocabulary is independent of SPDX 3's `Core/ExternalIdentifierType`; the SPDX 2.3 `externalRefs[].referenceCategory`/`referenceType` vocabulary is also independent. Verify by grep + by running the existing `cdx_regression` and `spdx_regression` test suites and confirming byte-identity preservation post-fix.

## Phase 1 — Design & contracts

### data-model.md

One new internal type (`SpdxIdType` enum or `&'static str` table) + one new pure function (`map_scheme_to_vocab`) + one new emitted JSON-LD shape change (the `comment` field on every non-vocab `externalIdentifier` element). No changes to internal `SchemeName` / `Identifier` types — those flow unchanged from milestones 073/074/076; only the emission-time mapping changes.

### contracts/

One contract: `spdx3-id-vocab-mapping.md`. Documents:
- The full per-scheme → SPDX 3 vocab mapping table (the Phase 0 §1 output).
- The `comment` field's exact value format (`"original-scheme: <name>"`).
- The `gitoid` detection regex + value-shape catalog from Phase 0 §2.
- The wire-format expectations the integration tests assert on (post-fix shape per scheme).
- The CI gate's behavior contract (extends the milestone-078 contract; no new gating semantics).

### quickstart.md

Operator-facing recipes:
1. **Inspect the post-fix wire shape** — `jq` snippets showing `externalIdentifier[]` entries with the new `comment` field for the 5 built-in non-vocab schemes.
2. **Verify a freshly-emitted SBOM passes the validator** — reuses milestone-078's recipe, now with `--image registry.example.com/img:tag` to exercise the new code path.
3. **Filter SBOMs by original mikebom scheme** — `jq` recipe showing how cross-tier correlation tooling can recover the original scheme by parsing the `comment` field's `"original-scheme: "` prefix.
4. **Per-scheme mapping reference** — single table mapping every mikebom scheme to its SPDX 3 vocab value + comment shape (mirrors the contract).
5. **Pre-PR gate behavior** — same as milestone 078; no new behavior to document.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. `/speckit.tasks` consumes plan.md + spec.md + Phase 1 docs and emits `tasks.md`. Estimated task count: **~10–12** — smaller than 078 because no validator integration to set up, no helper script to write, no CI workflow to update. Phases: Setup (1) + Foundational mapping (2–3) + US1 tests (3–4) + US2 tests (1–2) + US3 tests (1) + Polish (1–2).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all 12 principles + 4 strict boundaries with zero violations. Principle V audit is explicitly cited in the table above per the v1.4.0 amendment requirement.
