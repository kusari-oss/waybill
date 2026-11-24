# Research: Relabel a component tier in an edited SBOM

Code references are at `main` `f0c6706`.

## R1. Where `--retier` plugs in

- **Decision:** a new `EditOp::Retier { from, to }` variant in `waybill-cli/src/edit/mod.rs`, parsed and sequence-stamped by `cli/edit.rs` like `--drop` and `--redact`. It is applied in `edit::run`'s operation loop through a new adapter method, `set_tier(id, tier)`, implemented by each of the three adapters (`cdx.rs`, `spdx23.rs`, `spdx3.rs`).
- **Rationale:**
  - `run` already applies operations in command-line order.
  - `SbomAdapter` already abstracts the three formats.
  - `ComponentView.tier` is already read for the `--drop tier=` selector (`select.rs`), so matching a component's current tier needs no new reader.
- **Alternative:** a separate `sbom retier` command. Rejected: it would lose the derivation record, the signature chain and `verify-chain` (spec Clarifications).

## R2. Simultaneous mappings (FR-001)

- **The problem:** operations apply in sequence, so `--retier design=source --retier source=build` applied naively turns an original `design` into `build`.
- **Decision:** `run` keeps a run-local set of components relabelled in this edit. A component in it is skipped by later `--retier` operations, so each component is relabelled at most once, from its tier in the input.
- **Not used:** the marker's presence. A document already edited once carries markers from the earlier edit, and must still be relabellable.

## R3. Recording the mapping (FR-005, C194)

C194's `operations` array holds `OpReport { category, matched, changed }`, and parity compares that projection across formats.

- **Decision:** one `OpReport` per `--retier`, with `category: "retier"` and two new optional fields, `from` and `to`. They are serialised only when present, so every existing record is byte-identical.
- **Why it is safe:** C194's rule is "never the values" because a value can be a redacted path or name. Tier names are a closed technical vocabulary, not content from the original. `matched` counts the components with tier `from` before the operation, and `changed` the number relabelled; they differ only when R2 skips one.
- **Alternative:** encoding the mapping in the category (`retier:design=source`). Rejected: consumers group by category.

## R4. The per-component marker (FR-004; Clarification A)

- **Decision:** a new row, **C243 `waybill:retiered-from`**. It is a per-component string, the tier the component had in the scan that first produced it, and it appears in all three formats the way C5 `waybill:sbom-tier` does:
  - a CycloneDX property;
  - an SPDX 2.3 package annotation, with the `waybill-annotation/v1` envelope;
  - an SPDX 3 `Annotation` element on the package.

  Its parity extractor uses the same `cdx_anno!`, `spdx23_anno!` and `spdx3_anno!` macros as C218, in the same change.
- **On a second edit:** when an already-marked component is relabelled again, the marker keeps its first value. It records where the component started, and the derivation chain records every hop.
- **The name keeps it out of `waybill:sbom-tier`'s prefix.** `--drop-annotations waybill:sbom-tier` (#1683's use) then removes the label without touching the marker's namespace. The marker is also added to `PROTECTED_ANNOTATIONS`, alongside C21 and C194, so no edit can remove it (spec edge case).
- **The standards-native audit (Principle V):**
  - CycloneDX 1.6 has no per-component lifecycle field (`metadata.lifecycles` is document-level), and `pedigree` describes a component's ancestry as software, not an edit to its classification;
  - SPDX 2.3 has no equivalent;
  - SPDX 3's `software_sbomType` sits on the `Sbom` element.

  Finding: **KEEP-NO-NATIVE**, as for C5.

## R5. The document type: what carries it, and the operator assertion (FR-006)

**Three places carry it, not two.** The spec named the first and third; the second is added by amendment:
- CycloneDX `metadata.lifecycles[].phase`;
- **SPDX 2.3 `creationInfo.comment`**, `Scope: … Observed lifecycle phases: … .` (`generate/spdx/document.rs`, line 907);
- SPDX 3 `software_Sbom.software_sbomType`, plus the same comment.

**`--sbom-type` is not recorded in an emitted document.** It collapses all three to a single value. `waybill:generation-context` only names the mode (`filesystem-scan`), so nothing in the document says the type was asserted (`sbom_type_signaling.rs`, `sbom-types.md`).

- **Decision, inferring the assertion:** the edit computes the type the *original's own* component tiers would give. If the original's document type equals that, it was derived, and the edit recomputes it from the relabelled tiers. If it differs, it was asserted, or written by another tool, and the edit leaves it alone and says so in its report.
- **The ambiguous case:** an assertion that happens to equal the derived value, such as `--sbom-type build` on a document whose components are all `build`, is indistinguishable and is recomputed. The documentation states this.
- **The spec amendment:** FR-006's exception becomes "the document type is recomputed only when it equals what the original's own tiers give", and the SPDX 2.3 comment joins the list.
- **One source for both:** the edit and the emitter must compute the type the same way. `generate::lifecycle_phases::aggregate_phases` and `aggregate_spdx3_sbom_types` take `ResolvedComponent`s. Each gains a sibling over tier strings (`aggregate_phases_from_tiers`, `aggregate_spdx3_sbom_types_from_tiers`), which the existing functions call. The emitter's output is unchanged, and the edit reuses the same mapping, so the two cannot drift.

## R6. Completeness untouched (FR-007; Clarification A)

- **Decision:** `--retier` never calls `downgrade_graph_completeness`. That call is reserved for changed dependency lists, and a relabel changes none.
- **Documentation:** `sbom edit`'s page and C194's row say a milestone-177 `transitive-edges-unresolvable` clause may name an ecosystem whose `design` components were relabelled, because the clause describes resolution, not labels.

## R7. Validation (FR-002)

The parser refuses:
- a value with no `=`, or an empty side;
- a `TO` outside `{design, source, build, analyzed, deployed, runtime}`. It is parsed with `SbomType::parse`, so `--retier` and `--sbom-type` refuse the same values with the same message;
- `FROM` equal to `TO`;
- the same `FROM` twice with different targets.

`FROM` is matched exactly and may be any string, so `image` and `declared` can be relabelled. Every refusal happens before anything is read, through clap's `value_parser` plus a post-parse check, and nothing is written.

## R8. Parity and fixtures

- **Parity:** `tests/sbom_edit_parity.rs` already edits one scan's three documents and checks every universal-parity row. The new cases follow it, adding C5 (the tier) and C243 (the marker).
- **Fixtures:** synthetic `waybill-fixture-*` scans, as the existing `sbom_edit_*.rs` tests use. SC-001 runs on the committed corpus goldens for npm-express, hpack-ogmios-no-generated-cabal and haskell-language-server, which are read, never written.

## R9. Measured baseline (SC-001)

The consumer rule was modelled on the committed goldens at `main` `18691d8`: drop `design` and `scope: excluded`, then everything left unreachable from the root.

| Target | `required` lost today | `required` lost after `--retier design=source` |
|---|---|---|
| npm-express | 27 of 29 | 0 |
| hpack-ogmios-no-generated-cabal | 95 of 95 | 0 |
| haskell-language-server | 325 of 354 | 0 |

T016 re-measures this with the built command, and commits the measurement script beside the spec.
